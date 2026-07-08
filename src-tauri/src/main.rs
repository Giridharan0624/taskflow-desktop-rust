// Prevent a second console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app;
mod auth;
mod commands;
mod config;
mod error;
mod events;
mod lifecycle;
mod monitor;
mod queue;
mod state;
mod tray;
mod updater;
mod window_size;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::Manager;

use api::ApiClient;
use auth::AuthManager;
use state::AppState;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Resolve + validate configuration up front. `config()` panics if a
    // required field is missing (contract: fail fast at startup), matching the
    // Go app's `missingFields` panic in config.go.
    let cfg = config::config();
    tracing::info!(api_url = %cfg.api_url, "configuration loaded");

    let auth = Arc::new(AuthManager::new(&cfg.cognito_region, &cfg.cognito_client_id));
    let api = ApiClient::new(cfg.api_url.clone(), auth.clone());

    tauri::Builder::default()
        // Second launch focuses the existing window instead of starting a new
        // instance (M6 also hangs the single-install guard off this).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(AppState::default())
        .manage(auth)
        .manage(api)
        .invoke_handler(tauri::generate_handler![
            commands::system_cmds::get_app_version,
            commands::system_cmds::get_web_dashboard_url,
            commands::system_cmds::show_window,
            commands::system_cmds::show_tray_notification,
            commands::system_cmds::save_window_size,
            commands::system_cmds::get_idle_seconds,
            commands::system_cmds::get_session_info,
            commands::system_cmds::clear_local_cache,
            commands::system_cmds::set_auto_start,
            commands::system_cmds::get_auto_start,
            commands::update_cmds::check_for_update,
            commands::update_cmds::install_update,
            commands::auth_cmds::login,
            commands::auth_cmds::set_new_password,
            commands::auth_cmds::logout,
            commands::auth_cmds::is_authenticated,
            commands::data_cmds::get_current_user,
            commands::data_cmds::get_my_tasks,
            commands::timer_cmds::sign_in,
            commands::timer_cmds::sign_out,
            commands::timer_cmds::get_my_attendance,
        ])
        // Intercept window close into minimize-to-tray, unless a real quit is
        // already in progress.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let quitting = window
                    .state::<AppState>()
                    .quitting
                    .load(Ordering::SeqCst);
                if !quitting {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(app::setup)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
