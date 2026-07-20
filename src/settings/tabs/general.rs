use crate::theme::*;
use crate::ui_primitives::*;
// use rusttype::Font;

pub struct GeneralTabState<'a> {
    pub current_scale: f32,
    pub current_mode: &'a str,
    pub current_music_path: Option<&'a std::path::Path>,
    pub current_layer: crate::types::WindowLayer,
    pub run_on_startup: bool,
    pub scroll_offset: f32,
    pub available_monitors: &'a [(String, String)],
    pub current_monitor_name: Option<&'a str>,
}

fn card_y(base_y: f32, scroll_y: f32, scale: f32, off_y: f32) -> f32 {
    base_y * scale + off_y + scroll_y * scale
}

pub fn draw(
    buffer: &mut [u32],
    w: u32,
    h: u32,
    scale: f32,
    off_x: f32,
    off_y: f32,
    state: &mut GeneralTabState,
) -> (f32, f32, Option<(i32, i32, u32, u32)>) {
    let scroll_y = state.scroll_offset;
    let current_scale = state.current_scale;
    let current_mode = state.current_mode;
    let current_music_path = state.current_music_path;
    let current_layer = state.current_layer;
    let available_monitors = state.available_monitors;
    let current_monitor_name = state.current_monitor_name;
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sc = |val: f32| -> f32 { val * scale };

    let card_w = (560.0 * scale) as u32;
    let card1_y = card_y(120.0, scroll_y, scale, off_y) as i32;

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

    // Slider logic: range 0.1 to 3.0
    let progress = ((current_scale - 0.1) / 2.9).clamp(0.0, 1.0);
    let track_x = s(230) as i32;
    let track_y = card1_y + sc(75.0) as i32;
    let track_w = sc(300.0) as u32;
    let track_h = sc(6.0) as u32;

    // Background track
    draw_rounded_rect(
        buffer,
        w,
        track_x,
        track_y,
        track_w,
        track_h,
        (3.0 * scale) as u32,
        COLOR_BG_LIGHT,
        w,
        h,
    );

    // Filled track
    let fill_w = (sc(300.0) * progress).max(sc(6.0)) as u32;
    draw_rounded_rect(
        buffer,
        w,
        track_x,
        track_y,
        fill_w,
        track_h,
        (3.0 * scale) as u32,
        COLOR_PRIMARY,
        w,
        h,
    );

    // Knob
    let knob_size = sc(18.0) as u32;
    let knob_x = track_x + fill_w as i32 - (knob_size as i32 / 2);
    let knob_y = track_y + (track_h as i32 / 2) - (knob_size as i32 / 2);
    draw_rounded_rect(
        buffer,
        w,
        knob_x,
        knob_y,
        knob_size,
        knob_size,
        (9.0 * scale) as u32,
        0x00FFFFFF,
        w,
        h,
    );

    // Value text
    let val_text = format!("{:.2}x", current_scale);
    draw_text(
        buffer,
        w,
        &[],
        &val_text,
        track_x + track_w as i32 + sc(20.0) as i32,
        track_y - sc(6.0) as i32,
        sc(16.0),
        COLOR_TEXT_MAIN,
    );

    // 2. Behavior Mode
    let card2_y = card_y(280.0, scroll_y, scale, off_y) as i32;
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
    let card3_y = card_y(505.0, scroll_y, scale, off_y) as i32;
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
    let card4_y = card_y(665.0, scroll_y, scale, off_y) as i32;
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
    let card5_y = card_y(825.0, scroll_y, scale, off_y) as i32;
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
        let is_active = current_monitor_name.map(|n| n == name).unwrap_or(false);

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

        let display_name = if name.chars().count() > 8 {
            format!("{}...", name.chars().take(5).collect::<String>())
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

    // 6. Auto-Start
    let card6_y = card_y(825.0 + 60.0 + monitors_h + 20.0, scroll_y, scale, off_y) as i32;
    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        card6_y,
        card_w,
        (80.0 * scale) as u32,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );
    draw_text(
        buffer,
        w,
        &[],
        "Run on Startup",
        s(230) as i32,
        card6_y + sc(28.0) as i32,
        sc(16.0),
        COLOR_TEXT_MAIN,
    );
    draw_text(
        buffer,
        w,
        &[],
        "Automatically start Ameath when Windows boots",
        s(230) as i32,
        card6_y + sc(50.0) as i32,
        sc(12.0),
        COLOR_TEXT_SEC,
    );

    // Toggle Switch logic
    let toggle_x = s(210) as i32 + card_w as i32 - sc(80.0) as i32;
    let toggle_y = card6_y + sc(25.0) as i32;
    let toggle_w = sc(44.0) as u32;
    let toggle_h = sc(24.0) as u32;
    let (bg_color, knob_x) = if state.run_on_startup {
        (COLOR_PRIMARY, toggle_x + sc(22.0) as i32)
    } else {
        (COLOR_BORDER, toggle_x + sc(2.0) as i32)
    };

    // Background
    draw_rounded_rect(
        buffer, w, toggle_x, toggle_y, toggle_w, toggle_h, 12, bg_color, w, h,
    );
    // Knob
    draw_rounded_rect(
        buffer,
        w,
        knob_x,
        toggle_y + sc(2.0) as i32,
        sc(20.0) as u32,
        sc(20.0) as u32,
        10,
        0x00FFFFFF,
        w,
        h,
    );

    // Content height tracking
    let viewport_height = 600.0;
    let content_height = 825.0 + 60.0 + monitors_h + 40.0 + 100.0; // Added height for card 6

    (viewport_height, content_height, None)
}

#[cfg(test)]
mod tests {
    #[test]
    fn card_vertical_offsets_scale_consistently() {
        let scroll_offset = -30.0;
        let scale = 1.5;

        let card1 = super::card_y(120.0, scroll_offset, scale, 0.0);
        let card2 = super::card_y(280.0, scroll_offset, scale, 0.0);

        let gap = card2 - card1;
        assert!((gap - (280.0 - 120.0) * scale).abs() < 0.001);
    }
}
