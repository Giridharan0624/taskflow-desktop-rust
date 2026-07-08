//! API data models.
//!
//! Field names are **snake_case** to match the backend wire format directly
//! (the Python API emits snake_case; the Go client ran a snake→camel transform
//! before unmarshalling — we skip that and deserialize the raw shape). These
//! structs are also what Tauri serializes back to the webview, so the React
//! side reads snake_case properties. `#[serde(default)]` is applied liberally
//! so a missing/optional field never fails a whole response.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Payload for `POST /attendance/sign-in`, sent from the frontend via `invoke`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct StartTimerData {
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub task_title: String,
    #[serde(default)]
    pub project_name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub project_id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub domain: String,
    #[serde(default)]
    pub assigned_to: Vec<String>,
    #[serde(default)]
    pub deadline: String,
    #[serde(default)]
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub user_id: String,
    pub email: String,
    pub name: String,
    #[serde(default)]
    pub system_role: String,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub employee_id: Option<String>,
    #[serde(default)]
    pub skills: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentTask {
    pub task_id: String,
    pub project_id: String,
    pub task_title: String,
    pub project_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendanceSession {
    pub sign_in_at: String,
    #[serde(default)]
    pub sign_out_at: Option<String>,
    #[serde(default)]
    pub hours: Option<f64>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub task_title: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attendance {
    pub user_id: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub sessions: Vec<AttendanceSession>,
    #[serde(default)]
    pub total_hours: f64,
    #[serde(default)]
    pub current_sign_in_at: Option<String>,
    #[serde(default)]
    pub current_task: Option<CurrentTask>,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub user_email: String,
    #[serde(default)]
    pub system_role: String,
    /// "SIGNED_IN" or "SIGNED_OUT".
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub session_count: i64,
    /// Backend UTC timestamp — the frontend ticks against this to avoid local
    /// clock drift. Optional for older backends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_time: Option<String>,
}

/// Subset of `/orgs/current`'s `settings` object the desktop cares about.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrgSettings {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub features: HashMap<String, bool>,
}
