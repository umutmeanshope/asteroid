//! Vault synchronisation.
//!
//! [`engine`] holds the provider-independent three-way sync (local state vs.
//! remote state vs. the snapshot from the last successful sync). [`gdrive`]
//! implements the [`engine::Remote`] trait for Google Drive.

pub mod engine;
pub mod error;
pub mod gdrive;

pub use engine::{Remote, RemoteEntry, SyncOptions, SyncReport, SyncState, sync};
pub use error::{Error, Result};
