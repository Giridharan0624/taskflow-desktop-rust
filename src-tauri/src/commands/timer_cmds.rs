use std::sync::atomic::Ordering;

use tauri::{AppHandle, State};

use crate::api::{ApiClient, Attendance, StartTimerData};
use crate::commands::guard;
use crate::error::AppResult;
use crate::state::AppState;
use crate::{events, monitor, tray};

/// Sync derived timer flag + tray status + the activity monitor from an
/// attendance snapshot. Starting/stopping the monitor here keeps it strictly
/// bound to timer state.
fn reflect(app: &AppHandle, state: &AppState, attendance: &Attendance) {
    let signed_in = attendance.status == "SIGNED_IN";
    state.signed_in.store(signed_in, Ordering::SeqCst);
    tray::update_status(app, signed_in);

    let mut slot = state.monitor.lock().unwrap();
    // Always stop any prior monitor first (idempotent restart).
    if let Some(old) = slot.take() {
        old.stop();
    }
    if signed_in {
        *slot = Some(monitor::start(app));
    }
}

/// `SignIn` — start the timer on a task. Refreshes the org-settings cache (so
/// the monitor loops in M4/M5 gate correctly) and broadcasts `attendance:updated`.
#[tauri::command]
pub async fn sign_in(
    data: StartTimerData,
    app: AppHandle,
    api: State<'_, ApiClient>,
    state: State<'_, AppState>,
) -> AppResult<Attendance> {
    let attendance = guard(&app, &api, api.sign_in(data).await).await?;
    // Best-effort: warm the settings cache for feature gating.
    let _ = api.fetch_org_settings().await;
    reflect(&app, &state, &attendance);
    events::emit_attendance_updated(&app, &attendance);
    Ok(attendance)
}

/// `SignOut` — stop the timer. Broadcasts `attendance:updated`.
#[tauri::command]
pub async fn sign_out(
    app: AppHandle,
    api: State<'_, ApiClient>,
    state: State<'_, AppState>,
) -> AppResult<Attendance> {
    let attendance = guard(&app, &api, api.sign_out().await).await?;
    reflect(&app, &state, &attendance);
    events::emit_attendance_updated(&app, &attendance);
    Ok(attendance)
}

/// `GetMyAttendance` — today's attendance snapshot (frontend mount + poll).
#[tauri::command]
pub async fn get_my_attendance(
    app: AppHandle,
    api: State<'_, ApiClient>,
    state: State<'_, AppState>,
) -> AppResult<Attendance> {
    let attendance = guard(&app, &api, api.get_my_attendance().await).await?;
    reflect(&app, &state, &attendance);
    Ok(attendance)
}
