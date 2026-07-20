use std::sync::OnceLock;
// use windows::core::ComInterface;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, D2D1_FACTORY_TYPE_MULTI_THREADED,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HBITMAP, HDC, HGDIOBJ,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

static DWRITE_FACTORY: OnceLock<IDWriteFactory> = OnceLock::new();
static D2D_FACTORY: OnceLock<ID2D1Factory> = OnceLock::new();

pub fn get_dwrite_factory() -> &'static IDWriteFactory {
    DWRITE_FACTORY
        .get_or_init(|| unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap() })
}

pub fn get_d2d_factory() -> &'static ID2D1Factory {
    D2D_FACTORY.get_or_init(|| unsafe {
        D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, None).unwrap()
    })
}

#[cfg(target_os = "windows")]
pub struct RenderContext {
    hwnd: HWND,
    hdc_screen: HDC,
    hdc_mem: HDC,
    h_bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
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
                old_bitmap: HGDIOBJ(0),
                bits: std::ptr::null_mut(),
                width: 0,
                height: 0,
            }
        }
    }

    pub unsafe fn update(&mut self, data: &[u8], width: i32, height: i32, pos: Option<POINT>) {
        if width <= 0 || height <= 0 {
            return;
        }

        // Resize DIB section if dimensions changed
        if self.width != width || self.height != height {
            if self.h_bitmap.0 != 0 {
                if self.old_bitmap.0 != 0 {
                    let _ = SelectObject(self.hdc_mem, self.old_bitmap);
                }
                let _ = DeleteObject(self.h_bitmap);
                self.h_bitmap = HBITMAP(0);
                self.old_bitmap = HGDIOBJ(0);
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

            self.old_bitmap = SelectObject(self.hdc_mem, self.h_bitmap);
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

        let ppt_dst_ptr = pos.as_ref().map(|p| p as *const POINT);

        let _ = UpdateLayeredWindow(
            self.hwnd,
            self.hdc_screen,
            ppt_dst_ptr,
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
                if self.old_bitmap.0 != 0 {
                    let _ = SelectObject(self.hdc_mem, self.old_bitmap);
                }
                let _ = DeleteObject(self.h_bitmap);
            }
            let _ = DeleteDC(self.hdc_mem);
            ReleaseDC(HWND(0), self.hdc_screen);
        }
    }
}
