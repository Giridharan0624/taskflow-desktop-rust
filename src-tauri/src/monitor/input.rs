//! Global keyboard/mouse activity counters via polling (no low-level hook).
//!
//! Ports the Go `InputTracker`: each `poll()` samples `GetAsyncKeyState` for
//! rising edges on mouse buttons + keys and detects cursor movement, returning
//! cumulative totals. Called once per second by the monitor's sampling loop;
//! the caller diffs successive totals (with wrap + spike handling).

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    pub struct InputTracker {
        last: [bool; 256],
        kb_total: u32,
        ms_total: u32,
        cursor_seeded: bool,
        last_x: i32,
        last_y: i32,
    }

    impl InputTracker {
        pub fn new() -> Self {
            Self {
                last: [false; 256],
                kb_total: 0,
                ms_total: 0,
                cursor_seeded: false,
                last_x: 0,
                last_y: 0,
            }
        }

        pub fn poll(&mut self) -> (u32, u32) {
            unsafe {
                // Mouse buttons: L/R/M/X1/X2.
                for vk in [0x01i32, 0x02, 0x04, 0x05, 0x06] {
                    let down = (GetAsyncKeyState(vk) as u16 & 0x8000) != 0;
                    if down && !self.last[vk as usize] {
                        self.ms_total = self.ms_total.wrapping_add(1);
                    }
                    self.last[vk as usize] = down;
                }

                // Keyboard keys 0x08..0xFE (skips the mouse-button range above).
                let mut pressed = 0u32;
                for vk in 0x08i32..0xFF {
                    let down = (GetAsyncKeyState(vk) as u16 & 0x8000) != 0;
                    if down && !self.last[vk as usize] {
                        pressed += 1;
                    }
                    self.last[vk as usize] = down;
                }
                self.kb_total = self.kb_total.wrapping_add(pressed);

                // Mouse movement via cursor delta. The first sample seeds the
                // baseline without counting a phantom move.
                let mut pt = POINT { x: 0, y: 0 };
                if GetCursorPos(&mut pt) != 0 {
                    if !self.cursor_seeded {
                        self.last_x = pt.x;
                        self.last_y = pt.y;
                        self.cursor_seeded = true;
                    } else if pt.x != self.last_x || pt.y != self.last_y {
                        self.ms_total = self.ms_total.wrapping_add(1);
                        self.last_x = pt.x;
                        self.last_y = pt.y;
                    }
                }
            }
            (self.kb_total, self.ms_total)
        }
    }
}

#[cfg(not(windows))]
mod imp {
    /// Stub until per-OS X11 XInput2 / macOS CGEventSource counters land.
    pub struct InputTracker;

    impl InputTracker {
        pub fn new() -> Self {
            Self
        }
        pub fn poll(&mut self) -> (u32, u32) {
            (0, 0)
        }
    }
}

pub use imp::InputTracker;
