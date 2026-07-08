use std::sync::OnceLock;

use serde::Deserialize;

/// Resolved runtime configuration.
///
/// Source precedence (highest first):
///   1. Build-time injected values (`build.rs` → `env!()`), i.e. the shipped
///      staging/company/prod builds.
///   2. A local `config.json` next to the executable or in the current dir,
///      for `tauri dev` / local runs (mirrors the Go app's dev fallback).
///
/// A missing *required* field (everything except `web_dashboard_url`) is fatal:
/// `config()` panics at startup, matching the Go `missingFields` behavior.
#[derive(Debug, Clone)]
pub struct Config {
    pub api_url: String,
    pub cognito_region: String,
    /// Kept for completeness; the USER_PASSWORD_AUTH + refresh flows need only
    /// region + client_id. Used once JWKS token verification is added.
    #[allow(dead_code)]
    pub cognito_pool_id: String,
    pub cognito_client_id: String,
    /// Optional; only ever an http(s) URL with no userinfo (sanitized).
    pub web_dashboard_url: Option<String>,
}

/// Shape of the optional dev `config.json`. Keys match the Go app's file
/// fallback so the same file works for both.
#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    #[serde(default)]
    api_url: Option<String>,
    #[serde(default)]
    cognito_region: Option<String>,
    #[serde(default)]
    cognito_user_pool_id: Option<String>,
    #[serde(default)]
    cognito_client_id: Option<String>,
    #[serde(default)]
    web_dashboard_url: Option<String>,
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Returns the process-wide configuration, loading + validating it on first
/// call. Panics if a required field is unresolved.
pub fn config() -> &'static Config {
    CONFIG.get_or_init(load)
}

fn load() -> Config {
    // 1. Build-time injected (empty string when not provided at build time).
    let mut api_url = env!("TASKFLOW_API_URL").to_string();
    let mut cognito_region = env!("TASKFLOW_COGNITO_REGION").to_string();
    let mut cognito_pool_id = env!("TASKFLOW_COGNITO_POOL_ID").to_string();
    let mut cognito_client_id = env!("TASKFLOW_COGNITO_CLIENT_ID").to_string();
    let mut web_dashboard_url = env!("TASKFLOW_WEB_DASHBOARD_URL").to_string();

    // 2. Dev fallback: fill only the blanks from config.json.
    if let Some(file) = load_file_config() {
        fill_if_empty(&mut api_url, file.api_url);
        fill_if_empty(&mut cognito_region, file.cognito_region);
        fill_if_empty(&mut cognito_pool_id, file.cognito_user_pool_id);
        fill_if_empty(&mut cognito_client_id, file.cognito_client_id);
        fill_if_empty(&mut web_dashboard_url, file.web_dashboard_url);
    }

    // 3. Validate required fields.
    let mut missing = Vec::new();
    if api_url.is_empty() {
        missing.push("api_url");
    }
    if cognito_region.is_empty() {
        missing.push("cognito_region");
    }
    if cognito_pool_id.is_empty() {
        missing.push("cognito_user_pool_id");
    }
    if cognito_client_id.is_empty() {
        missing.push("cognito_client_id");
    }
    if !missing.is_empty() {
        panic!(
            "missing required configuration: {}. Provide them via build-time \
             TASKFLOW_* env vars, or a config.json for dev (see config.example.json).",
            missing.join(", ")
        );
    }

    Config {
        api_url,
        cognito_region,
        cognito_pool_id,
        cognito_client_id,
        web_dashboard_url: sanitize_dashboard_url(web_dashboard_url),
    }
}

fn fill_if_empty(target: &mut String, from: Option<String>) {
    if target.is_empty() {
        if let Some(v) = from {
            *target = v;
        }
    }
}

/// Look for `config.json` next to the executable first, then in the current
/// working directory (which is the project root under `tauri dev`).
fn load_file_config() -> Option<FileConfig> {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("config.json"));
        }
    }
    candidates.push(std::path::PathBuf::from("config.json"));
    candidates.push(std::path::PathBuf::from("src-tauri/config.json"));

    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            match serde_json::from_slice::<FileConfig>(&bytes) {
                Ok(cfg) => {
                    tracing::info!(path = %path.display(), "loaded dev config.json");
                    return Some(cfg);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "invalid config.json");
                }
            }
        }
    }
    None
}

/// Accept only http/https URLs with no embedded userinfo, else drop to `None`.
/// Mirrors `isSafeDashboardURL` in the Go app.
fn sanitize_dashboard_url(url: String) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    let is_http = lower.starts_with("http://") || lower.starts_with("https://");
    let has_userinfo = url
        .split_once("://")
        .map(|(_, rest)| {
            let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
            authority.contains('@')
        })
        .unwrap_or(false);
    if is_http && !has_userinfo {
        Some(url)
    } else {
        tracing::warn!(url = %url, "ignoring unsafe web_dashboard_url");
        None
    }
}
