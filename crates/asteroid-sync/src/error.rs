use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] asteroid_core::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Google Drive error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("sign-in failed: {0}")]
    Auth(String),
    #[error("{0}")]
    Unsafe(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
