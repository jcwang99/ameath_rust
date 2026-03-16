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

            let bg_color_val = 0xDCFFFFFF; // Semi-transparent white
            let border_color_val = match self.phase {
                PomodoroPhase::Work => 0xFFFB7299, // Bili Pink
                PomodoroPhase::Rest => 0xFF7BEAF7, // Soft Blue
            };
            let progress_color_val = match self.phase {
                PomodoroPhase::Work => 0x50FB7299, // Lighter Pink
                PomodoroPhase::Rest => 0x507BEAF7, // Soft Blue
            };

            let progress = 1.0 - (self.remaining.as_secs_f32() / self.total_duration.as_secs_f32());
            let fill_width = (width as f32 * progress) as u32;
            let radius = (8.0 * scale) as u32;

            let buffer_u32 = std::slice::from_raw_parts_mut(pixel_ptr as *mut u32, w_usize * h_usize);
            
            // 1. Generate high-quality AA mask for the whole rounded rect
            let mut mask = vec![0u8; (width * height) as usize];
            crate::ui_primitives::draw_rounded_rect_alpha_internal(
                &mut mask, 
                width as u32,
                0, 0, width as u32, height as u32,
                radius,
            );

            // 2. Multi-color Fill with AA
            for y in 0..height {
                for x in 0..width {
                    let idx = (y * width + x) as usize;
                    let edge_alpha = mask[idx] as u32;
                    if edge_alpha == 0 { continue; }

                    let is_fill = x < fill_width as i32;
                    let color_val = if is_fill { progress_color_val } else { bg_color_val };

                    let sa = (color_val >> 24) & 0xFF;
                    let sa = if sa == 0 && (color_val & 0xFFFFFF) != 0 { 255 } else { sa };
                    let effective_a = (sa * edge_alpha) / 255;
                    
                    let r = ((color_val >> 16) & 0xFF) * effective_a / 255;
                    let g = ((color_val >> 8) & 0xFF) * effective_a / 255;
                    let b = (color_val & 0xFF) * effective_a / 255;
                    
                    // Premultiplied BGR for GDI compatibility (bits are [B, G, R, A] in LE)
                    buffer_u32[idx] = (effective_a << 24) | (r << 16) | (g << 8) | b;
                }
            }

            // 3. Draw Border Line (AA)
            // Use border_alpha_internal directly to avoid redundant fills
            let mut border_alpha = vec![0u8; (width * height) as usize];
            crate::ui_primitives::draw_rounded_rect_border_alpha_internal(
                &mut border_alpha, width as u32, width as u32, height as u32, radius, 1
            );
            
            let b_rb = border_color_val & 0x00FF00FF;
            let b_g = border_color_val & 0x0000FF00;
            let b_a = 255u32;

            for i in 0..(w_usize * h_usize) {
                let edge_a = border_alpha[i] as u32;
                if edge_a == 0 { continue; }
                
                let sa = (b_a * edge_a) >> 8;
                let inv_a = 255 - sa;
                let d = buffer_u32[i];
                let d_rb = d & 0x00FF00FF;
                let d_g = d & 0x0000FF00;
                
                let res_rb = (b_rb * sa + d_rb * inv_a) >> 8;
                let res_g = (b_g * sa + d_g * inv_a) >> 8;
                let res_a = sa + (((d >> 24) & 0xFF) * inv_a >> 8);
                
                buffer_u32[i] = (res_a << 24) | (res_rb & 0x00FF00FF) | (res_g & 0x0000FF00);
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

            // Convert any GDI straight alpha / zero-alpha fields to premultiplied
            crate::ui_primitives::premultiply_alpha_buffer(buffer_u32);

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
