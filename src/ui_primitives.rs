use crate::render::{get_d2d_factory, get_dwrite_factory};
use rayon::prelude::*;
use rusttype::{Font, Scale};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
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
struct ScratchpadRenderer {
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

    fn new() -> Self {
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

    fn prepare(&mut self, tw: i32, th: i32) -> (&ID2D1DCRenderTarget, *mut u32) {
        unsafe {
            if self.rt.is_none() || self.width < tw || self.height < th {
                let target_w = tw.max(self.width).max(1024);
                let target_h = th.max(self.height).max(512);

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

                if self.rt.is_none() {
                    let d2d_factory = get_d2d_factory();
                    let props = D2D1_RENDER_TARGET_PROPERTIES {
                        r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format:
                                windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        ..Default::default()
                    };
                    self.rt = Some(d2d_factory.CreateDCRenderTarget(&props).unwrap());
                }
            }

            let rt = self.rt.as_ref().unwrap();
            let bind_rect = RECT {
                left: 0,
                top: 0,
                right: tw,
                bottom: th,
            };
            rt.BindDC(self.hdc_mem, &bind_rect).unwrap();
            (rt, self.bits)
        }
    }
}

#[cfg(target_os = "windows")]
thread_local! {
    static SCRATCHPAD: std::cell::RefCell<ScratchpadRenderer> = std::cell::RefCell::new(ScratchpadRenderer::new());
}

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct LayoutKey {
    pub text_hash: u64,
    pub font_size_bits: u32,
    pub max_w: u32,
    pub font_family_hash: u64,
    pub is_bold: bool,
    pub is_centered: bool,
}

#[derive(Hash, PartialEq, Eq, Clone)]
pub struct RasterKey {
    pub layout_key: LayoutKey,
    pub color: u32,
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
static RASTER_CACHE: OnceLock<RwLock<CacheState<RasterKey, RasterEntry>>> = OnceLock::new();

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

pub fn get_raster_cache() -> &'static RwLock<CacheState<RasterKey, RasterEntry>> {
    RASTER_CACHE.get_or_init(|| RwLock::new(CacheState::new()))
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
        lock.map.clear();
        lock.order.clear();
        lock.total_pixels = 0;
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
    };

    {
        let mut cache = get_layout_cache().write().unwrap();
        if cache.map.contains_key(&key) {
            // LRU Promotion
            if let Some(pos) = cache.order.iter().position(|k| k == &key) {
                cache.order.remove(pos);
            }
            cache.order.push(key.clone());
            return cache.map.get(&key).unwrap().clone();
        }

        // Evict if over limit (100 layouts is plenty)
        while cache.order.len() >= 100 {
            let oldest = cache.order.remove(0);
            cache.map.remove(&oldest);
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
                // ALWAYS use NEAR for paragraph alignment to avoid vertical displacement
                // in our large hardcoded layout height (10,000px).
                let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
                format
            })
            .clone()
    };

    unsafe {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let layout = dwrite_factory
            .CreateTextLayout(&wide_text, &text_format, max_w as f32, 10000.0)
            .unwrap();

        let mut cache = get_layout_cache().write().unwrap();
        // Eviction logic
        if cache.map.len() >= 100 {
            if !cache.order.is_empty() {
                let oldest = cache.order.remove(0);
                cache.map.remove(&oldest);
            }
        }
        cache.order.push(key.clone());
        cache.map.insert(key, layout.clone());
        layout
    }
}

fn get_or_create_layout(text: &str, font_size: f32, max_w: u32) -> IDWriteTextLayout {
    get_or_create_layout_ex(text, font_size, max_w, "Microsoft YaHei", false, false)
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

    // OPTIMIZATION: Use parallel chunks only for large enough rects
    if (end_y_idx - start_y_idx) * rect_w > 16384 {
        buffer[start_y_idx * surface_w_usize..end_y_idx * surface_w_usize]
            .par_chunks_mut(surface_w_usize)
            .for_each(|row| {
                row[start_x as usize..max_x as usize].fill(color);
            });
    } else {
        for dy in start_y_idx..end_y_idx {
            let row_start = dy * surface_w_usize + start_x as usize;
            buffer[row_start..row_start + rect_w].fill(color);
        }
    }
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

        if (end_y_idx - start_y_idx) * rect_w > 10000 {
            affected_rows
                .par_chunks_mut(surface_w_usize)
                .for_each(|row| {
                    for cx in start_x..max_x {
                        let bg = row[cx as usize];
                        let br = ((bg >> 16) & 0xFF) as f32;
                        let bg_g = ((bg >> 8) & 0xFF) as f32;
                        let bb = (bg & 0xFF) as f32;

                        let r = br * (1.0 - alpha) + fr * alpha;
                        let g = bg_g * (1.0 - alpha) + fg * alpha;
                        let b = bb * (1.0 - alpha) + fb * alpha;

                        row[cx as usize] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                    }
                });
        } else {
            for row in affected_rows.chunks_mut(surface_w_usize) {
                for cx in start_x..max_x {
                    let bg = row[cx as usize];
                    let br = ((bg >> 16) & 0xFF) as f32;
                    let bg_g = ((bg >> 8) & 0xFF) as f32;
                    let bb = (bg & 0xFF) as f32;

                    let r = br * (1.0 - alpha) + fr * alpha;
                    let g = bg_g * (1.0 - alpha) + fg * alpha;
                    let b = bb * (1.0 - alpha) + fb * alpha;

                    row[cx as usize] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
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

        while cache.order.len() >= 50 {
            let oldest = cache.order.remove(0);
            cache.map.remove(&oldest);
        }

        cache.order.push(key.clone());
        cache.map.insert(key, alpha);
    }
}

fn blit_alpha(
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
            let a = src_slice[i] as u32;
            let d = dest_slice[i];

            // Branchless SIMD-friendly blending
            let rb_dest = d & 0x00FF00FF;
            let g_dest = d & 0x0000FF00;

            let rb_src = color & 0x00FF00FF;
            let g_src = color & 0x0000FF00;

            let inv_a = 255 - a;
            let rb_res = (rb_src * a + rb_dest * inv_a) >> 8;
            let g_res = (g_src * a + g_dest * inv_a) >> 8;

            dest_slice[i] = (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00);
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
    let r_sq = r_i32 * r_i32;

    for cy in start_y..end_y {
        let dy = if (cy - y_off) < r {
            (r - (cy - y_off)) as i32
        } else if (cy - y_off) > h - r - 1 {
            ((cy - y_off) - (h - r - 1)) as i32
        } else {
            0
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
                if dx * dx + dy * dy <= r_sq {
                    row[cx as usize] = 255;
                }
            }
            // Middle
            if left_r_end < right_r_start {
                row[left_r_end as usize..right_r_start as usize].fill(255);
            }
            // Right corner
            for cx in right_r_start..end_x {
                let dx = cx as i32 - (end_x - r - 1) as i32;
                if dx * dx + dy * dy <= r_sq {
                    row[cx as usize] = 255;
                }
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
            buffer, surface_w, text, x, y, font_size, color, 10000, surface_h, 0.0,
        );
    }
}

#[cfg(target_os = "windows")]
pub fn draw_text_dw_h(
    buffer: &mut [u32],
    surface_w: u32,
    text: &str,
    text_hash: u64,
    x: i32,
    y: i32,
    font_size: f32,
    color: u32,
    max_w: u32,
    max_h: u32,
    scroll_offset: f32,
) {
    if text.is_empty() {
        return;
    }

    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let font_family_name = "Microsoft YaHei";
    let mut family_hasher = DefaultHasher::new();
    font_family_name.hash(&mut family_hasher);
    let font_family_hash = family_hasher.finish();

    let layout_key = LayoutKey {
        text_hash,
        font_size_bits: font_size.to_bits(),
        max_w,
        font_family_hash,
        is_bold: false,
        is_centered: false,
    };
    let key = RasterKey {
        layout_key: layout_key.clone(),
        color,
    };

    // Fast path: Raster Cache (Read-only first)
    let found_and_blit = {
        let cache = get_raster_cache().read().unwrap();
        if let Some(entry) = cache.map.get(&key) {
            blit_alpha_pixels(
                buffer,
                surface_w,
                x,
                y,
                entry.tw,
                entry.th,
                &entry.alpha,
                color,
                x.max(0) as u32 + max_w,
                y.max(0) as u32 + max_h,
                -(scroll_offset as i32),
            );
            true
        } else {
            false
        }
    };

    if found_and_blit {
        // Handle LRU promotion periodically or in a deferred way?
        // For now, let's at least avoid the write lock if we just need to blit.
        // To keep LRU perfectly accurate, we DO need a write, but maybe we can skip it 90% of the time.
        return;
    }

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
    scroll_offset: f32, // Logical pixels
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

    draw_text_dw_h(
        buffer,
        surface_w,
        text,
        text_hash,
        x,
        y,
        font_size,
        color,
        max_w,
        max_h,
        scroll_offset,
    );
}

#[cfg(target_os = "windows")]
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
    scroll_offset: f32,
) {
    unsafe {
        let layout = get_or_create_layout_ex(
            text,
            font_size,
            max_w,
            "Microsoft YaHei",
            layout_key.is_bold,
            layout_key.is_centered,
        );
        let mut metrics = std::mem::zeroed();
        layout.GetMetrics(&mut metrics).unwrap();

        let is_huge = metrics.height > 2500.0;

        // Target height: if huge, only render the visible window to save massive memory
        let tw = (metrics.width.ceil() as i32 + 10).min(max_w as i32 + 10);
        let th = if is_huge {
            // Render viewport-sized chunk (e.g. 1024px)
            1024.min(max_h as i32 + 2)
        } else {
            (metrics.height.ceil() as i32 + 2).min(2048)
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

            let r = ((color >> 16) & 0xFF) as f32 / 255.0;
            let g = ((color >> 8) & 0xFF) as f32 / 255.0;
            let b = (color & 0xFF) as f32 / 255.0;
            let brush = rt
                .CreateSolidColorBrush(&D2D1_COLOR_F { r, g, b, a: 1.0 }, None)
                .unwrap();

            let draw_offset_y = if is_huge {
                // For "huge" text, we draw a viewport-sized chunk.
                // scroll_offset is usually negative (down), so draw at scroll_offset (e.g. -500)
                // so that the line at layout_y = 500 is at scratchpad_y = 0.
                scroll_offset
            } else {
                0.0
            };

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
            // If huge, we already baked the scroll into the transform, so blit at offset 0
            let effective_scroll = if is_huge { 0.0 } else { scroll_offset };
            blit_alpha_pixels(
                buffer,
                surface_w,
                x,
                y,
                tw,
                th,
                &captured_alpha,
                color,
                x.max(0) as u32 + max_w,
                y.max(0) as u32 + max_h,
                -(effective_scroll as i32),
            );

            // ONLY skip cache if it's truly giant to avoid re-rasterizing medium text
            // Also bypass if it's a "huge" scrolled item to avoid stale rendering bug
            if metrics.height < 3000.0 && !is_huge {
                let raster_key = RasterKey { layout_key, color };
                let mut cache = get_raster_cache().write().unwrap();
                // Limit to ~1M pixels (~4MB)
                while cache.total_pixels + pixel_count > 1_000_000 && !cache.order.is_empty() {
                    let oldest_key = cache.order.remove(0);
                    if let Some(old_entry) = cache.map.remove(&oldest_key) {
                        cache.total_pixels -= old_entry.pixel_count;
                    }
                }
                cache.order.push(raster_key.clone());
                cache.total_pixels += pixel_count;
                cache.map.insert(
                    raster_key,
                    RasterEntry {
                        alpha: captured_alpha,
                        tw,
                        th,
                        pixel_count,
                    },
                );
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
    src_y_off: i32,
) {
    let surface_h = (buffer.len() as u32) / surface_w.max(1);

    // Physical clipping in destination space
    let start_y = dest_y.max(0);
    // max_h is an absolute boundary
    let end_y = (dest_y + (th - src_y_off))
        .min(max_h as i32)
        .min(surface_h as i32);
    if start_y >= end_y {
        return;
    }

    let start_x = dest_x.max(0);
    // max_w is an absolute boundary
    let end_x = (dest_x + tw).min(max_w as i32).min(surface_w as i32);
    if start_x >= end_x {
        return;
    }

    let sr = (color >> 16) & 0xFF;
    let sg = (color >> 8) & 0xFF;
    let sb = color & 0xFF;

    let surface_w_usize = surface_w as usize;
    let tw_usize = tw as usize;

    for y in start_y..end_y {
        let dy = y - dest_y;
        let src_row = dy + src_y_off;
        let src_row_off = src_row as usize * tw_usize;
        let row_idx = y as usize * surface_w_usize;
        let src_slice = &src_alpha[src_row_off + (start_x - dest_x) as usize..];
        let dest_slice = &mut buffer[row_idx + start_x as usize..row_idx + end_x as usize];

        blend_row_u8(dest_slice, src_slice, sr, sg, sb);
    }
}

#[inline(always)]
fn blend_row_u8(dest_slice: &mut [u32], src_alpha: &[u8], sr: u32, sg: u32, sb: u32) {
    let len = dest_slice.len();
    let color_v = (sr << 16) | (sg << 8) | sb;

    for i in 0..len {
        let a = src_alpha[i] as u32;
        let bg = dest_slice[i];
        let inv_a = 255 - a;

        let rb = bg & 0x00FF00FF;
        let g = bg & 0x0000FF00;

        let rb_res = ((color_v & 0x00FF00FF) * a + rb * inv_a) >> 8;
        let g_res = ((color_v & 0x0000FF00) * a + g * inv_a) >> 8;

        dest_slice[i] = (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00);
    }
}

pub fn get_metrics_dw_ex(
    text: &str,
    font_size: f32,
    max_w: u32,
    font_family_name: &str,
    is_bold: bool,
    is_centered: bool,
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
    get_metrics_dw_ex(text, font_size, max_w, "Microsoft YaHei", false, false)
}

pub fn text_width(_fonts: &[&Font], text: &str, scale: Scale) -> u32 {
    #[cfg(target_os = "windows")]
    {
        let font_size = scale.x;
        let layout = get_or_create_layout(text, font_size, 10000);
        unsafe {
            let mut metrics = std::mem::zeroed();
            layout.GetMetrics(&mut metrics).unwrap();
            return metrics.width.ceil() as u32;
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
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
) -> (f32, f32) {
    if text.is_empty() {
        return (0.0, 0.0);
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
        (px, py)
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

pub fn blit_32bit_premultiplied(
    buffer: &mut [u32],
    surface_w: u32,
    surface_h: u32,
    src_pixels: &[u32],
    dest_x: i32,
    dest_y: i32,
    max_w: u32,
    h: u32,
) {
    if dest_y < 0
        || (dest_y + h as i32) > surface_h as i32
        || dest_x < -(max_w as i32)
        || dest_x >= surface_w as i32
    {
        return;
    }

    let tw = max_w as usize;
    let start_x = dest_x.max(0);
    let end_x = (dest_x + max_w as i32).min(surface_w as i32);
    if start_x >= end_x {
        return;
    }

    let surface_w = surface_w as usize;
    let start_x_u = start_x as usize;
    let end_x_u = end_x as usize;
    let copy_len = end_x_u - start_x_u;
    let x_off = (start_x - dest_x) as usize;

    for y in 0..h {
        let dy = (dest_y + y as i32) as usize;
        let dest_row_base = dy * surface_w;
        let src_row_base = y as usize * tw;

        let src_slice = &src_pixels[src_row_base + x_off..src_row_base + x_off + copy_len];
        let dest_slice =
            &mut buffer[dest_row_base + start_x_u..dest_row_base + start_x_u + copy_len];

        for i in 0..copy_len {
            let s = src_slice[i];
            let a = (s >> 24) & 0xFF;
            if a == 0 {
                continue;
            }
            if a == 255 {
                dest_slice[i] = s;
                continue;
            }

            let d = dest_slice[i];
            let inv_a = 255 - a;

            // Premultiplied: dest = src + dest * (255 - a) / 255
            let rb_dest = d & 0x00FF00FF;
            let g_dest = d & 0x0000FF00;

            let rb_src = s & 0x00FF00FF;
            let g_src = s & 0x0000FF00;

            let rb_res = rb_src + ((rb_dest * inv_a) >> 8);
            let g_res = g_src + ((g_dest * inv_a) >> 8);

            dest_slice[i] = (rb_res & 0x00FF00FF) | (g_res & 0x0000FF00);
        }
    }
}
