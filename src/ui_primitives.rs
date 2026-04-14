use crate::render::{get_d2d_factory, get_dwrite_factory};
use rayon::prelude::*;
use rusttype::Font;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};
use windows::Win32::Foundation::{HWND, RECT};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Direct2D::{
    ID2D1DCRenderTarget, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::DirectWrite::{
    IDWriteTextFormat, IDWriteTextLayout, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Threading::{GetCurrentProcess, SetProcessWorkingSetSize};

#[cfg(target_os = "windows")]
pub struct ScratchpadRenderer {
    hdc_mem: HDC,
    h_bitmap: HBITMAP,
    rt: Option<ID2D1DCRenderTarget>,
    width: i32,
    height: i32,
    bits: *mut u32,
}

#[cfg(target_os = "windows")]
impl Drop for ScratchpadRenderer {
    fn drop(&mut self) {
        unsafe {
            if self.h_bitmap.0 != 0 {
                DeleteObject(self.h_bitmap);
            }
            if self.hdc_mem.0 != 0 {
                let _ = DeleteDC(self.hdc_mem);
            }
        }
    }
}

#[cfg(target_os = "windows")]
impl ScratchpadRenderer {
    fn reset(&mut self) {
        unsafe {
            if self.h_bitmap.0 != 0 {
                DeleteObject(self.h_bitmap);
                self.h_bitmap = HBITMAP(0);
            }
            self.width = 0;
            self.height = 0;
            self.bits = std::ptr::null_mut();
        }
    }

    pub fn new() -> Self {
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);
            ReleaseDC(HWND(0), hdc_screen);
            Self {
                hdc_mem,
                h_bitmap: HBITMAP(0),
                rt: None,
                width: 0,
                height: 0,
                bits: std::ptr::null_mut(),
            }
        }
    }

    pub fn prepare(&mut self, tw: i32, th: i32) -> (&ID2D1DCRenderTarget, *mut u32) {
        unsafe {
            // Check if buffer is too large (over 4MB) and needs reset to save memory
            const MAX_BUFFER_SIZE: i32 = 4 * 1024 * 1024 / 4; // 4MB in pixels (4 bytes each)
            let current_size = self.width * self.height;
            if current_size > MAX_BUFFER_SIZE && (tw * th) < current_size / 2 {
                // Reset buffer if it's over 4MB and request is significantly smaller
                self.reset();
            }

            if self.rt.is_none() || self.width < tw || self.height < th {
                // Limit max size to prevent excessive memory usage (max 4096x2048 = 32MB)
                let target_w = tw.max(self.width).max(1024).min(4096);
                let target_h = th.max(self.height).max(512).min(2048);

                if self.h_bitmap.0 != 0 {
                    let _ = DeleteObject(self.h_bitmap);
                }

                let bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: target_w,
                        biHeight: -target_h,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };

                let mut bits = std::ptr::null_mut();
                self.h_bitmap =
                    CreateDIBSection(self.hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
                        .unwrap();
                SelectObject(self.hdc_mem, self.h_bitmap);
                self.bits = bits as *mut u32;
                self.width = target_w;
                self.height = target_h;
                self.rt = None; // Reset Render Target to trigger recreation with new dimensions
            }

            // Try up to 2 times: once with current RT, if it fails due to resource loss, recreate and try once more.
            for attempt in 0..2 {
                if self.rt.is_none() {
                    let d2d_factory = get_d2d_factory();
                    let props = D2D1_RENDER_TARGET_PROPERTIES {
                        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                        ..Default::default()
                    };
                    match d2d_factory.CreateDCRenderTarget(&props) {
                        Ok(rt) => self.rt = Some(rt),
                        Err(e) => {
                            if attempt == 0 {
                                tracing::error!("CRITICAL: Failed to create D2D Render Target: {:?}. Retrying once...", e);
                                continue;
                            } else {
                                panic!("CRITICAL: Failed to create D2D Render Target after retry: {:?}", e);
                            }
                        }
                    }
                }

                let bind_rect = RECT {
                    left: 0,
                    top: 0,
                    right: tw,
                    bottom: th,
                };
                
                // 0x8899000C is D2DERR_RECREATE_TARGET
                // We call BindDC on a temporary reference to avoid holding a borrow 
                // when we might need to reset self.rt below.
                let bind_result = self.rt.as_ref().unwrap().BindDC(self.hdc_mem, &bind_rect);
                
                match bind_result {
                    Ok(_) => return (self.rt.as_ref().unwrap(), self.bits),
                    Err(e) if e.code().0 == 0x8899000Cu32 as i32 => {
                        tracing::warn!("Direct2D Render Target lost (D2DERR_RECREATE_TARGET), recreating... (attempt {})", attempt);
                        self.rt = None; // Now safe to assign because temp borrow in bind_result is gone
                    }
                    Err(e) => {
                        panic!("Direct2D BindDC critical failure: {:?}. Status: {}x{}", e, tw, th);
                    }
                }
            }
            panic!("Failed to recover from Direct2D resource loss after retry.");
        }
    }
}

#[cfg(target_os = "windows")]
thread_local! {
    static SCRATCHPAD: std::cell::RefCell<ScratchpadRenderer> = std::cell::RefCell::new(ScratchpadRenderer::new());
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
pub struct LayoutKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub max_w: u32,
    pub font_family_hash: u64,
    pub is_bold: bool,
    pub is_centered: bool,
    pub is_nowrap: bool,
}

#[derive(Hash, Eq, PartialEq, Clone)]
struct RasterKey {
    layout_key: LayoutKey,
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct FormatKey {
    font_family_hash: u64,
    font_size_bits: u32,
    is_bold: bool,
    is_centered: bool,
}

pub struct RasterEntry {
    pub alpha: Vec<u8>,
    pub tw: i32,
    pub th: i32,
    pub pixel_count: usize,
}

pub struct CacheState<K, V> {
    pub map: HashMap<K, V>,
    pub order: Vec<K>,
    pub total_pixels: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V> CacheState<K, V> {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
            total_pixels: 0,
        }
    }
}

static LAYOUT_CACHE: OnceLock<RwLock<CacheState<LayoutKey, IDWriteTextLayout>>> = OnceLock::new();
static FORMAT_CACHE: OnceLock<RwLock<HashMap<FormatKey, IDWriteTextFormat>>> = OnceLock::new();
static RASTER_CACHE: OnceLock<RwLock<CacheState<RasterKey, Arc<RasterEntry>>>> = OnceLock::new();

#[cfg(target_os = "windows")]
thread_local! {
    static LAST_RASTER_RESULT: std::cell::RefCell<Option<(RasterKey, Arc<RasterEntry>)>> = std::cell::RefCell::new(None);
}

pub fn get_raster_cache() -> &'static RwLock<CacheState<RasterKey, Arc<RasterEntry>>> {
    RASTER_CACHE.get_or_init(|| RwLock::new(CacheState::new()))
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct PrimitiveKey {
    w: u32,
    h: u32,
    r: u32,
}
static PRIMITIVE_CACHE: OnceLock<RwLock<CacheState<PrimitiveKey, Vec<u8>>>> = OnceLock::new();

fn get_layout_cache() -> &'static RwLock<CacheState<LayoutKey, IDWriteTextLayout>> {
    LAYOUT_CACHE.get_or_init(|| RwLock::new(CacheState::new()))
}

fn get_format_cache() -> &'static RwLock<HashMap<FormatKey, IDWriteTextFormat>> {
    FORMAT_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn harvest_memory() {
    if let Some(cache) = LAYOUT_CACHE.get() {
        let mut lock = cache.write().unwrap();
        lock.map.clear();
        lock.order.clear();
        lock.total_pixels = 0;
    }
    if let Some(cache) = RASTER_CACHE.get() {
        let mut lock = cache.write().unwrap();
        // Remove oldest 50% of entries to save memory while keeping some cache
        let len = lock.order.len();
        if len > 10 {
            let to_remove = len / 2;
            for _ in 0..to_remove {
                if let Some(oldest) = lock.order.pop() {
                    if let Some(entry) = lock.map.remove(&oldest) {
                        lock.total_pixels -= entry.pixel_count;
                    }
                }
            }
        }
    }
    if let Some(cache) = PRIMITIVE_CACHE.get() {
        let mut lock = cache.write().unwrap();
        lock.map.clear();
        lock.order.clear();
        lock.total_pixels = 0;
    }
    #[cfg(target_os = "windows")]
    {
        SCRATCHPAD.with(|sp| sp.borrow_mut().reset());
        // Force OS to reclaim unused physical memory
        unsafe {
            let handle = GetCurrentProcess();
            let _ = SetProcessWorkingSetSize(handle, !0, !0);
        }
    }
}

pub fn get_or_create_layout_ex(
    text: &str,
    font_size: f32,
    max_w: u32,
    font_family_name: &str,
    is_bold: bool,
    is_centered: bool,
    is_nowrap: bool,
) -> IDWriteTextLayout {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut text_hasher = DefaultHasher::new();
    text.hash(&mut text_hasher);
    let text_hash = text_hasher.finish();

    let mut family_hasher = DefaultHasher::new();
    font_family_name.hash(&mut family_hasher);
    let font_family_hash = family_hasher.finish();

    let key = LayoutKey {
        text_hash,
        font_size_bits: font_size.to_bits(),
        max_w,
        font_family_hash,
        is_bold,
        is_centered,
        is_nowrap,
    };

    {
        let cache_lock = get_layout_cache();
        // 1. Read lock first for performance
        {
            let cache = cache_lock.read().unwrap();
            if let Some(layout) = cache.map.get(&key) {
                return layout.clone();
            }
        }
    }

    let dwrite_factory = get_dwrite_factory();
    let format_key = FormatKey {
        font_family_hash,
        font_size_bits: font_size.to_bits(),
        is_bold,
        is_centered,
    };

    let text_format = {
        let mut cache = get_format_cache().write().unwrap();
        cache
            .entry(format_key)
            .or_insert_with(|| unsafe {
                let weight = if is_bold {
                    DWRITE_FONT_WEIGHT_BOLD
                } else {
                    DWRITE_FONT_WEIGHT_NORMAL
                };
                let format = dwrite_factory
                    .CreateTextFormat(
                        &windows::core::HSTRING::from(font_family_name),
                        None,
                        weight,
                        DWRITE_FONT_STYLE_NORMAL,
                        DWRITE_FONT_STRETCH_NORMAL,
                        font_size,
                        windows::core::w!("en-us"),
                    )
                    .unwrap();

                if is_centered {
                    let _ = format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
                } else {
                    let _ = format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
                }
                let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
                format
            })
            .clone()
    };

    let layout = unsafe {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let layout = dwrite_factory
            .CreateTextLayout(&wide_text, &text_format, max_w as f32, 1000000.0)
            .unwrap();

        if is_nowrap {
            use windows::Win32::Graphics::DirectWrite::DWRITE_WORD_WRAPPING_NO_WRAP;
            let _ = layout.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
        }
        layout
    };

    {
        let mut cache = get_layout_cache().write().unwrap();
        // Eviction logic
        while cache.map.len() >= 100 {
            if !cache.order.is_empty() {
                let oldest = cache.order.remove(0);
                cache.map.remove(&oldest);
            } else {
                break;
            }
        }
        cache.order.push(key.clone());
        cache.map.insert(key, layout.clone());
        layout
    }
}

fn get_or_create_layout(text: &str, font_size: f32, max_w: u32) -> IDWriteTextLayout {
    get_or_create_layout_ex(text, font_size, max_w, "Microsoft YaHei", false, false, false)
}

pub fn draw_rect(
    buffer: &mut [u32],
    surface_w: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    color: u32,
    max_w: u32,
    max_h: u32,
) {
    let color_a = (color >> 24) & 0xFF;
    let a = if color_a == 0 && (color & 0xFFFFFF) != 0 { 255 } else { color_a };

    let fill_color = if a == 255 {
        (color & 0xFFFFFF) | (0xFF << 24)
    } else if a == 0 {
        0
    } else {
        let r = ((color >> 16) & 0xFF) * a / 255;
        let g = ((color >> 8) & 0xFF) * a / 255;
        let b = (color & 0xFF) * a / 255;
        (a << 24) | (r << 16) | (g << 8) | b
    };

    let start_x = x.max(0);
    let start_y = y.max(0);
    let max_x = (x + width as i32).min(max_w as i32);
    let max_y = (y + height as i32).min(max_h as i32);

    if start_x >= max_x || start_y >= max_y {
        return;
    }

    let start_y_idx = start_y as usize;
    let end_y_idx = max_y as usize;
    let surface_w_usize = surface_w as usize;
    let rect_w = (max_x - start_x) as usize;

    // OPTIMIZATION: Parallelization threshold tuned. 
    if (end_y_idx - start_y_idx) * rect_w > 65536 {
        buffer[start_y_idx * surface_w_usize..end_y_idx * surface_w_usize]
            .par_chunks_mut(surface_w_usize)
            .for_each(|row| {
                row[start_x as usize..max_x as usize].fill(fill_color);
            });
    } else {
        for dy in start_y_idx..end_y_idx {
            let row_start = dy * surface_w_usize + start_x as usize;
            buffer[row_start..row_start + rect_w].fill(fill_color);
        }
    }
}

pub fn apply_opacity(color: u32, opacity: f32) -> u32 {
    let alpha = (opacity * 255.0).clamp(0.0, 255.0) as u32;
    (color & 0xFFFFFF) | (alpha << 24)
}

pub fn draw_rect_alpha(
    buffer: &mut [u32],
    surface_w: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    color: u32,
    alpha: f32,
    max_w: u32,
    max_h: u32,
) {
    let start_x = x.max(0);
    let start_y = y.max(0);
    let max_x = (x + width as i32).min(max_w as i32);
    let max_y = (y + height as i32).min(max_h as i32);

    if start_x >= max_x || start_y >= max_y {
        return;
    }

    let fr = ((color >> 16) & 0xFF) as f32;
    let fg = ((color >> 8) & 0xFF) as f32;
    let fb = (color & 0xFF) as f32;

    let start_y_idx = start_y as usize;
    let end_y_idx = max_y as usize;
    let surface_w_usize = surface_w as usize;

    if start_y_idx < end_y_idx {
        let affected_rows = &mut buffer[start_y_idx * surface_w_usize..end_y_idx * surface_w_usize];
        let rect_w = (max_x - start_x) as usize;

        if (end_y_idx - start_y_idx) * rect_w > 1000000 {
            affected_rows
                .par_chunks_mut(surface_w_usize)
                .for_each(|row| {
                    for cx in start_x..max_x {
                        let bg = row[cx as usize];
                        let br = ((bg >> 16) & 0xFF) as f32;
                        let bg_g = ((bg >> 8) & 0xFF) as f32;
                        let bb = (bg & 0xFF) as f32;
                        let color_sa = (color >> 24) & 0xFF;
                        let rect_alpha = if color_sa == 0 { alpha } else { (color_sa as f32 / 255.0) * alpha };
                        let inv_alpha = 1.0 - rect_alpha;

                        let r = br * inv_alpha + fr * rect_alpha;
                        let g = bg_g * inv_alpha + fg * rect_alpha;
                        let b = bb * inv_alpha + fb * rect_alpha;
                        let out_a = (rect_alpha * 255.0 + (bg >> 24) as f32 * inv_alpha) as u32;

                        row[cx as usize] = (out_a << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                    }
                });
        } else {
            for row in affected_rows.chunks_mut(surface_w_usize) {
                for cx in start_x..max_x {
                    let bg = row[cx as usize];
                    let br = ((bg >> 16) & 0xFF) as f32;
                    let bg_g = ((bg >> 8) & 0xFF) as f32;
                    let bb = (bg & 0xFF) as f32;

                    let color_sa = (color >> 24) & 0xFF;
                    let rect_alpha = if color_sa == 0 { alpha } else { (color_sa as f32 / 255.0) * alpha };
                    let inv_alpha = 1.0 - rect_alpha;

                    let r = br * inv_alpha + fr * rect_alpha;
                    let g = bg_g * inv_alpha + fg * rect_alpha;
                    let b = bb * inv_alpha + fb * rect_alpha;
                    let out_a = (rect_alpha * 255.0 + (bg >> 24) as f32 * inv_alpha) as u32;

                    row[cx as usize] = (out_a << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                }
            }
        }
    }
}

pub fn draw_rounded_rect(
    buffer: &mut [u32],
    surface_w: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    r: u32,
    color: u32,
    max_w: u32,
    max_h: u32,
) {
    if w == 0 || h == 0 {
        return;
    }

    // Limit radius to half of width/height to prevent overlapping corners and overflow
    let r = r.min(w / 2).min(h / 2);

    let key = PrimitiveKey { w, h, r };
    let cache_hit = {
        let cache_lock = PRIMITIVE_CACHE.get_or_init(|| RwLock::new(CacheState::new()));
        let mut cache = cache_lock.write().unwrap();
        if let Some(alpha) = cache.map.get(&key) {
            blit_alpha(buffer, surface_w, x, y, h, alpha, color, max_w, max_h);

            // LRU promotion
            if let Some(pos) = cache.order.iter().position(|k| k == &key) {
                let k = cache.order.remove(pos);
                cache.order.push(k);
            }
            true
        } else {
            false
        }
    };

    if cache_hit {
        return;
    }

    // Rasterize and cache (Alpha only)
    let mut alpha = vec![0u8; (w * h) as usize];
    draw_rounded_rect_alpha_internal(&mut alpha, w, 0, 0, w, h, r);

    // Blit now
    blit_alpha(buffer, surface_w, x, y, h, &alpha, color, max_w, max_h);

    // Update cache with LRU limit
    {
        let cache_lock = PRIMITIVE_CACHE.get_or_init(|| RwLock::new(CacheState::new()));
        let mut cache = cache_lock.write().unwrap();

        while cache.order.len() >= 20 {
            let oldest = cache.order.remove(0);
            cache.map.remove(&oldest);
        }

        cache.order.push(key.clone());
        cache.map.insert(key, alpha);
    }
}

pub fn draw_rounded_rect_with_border(
    buffer: &mut [u32],
    surface_w: u32,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    r: u32,
    fill_color: u32,
    border_color: u32,
    border_thickness: u32,
    max_w: u32,
    max_h: u32,
) {
    if w == 0 || h == 0 {
        return;
    }
    // Draw background
    draw_rounded_rect(buffer, surface_w, x, y, w, h, r, fill_color, max_w, max_h);

    // Draw border
    if border_thickness > 0 {
        let r = r.min(w / 2).min(h / 2);
        let mut alpha = vec![0u8; (w * h) as usize];
        draw_rounded_rect_border_alpha_internal(&mut alpha, w, w, h, r, border_thickness);
        blit_alpha(buffer, surface_w, x, y, h, &alpha, border_color, max_w, max_h);
    }
}

pub fn draw_rounded_rect_border_alpha_internal(
    alpha: &mut [u8],
    surface_w: u32,
    w: u32,
    h: u32,
    r: u32,
    thickness: u32,
) {
    let t = thickness as f32;
    let r_f = r as f32;
    let _r_sq = r_f * r_f;
    let r_inner = (r_f - t).max(0.0);
    let _r_inner_sq = r_inner * r_inner;

    for cy in 0..h {
        let dy = if cy < r {
            (r - cy) as i32
        } else if cy > h - r - 1 {
            (cy - (h - r - 1)) as i32
        } else {
            0
        };

        let row_idx = cy as usize * surface_w as usize;
        let row = &mut alpha[row_idx..row_idx + w as usize];

        if dy == 0 {
            // Vertical middle: draw side borders
            row[0..thickness as usize].fill(255);
            row[w as usize - thickness as usize..w as usize].fill(255);
        } else {
            let dy_f = dy as f32;
            let dy_sq = dy_f * dy_f;
            
            for cx in 0..w {
                let is_left = cx < r;
                let is_right = cx >= w - r;
                
                if is_left || is_right {
                    let dx = if is_left { (r - cx) as i32 } else { (cx - (w - r - 1)) as i32 };
                    let dx_f = dx as f32;
                    let dist = (dx_f * dx_f + dy_sq).sqrt();
                    
                    // Improved AA for smooth corners
                    let outer_coverage = (r_f + 0.5 - dist).clamp(0.0, 1.0);
                    let inner_coverage = (r_inner + 0.5 - dist).clamp(0.0, 1.0);
                    let coverage = (outer_coverage - inner_coverage).clamp(0.0, 1.0);
                    row[cx as usize] = (coverage * 255.0) as u8;
                } else {
                    // Middle horizontal part in top/bottom rounding zones
                    if cy < thickness || cy >= h - thickness {
                        row[cx as usize] = 255;
                    }
                }
            }
        }
    }
}

pub fn draw_circle(
    buffer: &mut [u32],
    surface_w: u32,
    cx: i32,
    cy: i32,
    radius: u32,
    color: u32,
    max_w: u32,
    max_h: u32,
) {
    let r_f = radius as f32;
    let _diameter = radius * 2;
    let start_x = (cx - radius as i32).max(0);
    let start_y = (cy - radius as i32).max(0);
    let end_x = (cx + radius as i32).min(max_w as i32);
    let end_y = (cy + radius as i32).min(max_h as i32);

    if start_x >= end_x || start_y >= end_y {
        return;
    }

    let alpha_base = (color >> 24) & 0xFF;
    let a_fixed = if alpha_base == 0 && (color & 0xFFFFFF) != 0 { 255 } else { alpha_base };
    let rb_src = color & 0x00FF00FF;
    let g_src = color & 0x0000FF00;

    for y in start_y..end_y {
        let dy = (y - cy) as f32;
        let dy_sq = dy * dy;
        let row_idx = y as usize * surface_w as usize;
        
        for x in start_x..end_x {
            let dx = (x - cx) as f32;
            let dist = (dx * dx + dy_sq).sqrt();
            let coverage = (r_f + 0.5 - dist).clamp(0.0, 1.0);
            let alpha = (a_fixed as f32 * coverage) as u32;
            
            if alpha == 0 {
                continue;
            }

            let idx = row_idx + x as usize;
            if alpha == 255 {
                buffer[idx] = (0xFF << 24) | (color & 0xFFFFFF);
            } else {
                let bg = buffer[idx];
                let inv_a = 255 - alpha;
                let rb_dest = bg & 0x00FF00FF;
                let g_dest = bg & 0x0000FF00;
                let rb_res = (rb_src * alpha + rb_dest * inv_a) >> 8;
                let g_res = (g_src * alpha + g_dest * inv_a) >> 8;
                let a_res = alpha + (((bg >> 24) & 0xFF) * inv_a >> 8);
                buffer[idx] = (a_res << 24) | (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00);
            }
        }
    }
}

pub fn blit_alpha(
    buffer: &mut [u32],
    surface_w: u32,
    dest_x: i32,
    dest_y: i32,
    h: u32,
    src_alpha: &[u8],
    color: u32,
    max_w: u32,
    max_h: u32,
) {
    let surface_h = (buffer.len() as u32) / surface_w.max(1);
    let start_y = dest_y.max(0);
    let end_y = (dest_y + h as i32).min(max_h as i32).min(surface_h as i32);
    if start_y >= end_y {
        return;
    }

    let tw = (src_alpha.len() as u32 / h) as i32;
    let start_x = dest_x.max(0);
    let end_x = (dest_x + tw).min(max_w as i32).min(surface_w as i32);
    if start_x >= end_x {
        return;
    }

    let surface_w = surface_w as usize;
    let tw = tw as usize;
    let start_x_u = start_x as usize;
    let end_x_u = end_x as usize;
    let copy_len = end_x_u - start_x_u;

    for y in start_y..end_y {
        let dy = (y - dest_y) as usize;
        let dest_row_base = y as usize * surface_w;
        let src_row_base = dy * tw;
        let x_off = (start_x - dest_x) as usize;

        let src_slice = &src_alpha[src_row_base + x_off..src_row_base + x_off + copy_len];
        let dest_slice =
            &mut buffer[dest_row_base + start_x_u..dest_row_base + start_x_u + copy_len];

        for i in 0..copy_len {
            let edge_alpha = src_slice[i] as u32;
            let d = dest_slice[i];

            // Branchless SIMD-friendly blending
            let rb_dest = d & 0x00FF00FF;
            let g_dest = d & 0x0000FF00;

            let rb_src = color & 0x00FF00FF;
            let g_src = color & 0x0000FF00;

            let color_a = (color >> 24) & 0xFF;
            let sa = if color_a == 0 && color != 0 { 255 } else { color_a };
            let effective_a = (sa * edge_alpha) >> 8;
            let inv_a = 255 - effective_a;
            
            let rb_res = (rb_src * effective_a + rb_dest * inv_a) >> 8;
            let g_res = (g_src * effective_a + g_dest * inv_a) >> 8;
            let a_res = effective_a + (((d >> 24) & 0xFF) * inv_a >> 8);

            dest_slice[i] = (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00) | (a_res << 24);
        }
    }
}

pub fn draw_rounded_rect_alpha_internal(
    alpha: &mut [u8],
    surface_w: u32,
    x_off: u32,
    y_off: u32,
    w: u32,
    h: u32,
    r: u32,
) {
    let start_x = x_off;
    let start_y = y_off;
    let end_x = x_off + w;
    let end_y = y_off + h;

    let r_i32 = r as i32;
    let _r_sq = r_i32 * r_i32;

    for cy in start_y..end_y {
        let cy_local = cy - y_off;
        let dy = if h < 2 * r {
            // For very short windows, transition at midpoint
            if cy_local < h / 2 {
                (r - cy_local) as i32
            } else {
                (cy_local - (h - r - 1)) as i32
            }
        } else {
            if cy_local < r {
                (r - cy_local) as i32
            } else if cy_local > h - r - 1 {
                (cy_local - (h - r - 1)) as i32
            } else {
                0
            }
        };

        let row_idx = cy as usize * surface_w as usize;
        let row = &mut alpha[row_idx..row_idx + surface_w as usize];

        if dy == 0 {
            row[start_x as usize..end_x as usize].fill(255);
        } else {
            let left_r_end = start_x + r;
            let right_r_start = end_x - r;

            // Left corner
            for cx in start_x..left_r_end {
                let dx = (start_x + r) as i32 - cx as i32;
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let coverage = (r as f32 + 0.5 - d).clamp(0.0, 1.0);
                row[cx as usize] = (coverage * 255.0) as u8;
            }
            // Middle
            if left_r_end < right_r_start {
                row[left_r_end as usize..right_r_start as usize].fill(255);
            }
            // Right corner
            for cx in right_r_start..end_x {
                let dx = cx as i32 - (end_x as i32 - r_i32 - 1);
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                let coverage = (r as f32 + 0.5 - d).clamp(0.0, 1.0);
                row[cx as usize] = (coverage * 255.0) as u8;
            }
        }
    }
}

pub fn draw_text(
    buffer: &mut [u32],
    surface_w: u32,
    _fonts: &[&Font],
    text: &str,
    x: i32,
    y: i32,
    font_size: f32,
    color: u32,
) {
    #[cfg(target_os = "windows")]
    {
        let surface_h = (buffer.len() as u32) / surface_w.max(1);
        // Use default family for standard draw_text
        draw_text_dw_ex(
            buffer, surface_w, text, x, y, font_size, color, 1000000, surface_h, 0.0, 0.0, 1000000,
        );
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Fallback for non-windows platforms
        // This is a placeholder and doesn't actually draw anything.
        // Real implementation would use a different text rendering library.
        let _ = (buffer, surface_w, _fonts, text, x, y, font_size, color);
    }
}

#[cfg(target_os = "windows")]
pub fn draw_text_dw_ex_nowrap(
    buffer: &mut [u32],
    surface_w: u32,
    text: &str,
    x: i32,
    y: i32,
    font_size: f32,
    color: u32,
    max_w: u32,
    max_h: u32,
    scroll_offset: f32,
    scroll_x: f32,
    layout_w: u32,
) {
    if text.is_empty() {
        return;
    }

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut text_hasher = DefaultHasher::new();
    text.hash(&mut text_hasher);
    let text_hash = text_hasher.finish();

    let font_family_name = "Microsoft YaHei";
    let mut family_hasher = DefaultHasher::new();
    font_family_name.hash(&mut family_hasher);
    let font_family_hash = family_hasher.finish();

    let layout_key = LayoutKey {
        text_hash,
        font_size_bits: font_size.to_bits(),
        max_w: layout_w,
        font_family_hash,
        is_bold: false,
        is_centered: false,
        is_nowrap: true,
    };

    draw_text_dw_ex_internal(
        buffer,
        surface_w,
        text,
        layout_key,
        x,
        y,
        font_size,
        color,
        max_w,
        max_h,
        scroll_offset,
        scroll_x,
        layout_w,
    );
}

#[cfg(target_os = "windows")]
pub fn draw_text_dw_ex(
    buffer: &mut [u32],
    surface_w: u32,
    text: &str,
    x: i32,
    y: i32,
    font_size: f32,
    color: u32,
    max_w: u32,
    max_h: u32,
    scroll_offset: f32,
    scroll_x: f32,
    layout_w: u32, // Added layout_w
) {
    if text.is_empty() {
        return;
    }

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Calculate layout key for raster cache
    let mut text_hasher = DefaultHasher::new();
    text.hash(&mut text_hasher);
    let text_hash = text_hasher.finish();

    let font_family_name = "Microsoft YaHei";
    let mut family_hasher = DefaultHasher::new();
    font_family_name.hash(&mut family_hasher);
    let font_family_hash = family_hasher.finish();

    let layout_key = LayoutKey {
        text_hash,
        font_size_bits: font_size.to_bits(),
        max_w: layout_w,
        font_family_hash,
        is_bold: false,
        is_centered: false,
        is_nowrap: false,
    };

    draw_text_dw_ex_internal(
        buffer,
        surface_w,
        text,
        layout_key,
        x,
        y,
        font_size,
        color,
        max_w,
        max_h,
        scroll_offset,
        scroll_x,
        layout_w,
    );
}

#[cfg(target_os = "windows")]
#[cfg(target_os = "windows")]
pub fn get_text_width(text: &str, font_size: f32, is_bold: bool) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    unsafe {
        let layout = get_or_create_layout_ex(
            text,
            font_size,
            2000, // Sufficiently large width for measurement
            "Microsoft YaHei",
            is_bold,
            false,
            false,
        );
        let mut metrics = std::mem::zeroed();
        if layout.GetMetrics(&mut metrics).is_ok() {
            metrics.width
        } else {
            0.0
        }
    }
}

fn draw_text_dw_ex_internal(
    buffer: &mut [u32],
    surface_w: u32,
    text: &str,
    layout_key: LayoutKey,
    x: i32,
    y: i32,
    font_size: f32,
    color: u32,
    max_w: u32,
    max_h: u32,
    scroll_offset: f32, // Re-added
    scroll_x: f32,
    layout_w: u32,
) {
    // --- 0. PREPARE KEY ---
    let raster_key = RasterKey { layout_key };

    // --- 1. L1 THREAD-LOCAL CACHE (ZERO LOCKS) ---
    if let Some((k, entry)) = LAST_RASTER_RESULT.with(|r| r.borrow().clone()) {
        if k == raster_key {
            blit_alpha_pixels(
                buffer,
                surface_w,
                x,
                y,
                entry.tw,
                entry.th,
                &entry.alpha,
                color,
                max_w,
                max_h,
                scroll_offset as i32,
                scroll_x as i32,
            );
            return;
        }
    }

    // --- 1. QUICK CACHE CHECK (RASTER_CACHE Read Lock) ---
    {
        if let Some(cache_lock) = RASTER_CACHE.get() {
            let cache = cache_lock.read().unwrap();
            if let Some(entry) = cache.map.get(&raster_key) {
                let entry_cloned = entry.clone();
                blit_alpha_pixels(
                    buffer,
                    surface_w,
                    x,
                    y,
                    entry_cloned.tw,
                    entry_cloned.th,
                    &entry_cloned.alpha,
                    color,
                    max_w,
                    max_h,
                    scroll_offset as i32,
                    scroll_x as i32,
                );
                
                // Update L1
                LAST_RASTER_RESULT.with(|r| {
                    *r.borrow_mut() = Some((raster_key, entry_cloned));
                });
                return;
            }
        }
    }

    unsafe {
        // --- 2. CACHE MISS: Only now we do the heavy work ---
        let layout = get_or_create_layout_ex(
            text,
            font_size,
            layout_w,
            "Microsoft YaHei",
            layout_key.is_bold,
            layout_key.is_centered,
            layout_key.is_nowrap,
        );
        let mut metrics = std::mem::zeroed();
        layout.GetMetrics(&mut metrics).unwrap();

        let is_huge = metrics.height > 1500.0;

        // Target height: if huge, only render the visible window to save massive memory
        // For marquee/scrolling, tw must be the full width of the text layout
        let tw = (metrics.width.ceil() as i32 + 10).min(layout_w as i32 + 10);
        let th = if is_huge {
            // Render viewport-sized chunk (e.g. 1024px or max_h)
            (max_h as i32 + 10).min(2048)
        } else {
            (metrics.height.ceil() as i32 + 2).min(1500)
        };

        SCRATCHPAD.with(|sp| {
            let mut sp = sp.borrow_mut();
            let (rt, scratch_bits) = sp.prepare(tw, th);

            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 0.0,
            }));

            let _r = ((color >> 16) & 0xFF) as f32 / 255.0;
            let _g = ((color >> 8) & 0xFF) as f32 / 255.0;
            let _b = (color & 0xFF) as f32 / 255.0;
            let brush = rt
                .CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }, None) // Always white for Mask
                .unwrap();

            let draw_offset_y = if is_huge { -scroll_offset } else { 0.0 };

            rt.DrawTextLayout(
                windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x: 0.0,
                    y: draw_offset_y,
                },
                &layout,
                &brush,
                windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            rt.EndDraw(None, None).unwrap();

            let pixel_count = (tw * th) as usize;
            let mut captured_alpha = vec![0u8; pixel_count];
            let src_w = sp.width;
            for dy in 0..th {
                let src_off = (dy * src_w) as usize;
                let dest_off = (dy * tw) as usize;
                for dx in 0..tw {
                    let pixel = *scratch_bits.add(src_off + dx as usize);
                    // D2D White on Transparent: the blue channel (or any) represents the mask
                    captured_alpha[dest_off + dx as usize] = (pixel & 0xFF) as u8;
                }
            }

            // Blit to screen
            // If huge, we baked the scroll into DrawTextLayout at y=0 of scratchpad
            // otherwise, it's a static full-rast and we use blit offset
            let final_src_y = if is_huge { 0 } else { scroll_offset as i32 };

            blit_alpha_pixels(
                buffer,
                surface_w,
                x,
                y,
                tw,
                th,
                &captured_alpha,
                color,
                max_w,
                max_h,
                final_src_y,
                scroll_x as i32,
            );

            // ONLY skip cache if it's truly giant to avoid re-rasterizing medium text
            // Also bypass if it's a "huge" scrolled item to avoid stale rendering bug
            if metrics.height < 3000.0 && !is_huge {
                let mut cache = get_raster_cache().write().unwrap();
                // Limit to ~1M pixels (~4MB)
                while cache.total_pixels + pixel_count > 1_000_000 && !cache.order.is_empty() {
                    let oldest = cache.order.remove(0);
                    if let Some(old_entry) = cache.map.remove(&oldest) {
                        cache.total_pixels -= old_entry.pixel_count;
                    }
                }
                cache.order.push(raster_key.clone());
                cache.total_pixels += pixel_count;
                let entry_arc = Arc::new(RasterEntry {
                    alpha: captured_alpha,
                    tw,
                    th,
                    pixel_count,
                });
                
                cache.map.insert(raster_key.clone(), entry_arc.clone());
                
                // Update L1
                LAST_RASTER_RESULT.with(|r| {
                    *r.borrow_mut() = Some((raster_key, entry_arc));
                });
            }
        });
    }
}

pub fn blit_alpha_pixels(
    buffer: &mut [u32],
    surface_w: u32,
    dest_x: i32,
    dest_y: i32,
    tw: i32,
    th: i32,
    src_alpha: &[u8],
    color: u32,
    max_w: u32,
    max_h: u32,
    src_y_off: i32, // These are the SCROLL offsets
    src_x_off: i32,
) {
    let surface_h = (buffer.len() as u32) / surface_w.max(1);

    // Apply scroll offset and clip to visible region (max_w / max_h)
    // src_x_off/src_y_off are likely negative as they represent scrolling the layout *left/up*
    // but the logic here handles them as offsets into the src_alpha mask.
    
    let start_y = dest_y.max(0);
    let end_y = (dest_y + max_h as i32).min(surface_h as i32);
    
    if start_y >= end_y {
        return;
    }

    let surface_w = surface_w as usize;
    let start_x_dest = dest_x.max(0);
    let end_x_dest = (dest_x + max_w as i32).min(surface_w as i32);
    
    if start_x_dest >= end_x_dest {
        return;
    }

    let _tw_u = tw as usize;
    
    let surface_w_usize = surface_w as usize;
    let tw_u = tw as usize;
    let start_y_idx = start_y as usize;
    let end_y_idx = end_y as usize;

    let total_pixels = (end_y - start_y) * (end_x_dest - start_x_dest);

    if total_pixels > 15000 {
        // Use par_chunks_mut to safely and efficiently parallelize mutation of different rows
        buffer[start_y_idx * surface_w_usize..end_y_idx * surface_w_usize]
            .par_chunks_mut(surface_w_usize)
            .enumerate()
            .for_each(|(i, row)| {
                let y = (start_y_idx + i) as i32;
                let dy = y - dest_y;
                let src_y = dy + src_y_off;
                if src_y >= 0 && src_y < th {
                    let src_row_base = src_y as usize * tw_u;
                    for x in start_x_dest..end_x_dest {
                        let dx = x - dest_x;
                        let src_x = dx + src_x_off;
                        if src_x >= 0 && src_x < tw {
                            let s_idx = src_row_base + src_x as usize;
                            let edge_alpha = src_alpha[s_idx] as u32;
                            if edge_alpha == 0 {
                                continue;
                            }

                            let d = row[x as usize];
                            let rb_dest = d & 0x00FF00FF;
                            let g_dest = d & 0x0000FF00;
                            let rb_src = color & 0x00FF00FF;
                            let g_src = color & 0x0000FF00;

                            let color_a = (color >> 24) & 0xFF;
                            let sa = if color_a == 0 && color != 0 { 255 } else { color_a };
                            let effective_a = (sa * edge_alpha) >> 8;
                            let inv_a = 255 - effective_a;

                            let rb_res = (rb_src * effective_a + rb_dest * inv_a) >> 8;
                            let g_res = (g_src * effective_a + g_dest * inv_a) >> 8;
                            let a_res = effective_a + (((d >> 24) & 0xFF) * inv_a >> 8);

                            row[x as usize] =
                                (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00) | (a_res << 24);
                        }
                    }
                }
            });
    } else {
        // Fallback to single-threaded for small areas to avoid Rayon overhead
        for y_idx in start_y_idx..end_y_idx {
            let y = y_idx as i32;
            let dy = y - dest_y;
            let src_y = dy + src_y_off;
            if src_y >= 0 && src_y < th {
                let row_start = y_idx * surface_w_usize;
                let src_row_base = src_y as usize * tw_u;
                for x in start_x_dest..end_x_dest {
                    let dx = x - dest_x;
                    let src_x = dx + src_x_off;
                    if src_x >= 0 && src_x < tw {
                        let s_idx = src_row_base + src_x as usize;
                        let edge_alpha = src_alpha[s_idx] as u32;
                        if edge_alpha == 0 {
                            continue;
                        }

                        let d_idx = row_start + x as usize;
                        let d = buffer[d_idx];
                        let rb_dest = d & 0x00FF00FF;
                        let g_dest = d & 0x0000FF00;
                        let rb_src = color & 0x00FF00FF;
                        let g_src = color & 0x0000FF00;

                        let color_a = (color >> 24) & 0xFF;
                        let sa = if color_a == 0 && color != 0 { 255 } else { color_a };
                        let effective_a = (sa * edge_alpha) >> 8;
                        let inv_a = 255 - effective_a;

                        let rb_res = (rb_src * effective_a + rb_dest * inv_a) >> 8;
                        let g_res = (g_src * effective_a + g_dest * inv_a) >> 8;
                        let a_res = effective_a + (((d >> 24) & 0xFF) * inv_a >> 8);

                        buffer[d_idx] =
                            (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00) | (a_res << 24);
                    }
                }
            }
        }
    }
}

#[inline(always)]
fn blend_row_u8(dest_slice: &mut [u32], src_alpha: &[u8], sr: u32, sg: u32, sb: u32, sa: u32) {
    let len = dest_slice.len();
    let color_v = (sa << 24) | (sr << 16) | (sg << 8) | sb;

    for i in 0..len {
        let a = src_alpha[i] as u32;
        let bg = dest_slice[i];
        let inv_a = 255 - a;

        let rb = bg & 0x00FF00FF;
        let g = bg & 0x0000FF00;

        let rb_res = ((color_v & 0x00FF00FF) * a + rb * inv_a) >> 8;
        let g_res = ((color_v & 0x0000FF00) * a + g * inv_a) >> 8;
        let a_res = (sa * a + (bg >> 24) * inv_a) >> 8;

        dest_slice[i] = (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00) | (a_res << 24);
    }
}

pub fn get_metrics_dw_ex(
    text: &str,
    font_size: f32,
    max_w: u32,
    font_family_name: &str,
    is_bold: bool,
    is_centered: bool,
    is_nowrap: bool,
) -> (f32, f32) {
    #[cfg(target_os = "windows")]
    {
        let layout = get_or_create_layout_ex(
            text,
            font_size,
            max_w,
            font_family_name,
            is_bold,
            is_centered,
            is_nowrap,
        );
        unsafe {
            let mut metrics = std::mem::zeroed();
            layout.GetMetrics(&mut metrics).unwrap();
            (metrics.width, metrics.height)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        (0.0, 0.0)
    }
}

pub fn get_metrics_dw(text: &str, font_size: f32, max_w: u32) -> (f32, f32) {
    get_metrics_dw_ex(text, font_size, max_w, "Microsoft YaHei", false, false, false)
}
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn wrap_text(
    text: &str,
    _fonts: &[&Font],
    scale: rusttype::Scale,
    max_width: u32,
) -> Vec<String> {
    unsafe {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let font_size = scale.x;
        let layout = get_or_create_layout(text, font_size, max_width);

        // Get line metrics to extract individual lines
        let mut line_count = 0;
        let _ = layout.GetLineMetrics(None, &mut line_count);
        let mut line_metrics = vec![std::mem::zeroed(); line_count as usize];
        if line_count > 0 {
            layout
                .GetLineMetrics(Some(&mut line_metrics), &mut line_count)
                .unwrap();
        }

        let mut lines = Vec::new();
        let mut start = 0;
        for lm in line_metrics {
            let end = start + lm.length as usize;
            if end <= wide_text.len() {
                let line_utf16 = &wide_text[start..end];
                lines.push(
                    String::from_utf16_lossy(line_utf16)
                        .trim_end_matches(['\r', '\n'])
                        .to_string(),
                );
            }
            start = end;
        }

        if lines.is_empty() && !text.is_empty() {
            lines.push(text.to_string());
        }
        lines
    }
}

#[cfg(target_os = "windows")]
pub fn get_cursor_index_from_xy(
    text: &str,
    font_size: f32,
    max_width: u32,
    lx: f32,
    ly: f32,
) -> usize {
    if text.is_empty() {
        return 0;
    }
    unsafe {
        let layout = get_or_create_layout(text, font_size, max_width);

        let mut trailing = false.into();
        let mut is_inside = false.into();
        let mut metrics = std::mem::zeroed();
        let _ = layout.HitTestPoint(lx, ly, &mut trailing, &mut is_inside, &mut metrics);

        let mut utf16_target = metrics.textPosition as usize;
        if trailing.as_bool() {
            utf16_target += metrics.length as usize;
        }

        let mut char_idx = 0;
        let mut current_utf16 = 0;
        for c in text.chars() {
            if current_utf16 >= utf16_target {
                break;
            }
            current_utf16 += c.len_utf16();
            char_idx += 1;
        }
        char_idx
    }
}

#[cfg(target_os = "windows")]
pub fn get_xy_from_cursor_index(
    text: &str,
    font_size: f32,
    max_width: u32,
    index: usize,
) -> (f32, f32, f32) {
    if text.is_empty() {
        return (0.0, 0.0, font_size * 1.35);
    }
    unsafe {
        let layout = get_or_create_layout(text, font_size, max_width);
        let mut utf16_pos = 0;
        for (i, c) in text.chars().enumerate() {
            if i == index {
                break;
            }
            utf16_pos += c.len_utf16();
        }

        let mut px = 0.0;
        let mut py = 0.0;
        let mut metrics = std::mem::zeroed();
        let _ = layout.HitTestTextPosition(utf16_pos as u32, false, &mut px, &mut py, &mut metrics);
        (px, py, metrics.height)
    }
}

#[cfg(target_os = "windows")]
pub fn get_selection_rects(
    text: &str,
    font_size: f32,
    max_width: u32,
    start: usize,
    end: usize,
) -> Vec<(f32, f32, f32, f32)> {
    if text.is_empty() || start == end {
        return Vec::new();
    }
    unsafe {
        let layout = get_or_create_layout(text, font_size, max_width);
        let min_idx = start.min(end);
        let max_idx = start.max(end);

        let mut utf16_start = 0;
        let mut utf16_len = 0;
        let mut char_count = 0;
        for c in text.chars() {
            if char_count < min_idx {
                utf16_start += c.len_utf16();
            } else if char_count < max_idx {
                utf16_len += c.len_utf16();
            } else {
                break;
            }
            char_count += 1;
        }

        if utf16_len == 0 {
            return Vec::new();
        }

        let mut count = 0;
        let _ = layout.HitTestTextRange(
            utf16_start as u32,
            utf16_len as u32,
            0.0,
            0.0,
            None,
            &mut count,
        );
        let mut metrics = vec![std::mem::zeroed(); count as usize];
        let _ = layout.HitTestTextRange(
            utf16_start as u32,
            utf16_len as u32,
            0.0,
            0.0,
            Some(&mut metrics),
            &mut count,
        );

        metrics
            .into_iter()
            .map(|m| (m.left, m.top, m.width, m.height))
            .collect()
    }
}

#[cfg(not(target_os = "windows"))]
pub fn wrap_text(
    text: &str,
    _fonts: &[&rusttype::Font],
    _scale: rusttype::Scale,
    _max_width: u32,
) -> Vec<String> {
    text.lines().map(|s| s.to_string()).collect()
}

#[cfg(not(target_os = "windows"))]
pub fn get_cursor_index_from_xy(
    _text: &str,
    _font_size: f32,
    _max_width: u32,
    _lx: f32,
    _ly: f32,
) -> usize {
    0
}

#[cfg(not(target_os = "windows"))]
pub fn get_xy_from_cursor_index(
    _text: &str,
    _font_size: f32,
    _max_width: u32,
    _index: usize,
) -> (f32, f32) {
    (0.0, 0.0)
}

#[cfg(not(target_os = "windows"))]
pub fn get_selection_rects(
    _text: &str,
    _font_size: f32,
    _max_width: u32,
    _start: usize,
    _end: usize,
) -> Vec<(f32, f32, f32, f32)> {
    Vec::new()
}



pub fn draw_triangle(
    buffer: &mut [u32],
    surface_w: u32,
    x: i32,
    y: i32,
    base: u32,
    height: u32,
    color: u32,
    direction_right: bool,
    max_w: u32,
    max_h: u32,
) {
    if base == 0 || height == 0 {
        return;
    }

    let color_a = (color >> 24) & 0xFF;
    let alpha = if color_a == 0 && (color & 0xFFFFFF) != 0 { 255 } else { color_a };
    if alpha == 0 {
        return;
    }
    
    let rb_src = color & 0x00FF00FF;
    let g_src = color & 0x0000FF00;

    let start_y = y.max(0);
    let end_y = (y + height as i32).min(max_h as i32);
    
    let start_x = x.max(0);
    let end_x = (x + base as i32).min(max_w as i32);
    
    if start_y >= end_y || start_x >= end_x { return; }

    let half_h = height as f32 / 2.0;

    for cur_y in start_y..end_y {
        let dy = (cur_y - y) as f32;
        // distance from center Y
        let dist = (dy - half_h).abs();
        
        // Triangle shape: wider at center, narrow at edges. Or inverted depending on direction.
        let ratio = 1.0 - (dist / half_h).clamp(0.0, 1.0);
        let max_w_at_y = (base as f32 * ratio) as i32;
        
        let (row_start_x, row_end_x) = if direction_right {
            // Flat on left |>. X starts at `x`, ends at `x + max_w_at_y`
            (x, x + max_w_at_y)
        } else {
            // Flat on right <|. X starts at `x + base - max_w_at_y`, ends at `x + base`
            (x + base as i32 - max_w_at_y, x + base as i32)
        };
        
        let c_start_x = row_start_x.max(start_x).min(end_x);
        let c_end_x = row_end_x.max(c_start_x).min(end_x);
        
        if c_start_x >= c_end_x { continue; }
        
        let row_idx = cur_y as usize * surface_w as usize;
        for cur_x in c_start_x..c_end_x {
            let idx = row_idx + cur_x as usize;
            if alpha == 255 {
                buffer[idx] = color;
            } else {
                let d = buffer[idx];
                let inv_a = 255 - alpha;
                let rb_dest = d & 0x00FF00FF;
                let g_dest = d & 0x0000FF00;
                let rb_res = (rb_src * alpha + rb_dest * inv_a) >> 8;
                let g_res = (g_src * alpha + g_dest * inv_a) >> 8;
                let a_res = alpha + (((d >> 24) & 0xFF) * inv_a >> 8);
                buffer[idx] = (a_res << 24) | (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00);
            }
        }
    }
}

/// Blits a 32-bit premultiplied pixel buffer onto another.
/// Optimized for asynchronous UI component composition.
pub fn blit_32bit_premultiplied(
    dst: &mut [u32],
    dst_w: u32,
    x: i32,
    y: i32,
    src_w: u32,
    src_h: u32,
    src: &[u32],
    opacity: f32,
    clip_w: u32,
    clip_h: u32,
) {
    if opacity <= 0.0 { return; }
    let global_alpha = (opacity * 255.0) as u32;

    let surface_h = (dst.len() as u32) / dst_w.max(1);
    let start_y = y.max(0);
    let end_y = (y + src_h as i32).min(y + clip_h as i32).min(surface_h as i32);
    let start_x = x.max(0);
    let end_x = (x + src_w as i32).min(x + clip_w as i32).min(dst_w as i32);

    if start_y >= end_y || start_x >= end_x { return; }

    for dy in start_y..end_y {
        let sy = dy - y;
        let dst_row_base = (dy as usize) * (dst_w as usize);
        let src_row_base = (sy as usize) * (src_w as usize);
        
        for dx in start_x..end_x {
            let sx = dx - x;
            let s_pixel = src[src_row_base + sx as usize];
            
            // Extract source components (premultiplied)
            let sa = ((s_pixel >> 24) & 0xFF) * global_alpha >> 8;
            if sa == 0 { continue; }

            let sr = ((s_pixel >> 16) & 0xFF) * global_alpha >> 8;
            let sg = ((s_pixel >> 8) & 0xFF) * global_alpha >> 8;
            let sb = (s_pixel & 0xFF) * global_alpha >> 8;

            let d_pixel = dst[dst_row_base + dx as usize];
            
            if sa >= 255 {
                dst[dst_row_base + dx as usize] = (sa << 24) | (sr << 16) | (sg << 8) | sb;
                continue;
            }

            // Standard premultiplied alpha blending:
            // dest = src + dest * (1 - src_alpha)
            let inv_sa = 255 - sa;
            
            let dr = (d_pixel >> 16) & 0xFF;
            let dg = (d_pixel >> 8) & 0xFF;
            let db = d_pixel & 0xFF;
            let da = (d_pixel >> 24) & 0xFF;

            let r = (sr + (dr * inv_sa >> 8)).min(255);
            let g = (sg + (dg * inv_sa >> 8)).min(255);
            let b = (sb + (db * inv_sa >> 8)).min(255);
            let a = (sa + (da * inv_sa >> 8)).min(255);

            dst[dst_row_base + dx as usize] = (a << 24) | (r << 16) | (g << 8) | b;
        }
    }
}

/// Converts a straight alpha buffer to premultiplied alpha in-place.
pub fn premultiply_alpha_buffer(buffer: &mut [u32]) {
    for pixel in buffer.iter_mut() {
        let color_a = (*pixel >> 24) & 0xFF;
        let a = if color_a == 0 && (*pixel & 0xFFFFFF) != 0 { 255 } else { color_a };
        
        if a == 255 {
            *pixel = (*pixel & 0xFFFFFF) | (0xFF << 24);
            continue;
        }
        if a == 0 {
            *pixel = 0;
            continue;
        }
        let r = ((*pixel >> 16) & 0xFF) * a / 255;
        let g = ((*pixel >> 8) & 0xFF) * a / 255;
        let b = (*pixel & 0xFF) * a / 255;
        *pixel = (a << 24) | (r << 16) | (g << 8) | b;
    }
}
