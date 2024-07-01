use {
    crate::{Attrs, Counter, DynamicLabels},
    metrics::Counter,
    parking_lot::Mutex,
    std::collections::HashSet,
};

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    metrics: Vec::new(),
    dyn_labels: None,
});

pub(super) struct Registry {
    metrics: Vec<Entry>,
    dyn_labels: Option<HashSet<&'static str>>,
}

impl Registry {
    pub(super) fn register_dyn_label(&mut self, label: &str) -> &'static str {
        let dyn_labels = if let Some(labels) = self.dyn_labels.as_mut() {
            labels
        } else {
            self.dyn_labels.insert(HashSet::new())
        };

        if let Some(label) = dyn_labels.get(label) {
            return label;
        }

        // By holding the lock we make sure that only unique
        // values are being leaked
        let label = label.to_string().leak();

        dyn_labels.insert(label);
        label
    }
}

pub struct Entry {
    metric_name: &'static str,
    metric_description: Option<&'static str>,
    metric_labels: DynamicLabels,
    metric: Metric,
}

impl Entry {
    fn new(metric: Metric, attrs: &Attrs) -> Self {
        Self {
            metric_name: attrs.static_.name,
            metric_description: attrs.static_.description,
            metric_labels: attrs.labels(),
            metric,
        }
    }
}

pub enum Metric {
    Counter(&'static Counter),
}

pub(super) fn with_lock<T>(f: impl FnOnce(&mut Registry) -> T) -> T {
    f(&mut REGISTRY.lock())
}
