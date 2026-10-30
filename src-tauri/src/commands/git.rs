//! GitHub sign-in and git-based vault sync.

use std::path::Path;
use std::sync::atomic::Ordering;

use asteroid_core::Vault;
use asteroid_git::github::{self, DeviceCode, Github, GithubUser, Repo};
use asteroid_git::history::{self, Against, DeletedFile, FileDiff, Version};
use asteroid_git::ignore::{self, IgnoreRules};
use asteroid_git::sync::SizeReport;
use asteroid_git::{Auth, Author, GitTarget};
use asteroid_sync::SyncReport;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use super::vault::{VaultInfo, activate};
use crate::config::{self, GITHUB_TOKEN, SyncLink};
use crate::state::{AppState, CmdResult, err};

#[derive(Serialize, Deserialize)]
struct StoredGithub {
    token: String,
    user: GithubUser,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    /// A GitHub OAuth client id is available (from settings or the environment).
    configured: bool,
    connected: bool,
    login: Option<String>,
}

fn client_id(app: &AppHandle) -> CmdResult<String> {
    config::credential(app, "githubClientId", "ASTEROID_GITHUB_CLIENT_ID", option_env!("ASTEROID_GITHUB_CLIENT_ID"))
        .ok_or_else(|| "Add a GitHub OAuth client ID in Settings › Sync first.".to_string())
}

fn stored(app: &AppHandle) -> Option<StoredGithub> {
    config::read_json(app, GITHUB_TOKEN)
}

fn github(app: &AppHandle) -> CmdResult<Github> {
    let s = stored(app).ok_or("GitHub is not connected.")?;
    Ok(Github::new(s.token))
}

#[tauri::command(async)]
pub fn github_status(app: AppHandle) -> GithubStatus {
    let s = stored(&app);
    GithubStatus { configured: client_id(&app).is_ok(), connected: s.is_some(), login: s.map(|s| s.user.login) }
}

/// Starts sign-in: returns the code the user enters on github.com.
#[tauri::command]
pub async fn github_device_start(app: AppHandle) -> CmdResult<DeviceCode> {
    github::start_device_flow(&client_id(&app)?).await.map_err(err)
}

/// Waits for the user to approve `code` on github.com, then stores the token.
#[tauri::command]
pub async fn github_device_poll(app: AppHandle, state: State<'_, AppState>, code: DeviceCode) -> CmdResult<String> {
    let flow = state.github_flow.fetch_add(1, Ordering::SeqCst) + 1;
    let client_id = client_id(&app)?;
    let token = github::poll_device_flow(&client_id, &code, || {
        app.state::<AppState>().github_flow.load(Ordering::SeqCst) != flow
    })
    .await
    .map_err(err)?;
    let user = Github::new(token.clone()).user().await.map_err(err)?;
    let login = user.login.clone();
    config::write_json(&app, GITHUB_TOKEN, &StoredGithub { token, user })?;
    Ok(login)
}

#[tauri::command(async)]
pub fn github_device_cancel(state: State<'_, AppState>) {
    state.github_flow.fetch_add(1, Ordering::SeqCst);
}

#[tauri::command(async)]
pub fn github_disconnect(app: AppHandle) -> CmdResult<()> {
    config::remove(&app, GITHUB_TOKEN)
}

#[tauri::command]
pub async fn github_repos(app: AppHandle) -> CmdResult<Vec<Repo>> {
    github(&app)?.repos().await.map_err(err)
}

#[tauri::command]
pub async fn github_create_repo(app: AppHandle, name: String, private: bool) -> CmdResult<Repo> {
    github(&app)?.create_repo(name.trim(), private).await.map_err(err)
}

/// Links the open vault to a repository. A vault that already is a git
/// repository keeps its branch; otherwise the repository's default is used.
#[tauri::command(async)]
pub fn git_link(state: State<'_, AppState>, repo: Repo) -> CmdResult<SyncLink> {
    let vault = state.vault()?;
    let branch = asteroid_git::sync::current_branch(vault.root()).unwrap_or(repo.default_branch);
    // Fails early on e.g. a different existing remote, before anything is saved.
    asteroid_git::sync::prepare(vault.root(), &repo.clone_url, &branch).map_err(err)?;
    let link = SyncLink::Git { remote_url: repo.clone_url, repo_name: repo.full_name, branch };
    link.save(&vault)?;
    Ok(link)
}

/// Clones a repository into `dest_parent/<repo name>` and opens it.
#[tauri::command]
pub async fn git_clone_vault(
    app: AppHandle,
    state: State<'_, AppState>,
    repo: Repo,
    dest_parent: String,
) -> CmdResult<VaultInfo> {
    let _running = state.sync_lock.try_lock().map_err(|_| "A sync is already running.")?;
    let dest = Path::new(&dest_parent).join(&repo.name);
    if dest.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false) {
        return Err(format!("{} already exists and is not empty.", dest.display()));
    }
    let auth = auth_for(&app, &repo.clone_url);
    let (url, branch, target_dir) = (repo.clone_url.clone(), repo.default_branch.clone(), dest.clone());
    blocking(move || {
        let target = GitTarget { url: &url, branch: &branch, auth: auth.as_ref() };
        asteroid_git::sync::clone(&target, &target_dir).map_err(err)
    })
    .await?;
    let vault = Vault::open(&dest).map_err(err)?;
    SyncLink::Git { remote_url: repo.clone_url, repo_name: repo.full_name, branch: repo.default_branch }.save(&vault)?;
    activate(&app, &state, vault)
}

/// One git sync of `vault`, reported in the same shape as a Drive sync.
pub async fn run_git_sync(app: &AppHandle, vault: &Vault, url: &str, branch: &str) -> CmdResult<SyncReport> {
    let auth = auth_for(app, url);
    let author = author(app, vault.root());
    let (root, url, branch) = (vault.root().to_path_buf(), url.to_string(), branch.to_string());
    let report = blocking(move || {
        let target = GitTarget { url: &url, branch: &branch, auth: auth.as_ref() };
        asteroid_git::sync::sync(&root, &target, &author, &device_name()).map_err(err)
    })
    .await?;
    Ok(SyncReport {
        uploaded: report.committed,
        downloaded: report.pulled,
        conflicts: report.conflicts,
        skipped: report.too_large.into_iter().map(|f| f.path).collect(),
        ..Default::default()
    })
}

/// Versions of one file in the vault's git history, newest first.
#[tauri::command]
pub async fn git_file_history(state: State<'_, AppState>, path: String) -> CmdResult<Vec<Version>> {
    let vault = state.vault()?;
    vault.resolve(&path).map_err(err)?;
    let root = vault.root().to_path_buf();
    blocking(move || history::file_history(&root, &path, 500).map_err(err)).await
}

/// A version's diff: against the version before it ("previous"), or the
/// file on disk compared with it ("working").
#[tauri::command]
pub async fn git_version_diff(state: State<'_, AppState>, path: String, commit: String, against: String) -> CmdResult<FileDiff> {
    let vault = state.vault()?;
    let against = if against == "working" { Against::Working } else { Against::Previous };
    let working = match against {
        Against::Working => Some(vault.read_bytes(&path).unwrap_or_default()),
        Against::Previous => None,
    };
    let root = vault.root().to_path_buf();
    blocking(move || history::diff_version(&root, &commit, &path, against, working.as_deref()).map_err(err)).await
}

/// A version's text, for previewing (invalid UTF-8 is replaced).
#[tauri::command]
pub async fn git_version_text(state: State<'_, AppState>, path: String, commit: String) -> CmdResult<String> {
    let root = state.vault()?.root().to_path_buf();
    let bytes = blocking(move || history::read_version(&root, &commit, &path).map_err(err)).await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Files deleted from the vault that git can still bring back.
#[tauri::command]
pub async fn git_deleted_files(state: State<'_, AppState>) -> CmdResult<Vec<DeletedFile>> {
    let vault = state.vault()?;
    blocking(move || history::deleted_files(vault.root(), 300, |p| vault.exists(p)).map_err(err)).await
}

/// Writes the file as it was at `commit` back into the vault. The next sync
/// commits it like any other edit, so restoring is itself undoable.
#[tauri::command]
pub async fn git_restore_version(state: State<'_, AppState>, path: String, commit: String) -> CmdResult<()> {
    let vault = state.vault()?;
    let root = vault.root().to_path_buf();
    let rel = path.clone();
    let bytes = blocking(move || history::read_version(&root, &commit, &rel).map_err(err)).await?;
    vault.write_bytes(&path, &bytes).map_err(err)?;
    state.with_mut(|open| open.index.refresh(&open.vault, &path))
}

/// The vault's "don't sync" rules (Asteroid's block of `.gitignore`).
#[tauri::command(async)]
pub fn git_ignore_rules(state: State<'_, AppState>) -> CmdResult<IgnoreRules> {
    Ok(ignore::read_rules(state.vault()?.root()))
}

/// Replaces the "don't sync" rules; returns them as stored (normalised).
#[tauri::command(async)]
pub fn git_set_ignore_rules(state: State<'_, AppState>, rules: IgnoreRules) -> CmdResult<IgnoreRules> {
    let vault = state.vault()?;
    for path in &rules.paths {
        vault.resolve(path.trim_end_matches('/')).map_err(err)?;
    }
    ignore::write_rules(vault.root(), &rules).map_err(err)?;
    Ok(ignore::read_rules(vault.root()))
}

/// The open vault measured against GitHub's file and repository size limits.
#[tauri::command]
pub async fn git_size_report(state: State<'_, AppState>) -> CmdResult<SizeReport> {
    let root = state.vault()?.root().to_path_buf();
    blocking(move || Ok(asteroid_git::sync::size_report(&root))).await
}

/// The GitHub token, for github.com remotes only.
fn auth_for(app: &AppHandle, url: &str) -> Option<Auth> {
    if !github::is_github_url(url) {
        return None;
    }
    stored(app).map(|s| Auth { username: s.user.login, token: s.token })
}

/// Commits are attributed to the GitHub account when connected, else to the
/// user's git config, else to a neutral name.
fn author(app: &AppHandle, root: &Path) -> Author {
    if let Some(s) = stored(app) {
        let email = s.user.noreply_email();
        return Author { name: s.user.name.unwrap_or(s.user.login), email };
    }
    asteroid_git::sync::configured_author(root).unwrap_or_else(|| Author {
        name: "Asteroid".into(),
        email: "asteroid@users.noreply.github.com".into(),
    })
}

fn device_name() -> String {
    gethostname::gethostname().to_string_lossy().trim().chars().take(40).collect()
}

/// git2 is blocking; run it off the async runtime's worker threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)?
}
