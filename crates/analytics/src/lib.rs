use {
    async_trait::async_trait,
    std::{
        sync::Arc,
        time::{Duration, Instant},
    },
    tap::Tap,
};
pub use {
    collectors::{BatchCollector, CollectionError, CollectorConfig},
    exporters::{AwsConfig, AwsError, AwsExporter, NoopExporter},
    serializers::{NoopBatchFactory, ParquetBatchFactory, ParquetConfig, ParquetError},
};

mod collectors;
mod exporters;
mod serializers;
pub mod time;

pub trait AnalyticsEvent: Send + Sync + 'static {}
impl<T> AnalyticsEvent for T where T: Send + Sync + 'static {}

#[async_trait]
pub trait Exporter: Clone + Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    async fn export(self, data: Vec<u8>) -> Result<(), Self::Error>;
}

pub trait ExportObserver<E>: Send + Sync + 'static {
    fn observe_export(&self, _elapsed: Duration, _res: &Result<(), E>) {}
}

pub trait BatchFactory<T>: Send + Sync + 'static {
    type Batch: Batch<T>;
    type Error: std::error::Error + Send + Sync + 'static;

    fn create(&self) -> Result<Self::Batch, Self::Error>;
}

pub trait Batch<T>: Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    fn push(&mut self, data: T) -> Result<(), Self::Error>;

    fn is_full(&self) -> bool;

    fn is_empty(&self) -> bool;

    fn serialize(self) -> Result<Vec<u8>, Self::Error>;
}

pub trait BatchObserver<T, E>: Send + Sync + 'static {
    fn observe_batch_push(&self, _res: &Result<(), E>) {}

    fn observe_batch_serialization(&self, _elapsed: Duration, _res: &Result<Vec<u8>, E>) {}
}

pub trait Collector<T>: Send + Sync + 'static {
    type Error: std::error::Error + Send + Sync + 'static;

    fn collect(&self, data: T) -> Result<(), Self::Error>;
}

pub trait CollectionObserver<T, E>: Send + Sync + 'static {
    fn observe_collection(&self, _res: &Result<(), E>) {}
}

#[derive(Clone)]
pub struct Observable<I, O> {
    inner: I,
    observer: O,
}

impl<T, I, O> BatchFactory<T> for Observable<I, O>
where
    I: BatchFactory<T>,
    O: BatchObserver<T, I::Error> + BatchObserver<T, <I::Batch as Batch<T>>::Error> + Clone,
{
    type Batch = Observable<I::Batch, O>;
    type Error = I::Error;

    fn create(&self) -> Result<Self::Batch, Self::Error> {
        Ok(Observable {
            inner: self.inner.create()?,
            observer: self.observer.clone(),
        })
    }
}

impl<T, I, O> Batch<T> for Observable<I, O>
where
    I: Batch<T>,
    O: BatchObserver<T, I::Error>,
{
    type Error = I::Error;

    fn push(&mut self, data: T) -> Result<(), Self::Error> {
        self.inner
            .push(data)
            .tap(|res| self.observer.observe_batch_push(res))
    }

    fn is_full(&self) -> bool {
        self.inner.is_full()
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    fn serialize(self) -> Result<Vec<u8>, Self::Error> {
        let time = Instant::now();

        self.inner.serialize().tap(|res| {
            self.observer
                .observe_batch_serialization(time.elapsed(), res)
        })
    }
}

impl<T, I, O> Collector<T> for Observable<I, O>
where
    I: Collector<T>,
    O: CollectionObserver<T, I::Error>,
{
    type Error = I::Error;

    fn collect(&self, data: T) -> Result<(), Self::Error> {
        self.inner
            .collect(data)
            .tap(|res| self.observer.observe_collection(res))
    }
}

#[async_trait]
impl<I, O> Exporter for Observable<I, O>
where
    I: Exporter,
    O: ExportObserver<I::Error> + Clone,
{
    type Error = I::Error;

    async fn export(self, data: Vec<u8>) -> Result<(), Self::Error> {
        let time = Instant::now();

        self.inner
            .export(data)
            .await
            .tap(|res| self.observer.observe_export(time.elapsed(), res))
    }
}

pub type BoxCollector<T> = Box<dyn Collector<T, Error = CollectionError>>;
pub type ArcCollector<T> = Arc<dyn Collector<T, Error = CollectionError>>;

pub trait AnalyticsExt {
    fn with_observer<O>(self, observer: O) -> Observable<Self, O>
    where
        Self: Sized,
    {
        Observable {
            inner: self,
            observer,
        }
    }

    fn boxed<T>(self) -> Box<dyn Collector<T, Error = Self::Error>>
    where
        Self: Collector<T> + Sized,
    {
        Box::new(self)
    }

    fn boxed_shared<T>(self) -> Arc<dyn Collector<T, Error = Self::Error>>
    where
        Self: Collector<T> + Sized,
    {
        Arc::new(self)
    }
}

impl<T> AnalyticsExt for T {}

impl<T, E> Collector<T> for Arc<dyn Collector<T, Error = E>>
where
    T: AnalyticsEvent,
    E: std::error::Error + Send + Sync + 'static,
{
    type Error = E;

    fn collect(&self, data: T) -> Result<(), Self::Error> {
        self.as_ref().collect(data)
    }
}

impl<T, E> Collector<T> for Box<dyn Collector<T, Error = E>>
where
    T: AnalyticsEvent,
    E: std::error::Error + Send + Sync + 'static,
{
    type Error = E;

    fn collect(&self, data: T) -> Result<(), Self::Error> {
        self.as_ref().collect(data)
    }
}

/// Creates a [`Collector`] that doesn't store or export any data.
pub fn noop_collector<T>() -> BatchCollector<T>
where
    T: AnalyticsEvent,
{
    BatchCollector::new(Default::default(), NoopBatchFactory, NoopExporter)
}
