use {
    super::{GeoData, GeoIpResolver},
    maxminddb::geoip2::City,
    std::net::IpAddr,
};

#[derive(Debug, thiserror::Error)]
pub enum LocalResolverError {
    #[error("Geoip data lookup is not supported")]
    NotSupported,
}

/// Local resolver that does not need DB files.
#[derive(Debug, Clone)]
pub struct LocalResolver {
    resolver_raw: Option<fn(IpAddr) -> City<'static>>,
    resolver: Option<fn(IpAddr) -> GeoData>,
}

impl LocalResolver {
    pub fn new(
        resolver_raw: Option<fn(IpAddr) -> City<'static>>,
        resolver: Option<fn(IpAddr) -> GeoData>,
    ) -> Self {
        Self {
            resolver_raw,
            resolver,
        }
    }
}

impl GeoIpResolver for LocalResolver {
    type Error = LocalResolverError;

    fn lookup_geo_data_raw(&self, addr: IpAddr) -> Result<City<'_>, Self::Error> {
        self.resolver_raw
            .ok_or(LocalResolverError::NotSupported)
            .map(|resolver| resolver(addr))
    }

    fn lookup_geo_data(&self, addr: IpAddr) -> Result<GeoData, Self::Error> {
        self.resolver
            .ok_or(LocalResolverError::NotSupported)
            .map(|resolver| resolver(addr))
    }
}
