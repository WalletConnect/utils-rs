pub use parquet::{self, errors::ParquetError as Error};
use {
    crate::AnalyticsEvent,
    arrow::datatypes::FieldRef,
    parquet::{arrow::ArrowWriter, basic::Compression, file::properties::WriterProperties},
    serde::{de::DeserializeOwned, Serialize},
    serde_arrow::schema::{SchemaLike, TracingOptions},
    std::{io, marker::PhantomData, sync::Arc},
};

#[derive(Debug, Clone)]
pub struct Config {
    /// The maximum number of records the batch can hold. Pushing more records
    /// will trigger export.
    ///
    /// Default value: `1024 * 128`.
    pub batch_capacity: usize,

    /// The data buffer initially allocated for serialization. Specifying a low
    /// value would cause memory reallocation potentially affecting performance.
    ///
    /// Default value: `1024 * 1024 * 130`.
    pub alloc_buffer_size: usize,

    /// Native [`parquet`] [`WriterProperties`]. Configures parquet export
    /// parameters.
    ///
    /// Default value: `WriterProperties::default()` with enabled GZIP
    /// compression.
    pub writer_properties: WriterProperties,

    /// Native [`serde_arrow`] [`TracingOptions`]. Configures parquet schema
    /// generation based on the implemented `serde` traits.
    ///
    /// Default value: `TracingOptions::default()`.
    pub tracing_options: TracingOptions,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            batch_capacity: 1024 * 128,
            alloc_buffer_size: 1024 * 1024 * 130,
            writer_properties: WriterProperties::builder()
                .set_compression(Compression::GZIP(Default::default()))
                .build(),
            tracing_options: Default::default(),
        }
    }
}

pub struct BatchFactory<T> {
    batch_capacity: usize,
    alloc_buffer_size: usize,
    writer_properties: WriterProperties,
    fields: Arc<Vec<FieldRef>>,
    _marker: PhantomData<T>,
}

impl<T> BatchFactory<T>
where
    T: AnalyticsEvent + Serialize + DeserializeOwned,
{
    pub fn new(config: Config) -> Result<Self, Error> {
        let fields =
            Vec::<FieldRef>::from_type::<T>(config.tracing_options).map_err(io::Error::other)?;

        Ok(Self {
            batch_capacity: config.batch_capacity,
            alloc_buffer_size: config.alloc_buffer_size,
            writer_properties: config.writer_properties,
            fields: Arc::new(fields),
            _marker: PhantomData,
        })
    }
}

impl<T> crate::BatchFactory<T> for BatchFactory<T>
where
    T: AnalyticsEvent + Serialize + DeserializeOwned,
{
    type Batch = Batch<T>;
    type Error = Error;

    fn create(&self) -> Result<Self::Batch, Self::Error> {
        Ok(Batch {
            capacity: self.batch_capacity,
            data: Vec::with_capacity(self.batch_capacity),
            buffer: Vec::with_capacity(self.alloc_buffer_size),
            fields: self.fields.clone(),
            writer_properties: self.writer_properties.clone(),
        })
    }
}

pub struct Batch<T> {
    capacity: usize,
    data: Vec<T>,
    buffer: Vec<u8>,
    fields: Arc<Vec<FieldRef>>,
    writer_properties: WriterProperties,
}

impl<T> crate::Batch<T> for Batch<T>
where
    T: AnalyticsEvent + Serialize + DeserializeOwned,
{
    type Error = Error;

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

    fn serialize(self) -> Result<Vec<u8>, Self::Error> {
        let batch =
            serde_arrow::to_record_batch(&self.fields, &self.data).map_err(io::Error::other)?;

        let mut writer =
            ArrowWriter::try_new(self.buffer, batch.schema(), Some(self.writer_properties))?;

        writer.write(&batch)?;
        writer.into_inner()
    }
}
