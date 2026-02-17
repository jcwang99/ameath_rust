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
    scroll_y: f32,
    current_scale: f32,
    current_mode: &str,
    current_music_path: Option<&std::path::Path>,
    current_layer: crate::types::WindowLayer,
    available_monitors: &[(String, String)],
    current_monitor_name: Option<&String>,
) -> (f32, f32) {
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };
    let sc = |val: f32| -> f32 { val * scale };

    let card_w = (560.0 * scale) as u32;
    let card1_y = (sy_val(120) as f32 + scroll_y) as i32;

    // 1. Pet Scale
    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        card1_y,
        card_w,
        (140.0 * scale) as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );
    draw_text(
        buffer,
        w,
        &[],
        "Pet Scale",
        s(230) as i32,
        card1_y + sc(20.0) as i32,
        sc(18.0),
        COLOR_TEXT_MAIN,
    );

    let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
    let labels = vec!["0.5x", "0.75x", "1.0x", "1.25x", "1.5x"];
    for (i, &val) in scales.iter().enumerate() {
        let mx = s(220 + i as u32 * 85) as i32;
        let my = card1_y + sc(60.0) as i32;
        let is_active = (current_scale - val).abs() < 0.01;
        let bg_col = if is_active {
            COLOR_PRIMARY
        } else {
            COLOR_BG_LIGHT
        };
        let text_col = if is_active {
            0x00FFFFFF
        } else {
            COLOR_TEXT_MAIN
        };
        draw_rounded_rect(
            buffer,
            w,
            mx,
            my,
            sc(75.0) as u32,
            sc(45.0) as u32,
            8,
            bg_col,
            w,
            h,
        );
        draw_text(
            buffer,
            w,
            &[],
            labels[i],
            mx + sc(12.0) as i32,
            my + sc(12.0) as i32,
            sc(14.0),
            text_col,
        );
    }

    // 2. Behavior Mode
    let card2_y = (sy_val(280) as f32 + scroll_y * scale) as i32;
    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        card2_y,
        card_w,
        (205.0 * scale) as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );
    draw_text(
        buffer,
        w,
        &[],
        "Behavior Mode",
        s(230) as i32,
        card2_y + sc(20.0) as i32,
        sc(18.0),
        COLOR_TEXT_MAIN,
    );
    let modes = vec!["Static", "Quiet", "Active", "Clingy"];
    for (i, mode) in modes.iter().enumerate() {
        let row = i / 2;
        let col = i % 2;
        let mx = s(230 + col as u32 * 165) as i32;
        let my = card2_y + sc(60.0 + row as f32 * 65.0) as i32;
        let is_active = *mode == current_mode;
        let b_col = if is_active {
            COLOR_PRIMARY
        } else {
            COLOR_BORDER
        };
        draw_rounded_rect(
            buffer,
            w,
            mx,
            my,
            sc(150.0) as u32,
            sc(55.0) as u32,
            8,
            b_col,
            w,
            h,
        );
        draw_rounded_rect(
            buffer,
            w,
            mx + 2,
            my + 2,
            sc(150.0) as u32 - 4,
            sc(55.0) as u32 - 4,
            6,
            COLOR_BG_CARD,
            w,
            h,
        );
        draw_text(
            buffer,
            w,
            &[],
            mode,
            mx + sc(25.0) as i32,
            my + sc(18.0) as i32,
            sc(15.0),
            if is_active {
                COLOR_PRIMARY
            } else {
                COLOR_TEXT_SEC
            },
        );
    }

    // 3. Music Directory
    let card3_y = (sy_val(505) as f32 + scroll_y * scale) as i32;
    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        card3_y,
        card_w,
        (140.0 * scale) as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );
    draw_text(
        buffer,
        w,
        &[],
        "Music Directory",
        s(230) as i32,
        card3_y + sc(20.0) as i32,
        sc(18.0),
        COLOR_TEXT_MAIN,
    );
    let p_btn_y = card3_y + sc(60.0) as i32;
    draw_rounded_rect(
        buffer,
        w,
        s(230) as i32,
        p_btn_y,
        sc(500.0) as u32,
        sc(45.0) as u32,
        8,
        COLOR_BORDER,
        w,
        h,
    );
    draw_rounded_rect(
        buffer,
        w,
        s(230) as i32 + 1,
        p_btn_y + 1,
        sc(500.0) as u32 - 2,
        sc(45.0) as u32 - 2,
        7,
        COLOR_BG_CARD,
        w,
        h,
    );
    let path = current_music_path
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "Click to select...".to_string());
    draw_text(
        buffer,
        w,
        &[],
        &path,
        s(245) as i32,
        p_btn_y + sc(12.0) as i32,
        sc(14.0),
        COLOR_TEXT_SEC,
    );

    // 4. Window Layer
    let card4_y = (sy_val(665) as f32 + scroll_y * scale) as i32;
    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        card4_y,
        card_w,
        (140.0 * scale) as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );
    draw_text(
        buffer,
        w,
        &[],
        "Window Layer",
        s(230) as i32,
        card4_y + sc(20.0) as i32,
        sc(18.0),
        COLOR_TEXT_MAIN,
    );
    let layers = vec!["Always on Top", "Standard (Bottom)"];
    for (i, layer_name) in layers.iter().enumerate() {
        let mx = s(230 + i as u32 * 210) as i32;
        let my = card4_y + sc(60.0) as i32;
        let is_active = if i == 0 {
            current_layer == crate::types::WindowLayer::Top
        } else {
            current_layer == crate::types::WindowLayer::Bottom
        };
        let b_col = if is_active {
            COLOR_PRIMARY
        } else {
            COLOR_BORDER
        };
        draw_rounded_rect(
            buffer,
            w,
            mx,
            my,
            sc(200.0) as u32,
            sc(55.0) as u32,
            8,
            b_col,
            w,
            h,
        );
        draw_rounded_rect(
            buffer,
            w,
            mx + 2,
            my + 2,
            sc(200.0) as u32 - 4,
            sc(55.0) as u32 - 4,
            6,
            COLOR_BG_CARD,
            w,
            h,
        );
        draw_text(
            buffer,
            w,
            &[],
            layer_name,
            mx + sc(20.0) as i32,
            my + sc(18.0) as i32,
            sc(14.0),
            if is_active {
                COLOR_PRIMARY
            } else {
                COLOR_TEXT_SEC
            },
        );
    }

    // 5. Monitor Selection
    let card5_y = (sy_val(825) as f32 + scroll_y * scale) as i32;
    let rows = (available_monitors.len() + 2) / 3;
    let monitors_h = if rows > 0 { rows as f32 * 65.0 } else { 65.0 };
    let card5_h = (60.0 + monitors_h) * scale;

    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        card5_y,
        card_w,
        card5_h as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );
    draw_text(
        buffer,
        w,
        &[],
        "Display Monitor",
        s(230) as i32,
        card5_y + sc(20.0) as i32,
        sc(18.0),
        COLOR_TEXT_MAIN,
    );

    for (i, (name, _)) in available_monitors.iter().enumerate() {
        let row = i / 3;
        let col = i % 3;
        let btn_x = s(230 + col as u32 * 110) as i32;
        let btn_y = card5_y + sc(60.0 + row as f32 * 65.0) as i32;
        let is_active = Some(name) == current_monitor_name;

        let bg_col = if is_active {
            COLOR_PRIMARY
        } else {
            COLOR_BG_LIGHT
        };
        let text_col = if is_active {
            0x00FFFFFF
        } else {
            COLOR_TEXT_MAIN
        };

        draw_rounded_rect(
            buffer,
            w,
            btn_x,
            btn_y,
            sc(100.0) as u32,
            sc(55.0) as u32,
            8,
            bg_col,
            w,
            h,
        );

        let display_name = if name.len() > 8 {
            format!("{}...", &name[..5])
        } else {
            name.clone()
        };

        draw_text(
            buffer,
            w,
            &[],
            &display_name,
            btn_x + sc(10.0) as i32,
            btn_y + sc(18.0) as i32,
            sc(12.0),
            text_col,
        );
    }

    // Content height tracking
    let viewport_height = 600.0;
    let content_height = 825.0 + 60.0 + monitors_h + 40.0;

    (viewport_height, content_height)
}
