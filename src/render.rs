#[cfg(target_os = "windows")]
use crate::types::PreprocessedFrame;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, POINT, SIZE};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HDC,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

#[cfg(target_os = "windows")]
pub unsafe fn update_layered_window(hwnd: HWND, frame: &PreprocessedFrame) {
    let hdc_screen = GetDC(HWND(0));
    let hdc_mem = CreateCompatibleDC(hdc_screen);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: frame.width,
            biHeight: -frame.height, // Negative for top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits = std::ptr::null_mut();
    let h_bitmap = CreateDIBSection(HDC(0), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        .expect("Failed to create DIB section");

    std::ptr::copy_nonoverlapping(frame.data.as_ptr(), bits as *mut u8, frame.data.len());

    let old_bitmap = SelectObject(hdc_mem, h_bitmap);

    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    let ppt_src = POINT { x: 0, y: 0 };
    let psize = SIZE {
        cx: frame.width,
        cy: frame.height,
    };

    let _ = UpdateLayeredWindow(
        hwnd,
        hdc_screen,
        None, // Keep current position
        Some(&psize),
        hdc_mem,
        Some(&ppt_src),
        None,
        Some(&blend),
        ULW_ALPHA,
    );

    SelectObject(hdc_mem, old_bitmap);
    DeleteObject(h_bitmap);
    DeleteDC(hdc_mem);
    ReleaseDC(HWND(0), hdc_screen);
}
