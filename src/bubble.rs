use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
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
    ID2D1DCRenderTarget, ID2D1DeviceContext, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_ROUNDED_RECT,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HDC,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};

use crate::render::get_d2d_factory;
use crate::ui_primitives::{get_metrics_dw_ex, get_or_create_layout_ex};

pub const BASE_BUBBLE_WIDTH: i32 = 250;
pub const BASE_BUBBLE_HEIGHT: i32 = 60;

struct BubbleRenderRequest {
    text: String,
    scale: f32,
}

struct BubbleRenderResult {
    pixels: Box<Vec<u8>>, // Use Box to avoid clone cost when sending through channel
    width: i32,
    height: i32,
    text_hash: u64,
}

pub struct SpeechBubble {
    pub text: String,
    pub show_until: Option<Instant>,
    pub current_width: i32,
    pub current_height: i32,

    // Async Worker
    tx: Sender<BubbleRenderRequest>,
    rx: Receiver<BubbleRenderResult>,
    tx_recycle: Sender<Vec<u8>>,

    // Display State
    last_rendered_hash: u64,
    current_pixels: Option<Vec<u8>>,
    current_scale: f32,
    is_working: bool,
}

impl SpeechBubble {
    pub fn new() -> Self {
        let (tx, rx_worker) = channel::<BubbleRenderRequest>();
        let (tx_worker, rx) = channel::<BubbleRenderResult>();
        let (tx_recycle, rx_recycle) = channel::<Vec<u8>>();

        // Spawn independent worker thread
        thread::spawn(move || {
            worker_loop(rx_worker, tx_worker, rx_recycle);
        });

        Self {
            text: String::new(),
            show_until: None,
            current_width: BASE_BUBBLE_WIDTH,
            current_height: BASE_BUBBLE_HEIGHT,
            tx,
            rx,
            tx_recycle,
            last_rendered_hash: 0,
            current_pixels: None,
            current_scale: 1.0,
            is_working: false,
        }
    }

    pub fn show(&mut self, text: &str, _duration: Duration, scale: f32) {
        let clean_text = Self::clean_markdown(text);

        // Only update if text actually changed
        if self.text != clean_text {
            self.text = clean_text;

            // Adaptive Duration
            let chars = self.text.chars().count();
            let dyn_duration = Duration::from_secs(2) + Duration::from_millis((chars * 100) as u64);
            self.show_until = Some(Instant::now() + dyn_duration);

            self.request_render(scale);
        }
    }

    fn request_render(&mut self, scale: f32) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        self.text.hash(&mut hasher);
        let hash = hasher.finish();

        if hash == self.last_rendered_hash && (scale - self.current_scale).abs() < 0.001 {
            return;
        }

        // Send request to worker
        let req = BubbleRenderRequest {
            text: self.text.clone(),
            scale,
        };

        let _ = self.tx.send(req);
        self.current_scale = scale;
        self.is_working = true;
    }

    pub fn keep_alive(&mut self) {
        if let Some(until) = self.show_until {
            if until < Instant::now() + Duration::from_secs(1) {
                self.show_until = Some(Instant::now() + Duration::from_secs(1));
            }
        }
    }

    fn clean_markdown(input: &str) -> String {
        let stripped = input.replace("**", "").replace("__", "").replace("`", "");
        let mut result = Vec::new();
        let mut empty_count = 0;
        for line in stripped.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                empty_count += 1;
                if empty_count == 1 {
                    result.push("");
                }
            } else {
                empty_count = 0;
                result.push(trimmed);
            }
        }
        result.join("\n").trim().to_string()
    }

    pub fn is_visible(&self) -> bool {
        if let Some(until) = self.show_until {
            Instant::now() < until
        } else {
            false
        }
    }

    pub fn pixel_data(&self) -> Option<&[u8]> {
        self.current_pixels.as_deref()
    }

    // Main thread just copies the latest valid buffer
    pub fn render_to_buffer(&mut self, buffer_ptr: *mut u8, _scale: f32) {
        // 1. Check for new results from worker (zero-copy: just move the Box)
        while let Ok(res) = self.rx.try_recv() {
            // Recycle the OLD buffer if we have one
            if let Some(old_pixels) = self.current_pixels.take() {
                let _ = self.tx_recycle.send(old_pixels);
            }

            self.current_pixels = Some(*res.pixels); // Unbox the Vec
            self.current_width = res.width;
            self.current_height = res.height;
            self.last_rendered_hash = res.text_hash;
            self.is_working = false;
        }

        // 2. Blit current valid frame
        if let Some(pixels) = &self.current_pixels {
            if !buffer_ptr.is_null() && pixels.len() > 0 {
                unsafe {
                    std::ptr::copy_nonoverlapping(pixels.as_ptr(), buffer_ptr, pixels.len());
                }
            }
        }
    }
}

// --- Worker Thread Logic ---

struct WorkerState {
    #[cfg(target_os = "windows")]
    cached_layout: Option<IDWriteTextLayout>,
    #[cfg(target_os = "windows")]
    cached_rt: Option<ID2D1DCRenderTarget>,
    #[cfg(target_os = "windows")]
    hdc_mem: HDC,
    #[cfg(target_os = "windows")]
    hdc_screen: HDC,
    #[cfg(target_os = "windows")]
    h_bitmap: windows::Win32::Graphics::Gdi::HBITMAP,
    #[cfg(target_os = "windows")]
    bitmap_capacity: (i32, i32), // Width, Height

    // Recycling
    pixel_buffer: Vec<u8>,
    rx_recycle: Receiver<Vec<u8>>,
}

fn worker_loop(
    rx: Receiver<BubbleRenderRequest>,
    tx: Sender<BubbleRenderResult>,
    rx_recycle: Receiver<Vec<u8>>,
) {
    #[cfg(target_os = "windows")]
    let mut state = unsafe {
        let hdc_screen = GetDC(HWND(0));
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        WorkerState {
            cached_layout: None,
            cached_rt: None,
            hdc_mem,
            hdc_screen,
            h_bitmap: windows::Win32::Graphics::Gdi::HBITMAP(0),
            bitmap_capacity: (0, 0),
            pixel_buffer: Vec::new(),
            rx_recycle,
        }
    };

    while let Ok(req) = rx.recv() {
        let _start = Instant::now();
        // Skip stale requests if channel is backed up
        let mut final_req = req;
        while let Ok(next_req) = rx.try_recv() {
            final_req = next_req;
        }

        #[cfg(target_os = "windows")]
        if let Some(result) = render_bubble_internal(&mut state, &final_req) {
            let _ = tx.send(result);
        }
    }

    // Cleanup
    #[cfg(target_os = "windows")]
    unsafe {
        if state.h_bitmap.0 != 0 {
            let _ = DeleteObject(state.h_bitmap);
        }
        let _ = DeleteDC(state.hdc_mem);
        ReleaseDC(HWND(0), state.hdc_screen);
    }
}

#[cfg(target_os = "windows")]
fn render_bubble_internal(
    state: &mut WorkerState,
    req: &BubbleRenderRequest,
) -> Option<BubbleRenderResult> {
    unsafe {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let scale = req.scale;
        let font_size = 18.0 * scale;
        let font_family = "Segoe UI Emoji";

        // Layout Calculation
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let padding = (24.0 * scale).ceil() as i32;
        let max_w_allowed =
            ((screen_w / 2) - padding * 2).max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);

        let (text_w, text_h) = get_metrics_dw_ex(
            &req.text,
            font_size,
            max_w_allowed as u32,
            font_family,
            true, // bold
            true, // centered
        );

        let tail_h = (20.0 * scale) as i32;
        let width_buffer = (16.0 * scale).ceil() as i32;
        let height_buffer = (32.0 * scale).ceil() as i32;

        let calc_w = (text_w as i32 + padding * 2 + width_buffer)
            .max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);
        let calc_h = text_h.ceil() as i32 + padding * 2 + tail_h + height_buffer;

        // Create Layout
        let layout = get_or_create_layout_ex(
            &req.text,
            font_size,
            (calc_w - padding * 2) as u32,
            font_family,
            true,
            true,
        );
        state.cached_layout = Some(layout);

        // Resize Buffer / Bitmap if needed
        let width = calc_w;
        let height = calc_h;

        // GDI Bitmap Recycling
        // Only recreate if current bitmap is too small or doesn't exist
        if state.h_bitmap.0 == 0
            || width > state.bitmap_capacity.0
            || height > state.bitmap_capacity.1
        {
            if state.h_bitmap.0 != 0 {
                let _ = DeleteObject(state.h_bitmap);
            }

            // Grow capacity with some buffer (e.g., align to 256px or just use current)
            // Using exact size for now, but trailing growth is better.
            // Let's alloc exactly what's needed to start, optimization step 2 could be exponential growth.
            let new_cap_w = width;
            let new_cap_h = height;

            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: new_cap_w,
                    biHeight: 0 - new_cap_h,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };

            let mut bits = std::ptr::null_mut();
            let h_bitmap =
                CreateDIBSection(state.hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, HANDLE(0), 0)
                    .unwrap();

            state.h_bitmap = h_bitmap;
            state.bitmap_capacity = (new_cap_w, new_cap_h);

            // Re-bind to DC
            SelectObject(state.hdc_mem, h_bitmap);
        }

        // We can't easily get the raw pointer again without keeping it,
        // but SelectObject + GetDIBits or just knowing it's bound to hdc_mem is enough for D2D?
        // Actually D2D BindDC writes to the DC's selected bitmap.
        // But we need to Read from it later.
        // CreateDIBSection gave us `bits`.
        // To access `bits` again without storing it, we can use GetObject but storing is better.
        // For now, let's just re-lock or rely on GDI GetDIBits?
        // Wait, the original code used `bits` directly.
        // If we reuse the bitmap, we need that pointer.
        // `SelectObject` returns the old object.
        // Issue: `CreateDIBSection` returns the pointer in `bits`. If we reuse `h_bitmap`, we lose `bits` unless we stored it.
        // However, `BITMAP` struct from `GetObject` contains `bmBits`.

        let mut bitmap_info: windows::Win32::Graphics::Gdi::BITMAP = std::mem::zeroed();
        windows::Win32::Graphics::Gdi::GetObjectW(
            state.h_bitmap,
            std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAP>() as i32,
            Some(&mut bitmap_info as *mut _ as *mut std::ffi::c_void),
        );
        let pixel_ptr = bitmap_info.bmBits as *mut u8;

        let w_usize = width as usize;
        let h_usize = height as usize;
        // Stride usually aligned to 4 bytes, but with 32bpp (4 bytes) it is usually width * 4.
        // Let's use bmWidthBytes if available, otherwise assume packed.

        // Clear used area
        // Note: We only need to clear the area we will convert to our buffer.
        // Since we copy row-by-row or whole block, clearing the whole capacity might be slow.
        // But we are only drawing `width` * `height`.
        // D2D Clear(0) applies to the render target.

        // D2D Rendering
        let d2d_factory = get_d2d_factory();
        let dc_rt = if let Some(ref rt) = state.cached_rt {
            rt.clone()
        } else {
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                ..Default::default()
            };
            let rt = d2d_factory.CreateDCRenderTarget(&props).unwrap();
            state.cached_rt = Some(rt.clone());
            rt
        };

        let rect_gdi = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };

        // Must BindDC every time because we might have drawn elsewhere or size changed?
        // BindDC documentation says it binds the RT to the DC. The DC has the Bitmap selected.
        if dc_rt.BindDC(state.hdc_mem, &rect_gdi).is_ok() {
            if let Ok(rt) = dc_rt.cast::<ID2D1DeviceContext>() {
                rt.BeginDraw();
                rt.Clear(None); // Clear the bound area to transparent

                let bg_color = D2D1_COLOR_F {
                    r: 1.0,
                    g: 235.0 / 255.0,
                    b: 240.0 / 255.0,
                    a: 1.0,
                };
                let border_color = D2D1_COLOR_F {
                    r: 1.0,
                    g: 180.0 / 255.0,
                    b: 190.0 / 255.0,
                    a: 1.0,
                };
                let white_color = D2D1_COLOR_F {
                    r: 1.0,
                    g: 1.0,
                    b: 1.0,
                    a: 1.0,
                };
                let text_color = D2D1_COLOR_F {
                    r: 100.0 / 255.0,
                    g: 60.0 / 255.0,
                    b: 70.0 / 255.0,
                    a: 1.0,
                };

                let bg_brush = rt.CreateSolidColorBrush(&bg_color, None).unwrap();
                let border_brush = rt.CreateSolidColorBrush(&border_color, None).unwrap();
                let white_brush = rt.CreateSolidColorBrush(&white_color, None).unwrap();
                let text_brush = rt.CreateSolidColorBrush(&text_color, None).unwrap();

                let padding = (24.0 * scale).ceil() as i32;
                let radius = 12.0 * scale;
                let main_h = height - tail_h;

                let outer = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 0.0,
                        top: 0.0,
                        right: width as f32,
                        bottom: main_h as f32,
                    },
                    radiusX: radius,
                    radiusY: radius,
                };
                let white_rect = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 1.0,
                        top: 1.0,
                        right: (width - 1) as f32,
                        bottom: (main_h - 1) as f32,
                    },
                    radiusX: radius - 1.0,
                    radiusY: radius - 1.0,
                };
                let inner = D2D1_ROUNDED_RECT {
                    rect: D2D_RECT_F {
                        left: 2.0,
                        top: 2.0,
                        right: (width - 2) as f32,
                        bottom: (main_h - 2) as f32,
                    },
                    radiusX: radius - 2.0,
                    radiusY: radius - 2.0,
                };

                rt.FillRoundedRectangle(&outer, &border_brush);
                rt.FillRoundedRectangle(&white_rect, &white_brush);
                rt.FillRoundedRectangle(&inner, &bg_brush);

                // Tail
                let center_x = width as f32 / 2.0;
                let hw = 8.0 * scale;
                let ty = 15.0 * scale;
                if let Ok(path) = d2d_factory.CreatePathGeometry() {
                    if let Ok(sink) = path.Open() {
                        sink.BeginFigure(
                            D2D_POINT_2F {
                                x: center_x - hw,
                                y: main_h as f32 - 1.0,
                            },
                            windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_BEGIN_FILLED,
                        );
                        sink.AddLine(D2D_POINT_2F {
                            x: center_x + hw,
                            y: main_h as f32 - 1.0,
                        });
                        sink.AddLine(D2D_POINT_2F {
                            x: center_x,
                            y: (main_h as f32 + ty),
                        });
                        sink.EndFigure(
                            windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_END_CLOSED,
                        );
                        sink.Close().unwrap();
                    }
                    rt.FillGeometry(&path, &bg_brush, None);
                    rt.DrawGeometry(&path, &border_brush, 1.0, None);
                }

                // Text
                if let Some(layout) = &state.cached_layout {
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

        // 3. Buffer Recycling
        // Try to get a recycled buffer
        if let Ok(old_buf) = state.rx_recycle.try_recv() {
            state.pixel_buffer = old_buf;
        }

        // Ensure buffer usage size
        let total_bytes = w_usize * h_usize * 4;
        if state.pixel_buffer.len() != total_bytes {
            state.pixel_buffer.resize(total_bytes, 0);
        }

        // Copy from GDI Bitmap to Vec<u8>
        // CRITICAL FIX: The GDI bitmap has a stride based on its CAPACITY, not the current render width.
        // We must copy row-by-row if the width differs, or if using a sub-region.
        if !pixel_ptr.is_null() {
            let cap_w = if state.bitmap_capacity.0 > 0 {
                state.bitmap_capacity.0 as usize
            } else {
                w_usize
            };
            let stride = cap_w * 4; // 32bpp = 4 bytes per pixel. GDI stride is always aligned to 4 bytes.
            let row_bytes = w_usize * 4;

            if stride == row_bytes {
                // Determine if we can just copy linearly
                // This happens if current width == capacity width
                std::ptr::copy_nonoverlapping(
                    pixel_ptr,
                    state.pixel_buffer.as_mut_ptr(),
                    total_bytes,
                );
            } else {
                // Must copy row by row
                let dest_ptr = state.pixel_buffer.as_mut_ptr();
                for y in 0..h_usize {
                    let src_offset = y * stride;
                    let dest_offset = y * row_bytes;
                    std::ptr::copy_nonoverlapping(
                        pixel_ptr.add(src_offset),
                        dest_ptr.add(dest_offset),
                        row_bytes,
                    );
                }
            }
        }

        // Hashing
        let mut hasher = DefaultHasher::new();
        req.text.hash(&mut hasher);
        let text_hash = hasher.finish();

        Some(BubbleRenderResult {
            pixels: Box::new(std::mem::take(&mut state.pixel_buffer)),
            width,
            height,
            text_hash,
        })
    }
}
