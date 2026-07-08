use tauri::{AppHandle, State};

use crate::api::{ApiClient, Task, User};
use crate::commands::guard;
use crate::error::{AppError, AppResult};
use crate::queue;

/// `GetCurrentUser` — the authed user's profile. Also serves as the frontend's
/// authoritative auth probe (a 401 here means the session is dead).
#[tauri::command]
pub async fn get_current_user(app: AppHandle, api: State<'_, ApiClient>) -> AppResult<User> {
    guard(&app, &api, api.get_current_user().await).await
}

/// `GetMyTasks` — the user's assigned tasks. On success the list is cached; when
/// offline (transport failure only, not a 4xx) the cache is served so the user
/// can still pick a task. A 4xx/auth error is authoritative and not masked.
#[tauri::command]
pub async fn get_my_tasks(app: AppHandle, api: State<'_, ApiClient>) -> AppResult<Vec<Task>> {
    match guard(&app, &api, api.get_my_tasks().await).await {
        Ok(tasks) => {
            queue::save_tasks(&app, &tasks);
            Ok(tasks)
        }
        Err(AppError::Network(_)) => queue::load_tasks(&app)
            .ok_or_else(|| AppError::Network("Offline and no cached tasks available".into())),
        Err(e) => Err(e),
    }
}
