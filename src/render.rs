#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HDC,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

#[cfg(target_os = "windows")]
pub unsafe fn update_layered_window_scaled(hwnd: HWND, data: &[u8], width: i32, height: i32) {
    let hdc_screen = GetDC(HWND(0));
    let hdc_mem = CreateCompatibleDC(hdc_screen);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits = std::ptr::null_mut();
    // Use hdc_screen for DIB section color table reference if needed
    let h_bitmap = CreateDIBSection(hdc_screen, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        .expect("Failed to create DIB section");

    if !bits.is_null() {
        std::ptr::copy_nonoverlapping(data.as_ptr(), bits as *mut u8, data.len());
    }

    let old_bitmap = SelectObject(hdc_mem, h_bitmap);

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let ppt_src = POINT { x: 0, y: 0 };
    let psize = SIZE {
        cx: width,
        cy: height,
    };

    let _ = UpdateLayeredWindow(
        hwnd,
        hdc_screen,
        None,
        Some(&psize),
        hdc_mem,
        Some(&ppt_src),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );

    SelectObject(hdc_mem, old_bitmap);
    let _ = DeleteObject(h_bitmap);
    let _ = DeleteDC(hdc_mem);
    ReleaseDC(HWND(0), hdc_screen);
}
