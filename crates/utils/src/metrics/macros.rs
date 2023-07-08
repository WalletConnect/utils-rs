/// Define a local static
/// [`ObservableGauge`](opentelemetry::metrics::ObservableGauge) and return a
/// reference to it, or immediately observe a value.
#[macro_export]
macro_rules! gauge {
    ($name:expr) => {{
        static METRIC: $crate::metrics::Lazy<$crate::metrics::otel::metrics::ObservableGauge<u64>> =
            $crate::metrics::Lazy::new(|| {
                $crate::metrics::ServiceMetrics::meter()
                    .u64_observable_gauge($name)
                    .init()
            });

        &METRIC
    }};

    ($name:expr, $value:expr) => {{
        $crate::gauge!($name, $value, &[]);
    }};

    ($name:expr, $value:expr, $tags:expr) => {{
        $crate::gauge!($name).observe(&$crate::metrics::otel::Context::new(), $value as u64, $tags);
    }};
}

/// Define a local static [`Histogram`](opentelemetry::metrics::Histogram) and
/// return a reference to it, or immediately record a value.
#[macro_export]
macro_rules! histogram {
    ($name:expr) => {{
        static METRIC: $crate::metrics::Lazy<$crate::metrics::otel::metrics::Histogram<f64>> =
            $crate::metrics::Lazy::new(|| {
                $crate::metrics::ServiceMetrics::meter()
                    .f64_histogram($name)
                    .init()
            });

        &METRIC
    }};

    ($name:expr, $value:expr) => {{
        $crate::histogram!($name, $value, &[]);
    }};

    ($name:expr, $value:expr, $tags:expr) => {{
        $crate::histogram!($name).record(
            &$crate::metrics::otel::Context::new(),
            $value as f64,
            $tags,
        );
    }};
}

/// Define a local static [`Counter`](opentelemetry::metrics::Counter) and
/// return a reference to it, or immediately add a value.
#[macro_export]
macro_rules! counter {
    ($name:expr) => {{
        static METRIC: $crate::metrics::Lazy<$crate::metrics::otel::metrics::Counter<u64>> =
            $crate::metrics::Lazy::new(|| {
                $crate::metrics::ServiceMetrics::meter()
                    .u64_counter($name)
                    .init()
            });

        &METRIC
    }};

    ($name:expr, $value:expr) => {{
        $crate::counter!($name, $value, &[]);
    }};

    ($name:expr, $value:expr, $tags:expr) => {{
        $crate::counter!($name).add(&$crate::metrics::otel::Context::new(), $value as u64, $tags);
    }};
}
