use {crate::Resolver, bitflags::bitflags, std::net::IpAddr};

#[cfg(feature = "middleware")]
pub mod middleware;

bitflags! {
    /// Values used to configure the response behavior when geo data could not be retrieved.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BlockingPolicy: u8 {
        const Block                 = 0b00;
        const AllowMissingGeoData   = 0b01;
        const AllowExtractFailure   = 0b10;
        const AllowAll              = 0b11;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
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
struct Zone {
    country: String,
    subdivisions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ZoneFilter {
    blocked_zones: Vec<Zone>,
    blocking_policy: BlockingPolicy,
}

impl ZoneFilter {
    pub fn new(blocked_zones: Vec<String>, blocking_policy: BlockingPolicy) -> Self {
        let blocked_zones = blocked_zones
            .iter()
            .filter_map(|zone| {
                zone.split(':')
                    .collect::<Vec<_>>()
                    .split_first()
                    .map(|(country, subdivisions)| Zone {
                        country: country.to_string(),
                        subdivisions: subdivisions.iter().map(|&s| s.to_string()).collect(),
                    })
            })
            .collect::<Vec<_>>();

        Self {
            blocked_zones,
            blocking_policy,
        }
    }

    /// Checks whether the IP address is blocked. Returns an error if it's
    /// blocked or if the lookup has failed for any reason.
    pub fn check<R>(&self, addr: IpAddr, resolver: &R) -> Result<(), Error>
    where
        R: Resolver,
    {
        let geo_data = resolver
            .lookup_geo_data_raw(addr)
            .map_err(|_| Error::UnableToExtractGeoData)?;

        let country = geo_data
            .country
            .and_then(|country| country.iso_code)
            .ok_or(Error::CountryNotFound)?;

        let zone_blocked = self.blocked_zones.iter().any(|blocked_zone| {
            if blocked_zone.country == country {
                if blocked_zone.subdivisions.is_empty() {
                    true
                } else {
                    geo_data
                        .subdivisions
                        .as_deref()
                        .map_or(false, |subdivisions| {
                            subdivisions
                                .iter()
                                .filter_map(|sub| sub.iso_code)
                                .any(|sub| {
                                    blocked_zone
                                        .subdivisions
                                        .iter()
                                        .any(|blocked_sub| sub.eq_ignore_ascii_case(blocked_sub))
                                })
                        })
                }
            } else {
                false
            }
        });

        if zone_blocked {
            Err(Error::Blocked)
        } else {
            Ok(())
        }
    }

    /// Applies selected blocking policy to the [`Blacklist::check()`] result,
    /// which may ignore some of the errors.
    pub fn apply_policy(&self, check_result: Result<(), Error>) -> Result<(), Error> {
        if let Err(err) = check_result {
            let policy = self.blocking_policy;

            let is_blocked = matches!(err, Error::UnableToExtractIPAddress | Error::UnableToExtractGeoData if !policy.contains(BlockingPolicy::AllowExtractFailure))
                || matches!(err, Error::CountryNotFound if !policy.contains(BlockingPolicy::AllowMissingGeoData))
                || matches!(err, Error::Blocked);

            if is_blocked {
                Err(err)
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }
}
