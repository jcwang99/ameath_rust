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
    pub system_prompt_metrics_cache: f32,
    pub system_prompt_hash: u64,
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
    let card_h = (1300.0 * scale) as u32;
    let card_y_raw = (sy_val(120) as f32 + scroll_y * scale) as i32;

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

    let fields_count = 12;
    // Calculate visible range
    let viewport_min_y = card_y_raw as f32;
    let viewport_max_y = h as f32;

    for i in 0..fields_count {
        let (label, fx, fy, fw, is_multiline) = match i {
            0 => ("API Key", 230.0, 30.0, 500.0, false),
            1 => ("Base URL", 230.0, 130.0, 500.0, false),
            2 => ("Model", 230.0, 230.0, 500.0, false),
            3 => ("ReAct Steps", 230.0, 330.0, 150.0, false),
            4 => ("L1 Summary", 405.0, 330.0, 150.0, false),
            5 => ("L2 Merge", 580.0, 330.0, 150.0, false),
            6 => ("Interact Interval (min)", 230.0, 430.0, 150.0, false),
            7 => ("Tavily Key", 230.0, 530.0, 500.0, false),
            8 => ("Brave Key", 230.0, 630.0, 500.0, false),
            9 => ("Firecrawl URL", 230.0, 730.0, 500.0, false),
            10 => ("Firecrawl Key", 230.0, 830.0, 500.0, false),
            11 => ("System Prompt", 230.0, 930.0, 500.0, true),
            _ => ("", 0.0, 0.0, 0.0, false),
        };

        let fy_scaled_raw = card_y_raw + sc(fy) as i32;
        let input_y_raw = fy_scaled_raw + sc(25.0) as i32;
        let input_h = if is_multiline {
            let input_h_logical = 250.0;
            sc(input_h_logical) as u32
        } else {
            sc(45.0) as u32
        };

        // Clipping Check: Skip field if completely outside viewport
        if fy_scaled_raw as f32 + input_h as f32 + sc(20.0) < viewport_min_y
            || fy_scaled_raw as f32 > viewport_max_y
        {
            continue;
        }

        // Use references to avoid cloning large strings every frame
        let temp_num: String;
        let val: &str = match i {
            0 => &ai_config.api_key,
            1 => &ai_config.base_url,
            2 => &ai_config.model,
            3 => {
                temp_num = ai_config.react_limit.to_string();
                &temp_num
            }
            4 => {
                temp_num = ai_config.l1_summary_threshold.to_string();
                &temp_num
            }
            5 => {
                temp_num = ai_config.l2_merge_threshold.to_string();
                &temp_num
            }
            6 => {
                temp_num = ai_config.interaction_frequency.to_string();
                &temp_num
            }
            7 => &ai_config.tavily_api_key,
            8 => &ai_config.brave_api_key,
            9 => &ai_config.firecrawl_url,
            10 => &ai_config.firecrawl_api_key,
            11 => &ai_config.system_prompt,
            _ => "",
        };

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

        let input_w = sc(fw as f32) as u32;

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
        let is_masked =
            (i == 0 || i == 7 || i == 8 || i == 10) && !val.is_empty() && !state.show_api_key;
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

        if i == 0 || i == 7 || i == 8 || i == 10 {
            let eye_x = s(fx as u32 + fw as u32 - 45) as i32;
            let eye_y = input_y_raw + sc(12.0) as i32;
            let eye_col = if state.show_api_key {
                COLOR_PRIMARY
            } else {
                COLOR_TEXT_SEC
            };
            draw_rect(buffer, w, eye_x, eye_y + 4, 16, 16, eye_col, w, h);
        }

        if i == 11 {
            // Multi-line rendering for System Prompt
            let final_text: String = display_chars.iter().collect();
            let max_width = sc(500.0 - 40.0) as u32;
            // Optimization: Use cached metrics if possible
            let layout_h = if state.system_prompt_metrics_cache > 0.0 {
                state.system_prompt_metrics_cache
            } else {
                let (_, mh) = get_metrics_dw(&final_text, sc(14.0), max_width);
                mh
            };
            let full_content_h_px = layout_h + sc(24.0);

            let sys_logical_y = 120.0 + fy + 25.0;
            let sys_logical_h = if is_multiline { 250.0 } else { 45.0 };

            *state.active_sys_prompt_rect = Some((
                230.0,
                sys_logical_y as f64,
                730.0,
                (sys_logical_y + sys_logical_h) as f64,
            ));
            *state.active_sys_prompt_content_height = full_content_h_px / scale;

            let start_text_raw = input_y_raw + sc(12.0) as i32;
            let box_bottom_raw = input_y_raw + input_h as i32;

            if is_focused {
                if let Some(sel_start_idx) = state.selection_start {
                    let rects = get_selection_rects(
                        &final_text,
                        sc(14.0),
                        max_width,
                        sel_start_idx,
                        state.cursor_pos,
                    );
                    for (rx, ry, rw, rh) in rects {
                        let draw_x = s(fx as u32) as i32 + sc(15.0) as i32 + rx as i32;
                        let draw_y_f = start_text_raw as f32
                            + ry
                            + (state.system_prompt_scroll_offset * scale);

                        if draw_y_f >= (start_text_raw as f32 - rh)
                            && draw_y_f <= (box_bottom_raw as f32)
                        {
                            draw_rect_alpha(
                                buffer,
                                w,
                                draw_x,
                                draw_y_f as i32,
                                rw as u32,
                                rh as u32,
                                0x00AADDFF,
                                0.4,
                                w,
                                h,
                            );
                        }
                    }
                }
            }

            draw_text_dw_h(
                buffer,
                w,
                &final_text,
                state.system_prompt_hash,
                s(fx as u32) as i32 + sc(15.0) as i32,
                start_text_raw,
                sc(14.0),
                display_col,
                max_width,
                input_h.saturating_sub(sc(24.0) as u32), // Standardized 12+12 padding
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
    let viewport_h = 600.0;
    let content_h = 930.0 + 250.0 + 170.0;
    (viewport_h, content_h)
}
