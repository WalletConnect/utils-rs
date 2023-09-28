use std::{net::IpAddr, sync::Arc};

pub mod local;
pub mod maxmind;

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

    /// Lookup the geo data for the given IP address.
    fn lookup_geo_data(&self, addr: IpAddr) -> Result<GeoData, Self::Error>;
}
