use thiserror::Error as ThisError;

#[derive(ThisError, Debug)]
pub enum RegistryError {
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),

    #[error("invalid config: {0}")]
    Config(&'static str),

    #[error("invalid response: {0}")]
    Response(String),
}
