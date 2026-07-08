# Cross-platform activity monitoring — status & plan

## Current state

Activity monitoring (idle, keyboard/mouse counters, active-window, screenshots)
is **implemented on Windows only**. The non-Windows branches are `#[cfg]` stubs
so the app compiles and runs everywhere, but they capture nothing.

| Function (`src-tauri/src/monitor/`) | Windows | Linux | macOS |
|---|---|---|---|
| `idle_seconds()` | `GetLastInputInfo` | stub → `0` | stub → `0` |
| `InputTracker::poll()` | `GetAsyncKeyState` + cursor delta | stub → `(0,0)` | stub → `(0,0)` |
| `active_app()` | `GetForegroundWindow` → process | stub → `None` | stub → `None` |
| `capture_jpeg()` | GDI `BitBlt` → JPEG | stub → `None` | stub → `None` |
| `is_screen_locked()` | `OpenInputDesktop` | stub → `false` | stub → `false` |
| `session_info()` | ✅ real | ✅ real (X11/Wayland) | ✅ real (quartz) |

Net effect on Linux/macOS today: the monitor loop and heartbeat run and the
timer tracks time, but every activity signal is `0`/empty and no screenshots are
taken. Effectively inert monitoring.

## There are good cross-platform crates

The Rust ecosystem covers almost all of this cross-platform, which lets us
replace the Windows FFI **and** both stubs with a single implementation.

| Capability | Crate (recommended) | Coverage | Notes |
|---|---|---|---|
| Screenshots | **`xcap`** | Win / macOS / Linux X11 **+ Wayland** | Captures monitors → `image::RgbaImage`; Wayland via desktop portal + PipeWire. Replaces GDI + both stubs. |
| Active window | **`x-win`** (alt: `active-win-pos-rs`) | Win / macOS / Linux X11 (Wayland partial) | Returns title + app path + pid. |
| Idle time | **`user-idle`** | Win / macOS / Linux X11 | Direct idle duration. |
| Input counters | **`device_query`** (polling) or `rdev` (events) | Win / macOS / Linux X11 | `device_query` maps 1:1 to the current model: poll pressed keys + cursor each second, diff for edges. |
| Screen-lock | *(no strong cross-platform crate)* | — | Keep per-OS (`OpenInputDesktop` on Win) or fall back to the idle heuristic. |

## Two real ceilings (OS limits, not crate gaps)

1. **Wayland** — no global input API and restricted window access for *any*
   library. Input counters and per-app tracking legitimately can't work; only
   portal-based screenshots do (with a prompt). `session_info()` already reports
   this so the UI can degrade honestly (`canTrackWindows: false`).
2. **macOS permissions** — Screen Recording (TCC) for capture and Accessibility
   for input/titles must be granted by the user at runtime. No crate avoids
   this; the app must detect denial and surface a "grant permission" state.

## Plan

Refactor `monitor/{idle,input,active_window,screenshot}.rs` to the crates above,
keeping the loop/edge-counting/jitter/heartbeat logic identical — only the
platform primitives change:

1. `idle.rs` → `user-idle`.
2. `input.rs` → `device_query` (same rising-edge + cursor-delta logic, now
   backed by `DeviceState::get_keys()` / `get_mouse()`).
3. `active_window.rs` → `x-win`.
4. `screenshot.rs` → `xcap` for capture; keep `OpenInputDesktop` (Win) for lock,
   idle-heuristic elsewhere.
5. Drop most `windows-sys` usage and all Linux/macOS stubs.
6. macOS: detect permission denial, gate `monitor::start`, expose a UI hint.

### Trade-offs

- **Windows screenshot fidelity**: `xcap` uses Windows Graphics Capture/BitBlt,
  not the hand-rolled DXGI path — functionally equivalent, different edge cases.
- A few more dependencies; Linux builds need X11 dev libs (already installed in
  `.github/workflows/build.yml`).
- **Cannot be runtime-tested off-platform.** The CI compiles Linux/macOS on real
  runners, but actual capture (especially macOS TCC) can only be validated
  on-device.

### Verification

- `build.yml` CI must stay green on all three OSes after the refactor.
- Runtime: on each OS, start the timer and confirm a `/activity/heartbeat` POST
  with non-zero counters and (where the tenant flag is on) a screenshot upload.
