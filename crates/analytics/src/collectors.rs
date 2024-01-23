use {
    crate::{AnalyticsEvent, Batch, BatchFactory, Collector, Exporter},
    std::{marker::PhantomData, pin::pin, time::Duration},
    tokio::sync::{mpsc, mpsc::error::TrySendError},
};

#[derive(Debug, thiserror::Error)]
enum InternalError {
    #[error("Batch error: {0}")]
    Batch(String),

    #[error("Export error: {0}")]
    Export(String),

    #[error("Serialization failed")]
    Serialization,
}

#[derive(Debug, thiserror::Error)]
pub enum CollectionError {
    #[error("Data channel overflow")]
    DataChannelOverflow,

    #[error("Data channel closed")]
    DataChannelClosed,
}

impl<T> From<TrySendError<T>> for CollectionError {
    fn from(val: TrySendError<T>) -> Self {
        match val {
            TrySendError::Full(_) => Self::DataChannelOverflow,
            TrySendError::Closed(_) => Self::DataChannelClosed,
        }
    }
}

pub struct CollectorConfig {
    /// Data collection queue capacity. Overflowing the queue would cause excess
    /// data to be dropped.
    pub data_queue_capacity: usize,

    /// Maximum interval between batch data exports.
    pub export_interval: Duration,
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            data_queue_capacity: 8192,
            export_interval: Duration::from_secs(5 * 60),
        }
    }
}

pub struct BatchCollector<T> {
    data_tx: mpsc::Sender<T>,
}

impl<T> BatchCollector<T>
where
    T: AnalyticsEvent,
{
    pub fn new<B, E>(config: CollectorConfig, batch_factory: B, exporter: E) -> Self
    where
        B: BatchFactory<T>,
        B::Error: std::error::Error,
        E: Exporter,
    {
        let (data_tx, data_rx) = mpsc::channel(config.data_queue_capacity);

        tokio::spawn(async move {
            let event_loop = EventLoop::new(batch_factory, exporter, config);

            if let Err(err) = event_loop.run(data_rx).await {
                tracing::warn!(?err, "analytics event loop failed");
            }
        });

        Self { data_tx }
    }
}

impl<T> Collector<T> for BatchCollector<T>
where
    T: AnalyticsEvent,
{
    type Error = CollectionError;

    fn collect(&self, data: T) -> Result<(), Self::Error> {
        self.data_tx.try_send(data).map_err(Into::into)
    }
}

struct EventLoop<T, B, E> {
    batch_factory: B,
    exporter: E,
    config: CollectorConfig,
    _marker: PhantomData<T>,
}

impl<T, B, E> EventLoop<T, B, E>
where
    T: AnalyticsEvent,
    B: BatchFactory<T>,
    B::Error: std::error::Error,
    E: Exporter,
    E::Error: std::error::Error,
{
    fn new(batch_factory: B, exporter: E, config: CollectorConfig) -> Self {
        Self {
            batch_factory,
            exporter,
            config,
            _marker: PhantomData,
        }
    }

    async fn run(self, data_rx: mpsc::Receiver<T>) -> Result<(), InternalError> {
        let mut data_rx = pin!(data_rx);
        let mut export_interval = pin!(tokio::time::interval(self.config.export_interval));

        let mut current_batch = self
            .batch_factory
            .create()
            .map_err(|err| InternalError::Batch(err.to_string()))?;

        loop {
            tokio::select! {
                data = data_rx.recv() => match data {
                    Some(data) => {
                        if let Err(err) = current_batch.push(data) {
                            tracing::warn!(?err, "failed to push data to batch");

                            // Data push error is considered transient, so try to replace the
                            // broken batch and continue. If we can't create a new batch, exit
                            // the event loop with an error.
                            self.replace_batch(&mut current_batch)?;
                            export_interval.reset();

                            continue;
                        }

                        // Export the batch if it's at capacity.
                        if current_batch.is_full() {
                            self.export_batch(&mut current_batch)?;
                            export_interval.reset();
                        }
                    },

                    // The transmitter has been dropped. Export current batch and shutdown.
                    None => {
                        return self.export_batch(&mut current_batch);
                    },
                },

                _ = export_interval.tick() => {
                    self.export_batch(&mut current_batch)?;
                }
            }
        }
    }

    fn replace_batch(&self, current_batch: &mut B::Batch) -> Result<B::Batch, InternalError> {
        let next_batch = self
            .batch_factory
            .create()
            .map_err(|err| InternalError::Batch(err.to_string()))?;

        Ok(std::mem::replace(current_batch, next_batch))
    }

    fn export_batch(&self, current_batch: &mut B::Batch) -> Result<(), InternalError> {
        if current_batch.is_empty() {
            return Ok(());
        }

        let current_batch = self.replace_batch(current_batch)?;
        let exporter = self.exporter.clone();

        tokio::spawn(async move {
            let result = async {
                let data = tokio::task::spawn_blocking(move || current_batch.serialize())
                    .await
                    .map_err(|_| InternalError::Serialization)?
                    .map_err(|err| InternalError::Batch(err.to_string()))?;

                exporter
                    .export(data)
                    .await
                    .map_err(|err| InternalError::Export(err.to_string()))
            }
            .await;

            if let Err(err) = result {
                tracing::warn!(?err, "failed to export batch data");
            }
        });

        Ok(())
    }
}
