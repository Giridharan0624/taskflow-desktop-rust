//! Seconds since the last user input. Stateless — safe to call any time,
//! including when the timer/monitor is not running (drives the idle prompt).

/// Windows: `GetLastInputInfo` vs `GetTickCount`.
#[cfg(windows)]
pub fn idle_seconds() -> u64 {
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    unsafe {
        let mut lii = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut lii) == 0 {
            return 0;
        }
        // GetTickCount wraps ~49.7 days; wrapping_sub handles the rollover.
        let idle_ms = GetTickCount().wrapping_sub(lii.dwTime);
        (idle_ms / 1000) as u64
    }
}

/// Non-Windows fallback (real Linux X11/logind + macOS impls land in M4 follow-up
/// per-OS work; until then idle is reported as 0 so the prompt never fires).
#[cfg(not(windows))]
pub fn idle_seconds() -> u64 {
    0
}
