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
    pub system_prompt_metrics_cache: &'a mut f32,
    pub system_prompt_hash: u64,
    pub config_hash: u64,
    pub draw_cursor: bool,
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
) -> (f32, f32, Option<(i32, i32, u32, u32)>) {
    let scrollable_h_logical = (h as f32 / scale) - 130.0;
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };
    let sc = |val: f32| -> f32 { val * scale };
    let mut cursor_rect = None;

    let card_w = (560.0 * scale) as u32;
    let card_h = (1450.0 * scale) as u32;
    let card_y_raw = (sy_val(120) as f32 + scroll_y * scale) as i32;
    let fields_count = 12;

    // WHOLE CARD CACHING FOR AI TAB
    let mut ai_card_hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    state.config_hash.hash(&mut ai_card_hasher);
    state.focused_field.hash(&mut ai_card_hasher);
    state.show_api_key.hash(&mut ai_card_hasher);
    state
        .system_prompt_scroll_offset
        .to_bits()
        .hash(&mut ai_card_hasher);
    scale.to_bits().hash(&mut ai_card_hasher);
    let ai_card_hash = ai_card_hasher.finish();

    let ai_card_key = LayoutKey {
        text_hash: ai_card_hash,
        font_size_bits: scale.to_bits(),
        max_w: card_w,
        font_family_hash: 0xBAADF00D, // Unique sentinel for AI tab
        is_bold: false,
        is_centered: false,
    };

    let ai_blit_success = {
        let cache = get_raster_cache().read().unwrap();
        if let Some(entry) = cache.map.get(&ai_card_key) {
            blit_opaque(
                buffer,
                w,
                s(210) as i32,
                card_y_raw,
                entry.tw,
                entry.th,
                &entry.pixels,
                w,
                h,
                0,
            );
            true
        } else {
            false
        }
    };

    if !ai_blit_success {
        // Render the entire 1300px card into a temporary buffer
        let mut ai_card_buffer = vec![COLOR_BG_APP; (card_w * card_h) as usize];

        // 1. Background
        draw_rounded_rect_internal(
            &mut ai_card_buffer,
            card_w,
            0,
            0,
            card_w,
            card_h,
            12,
            COLOR_BG_CARD,
        );

        let mut temp_num = String::new();
        for i in 0..fields_count {
            temp_num.clear();
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

            let fx_rel = sc(fx - 210.0) as i32;
            let fy_rel = sc(fy) as i32;
            let input_y_rel = fy_rel + sc(25.0) as i32;
            let input_h = if is_multiline {
                sc(250.0) as u32
            } else {
                sc(45.0) as u32
            };

            let input_w = sc(fw as f32) as u32;

            let is_focused = state.focused_field == Some(i);
            let border_col = if is_focused {
                COLOR_PRIMARY
            } else {
                COLOR_BORDER
            };

            // Label
            draw_text_dw_ex(
                &mut ai_card_buffer,
                card_w,
                label,
                fx_rel,
                fy_rel,
                sc(14.0),
                COLOR_TEXT_SEC,
                card_w,
                sc(20.0) as u32,
                0.0,
            );

            // Input Border
            draw_rounded_rect_internal(
                &mut ai_card_buffer,
                card_w,
                fx_rel as u32,
                input_y_rel as u32,
                input_w,
                input_h,
                8,
                border_col,
            );

            // Input BG
            draw_rounded_rect_internal(
                &mut ai_card_buffer,
                card_w,
                (fx_rel + 1) as u32,
                (input_y_rel + 1) as u32,
                input_w - 2,
                input_h.saturating_sub(2),
                7,
                COLOR_BG_CARD,
            );

            // Value
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

            let val_chars: Vec<char> = val.chars().collect();
            let is_masked =
                (i == 0 || i == 7 || i == 8 || i == 10) && !val.is_empty() && !state.show_api_key;
            let display_chars: Vec<char> = if is_masked {
                let mask_char = if is_focused { '•' } else { '*' };
                std::iter::repeat(mask_char)
                    .take(val_chars.len().min(32))
                    .collect()
            } else {
                if val.is_empty() {
                    (if is_focused { "" } else { "None" }).chars().collect()
                } else {
                    if !is_focused && val_chars.len() > 50 && i != 11 {
                        let mut v = val_chars.iter().take(47).cloned().collect::<Vec<_>>();
                        v.extend("...".chars());
                        v
                    } else {
                        val_chars.clone()
                    }
                }
            };

            let display_col = if val.is_empty() {
                COLOR_TEXT_DIM
            } else {
                COLOR_TEXT_MAIN
            };
            let final_text: String = display_chars.iter().collect();

            if i == 11 {
                // Multi-line for System Prompt
                draw_text_dw_h(
                    &mut ai_card_buffer,
                    card_w,
                    &final_text,
                    state.system_prompt_hash,
                    fx_rel + sc(15.0) as i32,
                    input_y_rel + sc(12.0) as i32,
                    sc(14.0),
                    display_col,
                    sc(500.0 - 40.0) as u32,
                    input_h.saturating_sub(sc(24.0) as u32),
                    state.system_prompt_scroll_offset * scale,
                );
            } else {
                draw_text_dw_ex(
                    &mut ai_card_buffer,
                    card_w,
                    &final_text,
                    fx_rel + sc(15.0) as i32,
                    input_y_rel + sc(12.0) as i32,
                    sc(14.0),
                    display_col,
                    input_w - sc(30.0) as u32,
                    sc(30.0) as u32,
                    0.0,
                );
            }

            // Eye Icon for password fields
            if i == 0 || i == 7 || i == 8 || i == 10 {
                let eye_x = fx_rel + sc(fw as f32 - 45.0) as i32;
                let eye_y = input_y_rel + sc(12.0) as i32;
                let eye_col = if state.show_api_key {
                    COLOR_PRIMARY
                } else {
                    COLOR_TEXT_SEC
                };
                // Draw eye as a simple rect for now
                draw_rect_internal(
                    &mut ai_card_buffer,
                    card_w,
                    eye_x as u32,
                    (eye_y + 4) as u32,
                    16,
                    16,
                    eye_col,
                );
            }
        }

        // Blit to screen
        blit_opaque(
            buffer,
            w,
            s(210) as i32,
            card_y_raw,
            card_w as i32,
            card_h as i32,
            &ai_card_buffer,
            w,
            h,
            0,
        );

        // Store in cache
        let pixel_count = ai_card_buffer.len();
        let mut cache = get_raster_cache().write().unwrap();
        while cache.total_pixels + pixel_count > (16 * 1024 * 1024 / 4) && !cache.order.is_empty() {
            let oldest = cache.order.remove(0);
            if let Some(entry) = cache.map.remove(&oldest) {
                cache.total_pixels -= entry.pixel_count;
            }
        }
        cache.order.push(ai_card_key.clone());
        cache.total_pixels += pixel_count;
        cache.map.insert(
            ai_card_key,
            RasterEntry {
                pixels: ai_card_buffer,
                tw: card_w as i32,
                th: card_h as i32,
                pixel_count,
            },
        );
    }

    // Interaction State Restoration (Required for scrolling/clicking prompt regardless of cache)
    {
        let fy = 930.0;
        let fy_scaled_raw = card_y_raw + sc(fy) as i32;
        let _input_y_raw = fy_scaled_raw + sc(25.0) as i32;

        let max_width = sc(500.0 - 40.0) as u32;
        let layout_h: f32 = if *state.system_prompt_metrics_cache > 0.0f32 {
            *state.system_prompt_metrics_cache
        } else {
            let (_, mh) = get_metrics_dw(&ai_config.system_prompt, sc(14.0), max_width);
            *state.system_prompt_metrics_cache = mh;
            mh
        };
        let full_content_h_px = layout_h + sc(24.0);

        let sys_logical_y = 120.0 + fy + 25.0;
        let sys_logical_h = 250.0;

        *state.active_sys_prompt_rect = Some((
            230.0,
            sys_logical_y as f64,
            730.0,
            (sys_logical_y + sys_logical_h) as f64,
        ));
        *state.active_sys_prompt_content_height = full_content_h_px / scale;
    }

    // EXTRA OVERLAYS (Cursor, focus indications that need to be dynamic)
    let mut temp_num = String::new();
    if let Some(i) = state.focused_field {
        temp_num.clear();
        let (_label, fx, fy, _fw, is_multiline) = match i {
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
            sc(250.0) as u32
        } else {
            sc(45.0) as u32
        };
        let box_bottom_raw = input_y_raw + input_h as i32;

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

        let val_chars: Vec<char> = val.chars().collect();
        let is_masked =
            (i == 0 || i == 7 || i == 8 || i == 10) && !val.is_empty() && !state.show_api_key;
        let display_chars: Vec<char> = if is_masked {
            let mask_char = '•';
            std::iter::repeat(mask_char)
                .take(val_chars.len().min(32))
                .collect()
        } else {
            if val.is_empty() {
                Vec::new()
            } else {
                val_chars
            }
        };

        let final_text: String = display_chars.iter().collect();
        let text_start_x = s(fx as u32) as i32 + sc(15.0) as i32;
        let text_start_y = input_y_raw + sc(12.0) as i32;

        // SELECTION DRAWING
        if let Some(sel_start_idx) = state.selection_start {
            if i == 11 {
                let rects = get_selection_rects(
                    &final_text,
                    sc(14.0),
                    sc(500.0 - 40.0) as u32,
                    sel_start_idx,
                    state.cursor_pos,
                );
                for (rx, ry, rw, rh) in rects {
                    let draw_y_f =
                        text_start_y as f32 + ry + (state.system_prompt_scroll_offset * scale);
                    if draw_y_f >= (text_start_y as f32 - rh) && draw_y_f <= (box_bottom_raw as f32)
                    {
                        draw_rect_alpha(
                            buffer,
                            w,
                            text_start_x + rx as i32,
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
            } else {
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
        }

        let (px, py) = if i == 11 {
            get_xy_from_cursor_index(
                &final_text,
                sc(14.0),
                sc(500.0 - 40.0) as u32,
                state.cursor_pos,
            )
        } else {
            let cur_idx = state.cursor_pos.min(display_chars.len());
            let left_s: String = display_chars[..cur_idx].iter().collect();
            let lx = text_width(&[], &left_s, Scale::uniform(sc(14.0)));
            (lx as f32, 0.0)
        };

        let cursor_x = text_start_x + px as i32;
        let cursor_y = text_start_y as f32
            + py
            + (if i == 11 {
                state.system_prompt_scroll_offset * scale
            } else {
                0.0
            });

        let cursor_visible =
            (std::time::Instant::now() - state.last_cursor_action).as_millis() % 1000 < 500;
        if cursor_y >= (input_y_raw as f32) && cursor_y <= (box_bottom_raw as f32 - sc(20.0)) {
            // Filter out clearly invalid coordinates before caching
            if cursor_x >= 0 && cursor_y >= 0.0 {
                cursor_rect = Some((cursor_x, cursor_y as i32, 2, sc(22.0) as u32));
            }
            if state.draw_cursor && cursor_visible {
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
    }

    // System Prompt Scrollbar
    {
        let fy = 930.0;
        let fy_scaled_raw = card_y_raw + sc(fy) as i32;
        let input_y_raw = fy_scaled_raw + sc(25.0) as i32;
        let input_h = sc(250.0);
        let max_width = sc(500.0 - 40.0) as u32;
        let layout_h: f32 = if *state.system_prompt_metrics_cache > 0.0f32 {
            *state.system_prompt_metrics_cache
        } else {
            let (_, mh) = get_metrics_dw(&ai_config.system_prompt, sc(14.0), max_width);
            mh
        };
        let full_content_h_px = layout_h + sc(24.0);

        if full_content_h_px > input_h {
            let sb_w = sc(4.0) as u32;
            let sb_x = s(230 + 480);
            draw_rect(
                buffer,
                w,
                sb_x as i32,
                input_y_raw,
                sb_w,
                input_h as u32,
                COLOR_BORDER,
                w,
                h,
            );
            let ratio = input_h / full_content_h_px;
            let thumb_h = (input_h * ratio).max(sc(20.0));
            let max_scroll = -(full_content_h_px - input_h);
            let progress = if max_scroll.abs() < 1.0 {
                0.0
            } else {
                (state.system_prompt_scroll_offset * scale / max_scroll).clamp(0.0, 1.0)
            };
            let thumb_y = input_y_raw as f32 + (input_h - thumb_h) * progress;
            draw_rect(
                buffer,
                w,
                sb_x as i32,
                thumb_y as i32,
                sb_w,
                thumb_h as u32,
                0x00A0A0A0,
                w,
                h,
            );
        }
    }

    // Content height tracking
    let viewport_h = scrollable_h_logical;
    let content_h = 1450.0;
    (viewport_h, content_h, cursor_rect)
}
