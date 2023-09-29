use std::{net::IpAddr, sync::Arc};

pub use {
    crate::maxminddb::geoip2::City,
    local::LocalResolver,
    maxmind::{MaxMindResolver, MaxMindResolverError},
};

mod local;
mod maxmind;

#[derive(Debug, Clone)]
pub struct GeoData {
    pub continent: Option<Arc<str>>,
    pub country: Option<Arc<str>>,
    pub region: Option<Vec<String>>,
    pub city: Option<Arc<str>>,
}

pub trait GeoIpResolver: Clone {
    /// The error type produced by the resolver.
    type Error;

    /// Lookup the raw geo data for the given IP address.
    fn lookup_geo_data_raw(&self, addr: IpAddr) -> Result<City<'_>, Self::Error>;

    /// Lookup the geo data for the given IP address.
    fn lookup_geo_data(&self, addr: IpAddr) -> Result<GeoData, Self::Error>;
}
