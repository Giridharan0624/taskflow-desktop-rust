//! Global keyboard/mouse activity counters via polling (no low-level hook).
//!
//! Cross-platform via `device_query` (Windows / macOS / Linux X11). Keeps the Go
//! `InputTracker` model exactly: each `poll()` (once per second) counts rising
//! edges on keys + mouse buttons and detects cursor movement, returning
//! cumulative totals; the caller diffs successive totals.
//!
//! Wayland has no global-input API, so `device_query` sees nothing there and the
//! counters stay 0 (`session_info()` reports the degradation). macOS requires
//! Accessibility permission for key events.

use std::collections::HashSet;

use device_query::{DeviceQuery, DeviceState, Keycode};

pub struct InputTracker {
    device: DeviceState,
    last_keys: HashSet<Keycode>,
    last_buttons: Vec<bool>,
    kb_total: u32,
    ms_total: u32,
    cursor_seeded: bool,
    last_pos: (i32, i32),
}

impl InputTracker {
    pub fn new() -> Self {
        Self {
            device: DeviceState::new(),
            last_keys: HashSet::new(),
            last_buttons: Vec::new(),
            kb_total: 0,
            ms_total: 0,
            cursor_seeded: false,
            last_pos: (0, 0),
        }
    }

    pub fn poll(&mut self) -> (u32, u32) {
        // Keyboard: count keys newly pressed since the last sample.
        let keys: HashSet<Keycode> = self.device.get_keys().into_iter().collect();
        let new_presses = keys.difference(&self.last_keys).count() as u32;
        self.kb_total = self.kb_total.wrapping_add(new_presses);
        self.last_keys = keys;

        let mouse = self.device.get_mouse();

        // Mouse buttons: rising edges. `button_pressed[0]` is unused; 1.. are buttons.
        for (i, &down) in mouse.button_pressed.iter().enumerate() {
            let was = self.last_buttons.get(i).copied().unwrap_or(false);
            if down && !was {
                self.ms_total = self.ms_total.wrapping_add(1);
            }
        }
        self.last_buttons = mouse.button_pressed.clone();

        // Mouse movement via cursor delta; first sample seeds without counting.
        let pos = mouse.coords;
        if !self.cursor_seeded {
            self.last_pos = pos;
            self.cursor_seeded = true;
        } else if pos != self.last_pos {
            self.ms_total = self.ms_total.wrapping_add(1);
            self.last_pos = pos;
        }

        (self.kb_total, self.ms_total)
    }
}
