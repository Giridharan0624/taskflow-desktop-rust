//! Self-updater: GitHub Releases poll → sha256 + ed25519 verify → launch the
//! platform installer.
//!
//! Security note (fixes the Go bug): the Go app treated an empty release public
//! key as "unsigned mode" and silently skipped verification, so a compromised
//! release could swap both the binary and its SHA256SUMS. Here, **release builds
//! refuse to install when no public key is configured**.

mod install;
mod verify;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::error::{AppError, AppResult};

const GITHUB_REPO: &str = "Giridharan0624/taskflow-desktop";
const CHECKSUM_ASSET: &str = "SHA256SUMS";
const SIGNATURE_ASSET: &str = "SHA256SUMS.sig";
const USER_AGENT: &str = "taskflow-desktop-updater";

/// Guards against two concurrent installs (double-click / tray + UI).
static INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    pub current_version: String,
    pub download_url: String,
    pub release_notes: String,
    pub file_name: String,
    pub size: u64,
    #[serde(skip)]
    checksum_url: String,
    #[serde(skip)]
    signature_url: String,
}

impl UpdateInfo {
    fn none(current: &str, latest: &str) -> Self {
        UpdateInfo {
            available: false,
            version: latest.to_string(),
            current_version: current.to_string(),
            download_url: String::new(),
            release_notes: String::new(),
            file_name: String::new(),
            size: 0,
            checksum_url: String::new(),
            signature_url: String::new(),
        }
    }
}

#[derive(Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

pub fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn client(timeout: Duration) -> AppResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .build()
        .map_err(|e| AppError::Message(e.to_string()))
}

fn net(e: reqwest::Error) -> AppError {
    AppError::Network(e.to_string())
}

/// Query GitHub for the latest release and decide whether it's newer. Debug
/// builds always report "no update" so local dev isn't nagged.
pub async fn check_for_update() -> AppResult<UpdateInfo> {
    let current = current_version();
    if cfg!(debug_assertions) {
        return Ok(UpdateInfo::none(&current, &current));
    }
    // Never advertise an update we would then refuse to install: without a
    // release key, `install_update` fail-closes on unsigned releases, so
    // surfacing the banner would dead-end the user. Stay quiet until signing
    // is configured (see RELEASE.md).
    if !verify::pubkey_present() {
        tracing::warn!(
            "update check skipped: no release public key configured — unsigned updates are refused"
        );
        return Ok(UpdateInfo::none(&current, &current));
    }

    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let resp = client(Duration::from_secs(30))?
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(net)?;
    if !resp.status().is_success() {
        return Ok(UpdateInfo::none(&current, &current));
    }
    let release: Release = resp.json().await.map_err(|e| AppError::Message(e.to_string()))?;
    let latest = release.tag_name.trim_start_matches('v').to_string();

    if !is_newer(&latest, &current) {
        return Ok(UpdateInfo::none(&current, &latest));
    }

    let (Some(asset), Some(checksum)) = (
        install::find_platform_asset(&release.assets),
        release.assets.iter().find(|a| a.name == CHECKSUM_ASSET),
    ) else {
        // No installer for this platform (or no checksums) — nothing to offer.
        return Ok(UpdateInfo::none(&current, &latest));
    };
    let signature = release.assets.iter().find(|a| a.name == SIGNATURE_ASSET);

    Ok(UpdateInfo {
        available: true,
        version: latest,
        current_version: current,
        download_url: asset.browser_download_url.clone(),
        release_notes: release.body.clone(),
        file_name: asset.name.clone(),
        size: asset.size,
        checksum_url: checksum.browser_download_url.clone(),
        signature_url: signature
            .map(|s| s.browser_download_url.clone())
            .unwrap_or_default(),
    })
}

/// Download, verify, and launch the installer, then exit. Single-flight.
pub async fn install_update(app: &AppHandle) -> AppResult<()> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err(AppError::Message("An update is already installing.".into()));
    }
    let result = do_install(app).await;
    if result.is_err() {
        INSTALLING.store(false, Ordering::SeqCst);
    }
    result
}

async fn do_install(app: &AppHandle) -> AppResult<()> {
    let info = check_for_update().await?;
    if !info.available {
        return Err(AppError::Message("No update available.".into()));
    }

    let http = client(Duration::from_secs(300))?;
    let asset = http
        .get(&info.download_url)
        .send()
        .await
        .map_err(net)?
        .bytes()
        .await
        .map_err(net)?;
    let sums = http
        .get(&info.checksum_url)
        .send()
        .await
        .map_err(net)?
        .text()
        .await
        .map_err(net)?;

    // 1) Signature over SHA256SUMS — the anchor of trust.
    if verify::pubkey_present() {
        if info.signature_url.is_empty() {
            return Err(AppError::Message(
                "release is signed-mode but no SHA256SUMS.sig was published.".into(),
            ));
        }
        let sig = http
            .get(&info.signature_url)
            .send()
            .await
            .map_err(net)?
            .bytes()
            .await
            .map_err(net)?;
        verify::verify_signature(sums.as_bytes(), &sig).map_err(AppError::Message)?;
        tracing::info!("SHA256SUMS signature verified against release key");
    } else if !cfg!(debug_assertions) {
        // The fix: never install an unsigned update in a release build.
        return Err(AppError::Message(
            "refusing update: no release public key configured (unsigned releases are not allowed)."
                .into(),
        ));
    }

    // 2) Asset checksum against the (now trusted) SHA256SUMS.
    let expected = verify::expected_hash(&sums, &info.file_name)
        .ok_or_else(|| AppError::Message("no checksum entry for the installer.".into()))?;
    let actual = verify::sha256_hex(&asset);
    if actual != expected {
        return Err(AppError::Message(
            "installer checksum mismatch — refusing to install.".into(),
        ));
    }

    // 3) Stage + launch.
    let path = std::env::temp_dir().join(&info.file_name);
    std::fs::write(&path, &asset)
        .map_err(|e| AppError::Message(format!("failed to write installer: {e}")))?;

    match install::launch(&path) {
        Ok(()) => {
            app.exit(0);
            Ok(())
        }
        Err(install::InstallError::PackageManaged) => {
            crate::events::emit_update_package_managed(
                app,
                &info.version,
                "Please update via your system package manager.",
            );
            // Not an error the UI should show as failure — reset the guard.
            INSTALLING.store(false, Ordering::SeqCst);
            Ok(())
        }
        Err(install::InstallError::Other(m)) => Err(AppError::Message(m)),
    }
}

/// True when `latest` is a strictly newer semver than `current`.
fn is_newer(latest: &str, current: &str) -> bool {
    match (semver::Version::parse(latest), semver::Version::parse(current)) {
        (Ok(l), Ok(c)) => l > c,
        _ => false,
    }
}
