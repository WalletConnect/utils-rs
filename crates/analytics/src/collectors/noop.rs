use crate::{AnalyticsCollector, AnalyticsEvent};

pub struct NoopCollector;

impl<T> AnalyticsCollector<T> for NoopCollector
where
    T: AnalyticsEvent,
{
    fn collect(&self, _: T) {}
}
