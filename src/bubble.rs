use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush,
    GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, ANSI_CHARSET, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER,
    DT_WORDBREAK, FF_SWISS, FW_BOLD, HFONT, NONANTIALIASED_QUALITY, OUT_DEFAULT_PRECIS,
    TRANSPARENT,
};

pub const BASE_BUBBLE_WIDTH: i32 = 180;
pub const BASE_BUBBLE_HEIGHT: i32 = 90;
pub const MAX_BUBBLE_WIDTH: i32 = 300;

pub struct SpeechBubble {
    pub text: String,
    pub show_until: Option<Instant>,
    pub font: Option<HFONT>,
    pub current_width: i32,
    pub current_height: i32,
}

impl SpeechBubble {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            show_until: None,
            font: None,
            current_width: BASE_BUBBLE_WIDTH,
            current_height: BASE_BUBBLE_HEIGHT,
        }
    }

    pub fn show(&mut self, text: &str, duration: Duration, scale: f32) {
        self.text = text.to_string();
        self.show_until = Some(Instant::now() + duration);
        self.calculate_size(scale);
    }

    fn calculate_size(&mut self, scale: f32) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let font_size = (14.0 * scale) as i32;
            let font_name: Vec<u16> = "SimSun".encode_utf16().chain(Some(0)).collect();
            use windows::core::PCWSTR;
            let temp_font = CreateFontW(
                font_size,
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
            );
            let old_font = SelectObject(hdc_mem, temp_font);

            let padding = (12.0 * scale) as i32;
            let max_w = (MAX_BUBBLE_WIDTH as f32 * scale) as i32 - padding * 2;
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: max_w,
                bottom: 1000,
            };

            let mut wide_text: Vec<u16> = self.text.encode_utf16().chain(Some(0)).collect();
            use windows::Win32::Graphics::Gdi::DT_CALCRECT;
            DrawTextW(
                hdc_mem,
                &mut wide_text,
                &mut rect,
                DT_CENTER | DT_WORDBREAK | DT_CALCRECT,
            );

            let text_w = rect.right - rect.left;
            let text_h = rect.bottom - rect.top;

            let tail_h = (20.0 * scale) as i32;
            let calc_w = (text_w + padding * 2).max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);
            let calc_h =
                (text_h + padding * 2 + tail_h).max((BASE_BUBBLE_HEIGHT as f32 * scale) as i32);

            self.current_width = calc_w;
            self.current_height = calc_h;

            SelectObject(hdc_mem, old_font);
            DeleteObject(temp_font);
            DeleteDC(hdc_mem);
            ReleaseDC(HWND(0), hdc_screen);
        }
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
    pub fn render_to_buffer(&mut self, buffer_ptr: *mut u8, scale: f32) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let width = self.current_width;
            let height = self.current_height;

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: 0 - height,
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
            let w_usize = width as usize;
            let h_usize = height as usize;

            let bg_color = [0xE9, 0xDE, 0xFF, 0xFF]; // Soft Purple
            let border_color = [0x69, 0x4B, 0x8A, 0xFF]; // Darker Purple border
            let white_color = [0xFF, 0xFF, 0xFF, 0xFF];

            std::ptr::write_bytes(pixel_ptr, 0, w_usize * h_usize * 4);

            let tail_h = (20.0 * scale) as i32;
            let main_h = height - tail_h;

            for y in 0..height {
                for x in 0..width {
                    let idx = (y * width + x) as usize * 4;
                    let in_rect = x < width && y < main_h;

                    if in_rect {
                        if x == 0 || x == width - 1 || y == 0 || y == main_h - 1 {
                            *pixel_ptr.add(idx) = border_color[0];
                            *pixel_ptr.add(idx + 1) = border_color[1];
                            *pixel_ptr.add(idx + 2) = border_color[2];
                            *pixel_ptr.add(idx + 3) = 255;
                        } else if x == 1 || x == width - 2 || y == 1 || y == main_h - 2 {
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
                    } else if y >= main_h {
                        let center_x = width / 2;
                        let ty = y - main_h;
                        if ty < (15.0 * scale) as i32 {
                            let hw_val = (8.0 * scale) as i32;
                            let half_w = hw_val - (ty / 2);
                            let rel_x = x - center_x;
                            if rel_x.abs() <= half_w {
                                if rel_x.abs() == half_w || ty == ((15.0 * scale) as i32 - 1) {
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
            SetTextColor(hdc_mem, COLORREF(0x004A3B5C));

            let font_size = (14.0 * scale) as i32;
            let _ = self.font; // Silence unused warning if needed, or just remove

            use windows::core::PCWSTR;
            let font_name: Vec<u16> = "SimSun".encode_utf16().chain(Some(0)).collect();
            let temp_font = CreateFontW(
                font_size,
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
            );

            let old_font = SelectObject(hdc_mem, temp_font);

            let padding = (12.0 * scale) as i32;
            let mut rect = RECT {
                left: padding,
                top: padding,
                right: width - padding,
                bottom: main_h - padding,
            };

            let mut wide_text: Vec<u16> = self.text.encode_utf16().chain(Some(0)).collect();
            DrawTextW(hdc_mem, &mut wide_text, &mut rect, DT_CENTER | DT_WORDBREAK);

            GdiFlush();

            // --- Alpha Fix ---
            for i in 0..(w_usize * h_usize) {
                let base = i * 4;
                let b = *pixel_ptr.add(base);
                let g = *pixel_ptr.add(base + 1);
                let r = *pixel_ptr.add(base + 2);
                if (b != 0 || g != 0 || r != 0) && *pixel_ptr.add(base + 3) == 0 {
                    *pixel_ptr.add(base + 3) = 255;
                }
            }

            std::ptr::copy_nonoverlapping(pixel_ptr, buffer_ptr, w_usize * h_usize * 4);

            SelectObject(hdc_mem, old_bitmap);
            SelectObject(hdc_mem, old_font);
            let _ = DeleteObject(temp_font);
            let _ = DeleteObject(h_bitmap);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(HWND(0), hdc_screen);
        }
    }
}
