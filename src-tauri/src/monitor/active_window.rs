//! Foreground application name.
//!
//! Cross-platform via `x-win` (Windows / macOS / Linux X11). Returns the process
//! name (falling back to the window title). On Wayland window access is
//! restricted, so this returns None there — `session_info()` flags it.

pub fn active_app() -> Option<String> {
    let win = x_win::get_active_window().ok()?;
    let name = win.info.name.trim();
    if !name.is_empty() {
        return Some(name.to_string());
    }
    let title = win.title.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}
