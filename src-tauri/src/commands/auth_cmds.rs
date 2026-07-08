use std::sync::Arc;

use tauri::State;

use std::sync::atomic::Ordering;

use crate::api::ApiClient;
use crate::auth::{AuthManager, LoginResult};
use crate::error::AppResult;
use crate::state::AppState;

/// `Login` — Cognito USER_PASSWORD_AUTH. Returns `{ requiresNewPassword }`.
#[tauri::command]
pub async fn login(
    email: String,
    password: String,
    auth: State<'_, Arc<AuthManager>>,
) -> AppResult<LoginResult> {
    auth.login(&email, &password).await
}

/// `SetNewPassword` — completes the NEW_PASSWORD_REQUIRED challenge. The
/// challenge session is held in backend state and never round-trips the webview.
#[tauri::command]
pub async fn set_new_password(
    new_password: String,
    auth: State<'_, Arc<AuthManager>>,
) -> AppResult<()> {
    auth.complete_new_password(&new_password).await
}

/// `Logout` — stops the monitor, clears tokens (memory + keyring) and the cached
/// org settings so the next user on a shared machine doesn't inherit the prior
/// tenant's flags.
#[tauri::command]
pub async fn logout(
    auth: State<'_, Arc<AuthManager>>,
    api: State<'_, ApiClient>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    if let Some(monitor) = state.monitor.lock().unwrap().take() {
        monitor.stop();
    }
    state.signed_in.store(false, Ordering::SeqCst);
    auth.logout().await;
    api.clear_settings_cache();
    Ok(())
}

/// Cheap session-presence check for the frontend's initial auth gate.
#[tauri::command]
pub async fn is_authenticated(auth: State<'_, Arc<AuthManager>>) -> AppResult<bool> {
    Ok(auth.is_authenticated().await)
}
