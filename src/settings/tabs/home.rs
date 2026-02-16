use crate::theme::*;
use crate::ui_primitives::*;
// use rusttype::Font;

pub fn draw(buffer: &mut [u32], w: u32, h: u32, scale: f32, off_x: f32, off_y: f32) -> (f32, f32) {
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

    let viewport_height = sc(600.0);
    let content_height = sc(320.0);

    (viewport_height, content_height)
}
