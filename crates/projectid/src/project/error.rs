use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum AccessError {
    #[error("invalid key")]
    KeyInvalid,

    #[error("origin not allowed")]
    OriginNotAllowed,

    #[error("project is inactive")]
    ProjectInactive,
}
