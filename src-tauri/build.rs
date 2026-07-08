use std::env;

/// Build-time configuration injection.
///
/// Mirrors the Go app's `-ldflags -X` approach: each `TASKFLOW_*` env var
/// present at build time is baked into the binary via `cargo:rustc-env`, then
/// read back through `env!()` in `config.rs`. Vars that are unset are emitted
/// as empty strings so `env!()` always resolves — `config.rs` decides at
/// startup whether a missing required value is fatal (panic) or falls back to
/// a local `config.json` for dev.
const INJECTED: &[&str] = &[
    "TASKFLOW_API_URL",
    "TASKFLOW_COGNITO_REGION",
    "TASKFLOW_COGNITO_POOL_ID",
    "TASKFLOW_COGNITO_CLIENT_ID",
    "TASKFLOW_WEB_DASHBOARD_URL",
];

fn main() {
    for key in INJECTED {
        let val = env::var(key).unwrap_or_default();
        println!("cargo:rustc-env={key}={val}");
        println!("cargo:rerun-if-env-changed={key}");
    }

    tauri_build::build();
}
