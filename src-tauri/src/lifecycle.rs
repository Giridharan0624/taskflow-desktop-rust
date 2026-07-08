//! Application lifecycle: minimize-to-tray, quit, and auto-sign-out.
//!
//! The timer must not keep running after the app goes away, so every teardown
//! path — the tray Quit item, SIGTERM, and Ctrl-C — routes through
//! `auto_sign_out`, which is idempotent (run-once) and time-bounded so a slow
//! network can't hang process exit. Ordinary window-close is intercepted into
//! minimize-to-tray instead of quitting (see `main.rs` window-event handler).

use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::api::ApiClient;
use crate::state::AppState;

/// Begin a real quit: flag it (so the window handler stops intercepting),
/// auto-sign-out if the timer is running, then exit.
pub fn request_quit(app: &AppHandle) {
    app.state::<AppState>().quitting.store(true, Ordering::SeqCst);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        auto_sign_out(&app).await;
        app.exit(0);
    });
}

/// Sign the user out if the timer is active. Runs at most once (guarded by
/// `auto_signed_out`) and gives the backend at most 5s so shutdown never hangs.
pub async fn auto_sign_out(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.auto_signed_out.swap(true, Ordering::SeqCst) {
        return; // already ran on another teardown path
    }
    if !state.signed_in.load(Ordering::SeqCst) {
        return; // nothing to sign out of
    }

    let api = app.state::<ApiClient>();
    match tokio::time::timeout(Duration::from_secs(5), api.sign_out()).await {
        Ok(Ok(_)) => tracing::info!("auto sign-out on exit"),
        Ok(Err(e)) => tracing::warn!(error = %e, "auto sign-out failed"),
        Err(_) => tracing::warn!("auto sign-out timed out"),
    }
}

/// Install OS-signal handlers that trigger a clean quit. Called from setup.
pub fn install_signal_handlers(app: &AppHandle) {
    // Ctrl-C on all platforms.
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                tracing::info!("Ctrl-C received, quitting");
                auto_sign_out(&app).await;
                app.exit(0);
            }
        });
    }

    // SIGTERM (service stop / OS shutdown) on Unix.
    #[cfg(unix)]
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            use tokio::signal::unix::{signal, SignalKind};
            if let Ok(mut term) = signal(SignalKind::terminate()) {
                term.recv().await;
                tracing::info!("SIGTERM received, quitting");
                auto_sign_out(&app).await;
                app.exit(0);
            }
        });
    }
}
