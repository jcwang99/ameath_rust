use image::DynamicImage;
use std::mem;

use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
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

pub fn resize_screenshot(img: DynamicImage, max_dim: u32) -> DynamicImage {
    let w = img.width();
    let h = img.height();

    if w <= max_dim && h <= max_dim {
        return img;
    }

    // Resize whilst preserving aspect ratio
    img.resize(max_dim, max_dim, image::imageops::FilterType::Lanczos3)
}
