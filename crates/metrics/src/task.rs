use {
    super::{
        duration_as_millis_f64,
        future::{AsTaskName, TaskMetricsRecorder},
        otel,
        ServiceMetrics,
    },
    once_cell::sync::OnceCell,
    opentelemetry::metrics::{Counter, Histogram},
    smallvec::SmallVec,
    std::{ops::Deref, sync::Arc, time::Duration},
};

/// Wrapper for [`OtelTaskMetricsRecorder`], which can be statically
/// initialized.
pub struct TaskMetrics {
    prefix: &'static str,
    inner: OnceCell<OtelTaskMetricsRecorder>,
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
pub struct OtelTaskMetricsRecorder {
    inner: Arc<OtelRecorderInner>,
    name: &'static str,
    attributes: SmallVec<[otel::KeyValue; 2]>,
}

impl OtelTaskMetricsRecorder {
    pub fn new(prefix: &str) -> Self {
        Self {
            inner: Arc::new(OtelRecorderInner::new(prefix)),
            name: "unknown",
            attributes: SmallVec::new(),
        }
    }

    /// Clones the current recording context with a new task name.
    pub fn with_name<N>(&self, name: N) -> Self
    where
        N: AsTaskName,
    {
        Self {
            inner: self.inner.clone(),
            name: name.as_task_name(),
            attributes: self.attributes.clone(),
        }
    }

    /// Clones the current recording context with a new set of attributes.
    pub fn with_attributes(
        &self,
        attributes: impl IntoIterator<Item = otel::KeyValue>,
    ) -> OtelTaskMetricsRecorder {
        Self {
            inner: self.inner.clone(),
            name: self.name.clone(),
            attributes: attributes.into_iter().collect(),
        }
    }

    fn combine_attributes(&self) -> SmallVec<[otel::KeyValue; 4]> {
        let name = [otel::KeyValue::new("task_name", self.name)];
        let extra = self.attributes.iter().cloned();
        name.into_iter().chain(extra).collect()
    }
}

impl TaskMetricsRecorder for OtelTaskMetricsRecorder {
    fn record_task_started(&self) {
        self.inner
            .tasks_started
            .add(&otel::Context::new(), 1, &self.combine_attributes());
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

        let mut attrs = self.combine_attributes();
        attrs.push(otel::KeyValue::new("completed", completed));

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
