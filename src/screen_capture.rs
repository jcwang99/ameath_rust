use image::DynamicImage;
use std::mem;

use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    EnumDisplayMonitors, GetDC, GetDIBits, GetMonitorInfoW, ReleaseDC, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC, HMONITOR, MONITORINFO, SRCCOPY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetDesktopWindow, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};

pub fn capture_primary_monitor() -> Result<DynamicImage, String> {
    unsafe {
        let hwnd_desktop = GetDesktopWindow();
        let hdc_screen = GetDC(hwnd_desktop);
        if hdc_screen.is_invalid() {
            return Err("Failed to get DC for desktop".to_string());
        }

        // Get screen dimensions
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);

        if w == 0 || h == 0 {
            ReleaseDC(hwnd_desktop, hdc_screen);
            return Err("Screen dimensions are zero".to_string());
        }

        let hdc_mem = CreateCompatibleDC(hdc_screen);
        if hdc_mem.is_invalid() {
            ReleaseDC(hwnd_desktop, hdc_screen);
            return Err("Failed to create compatible DC".to_string());
        }

        let hbm_screen = CreateCompatibleBitmap(hdc_screen, w, h);
        if hbm_screen.is_invalid() {
            DeleteDC(hdc_mem);
            ReleaseDC(hwnd_desktop, hdc_screen);
            return Err("Failed to create compatible bitmap".to_string());
        }

        let old_obj = SelectObject(hdc_mem, hbm_screen);

        // BitBlt from screen to memory DC
        let bitblt_res = BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, 0, 0, SRCCOPY);
        if bitblt_res.is_err() {
            SelectObject(hdc_mem, old_obj);
            DeleteObject(hbm_screen);
            DeleteDC(hdc_mem);
            ReleaseDC(hwnd_desktop, hdc_screen);
            return Err("BitBlt failed".to_string());
        }

        // Get bits
        let bi = BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h, // Top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let mut pixels: Vec<u8> = vec![0; (w * h * 4) as usize];
        let mut info = BITMAPINFO {
            bmiHeader: bi,
            ..Default::default()
        };

        let get_bits_res = GetDIBits(
            hdc_mem,
            hbm_screen,
            0,
            h as u32,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut info,
            DIB_RGB_COLORS,
        );

        // Cleanup GDI objects
        SelectObject(hdc_mem, old_obj);
        DeleteObject(hbm_screen);
        DeleteDC(hdc_mem);
        ReleaseDC(hwnd_desktop, hdc_screen);

        if get_bits_res == 0 {
            return Err("GetDIBits failed".to_string());
        }

        // Create DynamicImage from pixels (BGRA -> RGBA)
        // GDI returns BGRA, image crate expects RGBA or we can convert
        // Let's iterate and swap B and R
        for chunk in pixels.chunks_exact_mut(4) {
            let b = chunk[0];
            let r = chunk[2];
            chunk[0] = r;
            chunk[2] = b;
        }

        // Create Rgba8 buffer
        let img_buf =
            image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w as u32, h as u32, pixels)
                .ok_or("Failed to create ImageBuffer")?;

        Ok(DynamicImage::ImageRgba8(img_buf))
    }
}

pub fn capture_all_monitors() -> Result<Vec<DynamicImage>, String> {
    unsafe {
        let mut monitors: Vec<RECT> = Vec::new();
        unsafe extern "system" fn monitor_enum_proc(
            hmonitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            let monitors = &mut *(data.0 as *mut Vec<RECT>);
            let mut info = MONITORINFO::default();
            info.cbSize = mem::size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(hmonitor, &mut info).as_bool() {
                monitors.push(info.rcMonitor);
            }
            true.into()
        }

        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut monitors as *mut Vec<RECT> as isize),
        );

        if monitors.is_empty() {
            return match capture_primary_monitor() {
                Ok(img) => Ok(vec![img]),
                Err(e) => Err(e),
            };
        }

        let hwnd_desktop = GetDesktopWindow();
        let hdc_screen = GetDC(hwnd_desktop);
        if hdc_screen.is_invalid() {
            return Err("Failed to get DC for desktop".to_string());
        }

        let mut images = Vec::new();

        for rect in monitors {
            let w = rect.right - rect.left;
            let h = rect.bottom - rect.top;
            let x = rect.left;
            let y = rect.top;

            if w <= 0 || h <= 0 {
                continue;
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            if hdc_mem.is_invalid() {
                continue;
            }

            let hbm_screen = CreateCompatibleBitmap(hdc_screen, w, h);
            if hbm_screen.is_invalid() {
                DeleteDC(hdc_mem);
                continue;
            }

            let old_obj = SelectObject(hdc_mem, hbm_screen);

            // BitBlt from screen to memory DC
            let bitblt_res = BitBlt(hdc_mem, 0, 0, w, h, hdc_screen, x, y, SRCCOPY);
            if bitblt_res.is_err() {
                SelectObject(hdc_mem, old_obj);
                DeleteObject(hbm_screen);
                DeleteDC(hdc_mem);
                continue;
            }

            let bi = BITMAPINFOHEADER {
                biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };

            let mut pixels: Vec<u8> = vec![0; (w * h * 4) as usize];
            let mut info = BITMAPINFO {
                bmiHeader: bi,
                ..Default::default()
            };

            let get_bits_res = GetDIBits(
                hdc_mem,
                hbm_screen,
                0,
                h as u32,
                Some(pixels.as_mut_ptr() as *mut _),
                &mut info,
                DIB_RGB_COLORS,
            );

            SelectObject(hdc_mem, old_obj);
            DeleteObject(hbm_screen);
            DeleteDC(hdc_mem);

            if get_bits_res == 0 {
                continue;
            }

            for chunk in pixels.chunks_exact_mut(4) {
                let b = chunk[0];
                let r = chunk[2];
                chunk[0] = r;
                chunk[2] = b;
            }

            if let Some(img_buf) =
                image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w as u32, h as u32, pixels)
            {
                images.push(DynamicImage::ImageRgba8(img_buf));
            }
        }

        ReleaseDC(hwnd_desktop, hdc_screen);

        if images.is_empty() {
            Err("Failed to capture any monitors".to_string())
        } else {
            tracing::debug!("[ScreenCapture] Captured {} monitor(s)", images.len());
            Ok(images)
        }
    }
}


pub fn compress_to_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buffer);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality);
    encoder
        .encode_image(img)
        .map_err(|e| format!("JPEG encoding failed: {}", e))?;
    Ok(buffer)
}
