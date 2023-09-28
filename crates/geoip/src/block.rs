use {crate::resolver::GeoIpResolver, bitflags::bitflags, hyper::StatusCode, std::net::IpAddr};

#[cfg(feature = "middleware")]
pub mod middleware;

bitflags! {
    /// Values used to configure the response behavior when geo data could not be retrieved.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BlockingPolicy: u8 {
        const Block                 = 0b00;
        const AllowMissingCountry   = 0b01;
        const AllowExtractFailure   = 0b10;
        const AllowAll              = 0b11;
    }
}

impl BlockingPolicy {
    pub fn resolve_http_response(
        &self,
        check_result: Result<(), GeoBlockError>,
    ) -> Result<(), StatusCode> {
        match check_result {
            Ok(_) => Ok(()),

            Err(err) => match err {
                GeoBlockError::Blocked => Err(StatusCode::UNAUTHORIZED),

                GeoBlockError::UnableToExtractIPAddress | GeoBlockError::UnableToExtractGeoData => {
                    tracing::info!("unable to extract client IP address");

                    if self.contains(BlockingPolicy::AllowExtractFailure) {
                        Ok(())
                    } else {
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }

                GeoBlockError::CountryNotFound => {
                    tracing::warn!("country not found");

                    if self.contains(BlockingPolicy::AllowMissingCountry) {
                        Ok(())
                    } else {
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            },
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GeoBlockError {
    #[error("Country is blocked")]
    Blocked,

    #[error("Unable to extract IP address")]
    UnableToExtractIPAddress,

    #[error("Unable to extract geo data from IP address")]
    UnableToExtractGeoData,

    #[error("Country could not be found in database")]
    CountryNotFound,
}

#[derive(Debug, Clone)]
pub struct Blacklist {
    blocked_countries: Vec<String>,
}

impl Blacklist {
    pub fn new(blocked_countries: Vec<String>) -> Self {
        Self { blocked_countries }
    }

    /// Checks whether the IP address is blocked. Returns an error if it's
    /// blocked or if the lookup has failed for any reason.
    pub fn check<R>(&self, addr: IpAddr, resolver: &R) -> Result<(), GeoBlockError>
    where
        R: GeoIpResolver,
    {
        let country = resolver
            .lookup_geo_data_raw(addr)
            .map_err(|_| GeoBlockError::UnableToExtractGeoData)?
            .country
            .and_then(|country| country.iso_code)
            .ok_or(GeoBlockError::CountryNotFound)?;

        let blocked = self
            .blocked_countries
            .iter()
            .any(|blocked_country| blocked_country == country);

        if blocked {
            Err(GeoBlockError::Blocked)
        } else {
            Ok(())
        }
    }
}
