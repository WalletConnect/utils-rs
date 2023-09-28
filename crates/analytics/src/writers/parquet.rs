use {
    crate::{
        collectors::{
            batch::{BatchBuffer, BatchCollector, BatchError, BatchOpts},
            BatchExporter,
            BatchWriter,
        },
        AnalyticsEvent,
    },
    parquet::{
        basic::Compression,
        errors::ParquetError,
        file::{properties::WriterProperties, writer::SerializedFileWriter},
        record::RecordWriter,
    },
    std::sync::Arc,
};

/// Re-export for use outside of this module.
pub type ParquetWriterError = ParquetError;

pub struct ParquetWriter<T> {
    data: Vec<T>,
    writer: SerializedFileWriter<BatchBuffer>,
}

impl<T> ParquetWriter<T> {
    pub fn new<E>(
        opts: BatchOpts,
        exporter: E,
    ) -> Result<BatchCollector<T>, BatchError<<Self as BatchWriter<T>>::Error>>
    where
        T: AnalyticsEvent,
        [T]: RecordWriter<T>,
        E: BatchExporter,
    {
        BatchCollector::new::<Self, _>(opts, exporter)
    }
}

impl<T> BatchWriter<T> for ParquetWriter<T>
where
    T: AnalyticsEvent,
    [T]: RecordWriter<T>,
{
    type Error = ParquetWriterError;

    fn create(buffer: BatchBuffer, opts: &BatchOpts) -> Result<Self, Self::Error> {
        let props = WriterProperties::builder()
            .set_compression(Compression::GZIP(Default::default()))
            .build();
        let props = Arc::new(props);
        let schema = ([] as [T; 0]).schema()?;

        Ok(Self {
            data: Vec::with_capacity(opts.export_row_threshold),
            writer: SerializedFileWriter::new(buffer, schema, props)?,
        })
    }

    fn write(&mut self, data: T) -> Result<(), Self::Error> {
        self.data.push(data);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn into_buffer(self) -> Result<Vec<u8>, Self::Error> {
        let mut writer = self.writer;
        let mut row_group_writer = writer.next_row_group()?;

        self.data
            .as_slice()
            .write_to_row_group(&mut row_group_writer)?;

        row_group_writer.close()?;

        writer.into_inner().map(BatchBuffer::into_inner)
    }
}
