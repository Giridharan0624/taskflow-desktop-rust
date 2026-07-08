# Releasing (cross-platform, via GitHub Actions)

Two workflows in `.github/workflows/`:

- **`build.yml`** — on every push/PR to `main`, compiles the app on
  **Windows + Linux + macOS**. This is the cross-platform gate (proves the
  Linux/macOS `#[cfg]` code builds). No secrets needed.
- **`release.yml`** — on a `v*` tag, builds the installers on all three OSes,
  generates `SHA256SUMS`, Ed25519-signs it, and publishes a GitHub Release with:
  `*-setup.exe` (Windows NSIS), `*.AppImage` + `*.deb` (Linux), `*.dmg` (macOS),
  plus `SHA256SUMS` and `SHA256SUMS.sig`. These are exactly what the in-app
  updater verifies.

## One-time setup

### 1. Config secrets (V2 stack — baked into release binaries)

Repo → Settings → Secrets and variables → Actions → **New repository secret**:

| Secret | Value |
|---|---|
| `TASKFLOW_API_URL` | `https://mcx0iyvisf.execute-api.ap-south-1.amazonaws.com/prod` |
| `TASKFLOW_COGNITO_REGION` | `ap-south-1` |
| `TASKFLOW_COGNITO_POOL_ID` | `ap-south-1_yWxQYrYXp` |
| `TASKFLOW_COGNITO_CLIENT_ID` | `6eaa6ej7a3j1p5jm5ooq1ui0g3` |
| `TASKFLOW_WEB_DASHBOARD_URL` | *(optional)* the V2 web app URL |

(These mirror `build-env.v2.ps1`, which stays local/git-ignored.)

### 2. Update signing key (Ed25519)

Generate a keypair **offline** (never let the private seed touch CI or git):

```bash
python3 - <<'PY'
import base64
from nacl.signing import SigningKey
k = SigningKey.generate()
print("PRIVATE (GH secret RELEASE_SIGNING_KEY_B64):")
print(base64.b64encode(bytes(k)).decode())
print("PUBLIC (paste into src-tauri/release.pub):")
print(base64.b64encode(bytes(k.verify_key)).decode())
PY
```

- Store the **PRIVATE** line as the secret `RELEASE_SIGNING_KEY_B64`.
- Paste the **PUBLIC** line as the only non-comment line in
  [`src-tauri/release.pub`](src-tauri/release.pub), commit it.

Until this is done, releases ship **unsigned** and release builds of the app
**refuse to auto-install** (fail-closed — the deliberate fix for the Go app's
empty-key bug). The `build.yml` workflow logs a warning when unsigned.

## Cutting a release

1. Bump the version in **both** `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json` to e.g. `0.2.0` (must match the tag).
2. Commit, then tag and push:
   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```
3. `release.yml` builds all platforms, signs, and publishes the release.
4. The running app's startup update check finds it, verifies the signature +
   checksum, and offers "Install & restart".

## Notes / gaps

- **macOS**: `macos-latest` is Apple Silicon (arm64) → arm64 `.dmg`. For an
  Intel build add a `macos-13` matrix entry (`x86_64-apple-darwin`), or build a
  universal binary with `--target universal-apple-darwin`.
- **Code signing / notarization** is not configured — installers are unsigned,
  so Windows SmartScreen and macOS Gatekeeper will warn. Add Authenticode
  (Windows) and Apple notarization (macOS) secrets + steps for public
  distribution. The Ed25519 signing above is for the *update channel*, separate
  from OS code-signing.
- **Linux platform code** (idle/input/active-window/screenshot) is still
  `#[cfg]`-stubbed — the Linux bundle builds and runs, but activity monitoring
  is inert until those are implemented.
