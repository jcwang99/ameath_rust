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

pub const POMODORO_WIDTH: i32 = 160;
pub const POMODORO_HEIGHT: i32 = 30;

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
    pub visible: bool,
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
            visible: false,
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

        // Limit tick rate
        if elapsed < Duration::from_millis(100) {
            return None;
        }
        self.last_update = now;

        if self.remaining > elapsed {
            self.remaining -= elapsed;
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

    pub fn render_to_buffer(&mut self, buffer_ptr: *mut u8) {
        #[cfg(target_os = "windows")]
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: POMODORO_WIDTH,
                    biHeight: 0 - POMODORO_HEIGHT,
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
            let width = POMODORO_WIDTH as usize;
            let height = POMODORO_HEIGHT as usize;

            // Clear buffer
            std::ptr::write_bytes(pixel_ptr, 0, width * height * 4);

            // Styling
            let bg_color = [0xFF, 0xFF, 0xFF, 200]; // Semi-transparent white
            let border_color = match self.phase {
                PomodoroPhase::Work => [0xFB, 0x72, 0x99, 255], // Pink
                PomodoroPhase::Rest => [0x7B, 0xEA, 0xF7, 255], // Blue
            };
            let progress_color = match self.phase {
                PomodoroPhase::Work => [0xFB, 0x72, 0x99, 100], // Pink light
                PomodoroPhase::Rest => [0x7B, 0xEA, 0xF7, 100], // Blue light
            };

            let progress = 1.0 - (self.remaining.as_secs_f32() / self.total_duration.as_secs_f32());
            let fill_width = (width as f32 * progress) as usize;

            // Manual Pixel Drawing for Background & Border (Rounded Rect simplified)
            for y in 0..height {
                for x in 0..width {
                    let idx = (y * width + x) * 4;

                    // Simple Border
                    let is_border = x == 0 || x == width - 1 || y == 0 || y == height - 1;

                    if is_border {
                        *pixel_ptr.add(idx) = border_color[2]; // B
                        *pixel_ptr.add(idx + 1) = border_color[1]; // G
                        *pixel_ptr.add(idx + 2) = border_color[0]; // R
                        *pixel_ptr.add(idx + 3) = border_color[3]; // A
                    } else {
                        // Progress Bar Fill
                        let is_fill = x < fill_width;
                        let color = if is_fill { progress_color } else { bg_color };

                        *pixel_ptr.add(idx) = color[2];
                        *pixel_ptr.add(idx + 1) = color[1];
                        *pixel_ptr.add(idx + 2) = color[0];
                        *pixel_ptr.add(idx + 3) = color[3];
                    }
                }
            }

            // --- Text Rendering ---
            SetBkMode(hdc_mem, TRANSPARENT);
            SetTextColor(hdc_mem, COLORREF(0x004A3B5C)); // Dark text

            if self.font.is_none() {
                use windows::core::PCWSTR;
                let font_name: Vec<u16> =
                    "Microsoft YaHei UI".encode_utf16().chain(Some(0)).collect();
                self.font = Some(CreateFontW(
                    16,
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
                right: POMODORO_WIDTH,
                bottom: POMODORO_HEIGHT,
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

            // --- Final Alpha Correctness for Text ---
            // GDI text drawing might not set alpha channel for anti-aliasing pixels correctly if not doing alpha-blend.
            // But we used DrawText on a 32-bit bitmap. The text pixels might have 0 alpha if the text color is 0x00......
            // Actually standard GDI ignores alpha. We need to manually set alpha for non-background pixels where we expect text.
            // HACK: scan buffer, if pixel matches text color roughly, force alpha 255.
            // Better: Init background with alpha, DrawText draws RGB. We assume text is opaque.
            // We can iterate and set alpha=255 for any pixel that differs significantly from bg/fill color?
            // Or just force alpha=255 for everything inside border?
            // The background fill already set alpha=200. Text drawing by GDI usually leaves high byte 0.
            // We need to fix alpha for text pixels.
            for i in 0..(width * height) {
                let base = i * 4;
                // Read current RGB
                let b = *pixel_ptr.add(base);
                let g = *pixel_ptr.add(base + 1);
                let r = *pixel_ptr.add(base + 2);
                let a = *pixel_ptr.add(base + 3);

                // If alpha is 0 (from GDI text drawing?) or we want to ensure text is visible
                // Text color is Dark (0x5C, 0x3B, 0x4A).
                // If pixel is dark, it's likely text.
                if r < 100 && g < 100 && b < 100 {
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
