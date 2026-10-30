//! Git-based vault sync and GitHub sign-in. Independent of the UI so the
//! mobile apps can share it.

pub mod github;
pub mod history;
pub mod ignore;
pub mod sync;

pub use sync::{Auth, Author, GitReport, GitTarget};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Git(#[from] git2::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Auth(String),
    #[error("{0}")]
    Api(String),
    #[error("{0}")]
    Setup(String),
    #[error("{0}")]
    Network(String),
    /// The remote moved on while pushing; the caller fetches and retries.
    #[error("The remote rejected the push: {0}")]
    Rejected(String),
    #[error("Cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;
