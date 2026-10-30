use std::sync::{Mutex, RwLock};

use asteroid_core::{Index, Vault};

pub struct OpenVault {
    pub vault: Vault,
    pub index: Index,
}

#[derive(Default)]
pub struct AppState {
    pub open: RwLock<Option<OpenVault>>,
    pub watcher: Mutex<Option<crate::watcher::SharedWatcher>>,
    /// Serialises opening and closing vaults; both now run off the UI thread
    /// and could otherwise interleave.
    pub switching: Mutex<()>,
    /// Held for the duration of a sync so only one runs at a time.
    pub sync_lock: tokio::sync::Mutex<()>,
    /// Bumped to cancel a GitHub sign-in that's waiting for approval.
    pub github_flow: std::sync::atomic::AtomicU64,
}

pub type CmdResult<T> = Result<T, String>;

pub fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

impl AppState {
    /// A cheap clone of the open vault handle, so no lock is held across
    /// slow work or `.await` points.
    pub fn vault(&self) -> CmdResult<Vault> {
        self.open
            .read()
            .map_err(err)?
            .as_ref()
            .map(|o| o.vault.clone())
            .ok_or_else(|| "No vault is open".to_string())
    }

    pub fn with<T>(&self, f: impl FnOnce(&OpenVault) -> T) -> CmdResult<T> {
        let guard = self.open.read().map_err(err)?;
        let open = guard.as_ref().ok_or("No vault is open")?;
        Ok(f(open))
    }

    pub fn with_mut<T>(&self, f: impl FnOnce(&mut OpenVault) -> T) -> CmdResult<T> {
        let mut guard = self.open.write().map_err(err)?;
        let open = guard.as_mut().ok_or("No vault is open")?;
        Ok(f(open))
    }
}
