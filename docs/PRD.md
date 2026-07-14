# TaskFlow Desktop (Rust/Tauri) — Product Requirements

## 1. Overview

TaskFlow Desktop is the companion app for the TaskFlow web platform: a small,
always-available window for **time tracking** and **activity monitoring** that
runs alongside a person's work. This document covers the **Rust + Tauri v2**
implementation (`desktop-rust/`), a from-scratch rewrite of the original Go +
Wails app (`desktop/`), targeting **full feature parity** with a lighter,
faster, cross-platform foundation.

- **Backend**: the same multi-tenant TaskFlow API (Cognito auth, `/attendance`,
  `/users/me`, `/activity/heartbeat`, presigned S3 uploads). The desktop app is
  a thin, native client — it holds no business logic the server doesn't.
- **Relationship to web**: the timer is migrating from web to desktop-only; the
  desktop app must reach parity before the web timer is retired.

## 2. Goals & non-goals

**Goals**
- Reproduce the Go app's **exact UI and every operation**, 1:1.
- Track work time reliably, tied to a task, with the counters continuing to add
  up correctly across network drops and restarts.
- Capture activity signals (keyboard/mouse intensity, active app, periodic
  screenshots) **only while the timer runs** and **only when the tenant enables
  them** — privacy is opt-in per organization.
- Ship small, start fast, sip memory. Auto-update safely.
- Build and run on **Windows, macOS, and Linux** from one codebase.

**Non-goals**
- No project/task management UI (that lives in the web app).
- No offline task *creation* — the app reflects server state; it queues activity
  data offline but does not author work items offline.
- No telemetry beyond the activity heartbeats the product already defines.

## 3. Users

- **Team member** (primary): signs in, picks a task, starts/stops the timer,
  occasionally switches tasks or logs a meeting. Wants it to stay out of the way
  (tray) and never lose their time.
- **Org admin / owner** (indirect): relies on the activity data the app reports;
  controls whether screenshots and activity monitoring are on per tenant.

## 4. Feature requirements

### 4.1 Authentication
- Email + password sign-in via **Cognito USER_PASSWORD_AUTH**.
- First-login **new-password** challenge (NEW_PASSWORD_REQUIRED).
- **Session persistence**: tokens stored in the OS keyring; relaunch stays
  signed in. Token refresh is automatic and de-duplicated.
- Sign-out clears tokens and stops all monitoring.
- A 401 anywhere tears the session down and returns the user to login.

### 4.2 Timer & attendance
- Start the timer on a **task** with a **mandatory description** (Start is
  disabled until a task + description are set).
- **Meeting mode**: start without a task ("Meeting").
- **Switch task while running**: starting a new task stops the prior one
  server-side; a warning shows the running task and elapsed time.
- **Stop** with a 5-second **Undo** window.
- Live clock ticks against **server time** (not the local clock) so cross-device
  elapsed agrees; an optimistic timestamp covers the click→server gap.
- Today's total, per-task session grouping, and a **session inspector**
  (per-task session drill-down).
- **Idle handling**: a "still working?" prompt after a user-set threshold, and a
  hard **auto-stop at 15 minutes** idle (with notification + Undo).

### 4.3 Activity monitoring (while timer active only)
- Per-second **idle detection** and **keyboard/mouse activity counters**.
- **Active-window / app** sampling for a per-app time breakdown.
- **Heartbeat** to `/activity/heartbeat` every 5 minutes with the exact payload
  the backend expects (counts, active/idle seconds, top app, app breakdown).
- **Screenshots** on a jittered 9–10 min interval, skipped when the screen is
  locked, uploaded via presigned S3 URL.
- **Feature gating**: screenshots **fail-closed** (off unless the tenant opts
  in); activity monitoring **fails-open** (default on).

### 4.4 Resilience
- **Offline queue**: heartbeats and screenshots are backlogged to disk on
  failure and drained when connectivity returns; the tasks list is cached for
  offline task selection.
- `network:error` / `network:restored` surfaced in the UI.

### 4.5 System integration
- **System tray**: menu (Open, Open dashboard, Quit), status tooltip, left-click
  restores the window.
- **Minimize to tray** on window close; real quit only via tray/Quit or OS
  signal, and quitting **auto-signs-out** a running timer.
- **Native notifications** (gated by the user's notification preference).
- **Launch at login** toggle.
- **Window size** persisted across runs.

### 4.6 Self-update
- Checks GitHub Releases on startup; offers "Install & restart" when a newer
  version exists.
- Verifies the download with **SHA-256 + an Ed25519 signature** over
  `SHA256SUMS`. **Release builds refuse to install unsigned updates** (fixing a
  latent bug in the Go app where an empty key silently disabled verification).
- Launches the platform installer (NSIS / dmg); Linux defers to the package
  manager.

### 4.7 Settings
- Theme (light / dark / system), daily goal hours, idle-prompt threshold,
  notification policy (all / errors-only / off), launch-at-login, and clear
  local cache. All persisted locally.

## 5. Platform support

| Platform | Status |
|---|---|
| **Windows** | Fully implemented and runtime-verified (dev host). NSIS installer. |
| **macOS** | Cross-platform code compiles in CI; needs on-device validation + TCC/Accessibility permission UX. `.dmg`, arm64. |
| **Linux** | Cross-platform code compiles in CI. `.AppImage` + `.deb`. Wayland degrades input/window tracking (surfaced honestly). |

Activity primitives use cross-platform crates (`user-idle`, `device_query`,
`x-win`, `xcap`). See `CROSS-PLATFORM-MONITORING.md`.

## 6. Success metrics

- **Parity**: every Go-app command, event, and screen present and behaving
  identically. (Met — the UI is a verbatim port over a Tauri bridge.)
- **Footprint** (measured, 5-run avg vs the Go app): binary **2.2× smaller**
  (7 MB vs 15.5 MB), cold... warm startup **~4.5× faster** (129 ms vs 588 ms),
  ~11% less private RAM. See `BENCHMARK` / the shared report.
- **Reliability**: no lost time across a network drop or a forced quit while the
  timer runs (offline queue + auto-sign-out).
- **Safety**: no unsigned auto-update ever installs in a release build.

## 7. Open items / roadmap

- macOS + Linux **runtime** validation (permissions, real capture) on-device.
- macOS "grant permission" UX for Screen Recording / Accessibility.
- Windows screenshot fidelity: `xcap` (GDI/Graphics Capture) today; DXGI Desktop
  Duplication is a possible upgrade.
- Defense-in-depth: extra DPAPI wrap on Windows keyring tokens.
- Code-signing / notarization for public distribution (separate from update
  signing).
