#[cfg(target_os = "windows")]
use crate::render::{get_d2d_factory, get_dwrite_factory};
use rusttype::{Font, Scale};
#[cfg(target_os = "windows")]
// use windows::core::ComInterface;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::RECT;
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
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
};

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

    for cy in start_y..max_y {
        for cx in start_x..max_x {
            let idx = (cy * surface_w as i32 + cx) as usize;
            if idx < buffer.len() {
                buffer[idx] = color;
            }
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

    for cy in start_y..max_y {
        for cx in start_x..max_x {
            let idx = (cy * surface_w as i32 + cx) as usize;
            if idx < buffer.len() {
                let bg = buffer[idx];
                let br = ((bg >> 16) & 0xFF) as f32;
                let bg_g = ((bg >> 8) & 0xFF) as f32;
                let bb = (bg & 0xFF) as f32;

                let r = br * (1.0 - alpha) + fr * alpha;
                let g = bg_g * (1.0 - alpha) + fg * alpha;
                let b = bb * (1.0 - alpha) + fb * alpha;

                buffer[idx] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
            }
        }
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

    for cy in start_y..end_y {
        let dy = if cy < y + r {
            (y + r) - cy
        } else if cy >= y + h - r {
            cy - (y + h - r - 1)
        } else {
            0
        };

        let row_off = cy as usize * surface_w as usize;
        if dy == 0 {
            // Straight middle rows
            buffer[row_off + start_x as usize..row_off + end_x as usize].fill(color);
        } else {
            let left_r_end = (x + r).min(end_x);
            let right_r_start = (x + w - r).max(start_x);

            // Left corner
            for cx in start_x..left_r_end {
                let dx = (x + r) - cx;
                if dx * dx + dy * dy <= r_sq {
                    buffer[row_off + cx as usize] = color;
                }
            }
            // Middle
            if left_r_end < right_r_start {
                buffer[row_off + left_r_end as usize..row_off + right_r_start as usize].fill(color);
            }
            // Right corner
            for cx in right_r_start..end_x {
                let dx = cx - (x + w - r - 1);
                if dx * dx + dy * dy <= r_sq {
                    buffer[row_off + cx as usize] = color;
                }
            }
        }
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

    let dwrite_factory = get_dwrite_factory();
    let d2d_factory = get_d2d_factory();

    unsafe {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let font_family = windows::core::w!("Microsoft YaHei");

        let text_format = dwrite_factory
            .CreateTextFormat(
                font_family,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size,
                windows::core::w!("en-us"),
            )
            .unwrap();

        // 1. Measure with unconstrained bounds to get true content size
        let layout_measure = dwrite_factory
            .CreateTextLayout(&wide_text, &text_format, 10000.0, 10000.0)
            .unwrap();

        let mut metrics = std::mem::zeroed();
        layout_measure.GetMetrics(&mut metrics).unwrap();

        // Add 2px padding for safety against anti-aliasing clipping
        let tw = (metrics.width.ceil() as i32) + 2;
        let th = (metrics.height.ceil() as i32) + 2;

        // 2. Prepare GDI surface (tw x th)
        let hdc_screen = windows::Win32::Graphics::Gdi::GetDC(windows::Win32::Foundation::HWND(0));
        let hdc_mem = CreateCompatibleDC(hdc_screen);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: tw,
                biHeight: -th, // Top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits = std::ptr::null_mut();
        let h_bitmap = CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let old_obj = SelectObject(hdc_mem, h_bitmap);

        // 3. Render Target over DC
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            ..Default::default()
        };

        let rt: ID2D1DCRenderTarget = d2d_factory.CreateDCRenderTarget(&props).unwrap();

        let target_rect = RECT {
            left: 0,
            top: 0,
            right: tw,
            bottom: th,
        };
        rt.BindDC(hdc_mem, &target_rect).unwrap();

        rt.BeginDraw();
        // Clear to transparent
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

        // 4. Blit with proper bounds checking
        let src_ptr = bits as *const u32;
        let surface_h = (buffer.len() as u32) / surface_w.max(1);

        let sr = ((color >> 16) & 0xFF) as u32;
        let sg = ((color >> 8) & 0xFF) as u32;
        let sb = (color & 0xFF) as u32;

        for dy in 0..th {
            let win_y = y + dy;
            if win_y < 0 || win_y >= surface_h as i32 {
                continue;
            }
            for dx in 0..tw {
                let win_x = x + dx;
                if win_x < 0 || win_x >= surface_w as i32 {
                    continue;
                }

                let src_idx = (dy * tw + dx) as usize;
                let pixel = *src_ptr.add(src_idx);
                let a = (pixel >> 24) & 0xFF;

                if a > 0 {
                    let dest_idx = (win_y * surface_w as i32 + win_x) as usize;
                    if dest_idx < buffer.len() {
                        if a == 255 {
                            buffer[dest_idx] = (sr << 16) | (sg << 8) | sb;
                        } else {
                            let bg = buffer[dest_idx];
                            let br = (bg >> 16) & 0xFF;
                            let bg_g = (bg >> 8) & 0xFF;
                            let bb = bg & 0xFF;

                            let inv_a = 255 - a;
                            let out_r = (sr * a + br * inv_a) / 255;
                            let out_g = (sg * a + bg_g * inv_a) / 255;
                            let out_b = (sb * a + bb * inv_a) / 255;
                            buffer[dest_idx] = (out_r << 16) | (out_g << 8) | out_b;
                        }
                    }
                }
            }
        }

        SelectObject(hdc_mem, old_obj);
        let _ = DeleteObject(h_bitmap);
        let _ = DeleteDC(hdc_mem);
        windows::Win32::Graphics::Gdi::ReleaseDC(windows::Win32::Foundation::HWND(0), hdc_screen);
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

    let dwrite_factory = get_dwrite_factory();
    let d2d_factory = get_d2d_factory();

    unsafe {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let font_family = windows::core::w!("Microsoft YaHei");

        let text_format = dwrite_factory
            .CreateTextFormat(
                font_family,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size,
                windows::core::w!("en-us"),
            )
            .unwrap();

        let layout = dwrite_factory
            .CreateTextLayout(&wide_text, &text_format, max_w as f32, 10000.0)
            .unwrap();

        // Prepare GDI surface (max_w x max_h)
        let tw = max_w as i32;
        let th = max_h as i32;

        let hdc_screen = windows::Win32::Graphics::Gdi::GetDC(windows::Win32::Foundation::HWND(0));
        let hdc_mem = CreateCompatibleDC(hdc_screen);

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: tw,
                biHeight: -th,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits = std::ptr::null_mut();
        let h_bitmap = CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
        let old_obj = SelectObject(hdc_mem, h_bitmap);

        let props = D2D1_RENDER_TARGET_PROPERTIES {
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            ..Default::default()
        };

        let rt: ID2D1DCRenderTarget = d2d_factory.CreateDCRenderTarget(&props).unwrap();
        let target_rect = RECT {
            left: 0,
            top: 0,
            right: tw,
            bottom: th,
        };
        rt.BindDC(hdc_mem, &target_rect).unwrap();

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

        let src_ptr = bits as *const u32;
        let surface_h = (buffer.len() as u32) / surface_w.max(1);

        let sr = ((color >> 16) & 0xFF) as u32;
        let sg = ((color >> 8) & 0xFF) as u32;
        let sb = (color & 0xFF) as u32;

        for dy in 0..th {
            let win_y = y + dy;
            if win_y < 0 || win_y >= surface_h as i32 {
                continue;
            }
            for dx in 0..tw {
                let win_x = x + dx;
                if win_x < 0 || win_x >= surface_w as i32 {
                    continue;
                }

                let src_idx = (dy * tw + dx) as usize;
                let pixel = *src_ptr.add(src_idx);
                let a = (pixel >> 24) & 0xFF;

                if a > 0 {
                    let dest_idx = (win_y * surface_w as i32 + win_x) as usize;
                    if dest_idx < buffer.len() {
                        if a == 255 {
                            buffer[dest_idx] = (sr << 16) | (sg << 8) | sb;
                        } else {
                            let bg = buffer[dest_idx];
                            let br = (bg >> 16) & 0xFF;
                            let bg_g = (bg >> 8) & 0xFF;
                            let bb = bg & 0xFF;

                            let inv_a = 255 - a;
                            let out_r = (sr * a + br * inv_a) / 255;
                            let out_g = (sg * a + bg_g * inv_a) / 255;
                            let out_b = (sb * a + bb * inv_a) / 255;
                            buffer[dest_idx] = (out_r << 16) | (out_g << 8) | out_b;
                        }
                    }
                }
            }
        }

        SelectObject(hdc_mem, old_obj);
        let _ = DeleteObject(h_bitmap);
        let _ = DeleteDC(hdc_mem);
        windows::Win32::Graphics::Gdi::ReleaseDC(windows::Win32::Foundation::HWND(0), hdc_screen);
    }
}

pub fn get_metrics_dw(text: &str, font_size: f32, max_w: u32) -> (f32, f32) {
    #[cfg(target_os = "windows")]
    {
        let dwrite_factory = get_dwrite_factory();
        unsafe {
            let wide_text: Vec<u16> = text.encode_utf16().collect();
            let text_format = dwrite_factory
                .CreateTextFormat(
                    windows::core::w!("Microsoft YaHei"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    font_size,
                    windows::core::w!("en-us"),
                )
                .unwrap();

            let layout = dwrite_factory
                .CreateTextLayout(&wide_text, &text_format, max_w as f32, 10000.0)
                .unwrap();

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

pub fn text_width(_fonts: &[&Font], text: &str, scale: Scale) -> u32 {
    #[cfg(target_os = "windows")]
    {
        let dwrite_factory = get_dwrite_factory();
        unsafe {
            let wide_text: Vec<u16> = text.encode_utf16().collect();
            let font_size = scale.x;

            let text_format = dwrite_factory
                .CreateTextFormat(
                    windows::core::w!("Microsoft YaHei"),
                    None,
                    DWRITE_FONT_WEIGHT_NORMAL,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    font_size,
                    windows::core::w!("en-us"),
                )
                .unwrap();

            let layout = dwrite_factory
                .CreateTextLayout(&wide_text, &text_format, 10000.0, 10000.0)
                .unwrap();

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
    let dwrite_factory = get_dwrite_factory();
    unsafe {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let font_size = scale.x;

        let text_format = dwrite_factory
            .CreateTextFormat(
                windows::core::w!("Microsoft YaHei"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size,
                windows::core::w!("en-us"),
            )
            .unwrap();

        let layout = dwrite_factory
            .CreateTextLayout(
                &wide_text,
                &text_format,
                max_width as f32,
                10000.0, // Large height
            )
            .unwrap();

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
    let dwrite_factory = get_dwrite_factory();
    unsafe {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let font_family = windows::core::w!("Microsoft YaHei");
        let text_format = dwrite_factory
            .CreateTextFormat(
                font_family,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size,
                windows::core::w!("en-us"),
            )
            .unwrap();

        let layout = dwrite_factory
            .CreateTextLayout(&wide_text, &text_format, max_width as f32, 10000.0)
            .unwrap();

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
    let dwrite_factory = get_dwrite_factory();
    unsafe {
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let mut utf16_pos = 0;
        for (i, c) in text.chars().enumerate() {
            if i == index {
                break;
            }
            utf16_pos += c.len_utf16();
        }

        let font_family = windows::core::w!("Microsoft YaHei");
        let text_format = dwrite_factory
            .CreateTextFormat(
                font_family,
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size,
                windows::core::w!("en-us"),
            )
            .unwrap();

        let layout = dwrite_factory
            .CreateTextLayout(&wide_text, &text_format, max_width as f32, 10000.0)
            .unwrap();

        let mut px = 0.0;
        let mut py = 0.0;
        let mut metrics = std::mem::zeroed();
        let _ = layout.HitTestTextPosition(utf16_pos as u32, false, &mut px, &mut py, &mut metrics);
        (px, py)
    }
}

#[cfg(not(target_os = "windows"))]
pub fn wrap_text(
    text: &str,
    _fonts: &[&Font],
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
