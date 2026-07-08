//! Non-auth managed state.
//!
//! Auth/session/token state lives in [`crate::auth::AuthManager`] (its own
//! managed state, async-locked). `AppState` holds the timer/lifecycle flags and
//! the activity-monitor handle.

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use crate::monitor::MonitorHandle;

/// Process-wide managed state, registered via `.manage(AppState::default())`.
#[derive(Default)]
pub struct AppState {
    /// True while the timer is running (`status == SIGNED_IN`). Read at exit to
    /// decide whether an auto-sign-out is needed.
    pub signed_in: AtomicBool,
    /// Set when a real quit is in progress, so the window close handler stops
    /// intercepting into minimize-to-tray.
    pub quitting: AtomicBool,
    /// Run-once guard so the exit auto-sign-out fires at most once across the
    /// tray-quit / SIGTERM / Ctrl-C paths.
    pub auto_signed_out: AtomicBool,
    /// The running activity monitor (present only while the timer is active).
    pub monitor: Mutex<Option<MonitorHandle>>,
}
