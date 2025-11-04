//! Instrumentation machinery for collecting [`Future`] metrics.
//!
//! Usage:
//!
//! ```
//! use wc_metrics::{
//!     self as metrics,
//!     label_name,
//!     BoolLabel,
//!     FutureExt,
//!     FutureMetrics,
//!     LabeledFutureMetrics2,
//!     Lazy,
//! };
//!
//! type MyBoolLabelA = BoolLabel<{ label_name("my_bool_label_a") }>;
//! type MyBoolLabelB = BoolLabel<{ label_name("my_bool_label_b") }>;
//!
//! static FUTURE_METRICS_A: Lazy<FutureMetrics> = metrics::new("my_future_a");
//! static FUTURE_METRICS_B: Lazy<LabeledFutureMetrics2<MyBoolLabelA, MyBoolLabelB>> =
//!     metrics::builder("my_future_b")
//!         .with_static_labels(&[("labelA", "valueA"), ("labelA", "valueA")])
//!         .build();
//!
//! let fut_a = async {}.with_metrics(&FUTURE_METRICS_A);
//! let fut_b = async {}.with_metrics(
//!     FUTURE_METRICS_B.resolve_labels((MyBoolLabelA::new(false), MyBoolLabelB::new(true))),
//! );
//! ```

use {
    crate::{
        sealed::{Attrs, Metric},
        Lazy,
    },
    futures::future::FusedFuture,
    metrics::{counter, gauge, histogram, Counter, Gauge, Histogram, Label},
    std::{
        future::Future,
        pin::Pin,
        task::{Context, Poll},
        time::{Duration, Instant},
    },
};

/// Metric names used by this module.
pub mod name {
    pub const FUTURE_DURATION: &str = "future_duration";
    pub const FUTURE_CANCELLED_DURATION: &str = "future_cancelled_duration";

    pub const FUTURES_CREATED: &str = "futures_created_count";
    pub const FUTURES_STARTED: &str = "futures_started_count";
    pub const FUTURES_FINISHED: &str = "futures_finished_count";
    pub const FUTURES_CANCELLED: &str = "futures_cancelled_count";

    pub const FUTURES_IN_FLIGHT: &str = "futures_in_flight";

    pub const FUTURE_POLL_DURATION: &str = "future_poll_duration";
    pub const FUTURE_POLL_DURATION_MAX: &str = "future_poll_duration_max";
    pub const FUTURE_POLLS: &str = "future_polls_count";
}

/// Metrics collected during a [`Future`] execution.
pub struct Metrics {
    duration: Histogram,
    cancelled_duration: Histogram,

    created: Counter,
    started: Counter,
    finished: Counter,
    cancelled: Counter,

    in_flight: Gauge,

    poll_duration: Histogram,
    poll_duration_max: Gauge,
    polls: Counter,
}

impl Metric for Metrics {
    fn register(attrs: &Attrs) -> Self {
        let mut labels = attrs.labels();
        let name = Label::from_static_parts("future_name", attrs.name());
        labels.push(name);

        Self {
            duration: histogram!(name::FUTURE_DURATION, labels.iter()),
            cancelled_duration: histogram!(name::FUTURE_CANCELLED_DURATION, labels.iter()),
            created: counter!(name::FUTURES_CREATED, labels.iter()),
            started: counter!(name::FUTURES_STARTED, labels.iter()),
            finished: counter!(name::FUTURES_FINISHED, labels.iter()),
            cancelled: counter!(name::FUTURES_CANCELLED, labels.iter()),
            in_flight: gauge!(name::FUTURES_IN_FLIGHT, labels.iter()),
            poll_duration: histogram!(name::FUTURE_POLL_DURATION, labels.iter()),
            poll_duration_max: gauge!(name::FUTURE_POLL_DURATION_MAX, labels.iter()),
            polls: counter!(name::FUTURE_POLLS, labels.iter()),
        }
    }
}

/// Convienience extension `trait` for creating [`Metered`] [`Future`]s.
pub trait FutureExt: Sized {
    /// Consumes the future, returning a new future that records the executiion
    /// metrics of the inner future.
    fn with_metrics(self, metrics: impl Into<&'static Metrics>) -> Metered<Self> {
        Metered::new(self, metrics)
    }
}

impl<F> FutureExt for F where F: Future {}

/// [`Future`] wrapper collecting [`Metrics`] of inner [`Future`] `F`.
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

    poll_duration_sum: Duration,
    poll_duration_max: Duration,
    polls_count: usize,

    metrics: &'static Metrics,
}

impl<F> Metered<F> {
    fn new(future: F, metrics: impl Into<&'static Metrics>) -> Self {
        let metrics = metrics.into();

        metrics.created.increment(1);

        Self {
            future,
            state: State {
                started_at: None,
                is_finished: false,
                poll_duration_sum: Duration::from_secs(0),
                poll_duration_max: Duration::from_secs(0),
                polls_count: 0,
                metrics,
            },
        }
    }
}

impl From<&'static Lazy<Metrics>> for &'static Metrics {
    fn from(lazy: &'static Lazy<Metrics>) -> Self {
        lazy.get_or_register()
    }
}

impl<F: Future> Future for Metered<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        let mut this = self.project();
        let state = &mut this.state;

        if state.started_at.is_none() {
            state.started_at = Some(Instant::now());
            state.metrics.started.increment(1);
            state.metrics.in_flight.increment(1);
        }

        let poll_started_at = Instant::now();
        let result = this.future.poll(cx);
        let poll_duration = poll_started_at.elapsed();

        state.poll_duration_sum += poll_duration;
        state.poll_duration_max = state.poll_duration_max.max(poll_duration);
        state.polls_count += 1;

        if result.is_ready() {
            state.is_finished = true;

            state.metrics.finished.increment(1);

            if let Some(started_at) = state.started_at {
                state.metrics.duration.record(started_at.elapsed())
            }
        }

        result
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.metrics.in_flight.decrement(1);

        if !self.is_finished {
            self.metrics.cancelled.increment(1);

            if let Some(started_at) = self.started_at {
                self.metrics.cancelled_duration.record(started_at.elapsed())
            }
        }

        self.metrics.poll_duration.record(self.poll_duration_sum);
        self.metrics.poll_duration_max.set(self.poll_duration_max);
        self.metrics.polls.increment(self.polls_count as u64);
    }
}

impl<F: Future> FusedFuture for Metered<F> {
    fn is_terminated(&self) -> bool {
        self.state.is_finished
    }
}
