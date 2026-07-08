//! REST API client.
//!
//! One pooled `reqwest::Client`, bearer-authenticated with the current Cognito
//! ID token (fetched via `AuthManager::valid_id_token`, which refreshes on
//! demand). A 401 maps to `AppError::Unauthorized` so the command layer can
//! wipe the session and emit `auth:expired`. Org settings are cached in memory
//! to gate the monitor loops (M4/M5): screenshots fail **closed**, activity
//! monitoring fails **open** — matching the backend's `require_feature` defaults.

mod models;

pub use models::*;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::json;

use crate::auth::AuthManager;
use crate::error::{AppError, AppResult};

pub struct ApiClient {
    http: reqwest::Client,
    /// Longer-timeout client for multi-MB S3 screenshot PUTs on slow uplinks.
    uploads: reqwest::Client,
    base_url: String,
    auth: Arc<AuthManager>,
    settings: Mutex<Option<OrgSettings>>,
}

impl ApiClient {
    pub fn new(base_url: String, auth: Arc<AuthManager>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("build api reqwest client");
        let uploads = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .expect("build upload reqwest client");
        Self {
            http,
            uploads,
            base_url: base_url.trim_end_matches('/').to_string(),
            auth,
            settings: Mutex::new(None),
        }
    }

    /// Shared auth manager (so commands can log out on 401 without a second
    /// managed-state lookup).
    pub fn auth(&self) -> &Arc<AuthManager> {
        &self.auth
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn bearer(&self) -> AppResult<String> {
        self.auth.valid_id_token().await
    }

    // --- endpoints -------------------------------------------------------

    /// `GET /users/me`.
    pub async fn get_current_user(&self) -> AppResult<User> {
        self.get_json("/users/me").await
    }

    /// `GET /users/me/tasks`.
    pub async fn get_my_tasks(&self) -> AppResult<Vec<Task>> {
        self.get_json("/users/me/tasks").await
    }

    /// `GET /attendance/me`.
    pub async fn get_my_attendance(&self) -> AppResult<Attendance> {
        self.get_json("/attendance/me").await
    }

    /// `POST /attendance/sign-in` (201). Body is snake_case with empty fields
    /// omitted — the backend wants absent, not "".
    pub async fn sign_in(&self, data: StartTimerData) -> AppResult<Attendance> {
        let mut body = serde_json::Map::new();
        if !data.task_id.is_empty() {
            body.insert("task_id".into(), json!(data.task_id));
        }
        if !data.project_id.is_empty() {
            body.insert("project_id".into(), json!(data.project_id));
        }
        if !data.task_title.is_empty() {
            body.insert("task_title".into(), json!(data.task_title));
        }
        if !data.project_name.is_empty() {
            body.insert("project_name".into(), json!(data.project_name));
        }
        if !data.description.is_empty() {
            body.insert("description".into(), json!(data.description));
        }

        let token = self.bearer().await?;
        let resp = self
            .http
            .post(self.url("/attendance/sign-in"))
            .bearer_auth(token)
            .json(&body)
            .send()
            .await
            .map_err(net_err)?;
        self.parse(resp).await
    }

    /// `PUT /attendance/sign-out` (200), empty JSON body.
    pub async fn sign_out(&self) -> AppResult<Attendance> {
        let token = self.bearer().await?;
        let resp = self
            .http
            .put(self.url("/attendance/sign-out"))
            .bearer_auth(token)
            .json(&json!({}))
            .send()
            .await
            .map_err(net_err)?;
        self.parse(resp).await
    }

    /// `GET /orgs/current` → extract `.settings`, cache it. A null settings
    /// object (brand-new org) yields empty settings so fail-closed gates still
    /// behave.
    pub async fn fetch_org_settings(&self) -> AppResult<OrgSettings> {
        #[derive(serde::Deserialize)]
        struct Envelope {
            #[serde(default)]
            settings: Option<OrgSettings>,
        }
        let env: Envelope = self.get_json("/orgs/current").await?;
        let settings = env.settings.unwrap_or_default();
        *self.settings.lock().unwrap() = Some(settings.clone());
        Ok(settings)
    }

    /// Upload a screenshot: presign → validate host → PUT to S3, returning the
    /// CDN file URL. Retries once on a 403 (expired presign).
    pub async fn upload_screenshot(&self, jpeg: &[u8], filename: &str) -> AppResult<String> {
        match self.upload_screenshot_once(jpeg, filename).await {
            Err(AppError::Message(m)) if m.contains("403") => {
                // Presign likely expired between fetch and PUT — retry once fresh.
                self.upload_screenshot_once(jpeg, filename).await
            }
            other => other,
        }
    }

    async fn upload_screenshot_once(&self, jpeg: &[u8], filename: &str) -> AppResult<String> {
        #[derive(serde::Deserialize)]
        struct Presign {
            upload_url: String,
            file_url: String,
        }

        let token = self.bearer().await?;
        let resp = self
            .http
            .get(self.url("/uploads/presign"))
            .query(&[
                ("type", "screenshot"),
                ("filename", filename),
                ("contentType", "image/jpeg"),
            ])
            .bearer_auth(token)
            .send()
            .await
            .map_err(net_err)?;
        let presign: Presign = self.parse(resp).await?;

        // Never PUT a frame of the user's screen anywhere but https S3 — a
        // compromised backend must not be able to redirect it to an attacker.
        if !presign.upload_url.starts_with("https://")
            || !presign.upload_url.contains("amazonaws.com")
        {
            return Err(AppError::Message(
                "refusing screenshot upload: untrusted upload URL".into(),
            ));
        }

        let put = self
            .uploads
            .put(&presign.upload_url)
            .header("Content-Type", "image/jpeg")
            .body(jpeg.to_vec())
            .send()
            .await
            .map_err(net_err)?;
        if put.status().is_success() {
            Ok(presign.file_url)
        } else {
            Err(AppError::Message(format!(
                "S3 upload failed {}",
                put.status().as_u16()
            )))
        }
    }

    /// `POST /activity/heartbeat` — consumed by the monitor heartbeat loop.
    pub async fn send_heartbeat(&self, payload: serde_json::Value) -> AppResult<()> {
        let token = self.bearer().await?;
        let resp = self
            .http
            .post(self.url("/activity/heartbeat"))
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await
            .map_err(net_err)?;
        let status = resp.status();
        if status.is_success() {
            Ok(())
        } else if status.as_u16() == 401 {
            Err(AppError::Unauthorized)
        } else {
            Err(parse_api_error(&resp.bytes().await.unwrap_or_default(), status.as_u16()))
        }
    }

    // --- feature gating (read from cached settings) ----------------------

    /// Screenshots gate — fail **closed** (no settings / key absent → false).
    #[allow(dead_code)]
    pub fn screenshots_enabled(&self) -> bool {
        self.settings
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|s| s.features.get("screenshots").copied())
            .unwrap_or(false)
    }

    /// Activity-monitoring gate — fail **open** (no settings / key absent → true).
    #[allow(dead_code)]
    pub fn activity_monitoring_enabled(&self) -> bool {
        match self.settings.lock().unwrap().as_ref() {
            None => true,
            Some(s) => s.features.get("activity_monitoring").copied().unwrap_or(true),
        }
    }

    /// Drop the cached settings on logout so the next tenant on a shared
    /// machine doesn't inherit the previous one's flags.
    pub fn clear_settings_cache(&self) {
        *self.settings.lock().unwrap() = None;
    }

    // --- helpers ---------------------------------------------------------

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        let token = self.bearer().await?;
        let resp = self
            .http
            .get(self.url(path))
            .bearer_auth(token)
            .send()
            .await
            .map_err(net_err)?;
        self.parse(resp).await
    }

    async fn parse<T: DeserializeOwned>(&self, resp: reqwest::Response) -> AppResult<T> {
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(net_err)?;
        if status.is_success() {
            serde_json::from_slice::<T>(&bytes)
                .map_err(|e| AppError::Message(format!("Malformed API response: {e}")))
        } else if status.as_u16() == 401 {
            Err(AppError::Unauthorized)
        } else {
            Err(parse_api_error(&bytes, status.as_u16()))
        }
    }
}

fn net_err(e: reqwest::Error) -> AppError {
    AppError::Network(format!("Network error: {e}"))
}

/// Best-effort parse of the backend error envelope (`{error:{message}}` or
/// `{message}`), falling back to a status-coded generic message.
fn parse_api_error(bytes: &[u8], status: u16) -> AppError {
    #[derive(serde::Deserialize)]
    struct Inner {
        message: Option<String>,
    }
    #[derive(serde::Deserialize)]
    struct Env {
        error: Option<Inner>,
        message: Option<String>,
    }
    if let Ok(env) = serde_json::from_slice::<Env>(bytes) {
        if let Some(m) = env.error.and_then(|e| e.message) {
            return AppError::Message(m);
        }
        if let Some(m) = env.message {
            return AppError::Message(m);
        }
    }
    AppError::Message(format!("Request failed ({status})"))
}
