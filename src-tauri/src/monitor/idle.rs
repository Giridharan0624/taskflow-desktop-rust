//! Seconds since the last user input. Stateless — safe to call any time,
//! including when the timer/monitor is not running (drives the idle prompt).
//!
//! Cross-platform via the `user-idle` crate (Windows / macOS / Linux X11). On
//! Wayland there is no idle API, so it errors and we report 0 (never idle) —
//! `session_info()` already flags Wayland as degraded.

pub fn idle_seconds() -> u64 {
    user_idle::UserIdle::get_time()
        .map(|t| t.as_seconds())
        .unwrap_or(0)
}
