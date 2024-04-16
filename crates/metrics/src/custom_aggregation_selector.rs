use opentelemetry_sdk::metrics::{
    reader::{AggregationSelector, DefaultAggregationSelector},
    Aggregation, InstrumentKind,
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
                max_size: 160,
                max_scale: 20,
                record_min_max: true,
            },
            x => self.default_aggregation_selector.aggregation(x),
        }
    }
}
