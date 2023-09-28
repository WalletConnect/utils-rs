use {
    crate::{
        collectors::{batch::BatchOpts, BatchExporter},
        writers::parquet::ParquetWriter,
        Analytics,
    },
    async_trait::async_trait,
    parquet_derive::ParquetRecordWriter,
    std::time::Duration,
    tokio::sync::{mpsc, mpsc::error::TrySendError},
};

#[derive(Clone)]
struct MockExporter(mpsc::Sender<Vec<u8>>);

#[async_trait]
impl BatchExporter for MockExporter {
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

    let opts = BatchOpts {
        batch_alloc_size: 8192,
        export_time_threshold: Duration::from_millis(100),
        export_size_threshold: 8192,
        export_check_interval: Duration::from_millis(200),
        ..Default::default()
    };

    let collector = ParquetWriter::new(opts, MockExporter(tx)).unwrap();
    let analytics = Analytics::new(collector);

    analytics.collect(DataA {
        a: 1,
        b: "foo",
        c: true,
    });

    // Expect to receive result after ~200ms, due to sheet expiration.
    tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .unwrap()
        .unwrap();

    // Expect to receive timeout, since we're not writing anything.
    {
        let res = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;

        assert!(res.is_err());
    }
}

#[tokio::test]
async fn export_by_num_rows() {
    let (tx, mut rx) = mpsc::channel(32);

    let opts = BatchOpts {
        batch_alloc_size: 8192,
        export_row_threshold: 2,
        ..Default::default()
    };

    let collector = ParquetWriter::new(opts, MockExporter(tx)).unwrap();
    let analytics = Analytics::new(collector);

    analytics.collect(DataA {
        a: 1,
        b: "foo",
        c: true,
    });

    analytics.collect(DataA {
        a: 2,
        b: "bar",
        c: false,
    });

    // Expect to receive result instantly due to row number threshold triggering
    // export.
    tokio::time::timeout(Duration::from_millis(500), rx.recv())
        .await
        .unwrap()
        .unwrap();
}
