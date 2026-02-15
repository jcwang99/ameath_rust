#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

#[cfg(target_os = "windows")]
pub struct RenderContext {
    hwnd: HWND,
    hdc_screen: HDC,
    hdc_mem: HDC,
    h_bitmap: HBITMAP,
    bits: *mut u8,
    width: i32,
    height: i32,
}

#[cfg(target_os = "windows")]
impl RenderContext {
    pub fn new(hwnd: HWND) -> Self {
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            Self {
                hwnd,
                hdc_screen,
                hdc_mem,
                h_bitmap: HBITMAP(0),
                bits: std::ptr::null_mut(),
                width: 0,
                height: 0,
            }
        }
    }

    pub unsafe fn update(&mut self, data: &[u8], width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }

        // Resize DIB section if dimensions changed
        if self.width != width || self.height != height {
            if self.h_bitmap.0 != 0 {
                let _ = DeleteObject(self.h_bitmap);
            }

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
            self.h_bitmap =
                CreateDIBSection(self.hdc_screen, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                    .expect("Failed to create DIB section");

            SelectObject(self.hdc_mem, self.h_bitmap);
            self.bits = bits as *mut u8;
            self.width = width;
            self.height = height;
        }

        // Copy pixel data
        if !self.bits.is_null() && !data.is_empty() {
            let len = (width * height * 4) as usize;
            std::ptr::copy_nonoverlapping(data.as_ptr(), self.bits, data.len().min(len));
        }

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
            self.hwnd,
            self.hdc_screen,
            None,
            Some(&psize),
            self.hdc_mem,
            Some(&ppt_src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
    }
}

#[cfg(target_os = "windows")]
impl Drop for RenderContext {
    fn drop(&mut self) {
        unsafe {
            if self.h_bitmap.0 != 0 {
                let _ = DeleteObject(self.h_bitmap);
            }
            let _ = DeleteDC(self.hdc_mem);
            ReleaseDC(HWND(0), self.hdc_screen);
        }
    }
}
