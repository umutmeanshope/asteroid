//! Settings are owned by the UI; the backend only persists them and reads
//! the Google OAuth client fields.

use tauri::AppHandle;

use crate::config::{self, SETTINGS};
use crate::state::CmdResult;

#[tauri::command(async)]
pub fn load_settings(app: AppHandle) -> serde_json::Value {
    config::read_json(&app, SETTINGS).unwrap_or_else(|| serde_json::json!({}))
}

#[tauri::command(async)]
pub fn save_settings(app: AppHandle, settings: serde_json::Value) -> CmdResult<()> {
    config::write_json(&app, SETTINGS, &settings)
}
