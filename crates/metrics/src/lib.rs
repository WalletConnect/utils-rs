pub use {future::*, once_cell::sync::Lazy, opentelemetry as otel, task::*};
use {
    opentelemetry_sdk::metrics::SdkMeterProvider,
    otel::metrics::{Meter, MeterProvider},
    prometheus::{Error as PrometheusError, Registry, TextEncoder},
    std::{
        sync::{Arc, Mutex},
        time::Duration,
    },
};

pub mod future;
pub mod macros;
pub mod task;

const DEFAULT_SERVICE_NAME: &str = "unknown_service";

static SERVICE_NAME: Mutex<Option<&str>> = Mutex::new(None);

static METRICS_CORE: Lazy<Arc<ServiceMetrics>> = Lazy::new(|| {
    let service_name = SERVICE_NAME.lock().unwrap().unwrap_or(DEFAULT_SERVICE_NAME);

    let registry = Registry::new();
    let prometheus_exporter = opentelemetry_prometheus::exporter()
        .with_registry(registry.clone())
        .build()
        .unwrap();
    let provider = SdkMeterProvider::builder()
        .with_reader(prometheus_exporter)
        .build();
    let meter = provider.meter(service_name);

    Arc::new(ServiceMetrics { registry, meter })
});

/// Global application metrics access.
///
/// The main functionality is to provide global access to opentelemetry's
/// [`Meter`].
pub struct ServiceMetrics {
    registry: Registry,
    meter: Meter,
}

impl ServiceMetrics {
    /// Initializes service metrics with the default name.
    ///
    /// # Panics
    ///
    /// Panics if either prometheus exporter or opentelemetry meter fails to
    /// initialize.
    pub fn init() {
        Lazy::force(&METRICS_CORE);
    }

    /// Initializes service metrics with the specified name.
    ///
    /// # Panics
    ///
    /// Panics if either prometheus exporter or opentelemetry meter fails to
    /// initialize.
    pub fn init_with_name(name: &'static str) {
        *SERVICE_NAME.lock().unwrap() = Some(name);
        Lazy::force(&METRICS_CORE);
    }

    /// Generates export data in Prometheus format, serialized into string.
    pub fn export() -> Result<String, PrometheusError> {
        let data = Self::get().registry.gather();
        TextEncoder::new().encode_to_string(&data)
    }

    /// Returns a static reference to [`Meter`] which can be used to set up
    /// global static counters. See [`crate::counter`] macro for an example.
    #[inline]
    pub fn meter() -> &'static Meter {
        &Self::get().meter
    }

    /// Global access to the application metrics singleton.
    #[inline]
    fn get() -> &'static Self {
        METRICS_CORE.as_ref()
    }
}

#[inline]
pub fn duration_as_millis_f64(val: Duration) -> f64 {
    val.as_secs_f64() * 1000.0
}

#[inline]
pub fn value_bucket<const NUM_BUCKETS: usize>(
    size: usize,
    buckets: &'static [usize; NUM_BUCKETS],
) -> usize {
    *buckets
        .iter()
        .find(|&bucket| size <= *bucket)
        .or_else(|| buckets.last())
        .unwrap_or(&0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_buckets() {
        const BUCKETS: [usize; 8] = [128, 256, 512, 2048, 4096, 65535, 131070, 262140];

        assert_eq!(value_bucket(0, &BUCKETS), 128);
        assert_eq!(value_bucket(65536, &BUCKETS), 131070);
        assert_eq!(value_bucket(131070, &BUCKETS), 131070);
        assert_eq!(value_bucket(131071, &BUCKETS), 262140);
        assert_eq!(value_bucket(usize::MAX, &BUCKETS), 262140);
    }
}
