//! Google Drive backend.
//!
//! Sign-in uses OAuth 2.0 for installed apps: authorization code + PKCE with
//! a loopback redirect to `127.0.0.1` (the flow Google recommends for desktop
//! apps). Mobile shells will need a custom-scheme redirect instead.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::{RequestBuilder, Response, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::engine::{Remote, RemoteEntry, hex};
use crate::{Error, Result};

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3";
const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const FILE_FIELDS: &str = "id,name,mimeType,md5Checksum,modifiedTime";
/// Full Drive scope so existing folders (e.g. a vault uploaded from another
/// device) are visible. A published app would switch to `drive.file` plus
/// the Google Picker to avoid Google's restricted-scope verification.
pub const SCOPE: &str = "https://www.googleapis.com/auth/drive";
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// OAuth client registered in Google Cloud Console (type "Desktop app").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthClient {
    pub client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DriveFolder {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DriveFile {
    id: String,
    name: String,
    mime_type: String,
    md5_checksum: Option<String>,
    modified_time: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
    next_page_token: Option<String>,
}

fn random_token(bytes: usize) -> Result<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| Error::Auth(e.to_string()))?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Runs the interactive sign-in. `open_browser` receives the consent URL.
pub async fn authorize(client: &OAuthClient, open_browser: impl FnOnce(&str) -> Result<()>) -> Result<Token> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirect_uri = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let verifier = random_token(48)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_token(18)?;

    let url = url::Url::parse_with_params(
        AUTH_URL,
        &[
            ("client_id", client.client_id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state.as_str()),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )
    .map_err(|e| Error::Auth(e.to_string()))?;
    open_browser(url.as_str())?;

    let code = tokio::time::timeout(SIGN_IN_TIMEOUT, wait_for_code(&listener, &state))
        .await
        .map_err(|_| Error::Auth("timed out waiting for the browser".into()))??;

    let mut form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code),
        ("client_id", client.client_id.clone()),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
    ];
    if let Some(secret) = &client.client_secret {
        form.push(("client_secret", secret.clone()));
    }
    let resp = token_request(&reqwest::Client::new(), &form).await?;
    Ok(Token {
        access_token: resp.access_token,
        refresh_token: resp.refresh_token,
        expires_at_ms: now_ms() + resp.expires_in.unwrap_or(3600) * 1000,
    })
}

/// Accepts connections on the loopback listener until Google redirects back
/// with an authorization code (or an error).
async fn wait_for_code(listener: &TcpListener, expected_state: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let mut len = 0;
        while len < buf.len() {
            let n = stream.read(&mut buf[len..]).await?;
            if n == 0 {
                break;
            }
            len += n;
            if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        let request = String::from_utf8_lossy(&buf[..len]);
        let target = request.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/");
        let params: HashMap<String, String> = url::Url::parse(&format!("http://localhost{target}"))
            .map(|u| u.query_pairs().into_owned().collect())
            .unwrap_or_default();

        if !params.contains_key("code") && !params.contains_key("error") {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            continue;
        }
        let result = if let Some(error) = params.get("error") {
            Err(Error::Auth(format!("Google returned \"{error}\"")))
        } else if params.get("state").map(String::as_str) != Some(expected_state) {
            Err(Error::Auth("the response did not match this sign-in attempt".into()))
        } else {
            Ok(params["code"].clone())
        };
        let page = callback_page(result.is_ok());
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
        return result;
    }
}

fn callback_page(ok: bool) -> String {
    let (title, body) = if ok {
        ("Connected", "Asteroid is now connected to Google Drive. You can close this tab.")
    } else {
        ("Sign-in failed", "Return to Asteroid and try again.")
    };
    format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title><style>body{{font:15px -apple-system,BlinkMacSystemFont,'SF Pro Text',system-ui,sans-serif;display:grid;place-items:center;height:100vh;margin:0;background:#f5f5f7;color:#1d1d1f}}div{{background:#fff;padding:32px 40px;border-radius:18px;box-shadow:0 8px 30px rgba(0,0,0,.08);text-align:center}}h1{{font-size:22px;margin:0 0 8px}}p{{color:#6e6e73;margin:0}}@media(prefers-color-scheme:dark){{body{{background:#000;color:#f5f5f7}}div{{background:#1c1c1e}}p{{color:#98989d}}}}</style><div><h1>{title}</h1><p>{body}</p></div>"
    )
}

async fn token_request(http: &reqwest::Client, form: &[(&str, String)]) -> Result<TokenResponse> {
    let resp = http.post(TOKEN_URL).form(form).send().await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();
        let message = body
            .get("error_description")
            .or_else(|| body.get("error"))
            .and_then(|v| v.as_str())
            .unwrap_or(status.as_str())
            .to_string();
        return Err(Error::Auth(message));
    }
    Ok(resp.json().await?)
}

/// An authenticated Drive client. Refreshes its access token as needed;
/// callers should persist [`Drive::token`] after use.
pub struct Drive {
    http: reqwest::Client,
    client: OAuthClient,
    token: Token,
}

impl Drive {
    pub fn new(client: OAuthClient, token: Token) -> Self {
        Self { http: reqwest::Client::new(), client, token }
    }

    pub fn token(&self) -> &Token {
        &self.token
    }

    async fn refresh(&mut self) -> Result<()> {
        let refresh_token = self
            .token
            .refresh_token
            .clone()
            .ok_or_else(|| Error::Auth("session expired; reconnect Google Drive".into()))?;
        let mut form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token),
            ("client_id", self.client.client_id.clone()),
        ];
        if let Some(secret) = &self.client.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        let resp = token_request(&self.http, &form).await?;
        self.token.access_token = resp.access_token;
        self.token.expires_at_ms = now_ms() + resp.expires_in.unwrap_or(3600) * 1000;
        if resp.refresh_token.is_some() {
            self.token.refresh_token = resp.refresh_token;
        }
        Ok(())
    }

    /// Sends a request built by `build`, refreshing the token on 401 and
    /// backing off on rate limits and transient server errors.
    async fn send<F>(&mut self, build: F) -> Result<Response>
    where
        F: Fn(&reqwest::Client) -> RequestBuilder + Send + Sync,
    {
        if self.token.expires_at_ms - now_ms() < 60_000 {
            self.refresh().await?;
        }
        let mut refreshed = false;
        let mut delay = Duration::from_millis(500);
        for attempt in 0..5 {
            let resp = build(&self.http).bearer_auth(&self.token.access_token).send().await?;
            let status = resp.status();
            if status.is_success() {
                return Ok(resp);
            }
            if status == StatusCode::UNAUTHORIZED && !refreshed {
                self.refresh().await?;
                refreshed = true;
                continue;
            }
            let retryable = status == StatusCode::TOO_MANY_REQUESTS
                || status.is_server_error()
                || status == StatusCode::FORBIDDEN && is_rate_limited(&resp);
            if retryable && attempt < 4 {
                tokio::time::sleep(delay).await;
                delay *= 2;
                continue;
            }
            return Err(api_error(resp).await);
        }
        unreachable!("the final attempt always returns")
    }

    pub async fn user_email(&mut self) -> Result<String> {
        let resp = self.send(|h| h.get(format!("{API}/about")).query(&[("fields", "user(emailAddress)")])).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body["user"]["emailAddress"].as_str().unwrap_or_default().to_string())
    }

    async fn children(&mut self, parent: &str, folders_only: bool) -> Result<Vec<DriveFile>> {
        let mut q = format!("'{parent}' in parents and trashed = false");
        if folders_only {
            q.push_str(&format!(" and mimeType = '{FOLDER_MIME}'"));
        }
        let fields = format!("nextPageToken,files({FILE_FIELDS})");
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        loop {
            let token = page.clone();
            let resp = self
                .send(|h| {
                    let mut req = h.get(format!("{API}/files")).query(&[
                        ("q", q.as_str()),
                        ("fields", fields.as_str()),
                        ("pageSize", "1000"),
                        ("orderBy", "name"),
                        ("supportsAllDrives", "true"),
                        ("includeItemsFromAllDrives", "true"),
                    ]);
                    if let Some(t) = &token {
                        req = req.query(&[("pageToken", t.as_str())]);
                    }
                    req
                })
                .await?;
            let list: FileList = resp.json().await?;
            out.extend(list.files);
            match list.next_page_token {
                Some(t) => page = Some(t),
                None => return Ok(out),
            }
        }
    }

    /// Lists folders directly inside `parent` (`"root"` is My Drive).
    pub async fn list_folders(&mut self, parent: &str) -> Result<Vec<DriveFolder>> {
        let files = self.children(parent, true).await?;
        Ok(files
            .into_iter()
            .filter(|f| !f.name.starts_with('.'))
            .map(|f| DriveFolder { id: f.id, name: f.name })
            .collect())
    }

    pub async fn create_folder(&mut self, name: &str, parent: &str) -> Result<DriveFolder> {
        let body = serde_json::json!({ "name": name, "mimeType": FOLDER_MIME, "parents": [parent] });
        let resp = self
            .send(|h| {
                h.post(format!("{API}/files"))
                    .query(&[("fields", "id,name"), ("supportsAllDrives", "true")])
                    .json(&body)
            })
            .await?;
        let file: serde_json::Value = resp.json().await?;
        Ok(DriveFolder {
            id: file["id"].as_str().unwrap_or_default().to_string(),
            name: file["name"].as_str().unwrap_or(name).to_string(),
        })
    }
}

fn is_rate_limited(resp: &Response) -> bool {
    // Drive reports per-user rate limits as 403 with this reason; the body is
    // not available without consuming the response, so use the header hint.
    resp.headers().get("x-ratelimit-reason").is_some()
}

async fn api_error(resp: Response) -> Error {
    let status = resp.status().as_u16();
    let body: serde_json::Value = resp.json().await.unwrap_or_default();
    let message = body["error"]["message"].as_str().unwrap_or("request failed").to_string();
    Error::Api { status, message }
}

/// One vault folder on Drive, usable as a sync [`Remote`].
pub struct DriveVault<'a> {
    drive: &'a mut Drive,
    root_id: String,
    /// Vault-relative folder path -> Drive folder id (`""` is the root).
    folders: HashMap<String, String>,
}

impl<'a> DriveVault<'a> {
    pub fn new(drive: &'a mut Drive, root_id: impl Into<String>) -> Self {
        let root_id = root_id.into();
        let folders = HashMap::from([(String::new(), root_id.clone())]);
        Self { drive, root_id, folders }
    }

    async fn ensure_folder(&mut self, rel: &str) -> Result<String> {
        if let Some(id) = self.folders.get(rel) {
            return Ok(id.clone());
        }
        let mut current = String::new();
        let mut parent_id = self.root_id.clone();
        for part in rel.split('/').filter(|p| !p.is_empty()) {
            current = asteroid_core::vault::join(&current, part);
            parent_id = match self.folders.get(&current) {
                Some(id) => id.clone(),
                None => {
                    let folder = self.drive.create_folder(part, &parent_id).await?;
                    self.folders.insert(current.clone(), folder.id.clone());
                    folder.id
                }
            };
        }
        Ok(parent_id)
    }

    fn entry(path: String, file: serde_json::Value) -> Result<RemoteEntry> {
        let md5 = file["md5Checksum"].as_str().unwrap_or_default().to_string();
        let id = file["id"].as_str().ok_or_else(|| Error::Api { status: 200, message: "missing file id".into() })?;
        Ok(RemoteEntry { id: id.to_string(), path, md5 })
    }
}

impl Remote for DriveVault<'_> {
    async fn list(&mut self) -> Result<Vec<RemoteEntry>> {
        self.folders = HashMap::from([(String::new(), self.root_id.clone())]);
        let mut found: HashMap<String, (RemoteEntry, String)> = HashMap::new();
        let mut queue = VecDeque::from([(String::new(), self.root_id.clone())]);
        while let Some((folder_path, folder_id)) = queue.pop_front() {
            for file in self.drive.children(&folder_id, false).await? {
                // Hidden names and names that cannot be paths are not synced.
                if file.name.starts_with('.') || file.name.contains('/') {
                    continue;
                }
                let path = asteroid_core::vault::join(&folder_path, &file.name);
                if file.mime_type == FOLDER_MIME {
                    self.folders.insert(path.clone(), file.id.clone());
                    queue.push_back((path, file.id));
                    continue;
                }
                // Google Docs/Sheets have no binary content to sync.
                let Some(md5) = file.md5_checksum else { continue };
                let modified = file.modified_time.unwrap_or_default();
                // Drive allows duplicate names; keep the most recent one.
                let newer = found.get(&path).is_none_or(|(_, m)| modified > *m);
                if newer {
                    found.insert(path.clone(), (RemoteEntry { id: file.id, path, md5 }, modified));
                }
            }
        }
        Ok(found.into_values().map(|(e, _)| e).collect())
    }

    async fn download(&mut self, entry: &RemoteEntry) -> Result<Vec<u8>> {
        let id = entry.id.clone();
        let resp = self
            .drive
            .send(|h| h.get(format!("{API}/files/{id}")).query(&[("alt", "media"), ("supportsAllDrives", "true")]))
            .await?;
        Ok(resp.bytes().await?.to_vec())
    }

    async fn upload(&mut self, path: &str, data: Vec<u8>, existing: Option<&RemoteEntry>) -> Result<RemoteEntry> {
        let fields = FILE_FIELDS;
        let resp = match existing {
            Some(entry) => {
                let id = entry.id.clone();
                self.drive
                    .send(|h| {
                        h.patch(format!("{UPLOAD_API}/files/{id}"))
                            .query(&[("uploadType", "media"), ("fields", fields), ("supportsAllDrives", "true")])
                            .header("Content-Type", "application/octet-stream")
                            .body(data.clone())
                    })
                    .await?
            }
            None => {
                let parent = self.ensure_folder(asteroid_core::vault::parent_of(path)).await?;
                let name = asteroid_core::vault::file_name_of(path);
                let meta = serde_json::json!({ "name": name, "parents": [parent] }).to_string();
                let mut nonce = [0u8; 12];
                getrandom::fill(&mut nonce).map_err(|e| Error::Auth(e.to_string()))?;
                let boundary = format!("asteroid-{}", hex(&nonce));
                let mut body = Vec::with_capacity(data.len() + meta.len() + 256);
                body.extend_from_slice(
                    format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes(),
                );
                body.extend_from_slice(&data);
                body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
                let content_type = format!("multipart/related; boundary={boundary}");
                self.drive
                    .send(|h| {
                        h.post(format!("{UPLOAD_API}/files"))
                            .query(&[("uploadType", "multipart"), ("fields", fields), ("supportsAllDrives", "true")])
                            .header("Content-Type", content_type.as_str())
                            .body(body.clone())
                    })
                    .await?
            }
        };
        let file: serde_json::Value = resp.json().await?;
        Self::entry(path.to_string(), file)
    }

    async fn delete(&mut self, entry: &RemoteEntry) -> Result<()> {
        // Move to Drive's trash rather than deleting permanently.
        let id = entry.id.clone();
        let body = serde_json::json!({ "trashed": true });
        self.drive
            .send(|h| {
                h.patch(format!("{API}/files/{id}"))
                    .query(&[("supportsAllDrives", "true")])
                    .json(&body)
            })
            .await?;
        Ok(())
    }
}
