pub use parquet::errors::ParquetError;
use {
    crate::{AnalyticsEvent, Batch, BatchFactory},
    parquet::{
        basic::Compression,
        file::{properties::WriterProperties, writer::SerializedFileWriter},
        record::RecordWriter,
    },
    std::{convert::Infallible, sync::Arc},
};

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

#[derive(Debug, Clone)]
pub struct ParquetConfig {
    /// The maximum number of records the batch can hold. Pushing more records
    /// will trigger export.
    pub batch_capacity: usize,

    /// The data buffer initially allocated for serialization. Specifying a low
    /// value would cause memory reallocation potentially affecting performance.
    pub alloc_buffer_size: usize,
}

impl Default for ParquetConfig {
    fn default() -> Self {
        Self {
            batch_capacity: 1024 * 128,
            alloc_buffer_size: 1024 * 1024 * 130,
        }
    }
}

pub struct ParquetBatchFactory {
    config: ParquetConfig,
}

impl ParquetBatchFactory {
    pub fn new(config: ParquetConfig) -> Self {
        Self { config }
    }
}

impl<T> BatchFactory<T> for ParquetBatchFactory
where
    T: AnalyticsEvent,
    [T]: RecordWriter<T>,
{
    type Batch = ParquetBatch<T>;
    type Error = ParquetError;

    fn create(&self) -> Result<Self::Batch, Self::Error> {
        let props = WriterProperties::builder()
            .set_compression(Compression::GZIP(Default::default()))
            .build();
        let props = Arc::new(props);
        let schema = ([] as [T; 0]).schema()?;

        Ok(ParquetBatch {
            capacity: self.config.batch_capacity,
            data: Vec::with_capacity(self.config.batch_capacity),
            writer: SerializedFileWriter::new(
                Vec::with_capacity(self.config.alloc_buffer_size),
                schema,
                props,
            )?,
        })
    }
}

pub struct ParquetBatch<T> {
    capacity: usize,
    data: Vec<T>,
    writer: SerializedFileWriter<Vec<u8>>,
}

impl<T> Batch<T> for ParquetBatch<T>
where
    T: AnalyticsEvent,
    [T]: RecordWriter<T>,
{
    type Error = ParquetError;

    fn push(&mut self, data: T) -> Result<(), Self::Error> {
        self.data.push(data);
        Ok(())
    }

    fn is_full(&self) -> bool {
        self.data.len() >= self.capacity
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    fn serialize(mut self) -> Result<Vec<u8>, Self::Error> {
        let mut row_group_writer = self.writer.next_row_group()?;

        self.data
            .as_slice()
            .write_to_row_group(&mut row_group_writer)?;

        row_group_writer.close()?;

        self.writer.into_inner()
    }
}
