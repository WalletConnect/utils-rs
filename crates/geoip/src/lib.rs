pub use maxminddb;
use {
    aws_sdk_s3::{
        error::SdkError,
        operation::get_object::GetObjectError,
        primitives::ByteStreamError,
        Client as S3Client,
    },
    bytes::Bytes,
    maxminddb::geoip2::City,
    std::{net::IpAddr, ops::Deref, sync::Arc},
};

pub mod block;

#[derive(Debug, Clone)]
pub struct Data {
    pub continent: Option<Arc<str>>,
    pub country: Option<Arc<str>>,
    pub region: Option<Vec<String>>,
    pub city: Option<Arc<str>>,
}

pub trait Resolver: Clone {
    /// The error type produced by the resolver.
    type Error;

    /// Lookup the raw geo data for the given IP address.
    fn lookup_geo_data_raw(&self, addr: IpAddr) -> Result<City<'_>, Self::Error>;

    /// Lookup the geo data for the given IP address.
    fn lookup_geo_data(&self, addr: IpAddr) -> Result<Data, Self::Error>;
}

impl<T> Resolver for &T
where
    T: Resolver,
{
    type Error = T::Error;

    fn lookup_geo_data_raw(&self, addr: IpAddr) -> Result<City<'_>, Self::Error> {
        let r = <&T>::deref(self);
        r.lookup_geo_data_raw(addr)
    }

    fn lookup_geo_data(&self, addr: IpAddr) -> Result<Data, Self::Error> {
        let r = <&T>::deref(self);
        r.lookup_geo_data(addr)
    }
}

impl<T> Resolver for Arc<T>
where
    T: Resolver,
{
    type Error = T::Error;

    fn lookup_geo_data_raw(&self, addr: IpAddr) -> Result<City<'_>, Self::Error> {
        let r = self.deref();
        r.lookup_geo_data_raw(addr)
    }

    fn lookup_geo_data(&self, addr: IpAddr) -> Result<Data, Self::Error> {
        let r = self.deref();
        r.lookup_geo_data(addr)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LocalResolverError {
    #[error("Geoip data lookup is not supported")]
    NotSupported,
}

/// Local resolver that does not need DB files.
#[derive(Debug, Clone)]
pub struct LocalResolver {
    resolver_raw: Option<fn(IpAddr) -> City<'static>>,
    resolver: Option<fn(IpAddr) -> Data>,
}

impl LocalResolver {
    pub fn new(
        resolver_raw: Option<fn(IpAddr) -> City<'static>>,
        resolver: Option<fn(IpAddr) -> Data>,
    ) -> Self {
        Self {
            resolver_raw,
            resolver,
        }
    }
}

impl Resolver for LocalResolver {
    type Error = LocalResolverError;

    fn lookup_geo_data_raw(&self, addr: IpAddr) -> Result<City<'_>, Self::Error> {
        self.resolver_raw
            .ok_or(LocalResolverError::NotSupported)
            .map(|resolver| resolver(addr))
    }

    fn lookup_geo_data(&self, addr: IpAddr) -> Result<Data, Self::Error> {
        self.resolver
            .ok_or(LocalResolverError::NotSupported)
            .map(|resolver| resolver(addr))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MaxMindResolverError {
    #[error("S3 get object failed: {0}")]
    GetObject(Box<SdkError<GetObjectError>>),

    #[error("Byte stream error: {0}")]
    ByteStream(Box<ByteStreamError>),

    #[error("MaxMind DB lookup error: {0}")]
    MaxMindDB(#[from] maxminddb::MaxMindDBError),
}

impl From<SdkError<GetObjectError>> for MaxMindResolverError {
    fn from(e: SdkError<GetObjectError>) -> Self {
        MaxMindResolverError::GetObject(Box::new(e))
    }
}

impl From<ByteStreamError> for MaxMindResolverError {
    fn from(e: ByteStreamError) -> Self {
        MaxMindResolverError::ByteStream(Box::new(e))
    }
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

impl Resolver for MaxMindResolver {
    type Error = MaxMindResolverError;

    fn lookup_geo_data_raw(&self, addr: IpAddr) -> Result<City<'_>, Self::Error> {
        self.reader.lookup::<City>(addr).map_err(Into::into)
    }

    fn lookup_geo_data(&self, addr: IpAddr) -> Result<Data, Self::Error> {
        let lookup_data = self.lookup_geo_data_raw(addr)?;

        Ok(Data {
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
