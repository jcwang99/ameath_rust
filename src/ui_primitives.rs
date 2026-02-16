use rusttype::{point, Font, Scale};

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

pub fn draw_text(
    buffer: &mut [u32],
    surface_w: u32,
    fonts: &[&Font],
    text: &str,
    x: i32,
    y: i32,
    scale: f32,
    color: u32,
) {
    if fonts.is_empty() {
        return;
    }
    let rustscale = Scale::uniform(scale);
    let primary_font = fonts[0];
    let v_metrics = primary_font.v_metrics(rustscale);

    let fg_r = ((color >> 16) & 0xFF) as f32;
    let fg_g = ((color >> 8) & 0xFF) as f32;
    let fg_b = (color & 0xFF) as f32;

    let mut current_x = x as f32;

    for c in text.chars() {
        let mut best_font = primary_font;
        if primary_font.glyph(c).id().0 == 0 {
            for &f in &fonts[1..] {
                if f.glyph(c).id().0 != 0 {
                    best_font = f;
                    break;
                }
            }
        }

        let glyph = best_font
            .glyph(c)
            .scaled(rustscale)
            .positioned(point(current_x, y as f32 + v_metrics.ascent));

        if let Some(bounding_box) = glyph.pixel_bounding_box() {
            glyph.draw(|gx, gy, v| {
                let px_i = gx as i32 + bounding_box.min.x;
                let py_i = gy as i32 + bounding_box.min.y;

                if px_i >= 0 && px_i < surface_w as i32 && py_i >= 0 {
                    let px = px_i as u32;
                    let py = py_i as u32;
                    let idx = (py * surface_w + px) as usize;
                    if idx < buffer.len() {
                        let alpha = v;
                        if alpha > 0.0 {
                            let bg = buffer[idx];
                            let r = ((bg >> 16) & 0xFF) as f32;
                            let g = ((bg >> 8) & 0xFF) as f32;
                            let b = (bg & 0xFF) as f32;

                            // Blend
                            let out_r = r * (1.0 - alpha) + fg_r * alpha;
                            let out_g = g * (1.0 - alpha) + fg_g * alpha;
                            let out_b = b * (1.0 - alpha) + fg_b * alpha;

                            buffer[idx] =
                                ((out_r as u32) << 16) | ((out_g as u32) << 8) | (out_b as u32);
                        }
                    }
                }
            });
        }
        current_x += glyph.unpositioned().h_metrics().advance_width;
    }
}

pub fn text_width(fonts: &[&Font], text: &str, scale: Scale) -> u32 {
    let mut current_x = 0.0;
    for c in text.chars() {
        let mut best_font = fonts[0];
        if !fonts.is_empty() && fonts[0].glyph(c).id().0 == 0 {
            for &f in &fonts[1..] {
                if f.glyph(c).id().0 != 0 {
                    best_font = f;
                    break;
                }
            }
        }
        current_x += best_font.glyph(c).scaled(scale).h_metrics().advance_width;
    }
    current_x as u32
}

pub fn wrap_text(
    text: &str,
    fonts: &[&Font],
    scale: rusttype::Scale,
    max_width: u32,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0.0;

    for c in text.chars() {
        if c == '\n' {
            lines.push(current_line);
            current_line = String::new();
            current_width = 0.0;
            continue;
        }

        let mut best_font = fonts[0];
        if !fonts.is_empty() && fonts[0].glyph(c).id().0 == 0 {
            for &f in &fonts[1..] {
                if f.glyph(c).id().0 != 0 {
                    best_font = f;
                    break;
                }
            }
        }

        let g = best_font.glyph(c).scaled(scale);
        let h_metrics = g.h_metrics();
        let advance = h_metrics.advance_width;

        if current_width + advance > max_width as f32 {
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
                current_width = 0.0;
            }
        }

        current_line.push(c);
        current_width += advance;
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}
