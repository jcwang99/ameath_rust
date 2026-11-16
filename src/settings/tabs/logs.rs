use crate::theme::*;
use crate::ui_primitives::*;

pub struct LogsTabState<'a> {
    pub content: &'a str,
    pub scroll_offset: f32,
}

pub fn draw(
    buffer: &mut [u32],
    w: u32,
    h: u32,
    scale: f32,
    off_x: f32,
    off_y: f32,
    state: &LogsTabState,
) -> (f32, f32, Option<(i32, i32, u32, u32)>) {
    let s = |value: u32| -> u32 { (value as f32 * scale + off_x) as u32 };
    let sy = |value: f32| -> i32 { (value * scale + off_y) as i32 };
    let sc = |value: f32| -> f32 { value * scale };

    let content = if state.content.trim().is_empty() {
        "No application logs available."
    } else {
        state.content
    };
    let (_, text_height) = get_metrics_dw(content, sc(13.0), sc(500.0) as u32);
    let viewport_height = 600.0;
    let text_viewport_height = 480.0;
    let content_height = (text_height / scale + 120.0).max(viewport_height);
    let card_y = sy(120.0);

    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        card_y,
        sc(560.0) as u32,
        sc(560.0) as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );
    draw_text(
        buffer,
        w,
        &[],
        "Recent Logs",
        s(230) as i32,
        card_y + sc(20.0) as i32,
        sc(18.0),
        COLOR_TEXT_MAIN,
    );
    draw_text_dw_ex(
        buffer,
        w,
        content,
        s(230) as i32,
        card_y + sc(55.0) as i32,
        sc(13.0),
        COLOR_TEXT_SEC,
        sc(500.0) as u32,
        sc(text_viewport_height) as u32,
        -state.scroll_offset * scale,
        0.0,
        sc(500.0) as u32,
    );

    let _ = h;
    (viewport_height, content_height, None)
}
