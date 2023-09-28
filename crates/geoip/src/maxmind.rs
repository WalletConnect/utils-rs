use {
    crate::{GeoData, GeoIpResolver},
    aws_sdk_s3::{
        error::SdkError,
        operation::get_object::GetObjectError,
        primitives::ByteStreamError,
        Client as S3Client,
    },
    bytes::Bytes,
    maxminddb::geoip2::City,
    std::{net::IpAddr, sync::Arc},
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum MaxMindResolverError {
    #[error("S3 get object failed: {0}")]
    GetObject(#[from] SdkError<GetObjectError>),

    #[error("Byte stream error: {0}")]
    ByteStream(#[from] ByteStreamError),

    #[error("MaxMind DB lookup error: {0}")]
    MaxMindDB(#[from] maxminddb::MaxMindDBError),
}

#[derive(Debug, Clone)]
pub struct MaxMindResolver {
    reader: Arc<maxminddb::Reader<Bytes>>,
}

impl MaxMindResolver {
    pub async fn from_aws_s3(
        s3_client: &S3Client,
        bucket: impl Into<String>,
        key: impl Into<String>,
    ) -> Result<Self, MaxMindResolverError> {
        let s3_object = s3_client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await?;
        let geo_data = s3_object.body.collect().await?.into_bytes();

        Self::from_buffer(geo_data)
    }

    pub fn from_buffer(buffer: Bytes) -> Result<Self, MaxMindResolverError> {
        let reader = maxminddb::Reader::from_source(buffer)?;
        Ok(Self {
            reader: Arc::new(reader),
        })
    }
}

impl GeoIpResolver for MaxMindResolver {
    type Error = MaxMindResolverError;

    fn lookup_geo_data(&self, addr: IpAddr) -> Result<GeoData, Self::Error> {
        let lookup_data = self.reader.lookup::<City>(addr)?;

        Ok(GeoData {
            continent: lookup_data
                .continent
                .and_then(|continent| continent.code.map(Into::into)),
            country: lookup_data
                .country
                .and_then(|country| country.iso_code.map(Into::into)),
            region: lookup_data.subdivisions.map(|divs| {
                divs.into_iter()
                    .filter_map(|div| div.iso_code)
                    .map(Into::into)
                    .collect()
            }),
            city: lookup_data
                .city
                .and_then(|city| city.names)
                .and_then(|city_names| city_names.get("en").copied().map(Into::into)),
        })
    }
}
