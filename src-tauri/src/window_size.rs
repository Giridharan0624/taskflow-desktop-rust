//! Persist + restore the main window's size.
//!
//! A tiny JSON file under the app-data dir. This is the one piece of on-disk
//! state that exists before the `queue` module (M5); when queue lands it will
//! absorb this and the window-size store will move there for consistency.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalSize};

const FILE: &str = "window.json";

#[derive(Serialize, Deserialize)]
struct WindowSize {
    width: u32,
    height: u32,
}

fn path(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(FILE))
}

/// Persist the given size (best-effort; failures are logged, not fatal).
pub fn save(app: &AppHandle, width: u32, height: u32) {
    let Some(path) = path(app) else { return };
    match serde_json::to_vec(&WindowSize { width, height }) {
        Ok(bytes) => {
            if let Err(e) = std::fs::write(&path, bytes) {
                tracing::warn!(error = %e, "failed to persist window size");
            }
        }
        Err(_) => {}
    }
}

/// Apply the persisted size to the main window at startup, if any.
pub fn restore(app: &AppHandle) {
    let Some(path) = path(app) else { return };
    let Ok(bytes) = std::fs::read(&path) else { return };
    let Ok(size) = serde_json::from_slice::<WindowSize>(&bytes) else {
        return;
    };
    if let Some(w) = app.get_webview_window("main") {
        // Physical px on both save and restore keeps the round-trip exact.
        let _ = w.set_size(PhysicalSize::new(size.width, size.height));
    }
}
