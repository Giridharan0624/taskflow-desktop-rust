//! Tauri command layer.
//!
//! Every `#[command]` is a thin wrapper: read managed state, (for post-auth
//! commands) call the subsystem, map to `AppError`. No business logic lives
//! here so the IPC surface stays auditable against the Wails binding contract.

// Public so `generate_handler!` in main.rs can reach both the command fns and
// the hidden `__cmd__*` helpers the `#[tauri::command]` macro generates beside
// them (a named `pub use` would re-export only the fn, not the helpers).
pub mod auth_cmds;
pub mod data_cmds;
pub mod system_cmds;
pub mod timer_cmds;
pub mod update_cmds;

use tauri::AppHandle;

use crate::api::ApiClient;
use crate::error::{AppError, AppResult};

/// Wrap an API result so a 401 (`Unauthorized`) tears down the session and
/// notifies the UI via `auth:expired`, exactly once, at the command boundary.
pub(crate) async fn guard<T>(app: &AppHandle, api: &ApiClient, result: AppResult<T>) -> AppResult<T> {
    if matches!(result, Err(AppError::Unauthorized)) {
        api.auth().logout().await;
        api.clear_settings_cache();
        crate::events::emit_auth_expired(app);
    }
    result
}
