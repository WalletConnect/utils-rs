use {
    super::{BatchExporter, BatchWriter},
    crate::{AnalyticsCollector, AnalyticsEvent},
    std::{
        fmt::{Debug, Display},
        io::Write,
        marker::PhantomData,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::{Duration, Instant},
    },
    tokio::{
        sync::{mpsc, mpsc::error::TrySendError},
        time::MissedTickBehavior,
    },
    tracing::error,
};

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone)]
pub struct BatchOpts {
    /// Data collection queue size. Overflowing it will either drop additional
    /// analytics events, or cause an `await` (in async version) until there's
    /// room in the queue.
    pub event_queue_limit: usize,

    /// The amount of time after which current batch will be exported regardless
    /// of how much data it has collected.
    pub export_time_threshold: Duration,

    /// Writer buffer size threshold, going above which will cause data export
    /// and new batch allocation.
    pub export_size_threshold: usize,

    /// The maximum number of events that a single batch can contain. Going
    /// above this limit will cause data export and new batch allocation.
    pub export_row_threshold: usize,

    /// Allocation size for the batch data buffer. Overflowing it will cause
    /// reallocation of data, so it's best to set the export threshold below
    /// this value to never overflow.
    pub batch_alloc_size: usize,

    /// The interval at which the background task will check if current batch
    /// has expired and needs to be exported.
    pub export_check_interval: Duration,
}

impl Default for BatchOpts {
    fn default() -> Self {
        Self {
            event_queue_limit: 2048,
            export_time_threshold: Duration::from_secs(60 * 5),
            export_size_threshold: 1024 * 1024 * 128,
            export_row_threshold: 1024 * 128,
            batch_alloc_size: 1024 * 1024 * 130,
            export_check_interval: Duration::from_secs(30),
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, thiserror::Error)]
pub enum BatchError<T: Debug + Display> {
    #[error("event queue overflow")]
    EventQueueOverflow,

    #[error("event queue channel closed")]
    EventQueueChannelClosed,

    #[error("writer error: {0}")]
    Writer(T),

    #[error("export error: {0}")]
    Export(anyhow::Error),
}

impl<T, E> From<TrySendError<T>> for BatchError<E>
where
    T: AnalyticsEvent,
    E: Debug + Display,
{
    fn from(val: TrySendError<T>) -> Self {
        match val {
            TrySendError::Full(_) => Self::EventQueueOverflow,
            TrySendError::Closed(_) => Self::EventQueueChannelClosed,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////

/// Wrapper around the `Vec<u8>` buffer to accurately report the underlying
/// buffer size after giving away its ownership.
pub struct BatchBuffer {
    data: Vec<u8>,
    size_bytes: Arc<AtomicUsize>,
}

impl BatchBuffer {
    fn new(capacity: usize) -> Self {
        let data = Vec::with_capacity(capacity);
        let size_bytes = Arc::new(AtomicUsize::new(0));
        Self { data, size_bytes }
    }

    pub(crate) fn into_inner(self) -> Vec<u8> {
        self.data
    }

    fn size_bytes(&self) -> &Arc<AtomicUsize> {
        &self.size_bytes
    }
}

impl Write for BatchBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let len = self.data.write(buf)?;
        self.size_bytes.fetch_add(len, Ordering::Relaxed);
        Ok(len)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.data.flush()
    }
}

////////////////////////////////////////////////////////////////////////////////

struct Batch<T: AnalyticsEvent, W: BatchWriter<T>> {
    data: W,
    num_rows: usize,
    size_bytes: Arc<AtomicUsize>,
    expiration: Instant,
    has_expiration: bool,
    _marker: PhantomData<T>,
}

impl<T, W> Batch<T, W>
where
    T: AnalyticsEvent,
    W: BatchWriter<T>,
{
    fn new(opts: &BatchOpts) -> Result<Self, BatchError<W::Error>> {
        let buffer = BatchBuffer::new(opts.batch_alloc_size);
        let size_bytes = buffer.size_bytes().clone();
        let data = W::create(buffer, opts).map_err(BatchError::Writer)?;

        Ok(Self {
            data,
            num_rows: 0,
            size_bytes,
            expiration: Instant::now(),
            has_expiration: false,
            _marker: PhantomData,
        })
    }

    fn write(&mut self, data: T) -> Result<usize, BatchError<W::Error>> {
        self.data.write(data).map_err(BatchError::Writer)?;
        self.num_rows += 1;
        Ok(self.size_bytes())
    }

    fn flush(&mut self) -> Result<(), BatchError<W::Error>> {
        self.data.flush().map_err(BatchError::Writer)
    }

    fn num_rows(&self) -> usize {
        self.num_rows
    }

    fn size_bytes(&self) -> usize {
        self.size_bytes.load(Ordering::Relaxed)
    }

    fn has_expiration(&self) -> bool {
        self.has_expiration
    }

    fn set_expiration(&mut self, expiration: Instant) {
        self.has_expiration = true;
        self.expiration = expiration;
    }

    fn expiration(&self) -> Instant {
        self.expiration
    }

    fn into_buffer(self) -> Result<Vec<u8>, BatchError<W::Error>> {
        self.data.into_buffer().map_err(BatchError::Writer)
    }
}

////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
enum ControlEvent<T> {
    Process(T),
    Shutdown,
}

struct Batcher<T: AnalyticsEvent, E: BatchExporter, W: BatchWriter<T>> {
    opts: BatchOpts,
    ctrl_rx: mpsc::Receiver<ControlEvent<T>>,
    batch: Batch<T, W>,
    exporter: E,
}

impl<T, E, W> Batcher<T, E, W>
where
    T: AnalyticsEvent,
    E: BatchExporter,
    W: BatchWriter<T>,
{
    fn new(
        opts: impl Into<BatchOpts>,
        ctrl_rx: mpsc::Receiver<ControlEvent<T>>,
        exporter: E,
    ) -> Result<Self, BatchError<W::Error>> {
        let opts = opts.into();
        let batch = Batch::new(&opts)?;

        Ok(Self {
            opts,
            ctrl_rx,
            batch,
            exporter,
        })
    }

    fn process(&mut self, evt: T) -> Result<(), BatchError<W::Error>> {
        if !self.batch.has_expiration() {
            self.batch
                .set_expiration(Instant::now() + self.opts.export_time_threshold);
        }

        let size = self.batch.write(evt)?;
        let num_rows = self.batch.num_rows();

        if size >= self.opts.export_size_threshold || num_rows >= self.opts.export_row_threshold {
            self.export()
        } else {
            Ok(())
        }
    }

    fn export(&mut self) -> Result<(), BatchError<W::Error>> {
        self.batch.flush()?;

        if self.has_data() {
            let next_batch = Batch::new(&self.opts)?;
            let prev_batch = std::mem::replace(&mut self.batch, next_batch);
            let exporter = self.exporter.clone();

            // We want to continue processing events while exporting, so spawn a separate
            // task.
            tokio::spawn(async move {
                if let Err(error) = export_internal(exporter, prev_batch).await {
                    error!(%error, bug = true, "analytics data export failed");
                }
            });
        }

        Ok(())
    }

    fn expiration(&self) -> Instant {
        self.batch.expiration()
    }

    fn has_data(&self) -> bool {
        self.batch.num_rows() > 0
    }
}

async fn export_internal<T, E, W>(
    exporter: E,
    batch: Batch<T, W>,
) -> Result<(), BatchError<W::Error>>
where
    T: AnalyticsEvent,
    E: BatchExporter,
    W: BatchWriter<T>,
{
    // Writing batch data into buffer may be a CPU-heavy operation, so run it in a
    // separate thread.
    let data = tokio::task::spawn_blocking(move || batch.into_buffer())
        .await
        .map_err(|err| BatchError::Export(err.into()))??;

    exporter
        .export(data)
        .await
        .map_err(|err| BatchError::Export(err.into()))
}

////////////////////////////////////////////////////////////////////////////////

pub struct BatchCollector<T: AnalyticsEvent> {
    ctrl_tx: mpsc::Sender<ControlEvent<T>>,
}

impl<T> BatchCollector<T>
where
    T: AnalyticsEvent,
{
    pub fn new<W, E>(opts: BatchOpts, exporter: E) -> Result<Self, BatchError<W::Error>>
    where
        W: BatchWriter<T>,
        E: BatchExporter,
    {
        let (ctrl_tx, ctrl_rx) = mpsc::channel(opts.event_queue_limit);
        let mut inner = Batcher::<T, E, W>::new(opts.clone(), ctrl_rx, exporter)?;
        let export_check_interval = opts.export_check_interval;

        tokio::spawn(async move {
            let mut export_interval = tokio::time::interval(export_check_interval);
            export_interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

            loop {
                tokio::select! {
                    ctrl = inner.ctrl_rx.recv() => match ctrl {
                        Some(ControlEvent::Process(data)) => {
                            if let Err(error) = inner.process(data) {
                                error!(
                                    %error,
                                    bug = true,
                                    "analytics collector data processing failed"
                                );
                            }
                        },

                        Some(ControlEvent::Shutdown) => {
                            if let Err(error) = inner.export() {
                                error!(
                                    %error,
                                    bug = true,
                                    trigger = "collector_shutdown",
                                    "analytics collector failed to start data export"
                                );
                            }

                            break;
                        },

                        _ => break,
                    },

                    _ = export_interval.tick() => {
                        if Instant::now() > inner.expiration() {
                            if let Err(error) = inner.export() {
                                error!(
                                    %error,
                                    bug = true,
                                    trigger = "batch_expired",
                                    "analytics collector failed to start data export"
                                );
                            }
                        }
                    }
                };
            }
        });

        Ok(Self { ctrl_tx })
    }
}

impl<T> AnalyticsCollector<T> for BatchCollector<T>
where
    T: AnalyticsEvent,
{
    fn collect(&self, data: T) {
        if let Err(error) = self.ctrl_tx.try_send(ControlEvent::Process(data)) {
            error!(
                %error,
                bug = true,
                "failed to send data collection command to analytics collector"
            );
        }
    }
}

impl<T> Drop for BatchCollector<T>
where
    T: AnalyticsEvent,
{
    fn drop(&mut self) {
        if let Err(error) = self.ctrl_tx.try_send(ControlEvent::Shutdown) {
            error!(
                %error,
                bug = true,
                "failed to send shutdown command to analytics collector"
            );
        }
    }
}
