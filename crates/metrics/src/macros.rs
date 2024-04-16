/// Define a local static
/// [`ObservableGauge`](opentelemetry::metrics::ObservableGauge) and return a
/// reference to it, or immediately observe a value.
#[macro_export]
macro_rules! gauge {
    ($name:expr) => {{
        static METRIC: $crate::Lazy<$crate::otel::metrics::ObservableGauge<u64>> =
            $crate::Lazy::new(|| {
                $crate::ServiceMetrics::meter()
                    .u64_observable_gauge($name)
                    .init()
            });

        &METRIC
    }};

    ($name:expr, $value:expr) => {{
        $crate::gauge!($name, $value, &[]);
    }};

    ($name:expr, $value:expr, $tags:expr) => {{
        $crate::gauge!($name).observe($value as u64, $tags);
    }};
}

/// Define a local static [`Histogram`](opentelemetry::metrics::Histogram) and
/// return a reference to it, or immediately record a value.
#[macro_export]
macro_rules! histogram {
    ($name:expr) => {{
        static METRIC: $crate::Lazy<$crate::otel::metrics::Histogram<f64>> =
            $crate::Lazy::new(|| $crate::ServiceMetrics::meter().f64_histogram($name).init());

        &METRIC
    }};

    ($name:expr, $value:expr) => {{
        $crate::histogram!($name, $value, &[]);
    }};

    ($name:expr, $value:expr, $tags:expr) => {{
        $crate::histogram!($name).record($value as f64, $tags);
    }};
}

/// Define a local static [`Counter`](opentelemetry::metrics::Counter) and
/// return a reference to it, or immediately add a value.
#[macro_export]
macro_rules! counter {
    ($name:expr) => {{
        static METRIC: $crate::Lazy<$crate::otel::metrics::Counter<u64>> =
            $crate::Lazy::new(|| $crate::ServiceMetrics::meter().u64_counter($name).init());

        &METRIC
    }};

    ($name:expr, $value:expr) => {{
        $crate::counter!($name, $value, &[]);
    }};

    ($name:expr, $value:expr, $tags:expr) => {{
        $crate::counter!($name).add($value as u64, $tags);
    }};
}
