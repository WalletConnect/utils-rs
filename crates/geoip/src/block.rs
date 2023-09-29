use {crate::Resolver, bitflags::bitflags, std::net::IpAddr};

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
pub struct CountryFilter {
    blocked_countries: Vec<String>,
    blocking_policy: BlockingPolicy,
}

impl CountryFilter {
    pub fn new(blocked_countries: Vec<String>, blocking_policy: BlockingPolicy) -> Self {
        Self {
            blocked_countries,
            blocking_policy,
        }
    }

    /// Checks whether the IP address is blocked. Returns an error if it's
    /// blocked or if the lookup has failed for any reason.
    pub fn check<R>(&self, addr: IpAddr, resolver: &R) -> Result<(), Error>
    where
        R: Resolver,
    {
        let country = resolver
            .lookup_geo_data_raw(addr)
            .map_err(|_| Error::UnableToExtractGeoData)?
            .country
            .and_then(|country| country.iso_code)
            .ok_or(Error::CountryNotFound)?;

        let blocked = self
            .blocked_countries
            .iter()
            .any(|blocked_country| blocked_country == country);

        if blocked {
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
                || matches!(err, Error::CountryNotFound if !policy.contains(BlockingPolicy::AllowMissingCountry))
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
