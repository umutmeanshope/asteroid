//! Provider-independent three-way sync.
//!
//! For every path we compare the local content hash, the remote content hash
//! and the hash recorded at the last successful sync (the *base*). A side
//! that differs from the base has changed; when both changed to different
//! content we keep both copies instead of guessing.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::future::Future;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use asteroid_core::Vault;
use asteroid_core::vault::{self, parent_of};

use crate::{Error, Result};

const STATE_FILE: &str = "sync-state.json";

/// A file as seen on the remote side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub id: String,
    /// Vault-relative path with `/` separators.
    pub path: String,
    /// Lowercase hex MD5 of the content.
    pub md5: String,
}

/// A storage backend the engine can sync a vault with.
pub trait Remote {
    fn list(&mut self) -> impl Future<Output = Result<Vec<RemoteEntry>>> + Send;
    fn download(&mut self, entry: &RemoteEntry) -> impl Future<Output = Result<Vec<u8>>> + Send;
    /// Creates the file, or replaces the content of `existing`.
    fn upload(
        &mut self,
        path: &str,
        data: Vec<u8>,
        existing: Option<&RemoteEntry>,
    ) -> impl Future<Output = Result<RemoteEntry>> + Send;
    /// Removes the file; implementations should prefer a recoverable trash.
    fn delete(&mut self, entry: &RemoteEntry) -> impl Future<Output = Result<()>> + Send;
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CachedHash {
    size: u64,
    modified_ms: i64,
    md5: String,
}

/// Persistent per-vault sync state, stored in `.asteroid/sync-state.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncState {
    /// Content hash of each path as of the last successful sync.
    base: BTreeMap<String, String>,
    /// Hash cache so unchanged files are not re-read on every sync.
    #[serde(default)]
    hashes: BTreeMap<String, CachedHash>,
    #[serde(default)]
    pub last_sync_ms: Option<i64>,
}

impl SyncState {
    pub fn load(vault: &Vault) -> Self {
        fs::read(vault.meta_dir().join(STATE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, vault: &Vault) -> Result<()> {
        let json = serde_json::to_vec_pretty(self)?;
        vault.write_bytes(&vault::join(vault::META_DIR, STATE_FILE), &json)?;
        Ok(())
    }

    /// Forgets everything, e.g. when the vault is linked to another folder.
    pub fn reset(&mut self) {
        *self = SyncState::default();
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SyncOptions {
    /// Allow a sync that would delete more than half of the synced files.
    pub allow_mass_delete: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncReport {
    pub uploaded: Vec<String>,
    pub downloaded: Vec<String>,
    pub deleted_local: Vec<String>,
    pub deleted_remote: Vec<String>,
    /// Conflict copies created (paths of the preserved local versions).
    pub conflicts: Vec<String>,
    pub errors: Vec<String>,
    /// Files deliberately left out (e.g. too large for the remote).
    pub skipped: Vec<String>,
}

impl SyncReport {
    pub fn changed_local(&self) -> bool {
        !self.downloaded.is_empty() || !self.deleted_local.is_empty() || !self.conflicts.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Upload(String),
    Download(String),
    DeleteLocal(String),
    DeleteRemote(String),
    Conflict(String),
    /// Both sides already match: just record the base.
    Record(String, String),
    /// Gone on both sides: drop it from the base.
    Forget(String),
}

/// Decides what to do for each path. Pure, so it is exhaustively testable.
pub fn plan(
    local: &BTreeMap<String, String>,
    remote: &BTreeMap<String, String>,
    base: &BTreeMap<String, String>,
) -> Vec<Action> {
    let paths: BTreeSet<&String> = local.keys().chain(remote.keys()).chain(base.keys()).collect();
    let mut actions = Vec::new();
    for path in paths {
        let (l, r, b) = (local.get(path), remote.get(path), base.get(path));
        let p = path.clone();
        let action = match (l, r) {
            (Some(l), Some(r)) if l == r => {
                if b == Some(l) { continue } else { Action::Record(p, l.clone()) }
            }
            (None, None) => Action::Forget(p),
            _ => match b {
                None => match (l, r) {
                    (Some(_), None) => Action::Upload(p),
                    (None, Some(_)) => Action::Download(p),
                    _ => Action::Conflict(p),
                },
                Some(b) => {
                    let local_changed = l != Some(b);
                    let remote_changed = r != Some(b);
                    match (local_changed, remote_changed, l, r) {
                        (false, true, _, None) => Action::DeleteLocal(p),
                        (false, true, _, Some(_)) => Action::Download(p),
                        (true, false, None, _) => Action::DeleteRemote(p),
                        (true, false, Some(_), _) => Action::Upload(p),
                        // Both changed. An edit beats a deletion.
                        (true, true, None, Some(_)) => Action::Download(p),
                        (true, true, Some(_), None) => Action::Upload(p),
                        _ => Action::Conflict(p),
                    }
                }
            },
        };
        actions.push(action);
    }
    actions
}

pub fn md5_hex(data: &[u8]) -> String {
    hex(&Md5::digest(data))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(DIGITS[(b >> 4) as usize] as char);
        out.push(DIGITS[(b & 0xf) as usize] as char);
    }
    out
}

/// Hashes every local file, reusing cached hashes for unchanged files.
fn local_hashes(vault: &Vault, state: &mut SyncState) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut fresh = BTreeMap::new();
    for file in vault.files()? {
        let md5 = match state.hashes.get(&file.path) {
            Some(c) if c.size == file.size && c.modified_ms == file.modified_ms => c.md5.clone(),
            _ => md5_hex(&vault.read_bytes(&file.path)?),
        };
        fresh.insert(
            file.path.clone(),
            CachedHash { size: file.size, modified_ms: file.modified_ms, md5: md5.clone() },
        );
        out.insert(file.path, md5);
    }
    state.hashes = fresh;
    Ok(out)
}

fn remember_local(vault: &Vault, state: &mut SyncState, path: &str, md5: &str) {
    if let Ok(meta) = vault.resolve(path).and_then(|p| Ok(fs::metadata(p)?)) {
        state.hashes.insert(
            path.to_string(),
            CachedHash { size: meta.len(), modified_ms: vault::modified_ms(&meta), md5: md5.to_string() },
        );
    }
}

/// Runs one full sync of `vault` against `remote`, updating `state`.
/// Per-file failures are collected in the report rather than aborting.
pub async fn sync<R: Remote>(
    vault: &Vault,
    remote: &mut R,
    state: &mut SyncState,
    options: SyncOptions,
) -> Result<SyncReport> {
    let local = local_hashes(vault, state)?;
    let remote_entries: BTreeMap<String, RemoteEntry> =
        remote.list().await?.into_iter().map(|e| (e.path.clone(), e)).collect();
    let remote_hashes: BTreeMap<String, String> =
        remote_entries.iter().map(|(p, e)| (p.clone(), e.md5.clone())).collect();

    let actions = plan(&local, &remote_hashes, &state.base);
    guard_against_mass_deletion(&actions, state.base.len(), options)?;

    let mut report = SyncReport::default();
    for action in actions {
        let result = apply(vault, remote, state, &local, &remote_entries, &action, &mut report).await;
        if let Err(err) = result {
            report.errors.push(format!("{}: {err}", action_path(&action)));
        }
    }
    state.last_sync_ms = Some(chrono::Utc::now().timestamp_millis());
    state.save(vault)?;
    Ok(report)
}

fn action_path(action: &Action) -> &str {
    match action {
        Action::Upload(p)
        | Action::Download(p)
        | Action::DeleteLocal(p)
        | Action::DeleteRemote(p)
        | Action::Conflict(p)
        | Action::Record(p, _)
        | Action::Forget(p) => p,
    }
}

/// Refuses syncs that would wipe out most of the vault on either side, which
/// usually means the wrong folder was linked or a listing came back empty.
fn guard_against_mass_deletion(actions: &[Action], base_len: usize, options: SyncOptions) -> Result<()> {
    if options.allow_mass_delete || base_len < 10 {
        return Ok(());
    }
    let local_deletes = actions.iter().filter(|a| matches!(a, Action::DeleteLocal(_))).count();
    let remote_deletes = actions.iter().filter(|a| matches!(a, Action::DeleteRemote(_))).count();
    let limit = base_len / 2;
    if local_deletes > limit {
        return Err(Error::Unsafe(format!(
            "Sync stopped: it would remove {local_deletes} of {base_len} local files. Check the linked Drive folder."
        )));
    }
    if remote_deletes > limit {
        return Err(Error::Unsafe(format!(
            "Sync stopped: it would remove {remote_deletes} of {base_len} files from Drive."
        )));
    }
    Ok(())
}

async fn apply<R: Remote>(
    vault: &Vault,
    remote: &mut R,
    state: &mut SyncState,
    local: &BTreeMap<String, String>,
    remote_entries: &BTreeMap<String, RemoteEntry>,
    action: &Action,
    report: &mut SyncReport,
) -> Result<()> {
    match action {
        Action::Upload(path) => {
            let data = vault.read_bytes(path)?;
            let md5 = md5_hex(&data);
            remote.upload(path, data, remote_entries.get(path)).await?;
            state.base.insert(path.clone(), md5);
            report.uploaded.push(path.clone());
        }
        Action::Download(path) => {
            let entry = &remote_entries[path];
            let data = remote.download(entry).await?;
            let md5 = md5_hex(&data);
            vault.write_bytes(path, &data)?;
            remember_local(vault, state, path, &md5);
            state.base.insert(path.clone(), md5);
            report.downloaded.push(path.clone());
        }
        Action::DeleteLocal(path) => {
            vault.trash(path)?;
            state.base.remove(path);
            state.hashes.remove(path);
            report.deleted_local.push(path.clone());
        }
        Action::DeleteRemote(path) => {
            remote.delete(&remote_entries[path]).await?;
            state.base.remove(path);
            report.deleted_remote.push(path.clone());
        }
        Action::Conflict(path) => {
            // Keep the local edit as a sibling copy, take the remote version
            // at the original path, and upload the copy so no edit is lost.
            let entry = &remote_entries[path];
            let remote_data = remote.download(entry).await?;
            let local_data = vault.read_bytes(path)?;
            let copy = conflict_path(vault, path)?;
            vault.write_bytes(&copy, &local_data)?;
            vault.write_bytes(path, &remote_data)?;
            let remote_md5 = md5_hex(&remote_data);
            remember_local(vault, state, path, &remote_md5);
            state.base.insert(path.clone(), remote_md5);

            let copy_md5 = local.get(path).cloned().unwrap_or_else(|| md5_hex(&local_data));
            remote.upload(&copy, local_data, None).await?;
            remember_local(vault, state, &copy, &copy_md5);
            state.base.insert(copy.clone(), copy_md5);
            report.conflicts.push(copy);
        }
        Action::Record(path, md5) => {
            state.base.insert(path.clone(), md5.clone());
        }
        Action::Forget(path) => {
            state.base.remove(path);
        }
    }
    Ok(())
}

fn conflict_path(vault: &Vault, path: &str) -> Result<String> {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M");
    let stem = vault::stem_of(path);
    let ext = vault::extension(path);
    let base = format!("{stem} (conflict {stamp})");
    Ok(vault.free_path(parent_of(path), &base, ext.as_deref())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn plan_covers_every_case() {
        let local = map(&[("same", "1"), ("new-local", "1"), ("edit-local", "2"), ("edit-remote", "1"), ("both", "2"), ("del-remote", "1"), ("edit-vs-del", "2")]);
        let remote = map(&[("same", "1"), ("new-remote", "1"), ("edit-local", "1"), ("edit-remote", "2"), ("both", "3"), ("del-local", "1"), ("del-vs-edit", "2")]);
        let base = map(&[("same", "1"), ("edit-local", "1"), ("edit-remote", "1"), ("both", "1"), ("del-local", "1"), ("del-remote", "1"), ("gone", "1"), ("edit-vs-del", "1"), ("del-vs-edit", "1")]);
        let actions = plan(&local, &remote, &base);
        let expect = [
            Action::Conflict("both".into()),
            Action::DeleteRemote("del-local".into()),
            Action::DeleteLocal("del-remote".into()),
            Action::Download("del-vs-edit".into()),
            Action::Upload("edit-local".into()),
            Action::Download("edit-remote".into()),
            Action::Upload("edit-vs-del".into()),
            Action::Forget("gone".into()),
            Action::Upload("new-local".into()),
            Action::Download("new-remote".into()),
        ];
        assert_eq!(actions, expect);
    }

    #[test]
    fn first_sync_with_identical_content_records_base() {
        let both = map(&[("a", "1")]);
        assert_eq!(plan(&both, &both, &BTreeMap::new()), [Action::Record("a".into(), "1".into())]);
    }

    /// In-memory remote for end-to-end engine tests.
    #[derive(Default)]
    struct MemRemote {
        files: HashMap<String, Vec<u8>>,
    }

    impl Remote for MemRemote {
        async fn list(&mut self) -> Result<Vec<RemoteEntry>> {
            Ok(self
                .files
                .iter()
                .map(|(p, d)| RemoteEntry { id: p.clone(), path: p.clone(), md5: md5_hex(d) })
                .collect())
        }
        async fn download(&mut self, entry: &RemoteEntry) -> Result<Vec<u8>> {
            Ok(self.files[&entry.path].clone())
        }
        async fn upload(&mut self, path: &str, data: Vec<u8>, _: Option<&RemoteEntry>) -> Result<RemoteEntry> {
            let md5 = md5_hex(&data);
            self.files.insert(path.to_string(), data);
            Ok(RemoteEntry { id: path.into(), path: path.into(), md5 })
        }
        async fn delete(&mut self, entry: &RemoteEntry) -> Result<()> {
            self.files.remove(&entry.path);
            Ok(())
        }
    }

    #[tokio::test]
    async fn two_devices_converge_and_conflicts_keep_both() {
        let (d1, d2) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let (a, b) = (Vault::open(d1.path()).unwrap(), Vault::open(d2.path()).unwrap());
        let (mut sa, mut sb) = (SyncState::default(), SyncState::default());
        let mut remote = MemRemote::default();
        let opts = SyncOptions::default();

        a.write_text("Note.md", "v1").unwrap();
        a.write_text("Sub/Other.md", "x").unwrap();
        sync(&a, &mut remote, &mut sa, opts).await.unwrap();
        let r = sync(&b, &mut remote, &mut sb, opts).await.unwrap();
        assert_eq!(r.downloaded.len(), 2);
        assert_eq!(b.read_text("Sub/Other.md").unwrap(), "x");

        // Concurrent edits to the same note on both devices.
        a.write_text("Note.md", "from a").unwrap();
        b.write_text("Note.md", "from b").unwrap();
        sync(&a, &mut remote, &mut sa, opts).await.unwrap();
        let r = sync(&b, &mut remote, &mut sb, opts).await.unwrap();
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(b.read_text("Note.md").unwrap(), "from a");
        assert_eq!(b.read_text(&r.conflicts[0]).unwrap(), "from b");

        // Deletion propagates and lands in the other device's trash.
        b.trash("Sub/Other.md").unwrap();
        sync(&b, &mut remote, &mut sb, opts).await.unwrap();
        let r = sync(&a, &mut remote, &mut sa, opts).await.unwrap();
        assert_eq!(r.deleted_local, ["Sub/Other.md"]);
        assert!(a.exists(".trash/Sub/Other.md"));

        // Device A now has the conflict copy too: both sides converge.
        let files = |v: &Vault| {
            let mut f: Vec<_> = v.files().unwrap().into_iter().map(|f| f.path).collect();
            f.sort();
            f
        };
        assert_eq!(files(&a), files(&b));
    }

    #[tokio::test]
    async fn refuses_to_wipe_local_vault_when_remote_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        let mut state = SyncState::default();
        let mut remote = MemRemote::default();
        for i in 0..12 {
            vault.write_text(&format!("n{i}.md"), &i.to_string()).unwrap();
        }
        sync(&vault, &mut remote, &mut state, SyncOptions::default()).await.unwrap();
        remote.files.clear();
        let err = sync(&vault, &mut remote, &mut state, SyncOptions::default()).await.unwrap_err();
        assert!(matches!(err, Error::Unsafe(_)));
        assert_eq!(vault.files().unwrap().len(), 12);
    }
}
