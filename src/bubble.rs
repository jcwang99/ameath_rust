use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush,
    GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, AC_SRC_ALPHA, AC_SRC_OVER,
    ANSI_CHARSET, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, CLIP_DEFAULT_PRECIS,
    DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER, DT_WORDBREAK, FF_SWISS, FW_BOLD, HFONT,
    NONANTIALIASED_QUALITY, OUT_DEFAULT_PRECIS, TRANSPARENT,
};

pub const BUBBLE_WIDTH: i32 = 180;
pub const BUBBLE_HEIGHT: i32 = 80;

pub struct SpeechBubble {
    pub text: String,
    pub show_until: Option<Instant>,
    pub font: Option<HFONT>,
}

impl SpeechBubble {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            show_until: None,
            font: None,
        }
    }

    pub fn show(&mut self, text: &str, duration: Duration) {
        self.text = text.to_string();
        self.show_until = Some(Instant::now() + duration);
    }

    pub fn hide(&mut self) {
        self.show_until = None;
    }

    pub fn is_visible(&self) -> bool {
        if let Some(until) = self.show_until {
            Instant::now() < until
        } else {
            false
        }
    }

    /// Renders the bubble into a provided BGRA buffer (stride must be width * 4).
    /// Target buffer should be BUBBLE_WIDTH * BUBBLE_HEIGHT * 4.
    pub fn render_to_buffer(&mut self, buffer_ptr: *mut u8) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: BUBBLE_WIDTH,
                    biHeight: 0 - BUBBLE_HEIGHT,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits = std::ptr::null_mut();
            let h_bitmap =
                CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
            let old_bitmap = SelectObject(hdc_mem, h_bitmap);

            // --- Manual Pixel Art Drawing ---
            let pixel_ptr = bits as *mut u8;
            let width = BUBBLE_WIDTH as usize;
            let height = BUBBLE_HEIGHT as usize;

            let bg_color = [0xE9, 0xDE, 0xFF, 0xFF]; // Soft Pink matching hair
            let border_color = [0x69, 0x4B, 0x8A, 0xFF]; // Darker Rose border
            let white_color = [0xFF, 0xFF, 0xFF, 0xFF];

            std::ptr::write_bytes(pixel_ptr, 0, width * height * 4);

            for y in 0..height {
                for x in 0..width {
                    let idx = (y * width + x) * 4;
                    let in_rect = x < width && y < (height - 20);

                    if in_rect {
                        if x == 0 || x == width - 1 || y == 0 || y == height - 21 {
                            *pixel_ptr.add(idx) = border_color[0];
                            *pixel_ptr.add(idx + 1) = border_color[1];
                            *pixel_ptr.add(idx + 2) = border_color[2];
                            *pixel_ptr.add(idx + 3) = 255;
                        } else if x == 1 || x == width - 2 || y == 1 || y == height - 22 {
                            *pixel_ptr.add(idx) = white_color[0];
                            *pixel_ptr.add(idx + 1) = white_color[1];
                            *pixel_ptr.add(idx + 2) = white_color[2];
                            *pixel_ptr.add(idx + 3) = 255;
                        } else {
                            *pixel_ptr.add(idx) = bg_color[0];
                            *pixel_ptr.add(idx + 1) = bg_color[1];
                            *pixel_ptr.add(idx + 2) = bg_color[2];
                            *pixel_ptr.add(idx + 3) = 255;
                        }
                    } else if y >= (height - 20) {
                        let center_x = width / 2;
                        let ty = y as i32 - (height as i32 - 20);
                        if ty < 15 {
                            let half_w = 8 - (ty / 2);
                            let rel_x = x as i32 - center_x as i32;
                            if rel_x.abs() <= half_w {
                                if rel_x.abs() == half_w || ty == 14 {
                                    *pixel_ptr.add(idx) = border_color[0];
                                    *pixel_ptr.add(idx + 1) = border_color[1];
                                    *pixel_ptr.add(idx + 2) = border_color[2];
                                    *pixel_ptr.add(idx + 3) = 255;
                                } else {
                                    *pixel_ptr.add(idx) = bg_color[0];
                                    *pixel_ptr.add(idx + 1) = bg_color[1];
                                    *pixel_ptr.add(idx + 2) = bg_color[2];
                                    *pixel_ptr.add(idx + 3) = 255;
                                }
                            }
                        }
                    }
                }
            }

            // --- Text Rendering ---
            SetBkMode(hdc_mem, TRANSPARENT);
            SetTextColor(hdc_mem, COLORREF(0x004A3B5C)); // Deep Brown/Purple

            if self.font.is_none() {
                use windows::core::PCWSTR;
                let font_name: Vec<u16> = "SimSun".encode_utf16().chain(Some(0)).collect();
                self.font = Some(CreateFontW(
                    14,
                    0,
                    0,
                    0,
                    FW_BOLD.0 as i32,
                    0,
                    0,
                    0,
                    ANSI_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    NONANTIALIASED_QUALITY.0 as u32,
                    (DEFAULT_PITCH.0 | FF_SWISS.0) as u32,
                    PCWSTR(font_name.as_ptr()),
                ));
            }

            if let Some(font) = self.font {
                SelectObject(hdc_mem, font);
            }

            let mut rect = RECT {
                left: 12,
                top: 12,
                right: BUBBLE_WIDTH - 12,
                bottom: BUBBLE_HEIGHT - 30,
            };

            let mut wide_text: Vec<u16> = self.text.encode_utf16().chain(Some(0)).collect();
            DrawTextW(hdc_mem, &mut wide_text, &mut rect, DT_CENTER | DT_WORDBREAK);

            GdiFlush();

            // --- Final Alpha Correctness ---
            for i in 0..(BUBBLE_WIDTH * BUBBLE_HEIGHT) as usize {
                let base = i * 4;
                let b = *pixel_ptr.add(base);
                let g = *pixel_ptr.add(base + 1);
                let r = *pixel_ptr.add(base + 2);
                if (b != 0 || g != 0 || r != 0) && *pixel_ptr.add(base + 3) == 0 {
                    *pixel_ptr.add(base + 3) = 255;
                }
            }

            // Copy to target buffer
            std::ptr::copy_nonoverlapping(pixel_ptr, buffer_ptr, width * height * 4);

            SelectObject(hdc_mem, old_bitmap);
            let _ = DeleteObject(h_bitmap);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(HWND(0), hdc_screen);
        }
    }
}
