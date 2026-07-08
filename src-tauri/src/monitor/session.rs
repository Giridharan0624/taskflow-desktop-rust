//! Display-server capabilities, surfaced to the frontend so it can honestly
//! communicate tracking limitations (mainly Wayland).

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    /// Whether per-app active-window tracking is available.
    pub can_track_windows: bool,
    /// "windows" | "quartz" | "x11" | "wayland" | "unknown".
    pub display_server: String,
    /// Human-readable note when tracking is degraded (else None).
    pub limitation: Option<String>,
}

#[cfg(windows)]
pub fn session_info() -> SessionInfo {
    SessionInfo {
        can_track_windows: true,
        display_server: "windows".into(),
        limitation: None,
    }
}

#[cfg(target_os = "macos")]
pub fn session_info() -> SessionInfo {
    SessionInfo {
        can_track_windows: true,
        display_server: "quartz".into(),
        limitation: None,
    }
}

#[cfg(target_os = "linux")]
pub fn session_info() -> SessionInfo {
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    if session_type.eq_ignore_ascii_case("wayland") {
        SessionInfo {
            can_track_windows: false,
            display_server: "wayland".into(),
            limitation: Some(
                "Per-app tracking is limited on Wayland; only overall active time is recorded."
                    .into(),
            ),
        }
    } else {
        SessionInfo {
            can_track_windows: true,
            display_server: "x11".into(),
            limitation: None,
        }
    }
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub fn session_info() -> SessionInfo {
    SessionInfo {
        can_track_windows: false,
        display_server: "unknown".into(),
        limitation: Some("Activity tracking is unavailable on this platform.".into()),
    }
}
