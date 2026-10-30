use std::path::Path;

use serde::Serialize;
use asteroid_core::vault::{self, stem_of};
use asteroid_core::{Entry, Index, Vault, relink};
use tauri::{AppHandle, State};

use crate::config::{self, RECENT, RecentVault, SyncLink};
use crate::state::{AppState, CmdResult, OpenVault, err};
use crate::watcher;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultInfo {
    pub name: String,
    pub path: String,
    pub sync: Option<SyncLink>,
    /// The vault is a git repository, so version history is available.
    pub has_history: bool,
}

impl VaultInfo {
    fn of(vault: &Vault) -> Self {
        Self {
            name: vault.name(),
            path: vault.root().display().to_string(),
            sync: SyncLink::load(vault),
            has_history: vault.root().join(".git").exists(),
        }
    }
}

#[tauri::command(async)]
pub fn recent_vaults(app: AppHandle) -> Vec<RecentVault> {
    let list: Vec<RecentVault> = config::read_json(&app, RECENT).unwrap_or_default();
    list.into_iter().filter(|v| Path::new(&v.path).is_dir()).collect()
}

#[tauri::command(async)]
pub fn forget_recent_vault(app: AppHandle, path: String) -> CmdResult<()> {
    let mut list: Vec<RecentVault> = config::read_json(&app, RECENT).unwrap_or_default();
    list.retain(|v| v.path != path);
    config::write_json(&app, RECENT, &list)
}

fn remember(app: &AppHandle, vault: &Vault) -> CmdResult<()> {
    let path = vault.root().display().to_string();
    let mut list: Vec<RecentVault> = config::read_json(app, RECENT).unwrap_or_default();
    list.retain(|v| v.path != path);
    list.insert(0, RecentVault {
        path,
        name: vault.name(),
        opened_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
    });
    list.truncate(12);
    config::write_json(app, RECENT, &list)
}

/// Makes `vault` the open vault: builds the index and starts watching.
pub fn activate(app: &AppHandle, state: &AppState, vault: Vault) -> CmdResult<VaultInfo> {
    let _switching = state.switching.lock().map_err(err)?;
    // Stop watching the previous vault before the slow work below.
    *state.watcher.lock().map_err(err)? = None;
    let index = Index::build(&vault).map_err(err)?;
    let info = VaultInfo::of(&vault);
    remember(app, &vault)?;
    *state.open.write().map_err(err)? = Some(OpenVault { vault: vault.clone(), index });
    // Started after the swap so its first events see the new vault as current.
    *state.watcher.lock().map_err(err)? = Some(watcher::start(app, &vault)?);
    Ok(info)
}

#[tauri::command(async)]
pub fn open_vault(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<VaultInfo> {
    let vault = Vault::open(&path).map_err(err)?;
    activate(&app, &state, vault)
}

#[tauri::command(async)]
pub fn create_vault(app: AppHandle, state: State<'_, AppState>, parent: String, name: String) -> CmdResult<VaultInfo> {
    let vault = Vault::create(&parent, name.trim()).map_err(err)?;
    activate(&app, &state, vault)
}

#[tauri::command(async)]
pub fn close_vault(state: State<'_, AppState>) -> CmdResult<()> {
    let _switching = state.switching.lock().map_err(err)?;
    *state.watcher.lock().map_err(err)? = None;
    *state.open.write().map_err(err)? = None;
    Ok(())
}

#[tauri::command(async)]
pub fn vault_tree(state: State<'_, AppState>) -> CmdResult<Vec<Entry>> {
    state.vault()?.tree().map_err(err)
}

#[tauri::command(async)]
pub fn read_note(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    state.vault()?.read_text(&path).map_err(err)
}

#[tauri::command(async)]
pub fn write_note(state: State<'_, AppState>, path: String, content: String) -> CmdResult<()> {
    let vault = state.vault()?;
    vault.write_text(&path, &content).map_err(err)?;
    state.with_mut(|open| open.index.insert_note(&path, content))
}

/// Raw bytes (PDFs, images) without JSON encoding overhead.
#[tauri::command(async)]
pub fn read_file(state: State<'_, AppState>, path: String) -> CmdResult<tauri::ipc::Response> {
    let bytes = state.vault()?.read_bytes(&path).map_err(err)?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command(async)]
pub fn create_note(state: State<'_, AppState>, folder: String, name: Option<String>) -> CmdResult<String> {
    let vault = state.vault()?;
    let path = vault.create_note(&folder, name.as_deref()).map_err(err)?;
    state.with_mut(|open| open.index.insert_note(&path, String::new()))?;
    Ok(path)
}

#[tauri::command(async)]
pub fn create_folder(state: State<'_, AppState>, parent: String, name: String) -> CmdResult<String> {
    state.vault()?.create_folder(&parent, name.trim()).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveResult {
    pub path: String,
    /// Notes whose links were rewritten to follow the move.
    pub updated_links: Vec<String>,
}

/// Finishes a rename or move that already happened on disk: optionally
/// rewrites links elsewhere in the vault, then updates the index.
fn finish_move(state: &AppState, vault: &Vault, from: &str, to: &str, update_links: bool) -> CmdResult<MoveResult> {
    // Planned against the index as it was, before the move is applied to it.
    let rewrites = if update_links {
        state.with(|open| relink::plan(&open.index, &[(from.to_string(), to.to_string())]))?
    } else {
        Vec::new()
    };
    for rewrite in &rewrites {
        vault.write_text(&rewrite.path, &rewrite.text).map_err(err)?;
    }
    state.with_mut(|open| {
        open.index.remove(from);
        open.index.refresh(&open.vault, to);
        for rewrite in &rewrites {
            open.index.insert_note(&rewrite.path, rewrite.text.clone());
        }
    })?;
    Ok(MoveResult { path: to.to_string(), updated_links: rewrites.into_iter().map(|r| r.path).collect() })
}

#[tauri::command(async)]
pub fn rename_entry(state: State<'_, AppState>, path: String, new_name: String, update_links: Option<bool>) -> CmdResult<MoveResult> {
    let vault = state.vault()?;
    // Renaming a file keeps its extension unless the new name repeats it,
    // so a title like "v1.2 notes" still becomes "v1.2 notes.md".
    let new_name = new_name.trim();
    let is_file = vault.resolve(&path).map(|p| p.is_file()).unwrap_or(false);
    let new_name = match vault::extension(&path) {
        Some(ext) if is_file && !new_name.to_lowercase().ends_with(&format!(".{ext}")) => format!("{new_name}.{ext}"),
        _ => new_name.to_string(),
    };
    if new_name == vault::file_name_of(&path) {
        return Ok(MoveResult { path, updated_links: Vec::new() });
    }
    let new_path = vault.rename(&path, &new_name).map_err(err)?;
    finish_move(&state, &vault, &path, &new_path, update_links.unwrap_or(true))
}

#[tauri::command(async)]
pub fn move_entry(state: State<'_, AppState>, path: String, new_parent: String, update_links: Option<bool>) -> CmdResult<MoveResult> {
    let vault = state.vault()?;
    let new_path = vault.move_entry(&path, &new_parent).map_err(err)?;
    finish_move(&state, &vault, &path, &new_path, update_links.unwrap_or(true))
}

#[tauri::command(async)]
pub fn delete_entry(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    state.vault()?.trash(&path).map_err(err)?;
    state.with_mut(|open| open.index.remove(&path))
}

/// Copies files from outside the vault (e.g. dropped PDFs) into `folder`.
#[tauri::command(async)]
pub fn import_files(state: State<'_, AppState>, paths: Vec<String>, folder: String) -> CmdResult<Vec<String>> {
    let vault = state.vault()?;
    let mut imported = Vec::new();
    for source in paths {
        let source = Path::new(&source);
        if !source.is_file() {
            continue;
        }
        let name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = vault::extension(&name);
        let rel = vault.free_path(&folder, stem_of(&name), ext.as_deref()).map_err(err)?;
        let dest = vault.resolve(&rel).map_err(err)?;
        std::fs::copy(source, dest).map_err(err)?;
        state.with_mut(|open| open.index.refresh(&open.vault, &rel))?;
        imported.push(rel);
    }
    Ok(imported)
}

#[tauri::command(async)]
pub fn reveal_entry(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let abs = state.vault()?.resolve(&path).map_err(err)?;
    tauri_plugin_opener::reveal_item_in_dir(abs).map_err(err)
}
