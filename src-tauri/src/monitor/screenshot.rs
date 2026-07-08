//! Primary-display screenshot capture + screen-lock detection.
//!
//! Windows uses GDI `BitBlt` (functional and dependency-light). DXGI Desktop
//! Duplication — faster and able to capture some HW-accelerated/protected
//! surfaces GDI misses — is a Windows performance follow-up. Linux/macOS capture
//! lands with their per-OS work; until then they return None.

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
#[cfg(windows)]
pub fn capture_jpeg() -> Option<Vec<u8>> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        if w <= 0 || h <= 0 {
            return None;
        }

        let screen = GetDC(null_mut());
        if screen.is_null() {
            return None;
        }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);

        let ok = BitBlt(mem, 0, 0, w, h, screen, 0, 0, SRCCOPY);

        // 32bpp top-down DIB (negative height) so rows are in natural order.
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h;
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB as u32;

        let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
        let scanlines = GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            buf.as_mut_ptr() as *mut _,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);

        if ok == 0 || scanlines == 0 {
            return None;
        }

        // BGRA (GDI order) → RGB for the JPEG encoder.
        let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
        for px in buf.chunks_exact(4) {
            rgb.push(px[2]);
            rgb.push(px[1]);
            rgb.push(px[0]);
        }

        let mut out = std::io::Cursor::new(Vec::new());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
            .encode(&rgb, w as u32, h as u32, image::ExtendedColorType::Rgb8)
            .ok()?;
        Some(out.into_inner())
    }
}

#[cfg(not(windows))]
pub fn capture_jpeg() -> Option<Vec<u8>> {
    None
}
