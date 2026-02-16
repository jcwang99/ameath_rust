use crate::theme::*;
use crate::types::AiConfig;
use crate::ui_primitives::*;
use rusttype::Scale;

pub struct AiTabState<'a> {
    pub focused_field: Option<usize>,
    pub show_api_key: bool,
    pub cursor_pos: usize,
    pub selection_start: Option<usize>,
    pub last_cursor_action: std::time::Instant,
    pub system_prompt_scroll_offset: f32,
    pub active_sys_prompt_content_height: &'a mut f32,
    pub active_sys_prompt_rect: &'a mut Option<(f64, f64, f64, f64)>,
}

pub fn draw(
    buffer: &mut [u32],
    w: u32,
    h: u32,
    scale: f32,
    off_x: f32,
    off_y: f32,
    scroll_y: f32,
    ai_config: &AiConfig,
    state: &mut AiTabState,
) -> (f32, f32) {
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };
    let sc = |val: f32| -> f32 { val * scale };

    let card_w = (560.0 * scale) as u32;
    let card_h = (950.0 * scale) as u32;
    let card_y_raw = (sy_val(120) as f32 + scroll_y) as i32;

    draw_rounded_rect(
        buffer,
        w,
        s(210) as i32,
        card_y_raw,
        card_w,
        card_h,
        12,
        COLOR_BG_CARD,
        w,
        h,
    );

    let fields = vec![
        ("API Key", ai_config.api_key.clone()),
        ("Base URL", ai_config.base_url.clone()),
        ("Model", ai_config.model.clone()),
        ("ReAct Steps", ai_config.react_limit.to_string()),
        ("L1 Summary", ai_config.l1_summary_threshold.to_string()),
        ("L2 Merge", ai_config.l2_merge_threshold.to_string()),
        ("Tavily Key", ai_config.tavily_api_key.clone()),
        ("System Prompt", ai_config.system_prompt.clone()),
        (
            "Interact Interval (min)",
            ai_config.interaction_frequency.to_string(),
        ),
    ];

    for (i, (label, val)) in fields.iter().enumerate() {
        let (fx, fy, fw) = match i {
            0 => (230.0, 30.0, 500.0),
            1 => (230.0, 130.0, 500.0),
            2 => (230.0, 230.0, 500.0),
            3 => (230.0, 330.0, 150.0),
            4 => (405.0, 330.0, 150.0),
            5 => (580.0, 330.0, 150.0),
            8 => (230.0, 430.0, 150.0),
            6 => (230.0, 530.0, 500.0),
            7 => (230.0, 630.0, 500.0),
            _ => (0.0, 0.0, 0.0),
        };

        let fy_scaled_raw = card_y_raw + sc(fy) as i32;
        draw_text(
            buffer,
            w,
            &[],
            label,
            s(fx as u32) as i32,
            fy_scaled_raw,
            sc(14.0),
            COLOR_TEXT_SEC,
        );

        let input_y_raw = fy_scaled_raw + sc(25.0) as i32;
        let input_w = sc(fw as f32) as u32;
        let input_h = if i == 7 {
            let input_h_logical = 250.0;
            sc(input_h_logical) as u32
        } else {
            sc(45.0) as u32
        };

        let is_focused = state.focused_field == Some(i);
        let border_col = if is_focused {
            COLOR_PRIMARY
        } else {
            COLOR_BORDER
        };
        draw_rounded_rect(
            buffer,
            w,
            s(fx as u32) as i32,
            input_y_raw,
            input_w,
            input_h,
            8,
            border_col,
            w,
            h,
        );
        draw_rounded_rect(
            buffer,
            w,
            s(fx as u32) as i32 + 1,
            input_y_raw + 1,
            input_w - 2,
            input_h.saturating_sub(2),
            7,
            COLOR_BG_CARD,
            w,
            h,
        );

        let val_chars: Vec<char> = val.chars().collect();
        let is_masked = (i == 0 || i == 6) && !val.is_empty() && !state.show_api_key;
        let mut display_chars: Vec<char> = if is_masked {
            let mask_char = if is_focused { '•' } else { '*' };
            std::iter::repeat(mask_char)
                .take(val_chars.len().min(32))
                .collect()
        } else {
            if val.is_empty() {
                (if is_focused { "" } else { "None" }).chars().collect()
            } else {
                val_chars.clone()
            }
        };

        let display_col = if val.is_empty() {
            COLOR_TEXT_DIM
        } else {
            COLOR_TEXT_MAIN
        };

        if i == 0 || i == 6 {
            let eye_x = s(fx as u32 + fw as u32 - 45) as i32;
            let eye_y = input_y_raw + sc(12.0) as i32;
            let eye_col = if state.show_api_key {
                COLOR_PRIMARY
            } else {
                COLOR_TEXT_SEC
            };
            draw_rect(buffer, w, eye_x, eye_y + 4, 16, 16, eye_col, w, h);
        }

        if i == 7 {
            // Multi-line rendering for System Prompt
            let final_text: String = display_chars.iter().collect();
            let max_width = sc(500.0 - 40.0) as u32;
            let (_, layout_h) = get_metrics_dw(&final_text, sc(14.0), max_width);
            let full_content_h_px = layout_h + sc(20.0);

            let sys_logical_y = input_y_raw as f64 / scale as f64;
            let sys_logical_h = input_h as f64 / scale as f64;

            *state.active_sys_prompt_rect =
                Some((230.0, sys_logical_y, 730.0, sys_logical_y + sys_logical_h));
            *state.active_sys_prompt_content_height = full_content_h_px / scale;

            let start_text_raw = input_y_raw + sc(12.0) as i32;
            let box_bottom_raw = input_y_raw + input_h as i32;

            if is_focused {
                if let Some(sel_start_idx) = state.selection_start {
                    let min_idx = sel_start_idx.min(state.cursor_pos);
                    let max_idx = sel_start_idx.max(state.cursor_pos);
                    if min_idx != max_idx {
                        let mut current_pos = 0;
                        for line in wrap_text(
                            &final_text,
                            &[],
                            rusttype::Scale::uniform(sc(14.0)),
                            max_width,
                        ) {
                            let line_len = line.chars().count();
                            let line_end = current_pos + line_len;
                            if max_idx > current_pos && min_idx < line_end {
                                let sel_in_line_start = min_idx.saturating_sub(current_pos);
                                let sel_in_line_end =
                                    (max_idx.saturating_sub(current_pos)).min(line_len);

                                let (lx_start, py_start) = get_xy_from_cursor_index(
                                    &final_text,
                                    sc(14.0),
                                    max_width,
                                    current_pos + sel_in_line_start,
                                );
                                let (lx_end, _) = get_xy_from_cursor_index(
                                    &final_text,
                                    sc(14.0),
                                    max_width,
                                    current_pos + sel_in_line_end,
                                );

                                let lx_width = (lx_end - lx_start).max(5.0);
                                let draw_x =
                                    s(fx as u32) as i32 + sc(15.0) as i32 + lx_start as i32;
                                let draw_y_f = start_text_raw as f32
                                    + py_start
                                    + (state.system_prompt_scroll_offset * scale);

                                if draw_y_f >= start_text_raw as f32 - sc(15.0)
                                    && draw_y_f <= (box_bottom_raw as f32 - sc(10.0))
                                {
                                    draw_rect_alpha(
                                        buffer,
                                        w,
                                        draw_x,
                                        draw_y_f as i32,
                                        lx_width as u32,
                                        sc(22.0) as u32,
                                        0x00AADDFF,
                                        0.4,
                                        w,
                                        h,
                                    );
                                }
                            }
                            current_pos += line_len;
                        }
                    }
                }
            }

            draw_text_ex(
                buffer,
                w,
                &final_text,
                s(fx as u32) as i32 + sc(15.0) as i32,
                start_text_raw,
                sc(14.0),
                display_col,
                max_width,
                input_h.saturating_sub(sc(20.0) as u32),
                state.system_prompt_scroll_offset * scale,
            );

            if is_focused {
                let (px, py) = get_xy_from_cursor_index(
                    &final_text,
                    sc(14.0),
                    sc(500.0 - 40.0) as u32,
                    state.cursor_pos,
                );

                let cursor_x = s(fx as u32) as i32 + sc(15.0) as i32 + px as i32;
                let cursor_y =
                    start_text_raw as f32 + py + (state.system_prompt_scroll_offset * scale);

                let cursor_visible =
                    (std::time::Instant::now() - state.last_cursor_action).as_millis() % 1000 < 500;

                if cursor_y >= start_text_raw as f32 - 1.0
                    && cursor_y <= (box_bottom_raw as f32 - sc(20.0))
                    && cursor_visible
                {
                    draw_rect(
                        buffer,
                        w,
                        cursor_x,
                        cursor_y as i32,
                        2,
                        sc(22.0) as u32,
                        COLOR_PRIMARY,
                        w,
                        h,
                    );
                }
            }

            // Scrollbar logic
            let max_sys_visual_h = sc(250.0);
            if full_content_h_px > max_sys_visual_h {
                let sb_w = sc(4.0) as u32;
                let sb_h = sc(240.0) as u32;
                let sb_x = s(fx as u32 + fw as u32 - 10) as i32;
                let sb_y = input_y_raw + sc(5.0) as i32;
                draw_rect(buffer, w, sb_x, sb_y, sb_w, sb_h, COLOR_BORDER, w, h);

                let ratio = max_sys_visual_h / full_content_h_px;
                let h_h = (sb_h as f32 * ratio).max(sc(20.0));
                let max_scroll = -(full_content_h_px - max_sys_visual_h);
                let progress = if max_scroll.abs() < 1.0 {
                    0.0
                } else {
                    state.system_prompt_scroll_offset * scale / max_scroll
                };
                let h_y = sb_y as f32 + (sb_h as f32 - h_h) * progress;
                draw_rect(
                    buffer, w, sb_x, h_y as i32, sb_w, h_h as u32, 0x00A0A0A0, w, h,
                );
            }
        } else {
            // Single line
            if !is_focused && display_chars.len() > 50 {
                display_chars = display_chars.iter().take(47).cloned().collect();
                display_chars.extend("...".chars());
            }
            let final_text: String = display_chars.iter().collect();
            let text_start_x = s(fx as u32) as i32 + sc(15.0) as i32;
            let text_start_y = input_y_raw as i32 + sc(12.0) as i32;

            if is_focused {
                if let Some(sel_start_idx) = state.selection_start {
                    let min_idx = sel_start_idx.min(state.cursor_pos).min(display_chars.len());
                    let max_idx = sel_start_idx.max(state.cursor_pos).min(display_chars.len());
                    if min_idx != max_idx {
                        let left_s: String = display_chars[..min_idx].iter().collect();
                        let mid_s: String = display_chars[min_idx..max_idx].iter().collect();
                        let lx = text_width(&[], &left_s, Scale::uniform(sc(14.0)));
                        let mx = text_width(&[], &mid_s, Scale::uniform(sc(14.0)));
                        draw_rect_alpha(
                            buffer,
                            w,
                            text_start_x + lx as i32,
                            text_start_y,
                            mx,
                            sc(22.0) as u32,
                            0x00AADDFF,
                            0.4,
                            w,
                            h,
                        );
                    }
                }

                draw_text(
                    buffer,
                    w,
                    &[],
                    &final_text,
                    text_start_x,
                    text_start_y,
                    sc(14.0),
                    display_col,
                );

                let cur_idx = state.cursor_pos.min(display_chars.len());
                let left_s: String = display_chars[..cur_idx].iter().collect();
                let lx = text_width(&[], &left_s, Scale::uniform(sc(14.0)));
                let cursor_x = text_start_x + lx as i32 + 1;
                let cursor_visible =
                    (std::time::Instant::now() - state.last_cursor_action).as_millis() % 1000 < 500;
                if cursor_x < (s(fx as u32) + input_w) as i32 && cursor_visible {
                    draw_rect(
                        buffer,
                        w,
                        cursor_x,
                        text_start_y,
                        2,
                        sc(22.0) as u32,
                        COLOR_PRIMARY,
                        w,
                        h,
                    );
                }
            } else {
                draw_text(
                    buffer,
                    w,
                    &[],
                    &final_text,
                    text_start_x,
                    text_start_y,
                    sc(14.0),
                    display_col,
                );
            }
        }
    }

    // Content height tracking
    let viewport_h_phys = sc(600.0);
    // Last card is System Prompt at 630.0, height 200.0. Add 100 padding.
    let content_h_phys = sc(630.0 + 200.0 + 100.0);
    (viewport_h_phys, content_h_phys)
}
