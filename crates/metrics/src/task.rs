use {
    super::{
        duration_as_millis_f64,
        future::{AsTaskName, TaskMetricsRecorder},
        otel,
        ServiceMetrics,
    },
    once_cell::sync::OnceCell,
    opentelemetry::metrics::{Counter, Histogram},
    std::{ops::Deref, sync::Arc, time::Duration},
};

/// Wrapper for [`OtelTaskMetricsRecorder`], which can be statically
/// initialized.
pub struct TaskMetrics {
    prefix: &'static str,
    inner: OnceCell<OtelTaskMetricsRecorder<()>>,
}

impl TaskMetrics {
    pub const fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            inner: OnceCell::new(),
        }
    }

    pub fn recorder(&self) -> &OtelTaskMetricsRecorder {
        self.inner
            .get_or_init(|| OtelTaskMetricsRecorder::new(self.prefix))
    }
}

impl Deref for TaskMetrics {
    type Target = OtelTaskMetricsRecorder;

    fn deref(&self) -> &Self::Target {
        self.recorder()
    }
}

/// Async task metrics recorder, which records the following data:
///  - `duration`: Total task duration, in milliseconds;
///  - `poll_duration`: Time spent in task `poll()` method, in milliseconds;
///  - `poll_entries`: Number of task `poll()` method entries;
///  - `started`: Number of tasks that were polled at least once;
///  - `finished`: Number of tasks that finished, either by polling to
///    completion or being dropped.
///
/// The above metrics are tracked using [`opentelemetry`] metrics API and are
/// prefixed according to the constructor arguments.
#[derive(Clone)]
pub struct OtelTaskMetricsRecorder<N: AsTaskName = ()> {
    inner: Arc<OtelRecorderInner>,
    name: Option<N>,
}

impl OtelTaskMetricsRecorder<()> {
    pub fn new(prefix: &str) -> Self {
        Self {
            inner: Arc::new(OtelRecorderInner::new(prefix)),
            name: None,
        }
    }
}

impl<N> OtelTaskMetricsRecorder<N>
where
    N: AsTaskName,
{
    #[inline]
    fn task_name_kv(&self) -> otel::KeyValue {
        let name: &'static str = self
            .name
            .as_ref()
            .map(AsTaskName::as_task_name)
            .unwrap_or_default();

        otel::KeyValue::new("task_name", name)
    }
}

impl<N1> OtelTaskMetricsRecorder<N1>
where
    N1: AsTaskName,
{
    /// Clones the current recording context with a new task name.
    pub fn with_name<N2>(&self, name: N2) -> OtelTaskMetricsRecorder<N2>
    where
        N2: AsTaskName,
    {
        OtelTaskMetricsRecorder {
            inner: self.inner.clone(),
            name: Some(name),
        }
    }
}

impl<N> TaskMetricsRecorder for OtelTaskMetricsRecorder<N>
where
    N: AsTaskName,
{
    fn record_task_started(&self) {
        self.inner
            .tasks_started
            .add(&otel::Context::new(), 1, &[self.task_name_kv()]);
    }

    fn record_task_finished(
        &self,
        total_duration: Duration,
        poll_duration: Duration,
        poll_entries: u64,
        completed: bool,
    ) {
        let total_duration_ms = duration_as_millis_f64(total_duration);
        let poll_duration_ms = duration_as_millis_f64(poll_duration);
        let attrs = [
            self.task_name_kv(),
            otel::KeyValue::new("completed", completed),
        ];
        let ctx = otel::Context::new();

        self.inner
            .total_duration
            .record(&ctx, total_duration_ms, &attrs);

        self.inner
            .poll_duration
            .record(&ctx, poll_duration_ms, &attrs);

        self.inner.poll_entries.add(&ctx, poll_entries, &attrs);
        self.inner.tasks_finished.add(&ctx, 1, &attrs);
    }
}

struct OtelRecorderInner {
    total_duration: Histogram<f64>,
    poll_duration: Histogram<f64>,
    poll_entries: Counter<u64>,
    tasks_started: Counter<u64>,
    tasks_finished: Counter<u64>,
}

impl OtelRecorderInner {
    fn new(prefix: &str) -> Self {
        let meter = ServiceMetrics::meter();

        Self {
            total_duration: meter.f64_histogram(format!("{prefix}_duration")).init(),
            poll_duration: meter
                .f64_histogram(format!("{prefix}_poll_duration"))
                .init(),
            poll_entries: meter.u64_counter(format!("{prefix}_poll_entries")).init(),
            tasks_started: meter.u64_counter(format!("{prefix}_started")).init(),
            tasks_finished: meter.u64_counter(format!("{prefix}_finished")).init(),
        }
    }
}
