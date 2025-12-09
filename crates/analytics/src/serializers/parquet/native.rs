pub use parquet::{self, errors::ParquetError as Error};
use {
    crate::AnalyticsEvent,
    parquet::{
        basic::Compression,
        file::{properties::WriterProperties, writer::SerializedFileWriter},
        record::RecordWriter,
        schema::types::TypePtr,
    },
    std::{marker::PhantomData, sync::Arc},
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            batch_capacity: 1024 * 128,
            alloc_buffer_size: 1024 * 1024 * 130,
            writer_properties: WriterProperties::builder()
                .set_compression(Compression::GZIP(Default::default()))
                .build(),
        }
    }
}

pub struct BatchFactory<T> {
    batch_capacity: usize,
    alloc_buffer_size: usize,
    writer_properties: Arc<WriterProperties>,
    schema: TypePtr,
    _marker: PhantomData<T>,
}

impl<T> BatchFactory<T>
where
    T: AnalyticsEvent,
    for<'a> &'a [T]: RecordWriter<T>,
{
    pub fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            batch_capacity: config.batch_capacity,
            alloc_buffer_size: config.alloc_buffer_size,
            writer_properties: Arc::new(config.writer_properties),
            schema: (&[] as &[T]).schema()?,
            _marker: PhantomData,
        })
    }
}

impl<T> crate::BatchFactory<T> for BatchFactory<T>
where
    T: AnalyticsEvent,
    for<'a> &'a [T]: RecordWriter<T>,
{
    type Batch = Batch<T>;
    type Error = Error;

    fn create(&self) -> Result<Self::Batch, Self::Error> {
        let writer = SerializedFileWriter::new(
            Vec::with_capacity(self.alloc_buffer_size),
            self.schema.clone(),
            self.writer_properties.clone(),
        )?;

        Ok(Batch {
            capacity: self.batch_capacity,
            data: Vec::with_capacity(self.batch_capacity),
            writer,
        })
    }
}

pub struct Batch<T> {
    capacity: usize,
    data: Vec<T>,
    writer: SerializedFileWriter<Vec<u8>>,
}

impl<T> crate::Batch<T> for Batch<T>
where
    T: AnalyticsEvent,
    for<'a> &'a [T]: RecordWriter<T>,
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

    fn serialize(mut self) -> Result<Vec<u8>, Self::Error> {
        let mut row_group_writer = self.writer.next_row_group()?;

        self.data
            .as_slice()
            .write_to_row_group(&mut row_group_writer)?;

        row_group_writer.close()?;

        self.writer.flush()?;
        self.writer.into_inner()
    }
}
