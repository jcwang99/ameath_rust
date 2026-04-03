use crate::theme::*;
use crate::ui_primitives::*;
// use rusttype::Font;

pub fn draw(
    buffer: &mut [u32],
    w: u32,
    h: u32,
    scale: f32,
    off_x: f32,
    off_y: f32,
) -> (f32, f32, Option<(i32, i32, u32, u32)>) {
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };
    let sc = |val: f32| -> f32 { val * scale };

    // Welcome card
    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        sy_val(120) as i32,
        (560.0 * scale) as u32,
        (200.0 * scale) as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );

    draw_text(
        buffer,
        w,
        &[],
        "Welcome back!",
        s(230) as i32,
        sy_val(150) as i32,
        sc(24.0),
        COLOR_TEXT_MAIN,
    );

    draw_text(
        buffer,
        w,
        &[],
        "Select a tab on the left to configure your desktop pet.",
        s(230) as i32,
        sy_val(200) as i32,
        sc(14.0),
        COLOR_TEXT_SEC,
    );

    // Quick Access Card
    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        sy_val(340) as i32,
        (560.0 * scale) as u32,
        (160.0 * scale) as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );

    draw_text(
        buffer,
        w,
        &[],
        "Quick Access",
        s(230) as i32,
        sy_val(370) as i32,
        sc(20.0),
        COLOR_TEXT_MAIN,
    );

    // Open Working Directory Button
    let btn_x = s(230) as i32;
    let btn_y = sy_val(415) as i32;
    let btn_w = (200.0 * scale) as u32;
    let btn_h = (45.0 * scale) as u32;

    draw_rounded_rect_with_border(
        buffer,
        w,
        btn_x,
        btn_y,
        btn_w,
        btn_h,
        8,
        COLOR_BG_APP,    // Button background
        COLOR_PRIMARY,   // Border color
        2,               // Border thickness
        w,
        h,
    );

    let btn_text = "Open Directory";
    let font_size = sc(14.0);
    // 获取文本度量信息以进行精确居中
    let (tw_text, th_text) = get_metrics_dw(btn_text, font_size, btn_w);
    
    draw_text(
        buffer,
        w,
        &[],
        btn_text,
        btn_x + (btn_w as i32 - tw_text as i32) / 2,
        btn_y + (btn_h as i32 - th_text as i32) / 2,
        font_size,
        COLOR_PRIMARY,
    );

    let viewport_height = 600.0;
    let content_height = 520.0;

    (viewport_height, content_height, None)
}
