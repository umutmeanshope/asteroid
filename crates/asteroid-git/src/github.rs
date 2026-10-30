//! GitHub sign-in through the OAuth device flow, plus the few REST calls the
//! repository picker needs. The device flow needs only a client ID (no secret
//! and no redirect), so the same code works on desktop and mobile.

use std::time::Duration;

use reqwest::{Client, RequestBuilder, StatusCode};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const API: &str = "https://api.github.com";
/// Read/write access to the user's repositories, including private ones.
const SCOPE: &str = "repo";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    #[serde(alias = "device_code")]
    pub device_code: String,
    #[serde(alias = "user_code")]
    pub user_code: String,
    #[serde(alias = "verification_uri")]
    pub verification_uri: String,
    #[serde(alias = "expires_in")]
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubUser {
    pub login: String,
    pub name: Option<String>,
    pub id: u64,
}

impl GithubUser {
    /// GitHub's private commit address, which still links commits to the account.
    pub fn noreply_email(&self) -> String {
        format!("{}+{}@users.noreply.github.com", self.id, self.login)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    #[serde(alias = "full_name")]
    pub full_name: String,
    pub name: String,
    pub private: bool,
    #[serde(alias = "clone_url")]
    pub clone_url: String,
    #[serde(alias = "default_branch")]
    pub default_branch: String,
    pub description: Option<String>,
    #[serde(alias = "updated_at")]
    pub updated_at: Option<String>,
}

fn http() -> Client {
    Client::builder()
        .user_agent(concat!("Asteroid/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("static client config")
}

/// Step 1: get a code for the user to enter at `verification_uri`.
pub async fn start_device_flow(client_id: &str) -> Result<DeviceCode> {
    let resp = http()
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .form(&[("client_id", client_id), ("scope", SCOPE)])
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status();
        return Err(Error::Auth(format!(
            "GitHub refused the sign-in request ({status}). Check the client ID and that Device Flow is enabled for the OAuth app."
        )));
    }
    let value: serde_json::Value = resp.json().await?;
    if let Some(error) = value["error"].as_str() {
        return Err(Error::Auth(device_error(error, &value)));
    }
    Ok(serde_json::from_value(value)?)
}

/// Step 2: poll until the user approves (or denies, or the code expires).
/// `cancelled` is checked between polls so a closed dialog stops waiting.
pub async fn poll_device_flow(client_id: &str, code: &DeviceCode, cancelled: impl Fn() -> bool) -> Result<String> {
    let http = http();
    let mut interval = code.interval.max(1);
    let deadline = std::time::Instant::now() + Duration::from_secs(code.expires_in);
    loop {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        if cancelled() {
            return Err(Error::Cancelled);
        }
        if std::time::Instant::now() > deadline {
            return Err(Error::Auth("The sign-in code expired. Try connecting again.".into()));
        }
        let value: serde_json::Value = http
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", client_id),
                ("device_code", code.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await?
            .json()
            .await?;
        if let Some(token) = value["access_token"].as_str() {
            return Ok(token.to_string());
        }
        match value["error"].as_str() {
            Some("authorization_pending") => {}
            // GitHub asks for a longer interval; it says by how much.
            Some("slow_down") => interval = value["interval"].as_u64().unwrap_or(interval + 5),
            Some(error) => return Err(Error::Auth(device_error(error, &value))),
            None => return Err(Error::Auth("Unexpected response from GitHub.".into())),
        }
    }
}

fn device_error(error: &str, value: &serde_json::Value) -> String {
    match error {
        "access_denied" => "Sign-in was cancelled on GitHub.".into(),
        "expired_token" => "The sign-in code expired. Try connecting again.".into(),
        "device_flow_disabled" => "Device Flow is disabled for this OAuth app. Enable it in the app's GitHub settings.".into(),
        "incorrect_client_credentials" => "GitHub doesn't recognise this client ID.".into(),
        _ => value["error_description"].as_str().unwrap_or(error).to_string(),
    }
}

/// An authenticated REST client.
pub struct Github {
    http: Client,
    token: String,
}

impl Github {
    pub fn new(token: impl Into<String>) -> Self {
        Self { http: http(), token: token.into() }
    }

    async fn send(&self, build: impl FnOnce(&Client) -> RequestBuilder) -> Result<reqwest::Response> {
        let resp = build(&self.http)
            .bearer_auth(&self.token)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await?;
        match resp.status() {
            s if s.is_success() => Ok(resp),
            StatusCode::UNAUTHORIZED => Err(Error::Auth("GitHub sign-in expired. Reconnect GitHub in Settings › Sync.".into())),
            status => {
                let body: serde_json::Value = resp.json().await.unwrap_or_default();
                let detail = body["errors"][0]["message"].as_str().or(body["message"].as_str()).unwrap_or("request failed");
                Err(Error::Api(format!("GitHub: {detail} ({status})")))
            }
        }
    }

    pub async fn user(&self) -> Result<GithubUser> {
        Ok(self.send(|h| h.get(format!("{API}/user"))).await?.json().await?)
    }

    /// Repositories the user owns or collaborates on, most recently updated first.
    pub async fn repos(&self) -> Result<Vec<Repo>> {
        let mut all = Vec::new();
        for page in 1..=10 {
            let batch: Vec<Repo> = self
                .send(|h| {
                    h.get(format!("{API}/user/repos")).query(&[
                        ("per_page", "100"),
                        ("sort", "updated"),
                        ("affiliation", "owner,collaborator,organization_member"),
                        ("page", &page.to_string()),
                    ])
                })
                .await?
                .json()
                .await?;
            let done = batch.len() < 100;
            all.extend(batch);
            if done {
                break;
            }
        }
        Ok(all)
    }

    pub async fn create_repo(&self, name: &str, private: bool) -> Result<Repo> {
        let body = serde_json::json!({
            "name": name,
            "private": private,
            "description": "Notes synced by Asteroid",
            "auto_init": false,
        });
        Ok(self.send(|h| h.post(format!("{API}/user/repos")).json(&body)).await?.json().await?)
    }
}

/// Whether `url` points at github.com, i.e. whether the GitHub token applies.
pub fn is_github_url(url: &str) -> bool {
    url.strip_prefix("https://")
        .and_then(|rest| rest.split('/').next())
        .is_some_and(|host| host.eq_ignore_ascii_case("github.com") || host.ends_with("@github.com"))
}
