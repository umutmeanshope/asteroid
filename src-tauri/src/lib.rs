//! Tauri shell: wires the platform-independent core and sync crates to the
//! web UI through commands and events. Kept thin so the same core can back
//! the mobile apps.

use tauri::Manager;

mod commands;
mod config;
mod state;
mod watcher;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::default())
        .setup(|app| {
            // Dev builds keep separate settings (see config::config_dir); say so.
            if cfg!(debug_assertions)
                && let Some(window) = app.get_webview_window("main")
            {
                let _ = window.set_title("Asteroid Dev");
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault::recent_vaults,
            commands::vault::forget_recent_vault,
            commands::vault::open_vault,
            commands::vault::create_vault,
            commands::vault::close_vault,
            commands::vault::vault_tree,
            commands::vault::read_note,
            commands::vault::write_note,
            commands::vault::read_file,
            commands::vault::create_note,
            commands::vault::create_folder,
            commands::vault::rename_entry,
            commands::vault::move_entry,
            commands::vault::delete_entry,
            commands::vault::import_files,
            commands::vault::reveal_entry,
            commands::index::note_index,
            commands::index::search,
            commands::index::backlinks,
            commands::index::outline,
            commands::index::resolve_link,
            commands::index::render_note,
            commands::index::render_embed,
            commands::index::tags,
            commands::settings::load_settings,
            commands::settings::save_settings,
            commands::sync::gdrive_status,
            commands::sync::gdrive_connect,
            commands::sync::gdrive_disconnect,
            commands::sync::gdrive_list_folders,
            commands::sync::gdrive_create_folder,
            commands::sync::sync_link,
            commands::sync::sync_unlink,
            commands::sync::sync_now,
            commands::sync::pull_vault,
            commands::git::github_status,
            commands::git::github_device_start,
            commands::git::github_device_poll,
            commands::git::github_device_cancel,
            commands::git::github_disconnect,
            commands::git::github_repos,
            commands::git::github_create_repo,
            commands::git::git_link,
            commands::git::git_clone_vault,
            commands::git::git_size_report,
            commands::git::git_file_history,
            commands::git::git_version_diff,
            commands::git::git_version_text,
            commands::git::git_deleted_files,
            commands::git::git_restore_version,
            commands::git::git_ignore_rules,
            commands::git::git_set_ignore_rules,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Asteroid");
}
