use {
    ::parquet::{
        file::reader::{FileReader as _, SerializedFileReader},
        record::RecordReader as _,
    },
    analytics::{
        parquet,
        AnalyticsExt,
        Batch,
        BatchCollector,
        BatchFactory,
        BatchObserver,
        CollectionObserver,
        Collector,
        CollectorConfig,
        ExportObserver,
        Exporter,
    },
    async_trait::async_trait,
    bytes::Bytes,
    parquet_derive::{ParquetRecordReader, ParquetRecordWriter},
    serde::{Deserialize, Serialize},
    serde_arrow::schema::TracingOptions,
    std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::sync::mpsc::{self, error::TrySendError},
};

#[derive(Clone)]
struct MockExporter(mpsc::Sender<Vec<u8>>);

#[async_trait]
impl Exporter for MockExporter {
    type Error = std::io::Error;

    async fn export(mut self, data: Vec<u8>) -> Result<(), Self::Error> {
        // Provide custom messages for clean log output.
        if let Err(TrySendError::Full(_)) = self.0.try_send(data) {
            panic!("send failed: channel is full");
        };

        Ok(())
    }
}

#[derive(
    Debug, Clone, ParquetRecordWriter, ParquetRecordReader, Serialize, Deserialize, PartialEq, Eq,
)]
struct Record {
    a: u32,
    b: String,
    c: bool,
}

#[tokio::test]
async fn export_by_timeout() {
    let (tx, mut rx) = mpsc::channel(32);

    let collector = BatchCollector::new(
        CollectorConfig {
            export_interval: Duration::from_millis(200),
            ..Default::default()
        },
        parquet::native::BatchFactory::new(parquet::native::Config {
            batch_capacity: 128,
            alloc_buffer_size: 8192,
            ..Default::default()
        })
        .unwrap(),
        MockExporter(tx),
    );

    collector
        .collect(Record {
            a: 1,
            b: "foo".into(),
            c: true,
        })
        .unwrap();

    // Expect to receive result after ~200ms, due to sheet expiration.
    tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .unwrap()
        .unwrap();

    // Expect to receive timeout, since we're not writing anything.
    let res = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn export_by_num_rows() {
    let (tx, mut rx) = mpsc::channel(32);

    let collector = BatchCollector::new(
        CollectorConfig {
            export_interval: Duration::from_millis(200),
            ..Default::default()
        },
        parquet::native::BatchFactory::new(parquet::native::Config {
            batch_capacity: 2,
            alloc_buffer_size: 8192,
            ..Default::default()
        })
        .unwrap(),
        MockExporter(tx),
    );

    collector
        .collect(Record {
            a: 1,
            b: "foo".into(),
            c: true,
        })
        .unwrap();

    collector
        .collect(Record {
            a: 2,
            b: "bar".into(),
            c: false,
        })
        .unwrap();

    // Expect to receive result instantly due to row number threshold triggering
    // export.
    tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .unwrap()
        .unwrap();
}

#[derive(Default, Clone)]
struct Observer {
    export: Arc<AtomicUsize>,
    batch_push: Arc<AtomicUsize>,
    batch_serialization: Arc<AtomicUsize>,
    collection: Arc<AtomicUsize>,
}

impl<E> ExportObserver<E> for Observer {
    fn observe_export(&self, _: Duration, _: &Result<(), E>) {
        self.export.fetch_add(1, Ordering::Relaxed);
    }
}

impl<T, E> BatchObserver<T, E> for Observer {
    fn observe_batch_push(&self, _: &Result<(), E>) {
        self.batch_push.fetch_add(1, Ordering::Relaxed);
    }

    fn observe_batch_serialization(&self, _: Duration, _: &Result<Vec<u8>, E>) {
        self.batch_serialization.fetch_add(1, Ordering::Relaxed);
    }
}

impl<T, E> CollectionObserver<T, E> for Observer {
    fn observe_collection(&self, _res: &Result<(), E>) {
        self.collection.fetch_add(1, Ordering::Relaxed);
    }
}

#[tokio::test]
async fn observability() {
    let (tx, mut rx) = mpsc::channel(32);

    let observer = Observer::default();

    let collector = BatchCollector::new(
        CollectorConfig {
            export_interval: Duration::from_millis(200),
            ..Default::default()
        },
        parquet::native::BatchFactory::new(parquet::native::Config {
            batch_capacity: 2,
            alloc_buffer_size: 8192,
            ..Default::default()
        })
        .unwrap()
        .with_observer(observer.clone()),
        MockExporter(tx).with_observer(observer.clone()),
    )
    .with_observer(observer.clone());

    collector
        .collect(Record {
            a: 1,
            b: "foo".into(),
            c: true,
        })
        .unwrap();

    collector
        .collect(Record {
            a: 2,
            b: "bar".into(),
            c: false,
        })
        .unwrap();

    // Expect to receive result instantly due to row number threshold triggering
    // export.
    tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(observer.export.load(Ordering::SeqCst), 1);
    assert_eq!(observer.batch_push.load(Ordering::SeqCst), 2);
    assert_eq!(observer.batch_serialization.load(Ordering::SeqCst), 1);
    assert_eq!(observer.collection.load(Ordering::SeqCst), 2);
}

#[test]
fn parquet_schema() {
    let schema_str = "
        message rust_schema {
            REQUIRED INT32 a (INTEGER(32, false));
            REQUIRED BINARY b (STRING);
            REQUIRED BOOLEAN c;
        }
    ";

    let from_str = parquet::schema_from_str(schema_str).unwrap();
    let from_native_writer = parquet::schema_from_native_writer::<Record>().unwrap();
    let from_serde_arrow = parquet::schema_from_serde_arrow::<Record>(
        "rust_schema",
        TracingOptions::new().strings_as_large_utf8(false),
    )
    .unwrap();

    assert_eq!(from_str, from_native_writer);
    assert_eq!(from_str, from_serde_arrow);
}

#[test]
fn parquet_native_serialization() {
    verify_parquet_serialization(parquet::native::BatchFactory::new(Default::default()).unwrap());
}

#[test]
fn parquet_serde_serialization() {
    verify_parquet_serialization(parquet::serde::BatchFactory::new(Default::default()).unwrap());
}

fn verify_parquet_serialization(factory: impl BatchFactory<Record>) {
    let expected_data = generate_records();
    let mut batch = factory.create().unwrap();

    for data in &expected_data {
        batch.push(data.clone()).unwrap();
    }

    let actual_data = read_records(batch.serialize().unwrap().into(), expected_data.len());
    assert_eq!(actual_data, expected_data);
}

fn generate_records() -> Vec<Record> {
    vec![
        Record {
            a: 1,
            b: "foo".into(),
            c: true,
        },
        Record {
            a: 2,
            b: "bar".into(),
            c: false,
        },
    ]
}

fn read_records(serialized: Bytes, num_records: usize) -> Vec<Record> {
    let mut samples = Vec::new();
    let reader = SerializedFileReader::new(serialized).unwrap();
    let mut row_group = reader.get_row_group(0).unwrap();
    samples
        .read_from_row_group(&mut *row_group, num_records)
        .unwrap();
    samples
}

// Ensure `parquet` used allows writing `Arc<str>` values.
#[derive(ParquetRecordWriter)]
struct _RecordWithArcStr {
    a: Arc<str>,
}
