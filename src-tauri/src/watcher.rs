//! Watches the open vault for external changes (other apps, sync, git),
//! keeps the index current and tells the UI which paths changed.
//!
//! Folders are watched one by one rather than recursively so hidden folders
//! (`.git`, `.obsidian`, `.trash`, …) cost nothing: a recursive inotify watch
//! would add a watch for every folder inside them too.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::time::Duration;

use asteroid_core::Vault;
use notify::event::ModifyKind;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Manager};
use walkdir::WalkDir;

use crate::state::{AppState, CmdResult, err};

const DEBOUNCE: Duration = Duration::from_millis(250);

/// Shared so the event thread can watch folders created later. The thread only
/// holds a weak handle: dropping the app's handle stops the watcher, which
/// closes the channel and ends the thread.
pub type SharedWatcher = Arc<Mutex<RecommendedWatcher>>;

#[derive(Clone, serde::Serialize)]
struct VaultChanged {
    paths: Vec<String>,
}

pub fn start(app: &AppHandle, vault: &Vault) -> CmdResult<SharedWatcher> {
    let (tx, rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut watcher = notify::recommended_watcher(tx).map_err(err)?;
    watch_tree(&mut watcher, vault.root());
    let shared = Arc::new(Mutex::new(watcher));

    let weak = Arc::downgrade(&shared);
    let app = app.clone();
    let vault = vault.clone();
    std::thread::spawn(move || {
        // The loop ends when the watcher (and with it the sender) is dropped.
        while let Ok(first) = rx.recv() {
            let mut paths = BTreeSet::new();
            collect(&vault, &weak, first, &mut paths);
            while let Ok(next) = rx.recv_timeout(DEBOUNCE) {
                collect(&vault, &weak, next, &mut paths);
            }
            if paths.is_empty() {
                continue;
            }
            let state = app.state::<AppState>();
            let current = state
                .with_mut(|open| {
                    if open.vault.root() != vault.root() {
                        return false;
                    }
                    for path in &paths {
                        open.index.refresh(&open.vault, path);
                    }
                    true
                })
                .unwrap_or(false);
            if current {
                let _ = app.emit("vault-changed", VaultChanged { paths: paths.into_iter().collect() });
            }
        }
    });
    Ok(shared)
}

/// Watches `root` and every visible folder below it, each non-recursively.
fn watch_tree(watcher: &mut RecommendedWatcher, root: &Path) {
    let dirs = WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir());
    for dir in dirs {
        // A folder that vanished mid-walk, or the inotify limit: skip it
        // rather than failing to open the vault.
        let _ = watcher.watch(dir.path(), RecursiveMode::NonRecursive);
    }
}

fn collect(vault: &Vault, watcher: &Weak<Mutex<RecommendedWatcher>>, event: notify::Result<notify::Event>, out: &mut BTreeSet<String>) {
    let Ok(event) = event else { return };
    if event.kind.is_access() {
        return;
    }
    for path in event.paths {
        let Some(rel) = vault.relative(&path) else { continue };
        if rel.is_empty() || Vault::is_hidden(&rel) {
            continue;
        }
        // New or moved-in folders need their own watches.
        let added = event.kind.is_create() || matches!(event.kind, EventKind::Modify(ModifyKind::Name(_)));
        if added && path.is_dir()
            && let Some(watcher) = watcher.upgrade()
            && let Ok(mut watcher) = watcher.lock()
        {
            watch_tree(&mut watcher, &path);
        }
        out.insert(rel);
    }
}
