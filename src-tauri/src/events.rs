//! Backend → frontend events.
//!
//! Names are one-to-one with the Wails `runtime.EventsEmit` names in the Go app
//! so the React frontend can `listen(...)` for the exact same contract. Emit
//! helpers are the only way these fire — payload types are enforced here.

use tauri::{AppHandle, Emitter};

use crate::api::Attendance;

/// Payload: `Attendance`. Fired after login/sign-in/sign-out.
pub const ATTENDANCE_UPDATED: &str = "attendance:updated";

/// Payload: `string` message. Fired after repeated network failures.
pub const NETWORK_ERROR: &str = "network:error";

/// Payload: none. Fired when connectivity is restored.
pub const NETWORK_RESTORED: &str = "network:restored";

/// Payload: `UpdateInfo`. Fired by the startup update check.
pub const UPDATE_AVAILABLE: &str = "update:available";

/// Payload: `{ version, message }`. Linux package-managed install path.
pub const UPDATE_PACKAGE_MANAGED: &str = "update:package-managed";

/// Payload: none. Fired on a 401 when refresh fails; UI returns to login.
pub const AUTH_EXPIRED: &str = "auth:expired";

pub fn emit_attendance_updated(app: &AppHandle, attendance: &Attendance) {
    let _ = app.emit(ATTENDANCE_UPDATED, attendance);
}

pub fn emit_auth_expired(app: &AppHandle) {
    let _ = app.emit(AUTH_EXPIRED, ());
}

#[allow(dead_code)] // wired into the monitor/queue loops in M4
pub fn emit_network_error(app: &AppHandle, message: &str) {
    let _ = app.emit(NETWORK_ERROR, message);
}

pub fn emit_network_restored(app: &AppHandle) {
    let _ = app.emit(NETWORK_RESTORED, ());
}

pub fn emit_update_available(app: &AppHandle, info: &crate::updater::UpdateInfo) {
    let _ = app.emit(UPDATE_AVAILABLE, info);
}

pub fn emit_update_package_managed(app: &AppHandle, version: &str, message: &str) {
    let _ = app.emit(
        UPDATE_PACKAGE_MANAGED,
        serde_json::json!({ "version": version, "message": message }),
    );
}
