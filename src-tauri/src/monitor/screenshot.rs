//! Primary-display screenshot capture + screen-lock detection.
//!
//! Capture is cross-platform via `xcap` (Windows / macOS / Linux X11; Wayland
//! goes through the desktop portal and may prompt). macOS requires Screen
//! Recording (TCC) permission — a denied grant yields an empty/None capture.
//!
//! Screen-lock detection has no good cross-platform crate: Windows uses
//! `OpenInputDesktop`; elsewhere we rely on the idle heuristic in the caller.

/// Whether the workstation is locked / on the secure desktop, in which case we
/// skip capture (avoids uploading a blank lock screen). On Windows a normal
/// process cannot open the input desktop while locked.
#[cfg(windows)]
pub fn is_screen_locked() -> bool {
    use windows_sys::Win32::System::StationsAndDesktops::{
        CloseDesktop, OpenInputDesktop, DESKTOP_READOBJECTS,
    };
    unsafe {
        let h = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if h.is_null() {
            true
        } else {
            CloseDesktop(h);
            false
        }
    }
}

#[cfg(not(windows))]
pub fn is_screen_locked() -> bool {
    false
}

/// Capture the primary display as a JPEG (quality 85). None on failure.
///
/// Uses `xcap` for capture, then encodes with the `image` crate. We pull raw
/// RGBA bytes out of xcap's buffer (`into_raw()` → `Vec<u8>`, version-agnostic)
/// and re-encode with our own `image` dependency, so xcap's internal `image`
/// version never has to match ours.
pub fn capture_jpeg() -> Option<Vec<u8>> {
    let monitors = xcap::Monitor::all().ok()?;
    // First monitor is the primary on every platform xcap supports.
    let monitor = monitors.into_iter().next()?;
    let rgba = monitor.capture_image().ok()?;

    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw(); // RGBA8

    // Drop the alpha channel (JPEG has none).
    let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for px in raw.chunks_exact(4) {
        rgb.push(px[0]);
        rgb.push(px[1]);
        rgb.push(px[2]);
    }

    let mut out = std::io::Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
        .ok()?;
    Some(out.into_inner())
}
