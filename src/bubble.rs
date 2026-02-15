use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use windows::core::ComInterface;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HANDLE, HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_POINT_2F, D2D_RECT_F,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1DeviceContext, ID2D1Factory,
    D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL,
    DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

pub const BASE_BUBBLE_WIDTH: i32 = 250;
pub const BASE_BUBBLE_HEIGHT: i32 = 60;

pub struct SpeechBubble {
    pub text: String,
    pub show_until: Option<Instant>,
    pub current_width: i32,
    pub current_height: i32,
    // Add cache: (pixel_data, width, height, scale)
    cached_bitmap: Option<(Vec<u8>, i32, i32, f32)>,
    #[cfg(target_os = "windows")]
    cached_layout: Option<windows::Win32::Graphics::DirectWrite::IDWriteTextLayout>,
}

#[cfg(target_os = "windows")]
static DWRITE_FACTORY: OnceLock<IDWriteFactory> = OnceLock::new();
#[cfg(target_os = "windows")]
static D2D_FACTORY: OnceLock<ID2D1Factory> = OnceLock::new();

#[cfg(target_os = "windows")]
fn get_dwrite_factory() -> &'static IDWriteFactory {
    DWRITE_FACTORY
        .get_or_init(|| unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).unwrap() })
}

#[cfg(target_os = "windows")]
fn get_d2d_factory() -> &'static ID2D1Factory {
    D2D_FACTORY.get_or_init(|| unsafe {
        D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).unwrap()
    })
}

impl SpeechBubble {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            show_until: None,
            current_width: BASE_BUBBLE_WIDTH,
            current_height: BASE_BUBBLE_HEIGHT,
            cached_bitmap: None,
            #[cfg(target_os = "windows")]
            cached_layout: None,
        }
    }

    pub fn show(&mut self, text: &str, _duration: Duration, scale: f32) {
        // MD Stripping / Basic Clean
        let clean_text = Self::clean_markdown(text);

        if self.text != clean_text {
            self.text = clean_text;
            self.cached_bitmap = None;
        }

        // Adaptive Duration
        // Base 2s + 0.1s per character
        let chars = self.text.chars().count();
        let dyn_duration = Duration::from_secs(2) + Duration::from_millis((chars * 100) as u64);

        self.show_until = Some(Instant::now() + dyn_duration);
        self.calculate_size(scale);
    }

    pub fn keep_alive(&mut self) {
        if let Some(until) = self.show_until {
            if until < Instant::now() + Duration::from_secs(1) {
                self.show_until = Some(Instant::now() + Duration::from_secs(1));
            }
        }
    }

    fn clean_markdown(input: &str) -> String {
        // Core stripper
        let stripped = input.replace("**", "").replace("__", "").replace("`", "");

        // Split into lines, trim each, remove empty, but allow single paragraph breaks
        let mut result = Vec::new();
        let mut empty_count = 0;

        for line in stripped.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                empty_count += 1;
                if empty_count == 1 {
                    result.push(""); // Allow one empty line (paragraph break)
                }
            } else {
                empty_count = 0;
                result.push(trimmed);
            }
        }

        result.join("\n").trim().to_string()
    }

    fn calculate_size(&mut self, scale: f32) {
        #[cfg(target_os = "windows")]
        unsafe {
            // Use Cached DirectWrite Factory
            let dwrite_factory = get_dwrite_factory();

            let font_size = 18.0 * scale;
            let text_format = dwrite_factory
                .CreateTextFormat(
                    windows::core::w!("Segoe UI Emoji"),
                    None,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    font_size,
                    windows::core::w!(""),
                )
                .unwrap();

            text_format
                .SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)
                .unwrap();
            text_format
                .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                .unwrap();

            // Dynamic Sizing based on Screen Metrics
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);

            let padding = (24.0 * scale).ceil() as i32;
            let max_w_allowed =
                ((screen_w / 2) - padding * 2).max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);

            let wide_text: Vec<u16> = self.text.encode_utf16().collect();
            let text_layout = dwrite_factory
                .CreateTextLayout(
                    &wide_text,
                    &text_format,
                    max_w_allowed as f32,
                    screen_h as f32, // Large enough limit
                )
                .unwrap();

            let mut metrics = std::mem::zeroed();
            text_layout.GetMetrics(&mut metrics).unwrap();

            let text_w = metrics.width;
            let text_h = metrics.height;

            let tail_h = (20.0 * scale) as i32;

            // Add buffers and use ceil for safety against floating point truncation
            let width_buffer = (16.0 * scale).ceil() as i32;
            let height_buffer = (32.0 * scale).ceil() as i32;

            let calc_w = (text_w as i32 + padding * 2 + width_buffer)
                .max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);

            let calc_h = text_h.ceil() as i32 + padding * 2 + tail_h + height_buffer;

            // --- CRITICAL: Re-constrain layout to expected interior size for centering ---
            let _ = text_layout.SetMaxWidth((calc_w - padding * 2) as f32);
            let _ = text_layout.SetMaxHeight((calc_h - padding * 2 - tail_h) as f32);

            self.cached_layout = Some(text_layout);

            if self.current_width != calc_w || self.current_height != calc_h {
                self.cached_bitmap = None; // Invalidate cache if size changes
            }
            self.current_width = calc_w;
            self.current_height = calc_h;
        }
    }

    pub fn is_visible(&self) -> bool {
        if let Some(until) = self.show_until {
            Instant::now() < until
        } else {
            false
        }
    }

    pub fn pixel_data(&self) -> Option<&[u8]> {
        self.cached_bitmap
            .as_ref()
            .map(|(data, _, _, _)| data.as_slice())
    }

    pub fn render_to_buffer(&mut self, buffer_ptr: *mut u8, scale: f32) {
        // Check cache
        if let Some((ref data, w, h, s)) = self.cached_bitmap {
            if w == self.current_width && h == self.current_height && (s - scale).abs() < 0.001 {
                if !buffer_ptr.is_null() {
                    unsafe {
                        std::ptr::copy_nonoverlapping(data.as_ptr(), buffer_ptr, data.len());
                    }
                }
                return;
            }
        }

        // Render and cache
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
                CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, HANDLE(0), 0).unwrap();

            if bits.is_null() || h_bitmap.is_invalid() {
                let _ = DeleteDC(hdc_mem);
                ReleaseDC(HWND(0), hdc_screen);
                return;
            }

            let old_bitmap = SelectObject(hdc_mem, h_bitmap);
            let pixel_ptr = bits as *mut u8;
            let w_usize = width as usize;
            let h_usize = height as usize;

            let bg_color_d2d = D2D1_COLOR_F {
                r: 0xE9 as f32 / 255.0,
                g: 0xDE as f32 / 255.0,
                b: 0xFF as f32 / 255.0,
                a: 1.0,
            };
            let border_color_d2d = D2D1_COLOR_F {
                r: 0x69 as f32 / 255.0,
                g: 0x4B as f32 / 255.0,
                b: 0x8A as f32 / 255.0,
                a: 1.0,
            };
            let white_color_d2d = D2D1_COLOR_F {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            };

            std::ptr::write_bytes(pixel_ptr, 0, w_usize * h_usize * 4);

            let tail_h = (20.0 * scale) as i32;
            let main_h = height - tail_h;

            // --- Text and Shape Rendering (Direct2D) ---
            let dwrite_factory = get_dwrite_factory();
            let d2d_factory = get_d2d_factory();

            let font_size = 18.0 * scale;
            let text_format = dwrite_factory
                .CreateTextFormat(
                    windows::core::w!("Segoe UI Emoji"),
                    None,
                    DWRITE_FONT_WEIGHT_BOLD,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    font_size,
                    windows::core::w!(""),
                )
                .unwrap();

            text_format
                .SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)
                .unwrap();
            text_format
                .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)
                .unwrap();

            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                ..Default::default()
            };

            let dc_rt: ID2D1DCRenderTarget = d2d_factory.CreateDCRenderTarget(&props).unwrap();

            // In windows-rs 0.52.0, ID2D1DCRenderTarget inherits from ID2D1RenderTarget methods.
            // If CreateSolidColorBrush isn't found, try ID2D1DeviceContext or call it directly.
            let padding = (24.0 * scale).ceil() as i32;
            let rect_gdi = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: height, // Bind to FULL height including tail
            };

            if dc_rt.BindDC(HDC(hdc_mem.0), &rect_gdi).is_ok() {
                if let Ok(rt) = dc_rt.cast::<ID2D1DeviceContext>() {
                    rt.BeginDraw();

                    // 1. Draw Background
                    let bg_brush = rt.CreateSolidColorBrush(&bg_color_d2d, None).unwrap();
                    let border_brush = rt.CreateSolidColorBrush(&border_color_d2d, None).unwrap();
                    let white_brush = rt.CreateSolidColorBrush(&white_color_d2d, None).unwrap();

                    let full_rect = D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: width as f32,
                        bottom: main_h as f32,
                    };

                    // Draw Outer Border
                    rt.FillRectangle(&full_rect, &border_brush);

                    // Draw White Inner Border
                    let white_rect = D2D_RECT_F {
                        left: 1.0,
                        top: 1.0,
                        right: (width - 1) as f32,
                        bottom: (main_h - 1) as f32,
                    };
                    rt.FillRectangle(&white_rect, &white_brush);

                    // Draw Core Background
                    let core_rect = D2D_RECT_F {
                        left: 2.0,
                        top: 2.0,
                        right: (width - 2) as f32,
                        bottom: (main_h - 2) as f32,
                    };
                    rt.FillRectangle(&core_rect, &bg_brush);

                    // 2. Draw Tail
                    let center_x = width as f32 / 2.0;
                    let hw_val = 8.0 * scale;
                    let ty_val = 15.0 * scale;

                    if let Ok(path) = d2d_factory.CreatePathGeometry() {
                        if let Ok(sink) = path.Open() {
                            sink.BeginFigure(
                                D2D_POINT_2F {
                                    x: center_x - hw_val,
                                    y: main_h as f32 - 1.0,
                                },
                                windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_BEGIN_FILLED,
                            );
                            sink.AddLine(D2D_POINT_2F {
                                x: center_x + hw_val,
                                y: main_h as f32 - 1.0,
                            });
                            sink.AddLine(D2D_POINT_2F {
                                x: center_x,
                                y: (main_h as f32 + ty_val),
                            });
                            sink.EndFigure(
                                windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_END_CLOSED,
                            );
                            sink.Close().unwrap();
                        }
                        rt.FillGeometry(&path, &bg_brush, None);
                        rt.DrawGeometry(&path, &border_brush, 1.0, None);
                    }

                    // 3. Draw Text
                    if let Some(layout) = &self.cached_layout {
                        let text_brush = rt
                            .CreateSolidColorBrush(
                                &D2D1_COLOR_F {
                                    r: 74.0 / 255.0,
                                    g: 59.0 / 255.0,
                                    b: 92.0 / 255.0,
                                    a: 1.0,
                                },
                                None,
                            )
                            .unwrap();

                        rt.DrawTextLayout(
                            D2D_POINT_2F {
                                x: padding as f32,
                                y: padding as f32,
                            },
                            layout,
                            &text_brush,
                            D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
                        );
                    }

                    rt.EndDraw(None, None).unwrap();
                }
            }

            GdiFlush();

            // --- Copy out to buffer AND Cache ---
            let byte_count = w_usize * h_usize * 4;
            if !buffer_ptr.is_null() {
                std::ptr::copy_nonoverlapping(pixel_ptr, buffer_ptr, byte_count);
            }

            // Create Vector for cache
            let mut cache_vec = Vec::with_capacity(byte_count);
            cache_vec.set_len(byte_count);
            std::ptr::copy_nonoverlapping(pixel_ptr, cache_vec.as_mut_ptr(), byte_count);

            self.cached_bitmap = Some((cache_vec, width, height, scale));

            SelectObject(hdc_mem, old_bitmap);
            let _ = DeleteObject(h_bitmap);
            // h_bitmap is actually the return of CreateDIBSection.
            // Wait, in the previous code it was h_bitmap.
            // Actually in windows-rs, CreateDIBSection returns HBITMAP.
            // But let's be careful.
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(HWND(0), hdc_screen);
        }
    }
}
