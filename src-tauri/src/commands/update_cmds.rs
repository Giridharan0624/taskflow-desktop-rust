use tauri::AppHandle;

use crate::error::AppResult;
use crate::updater::{self, UpdateInfo};

/// `CheckForUpdate` — query GitHub for a newer release.
#[tauri::command]
pub async fn check_for_update() -> AppResult<UpdateInfo> {
    updater::check_for_update().await
}

/// `InstallUpdate` — download, verify (sha256 + ed25519), launch installer, quit.
/// Takes no args by design: the URLs never cross the IPC boundary.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> AppResult<()> {
    updater::install_update(&app).await
}
