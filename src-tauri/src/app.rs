use std::sync::Arc;

use tauri::{App, Manager};

use crate::auth::AuthManager;
use crate::{lifecycle, tray, window_size};

/// Tauri `setup` hook — runs once after the app is built, before the event
/// loop starts.
///
/// M1: restore any persisted session. M3: build the tray, restore the window
/// size, and install OS-signal handlers for clean shutdown. M6 will also spawn
/// the background GitHub update check here.
pub fn setup(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        "TaskFlow desktop started"
    );

    // Restore session off the main thread so window show is never blocked.
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let auth = handle.state::<Arc<AuthManager>>();
        auth.restore().await;
    });

    window_size::restore(app.handle());
    tray::build(app.handle())?;
    lifecycle::install_signal_handlers(app.handle());

    // Background update check — never blocks window show. Emits update:available
    // if a newer signed release exists (no-op in debug builds).
    let update_handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        match crate::updater::check_for_update().await {
            Ok(info) if info.available => {
                crate::events::emit_update_available(&update_handle, &info);
            }
            Ok(_) => {}
            Err(e) => tracing::debug!(error = %e, "startup update check failed"),
        }
    });

    Ok(())
}
