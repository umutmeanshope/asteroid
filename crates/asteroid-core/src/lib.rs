//! Platform-independent core of Asteroid: vault file operations, markdown
//! parsing, the note index (search, backlinks, tags) and HTML rendering.
//!
//! Nothing here depends on a UI toolkit, so the same crate backs the desktop
//! app and the future iOS/Android shells.

pub mod error;
pub mod index;
pub mod parse;
pub mod relink;
pub mod render;
pub mod vault;

pub use error::{Error, Result};
pub use index::{Backlink, Index, NoteSummary, SearchHit, TagCount};
pub use parse::{Heading, Link};
pub use vault::{Entry, EntryKind, FileInfo, Vault};
