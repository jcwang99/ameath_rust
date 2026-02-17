use crate::theme::*;
use crate::ui_primitives::*;

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
    let current_y = start_y as f32 + state.scroll_offset;
    let calculated_content_height;

    state.history_item_rects.clear();
    if state.history_scroll_states.len() != state.history.len() {
        state.history_scroll_states.resize(state.history.len(), 0.0);
    }

    let item_h_fixed = sc(180.0);
    let spacing = sc(10.0);
    let total_item_h = item_h_fixed + spacing;

    // 1. Efficiently pre-calculate content height and all item rects
    calculated_content_height = state.history.len() as f32 * total_item_h;

    for i in 0..state.history.iter().len() {
        let logical_y = 140.0 + (i as f64 * 190.0);
        let logical_h = 180.0;
        state
            .history_item_rects
            .push((230.0, logical_y, 730.0, logical_y + logical_h));
    }

    // 2. Identify visible range
    let min_y_vis = sy_val(120) as f32;
    let max_y_vis = h as f32;

    for (i, (role, content)) in state.history.iter().enumerate() {
        let y_pos = current_y + (i as f32 * total_item_h);

        // Strict Clipping: Skip processing if bubble is completely off-screen
        if (y_pos + item_h_fixed) < min_y_vis || y_pos > max_y_vis {
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
        // Optimization: Use a local cache or avoid re-calculating metrics every frame if possible
        let (_mw, mh) = get_metrics_dw(content, sc(16.0), max_text_w);
        let full_content_h = mh.max(sc(20.0));
        let view_h = item_h_fixed - sc(40.0);

        let scroll = state.history_scroll_states[i];
        let start_text_y = y_pos + sc(35.0);

        // draw_text_dw_ex now uses RasterCache internally
        draw_text_dw_ex(
            buffer,
            w,
            content,
            s(240) as i32,
            start_text_y as i32,
            sc(16.0),
            COLOR_TEXT_MAIN,
            max_text_w,
            view_h as u32,
            scroll * scale,
        );

        if full_content_h > view_h {
            let sb_w = sc(4.0) as u32;
            let sb_h = view_h;
            let sb_x = s(230 + 480);
            let sb_y_raw = start_text_y;

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

    let viewport_height = sc(600.0);
    let content_height = calculated_content_height + sc(150.0);
    (viewport_height, content_height)
}
