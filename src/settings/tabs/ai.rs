use crate::theme::*;
use crate::types::AiConfig;
use crate::ui_primitives::*;

thread_local! {
    // Reduced from 512x512 (1MB) to 256x256 (256KB) to save memory
    static AI_SCRATCH_BUFFER: std::cell::RefCell<Vec<u32>> = std::cell::RefCell::new(Vec::with_capacity(256 * 256));
}

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
    pub draw_cursor: bool,
    pub mouse_pos: (f32, f32), // DESIGN SPACE relative to window top (lx, ly)
    pub content_mouse_pos: (f32, f32), // DESIGN SPACE relative to scroll content (dlx, dly)
    pub pressed_btn: Option<usize>,
    pub show_delete_dialog: bool,
    pub notification: Option<(String, std::time::Instant)>,
    pub field_scroll_offsets: [f32; 18],
    pub draw_card_background: bool,
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
    let card_h = (1950.0 * scale) as u32;
    let card_y_raw = (sy_val(120) as f32 + scroll_y * scale) as i32;
    let fields_count = 18;

    // Viewport boundaries (for visibility check)
    let min_y_vis = sy_val(120) as i32;
    let max_y_vis = h as i32;

    // 1. Background (Directly into main buffer, clipped to viewport)
    let card_start_x = s(210) as i32;
    let card_box_h = card_h as i32;
    if state.draw_card_background
        && (card_y_raw + card_box_h) >= min_y_vis
        && card_y_raw <= max_y_vis
    {
        draw_rounded_rect(
            buffer,
            w,
            card_start_x,
            card_y_raw,
            card_w,
            card_h,
            12,
            COLOR_BG_CARD,
            w,
            h,
        );
    }

    let mut temp_num = String::new();
    let active_profile = ai_config.active_profile();

    for i in 0..fields_count {
        temp_num.clear();
        let label_owned: String;
        let (label, fx, fy, fw, is_multiline) = match i {
            0 => {
                label_owned = format!(
                    "Active Profile ({}/{})",
                    ai_config.active_profile_index + 1,
                    ai_config.profiles.len()
                );
                (&label_owned as &str, 265.0, 30.0, 160.0, false)
            }
            1 => ("Multimodal (Vision)", 565.0, 30.0, 45.0, false),
            2 => ("API Key", 230.0, 130.0, 500.0, false),
            3 => ("Base URL", 230.0, 230.0, 500.0, false),
            4 => ("Model", 230.0, 330.0, 500.0, false),
            5 => ("ReAct Steps", 230.0, 430.0, 150.0, false),
            6 => ("L1 Summary", 405.0, 430.0, 150.0, false),
            7 => ("L2 Merge", 580.0, 430.0, 150.0, false),
            8 => ("Interact Interval (min)", 230.0, 530.0, 150.0, false),
            9 => ("Tavily Key", 230.0, 630.0, 500.0, false),
            10 => ("Brave Key", 230.0, 730.0, 500.0, false),
            11 => ("Firecrawl URL", 230.0, 830.0, 500.0, false),
            12 => ("Firecrawl Key", 230.0, 930.0, 500.0, false),
            13 => ("System Prompt", 230.0, 1030.0, 500.0, true),
            14 => (
                "Allow Screen Capture (Routine Checks)",
                405.0,
                530.0,
                20.0,
                false,
            ),
            15 => ("TTS Enabled (CosyVoice 3)", 230.0, 1330.0, 20.0, false),
            16 => ("TTS Ref Audio Path", 230.0, 1430.0, 500.0, false),
            17 => ("TTS Prompt Text", 230.0, 1530.0, 500.0, false),
            _ => ("", 0.0, 0.0, 0.0, false),
        };

        if i == 1 {
            // Multimodal toggle is handled specially below
            continue;
        }

        if i == 14 || i == 15 {
            if i == 14 && !ai_config.active_profile().is_multimodal {
                continue;
            }
            // Checkbox for Screen Capture or TTS
            let is_checked = if i == 14 {
                ai_config.active_interaction_screenshots_enabled
            } else {
                ai_config.tts_enabled
            };

            let box_x = s(fx as u32) as i32;
            // FIXED: Add sc(25.0) for field start + sc(12.5) for vertical centering (45-20)/2 = 37.5
            let box_y = card_y_raw + sc(fy + 25.0 + 12.5) as i32;
            let box_size = sc(20.0) as i32;

            // Box Background
            draw_rect(
                buffer,
                w,
                box_x,
                box_y,
                box_size as u32,
                box_size as u32,
                COLOR_BG_LIGHT,
                w,
                h,
            );

            // Box Outline (Manual)
            let border_color = COLOR_BORDER;
            draw_rect(
                buffer,
                w,
                box_x,
                box_y,
                box_size as u32,
                1,
                border_color,
                w,
                h,
            ); // Top
            draw_rect(
                buffer,
                w,
                box_x,
                box_y + box_size - 1,
                box_size as u32,
                1,
                border_color,
                w,
                h,
            ); // Bottom
            draw_rect(
                buffer,
                w,
                box_x,
                box_y,
                1,
                box_size as u32,
                border_color,
                w,
                h,
            ); // Left
            draw_rect(
                buffer,
                w,
                box_x + box_size - 1,
                box_y,
                1,
                box_size as u32,
                border_color,
                w,
                h,
            ); // Right

            if is_checked {
                let inner = sc(12.0) as i32;
                let offset = (box_size - inner) / 2;
                draw_rect(
                    buffer,
                    w,
                    box_x + offset,
                    box_y + offset,
                    inner as u32,
                    inner as u32,
                    COLOR_PRIMARY,
                    w,
                    h,
                );
            }

            draw_text_dw_ex(
                buffer,
                w,
                label,
                (box_x + box_size + 10) as i32,
                box_y + sc(1.0) as i32, // Vertically center text with box
                sc(14.0),               // Smaller font
                COLOR_TEXT_MAIN,
                (w as f32 - (box_x + box_size + 10) as f32) as u32,
                sc(30.0) as u32,
                0.0,
                0.0,
                1000000,
            );
            continue;
        }

        let fx_abs = s(fx as u32) as i32;
        let fy_abs = card_y_raw + sc(fy) as i32;
        let input_y_abs = fy_abs + sc(25.0) as i32;
        let input_h = if is_multiline {
            sc(250.0) as u32
        } else {
            sc(45.0) as u32
        };
        let input_w = sc(fw as f32) as u32;

        // Visibility Check for individual fields
        if (fy_abs + (input_h as i32 + sc(25.0) as i32)) < min_y_vis || fy_abs > max_y_vis {
            continue;
        }

        if i == 16 {
            // Button-style for Reference Audio Path
            let is_hovered = state.content_mouse_pos.0 >= fx
                && state.content_mouse_pos.0 <= fx + fw
                && state.content_mouse_pos.1 >= fy + 25.0
                && state.content_mouse_pos.1 <= fy + 25.0 + 45.0;

            let border_col = if is_hovered {
                COLOR_PRIMARY
            } else {
                COLOR_BORDER
            };

            draw_rounded_rect(
                buffer,
                w,
                s(fx as u32) as i32,
                card_y_raw + sc(fy + 25.0) as i32,
                sc(fw) as u32,
                sc(45.0) as u32,
                8,
                border_col,
                w,
                h,
            );
            draw_rounded_rect(
                buffer,
                w,
                (s(fx as u32) as i32).saturating_add(1),
                card_y_raw + sc(fy + 25.0) as i32 + 1,
                (sc(fw) as u32).saturating_sub(2),
                (sc(45.0) as u32).saturating_sub(2),
                7,
                COLOR_BG_CARD,
                w,
                h,
            );

            let ref_path = ai_config.tts_reference_audio.to_string_lossy();
            let path_str = if ref_path.is_empty() {
                "Select reference audio...".to_string()
            } else {
                ref_path.to_string()
            };

            draw_text(
                buffer,
                w,
                &[],
                &path_str,
                fx_abs.saturating_add(sc(15.0) as i32),
                input_y_abs.saturating_add(sc(12.0) as i32),
                sc(14.0),
                COLOR_TEXT_SEC,
            );
            continue;
        }

        let is_focused = state.focused_field == Some(i);
        let border_col = if is_focused {
            COLOR_PRIMARY
        } else {
            COLOR_BORDER
        };

        // Label
        draw_text_dw_ex(
            buffer,
            w,
            label,
            fx_abs,
            fy_abs,
            sc(14.0),
            COLOR_TEXT_SEC,
            w,
            sc(20.0) as u32,
            0.0,
            0.0,
            w,
        );

        // Input Border & BG
        draw_rounded_rect(
            buffer,
            w,
            fx_abs,
            input_y_abs,
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
            fx_abs + 1,
            input_y_abs + 1,
            input_w - 2,
            input_h.saturating_sub(2),
            7,
            COLOR_BG_CARD,
            w,
            h,
        );

        if i == 13 {
            // System Prompt content is handled by the dynamic overlay
            continue;
        }

        let val: &str = match i {
            0 => &active_profile.name,
            2 => &active_profile.api_key,
            3 => &active_profile.base_url,
            4 => &active_profile.model,
            5 => {
                temp_num = ai_config.react_limit.to_string();
                &temp_num
            }
            6 => {
                temp_num = ai_config.l1_summary_threshold.to_string();
                &temp_num
            }
            7 => {
                temp_num = ai_config.l2_merge_threshold.to_string();
                &temp_num
            }
            8 => {
                temp_num = ai_config.interaction_frequency.to_string();
                &temp_num
            }
            9 => &ai_config.tavily_api_key,
            10 => &ai_config.brave_api_key,
            11 => &ai_config.firecrawl_url,
            12 => &ai_config.firecrawl_api_key,
            17 => &ai_config.tts_prompt_text,
            _ => "",
        };

        let val_chars: Vec<char> = val.chars().collect();
        let is_masked =
            (i == 2 || i == 9 || i == 10 || i == 12) && !val.is_empty() && !state.show_api_key;
        let display_chars: Vec<char> = if is_masked {
            let mask_char = if is_focused { '•' } else { '*' };
            std::iter::repeat(mask_char).take(val_chars.len()).collect()
        } else {
            if val.is_empty() {
                (if is_focused { "" } else { "None" }).chars().collect()
            } else {
                if !is_focused && val_chars.len() > 50 {
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

        draw_text_dw_ex(
            buffer,
            w,
            &final_text,
            fx_abs + sc(15.0) as i32,
            input_y_abs + sc(12.0) as i32,
            sc(14.0),
            display_col,
            input_w.saturating_sub(sc(30.0) as u32),
            sc(30.0) as u32,
            0.0,
            state.field_scroll_offsets[i],
            1000000,
        );

        // Profile Controls [<] [>] [+] [-] standardized row
        if i == 0 {
            let btn_x_start = s(230) as i32;
            let btn_y_start = input_y_abs;
            let btn_w = sc(30.0) as u32;

            // [<] Prev Profile (at 230)
            let is_prev_hover = state.content_mouse_pos.0 >= 230.0
                && state.content_mouse_pos.0 <= 260.0
                && state.content_mouse_pos.1 >= 55.0
                && state.content_mouse_pos.1 <= 100.0;
            let prev_bg = if state.pressed_btn == Some(0) {
                COLOR_PRIMARY
            } else if is_prev_hover {
                0x00444444
            } else {
                COLOR_BG_CARD
            };
            draw_rounded_rect(
                buffer,
                w,
                btn_x_start,
                btn_y_start,
                btn_w,
                sc(45.0) as u32,
                8,
                prev_bg,
                w,
                h,
            );
            draw_text_dw_ex(
                buffer,
                w,
                "<",
                btn_x_start + sc(10.0) as i32,
                btn_y_start + sc(8.0) as i32,
                sc(20.0),
                COLOR_TEXT_MAIN,
                btn_w,
                sc(45.0) as u32,
                0.0,
                0.0,
                btn_w,
            );

            // [>] Next Profile (at 430)
            let next_x_design = 430.0;
            let next_x = s(next_x_design as u32) as i32;
            let is_next_hover = state.content_mouse_pos.0 >= next_x_design
                && state.content_mouse_pos.0 <= next_x_design + 30.0
                && state.content_mouse_pos.1 >= 55.0
                && state.content_mouse_pos.1 <= 100.0;
            let next_bg = if state.pressed_btn == Some(1) {
                COLOR_PRIMARY
            } else if is_next_hover {
                0x00444444
            } else {
                COLOR_BG_CARD
            };
            draw_rounded_rect(
                buffer,
                w,
                next_x,
                btn_y_start,
                btn_w,
                sc(45.0) as u32,
                8,
                next_bg,
                w,
                h,
            );
            draw_text_dw_ex(
                buffer,
                w,
                ">",
                next_x + sc(10.0) as i32,
                btn_y_start + sc(8.0) as i32,
                sc(20.0),
                COLOR_TEXT_MAIN,
                btn_w,
                sc(45.0) as u32,
                0.0,
                0.0,
                btn_w,
            );

            // [+] Add Profile (at 480)
            let add_x_design = 480.0;
            let add_x = s(add_x_design as u32) as i32;
            let add_w_design = 35.0;
            let add_w_abs = sc(add_w_design) as u32;
            let is_add_hover = state.content_mouse_pos.0 >= add_x_design
                && state.content_mouse_pos.0 <= add_x_design + add_w_design
                && state.content_mouse_pos.1 >= 55.0
                && state.content_mouse_pos.1 <= 100.0;
            let add_bg = if state.pressed_btn == Some(2) {
                COLOR_PRIMARY
            } else if is_add_hover {
                0x00444444
            } else {
                COLOR_BG_CARD
            };
            draw_rounded_rect(
                buffer,
                w,
                add_x,
                btn_y_start,
                add_w_abs,
                sc(45.0) as u32,
                8,
                add_bg,
                w,
                h,
            );
            draw_text_dw_ex(
                buffer,
                w,
                "+",
                add_x + sc(10.0) as i32,
                btn_y_start + sc(8.0) as i32,
                sc(20.0),
                COLOR_TEXT_MAIN,
                add_w_abs,
                sc(45.0) as u32,
                0.0,
                0.0,
                add_w_abs,
            );

            // [-] Delete Profile (at 525)
            let del_x_design = 525.0;
            let del_x = s(del_x_design as u32) as i32;
            let del_w_design = 35.0;
            let del_w_abs = sc(del_w_design) as u32;
            let is_del_hover = state.content_mouse_pos.0 >= del_x_design
                && state.content_mouse_pos.0 <= del_x_design + del_w_design
                && state.content_mouse_pos.1 >= 55.0
                && state.content_mouse_pos.1 <= 100.0;
            let del_bg = if state.pressed_btn == Some(3) {
                COLOR_PRIMARY
            } else if is_del_hover {
                0x00444444
            } else {
                COLOR_BG_CARD
            };
            draw_rounded_rect(
                buffer,
                w,
                del_x,
                btn_y_start,
                del_w_abs,
                sc(45.0) as u32,
                8,
                del_bg,
                w,
                h,
            );
            draw_text_dw_ex(
                buffer,
                w,
                "-",
                del_x + sc(14.0) as i32,
                btn_y_start + sc(6.0) as i32,
                sc(24.0),
                if is_del_hover { 0x00FFFFFF } else { 0x00FF6666 },
                del_w_abs,
                sc(45.0) as u32,
                0.0,
                0.0,
                sc(45.0) as u32,
            );
        }

        // Eye Icon
        if i == 2 || i == 9 || i == 10 || i == 12 {
            let eye_x = fx_abs + sc(fw as f32 - 45.0) as i32;
            let eye_y = input_y_abs + sc(12.0) as i32;
            let eye_col = if state.show_api_key {
                COLOR_PRIMARY
            } else {
                COLOR_TEXT_SEC
            };
            draw_rect(
                buffer,
                w,
                eye_x,
                eye_y + sc(4.0) as i32,
                sc(16.0) as u32,
                sc(16.0) as u32,
                eye_col,
                w,
                h,
            );
        }
    }

    // Multimodal Toggle (CheckBox style at 565)
    {
        let fx = 565.0;
        let fy = 30.0;
        let fx_abs = s(fx as u32) as i32;
        let fy_abs = card_y_raw + sc(fy) as i32;
        let toggle_y = fy_abs + sc(25.0) as i32;
        let toggle_dim = sc(45.0) as u32;

        let is_hover = state.content_mouse_pos.0 >= fx
            && state.content_mouse_pos.0 <= fx + 45.0
            && state.content_mouse_pos.1 >= fy + 25.0
            && state.content_mouse_pos.1 <= fy + 70.0;

        if fy_abs > min_y_vis && fy_abs < max_y_vis {
            draw_text_dw_ex(
                buffer,
                w,
                "Multimodal (Vision)",
                fx_abs,
                fy_abs,
                sc(14.0),
                COLOR_TEXT_SEC,
                sc(45.0) as u32 + 500,
                sc(20.0) as u32,
                0.0,
                0.0,
                sc(45.0) as u32 + 500,
            );
            let is_multimodal = active_profile.is_multimodal;
            let toggle_bg = if is_multimodal {
                COLOR_PRIMARY
            } else if state.pressed_btn == Some(101) {
                COLOR_PRIMARY
            } else if is_hover {
                0x00444444
            } else {
                COLOR_TEXT_SEC
            };

            // Draw border
            draw_rounded_rect(
                buffer, w, fx_abs, toggle_y, toggle_dim, toggle_dim, 8, toggle_bg, w, h,
            );
            // Draw inner box for border effect
            if !is_multimodal && state.pressed_btn != Some(101) && !is_hover {
                draw_rounded_rect(
                    buffer,
                    w,
                    fx_abs + 1,
                    toggle_y + 1,
                    toggle_dim - 2,
                    toggle_dim - 2,
                    7,
                    COLOR_BG_CARD,
                    w,
                    h,
                );
            }
            // Checkmark
            if is_multimodal {
                draw_text_dw_ex(
                    buffer,
                    w,
                    "✓",
                    fx_abs + sc(12.0) as i32,
                    toggle_y + sc(10.0) as i32,
                    sc(20.0),
                    0x00FFFFFF,
                    toggle_dim,
                    toggle_dim,
                    0.0,
                    0.0,
                    toggle_dim,
                );
            }
        }
    }

    // Response Mode Selector
    {
        let fx = 405.0;
        let fy = 1330.0;
        let fw = 325.0;
        let segment_w = fw / 3.0;
        let fx_abs = s(fx as u32) as i32;
        let fy_abs = card_y_raw + sc(fy) as i32;
        let input_y_abs = fy_abs + sc(25.0) as i32;
        let input_h = sc(45.0) as u32;

        draw_text_dw_ex(
            buffer,
            w,
            "Response Mode",
            fx_abs,
            fy_abs,
            sc(14.0),
            COLOR_TEXT_SEC,
            sc(fw) as u32,
            sc(20.0) as u32,
            0.0,
            0.0,
            sc(fw) as u32,
        );

        draw_rounded_rect(
            buffer,
            w,
            fx_abs,
            input_y_abs,
            sc(fw) as u32,
            input_h,
            10,
            COLOR_BORDER,
            w,
            h,
        );
        draw_rounded_rect(
            buffer,
            w,
            fx_abs + 1,
            input_y_abs + 1,
            sc(fw) as u32 - 2,
            input_h.saturating_sub(2),
            9,
            COLOR_BG_CARD,
            w,
            h,
        );

        let modes = [
            (crate::types::AiResponseMode::Auto, "Auto"),
            (crate::types::AiResponseMode::Streaming, "Streaming"),
            (crate::types::AiResponseMode::NonStreaming, "Non-stream"),
        ];

        for (idx, (mode, label)) in modes.iter().enumerate() {
            let seg_x = fx + segment_w * idx as f32;
            let seg_x_abs = s(seg_x as u32) as i32;
            let seg_w_abs = if idx == modes.len() - 1 {
                sc(fw - segment_w * idx as f32) as u32
            } else {
                sc(segment_w) as u32
            };
            let is_hovered = state.content_mouse_pos.0 >= seg_x
                && state.content_mouse_pos.0 <= seg_x + segment_w
                && state.content_mouse_pos.1 >= fy + 25.0
                && state.content_mouse_pos.1 <= fy + 70.0;
            let is_active = active_profile.response_mode == *mode;

            if is_active || is_hovered {
                draw_rounded_rect(
                    buffer,
                    w,
                    seg_x_abs + 2,
                    input_y_abs + 2,
                    seg_w_abs.saturating_sub(4),
                    input_h.saturating_sub(4),
                    8,
                    if is_active { COLOR_PRIMARY } else { 0x003A4048 },
                    w,
                    h,
                );
            }

            if idx > 0 {
                draw_rect(
                    buffer,
                    w,
                    seg_x_abs,
                    input_y_abs + sc(8.0) as i32,
                    1,
                    input_h.saturating_sub(sc(16.0) as u32),
                    0x00373D46,
                    w,
                    h,
                );
            }

            draw_text_dw_ex(
                buffer,
                w,
                label,
                seg_x_abs + sc(12.0) as i32,
                input_y_abs + sc(12.0) as i32,
                sc(14.0),
                if is_active {
                    0x00FFFFFF
                } else {
                    COLOR_TEXT_MAIN
                },
                seg_w_abs.saturating_sub(sc(20.0) as u32),
                sc(24.0) as u32,
                0.0,
                0.0,
                seg_w_abs.saturating_sub(sc(20.0) as u32),
            );
        }
    }

    // Interaction State Restoration for System Prompt
    {
        let fy = 1030.0;
        let fy_scaled_raw = card_y_raw + sc(fy) as i32;
        let _input_y_raw = fy_scaled_raw + sc(25.0) as i32;
        let _input_h = sc(250.0);

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

    // EXTRA OVERLAYS (Cursor, focus indications)
    if let Some(i) = state.focused_field {
        temp_num.clear();
        let (_label, fx, fy, fw, is_multiline) = match i {
            0 => ("Active Profile", 265.0, 30.0, 160.0, false),
            2 => ("API Key", 230.0, 130.0, 500.0, false),
            3 => ("Base URL", 230.0, 230.0, 500.0, false),
            4 => ("Model", 230.0, 330.0, 500.0, false),
            5 => ("ReAct Steps", 230.0, 430.0, 150.0, false),
            6 => ("L1 Summary", 405.0, 430.0, 150.0, false),
            7 => ("L2 Merge", 580.0, 430.0, 150.0, false),
            8 => ("Interact Interval (min)", 230.0, 530.0, 150.0, false),
            9 => ("Tavily Key", 230.0, 630.0, 500.0, false),
            10 => ("Brave Key", 230.0, 730.0, 500.0, false),
            11 => ("Firecrawl URL", 230.0, 830.0, 500.0, false),
            12 => ("Firecrawl Key", 230.0, 930.0, 500.0, false),
            13 => ("System Prompt", 230.0, 1030.0, 500.0, true),
            17 => ("TTS Prompt Text", 230.0, 1530.0, 500.0, false),
            _ => ("", 0.0, 0.0, 0.0, false),
        };

        if i != 1 {
            let fy_scaled_raw = card_y_raw + sc(fy) as i32;
            let input_y_raw = fy_scaled_raw + sc(25.0) as i32;
            let input_h = if is_multiline {
                sc(250.0) as u32
            } else {
                sc(45.0) as u32
            };
            let box_bottom_raw = input_y_raw + input_h as i32;

            let val: &str = match i {
                0 => &active_profile.name,
                2 => &active_profile.api_key,
                3 => &active_profile.base_url,
                4 => &active_profile.model,
                5 => {
                    temp_num = ai_config.react_limit.to_string();
                    &temp_num
                }
                6 => {
                    temp_num = ai_config.l1_summary_threshold.to_string();
                    &temp_num
                }
                7 => {
                    temp_num = ai_config.l2_merge_threshold.to_string();
                    &temp_num
                }
                8 => {
                    temp_num = ai_config.interaction_frequency.to_string();
                    &temp_num
                }
                9 => &ai_config.tavily_api_key,
                10 => &ai_config.brave_api_key,
                11 => &ai_config.firecrawl_url,
                12 => &ai_config.firecrawl_api_key,
                13 => &ai_config.system_prompt,
                17 => &ai_config.tts_prompt_text,
                _ => "",
            };

            let val_chars: Vec<char> = val.chars().collect();
            let is_masked =
                (i == 2 || i == 9 || i == 10 || i == 12) && !val.is_empty() && !state.show_api_key;
            let display_chars: Vec<char> = if is_masked {
                std::iter::repeat('•')
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

            // Selection and Cursor logic remains similar but with new field indices
            if let Some(sel_start_idx) = state.selection_start {
                if i == 13 {
                    let rects = get_selection_rects(
                        &ai_config.system_prompt,
                        sc(14.0),
                        sc(500.0 - 40.0) as u32,
                        sel_start_idx,
                        state.cursor_pos,
                    );
                    for (rx, ry, rw, rh) in rects {
                        let draw_y_f =
                            text_start_y as f32 + ry + (state.system_prompt_scroll_offset * scale);
                        if draw_y_f >= (text_start_y as f32 - rh)
                            && draw_y_f <= (box_bottom_raw as f32)
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
                        let (lx, _, _) =
                            get_xy_from_cursor_index(&final_text, sc(14.0), 1000000, min_idx);
                        let (rx, _, _) =
                            get_xy_from_cursor_index(&final_text, sc(14.0), 1000000, max_idx);

                        let scroll_px = state.field_scroll_offsets[i];
                        let draw_lx = (text_start_x as f32 + lx + scroll_px) as i32;
                        let draw_rx = (text_start_x as f32 + rx + scroll_px) as i32;

                        // Clip to box
                        let box_left = text_start_x;
                        let box_right = text_start_x + sc(fw as f32 - 30.0) as i32;

                        let final_lx = draw_lx.max(box_left).min(box_right);
                        let final_rx = draw_rx.max(box_left).min(box_right);

                        if final_rx > final_lx {
                            draw_rect_alpha(
                                buffer,
                                w,
                                final_lx,
                                text_start_y,
                                (final_rx - final_lx) as u32,
                                sc(22.0) as u32,
                                0x00AADDFF,
                                0.4,
                                w,
                                h,
                            );
                        }
                    }
                }
            }

            let (px, py, _ch) = if i == 13 {
                get_xy_from_cursor_index(
                    &ai_config.system_prompt,
                    sc(14.0),
                    sc(500.0 - 40.0) as u32,
                    state.cursor_pos,
                )
            } else {
                get_xy_from_cursor_index(
                    &final_text,
                    sc(14.0),
                    1000000,
                    state.cursor_pos.min(display_chars.len()),
                )
            };

            let scroll_px = if i == 13 {
                state.system_prompt_scroll_offset * scale
            } else {
                state.field_scroll_offsets[i]
            };

            let cursor_x =
                (text_start_x as f32 + px + (if i == 13 { 0.0 } else { scroll_px })) as i32;
            let cursor_y =
                (text_start_y as f32 + py + (if i == 13 { scroll_px } else { 0.0 })) as i32;

            // Clipping for single-line fields
            let is_inside = if i == 13 {
                cursor_y >= input_y_raw && cursor_y <= (box_bottom_raw - sc(20.0) as i32)
            } else {
                let box_left = text_start_x;
                let box_right = text_start_x + sc(fw as f32 - 30.0) as i32;
                cursor_x >= box_left && cursor_x <= box_right
            };

            let cursor_visible =
                (std::time::Instant::now() - state.last_cursor_action).as_millis() % 1000 < 500;

            if is_inside {
                cursor_rect = Some((cursor_x, cursor_y, 2, sc(22.0) as u32));
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
    }

    // DYNAMIC CONTENT OVERLAY (System Prompt)
    {
        let fx = 230.0;
        let fy = 1030.0;
        let fy_scaled_raw = card_y_raw + sc(fy) as i32;
        let input_y_raw = fy_scaled_raw + sc(25.0) as i32;
        let input_h = sc(250.0) as u32;
        let text_start_x = s(fx as u32) as i32 + sc(15.0) as i32;
        let text_start_y = input_y_raw + sc(12.0) as i32;

        draw_text_dw_h(
            buffer,
            w,
            &ai_config.system_prompt,
            state.system_prompt_hash,
            text_start_x,
            text_start_y,
            sc(14.0),
            if ai_config.system_prompt.is_empty() {
                COLOR_TEXT_DIM
            } else {
                COLOR_TEXT_MAIN
            },
            sc(500.0 - 40.0) as u32,
            input_h.saturating_sub(sc(24.0) as u32),
            state.system_prompt_scroll_offset * scale,
            state.field_scroll_offsets[13],
            sc(500.0 - 40.0) as u32,
        );

        let view_h = input_h.saturating_sub(sc(24.0) as u32) as f32;
        let (_, content_h_logical): (f32, f32) =
            get_metrics_dw(&ai_config.system_prompt, sc(14.0), sc(500.0 - 40.0) as u32);
        let full_content_h = content_h_logical + sc(24.0);

        if full_content_h > view_h && view_h > 0.0 {
            let sb_x = text_start_x + sc(500.0 - 40.0) as i32 + sc(4.0) as i32;
            let sb_y = text_start_y;
            let sb_w = sc(4.0) as u32;
            let track_h = view_h as u32;
            draw_rect(buffer, w, sb_x, sb_y, sb_w, track_h, 0x00333333, w, h);
            let ratio = view_h / full_content_h;
            let handle_h = (view_h * ratio).max(sc(20.0)).min(view_h) as u32;
            let max_scroll = (full_content_h - view_h).max(0.0f32);
            let progress = if max_scroll > 0.0 {
                (-state.system_prompt_scroll_offset * scale)
                    .max(0.0)
                    .min(max_scroll)
                    / max_scroll
            } else {
                0.0
            };
            let handle_y = sb_y + ((view_h - handle_h as f32) * progress) as i32;
            draw_rect(buffer, w, sb_x, handle_y, sb_w, handle_h, 0x007C4DFF, w, h);
        }
    }

    // 4. DIALOG OVERLAY (Deletion Confirmation)
    if state.show_delete_dialog {
        // Dim background
        for p in buffer.iter_mut() {
            let r = ((*p >> 16) & 0xFF) / 2;
            let g = ((*p >> 8) & 0xFF) / 2;
            let b = (*p & 0xFF) / 2;
            *p = (r << 16) | (g << 8) | b;
        }

        let dialog_dx = (800.0 - 300.0) / 2.0;
        let dialog_dy = (750.0 - 150.0) / 2.0;
        let dialog_x_abs = s(dialog_dx as u32) as i32;
        let dialog_y_abs = sy_val(dialog_dy as u32) as i32;
        let dialog_w_abs = sc(300.0) as u32;
        let dialog_h_abs = sc(150.0) as u32;

        draw_rounded_rect(
            buffer,
            w,
            dialog_x_abs,
            dialog_y_abs,
            dialog_w_abs,
            dialog_h_abs,
            12,
            COLOR_BG_LIGHT,
            w,
            h,
        );
        draw_text_dw_ex(
            buffer,
            w,
            "Delete Profile?",
            dialog_x_abs + sc(60.0) as i32,
            dialog_y_abs + sc(30.0) as i32,
            sc(18.0),
            COLOR_TEXT_MAIN,
            dialog_w_abs,
            dialog_h_abs,
            0.0,
            0.0,
            dialog_w_abs,
        );

        let btn_w_design = 80.0;
        let btn_h_design = 35.0;
        let btn_w_abs = sc(btn_w_design) as u32;
        let btn_h_abs = sc(btn_h_design) as u32;
        let btn_y_design = dialog_dy + 85.0;
        let btn_y_abs = sy_val(btn_y_design as u32) as i32;

        // NO button
        let no_x_design = dialog_dx + 50.0;
        let no_x_abs = s(no_x_design as u32) as i32;
        let is_no_hover = state.mouse_pos.0 >= no_x_design
            && state.mouse_pos.0 <= no_x_design + btn_w_design
            && state.mouse_pos.1 >= btn_y_design
            && state.mouse_pos.1 <= btn_y_design + btn_h_design;
        draw_rounded_rect(
            buffer,
            w,
            no_x_abs,
            btn_y_abs,
            btn_w_abs,
            btn_h_abs,
            8,
            if is_no_hover {
                COLOR_BORDER
            } else {
                COLOR_BG_SIDEBAR
            },
            w,
            h,
        );
        draw_text_dw_ex(
            buffer,
            w,
            "No",
            no_x_abs + sc(30.0) as i32,
            btn_y_abs + sc(8.0) as i32,
            sc(14.0),
            COLOR_TEXT_MAIN,
            btn_w_abs,
            btn_h_abs,
            0.0,
            0.0,
            btn_w_abs,
        );

        // YES button
        let yes_x_design = dialog_dx + 170.0;
        let yes_x_abs = s(yes_x_design as u32) as i32;
        let is_yes_hover = state.mouse_pos.0 >= yes_x_design
            && state.mouse_pos.0 <= yes_x_design + btn_w_design
            && state.mouse_pos.1 >= btn_y_design
            && state.mouse_pos.1 <= btn_y_design + btn_h_design;

        // Premium red Yes button: Solid red bg, White text. Brighter on hover.
        draw_rounded_rect(
            buffer,
            w,
            yes_x_abs,
            btn_y_abs,
            btn_w_abs,
            btn_h_abs,
            8,
            if is_yes_hover { 0x00FF6666 } else { 0x00FF4444 },
            w,
            h,
        );
        draw_text_dw_ex(
            buffer,
            w,
            "Yes",
            yes_x_abs + sc(28.0) as i32,
            btn_y_abs + sc(8.0) as i32,
            sc(14.0),
            0x00FFFFFF,
            btn_w_abs,
            btn_h_abs,
            0.0,
            0.0,
            btn_w_abs,
        );
    }

    // 5. TOAST NOTIFICATION
    if let Some((msg, start_time)) = &state.notification {
        let elapsed = start_time.elapsed().as_secs_f32();
        if elapsed < 2.0 {
            let toast_w = sc(150.0) as u32;
            let toast_h = sc(40.0) as u32;
            let toast_x = (w - toast_w) / 2;
            let toast_y = (h as f32 * 0.8) as u32;

            let alpha = if elapsed > 1.5 {
                (2.0 - elapsed) / 0.5
            } else {
                1.0
            };
            draw_rect_alpha(
                buffer,
                w,
                toast_x as i32,
                toast_y as i32,
                toast_w,
                toast_h,
                0x00444444,
                0.8 * alpha,
                w,
                h,
            );
            draw_text_dw_ex(
                buffer,
                w,
                msg,
                (toast_x + sc(25.0) as u32) as i32,
                (toast_y + sc(10.0) as u32) as i32,
                sc(14.0),
                COLOR_TEXT_MAIN,
                toast_w,
                toast_h,
                0.0,
                0.0,
                toast_w,
            );
        }
    }

    (scrollable_h_logical, 1650.0, cursor_rect)
}
