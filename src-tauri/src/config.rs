//! Small JSON files: app settings, recent vaults and the Google and GitHub
//! tokens live in the app config dir; each vault's sync link lives in its
//! `.asteroid` dir.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use asteroid_core::Vault;
use asteroid_core::vault::{META_DIR, join};
use tauri::{AppHandle, Manager};

use crate::state::{CmdResult, err};

pub fn config_dir(app: &AppHandle) -> CmdResult<PathBuf> {
    let dir = app.path().app_config_dir().map_err(err)?;
    fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir)
}

pub fn read_json<T: DeserializeOwned>(app: &AppHandle, name: &str) -> Option<T> {
    let path = config_dir(app).ok()?.join(name);
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub fn write_json<T: Serialize>(app: &AppHandle, name: &str, value: &T) -> CmdResult<()> {
    let path = config_dir(app)?.join(name);
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(value).map_err(err)?).map_err(err)?;
    fs::rename(tmp, path).map_err(err)
}

pub fn remove(app: &AppHandle, name: &str) -> CmdResult<()> {
    let path = config_dir(app)?.join(name);
    match fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(err(e)),
        _ => Ok(()),
    }
}

pub const SETTINGS: &str = "settings.json";
pub const RECENT: &str = "recent-vaults.json";
pub const GOOGLE_TOKEN: &str = "google-token.json";
pub const GITHUB_TOKEN: &str = "github-token.json";

/// An OAuth client credential from Settings, falling back to an environment
/// variable at run time and then at build time.
pub fn credential(app: &AppHandle, key: &str, env: &str, built: Option<&'static str>) -> Option<String> {
    let settings: serde_json::Value = read_json(app, SETTINGS).unwrap_or_default();
    settings[key]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| std::env::var(env).ok().filter(|s| !s.is_empty()))
        .or_else(|| built.map(String::from))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentVault {
    pub path: String,
    pub name: String,
    pub opened_ms: i64,
}

/// Where a vault syncs to. Serialised with a `provider` tag, so links saved
/// before git sync existed (`{"provider":"gdrive",…}`) still load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider")]
pub enum SyncLink {
    #[serde(rename = "gdrive", rename_all = "camelCase")]
    Drive { folder_id: String, folder_name: String },
    #[serde(rename = "git", rename_all = "camelCase")]
    Git { remote_url: String, repo_name: String, branch: String },
}

const SYNC_LINK: &str = "sync.json";

impl SyncLink {
    pub fn load(vault: &Vault) -> Option<Self> {
        serde_json::from_slice(&fs::read(vault.meta_dir().join(SYNC_LINK)).ok()?).ok()
    }

    pub fn save(&self, vault: &Vault) -> CmdResult<()> {
        let json = serde_json::to_vec_pretty(self).map_err(err)?;
        vault.write_bytes(&join(META_DIR, SYNC_LINK), &json).map_err(err)
    }

    pub fn remove(vault: &Vault) -> CmdResult<()> {
        match fs::remove_file(vault.meta_dir().join(SYNC_LINK)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(err(e)),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_links_keep_their_saved_format() {
        let drive: SyncLink = serde_json::from_str(r#"{"provider":"gdrive","folderId":"abc","folderName":"Notes"}"#).unwrap();
        assert!(matches!(drive, SyncLink::Drive { ref folder_id, .. } if folder_id == "abc"));
        let git = SyncLink::Git { remote_url: "https://github.com/a/b.git".into(), repo_name: "a/b".into(), branch: "main".into() };
        assert_eq!(
            serde_json::to_value(&git).unwrap(),
            serde_json::json!({"provider":"git","remoteUrl":"https://github.com/a/b.git","repoName":"a/b","branch":"main"})
        );
    }
}
