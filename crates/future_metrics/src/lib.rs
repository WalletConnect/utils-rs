use {
    metrics::{Counter, Histogram, Key, Label, Level, Metadata},
    std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
        time::{Duration, Instant},
    },
};

/// Target specified in [`metrics::Metadata`] for all metrics produced by this
/// crate.
pub const METADATA_TARGET: &str = "future_metrics";

/// Metric names used by this crate.
pub mod metric_name {
    pub const FUTURE_DURATION: &str = "future_duration";

    pub const FUTURES_CREATED: &str = "futures_created";
    pub const FUTURES_STARTED: &str = "futures_started";
    pub const FUTURES_FINISHED: &str = "futures_finished";
    pub const FUTURES_CANCELLED: &str = "futures_cancelled";

    pub const FUTURE_POLL_DURATION: &str = "future_poll_duration";
    pub const FUTURE_POLL_DURATION_TOTAL: &str = "future_poll_duration_total";
}

/// Creates a new label identifying a future by its name.
pub const fn future_name(s: &'static str) -> Label {
    Label::from_static_parts("future_name", s)
}

pub trait FutureExt: Sized {
    /// Consumes the future, returning a new future that records the executiion
    /// metrics of the inner future.
    ///
    /// It is expected that you provide at least one label identifying the
    /// future being metered.
    /// Consider using [`future_name`] label, or the [`FutureExt::with_metrics`]
    /// shortcut.
    fn with_labeled_metrics(self, labels: &'static [Label]) -> Metered<Self> {
        Metered::new(self, labels)
    }

    /// A shortcut for [`FutureExt::with_labeled_metrics`] using a single label
    /// only (presumably [`future_name`]).
    fn with_metrics(self, label: &'static Label) -> Metered<Self> {
        self.with_labeled_metrics(std::slice::from_ref(label))
    }
}

impl<F> FutureExt for F where F: Future {}

#[pin_project::pin_project]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Metered<F> {
    #[pin]
    future: F,
    state: State,
}

struct State {
    started_at: Option<Instant>,
    is_finished: bool,

    poll_duration_total: Duration,

    metrics: Metrics,
}

impl<F> Metered<F> {
    fn new(future: F, metric_labels: &'static [Label]) -> Self {
        let metrics = Metrics::new(metric_labels);

        metrics.created.increment(1);

        Self {
            future,
            state: State {
                started_at: None,
                is_finished: false,
                poll_duration_total: Duration::from_secs(0),
                metrics,
            },
        }
    }
}

impl<F: Future> Future for Metered<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        let s = &mut this.state;

        if s.started_at.is_none() {
            s.started_at = Some(Instant::now());
            s.metrics.started.increment(1);
        }

        let poll_started_at = Instant::now();
        let result = this.future.poll(cx);
        let poll_duration = poll_started_at.elapsed();

        s.metrics.poll_duration.record(poll_duration);
        s.poll_duration_total += poll_duration;

        if result.is_ready() && !s.is_finished {
            s.is_finished = true;
            s.metrics.finished.increment(1);
            s.metrics
                .duration
                .record(s.started_at.unwrap_or(poll_started_at).elapsed())
        }

        result
    }
}

impl Drop for State {
    fn drop(&mut self) {
        if !self.is_finished {
            self.metrics.cancelled.increment(1);
            if let Some(started_at) = self.started_at {
                self.metrics.duration.record(started_at.elapsed())
            }
        }

        self.metrics
            .poll_duration_total
            .record(duration_as_millis_f64(self.poll_duration_total));
    }
}

struct Metrics {
    duration: Histogram,

    created: Counter,
    started: Counter,
    finished: Counter,
    cancelled: Counter,

    poll_duration: Histogram,
    poll_duration_total: Histogram,
}

impl Metrics {
    fn new(labels: &'static [Label]) -> Self {
        metrics::with_recorder(|r| {
            let metadata = Metadata::new(METADATA_TARGET, Level::INFO, None);

            Self {
                duration: r.register_histogram(
                    &Key::from_static_parts(metric_name::FUTURE_DURATION, labels),
                    &metadata,
                ),
                created: r.register_counter(
                    &Key::from_static_parts(metric_name::FUTURES_CREATED, labels),
                    &metadata,
                ),
                started: r.register_counter(
                    &Key::from_static_parts(metric_name::FUTURES_STARTED, labels),
                    &metadata,
                ),
                finished: r.register_counter(
                    &Key::from_static_parts(metric_name::FUTURES_FINISHED, labels),
                    &metadata,
                ),
                cancelled: r.register_counter(
                    &Key::from_static_parts(metric_name::FUTURES_CANCELLED, labels),
                    &metadata,
                ),
                poll_duration: r.register_histogram(
                    &Key::from_static_parts(metric_name::FUTURE_POLL_DURATION, labels),
                    &metadata,
                ),
                poll_duration_total: r.register_histogram(
                    &Key::from_static_parts(metric_name::FUTURE_POLL_DURATION_TOTAL, labels),
                    &metadata,
                ),
            }
        })
    }
}

#[inline]
pub fn duration_as_millis_f64(val: Duration) -> f64 {
    val.as_secs_f64() * 1000.0
}