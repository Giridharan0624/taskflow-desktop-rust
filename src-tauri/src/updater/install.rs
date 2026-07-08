//! Per-OS installer launch + platform asset selection.

use std::path::Path;

use super::Asset;

pub enum InstallError {
    /// Linux: the app is managed by a system package manager — the user must
    /// update through it. Surfaced as the `update:package-managed` event.
    /// (Constructed only on Linux; dead on other targets.)
    #[allow(dead_code)]
    PackageManaged,
    Other(String),
}

/// Pick the installer asset for the current platform from a release's assets.
#[cfg(windows)]
pub fn find_platform_asset(assets: &[Asset]) -> Option<&Asset> {
    // The NSIS installer, not the raw taskflow-desktop.exe.
    assets.iter().find(|a| {
        let n = a.name.to_ascii_lowercase();
        n.ends_with(".exe") && n.contains("setup")
    })
}

#[cfg(target_os = "linux")]
pub fn find_platform_asset(assets: &[Asset]) -> Option<&Asset> {
    assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().ends_with(".appimage"))
}

#[cfg(target_os = "macos")]
pub fn find_platform_asset(assets: &[Asset]) -> Option<&Asset> {
    assets
        .iter()
        .find(|a| a.name.to_ascii_lowercase().ends_with(".dmg"))
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn find_platform_asset(_assets: &[Asset]) -> Option<&Asset> {
    None
}

/// Launch the downloaded installer. On success the caller exits so the installer
/// can replace files.
#[cfg(windows)]
pub fn launch(path: &Path) -> Result<(), InstallError> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    // "runas" → UAC elevation; "/S" → NSIS silent install (preserves autostart
    // registry entries from the prior install).
    let verb = wide("runas");
    let file = wide(&path.to_string_lossy());
    let params = wide("/S");
    let ret = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // ShellExecuteW returns a value > 32 on success.
    if (ret as isize) > 32 {
        Ok(())
    } else {
        Err(InstallError::Other(format!(
            "installer launch failed (ShellExecute code {})",
            ret as isize
        )))
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "macos")]
pub fn launch(path: &Path) -> Result<(), InstallError> {
    // Open the .dmg for the user to drag-install.
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|e| InstallError::Other(e.to_string()))
}

#[cfg(target_os = "linux")]
pub fn launch(_path: &Path) -> Result<(), InstallError> {
    // AppImage in-place swap is a follow-up; for now defer to the package
    // manager so we never leave a half-updated binary. The command layer turns
    // this into the `update:package-managed` event.
    Err(InstallError::PackageManaged)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn launch(_path: &Path) -> Result<(), InstallError> {
    Err(InstallError::Other("unsupported platform".into()))
}
