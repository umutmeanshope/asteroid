use serde::{Deserialize, Serialize};
use asteroid_core::{Index, Vault};
use asteroid_sync::gdrive::{self, Drive, DriveFolder, DriveVault, OAuthClient, Token};
use asteroid_sync::{SyncOptions, SyncReport, SyncState};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::vault::{VaultInfo, activate};
use crate::config::{self, GOOGLE_TOKEN, SyncLink};
use crate::state::{AppState, CmdResult, err};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredToken {
    #[serde(flatten)]
    token: Token,
    email: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveStatus {
    /// An OAuth client id is available (from settings or the environment).
    configured: bool,
    connected: bool,
    email: Option<String>,
}

/// The OAuth client comes from Settings, falling back to environment
/// variables at run time and then at build time.
fn oauth_client(app: &AppHandle) -> CmdResult<OAuthClient> {
    let client_id = config::credential(app, "googleClientId", "ASTEROID_GOOGLE_CLIENT_ID", option_env!("ASTEROID_GOOGLE_CLIENT_ID"))
        .ok_or("Add a Google OAuth client ID in Settings › Sync first.")?;
    let client_secret = config::credential(
        app,
        "googleClientSecret",
        "ASTEROID_GOOGLE_CLIENT_SECRET",
        option_env!("ASTEROID_GOOGLE_CLIENT_SECRET"),
    );
    Ok(OAuthClient { client_id, client_secret })
}

fn load_drive(app: &AppHandle) -> CmdResult<(Drive, Option<String>)> {
    let stored: StoredToken =
        config::read_json(app, GOOGLE_TOKEN).ok_or("Google Drive is not connected.")?;
    Ok((Drive::new(oauth_client(app)?, stored.token), stored.email))
}

/// Persists the (possibly refreshed) token after a Drive session.
fn save_drive(app: &AppHandle, drive: &Drive, email: Option<String>) -> CmdResult<()> {
    config::write_json(app, GOOGLE_TOKEN, &StoredToken { token: drive.token().clone(), email })
}

#[tauri::command(async)]
pub fn gdrive_status(app: AppHandle) -> DriveStatus {
    let stored: Option<StoredToken> = config::read_json(&app, GOOGLE_TOKEN);
    DriveStatus {
        configured: oauth_client(&app).is_ok(),
        connected: stored.is_some(),
        email: stored.and_then(|s| s.email),
    }
}

#[tauri::command]
pub async fn gdrive_connect(app: AppHandle) -> CmdResult<String> {
    let client = oauth_client(&app)?;
    let opener = app.clone();
    let token = gdrive::authorize(&client, move |url| {
        opener
            .opener()
            .open_url(url, None::<&str>)
            .map_err(|e| asteroid_sync::Error::Auth(e.to_string()))
    })
    .await
    .map_err(err)?;
    let mut drive = Drive::new(client, token);
    let email = drive.user_email().await.map_err(err)?;
    save_drive(&app, &drive, Some(email.clone()))?;
    Ok(email)
}

#[tauri::command(async)]
pub fn gdrive_disconnect(app: AppHandle) -> CmdResult<()> {
    config::remove(&app, GOOGLE_TOKEN)
}

#[tauri::command]
pub async fn gdrive_list_folders(app: AppHandle, parent: Option<String>) -> CmdResult<Vec<DriveFolder>> {
    let (mut drive, email) = load_drive(&app)?;
    let result = drive.list_folders(parent.as_deref().unwrap_or("root")).await;
    save_drive(&app, &drive, email)?;
    result.map_err(err)
}

#[tauri::command]
pub async fn gdrive_create_folder(app: AppHandle, name: String, parent: Option<String>) -> CmdResult<DriveFolder> {
    let (mut drive, email) = load_drive(&app)?;
    let result = drive.create_folder(name.trim(), parent.as_deref().unwrap_or("root")).await;
    save_drive(&app, &drive, email)?;
    result.map_err(err)
}

/// Links the open vault to a Drive folder. Sync state starts fresh.
#[tauri::command(async)]
pub fn sync_link(state: State<'_, AppState>, folder_id: String, folder_name: String) -> CmdResult<SyncLink> {
    let vault = state.vault()?;
    let link = SyncLink::Drive { folder_id, folder_name };
    link.save(&vault)?;
    let mut sync_state = SyncState::load(&vault);
    sync_state.reset();
    sync_state.save(&vault).map_err(err)?;
    Ok(link)
}

#[tauri::command(async)]
pub fn sync_unlink(state: State<'_, AppState>) -> CmdResult<()> {
    SyncLink::remove(&state.vault()?)
}

async fn run_sync(app: &AppHandle, vault: &Vault, folder_id: &str, options: SyncOptions) -> CmdResult<SyncReport> {
    let (mut drive, email) = load_drive(app)?;
    let mut sync_state = SyncState::load(vault);
    let result = {
        let mut remote = DriveVault::new(&mut drive, folder_id.to_string());
        asteroid_sync::sync(vault, &mut remote, &mut sync_state, options).await
    };
    save_drive(app, &drive, email)?;
    result.map_err(err)
}

#[tauri::command]
pub async fn sync_now(
    app: AppHandle,
    state: State<'_, AppState>,
    allow_mass_delete: Option<bool>,
) -> CmdResult<SyncReport> {
    let _running = state.sync_lock.try_lock().map_err(|_| "A sync is already running.")?;
    let vault = state.vault()?;
    let link = SyncLink::load(&vault).ok_or("This vault isn't set up to sync.")?;
    let report = match &link {
        SyncLink::Drive { folder_id, .. } => {
            let options = SyncOptions { allow_mass_delete: allow_mass_delete.unwrap_or(false) };
            run_sync(&app, &vault, folder_id, options).await?
        }
        SyncLink::Git { remote_url, branch, .. } => super::git::run_git_sync(&app, &vault, remote_url, branch).await?,
    };
    if report.changed_local() {
        let index = Index::build(&vault).map_err(err)?;
        state.with_mut(|open| {
            if open.vault.root() == vault.root() {
                open.index = index;
            }
        })?;
    }
    Ok(report)
}

/// Downloads a Drive folder into a new local vault and opens it.
#[tauri::command]
pub async fn pull_vault(
    app: AppHandle,
    state: State<'_, AppState>,
    folder_id: String,
    folder_name: String,
    dest_parent: String,
) -> CmdResult<VaultInfo> {
    let _running = state.sync_lock.try_lock().map_err(|_| "A sync is already running.")?;
    let dest = std::path::Path::new(&dest_parent).join(folder_name.trim());
    let not_empty = dest.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false);
    if not_empty {
        return Err(format!("{} already exists and is not empty.", dest.display()));
    }
    std::fs::create_dir_all(&dest).map_err(err)?;
    let vault = Vault::open(&dest).map_err(err)?;
    SyncLink::Drive { folder_id: folder_id.clone(), folder_name }.save(&vault)?;
    run_sync(&app, &vault, &folder_id, SyncOptions::default()).await?;
    activate(&app, &state, vault)
}
