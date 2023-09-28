use {
    crate::collectors::BatchExporter,
    async_trait::async_trait,
    aws_sdk_s3::{primitives::ByteStream, Client},
    chrono::{Datelike, Utc},
    std::sync::Arc,
    thiserror::Error as ThisError,
    tracing::info,
};

#[derive(Debug, Clone)]
pub struct AwsOpts {
    pub export_prefix: &'static str,
    pub export_name: &'static str,
    pub file_extension: &'static str,
    pub bucket_name: Arc<str>,
    pub s3_client: Client,
    pub node_ip: Arc<str>,
}

#[derive(Debug, ThisError)]
pub enum AwsError {
    #[error("error uploading to s3: {0}")]
    UploadError(String),

    #[error("unknown error: {0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct AwsExporter {
    opts: AwsOpts,
}

impl AwsExporter {
    pub fn new(opts: AwsOpts) -> Self {
        Self { opts }
    }
}

#[async_trait]
impl BatchExporter for AwsExporter {
    type Error = AwsError;

    async fn export(self, data: Vec<u8>) -> Result<(), Self::Error> {
        let now = Utc::now();

        let export_prefix = self.opts.export_prefix;
        let export_name = self.opts.export_name;
        let file_extension = self.opts.file_extension;
        let node_ip = &self.opts.node_ip;
        let (year, month, day) = (now.year(), now.month(), now.day());
        let timestamp = now.timestamp_millis();

        let key = format!(
            "{export_prefix}/dt={year}-{month:0>2}-{day:0>2}/{export_name}_{timestamp}_{node_ip}.\
             {file_extension}"
        );

        info!(
            bucket = self.opts.bucket_name.as_ref(),
            key = key.as_str(),
            "uploading analytics to s3"
        );

        self.opts
            .s3_client
            .put_object()
            .bucket(self.opts.bucket_name.as_ref())
            .key(key)
            .body(ByteStream::from(data))
            .send()
            .await
            .map_err(|err| AwsError::UploadError(err.to_string()))?;

        info!("analytics successfully uploaded");

        Ok(())
    }
}
