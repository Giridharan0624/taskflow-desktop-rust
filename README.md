# TaskFlow Desktop (Rust / Tauri v2)

A from-scratch Rust rewrite of the Go + Wails desktop companion (`../desktop`).
Rust backend (Tauri v2) + fresh React + Vite + Tailwind UI. Targets Windows,
Linux, and macOS at full feature parity with the Go app.

> **Status: M0–M6 complete — full feature parity, compiles + links clean.**
> M6 adds the self-updater (GitHub Releases poll → sha256 + **ed25519** verify →
> platform installer launch, with the security fix that **release builds refuse
> to install unsigned updates** — unlike the Go app, which silently skipped
> verification on an empty key), launch-at-login (`set/get_auto_start` via
> `tauri-plugin-autostart`), single-instance, and a background startup update
> check (`update:available` / `update:package-managed`).
>
> **Verified:** frontend builds clean; `cargo build` links a ~22 MB binary with
> zero warnings; runtime boot + Cognito round-trip confirmed. **Windows is the
> fully-implemented target**; Linux/macOS platform primitives (idle, input,
> active-window, screenshot, installer) are `#[cfg]`-gated stubs awaiting their
> per-OS pass, and are compile-gated out of the Windows build.
>
> **Build status:** frontend builds clean; **Rust compiles and links clean**
> (Rust 1.96.1 MSVC, Tauri 2.11.5) and the app has been **runtime boot-verified**
> (launches, renders, tray + keyring work; Cognito login round-trips to AWS).
> To exercise login/timer/heartbeats end-to-end, point `src-tauri/config.json`
> at the V2 stack (see below) and sign in.

## Prerequisites

1. **Rust** (stable) via [rustup](https://rustup.rs/).
2. **Tauri v2 system deps** — see <https://v2.tauri.app/start/prerequisites/>:
   - Windows: WebView2 (preinstalled on Win 11).
   - Linux: `webkit2gtk-4.1`, `libayatana-appindicator3`, `librsvg2`, build-essential.
   - macOS: Xcode Command Line Tools.
3. **Node 22 + npm** (already present).
4. Tauri CLI: `npm install` pulls `@tauri-apps/cli` locally (used via `npm run tauri`).

## Configure

Two ways to supply the Cognito / API config:

- **Dev:** copy `src-tauri/config.example.json` → `src-tauri/config.json` and fill
  in real values (git-ignored).
- **Release builds:** set `TASKFLOW_*` env vars before building (see
  `build-env.example.ps1`). Missing required values panic at startup by design.

Values come from the **V2 CDK stack outputs** (`taskflow-v2`) — API URL, Cognito
region/pool/client. Do not point this at the legacy `taskflow` stack.

## Run (M0)

```bash
npm install
npm run tauri dev      # builds Rust, starts Vite, opens the window
```

You should see the app window render "TaskFlow" with the backend version pulled
over IPC — that confirms the M0 slice.

## Build / package

```bash
npm run tauri build    # NSIS .exe (Win), AppImage/.deb (Linux), .dmg (macOS)
```

Output lands in `src-tauri/target/release/bundle/`. The Windows NSIS installer is
named `TaskFlow_<version>_x64-setup.exe` — the "setup" in the name is what the
self-updater's asset matcher looks for, so keep it.

Release builds should inject config via `TASKFLOW_*` env vars (see
`build-env.example.ps1`) rather than a bundled `config.json`.

### Signing releases (required for auto-update)

The updater refuses to auto-install unless `src-tauri/release.pub` contains a
base64 ed25519 public key. To enable signed auto-updates, commit the public key
there and have your release workflow publish, alongside the installer:
`SHA256SUMS` (sha256 of each asset) and `SHA256SUMS.sig` (base64 ed25519
signature of `SHA256SUMS` under the matching private key). Until then, release
builds simply won't offer updates (fail-closed) — this is the deliberate fix for
the Go app's empty-key bug.

## Layout

```
desktop-rust/
├─ src/                 React + Vite + Tailwind UI
│  └─ lib/ipc.ts        typed invoke() wrapper (replaces Wails bindings)
└─ src-tauri/
   ├─ build.rs          bakes TASKFLOW_* env vars into the binary
   ├─ tauri.conf.json   window + bundle config
   ├─ capabilities/     v2 permission ACL
   └─ src/
      ├─ main.rs        builder, plugins, managed state, command registry
      ├─ app.rs         setup hook (tray/lifecycle/update-check land here)
      ├─ config.rs      compile-time config + dev config.json fallback
      ├─ error.rs       AppError (serializes to {code,message})
      ├─ events.rs      backend→frontend event names
      ├─ state/         managed AppState (auth/session/attendance/idle)
      └─ commands/      thin #[command] wrappers per subsystem
```

The Go app under `../desktop/internal/{auth,api,monitor,tray,updater,queue,state,system}`
is the reference implementation — port behavior one-to-one.
