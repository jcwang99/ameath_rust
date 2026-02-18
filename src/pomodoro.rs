use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject, DrawTextW, GdiFlush,
    GetDC, ReleaseDC, SelectObject, SetBkMode, SetTextColor, ANSI_CHARSET, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS, DEFAULT_PITCH, DIB_RGB_COLORS, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, FF_SWISS, FW_BOLD, HFONT, NONANTIALIASED_QUALITY,
    OUT_DEFAULT_PRECIS, TRANSPARENT,
};

pub const BASE_POMODORO_WIDTH: i32 = 140;
pub const BASE_POMODORO_HEIGHT: i32 = 40;

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum PomodoroPhase {
    Work,
    Rest,
}

pub struct Pomodoro {
    pub phase: PomodoroPhase,
    pub remaining: Duration,
    pub total_duration: Duration,
    pub is_running: bool,
    pub last_update: Instant,
    pub font: Option<HFONT>,
    pub font_size: i32,
    pub visible: bool,
    pub current_scale: f32,

    // Cached render data
    cached_pixels: Option<Vec<u8>>,
    cached_scale: f32,
    cached_text: String,
    needs_redraw: bool,
}

impl Pomodoro {
    pub fn new() -> Self {
        Self {
            phase: PomodoroPhase::Work,
            remaining: Duration::from_secs(25 * 60),
            total_duration: Duration::from_secs(25 * 60),
            is_running: false,
            last_update: Instant::now(),
            font: None,
            font_size: 0,
            visible: false,
            current_scale: 1.0,
            cached_pixels: None,
            cached_scale: 0.0,
            cached_text: String::new(),
            needs_redraw: true,
        }
    }

    pub fn toggle(&mut self) -> bool {
        if self.is_running {
            self.stop();
            false
        } else {
            self.start();
            true
        }
    }

    pub fn start(&mut self) {
        if !self.is_running {
            self.is_running = true;
            self.visible = true;
            self.last_update = Instant::now();
        }
    }

    pub fn stop(&mut self) {
        self.is_running = false;
        self.visible = false;
        // Reset to default work state when stopped manually
        self.reset();
    }

    pub fn reset(&mut self) {
        self.phase = PomodoroPhase::Work;
        self.remaining = Duration::from_secs(25 * 60);
        self.total_duration = Duration::from_secs(25 * 60);
        self.is_running = false;
        self.visible = false;
    }

    pub fn update(&mut self) -> Option<String> {
        if !self.is_running {
            return None;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);

        // Limit tick rate to 10fps when running (100ms)
        if elapsed < Duration::from_millis(100) {
            return None;
        }
        self.last_update = now;

        if self.remaining > elapsed {
            let old_remaining = self.remaining.as_secs();
            self.remaining -= elapsed;
            // Only mark dirty if seconds changed (not every 100ms)
            if self.remaining.as_secs() != old_remaining {
                self.needs_redraw = true;
            }
            None
        } else {
            // Phase switch
            match self.phase {
                PomodoroPhase::Work => {
                    self.phase = PomodoroPhase::Rest;
                    self.remaining = Duration::from_secs(5 * 60);
                    self.total_duration = Duration::from_secs(5 * 60);
                    Some("Work finished! Take a break.".to_string())
                }
                PomodoroPhase::Rest => {
                    self.phase = PomodoroPhase::Work;
                    self.remaining = Duration::from_secs(25 * 60);
                    self.total_duration = Duration::from_secs(25 * 60);
                    Some("Break finished! Back to work.".to_string())
                }
            }
        }
    }

    pub fn get_text(&self) -> String {
        let secs = self.remaining.as_secs();
        let mins = secs / 60;
        let s = secs % 60;
        let phase_str = match self.phase {
            PomodoroPhase::Work => "Focus",
            PomodoroPhase::Rest => "Rest",
        };
        format!("{} {:02}:{:02}", phase_str, mins, s)
    }

    pub fn render_to_buffer(&mut self, buffer_ptr: *mut u8, scale: f32) {
        let text = self.get_text();
        let width = (BASE_POMODORO_WIDTH as f32 * scale) as usize;
        let height = (BASE_POMODORO_HEIGHT as f32 * scale) as usize;
        let total_bytes = width * height * 4;

        // Check if we can use cached data
        if !self.needs_redraw
            && self.cached_pixels.is_some()
            && (self.cached_scale - scale).abs() < 0.01
            && self.cached_text == text
            && self.cached_pixels.as_ref().unwrap().len() == total_bytes
        {
            // Use cached pixels
            if !buffer_ptr.is_null() {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.cached_pixels.as_ref().unwrap().as_ptr(),
                        buffer_ptr,
                        total_bytes,
                    );
                }
            }
            return;
        }

        // Need to render fresh
        self.needs_redraw = false;
        self.cached_scale = scale;
        self.cached_text = text.clone();

        #[cfg(target_os = "windows")]
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let width = (BASE_POMODORO_WIDTH as f32 * scale) as i32;
            let height = (BASE_POMODORO_HEIGHT as f32 * scale) as i32;

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

            // --- Drawing ---
            let pixel_ptr = bits as *mut u8;
            let w_usize = width as usize;
            let h_usize = height as usize;

            // Clear buffer
            std::ptr::write_bytes(pixel_ptr, 0, w_usize * h_usize * 4);

            // Styling (Bilibili-ish / Pet matching)
            let bg_color = [0xFF, 0xFF, 0xFF, 220]; // Semi-transparent white
            let border_color = match self.phase {
                PomodoroPhase::Work => [0xFB, 0x72, 0x99, 255], // Bili Pink
                PomodoroPhase::Rest => [0x7B, 0xEA, 0xF7, 255], // Soft Blue
            };
            let progress_color = match self.phase {
                PomodoroPhase::Work => [0xFB, 0x72, 0x99, 80], // Lighter Pink
                PomodoroPhase::Rest => [0x7B, 0xEA, 0xF7, 80], // Lighter Blue
            };

            let progress = 1.0 - (self.remaining.as_secs_f32() / self.total_duration.as_secs_f32());
            let fill_width = (width as f32 * progress) as i32;

            let radius = (8.0 * scale) as i32;

            for y in 0..height {
                for x in 0..width {
                    let idx = (y * width + x) as usize * 4;

                    // Rounded corner logic
                    let mut is_inside = true;
                    if x < radius && y < radius {
                        let dx = radius - x;
                        let dy = radius - y;
                        if dx * dx + dy * dy > radius * radius {
                            is_inside = false;
                        }
                    } else if x >= width - radius && y < radius {
                        let dx = x - (width - radius - 1);
                        let dy = radius - y;
                        if dx * dx + dy * dy > radius * radius {
                            is_inside = false;
                        }
                    } else if x < radius && y >= height - radius {
                        let dx = radius - x;
                        let dy = y - (height - radius - 1);
                        if dx * dx + dy * dy > radius * radius {
                            is_inside = false;
                        }
                    } else if x >= width - radius && y >= height - radius {
                        let dx = x - (width - radius - 1);
                        let dy = y - (height - radius - 1);
                        if dx * dx + dy * dy > radius * radius {
                            is_inside = false;
                        }
                    }

                    if !is_inside {
                        continue;
                    }

                    // Simplified: fill first, then draw border line if needed.
                    let is_fill = x < fill_width;
                    let color = if is_fill { progress_color } else { bg_color };

                    *pixel_ptr.add(idx) = color[2];
                    *pixel_ptr.add(idx + 1) = color[1];
                    *pixel_ptr.add(idx + 2) = color[0];
                    *pixel_ptr.add(idx + 3) = color[3];
                }
            }

            // Draw a proper thin border for the rounded shape
            for y in 0..height {
                for x in 0..width {
                    let idx = (y * width + x) as usize * 4;
                    // Check if it's an edge pixel of the rounded shape
                    let mut is_edge = false;
                    let r_sq = (radius * radius) as f32;
                    let r_sq_inner = ((radius - 1) * (radius - 1)) as f32;

                    let dist_sq = if x < radius && y < radius {
                        let dx = radius - x;
                        let dy = radius - y;
                        (dx * dx + dy * dy) as f32
                    } else if x >= width - radius && y < radius {
                        let dx = x - (width - radius - 1);
                        let dy = radius - y;
                        (dx * dx + dy * dy) as f32
                    } else if x < radius && y >= height - radius {
                        let dx = radius - x;
                        let dy = y - (height - radius - 1);
                        (dx * dx + dy * dy) as f32
                    } else if x >= width - radius && y >= height - radius {
                        let dx = x - (width - radius - 1);
                        let dy = y - (height - radius - 1);
                        (dx * dx + dy * dy) as f32
                    } else {
                        -1.0
                    };

                    if dist_sq >= 0.0 {
                        if dist_sq <= r_sq && dist_sq > r_sq_inner {
                            is_edge = true;
                        }
                    } else if x == 0 || x == width - 1 || y == 0 || y == height - 1 {
                        is_edge = true;
                    }

                    if is_edge {
                        *pixel_ptr.add(idx) = border_color[2];
                        *pixel_ptr.add(idx + 1) = border_color[1];
                        *pixel_ptr.add(idx + 2) = border_color[0];
                        *pixel_ptr.add(idx + 3) = border_color[3];
                    }
                }
            }

            // --- Text Rendering ---
            SetBkMode(hdc_mem, TRANSPARENT);
            SetTextColor(hdc_mem, COLORREF(0x004A3B5C)); // Deep Brown

            let target_font_size = (18.0 * scale) as i32;
            if self.font.is_none()
                || self.font_size != target_font_size
                || (self.current_scale - scale).abs() > 0.01
            {
                if let Some(old_f) = self.font {
                    DeleteObject(old_f);
                }
                self.font_size = target_font_size;
                self.current_scale = scale;

                use windows::core::PCWSTR;
                let font_name: Vec<u16> =
                    "Microsoft YaHei UI".encode_utf16().chain(Some(0)).collect();
                self.font = Some(CreateFontW(
                    target_font_size,
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
                left: 0,
                top: 0,
                right: width,
                bottom: height,
            };
            let text_str = self.get_text();
            let mut wide_text: Vec<u16> = text_str.encode_utf16().chain(Some(0)).collect();
            DrawTextW(
                hdc_mem,
                &mut wide_text,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );

            GdiFlush();

            // --- Final Alpha Correctness ---
            for i in 0..(w_usize * h_usize) {
                let base = i * 4;
                let b = *pixel_ptr.add(base);
                let g = *pixel_ptr.add(base + 1);
                let r = *pixel_ptr.add(base + 2);
                let a = *pixel_ptr.add(base + 3);

                // Anti-alias/GDI cleanup
                if (r < 120 && g < 120 && b < 120) && a < 200 {
                    *pixel_ptr.add(base + 3) = 255;
                }
            }

            // Cache the rendered pixels
            let mut cached = Vec::with_capacity(w_usize * h_usize * 4);
            cached.extend_from_slice(std::slice::from_raw_parts(pixel_ptr, w_usize * h_usize * 4));
            self.cached_pixels = Some(cached);

            std::ptr::copy_nonoverlapping(pixel_ptr, buffer_ptr, w_usize * h_usize * 4);

            SelectObject(hdc_mem, old_bitmap);
            let _ = DeleteObject(h_bitmap);
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(HWND(0), hdc_screen);
        }
    }
}
