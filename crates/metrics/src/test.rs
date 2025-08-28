use {
    metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle},
    prometheus_parse::{HistogramCount, Value},
    std::collections::HashMap,
    tikv_jemalloc_ctl as alloc,
};

#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[test]
fn suite() {
    use crate::examples::{
        macros_counter::counters, macros_future_metrics::future_metrics, macros_gauge::gauges,
        macros_histogram::histograms,
    };

    let mut metrics = Metrics::new();

    let allocated1 = measure_heap_allocation();

    counters(1);
    gauges(1.0);
    histograms(1.0);
    #[cfg(feature = "future")]
    smol::block_on(future_metrics());

    let allocated2 = measure_heap_allocation();

    assert!(allocated2 > allocated1);
    assert!(
        allocated2 - allocated1 < 1024 * 1024,
        "before: {allocated1}, after: {allocated2}"
    );

    metrics.scrape();

    metrics.assert_counters(1);
    metrics.assert_gauges(1.0);
    metrics.assert_histograms(1.0);
    #[cfg(feature = "future")]
    metrics.assert_future_metrics(1.0);

    const ITERATIONS: usize = 1_000_000;

    // Histograms are not included here, because their current implementation is
    // leaky in `metrics` itself. It needs to be consumed consistently by the
    // `prometheus` exporter to not leak.
    //
    // This crate adds identical abstractions on top of every `metrics` metric, so
    // we can be pretty sure that at least our own code doesn't leak by just
    // checking the counters.
    let allocated1 = measure_heap_allocation();
    for n in 0..ITERATIONS {
        counters(1);
        gauges(n as f64);
    }
    let allocated2 = measure_heap_allocation();

    assert_eq!(allocated1, allocated2,);

    metrics.scrape();

    metrics.assert_counters(ITERATIONS as u64 + 1);
    metrics.assert_gauges(ITERATIONS as f64 - 1.0);
}

struct Metrics {
    prometheus: PrometheusHandle,
    scrape: Option<prometheus_parse::Scrape>,
}

impl Metrics {
    fn new() -> Self {
        Self {
            prometheus: PrometheusBuilder::new()
                .set_buckets_for_metric(Matcher::Prefix("histogram".into()), &[0.0])
                .unwrap()
                .set_buckets_for_metric(Matcher::Prefix("future".into()), &[0.0])
                .unwrap()
                .install_recorder()
                .unwrap(),
            scrape: None,
        }
    }

    fn scrape(&mut self) {
        let rendered = self.prometheus.render();
        print!("{rendered}");

        self.scrape = Some(
            prometheus_parse::Scrape::parse(rendered.lines().map(ToString::to_string).map(Ok))
                .unwrap(),
        );
    }

    fn assert_counters(&mut self, value: u64) {
        self.assert_metrics("counter", Value::Counter(value as f64))
    }

    fn assert_gauges(&mut self, value: f64) {
        self.assert_metrics("gauge", Value::Gauge(value))
    }

    fn assert_histograms(&mut self, count: f64) {
        self.assert_metrics("histogram", expected_histogram(count))
    }

    fn assert_metrics(&mut self, ty: &'static str, value: Value) {
        let name = |n| format!("{ty}{n}");

        self.assert_metric(&name(1), None, &[], &value);
        self.assert_metric(&name(2), None, &[("e", "a")], &value);
        self.assert_metric(&name(3), None, &[("b", "true")], &value);
        self.assert_metric(&name(4), None, &[("s", "a")], &value);
        self.assert_metric(&name(5), None, &[("s", "42")], &value);

        let labels = &[("e", "a"), ("s1", "a"), ("s2", "42"), ("b", "true")];
        self.assert_metric(&name(6), None, labels, &value);

        self.assert_metric(&name(7), None, &[("st", "1")], &value);

        let labels = &[("st1", "1"), ("st2", "2")];
        self.assert_metric(&name(8), None, labels, &value);

        let labels = &[("s", "42"), ("st", "2")];
        self.assert_metric(&name(9), None, labels, &value);

        let labels = &[
            ("e", "a"),
            ("s1", "a"),
            ("s2", "42"),
            ("b", "true"),
            ("st1", "1"),
            ("st2", "2"),
        ];
        self.assert_metric(&name(10), None, labels, &value);

        self.assert_metric(&name(11), Some("description11"), &[], &value);
        self.assert_metric(&name(12), Some("description12"), &[("e", "a")], &value);
        self.assert_metric(&name(13), Some("description13"), &[("b", "true")], &value);
        self.assert_metric(&name(14), Some("description14"), &[("s", "a")], &value);
        self.assert_metric(&name(15), Some("description15"), &[("s", "42")], &value);

        let labels = &[("e", "a"), ("s1", "a"), ("s2", "42"), ("b", "true")];
        self.assert_metric(&name(16), Some("description16"), labels, &value);

        self.assert_metric(&name(17), Some("description17"), &[("st", "1")], &value);

        let labels = &[("st1", "1"), ("st2", "2")];
        self.assert_metric(&name(18), Some("description18"), labels, &value);

        let labels = &[("s", "42"), ("st", "2")];
        self.assert_metric(&name(19), Some("description19"), labels, &value);

        let labels = &[
            ("e", "a"),
            ("s1", "a"),
            ("s2", "42"),
            ("b", "true"),
            ("oe", "a"),
            ("os1", "a"),
            ("os2", "42"),
            ("ob", "true"),
            ("st1", "1"),
            ("st2", "2"),
        ];
        self.assert_metric(&name(20), Some("description20"), labels, &value);
    }

    fn assert_metric(
        &mut self,
        name: &str,
        description: Option<&str>,
        labels: &[(&str, &str)],
        value: &Value,
    ) {
        let labels: HashMap<_, _> = labels
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();

        let scrape = self.scrape.as_mut().unwrap();

        let samples = &mut scrape.samples;
        let idx = samples
            .iter()
            .position(|m| m.metric == name && *m.labels == labels)
            .unwrap_or_else(|| panic!("{name} {labels:?} missing"));
        let metric = samples.remove(idx);
        assert_eq!(&metric.value, value);

        let doc = scrape.docs.remove(name);
        assert_eq!(doc, description.map(ToString::to_string))
    }

    #[cfg(feature = "future")]
    fn assert_future_metrics(&mut self, count: f64) {
        use crate::future::name;

        self.assert_future_metrics_(name::FUTURES_CREATED, Value::Counter(count));
        self.assert_future_metrics_(name::FUTURES_STARTED, Value::Counter(count));
        self.assert_future_metrics_(name::FUTURES_FINISHED, Value::Counter(count));
        self.assert_future_metrics_(name::FUTURE_POLLS, Value::Counter(count));

        self.assert_future_metrics_(name::FUTURE_DURATION, expected_histogram(count));
        self.assert_future_metrics_(name::FUTURE_POLL_DURATION, expected_histogram(count));
    }

    #[cfg(feature = "future")]
    fn assert_future_metrics_(&mut self, name: &str, value: Value) {
        let future_names: Vec<String> = (1..=10).map(|n| format!("future_metrics{n}")).collect();
        let future_name = |n: usize| ("future_name", future_names[n].as_str());

        self.assert_metric(name, None, &[future_name(0)], &value);
        self.assert_metric(name, None, &[future_name(1), ("e", "a")], &value);
        self.assert_metric(name, None, &[future_name(2), ("b", "true")], &value);
        self.assert_metric(name, None, &[future_name(3), ("s", "a")], &value);
        self.assert_metric(name, None, &[future_name(4), ("s", "42")], &value);

        let labels = &[
            future_name(5),
            ("e", "a"),
            ("s1", "a"),
            ("s2", "42"),
            ("b", "true"),
        ];
        self.assert_metric(name, None, labels, &value);

        self.assert_metric(name, None, &[future_name(6), ("st", "1")], &value);

        let labels = &[future_name(7), ("st1", "1"), ("st2", "2")];
        self.assert_metric(name, None, labels, &value);

        let labels = &[future_name(8), ("s", "42"), ("st", "2")];
        self.assert_metric(name, None, labels, &value);

        let labels = &[
            future_name(9),
            ("e", "a"),
            ("s1", "a"),
            ("s2", "42"),
            ("b", "true"),
            ("oe", "a"),
            ("os1", "a"),
            ("os2", "42"),
            ("ob", "true"),
            ("st1", "1"),
            ("st2", "2"),
        ];
        self.assert_metric(name, None, labels, &value);
    }
}

fn measure_heap_allocation() -> usize {
    alloc::epoch::advance().unwrap();
    alloc::stats::allocated::read().unwrap()
}

fn expected_histogram(count: f64) -> Value {
    Value::Histogram(vec![
        HistogramCount {
            less_than: 0.0,
            count: 0.0,
        },
        HistogramCount {
            less_than: f64::INFINITY,
            count,
        },
    ])
}
