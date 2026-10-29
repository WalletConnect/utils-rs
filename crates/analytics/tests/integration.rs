use {
    analytics::{
        AnalyticsExt,
        BatchCollector,
        BatchFactory,
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
    parquet::{
        file::reader::{FileReader, SerializedFileReader},
        record::RowAccessor,
    },
    parquet_derive::ParquetRecordWriter,
    std::{
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::sync::{mpsc, mpsc::error::TrySendError, oneshot},
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

type PendingExport = (Vec<u8>, oneshot::Sender<Result<(), std::io::Error>>);

#[derive(Clone)]
struct ControlledExporter(mpsc::Sender<PendingExport>);

#[async_trait]
impl Exporter for ControlledExporter {
    type Error = std::io::Error;

    async fn export(self, data: Vec<u8>) -> Result<(), Self::Error> {
        let (tx, rx) = oneshot::channel();
        self.0.send((data, tx)).await.unwrap();
        rx.await.unwrap()
    }
}

fn record_ids(data: Vec<u8>) -> Vec<u32> {
    SerializedFileReader::new(bytes::Bytes::from(data))
        .unwrap()
        .get_row_iter(None)
        .unwrap()
        .map(|row| row.unwrap().get_uint(0).unwrap())
        .collect()
}

#[tokio::test]
async fn shutdown_drains_records_and_waits_for_exports() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let (tx, mut rx) = mpsc::channel(8);
        let (collector, shutdown) = BatchCollector::new_with_shutdown(
            CollectorConfig {
                data_queue_capacity: 3,
                ..Default::default()
            },
            ParquetBatchFactory::new(ParquetConfig {
                batch_capacity: 2,
                alloc_buffer_size: 8192,
            }),
            ControlledExporter(tx),
        );

        for a in 0..2 {
            collector
                .collect(DataA {
                    a,
                    b: "foo",
                    c: true,
                })
                .unwrap();
        }
        let (data, first_export) = rx.recv().await.unwrap();
        let mut ids = record_ids(data);
        for a in 2..5 {
            collector
                .collect(DataA {
                    a,
                    b: "foo",
                    c: true,
                })
                .unwrap();
        }

        let mut shutdown = tokio::spawn(shutdown.shutdown());
        while ids.len() < 5 {
            let (data, complete) = rx.recv().await.unwrap();
            ids.extend(record_ids(data));
            complete.send(Ok(())).unwrap();
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut shutdown)
                .await
                .is_err()
        );
        first_export.send(Ok(())).unwrap();
        shutdown.await.unwrap().unwrap();

        ids.sort_unstable();
        assert_eq!(ids, [0, 1, 2, 3, 4]);
        assert!(matches!(
            collector.collect(DataA {
                a: 5,
                b: "foo",
                c: true
            }),
            Err(analytics::CollectionError::DataChannelClosed)
        ));
    })
    .await
    .unwrap();
}

struct FailingBatchFactory {
    inner: ParquetBatchFactory,
    creates: AtomicUsize,
    fail_after: usize,
}

impl BatchFactory<DataA> for FailingBatchFactory {
    type Batch = <ParquetBatchFactory as BatchFactory<DataA>>::Batch;
    type Error = std::io::Error;

    fn create(&self) -> Result<Self::Batch, Self::Error> {
        if self.creates.fetch_add(1, Ordering::Relaxed) == self.fail_after {
            return Err(std::io::Error::other("batch failed"));
        }
        self.inner.create().map_err(std::io::Error::other)
    }
}

#[tokio::test]
async fn shutdown_waits_for_exports_after_an_error() {
    tokio::time::timeout(Duration::from_secs(5), async {
        for fail_after in [2, usize::MAX] {
            let (tx, mut rx) = mpsc::channel(8);
            let (collector, shutdown) = BatchCollector::new_with_shutdown(
                CollectorConfig::default(),
                FailingBatchFactory {
                    inner: ParquetBatchFactory::new(ParquetConfig {
                        batch_capacity: 1,
                        alloc_buffer_size: 8192,
                    }),
                    creates: AtomicUsize::new(0),
                    fail_after,
                },
                ControlledExporter(tx),
            );
            collector
                .collect(DataA {
                    a: 0,
                    b: "foo",
                    c: true,
                })
                .unwrap();
            let (_, first_export) = rx.recv().await.unwrap();
            collector
                .collect(DataA {
                    a: 1,
                    b: "foo",
                    c: true,
                })
                .unwrap();
            let blocked_export = if fail_after == 2 {
                first_export
            } else {
                let (_, second_export) = rx.recv().await.unwrap();
                first_export
                    .send(Err(std::io::Error::other("upload failed")))
                    .unwrap();
                second_export
            };

            let mut shutdown = tokio::spawn(shutdown.shutdown());
            assert!(
                tokio::time::timeout(Duration::from_millis(20), &mut shutdown)
                    .await
                    .is_err()
            );
            blocked_export.send(Ok(())).unwrap();
            shutdown.await.unwrap().unwrap();
        }
    })
    .await
    .unwrap();
}
