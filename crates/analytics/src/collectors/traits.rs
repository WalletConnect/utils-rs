use {
    crate::{
        collectors::batch::{BatchBuffer, BatchOpts},
        AnalyticsEvent,
    },
    async_trait::async_trait,
    std::{
        error::Error as StdError,
        fmt::{Debug, Display},
    },
};

#[async_trait]
pub trait BatchExporter: 'static + Clone + Send {
    type Error: StdError + Send + Sync + Debug + Display + 'static;

    async fn export(self, data: Vec<u8>) -> Result<(), Self::Error>;
}

pub trait BatchWriter<T: AnalyticsEvent>: 'static + Send + Sync + Sized {
    type Error: StdError + Send + Sync + Debug + Display + 'static;

    fn create(buffer: BatchBuffer, opts: &BatchOpts) -> Result<Self, Self::Error>;

    fn write(&mut self, data: T) -> Result<(), Self::Error>;

    fn flush(&mut self) -> Result<(), Self::Error>;

    fn into_buffer(self) -> Result<Vec<u8>, Self::Error>;
}
