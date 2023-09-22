use {
    hyper::{HeaderMap, StatusCode},
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum GeoBlockError {
    #[error("Country is blocked: {country}")]
    BlockedCountry { country: String },

    #[error("Unable to extract IP address")]
    UnableToExtractIPAddress,

    #[error("Unable to extract geo data from IP address")]
    UnableToExtractGeoData,

    #[error("Country could not be found in database")]
    CountryNotFound,

    #[error("Other Error")]
    Other {
        code: StatusCode,
        msg: Option<String>,
        headers: Option<HeaderMap>,
    },
}
