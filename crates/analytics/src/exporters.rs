use {
    async_trait::async_trait,
    aws_sdk_s3::{operation::put_object::PutObjectError, primitives::ByteStream, Client},
    chrono::{Datelike, Utc},
    future::FutureExt,
    std::{convert::Infallible, net::IpAddr, time::Duration},
    thiserror::Error as ThisError,
};

#[derive(Clone)]
pub struct NoopExporter;

#[async_trait]
impl crate::Exporter for NoopExporter {
    type Error = Infallible;

    async fn export(self, _: Vec<u8>) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct AwsConfig {
    /// Exported data S3 key prefix (i.e. directory).
    pub export_prefix: String,

    /// Exported data S3 key base name.
    pub export_name: String,

    /// Node IP address added as a suffix to the S3 key.
    pub node_addr: IpAddr,

    /// Exported data S3 key file extension.
    pub file_extension: String,

    /// Exported data S3 bucket.
    pub bucket_name: String,

    /// AWS S3 client used for uploading the data.
    pub s3_client: Client,

    /// Maximum allowed S3 data upload time.
    pub upload_timeout: Duration,
}

#[derive(Debug, ThisError)]
pub enum AwsError {
    #[error("Error uploading to s3: {0}")]
    Upload(PutObjectError),

    #[error("Timeout uploading to s3")]
    Timeout,
}

#[derive(Clone)]
pub struct AwsExporter {
    config: AwsConfig,
}

impl AwsExporter {
    pub fn new(config: AwsConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl crate::Exporter for AwsExporter {
    type Error = AwsError;

    async fn export(self, data: Vec<u8>) -> Result<(), Self::Error> {
        let time = Utc::now();

        let export_prefix = self.config.export_prefix;
        let export_name = self.config.export_name;
        let file_extension = self.config.file_extension;
        let node_ip = &self.config.node_addr;
        let (year, month, day) = (time.year(), time.month(), time.day());
        let timestamp = time.timestamp_millis();

        let key = format!(
            "{export_prefix}/dt={year}-{month:0>2}-{day:0>2}/{export_name}_{timestamp}_{node_ip}.\
             {file_extension}"
        );
        let bucket = &self.config.bucket_name;

        tracing::info!(bucket, key, "uploading analytics to s3");

        self.config
            .s3_client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(ByteStream::from(data))
            .send()
            .with_timeout(self.config.upload_timeout)
            .await
            .map_err(|_| AwsError::Timeout)?
            .map_err(|err| AwsError::Upload(err.into_service_error()))?;

        tracing::info!("analytics successfully uploaded");

        Ok(())
    }
}
