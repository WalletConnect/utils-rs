use {
    crate::{
        label::{DynamicLabel, ResolveLabels, WithLabel},
        sealed::{Decrement, Execute, Increment, Record, Set},
        Attrs, Metric, StaticAttrs,
    },
    metrics::{Counter, Gauge, Histogram, IntoF64},
    std::sync::OnceLock,
};

/// Lazily initialized metric.
///
/// Can only be used if assigned to a `static` variable.
///
/// Use [`Builder`](crate::Builder) to specify metric attributes known at the
/// complile time and to build [`Lazy`] metrics.
pub struct Lazy<M> {
    metric: OnceLock<M>,
    attrs: StaticAttrs,
}

impl<M: Metric> Lazy<M> {
    pub(super) const fn new(attrs: StaticAttrs) -> Self {
        Self {
            metric: OnceLock::new(),
            attrs,
        }
    }

    pub(crate) fn get_or_register(&self) -> &M {
        if let Some(m) = self.metric.get() {
            return m;
        };

        let attrs = Attrs {
            static_: self.attrs,
            dynamic: Default::default(),
        };

        self.metric.get_or_init(|| M::register(&attrs))
    }
}

impl Lazy<Counter> {
    /// See [`Counter::increment`].
    pub fn increment(&'static self, value: u64) {
        self.get_or_register().increment(value)
    }
}

impl Lazy<Gauge> {
    /// See [`Gauge::increment`].
    pub fn increment<T: IntoF64>(&'static self, value: T) {
        self.get_or_register().increment(value)
    }

    /// See [`Gauge::decrement`].
    pub fn decrement<T: IntoF64>(&'static self, value: T) {
        self.get_or_register().decrement(value)
    }

    /// See [`Gauge::set`].
    pub fn set<T: IntoF64>(&'static self, value: T) {
        self.get_or_register().set(value)
    }
}

impl Lazy<Histogram> {
    /// See [`Histogram::record`].
    pub fn record<T: IntoF64>(&'static self, value: T) {
        self.get_or_register().record(value)
    }
}

impl<L, M> Lazy<WithLabel<L, M>>
where
    L: DynamicLabel<M>,
{
    /// See [`WithLabel::resolve_label`].
    pub fn resolve_label<T>(&'static self, label: T) -> &'static M
    where
        WithLabel<L, M>: Metric + ResolveLabels<(T,), Target = M>,
    {
        self.get_or_register().resolve_label(label)
    }

    /// See [`WithLabel::resolve_labels`].
    pub fn resolve_labels<LS>(
        &'static self,
        labels: LS,
    ) -> &'static <WithLabel<L, M> as ResolveLabels<LS>>::Target
    where
        WithLabel<L, M>: Metric + ResolveLabels<LS>,
    {
        self.get_or_register().resolve_labels(labels)
    }

    /// Calls [`Counter::increment`] or [`Gauge::increment`] on the metric built
    /// using the provided labels.
    pub fn increment<T, Labels>(&'static self, value: T, labels: Labels)
    where
        WithLabel<L, M>: Metric + Execute<Increment<T>, Labels>,
    {
        self.get_or_register().execute(Increment(value), labels);
    }

    /// Calls [`Gauge::decrement`] on the metric built using the provided
    /// labels.
    pub fn decrement<T, Labels>(&'static self, value: T, labels: Labels)
    where
        WithLabel<L, M>: Metric + Execute<Decrement<T>, Labels>,
    {
        self.get_or_register().execute(Decrement(value), labels);
    }

    /// Calls [`Gauge::set`] on the metric built using the provided labels.
    pub fn set<T, Labels>(&'static self, value: T, labels: Labels)
    where
        WithLabel<L, M>: Metric + Execute<Set<T>, Labels>,
    {
        self.get_or_register().execute(Set(value), labels);
    }

    /// Calls [`Histogram::record`] on the metric built using the provided
    /// labels.
    pub fn record<T, Labels>(&'static self, value: T, labels: Labels)
    where
        WithLabel<L, M>: Metric + Execute<Record<T>, Labels>,
    {
        self.get_or_register().execute(Record(value), labels);
    }
}
