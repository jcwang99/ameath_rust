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
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_ROUNDED_RECT,
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
use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;

pub const BASE_BUBBLE_WIDTH: i32 = 250;
pub const BASE_BUBBLE_HEIGHT: i32 = 60;

pub enum BubbleContent {
    Text(String),
    Image(String), // Path to image
}

struct BubbleRenderRequest {
    content: BubbleContent,
    scale: f32,
}

impl Clone for BubbleContent {
    fn clone(&self) -> Self {
        match self {
            BubbleContent::Text(t) => BubbleContent::Text(t.clone()),
            BubbleContent::Image(p) => BubbleContent::Image(p.clone()),
        }
    }
}

impl Clone for BubbleRenderRequest {
    fn clone(&self) -> Self {
        Self {
            content: self.content.clone(),
            scale: self.scale,
        }
    }
}

struct BubbleRenderResult {
    frames: Vec<(Box<Vec<u8>>, Duration)>, 
    width: i32,
    height: i32,
    render_hash: u64,
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
    pub current_scale: f32,
    pub is_working: bool,
    pub content: BubbleContent,
    pub rect: Option<(i32, i32, i32, i32)>, 
    pub is_hover_recall: bool,

    // Animation state
    pub frames: Vec<(Vec<u8>, Duration)>,
    pub current_frame_idx: usize,
    pub last_frame_time: Instant,
}

impl SpeechBubble {
    pub fn new() -> Self {
        let (tx, rx_worker) = channel::<BubbleRenderRequest>();
        let (tx_worker, rx) = channel::<BubbleRenderResult>();
        let (tx_recycle, rx_recycle) = channel::<Vec<u8>>();

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
            current_scale: 1.0,
            is_working: false,
            content: BubbleContent::Text(String::new()),
            rect: None,
            is_hover_recall: false,
            frames: Vec::new(),
            current_frame_idx: 0,
            last_frame_time: Instant::now(),
        }
    }

    pub fn show(&mut self, text: &str, _duration: Duration, scale: f32) {
        let clean_text = Self::clean_markdown(text);
        if self.text != clean_text {
            self.text = clean_text.clone();
            self.content = BubbleContent::Text(clean_text);
            let chars = self.text.chars().count();
            let dyn_duration = Duration::from_secs(2) + Duration::from_millis((chars * 100) as u64);
            self.show_until = Some(Instant::now() + dyn_duration);
            self.request_render(scale);
        }
    }

    pub fn show_image(&mut self, path: &str, scale: f32) {
        if self.text != path {
            self.text = path.to_string(); 
            self.content = BubbleContent::Image(path.to_string());
            self.show_until = Some(Instant::now() + Duration::from_secs(4));
            self.request_render(scale);
        }
    }

    fn request_render(&mut self, scale: f32) {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        match &self.content {
            BubbleContent::Text(t) => { hasher.write_u8(0); t.hash(&mut hasher); }
            BubbleContent::Image(p) => { hasher.write_u8(1); p.hash(&mut hasher); }
        }
        let hash = hasher.finish();

        if hash == self.last_rendered_hash && (scale - self.current_scale).abs() < 0.001 {
            return;
        }

        let req = BubbleRenderRequest {
            content: self.content.clone(),
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
                if empty_count == 1 { result.push(""); }
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

    pub fn render_to_buffer(&mut self, buffer_ptr: *mut u8, _scale: f32) {
        while let Ok(res) = self.rx.try_recv() {
            self.frames = res.frames.into_iter().map(|(b, d)| (*b, d)).collect();
            self.current_width = res.width;
            self.current_height = res.height;
            self.last_rendered_hash = res.render_hash;
            self.is_working = false;
            self.current_frame_idx = 0;
            self.last_frame_time = Instant::now();
        }

        if self.frames.len() > 1 {
            let now = Instant::now();
            let mut elapsed = now.duration_since(self.last_frame_time);
            
            while elapsed >= self.frames[self.current_frame_idx % self.frames.len()].1.max(Duration::from_millis(10)) {
                let current_delay = self.frames[self.current_frame_idx % self.frames.len()].1.max(Duration::from_millis(10));
                elapsed -= current_delay;
                self.current_frame_idx = (self.current_frame_idx + 1) % self.frames.len();
                self.last_frame_time = now - elapsed; // Step the time forward by the delay
            }
        }

        if !self.frames.is_empty() {
            let pixels = &self.frames[self.current_frame_idx % self.frames.len()].0;
            if !buffer_ptr.is_null() && !pixels.is_empty() {
                unsafe {
                    std::ptr::copy_nonoverlapping(pixels.as_ptr(), buffer_ptr, pixels.len());
                }
            }
        }
    }
    
    pub fn get_rect(&self) -> Option<(i32, i32, i32, i32)> {
        self.rect
    }

    pub fn update_rect(&mut self, x: i32, y: i32, w: u32, h: u32) {
        self.rect = Some((x, y, w as i32, h as i32));
    }

    pub fn pixel_data(&self) -> Option<&Vec<u8>> {
        if self.frames.is_empty() {
            None
        } else {
            Some(&self.frames[self.current_frame_idx % self.frames.len()].0)
        }
    }

    pub fn next_frame_at(&self) -> Instant {
        if self.frames.len() <= 1 {
            return Instant::now() + Duration::from_secs(3600);
        }
        let current_delay = self.frames[self.current_frame_idx % self.frames.len()].1;
        self.last_frame_time + current_delay.max(Duration::from_millis(10))
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
    bitmap_capacity: (i32, i32), 
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
            rx_recycle,
        }
    };

    while let Ok(req) = rx.recv() {
        let mut final_req = req;
        while let Ok(next_req) = rx.try_recv() {
            final_req = next_req;
        }

        #[cfg(target_os = "windows")]
        if let Some(result) = render_bubble_internal(&mut state, &final_req) {
            let _ = tx.send(result);
        }
    }

    #[cfg(target_os = "windows")]
    unsafe {
        if state.h_bitmap.0 != 0 { let _ = DeleteObject(state.h_bitmap); }
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

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let padding = (24.0 * scale).ceil() as i32;
        let max_w_allowed =
            ((screen_w / 2) - padding * 2).max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);

        let tail_h = (20.0 * scale) as i32;
        let mut calc_w = 0;
        let mut calc_h = 0;

        let mut frames_data: Vec<(u32, u32, Vec<u8>, Duration)> = Vec::new();

        match &req.content {
            BubbleContent::Text(text) => {
                let (text_w, text_h) = get_metrics_dw_ex(text, font_size, max_w_allowed as u32, font_family, true, true);
                let width_buffer = (16.0 * scale).ceil() as i32;
                let height_buffer = (32.0 * scale).ceil() as i32;
                calc_w = (text_w as i32 + padding * 2 + width_buffer).max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);
                calc_h = text_h.ceil() as i32 + padding * 2 + tail_h + height_buffer;
                let layout = get_or_create_layout_ex(text, font_size, (calc_w - padding * 2) as u32, font_family, true, true);
                state.cached_layout = Some(layout);
            }
            BubbleContent::Image(path) => {
                state.cached_layout = None;
                
                let trimmed_path = path.trim();
                let lower_path = trimmed_path.to_lowercase();
                
                // 优先从嵌入资源加载
                let embedded_bytes = crate::stickers::get_sticker_bytes(trimmed_path);
                
                // 只要是嵌入资源（目前全是 GIF）或者路径以 .gif 结尾，就尝试动画解码
                let should_try_gif = embedded_bytes.is_some() || lower_path.ends_with(".gif");

                if should_try_gif {
                    let decoded = if let Some(bytes) = embedded_bytes {
                        // 内存读取
                        if let Ok(decoder) = GifDecoder::new(std::io::Cursor::new(bytes)) {
                            decoder.into_frames().collect_frames().ok()
                        } else {
                            None
                        }
                    } else if let Ok(file) = std::fs::File::open(trimmed_path) {
                        // 文件读取
                        if let Ok(decoder) = GifDecoder::new(file) {
                            decoder.into_frames().collect_frames().ok()
                        } else {
                            None
                        }
                    } else {
                        None
                    };

                    if let Some(frames) = decoded {
                        for f in frames {
                            let delay_ms = f.delay().numer_denom_ms().0;
                            let delay = Duration::from_millis(delay_ms as u64);
                            let (fw, fh) = (f.buffer().width(), f.buffer().height());
                            frames_data.push((fw, fh, f.into_buffer().into_raw(), delay));
                        }
                    }
                }

                if frames_data.is_empty() {
                    let img_result = if let Some(bytes) = embedded_bytes {
                        image::load_from_memory(bytes)
                    } else {
                        image::open(&path)
                    };

                    match img_result {
                        Ok(img) => {
                            let rgba = img.to_rgba8();
                            let (iw, ih) = rgba.dimensions();
                            frames_data.push((iw, ih, rgba.into_raw(), Duration::from_secs(1)));
                        }
                        Err(e) => {
                            let fallback = format!("[Image Error: {} | {}]", path, e);
                            let (tw, th) = get_metrics_dw_ex(&fallback, font_size, max_w_allowed as u32, font_family, true, true);
                            calc_w = (tw as i32 + padding * 2).max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);
                            calc_h = th.ceil() as i32 + padding * 2 + tail_h;
                            let layout = get_or_create_layout_ex(&fallback, font_size, (calc_w - padding * 2) as u32, font_family, true, true);
                            state.cached_layout = Some(layout);
                        }
                    }
                }

                if !frames_data.is_empty() {
                    let mut max_img_w = 0.0f32;
                    let mut max_img_h = 0.0f32;
                    for (iw, ih, _, _) in &frames_data {
                        let mut fw = *iw as f32 * scale;
                        let mut fh = *ih as f32 * scale;
                        if fw > max_w_allowed as f32 { let r = max_w_allowed as f32 / fw; fw = max_w_allowed as f32; fh *= r; }
                        let max_h_allowed = 400.0 * scale;
                        if fh > max_h_allowed { let r = max_h_allowed / fh; fh = max_h_allowed; fw *= r; }
                        max_img_w = max_img_w.max(fw);
                        max_img_h = max_img_h.max(fh);
                    }
                    calc_w = (max_img_w as i32 + padding * 2).max((BASE_BUBBLE_WIDTH as f32 * scale) as i32);
                    calc_h = max_img_h as i32 + padding * 2 + tail_h;
                }
            }
        }

        let width = calc_w;
        let height = calc_h;

        if state.h_bitmap.0 == 0 || width > state.bitmap_capacity.0 || height > state.bitmap_capacity.1 {
            if state.h_bitmap.0 != 0 { let _ = DeleteObject(state.h_bitmap); }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width, biHeight: -height, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0, ..Default::default()
                }, ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            state.h_bitmap = CreateDIBSection(state.hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, HANDLE(0), 0).unwrap();
            state.bitmap_capacity = (width, height);
            SelectObject(state.hdc_mem, state.h_bitmap);
        }

        let mut render_frames = Vec::new();
        let frame_count = if frames_data.is_empty() { 1 } else { frames_data.len() };

        for i in 0..frame_count {
            let d2d_factory = get_d2d_factory();
            let dc_rt = if let Some(ref rt) = state.cached_rt { rt.clone() } else {
                let props = D2D1_RENDER_TARGET_PROPERTIES {
                    pixelFormat: D2D1_PIXEL_FORMAT { format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                    ..Default::default()
                };
                let rt = d2d_factory.CreateDCRenderTarget(&props).unwrap();
                state.cached_rt = Some(rt.clone());
                rt
            };

            let rect_gdi = RECT { left: 0, top: 0, right: width, bottom: height };
            if dc_rt.BindDC(state.hdc_mem, &rect_gdi).is_ok() {
                if let Ok(rt) = dc_rt.cast::<ID2D1DeviceContext>() {
                    rt.BeginDraw();
                    rt.Clear(None);

                    let bg_color = D2D1_COLOR_F { r: 1.0, g: 235.0/255.0, b: 240.0/255.0, a: 1.0 };
                    let border_color = D2D1_COLOR_F { r: 1.0, g: 180.0/255.0, b: 190.0/255.0, a: 1.0 };
                    let white_color = D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
                    let text_color = D2D1_COLOR_F { r: 100.0/255.0, g: 60.0/255.0, b: 70.0/255.0, a: 1.0 };

                    let bg_brush = rt.CreateSolidColorBrush(&bg_color, None).unwrap();
                    let border_brush = rt.CreateSolidColorBrush(&border_color, None).unwrap();
                    let white_brush = rt.CreateSolidColorBrush(&white_color, None).unwrap();
                    let text_brush = rt.CreateSolidColorBrush(&text_color, None).unwrap();

                    let radius = 12.0 * scale;
                    let main_h = height - tail_h;

                    let outer = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: 0.0, top: 0.0, right: width as f32, bottom: main_h as f32 }, radiusX: radius, radiusY: radius };
                    rt.FillRoundedRectangle(&outer, &border_brush);
                    let white_rect = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: 1.0, top: 1.0, right: (width-1) as f32, bottom: (main_h-1) as f32 }, radiusX: radius-1.0, radiusY: radius-1.0 };
                    rt.FillRoundedRectangle(&white_rect, &white_brush);
                    let inner = D2D1_ROUNDED_RECT { rect: D2D_RECT_F { left: 2.0, top: 2.0, right: (width-2) as f32, bottom: (main_h-2) as f32 }, radiusX: radius-2.0, radiusY: radius-2.0 };
                    rt.FillRoundedRectangle(&inner, &bg_brush);

                    let tail_w = (20.0 * scale) as f32;
                    let tail_x = (width as f32 - tail_w) / 2.0;
                    let geometry = d2d_factory.CreatePathGeometry().unwrap();
                    let sink = geometry.Open().unwrap();
                    sink.BeginFigure(D2D_POINT_2F { x: tail_x, y: main_h as f32 - 2.0 }, windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_BEGIN_FILLED);
                    sink.AddLine(D2D_POINT_2F { x: tail_x + tail_w, y: main_h as f32 - 2.0 });
                    sink.AddLine(D2D_POINT_2F { x: width as f32 / 2.0, y: height as f32 });
                    sink.EndFigure(windows::Win32::Graphics::Direct2D::Common::D2D1_FIGURE_END_CLOSED);
                    sink.Close().unwrap();
                    rt.FillGeometry(&geometry, &bg_brush, None);
                    rt.DrawGeometry(&geometry, &border_brush, 2.0 * scale, None);

                    if !frames_data.is_empty() {
                        let (fw, fh, raw_pixels, _) = &frames_data[i];
                        let mut bgra = Vec::with_capacity(raw_pixels.len());
                        for chunk in raw_pixels.chunks_exact(4) {
                            let r = chunk[0] as f32; let g = chunk[1] as f32; let b = chunk[2] as f32; let a = chunk[3] as f32 / 255.0;
                            bgra.push((b * a) as u8); bgra.push((g * a) as u8); bgra.push((r * a) as u8); bgra.push(chunk[3]);
                        }
                        let bmp_props = windows::Win32::Graphics::Direct2D::D2D1_BITMAP_PROPERTIES {
                            pixelFormat: D2D1_PIXEL_FORMAT { format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                            dpiX: 96.0, dpiY: 96.0
                        };
                        if let Ok(bmp) = rt.CreateBitmap(windows::Win32::Graphics::Direct2D::Common::D2D_SIZE_U { width: *fw, height: *fh }, Some(bgra.as_ptr() as *const _), *fw * 4, &bmp_props) {
                            let mut dw = *fw as f32 * scale;
                            let mut dh = *fh as f32 * scale;
                            if dw > (width - padding*2) as f32 { let r = (width-padding*2) as f32 / dw; dw = (width-padding*2) as f32; dh *= r; }
                            if dh > (main_h - padding*2) as f32 { let r = (main_h-padding*2) as f32 / dh; dh = (main_h-padding*2) as f32; dw *= r; }
                            let ox = (width as f32 - padding as f32 * 2.0 - dw) / 2.0;
                            let oy = (main_h as f32 - padding as f32 * 2.0 - dh) / 2.0;
                            let dest = D2D_RECT_F { left: padding as f32 + ox, top: padding as f32 + oy, right: padding as f32 + ox + dw, bottom: padding as f32 + oy + dh };
                            rt.DrawBitmap(&bmp, Some(&dest), 1.0, windows::Win32::Graphics::Direct2D::D2D1_BITMAP_INTERPOLATION_MODE_LINEAR, None);
                        }
                    } else if let Some(layout) = &state.cached_layout {
                        rt.DrawTextLayout(D2D_POINT_2F { x: padding as f32, y: padding as f32 }, layout, &text_brush, D2D1_DRAW_TEXT_OPTIONS_ENABLE_COLOR_FONT);
                    }

                    rt.EndDraw(None, None).unwrap();
                }
            }

            GdiFlush();
            let mut bitmap_info: windows::Win32::Graphics::Gdi::BITMAP = std::mem::zeroed();
            windows::Win32::Graphics::Gdi::GetObjectW(state.h_bitmap, std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAP>() as i32, Some(&mut bitmap_info as *mut _ as *mut std::ffi::c_void));
            let pixel_ptr = bitmap_info.bmBits as *mut u8;
            let total_bytes = width as usize * height as usize * 4;
            let mut frame_buffer = vec![0u8; total_bytes];
            if !pixel_ptr.is_null() {
                std::ptr::copy_nonoverlapping(pixel_ptr, frame_buffer.as_mut_ptr(), total_bytes);
            }
            let delay = if frames_data.is_empty() { Duration::from_secs(1) } else { frames_data[i].3 };
            render_frames.push((Box::new(frame_buffer), delay));
        }

        let mut hasher = DefaultHasher::new();
        match &req.content {
            BubbleContent::Text(t) => { hasher.write_u8(0); t.hash(&mut hasher); }
            BubbleContent::Image(p) => { hasher.write_u8(1); p.hash(&mut hasher); }
        }
        let render_hash = hasher.finish();

        Some(BubbleRenderResult { frames: render_frames, width, height, render_hash })
    }
}
