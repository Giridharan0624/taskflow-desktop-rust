# Build-time config injection for the Rust/Tauri desktop app.
#
# Mirrors the Go app's ldflags approach: these env vars are read by build.rs
# and baked into the binary. Copy to build-env.staging.ps1 / build-env.company.ps1
# / build-env.prod.ps1 with real values (git-ignored), dot-source it, then build:
#
#   . .\build-env.staging.ps1
#   npm run tauri build
#
# For local `tauri dev` you can instead drop a src-tauri/config.json
# (see src-tauri/config.example.json) and skip these entirely.

$env:TASKFLOW_API_URL           = "https://<api-id>.execute-api.ap-south-1.amazonaws.com/prod"
$env:TASKFLOW_COGNITO_REGION    = "ap-south-1"
$env:TASKFLOW_COGNITO_POOL_ID   = "ap-south-1_XXXXXXXXX"
$env:TASKFLOW_COGNITO_CLIENT_ID = "xxxxxxxxxxxxxxxxxxxxxxxxxx"
$env:TASKFLOW_WEB_DASHBOARD_URL = "https://taskflow.neurostack.in"   # optional
