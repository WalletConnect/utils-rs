use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

/// Trait for tracking task execution related metrics with
/// [`TaskMetricsFuture`].
///
/// Most of the time [`OtelTaskMetricsRecorder`] should be used instead of
/// manual implementations of this trait, unless we want to support multiple
/// metrics tracking APIs.
pub trait TaskMetricsRecorder: Send + Sync + 'static {
    fn record_task_started(&self) {}

    fn record_task_finished(
        &self,
        _total_duration: Duration,
        _poll_duration: Duration,
        _poll_entries: u64,
        _completed: bool,
    ) {
    }
}

/// Trait that implements task name tagging using a static string.
pub trait AsTaskName: Send + Sync + 'static {
    fn as_task_name(&self) -> &'static str;
}

impl AsTaskName for () {
    fn as_task_name(&self) -> &'static str {
        ""
    }
}

impl AsTaskName for &'static str {
    fn as_task_name(&self) -> &'static str {
        self
    }
}

struct Stats<R: TaskMetricsRecorder> {
    started: Instant,
    completed: bool,
    poll_duration: Duration,
    poll_entries: u64,
    recorder: R,
}

impl<R> Stats<R>
where
    R: TaskMetricsRecorder,
{
    fn new(recorder: R) -> Self {
        recorder.record_task_started();

        Self {
            started: Instant::now(),
            completed: false,
            poll_duration: Duration::from_secs(0),
            poll_entries: 0,
            recorder,
        }
    }
}

impl<R> Drop for Stats<R>
where
    R: TaskMetricsRecorder,
{
    fn drop(&mut self) {
        self.recorder.record_task_finished(
            self.started.elapsed(),
            self.poll_duration,
            self.poll_entries,
            self.completed,
        );
    }
}

#[pin_project::pin_project]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct TaskMetricsFuture<F, R>
where
    F: Future,
    R: TaskMetricsRecorder,
{
    #[pin]
    inner: F,
    stats: Stats<R>,
}

impl<F, R> TaskMetricsFuture<F, R>
where
    F: Future,
    R: TaskMetricsRecorder,
{
    pub fn new(inner: F, recorder: R) -> Self {
        Self {
            inner,
            stats: Stats::new(recorder),
        }
    }
}

impl<F, R> Future for TaskMetricsFuture<F, R>
where
    F: Future,
    R: TaskMetricsRecorder,
{
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let poll_start = Instant::now();
        let this = self.project();
        let result = this.inner.poll(cx);

        if result.is_ready() {
            this.stats.completed = true;
        }

        this.stats.poll_entries += 1;
        this.stats.poll_duration += poll_start.elapsed();

        result
    }
}
