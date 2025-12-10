use {
    crate::{Batch, BatchFactory},
    std::convert::Infallible,
};

#[cfg(feature = "parquet")]
pub mod parquet;

pub struct NoopBatchFactory;

impl<T> BatchFactory<T> for NoopBatchFactory {
    type Batch = NoopBatch;
    type Error = Infallible;

    fn create(&self) -> Result<Self::Batch, Self::Error> {
        Ok(NoopBatch)
    }
}

pub struct NoopBatch;

impl<T> Batch<T> for NoopBatch {
    type Error = Infallible;

    fn push(&mut self, _: T) -> Result<(), Self::Error> {
        Ok(())
    }

    fn is_full(&self) -> bool {
        false
    }

    fn is_empty(&self) -> bool {
        true
    }

    fn serialize(self) -> Result<Vec<u8>, Self::Error> {
        Ok(Vec::new())
    }
}
