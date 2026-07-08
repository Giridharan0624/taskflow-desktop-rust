//! System tray icon, menu, and status tooltip.
//!
//! Uses Tauri v2's built-in tray (the `tray-icon` feature). Menu events and
//! left-click both run on Tauri's main event-loop thread, which sidesteps the
//! Win32 `Shell_NotifyIcon` thread-affinity trap the raw Go implementation had
//! to manage by hand.
//!
//! NOTE: the dynamic overlay dot (green when tracking) is represented here as a
//! tooltip status only. Pixel-compositing a badge onto the icon is a small
//! follow-up once the base build is confirmed on each platform.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::config;

const TRAY_ID: &str = "main";

/// Build and register the tray icon. Called once from the setup hook.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open TaskFlow", true, None::<&str>)?;
    let dashboard = MenuItem::with_id(app, "dashboard", "Open dashboard", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &dashboard, &sep, &quit])?;

    let icon = app
        .default_window_icon()
        .cloned()
        .expect("bundle icon configured in tauri.conf.json");

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("TaskFlow — idle")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main(app),
            "dashboard" => open_dashboard(app),
            "quit" => crate::lifecycle::request_quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left-click restores the window (matches the Go tray behavior).
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Reflect timer state in the tray tooltip. Called whenever attendance changes.
pub fn update_status(app: &AppHandle, signed_in: bool) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tip = if signed_in {
            "TaskFlow — tracking"
        } else {
            "TaskFlow — idle"
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}

/// Restore + focus the main window (`ShowWindow`).
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Open the configured web dashboard in the user's default browser.
fn open_dashboard(app: &AppHandle) {
    let Some(url) = config::config().web_dashboard_url.clone() else {
        return;
    };
    open_url(&url);
    // Keep the app in view after launching the browser.
    let _ = app;
}

/// Minimal per-OS "open in default browser". Avoids an extra crate; the URL is
/// already sanitized to http(s) with no userinfo by `config.rs`.
fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();

    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();

    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}
