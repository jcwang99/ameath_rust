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
    CreateCompatibleDC, CreateDIBSection, DeleteObject, GetDC, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};

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
impl ScratchpadRenderer {
    fn new() -> Self {
        unsafe {
            let hdc_screen = GetDC(HWND(0));
            let hdc_mem = CreateCompatibleDC(hdc_screen);
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
struct LayoutKey {
    text: String,
    font_size_bits: u32,
    max_w: u32,
    font_family: String,
    is_bold: bool,
    is_centered: bool,
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct FormatKey {
    font_family: String,
    font_size_bits: u32,
    is_bold: bool,
    is_centered: bool,
}

static LAYOUT_CACHE: OnceLock<RwLock<HashMap<LayoutKey, IDWriteTextLayout>>> = OnceLock::new();
static FORMAT_CACHE: OnceLock<RwLock<HashMap<FormatKey, IDWriteTextFormat>>> = OnceLock::new();

fn get_layout_cache() -> &'static RwLock<HashMap<LayoutKey, IDWriteTextLayout>> {
    LAYOUT_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn get_format_cache() -> &'static RwLock<HashMap<FormatKey, IDWriteTextFormat>> {
    FORMAT_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn get_or_create_layout_ex(
    text: &str,
    font_size: f32,
    max_w: u32,
    font_family_name: &str,
    is_bold: bool,
    is_centered: bool,
) -> IDWriteTextLayout {
    let key = LayoutKey {
        text: text.to_string(),
        font_size_bits: font_size.to_bits(),
        max_w,
        font_family: font_family_name.to_string(),
        is_bold,
        is_centered,
    };

    if let Ok(cache) = get_layout_cache().read() {
        if let Some(layout) = cache.get(&key) {
            return layout.clone();
        }
    }

    let dwrite_factory = get_dwrite_factory();
    let format_key = FormatKey {
        font_family: font_family_name.to_string(),
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
        if cache.len() > 300 {
            cache.clear();
        }
        cache.insert(key, layout.clone());
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

    if start_y_idx < end_y_idx {
        buffer[start_y_idx * surface_w_usize..end_y_idx * surface_w_usize]
            .par_chunks_mut(surface_w_usize)
            .for_each(|row| {
                row[start_x as usize..max_x as usize].fill(color);
            });
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
    }
}

pub fn draw_rounded_rect(
    buffer: &mut [u32],
    surface_w: u32,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    radius: u32,
    color: u32,
    max_w: u32,
    max_h: u32,
) {
    let start_x = x.max(0);
    let start_y = y.max(0);
    let end_x = (x + width as i32).min(max_w as i32);
    let end_y = (y + height as i32).min(max_h as i32);

    if start_x >= end_x || start_y >= end_y {
        return;
    }

    let r = radius as i32;
    let r_sq = r * r;
    let w = width as i32;
    let h = height as i32;

    let start_y_idx = start_y as usize;
    let end_y_idx = end_y as usize;
    let surface_w_usize = surface_w as usize;

    if start_y_idx < end_y_idx {
        let affected_rows = &mut buffer[start_y_idx * surface_w_usize..end_y_idx * surface_w_usize];
        affected_rows
            .par_chunks_mut(surface_w_usize)
            .enumerate()
            .for_each(|(i, row)| {
                let cy = (start_y_idx + i) as i32;
                let dy = if cy < y + r {
                    (y + r) - cy
                } else if cy >= y + h - r {
                    cy - (y + h - r - 1)
                } else {
                    0
                };

                if dy == 0 {
                    // Straight middle rows
                    row[start_x as usize..end_x as usize].fill(color);
                } else {
                    let left_r_end = (x + r).min(end_x);
                    let right_r_start = (x + w - r).max(start_x);

                    // Left corner
                    for cx in start_x..left_r_end {
                        let dx = (x + r) - cx;
                        if dx * dx + dy * dy <= r_sq {
                            row[cx as usize] = color;
                        }
                    }
                    // Middle
                    if left_r_end < right_r_start {
                        row[left_r_end as usize..right_r_start as usize].fill(color);
                    }
                    // Right corner
                    for cx in right_r_start..end_x {
                        let dx = cx - (x + w - r - 1);
                        if dx * dx + dy * dy <= r_sq {
                            row[cx as usize] = color;
                        }
                    }
                }
            });
    }
}

pub fn draw_text_ex(
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
    #[cfg(target_os = "windows")]
    {
        draw_text_dw_ex(
            buffer,
            surface_w,
            text,
            x,
            y,
            font_size,
            color,
            max_w,
            max_h,
            scroll_offset,
        );
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
        draw_text_dw(
            buffer, surface_w, text, x, y, font_size, color, surface_w, surface_h,
        );
    }
}

#[cfg(target_os = "windows")]
pub fn draw_text_dw(
    buffer: &mut [u32],
    surface_w: u32,
    text: &str,
    x: i32,
    y: i32,
    font_size: f32,
    color: u32,
    _max_w: u32,
    _max_h: u32,
) {
    if text.is_empty() {
        return;
    }

    unsafe {
        let layout_measure = get_or_create_layout(text, font_size, 10000);
        let mut metrics = std::mem::zeroed();
        layout_measure.GetMetrics(&mut metrics).unwrap();

        let tw = (metrics.width.ceil() as i32) + 2;
        let th = (metrics.height.ceil() as i32) + 2;

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

            rt.DrawTextLayout(
                windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F { x: 0.0, y: 0.0 },
                &layout_measure,
                &brush,
                windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            rt.EndDraw(None, None).unwrap();

            let sr = ((color >> 16) & 0xFF) as u32;
            let sg = ((color >> 8) & 0xFF) as u32;
            let sb = (color & 0xFF) as u32;

            let surface_h = (buffer.len() as u32) / surface_w.max(1);
            let start_y_idx = y.max(0) as usize;
            let end_y_idx = (y + th as i32).min(surface_h as i32).max(0) as usize;
            let surface_w_usize = surface_w as usize;

            if start_y_idx < end_y_idx {
                let affected_rows =
                    &mut buffer[start_y_idx * surface_w_usize..end_y_idx * surface_w_usize];
                let src_w = sp.width;

                if tw < 300 {
                    for i in 0..(end_y_idx - start_y_idx) {
                        let win_y = (start_y_idx + i) as i32;
                        let dy = win_y - y;
                        let row =
                            &mut affected_rows[i * surface_w_usize..(i + 1) * surface_w_usize];
                        for dx in 0..tw {
                            let win_x = x + dx;
                            if win_x < 0 || win_x >= surface_w as i32 {
                                continue;
                            }
                            let src_idx = (dy * src_w + dx) as usize;
                            let pixel = *scratch_bits.add(src_idx);
                            let a = (pixel >> 24) & 0xFF;
                            if a > 0 {
                                if a == 255 {
                                    row[win_x as usize] = (sr << 16) | (sg << 8) | sb;
                                } else {
                                    let bg = row[win_x as usize];
                                    let inv_a = 255 - a;
                                    let out_r = (sr * a + ((bg >> 16) & 0xFF) * inv_a) / 255;
                                    let out_g = (sg * a + ((bg >> 8) & 0xFF) * inv_a) / 255;
                                    let out_b = (sb * a + (bg & 0xFF) * inv_a) / 255;
                                    row[win_x as usize] = (out_r << 16) | (out_g << 8) | out_b;
                                }
                            }
                        }
                    }
                } else {
                    let src_addr = scratch_bits as usize;
                    affected_rows
                        .par_chunks_mut(surface_w_usize)
                        .enumerate()
                        .for_each(|(i, row)| {
                            let win_y = (start_y_idx + i) as i32;
                            let dy = win_y - y;
                            let src_ptr = src_addr as *const u32;
                            for dx in 0..tw {
                                let win_x = x + dx;
                                if win_x < 0 || win_x >= surface_w as i32 {
                                    continue;
                                }
                                let src_idx = (dy * src_w + dx) as usize;
                                let pixel = *src_ptr.add(src_idx);
                                let a = (pixel >> 24) & 0xFF;
                                if a > 0 {
                                    if a == 255 {
                                        row[win_x as usize] = (sr << 16) | (sg << 8) | sb;
                                    } else {
                                        let bg = row[win_x as usize];
                                        let inv_a = 255 - a;
                                        let out_r = (sr * a + ((bg >> 16) & 0xFF) * inv_a) / 255;
                                        let out_g = (sg * a + ((bg >> 8) & 0xFF) * inv_a) / 255;
                                        let out_b = (sb * a + (bg & 0xFF) * inv_a) / 255;
                                        row[win_x as usize] = (out_r << 16) | (out_g << 8) | out_b;
                                    }
                                }
                            }
                        });
                }
            }
        });
    }
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

    unsafe {
        let layout = get_or_create_layout(text, font_size, max_w);
        let tw = max_w as i32;
        let th = max_h as i32;

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

            rt.DrawTextLayout(
                windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x: 0.0,
                    y: scroll_offset,
                },
                &layout,
                &brush,
                windows::Win32::Graphics::Direct2D::D2D1_DRAW_TEXT_OPTIONS_NONE,
            );
            rt.EndDraw(None, None).unwrap();

            let sr = ((color >> 16) & 0xFF) as u32;
            let sg = ((color >> 8) & 0xFF) as u32;
            let sb = (color & 0xFF) as u32;

            let surface_h = (buffer.len() as u32) / surface_w.max(1);
            let start_y_idx = y.max(0) as usize;
            let end_y_idx = (y + th as i32).min(surface_h as i32).max(0) as usize;
            let surface_w_usize = surface_w as usize;

            if start_y_idx < end_y_idx {
                let affected_rows =
                    &mut buffer[start_y_idx * surface_w_usize..end_y_idx * surface_w_usize];
                let src_w = sp.width;
                let src_addr = scratch_bits as usize;

                affected_rows
                    .par_chunks_mut(surface_w_usize)
                    .enumerate()
                    .for_each(|(i, row)| {
                        let win_y = (start_y_idx + i) as i32;
                        let dy = win_y - y;
                        let src_ptr = src_addr as *const u32;
                        for dx in 0..tw {
                            let win_x = x + dx;
                            if win_x < 0 || win_x >= surface_w as i32 {
                                continue;
                            }
                            let src_idx = (dy * src_w + dx) as usize;
                            let pixel = *src_ptr.add(src_idx);
                            let a = (pixel >> 24) & 0xFF;
                            if a > 0 {
                                if a == 255 {
                                    row[win_x as usize] = (sr << 16) | (sg << 8) | sb;
                                } else {
                                    let bg = row[win_x as usize];
                                    let inv_a = 255 - a;
                                    let out_r = (sr * a + ((bg >> 16) & 0xFF) * inv_a) / 255;
                                    let out_g = (sg * a + ((bg >> 8) & 0xFF) * inv_a) / 255;
                                    let out_b = (sb * a + (bg & 0xFF) * inv_a) / 255;
                                    row[win_x as usize] = (out_r << 16) | (out_g << 8) | out_b;
                                }
                            }
                        }
                    });
            }
        });
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
