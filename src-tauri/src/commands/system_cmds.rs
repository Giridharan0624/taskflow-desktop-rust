use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

use crate::config;
use crate::error::{AppError, AppResult};
use crate::monitor::{self, SessionInfo};
use crate::{tray, window_size};

/// Build-injected app version (`GetAppVersion`).
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Configured web dashboard URL for the footer link (`GetWebDashboardURL`).
/// Empty string when unset — matches the Go binding's contract.
#[tauri::command]
pub fn get_web_dashboard_url() -> String {
    config::config()
        .web_dashboard_url
        .clone()
        .unwrap_or_default()
}

/// Restore + raise the main window (`ShowWindow`). Called from JS and the tray.
#[tauri::command]
pub fn show_window(app: AppHandle) -> AppResult<()> {
    tray::show_main(&app);
    Ok(())
}

/// Fire a native notification (`ShowTrayNotification`). The frontend gates this
/// by the user's notification setting before calling.
#[tauri::command]
pub fn show_tray_notification(app: AppHandle, title: String, message: String) -> AppResult<()> {
    app.notification()
        .builder()
        .title(title)
        .body(message)
        .show()
        .map_err(|e| AppError::Message(format!("notification failed: {e}")))?;
    Ok(())
}

/// Persist the window size (`SaveWindowSize`), debounced on the frontend.
#[tauri::command]
pub fn save_window_size(app: AppHandle, width: u32, height: u32) -> AppResult<()> {
    window_size::save(&app, width, height);
    Ok(())
}

/// Seconds since last user input (`GetIdleSeconds`) — drives the idle prompt.
/// Stateless; works whether or not the timer is running.
#[tauri::command]
pub fn get_idle_seconds() -> u64 {
    monitor::idle_seconds()
}

/// Display-server capabilities (`GetSessionInfo`) — surfaces Wayland limits etc.
#[tauri::command]
pub fn get_session_info() -> SessionInfo {
    monitor::session_info()
}

/// Wipe local caches/queues (`ClearLocalCache`). Keeps keyring tokens — the user
/// stays logged in.
#[tauri::command]
pub fn clear_local_cache(app: AppHandle) -> AppResult<()> {
    crate::queue::clear_all(&app);
    Ok(())
}

/// Enable/disable launch-at-login (`SetAutoStart`).
#[tauri::command]
pub fn set_auto_start(app: AppHandle, enabled: bool) -> AppResult<()> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| AppError::Message(format!("autostart change failed: {e}")))
}

/// Read the actual OS launch-at-login state (`GetAutoStart`).
#[tauri::command]
pub fn get_auto_start(app: AppHandle) -> AppResult<bool> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| AppError::Message(format!("autostart query failed: {e}")))
}
