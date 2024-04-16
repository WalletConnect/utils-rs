use opentelemetry_sdk::metrics::{
    reader::{AggregationSelector, DefaultAggregationSelector},
    Aggregation,
    InstrumentKind,
};

#[derive(Clone, Default, Debug)]
pub struct CustomAggregationSelector {
    default_aggregation_selector: DefaultAggregationSelector,
}

impl CustomAggregationSelector {
    /// Create a new default aggregation selector.
    pub fn new() -> Self {
        Self::default()
    }
}

impl AggregationSelector for CustomAggregationSelector {
    fn aggregation(&self, kind: InstrumentKind) -> Aggregation {
        match kind {
            InstrumentKind::Histogram => Aggregation::Base2ExponentialHistogram {
                // default from https://opentelemetry.io/blog/2022/exponential-histograms/#why-use-exponential-bucket-histograms
                max_size: 160,
                // maxumum https://github.com/open-telemetry/opentelemetry-rust/blob/2286378632d498ce4b2da109e5aa131b34ae1a8f/opentelemetry-sdk/src/metrics/aggregation.rs#L75
                max_scale: 20,
                record_min_max: true,
            },
            x => self.default_aggregation_selector.aggregation(x),
        }
    }
}
