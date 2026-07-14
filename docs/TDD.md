# TaskFlow Desktop (Rust/Tauri) — Technical Design

## 1. Stack

| Layer | Choice |
|---|---|
| Shell | **Tauri v2** (Rust backend + system WebView — WebView2 on Windows, WebKit on macOS/Linux) |
| Backend language | **Rust** (stable, `x86_64-pc-windows-msvc` dev host) |
| Async runtime | **tokio** (multi-thread) |
| Frontend | **Preact + Vite + Tailwind** — the Go app's UI copied **verbatim** |
| Bridge | `window.go.main.App` / `window.runtime` **shim** over Tauri `invoke`/`listen` |
| HTTP | `reqwest` (rustls) |
| Auth | hand-rolled Cognito over `reqwest`; `keyring` for token storage |
| Monitor | `user-idle`, `device_query`, `x-win`, `xcap` (cross-platform) |
| Crypto | `sha2`, `ed25519-dalek` (update verification) |

Single `src-tauri` **binary crate** with `#[cfg]`-gated platform code — no
multi-crate workspace (every subsystem shares one `AppHandle`, managed state,
event emitter, and error type).

## 2. Module map (`src-tauri/src/`)

```
main.rs        builder: plugins, .manage(state), command registry, window-event handler
app.rs         setup() hook: restore session, build tray, install signal handlers, startup update check
config.rs      compile-time config (env! via build.rs) + dev config.json fallback; panic on missing required
error.rs       AppError (thiserror) → serde {code,message}; NotAuthenticated / Unauthorized / Network / Message
events.rs       backend→frontend event names + typed emit helpers
state/          AppState (managed): signed_in / quitting / auto_signed_out flags, monitor handle
auth/           mod.rs (AuthManager: flows, singleflight refresh, restore) · cognito.rs · keyring.rs
api/            mod.rs (ApiClient: bearer, endpoints, settings cache, screenshot upload) · models.rs
monitor/        mod.rs (supervisor + sampling/heartbeat/screenshot loops) · idle · input · active_window · screenshot · session
tray/           menu, tooltip, open-browser, show_main
lifecycle.rs    auto_sign_out (run-once), request_quit, OS-signal handlers
updater/        mod.rs (GitHub poll, orchestrate) · verify.rs (sha256 + ed25519) · install.rs (per-OS launch)
queue/          on-disk backlogs (heartbeat JSONL, screenshot jpeg+json, tasks cache), clear_all
window_size.rs  persist/restore main window size
commands/       thin #[command] wrappers: auth_cmds · data_cmds · timer_cmds · system_cmds · update_cmds
```

Config injection: `build.rs` bakes `TASKFLOW_*` env vars via `cargo:rustc-env`;
`config.rs` reads them with `env!`, falls back to `src-tauri/config.json` for
dev, and **panics at startup if a required field is missing**.

## 3. Process & thread model

- **Tauri main thread** runs the event loop; window/tray/menu events dispatch here.
- **tokio** runs all async commands + background tasks.
- The **activity monitor** owns:
  - a dedicated **OS thread** for 1 s sampling (Win32/X11/CG handles are blocking
    and often `!Send`, so they stay off the async reactor), and
  - **tokio tasks** for the 5-min heartbeat and jittered screenshot loops.
- `!Send` platform handles never cross an `await`; only `Send` snapshots (counts,
  JPEG bytes) move between the sampler and the async loops via a shared
  `Arc<Mutex<Bucket>>`.

## 4. Frontend & the Wails-compat bridge

The UI is the Go app's Preact source copied unchanged (341 `class=` attrs,
`window.go.main.App.*` bindings, `window.runtime.EventsOn/Off`). Rather than
rewrite it, `src/lib/tauri-bridge.ts` installs those globals before render:

- `window.go.main.App.<Method>` → `invoke("<snake_command>", args)`
- `window.runtime.EventsOn/Off(name, cb)` → Tauri `listen`/unlisten
- **Key-case transform**: results are deep **snake→camel** (the UI expects
  camelCase, as Wails delivered); object args are **camel→snake** (Rust serde
  models are snake_case). Scalars pass through.
- Rejections are normalized to `Error(message)` so the UI's `friendlyError` works.

This makes the port a framework swap (React→Preact) + one bridge file, not a
line-by-line rewrite.

## 5. IPC contract

**Commands** (21) — all post-auth ones fail with `Unauthorized`→`auth:expired` on 401:

`login`, `set_new_password`, `logout`, `is_authenticated`,
`get_current_user`, `get_my_tasks`, `get_my_attendance`, `sign_in`, `sign_out`,
`get_app_version`, `get_web_dashboard_url`, `show_window`,
`show_tray_notification`, `save_window_size`, `get_idle_seconds`,
`get_session_info`, `clear_local_cache`, `set_auto_start`, `get_auto_start`,
`check_for_update`, `install_update`.

**Events** (backend→frontend): `attendance:updated`, `network:error`,
`network:restored`, `update:available`, `update:package-managed`, `auth:expired`.

**Models** (`api/models.rs`, snake_case matching the backend wire format):
`User`, `Task`, `Attendance` (+ `AttendanceSession`, `CurrentTask`),
`StartTimerData`, `OrgSettings`. `Attendance` derives `Default` so a `null`
`/attendance/me` (no record today) maps to an empty SIGNED_OUT state.

## 6. Auth (`auth/`)

- **Cognito** (`cognito.rs`): three unauthenticated JSON POSTs over `reqwest` —
  `InitiateAuth` (USER_PASSWORD_AUTH), `RespondToAuthChallenge`
  (NEW_PASSWORD_REQUIRED), and REFRESH_TOKEN_AUTH. No SRP, no aws-sdk.
- **AuthManager** (`mod.rs`): owns tokens + the challenge session (never crosses
  IPC) behind one `tokio::sync::Mutex`. `valid_id_token()` refreshes on expiry;
  the lock gives **singleflight** refresh for free (concurrent callers coalesce).
  On refresh failure → clear session → `Unauthorized`.
- **Persistence** (`keyring.rs`): OS keyring (Credential Manager / Keychain /
  Secret Service) via `keyring`, base64 + chunked (`key.0..N` + count) to clear
  the Windows blob-size limit. Session restored at startup.

## 7. API client (`api/`)

One pooled `reqwest::Client` (rustls), bearer-authed with the current ID token.
Endpoints: `GET /users/me`, `/users/me/tasks`, `/attendance/me`;
`POST /attendance/sign-in`; `PUT /attendance/sign-out`; `GET /orgs/current`
(→ cached `settings`); `POST /activity/heartbeat`; presigned screenshot PUT
(host-validated to https S3, retried once on a 403 expired-presign). A separate
long-timeout client is used for multi-MB screenshot uploads.

## 8. Activity monitor (`monitor/`)

Started from `sign_in`, stopped from `sign_out`/`logout` (handle in `AppState`).

- **Sampling thread (1 s)**: `idle_seconds()` (`user-idle`), input deltas
  (`device_query` — rising-edge key/button counting + cursor-movement, uint32
  wrap + <1000 spike cap), and every 5 s the active app (`x-win`), accumulated
  into a shared `Bucket` (30-app cap).
- **Heartbeat task (5 min)**: drains the offline backlog, then snapshots+resets
  the bucket and POSTs it (RFC3339 timestamp + counts + top_app + app_breakdown),
  gated by `activity_monitoring` (fail-open).
- **Screenshot task (jittered 9–10 min)**: gated by `screenshots` (fail-closed);
  skips when locked (`OpenInputDesktop` on Windows); captures via `xcap` on a
  blocking task → JPEG (q85); presigned upload; backlog on failure.
- **Network status**: a shared flag drives one `network:error` / `network:restored`
  pair across both loops.

Platform coverage & Wayland/macOS-permission limits: see
`CROSS-PLATFORM-MONITORING.md`.

## 9. Lifecycle (`lifecycle.rs`, `main.rs`)

- Window **close-request** → `prevent_close()` + `hide()` (minimize to tray),
  unless a real quit is in progress.
- **Quit paths** — tray Quit, SIGTERM, Ctrl-C — all route through
  `auto_sign_out`: **run-once** (atomic guard), signs out only if the timer is
  active, **5 s-bounded** so shutdown never hangs, then `app.exit`.

## 10. Updater (`updater/`)

`check_for_update` polls GitHub `releases/latest`, compares semver, and picks the
platform asset (`.exe`+"setup" / `.AppImage` / `.dmg`). `install_update`
downloads the asset + `SHA256SUMS` (+ `.sig`), **verifies the Ed25519 signature
over `SHA256SUMS`** (hard-fail if no pubkey in a release build — the security
fix), checks the asset's SHA-256, stages to temp, and launches the installer
(NSIS via `ShellExecuteW` runas /S; `open` on macOS; package-managed on Linux),
then exits. Single-install guarded by an atomic. Debug builds short-circuit.

## 11. Security

- Tokens live in the OS keyring, never on plain disk. (Extra DPAPI wrap on
  Windows is a planned defense-in-depth follow-up.)
- Update channel is **signed** (Ed25519 over SHA256SUMS); unsigned installs are
  refused in release builds.
- Screenshot upload URLs are validated (https + `amazonaws.com`) before any pixel
  data is PUT — a compromised backend can't redirect a frame elsewhere.
- Screenshots are **fail-closed** on the tenant flag; challenge sessions and
  refresh tokens never cross the IPC boundary.
- Build-time secrets are injected via env/`build.rs`, not committed.

## 12. Build & release

- **`build.rs`** injects `TASKFLOW_*` config at compile time.
- **CI — `.github/workflows/build.yml`**: on push/PR, compiles on
  Windows + Linux + macOS (the cross-platform gate; installs the X11/xcb/pipewire
  dev libs on Linux). No secrets needed.
- **Release — `.github/workflows/release.yml`**: on a `v*` tag, builds installers
  per OS (NSIS / AppImage+deb / dmg), generates `SHA256SUMS`, Ed25519-signs it,
  and publishes a GitHub Release. Version must match across the tag, `Cargo.toml`,
  and `tauri.conf.json`.
- **Bundle config** (`tauri.conf.json`): NSIS `installerIcon`, `.deb` runtime
  `depends` (webkit/appindicator/X11/xcb/pipewire), macOS dmg layout, TaskFlow
  icon set for window/installer/tray.

## 13. Testing & verification

- **Compile gate**: CI builds all three OSes on every push — catches
  platform-specific breakage the Windows dev host can't.
- **Runtime**: Windows verified (login → timer → app runs against V2). Linux/macOS
  runtime (real capture, macOS permissions) is the outstanding validation.
- **End-to-end to confirm**: a live 5-min timer producing a real
  `/activity/heartbeat` 200 with non-zero counters, and a screenshot upload.

## 14. Known limitations

- Linux/macOS activity capture is compile-verified, not runtime-verified.
- Wayland: no global input / limited window access (reported via `session_info`).
- macOS: capture/input require runtime TCC/Accessibility grants (no UX yet).
- Installers are not OS-code-signed (SmartScreen/Gatekeeper warn) — separate from
  the Ed25519 *update* signing.
- Single-monitor screenshot (primary display only), matching the Go app.
