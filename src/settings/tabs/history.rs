use crate::theme::*;
use crate::ui_primitives::*;
use rusttype::Scale;

pub struct HistoryTabState<'a> {
    pub history: &'a [(String, String)],
    pub history_scroll_states: &'a mut Vec<f32>,
    pub history_item_rects: &'a mut Vec<(f64, f64, f64, f64)>,
    pub scroll_offset: f32,
}

pub fn draw(
    buffer: &mut [u32],
    w: u32,
    h: u32,
    scale: f32,
    off_x: f32,
    off_y: f32,
    state: &mut HistoryTabState,
) -> (f32, f32) {
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };
    let sc = |val: f32| -> f32 { val * scale };

    let start_y = sy_val(140);
    let mut current_y = start_y as f32 + state.scroll_offset;
    let mut calculated_content_height = 0.0;

    state.history_item_rects.clear();
    if state.history_scroll_states.len() != state.history.len() {
        state.history_scroll_states.resize(state.history.len(), 0.0);
    }

    let item_h_fixed = sc(180.0);

    for (i, (role, content)) in state.history.iter().enumerate() {
        let logical_y = 140.0 + (i as f64 * 190.0);
        let logical_h = 180.0;
        state
            .history_item_rects
            .push((230.0, logical_y, 730.0, logical_y + logical_h));

        let y_pos = current_y;
        calculated_content_height += item_h_fixed + sc(10.0);
        current_y += item_h_fixed + sc(10.0);

        let min_y_vis = sy_val(120) as f32;
        if (y_pos + item_h_fixed) < min_y_vis || y_pos > h as f32 {
            continue;
        }

        let y_pos_i = y_pos as i32;
        draw_rounded_rect(
            buffer,
            w,
            s(230) as i32,
            y_pos_i,
            sc(490.0) as u32,
            item_h_fixed as u32,
            8,
            COLOR_BG_CARD,
            w,
            h,
        );

        let role_col = if role == "user" {
            COLOR_USER_ROLE
        } else {
            COLOR_AI_ROLE
        };
        draw_text(
            buffer,
            w,
            &[],
            role,
            s(240) as i32,
            y_pos_i + sc(10.0) as i32,
            sc(14.0),
            role_col,
        );

        let max_text_w = sc(450.0) as u32;
        let lines = wrap_text(content, &[], Scale::uniform(sc(16.0)), max_text_w);
        let full_content_h = (lines.len() as f32 * sc(20.0)).max(sc(20.0));
        let view_h = item_h_fixed - sc(40.0);

        let scroll = state.history_scroll_states[i];
        let start_text_y = y_pos + sc(35.0);
        let end_text_y = y_pos + item_h_fixed - sc(10.0);

        for (li, line) in lines.iter().enumerate() {
            let line_y = start_text_y + (li as f32 * sc(20.0)) + (scroll * scale);
            if line_y < start_text_y - sc(5.0) {
                continue;
            }
            if line_y > end_text_y - sc(15.0) {
                break;
            }
            if line_y < 0.0 || line_y > h as f32 {
                continue;
            }

            draw_text(
                buffer,
                w,
                &[],
                line,
                s(240) as i32,
                line_y as i32,
                sc(16.0),
                COLOR_TEXT_MAIN,
            );
        }

        if full_content_h > view_h {
            let sb_w = sc(4.0) as u32;
            let sb_h = view_h;
            let sb_x = s(230 + 480);
            let sb_y_raw = start_text_y;

            if sb_y_raw + sb_h > 0.0 && sb_y_raw < h as f32 {
                draw_rect(
                    buffer,
                    w,
                    sb_x as i32,
                    sb_y_raw as i32,
                    sb_w,
                    sb_h as u32,
                    COLOR_BORDER,
                    w,
                    h,
                );
                let ratio = view_h / full_content_h;
                let h_h = (view_h * ratio).max(sc(20.0));
                let max_scroll = -(full_content_h - view_h);
                let progress = if max_scroll.abs() < 1.0 {
                    0.0
                } else {
                    (scroll * scale / max_scroll).clamp(0.0, 1.0)
                };
                let h_y = sb_y_raw + (view_h - h_h) * progress;
                draw_rect(
                    buffer,
                    w,
                    sb_x as i32,
                    h_y as i32,
                    sb_w,
                    h_h as u32,
                    0x00A0A0A0,
                    w,
                    h,
                );
            }
        }
    }

    let viewport_height = sc(600.0);
    let content_height = calculated_content_height + sc(150.0);
    (viewport_height, content_height)
}
