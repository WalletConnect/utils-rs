use {
    analytics::{
        AnalyticsExt,
        BatchCollector,
        BatchObserver,
        CollectionObserver,
        Collector,
        CollectorConfig,
        ExportObserver,
        Exporter,
        ParquetBatchFactory,
        ParquetConfig,
    },
    async_trait::async_trait,
    parquet_derive::ParquetRecordWriter,
    std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::sync::{mpsc, mpsc::error::TrySendError},
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

#[derive(ParquetRecordWriter)]
struct DataA {
    a: u32,
    b: &'static str,
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
        ParquetBatchFactory::new(ParquetConfig {
            batch_capacity: 128,
            alloc_buffer_size: 8192,
        }),
        MockExporter(tx),
    );

    collector
        .collect(DataA {
            a: 1,
            b: "foo",
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
        ParquetBatchFactory::new(ParquetConfig {
            batch_capacity: 2,
            alloc_buffer_size: 8192,
        }),
        MockExporter(tx),
    );

    collector
        .collect(DataA {
            a: 1,
            b: "foo",
            c: true,
        })
        .unwrap();

    collector
        .collect(DataA {
            a: 2,
            b: "bar",
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
        ParquetBatchFactory::new(ParquetConfig {
            batch_capacity: 2,
            alloc_buffer_size: 8192,
        })
        .with_observer(observer.clone()),
        MockExporter(tx).with_observer(observer.clone()),
    )
    .with_observer(observer.clone());

    collector
        .collect(DataA {
            a: 1,
            b: "foo",
            c: true,
        })
        .unwrap();

    collector
        .collect(DataA {
            a: 2,
            b: "bar",
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
