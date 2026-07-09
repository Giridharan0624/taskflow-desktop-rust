# Cross-platform activity monitoring — status

## Current state — DONE (cross-platform via crates)

Activity monitoring now uses **single cross-platform implementations** (no more
per-OS FFI or stubs), compile-verified on Windows locally and on Linux/macOS via
`build.yml` CI.

| Function (`src-tauri/src/monitor/`) | Implementation | Win | Linux X11 | macOS | Wayland |
|---|---|---|---|---|---|
| `idle_seconds()` | `user-idle` | ✅ | ✅ | ✅ | ✗ → 0 |
| `InputTracker::poll()` | `device_query` (rising-edge + cursor delta) | ✅ | ✅ | ✅¹ | ✗ → 0 |
| `active_app()` | `x-win` | ✅ | ✅ | ✅¹ | ⚠ partial |
| `capture_jpeg()` | `xcap` → JPEG (via `image`) | ✅ | ✅ | ✅² | ⚠ portal prompt |
| `is_screen_locked()` | `OpenInputDesktop` (Win) / else `false` | ✅ | idle heuristic | idle heuristic | — |
| `session_info()` | native | ✅ | ✅ | ✅ | ✅ (flags degraded) |

¹ macOS needs **Accessibility** permission for key events + window titles.
² macOS needs **Screen Recording** (TCC) permission for capture.

The sampling loop, edge-counting, jitter, gating, and heartbeat payload are
unchanged — only the platform primitives were swapped to crates.

## Crates used

| Capability | Crate | Coverage |
|---|---|---|
| Screenshots | **`xcap`** | Win / macOS / Linux X11 + Wayland (portal) |
| Active window | **`x-win`** | Win / macOS / Linux X11 (Wayland partial) |
| Idle time | **`user-idle`** | Win / macOS / Linux X11 |
| Input counters | **`device_query`** | Win / macOS / Linux X11 |
| Screen-lock | *(no cross-platform crate)* | Win `OpenInputDesktop`; else idle heuristic |

## Two real ceilings (OS limits, not crate gaps)

1. **Wayland** — no global input API and restricted window access for *any*
   library. Input counters and per-app tracking legitimately can't work; only
   portal-based screenshots do (with a prompt). `session_info()` already reports
   this so the UI can degrade honestly (`canTrackWindows: false`).
2. **macOS permissions** — Screen Recording (TCC) for capture and Accessibility
   for input/titles must be granted by the user at runtime. No crate avoids
   this; the app must detect denial and surface a "grant permission" state.

## Done

- `idle.rs` → `user-idle`; `input.rs` → `device_query` (same rising-edge +
  cursor-delta model); `active_window.rs` → `x-win`; `screenshot.rs` → `xcap`
  (JPEG via `image`), keeping `OpenInputDesktop` for Windows lock detection.
- Dropped the Linux/macOS stubs and trimmed `windows-sys` to just lock detection
  + the updater's `ShellExecuteW`.
- CI (`build.yml` / `release.yml`) installs the X11/xcb/pipewire dev libs the
  crates need on Linux.

### Trade-offs / notes

- **Windows screenshot fidelity**: `xcap` uses Windows Graphics Capture/BitBlt,
  not the hand-rolled DXGI path — functionally equivalent, different edge cases.
- **Cannot be runtime-tested off-platform.** Compile-verified on Windows locally
  and Linux/macOS via CI; actual capture (especially macOS TCC) can only be
  validated on-device.

## Remaining follow-ups

1. **macOS permissions UX** — detect Accessibility / Screen-Recording denial,
   gate `monitor::start`, and surface a "grant permission" hint in the UI.
2. **Runtime validation per OS** — start the timer and confirm a
   `/activity/heartbeat` POST with non-zero counters and (where the tenant flag
   is on) a screenshot upload, on real Linux + macOS machines.
3. **Wayland input** — inherently unavailable; keep reporting it via
   `session_info()` rather than faking data.
