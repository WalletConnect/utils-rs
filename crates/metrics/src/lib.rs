//! Alternative API facade to the [`metrics`] backend.
//!
//! Priorities: performance, ergonomics (in that order).
//!
//! Metrics reporting should be as fast as possible, without substantially
//! hurting the code ergonomics.
//!
//! A trivial atomic counter increment MUST NOT allocate stuff on the heap
//! (looking at you [`metrics::counter`] and it SHOULD NOT acquire locks or do
//! [`HashMap`](std::collections::HashMap) lookups unless absolutely necessary.
//!
//! If your metric is only being used once, or you can cache it somewhere
//! consider using [`counter`], [`gauge`] or [`histogram`] convinience macros.
//! The macros are completely optional and the machinery can be used
//! as is without them.
//!
//! # Usage
//!
//! ```
//! use wc_metrics::{
//!     self as metrics,
//!     enum_ordinalize::Ordinalize,
//!     label_name,
//!     BoolLabel,
//!     Counter,
//!     Enum,
//!     EnumLabel,
//!     Gauge,
//!     Histogram,
//!     LabeledCounter2,
//!     LabeledGauge3,
//!     LabeledHistogram,
//!     Lazy,
//!     Optional,
//!     StringLabel,
//! };
//!
//! #[derive(Clone, Copy, Ordinalize)]
//! enum MyEnum {
//!     A,
//!     B,
//! }
//!
//! type MyEnumLabel = EnumLabel<{ label_name("my_enum_label") }, MyEnum>;
//!
//! impl Enum for MyEnum {
//!     fn as_str(&self) -> &'static str {
//!         match self {
//!             Self::A => "a",
//!             Self::B => "b",
//!         }
//!     }
//! }
//!
//! type MyBoolLabel = BoolLabel<{ label_name("my_bool_label") }>;
//! type MyStringLabel = StringLabel<{ label_name("my_string_label") }>;
//! type MyU8StringLabel = StringLabel<{ label_name("my_u8_label") }, u8>;
//!
//! static COUNTER_A: Lazy<Counter> = metrics::new("counter_a");
//!
//! static GAUGE_A: Lazy<Gauge> = metrics::builder("gauge_a")
//!     .with_description("My gauge")
//!     .with_static_labels(&[("labelA", "valueA"), ("labelA", "valueA")])
//!     .build();
//!
//! static HISTOGRAM_A: Lazy<LabeledHistogram<MyEnumLabel>> = metrics::new("histogram_a");
//!
//! static COUNTER_B: Lazy<LabeledCounter2<MyStringLabel, MyBoolLabel>> =
//!     metrics::builder("counter_b")
//!         .with_description("My labeled counter")
//!         .build();
//!
//! static GAUGE_B: Lazy<LabeledGauge3<MyU8StringLabel, MyEnumLabel, Optional<MyBoolLabel>>> =
//!     metrics::new("gauge_b");
//!
//! COUNTER_A.increment(1);
//! GAUGE_A.set(42);
//! HISTOGRAM_A.record(1000, (MyEnumLabel::new(MyEnum::A),));
//! COUNTER_B.increment(2u64, (MyStringLabel::new("test"), MyBoolLabel::new(false)));
//!
//! let labels = (MyU8StringLabel::new(&42), MyEnumLabel::new(MyEnum::B), None);
//! GAUGE_B.decrement(2, labels);
//! ```

// Re-export it to make sure that we use the compatible version.
#[cfg(feature = "exporter_prometheus")]
pub use metrics_exporter_prometheus as exporter_prometheus;
pub use {
    enum_ordinalize,
    label::{label_name, BoolLabel, Enum, EnumLabel, LabelName, Optional, StringLabel, WithLabel},
    lazy::Lazy,
    metrics::{self as backend, Counter, Gauge, Histogram},
};
use {
    label::{DynamicLabels, Labeled, Labeled2, Labeled3, Labeled4, StaticLabels},
    metrics::{IntoF64, Label},
    sealed::{Attrs, Decrement, Execute, Increment, Metric, Record, Set},
};

mod label;
mod lazy;
mod macros;

#[cfg(test)]
mod examples;
#[cfg(test)]
mod test;

#[cfg(feature = "future")]
pub mod future;
#[cfg(feature = "future")]
pub use future::{FutureExt, Metrics as FutureMetrics};

/// Builder of [`Lazy`] metrics.
///
/// Intended to be used exclusively in const contexts to specify metric
/// attributes known at the compile time and to assign [`Lazy`] metrics to
/// `static` variables.
pub struct Builder {
    attrs: StaticAttrs,
}

/// Creates a new [`Builder`] with the specified metric `name`.
///
/// For `future` metrics `name` is going to be used as `future_name` label
/// value instead.
pub const fn builder(name: &'static str) -> Builder {
    Builder {
        attrs: StaticAttrs {
            name,
            description: None,
            labels: &[],
        },
    }
}

/// Creates a new [`Lazy`] metric with the specified `name`.
///
/// For `future` metrics `name` is going to be used as `future_name` label
/// value instead.
pub const fn new<M: Metric>(name: &'static str) -> Lazy<M> {
    builder(name).build()
}

impl Builder {
    /// Specifies description of the metric.
    ///
    /// No-op for `future` metrics.
    pub const fn with_description(mut self, description: &'static str) -> Self {
        self.attrs.description = Some(description);
        self
    }

    /// Specifies statically known metric labels.
    pub const fn with_static_labels(
        mut self,
        labels: &'static [(&'static str, &'static str)],
    ) -> Self {
        self.attrs.labels = labels;
        self
    }

    /// Builds the [`Lazy`] metric.
    pub const fn build<M: Metric>(self) -> Lazy<M> {
        Lazy::new(self.attrs)
    }
}

impl Attrs {
    fn name(&self) -> &'static str {
        self.static_.name
    }

    fn description(&self) -> Option<&'static str> {
        self.static_.description
    }

    fn labels(&self) -> DynamicLabels {
        let mut labels = self.dynamic.labels.clone();
        let static_ = self.static_.labels.iter();
        labels.extend(static_.map(|(k, v)| Label::from_static_parts(k, v)));
        labels
    }

    fn with_label(&self, label: Label) -> Self {
        let mut this = self.clone();
        this.dynamic.labels.push(label);
        this
    }
}

#[derive(Clone, Copy, Debug)]
struct StaticAttrs {
    name: &'static str,
    description: Option<&'static str>,
    labels: StaticLabels,
}

#[derive(Clone, Debug, Default)]
struct DynamicAttrs {
    labels: DynamicLabels,
}

mod sealed {
    use crate::{DynamicAttrs, StaticAttrs};

    #[derive(Clone, Debug)]
    pub struct Attrs {
        pub(super) static_: StaticAttrs,
        pub(super) dynamic: DynamicAttrs,
    }

    pub trait Metric {
        fn register(attrs: &Attrs) -> Self;
    }

    pub trait Execute<Op, L> {
        fn execute(&self, op: Op, labels: L);
    }

    pub struct Increment<T>(pub T);
    pub struct Decrement<T>(pub T);
    pub struct Set<T>(pub T);
    pub struct Record<T>(pub T);
}

pub type LabeledCounter<A> = Labeled<Counter, A>;
pub type LabeledCounter2<A, B> = Labeled2<Counter, A, B>;
pub type LabeledCounter3<A, B, C> = Labeled3<Counter, A, B, C>;
pub type LabeledCounter4<A, B, C, D> = Labeled4<Counter, A, B, C, D>;

impl Metric for Counter {
    fn register(attrs: &Attrs) -> Self {
        let counter = backend::counter!(attrs.name(), attrs.labels().iter());
        if let Some(desc) = attrs.description() {
            backend::describe_counter!(attrs.name(), desc);
        }
        counter
    }
}

impl<T> Execute<Increment<T>, ()> for Counter
where
    T: Into<u64>,
{
    fn execute(&self, op: Increment<T>, _labels: ()) {
        self.increment(op.0.into())
    }
}

pub type LabeledGauge<A> = Labeled<Gauge, A>;
pub type LabeledGauge2<A, B> = Labeled2<Gauge, A, B>;
pub type LabeledGauge3<A, B, C> = Labeled3<Gauge, A, B, C>;
pub type LabeledGauge4<A, B, C, D> = Labeled4<Gauge, A, B, C, D>;

impl Metric for Gauge {
    fn register(attrs: &Attrs) -> Self {
        let gauge = backend::gauge!(attrs.name(), attrs.labels().iter());
        if let Some(desc) = attrs.description() {
            backend::describe_gauge!(attrs.name(), desc);
        }
        gauge
    }
}

impl<T> Execute<Increment<T>, ()> for Gauge
where
    T: IntoF64,
{
    fn execute(&self, op: Increment<T>, _labels: ()) {
        self.increment(op.0)
    }
}

impl<T> Execute<Decrement<T>, ()> for Gauge
where
    T: IntoF64,
{
    fn execute(&self, op: Decrement<T>, _labels: ()) {
        self.decrement(op.0)
    }
}

impl<T> Execute<Set<T>, ()> for Gauge
where
    T: IntoF64,
{
    fn execute(&self, op: Set<T>, _labels: ()) {
        self.set(op.0)
    }
}

pub type LabeledHistogram<A> = Labeled<Histogram, A>;
pub type LabeledHistogram2<A, B> = Labeled2<Histogram, A, B>;
pub type LabeledHistogram3<A, B, C> = Labeled3<Histogram, A, B, C>;
pub type LabeledHistogram4<A, B, C, D> = Labeled4<Histogram, A, B, C, D>;

impl Metric for Histogram {
    fn register(attrs: &Attrs) -> Self {
        let histogram = backend::histogram!(attrs.name(), attrs.labels().iter());
        if let Some(desc) = attrs.description() {
            backend::describe_histogram!(attrs.name(), desc);
        }
        histogram
    }
}

impl<T> Execute<Record<T>, ()> for Histogram
where
    T: IntoF64,
{
    fn execute(&self, op: Record<T>, _labels: ()) {
        self.record(op.0)
    }
}

#[cfg(feature = "future")]
pub type LabeledFutureMetrics<A> = Labeled<FutureMetrics, A>;
#[cfg(feature = "future")]
pub type LabeledFutureMetrics2<A, B> = Labeled2<FutureMetrics, A, B>;
#[cfg(feature = "future")]
pub type LabeledFutureMetrics3<A, B, C> = Labeled3<FutureMetrics, A, B, C>;
#[cfg(feature = "future")]
pub type LabeledFutureMetrics4<A, B, C, D> = Labeled4<FutureMetrics, A, B, C, D>;

pub type OptionalEnumLabel<const NAME: LabelName, T> = Optional<EnumLabel<NAME, T>>;
pub type OptionalBoolLabel<const NAME: LabelName> = Optional<BoolLabel<NAME>>;
pub type OptionalStringLabel<const NAME: LabelName, T = String> = Optional<StringLabel<NAME, T>>;
