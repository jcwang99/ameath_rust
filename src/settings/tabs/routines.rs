use crate::theme::*;
use crate::ui_primitives::*;
use crate::types::{RoutineDef, RoutinesConfig, ScheduleType};

pub struct RoutinesTabState<'a> {
    pub config: &'a RoutinesConfig,
    pub editing_routine: &'a Option<RoutineDef>,
    pub focused_field: Option<usize>,
    pub cursor_pos: usize,
    pub scroll_offset: f32,
    pub mouse_pos: (f32, f32),
    pub pressed_btn: Option<usize>,
    pub memo_scroll_offset: f32,
    pub memo_rect: &'a mut Option<(f64, f64, f64, f64)>,
    pub memo_content_height: &'a mut f32,
    pub selection_start: Option<usize>,
}

pub fn draw(
    buffer: &mut [u32],
    w: u32,
    h: u32,
    scale: f32,
    off_x: f32,
    off_y: f32,
    state: &mut RoutinesTabState,
) -> (f32, f32, Option<(i32, i32, u32, u32)>) {
    let mut cursor_rect = None;
    let s = |val: f32| -> u32 { (val * scale + off_x) as u32 };
    let sy_val = |val: f32| -> u32 { (val * scale + off_y) as u32 };
    let sc = |val: f32| -> f32 { val * scale };

    let mut current_y = 120.0 + state.scroll_offset;
    
    if let Some(ref editing) = state.editing_routine {
        // Draw Editor
        let card_w = 560.0;
        let card_h = 680.0;
        draw_rounded_rect(buffer, w, s(210.0) as i32, sy_val(current_y) as i32, sc(card_w) as u32, sc(card_h) as u32, 12, COLOR_BG_CARD, w, h);
        
        draw_text(buffer, w, &[], "Edit Routine", s(230.0) as i32, sy_val(current_y + 20.0) as i32, sc(20.0), COLOR_TEXT_MAIN);
        
        current_y += 60.0;
        
        // 1. Title Input (Field 501)
        draw_text(buffer, w, &[], "Title:", s(230.0) as i32, sy_val(current_y + 10.0) as i32, sc(16.0), COLOR_TEXT_SEC);
        let input_title_y = sy_val(current_y + 35.0) as i32;
        let is_focused = state.focused_field == Some(501);
        draw_rounded_rect(buffer, w, s(230.0) as i32, input_title_y, sc(500.0) as u32, sc(40.0) as u32, 8, if is_focused { COLOR_PRIMARY } else { COLOR_BORDER }, w, h);
        draw_rounded_rect(buffer, w, s(230.0) as i32 + 1, input_title_y + 1, sc(500.0) as u32 - 2, sc(40.0) as u32 - 2, 7, COLOR_BG_LIGHT, w, h);
        
        draw_text(buffer, w, &[], &editing.title, s(240.0) as i32, input_title_y + sc(10.0) as i32, sc(16.0), COLOR_TEXT_MAIN);
        if is_focused {
            let (px, _py, _ch) = crate::ui_primitives::get_xy_from_cursor_index(
                &editing.title, sc(16.0), sc(480.0) as u32, state.cursor_pos,
            );
            cursor_rect = Some((s(240.0) as i32 + px as i32, input_title_y + sc(8.0) as i32, sc(2.0) as u32, sc(20.0) as u32));
        }
        
        current_y += 90.0;
        
        // 2. Schedule Type selection
        draw_text(buffer, w, &[], "Schedule Type:", s(230.0) as i32, sy_val(current_y + 10.0) as i32, sc(16.0), COLOR_TEXT_SEC);
        current_y += 35.0;
        let types = [
            (ScheduleType::Daily, "Daily"),
            (ScheduleType::Weekly, "Weekly"),
            (ScheduleType::Monthly, "Monthly"),
            (ScheduleType::IntervalDays, "Days"),
            (ScheduleType::IntervalHours, "Hours"),
            (ScheduleType::IntervalMinutes, "Mins"),
        ];
        
        for (i, (t, label)) in types.iter().enumerate() {
            let row = i / 3;
            let col = i % 3;
            let btn_x = s(230.0 + col as f32 * 110.0) as i32;
            let btn_y = sy_val(current_y + row as f32 * 50.0) as i32;
            let active = editing.schedule_type == *t;
            draw_rounded_rect(buffer, w, btn_x, btn_y, sc(100.0) as u32, sc(40.0) as u32, 8, if active { COLOR_PRIMARY } else { COLOR_BORDER }, w, h);
            draw_rounded_rect(buffer, w, btn_x + 1, btn_y + 1, sc(100.0) as u32 - 2, sc(40.0) as u32 - 2, 7, if active { COLOR_PRIMARY } else { COLOR_BG_LIGHT }, w, h);
            draw_text(buffer, w, &[], label, btn_x + sc(15.0) as i32, btn_y + sc(10.0) as i32, sc(14.0), if active { 0x00FFFFFF } else { COLOR_TEXT_MAIN });
        }
        current_y += 110.0;
        
        // 3. Dynamic Value Input (Field 502) - Used for Interval / Day of Month / Day of Week
        let val_label = match editing.schedule_type {
            ScheduleType::Daily => "N/A",
            ScheduleType::Weekly => "Day of Week (0=Mon, 6=Sun):",
            ScheduleType::Monthly => "Day of Month (1-31):",
            ScheduleType::IntervalDays | ScheduleType::IntervalHours | ScheduleType::IntervalMinutes => "Interval Amount:",
        };
        
        if editing.schedule_type != ScheduleType::Daily {
            draw_text(buffer, w, &[], val_label, s(230.0) as i32, sy_val(current_y + 10.0) as i32, sc(16.0), COLOR_TEXT_SEC);
            let input_val_y = sy_val(current_y + 35.0) as i32;
            let is_focused_val = state.focused_field == Some(502);
            draw_rounded_rect(buffer, w, s(230.0) as i32, input_val_y, sc(200.0) as u32, sc(40.0) as u32, 8, if is_focused_val { COLOR_PRIMARY } else { COLOR_BORDER }, w, h);
            draw_rounded_rect(buffer, w, s(230.0) as i32 + 1, input_val_y + 1, sc(200.0) as u32 - 2, sc(40.0) as u32 - 2, 7, COLOR_BG_LIGHT, w, h);
            
            let display_val = match editing.schedule_type {
                ScheduleType::Weekly => editing.day_of_week.unwrap_or(0).to_string(),
                ScheduleType::Monthly => editing.day_of_month.unwrap_or(1).to_string(),
                _ => editing.interval.unwrap_or(1).to_string(),
            };
            
            draw_text(buffer, w, &[], &display_val, s(240.0) as i32, input_val_y + sc(10.0) as i32, sc(16.0), COLOR_TEXT_MAIN);
            if is_focused_val {
                let (px, _py, _ch) = crate::ui_primitives::get_xy_from_cursor_index(
                    &display_val, sc(16.0), sc(180.0) as u32, state.cursor_pos,
                );
                cursor_rect = Some((s(240.0) as i32 + px as i32, input_val_y + sc(8.0) as i32, sc(2.0) as u32, sc(20.0) as u32));
            }
        }
        
        // 4. Time Input (Field 503) - Used for Daily, Weekly, Monthly
        if matches!(editing.schedule_type, ScheduleType::Daily | ScheduleType::Weekly | ScheduleType::Monthly) {
            draw_text(buffer, w, &[], "Time of Day (HH:MM):", s(450.0) as i32, sy_val(current_y + 10.0) as i32, sc(16.0), COLOR_TEXT_SEC);
            let input_time_y = sy_val(current_y + 35.0) as i32;
            let is_focused_time = state.focused_field == Some(503);
            draw_rounded_rect(buffer, w, s(450.0) as i32, input_time_y, sc(200.0) as u32, sc(40.0) as u32, 8, if is_focused_time { COLOR_PRIMARY } else { COLOR_BORDER }, w, h);
            draw_rounded_rect(buffer, w, s(450.0) as i32 + 1, input_time_y + 1, sc(200.0) as u32 - 2, sc(40.0) as u32 - 2, 7, COLOR_BG_LIGHT, w, h);
            
            let display_time = editing.time_of_day.as_deref().unwrap_or("00:00");
            draw_text(buffer, w, &[], display_time, s(460.0) as i32, input_time_y + sc(10.0) as i32, sc(16.0), COLOR_TEXT_MAIN);
            if is_focused_time {
                let (px, _py, _ch) = crate::ui_primitives::get_xy_from_cursor_index(
                    display_time, sc(16.0), sc(180.0) as u32, state.cursor_pos,
                );
                cursor_rect = Some((s(460.0) as i32 + px as i32, input_time_y + sc(8.0) as i32, sc(2.0) as u32, sc(20.0) as u32));
            }
        }
        
        current_y += 90.0;
        
        // 5. Memo Input (Field 504)
        draw_text(buffer, w, &[], "Action Memo:", s(230.0) as i32, sy_val(current_y + 10.0) as i32, sc(16.0), COLOR_TEXT_SEC);
        let input_memo_y = sy_val(current_y + 35.0) as i32;
        let is_focused_memo = state.focused_field == Some(504);
        draw_rounded_rect(buffer, w, s(230.0) as i32, input_memo_y, sc(500.0) as u32, sc(100.0) as u32, 8, if is_focused_memo { COLOR_PRIMARY } else { COLOR_BORDER }, w, h);
        draw_rounded_rect(buffer, w, s(230.0) as i32 + 1, input_memo_y + 1, sc(500.0) as u32 - 2, sc(100.0) as u32 - 2, 7, COLOR_BG_LIGHT, w, h);
        
        // Multiline rendering for memo
        #[cfg(target_os = "windows")]
        {
            let max_w = sc(480.0) as u32;
            let (w_actual, h_actual) = crate::ui_primitives::get_metrics_dw(&editing.memo, sc(14.0), max_w);
            *state.memo_content_height = h_actual;
            *state.memo_rect = Some((
                s(240.0) as f64,
                (input_memo_y as f32 + sc(10.0)) as f64,
                s(240.0) as f64 + max_w as f64,
                (input_memo_y as f32 + sc(10.0) + sc(80.0)) as f64, // The visible rect
            ));
            
            let view_h = sc(80.0);
            crate::ui_primitives::draw_text_dw_ex(
                buffer, w, &editing.memo,
                s(240.0) as i32, input_memo_y + sc(10.0) as i32,
                sc(14.0), COLOR_TEXT_MAIN,
                max_w, view_h as u32, -state.memo_scroll_offset * scale, 0.0, max_w
            );

            // Scrollbar
            let full_content_h = h_actual + sc(20.0);
            if full_content_h > view_h && view_h > 0.0 {
                let sb_x = s(240.0) as i32 + max_w as i32 + sc(4.0) as i32;
                let sb_y = input_memo_y + sc(10.0) as i32;
                let sb_w = sc(4.0) as u32;
                crate::ui_primitives::draw_rect(buffer, w, sb_x, sb_y, sb_w, view_h as u32, 0x00333333, w, h);
                let ratio = view_h / full_content_h;
                let handle_h = (view_h * ratio).max(sc(20.0)).min(view_h) as u32;
                let max_scroll = (full_content_h - view_h).max(0.0f32);
                let progress = if max_scroll > 0.0 {
                    (-state.memo_scroll_offset * scale)
                        .max(0.0)
                        .min(max_scroll)
                        / max_scroll
                } else {
                    0.0
                };
                let handle_y = sb_y + ((view_h - handle_h as f32) * progress) as i32;
                crate::ui_primitives::draw_rect(buffer, w, sb_x, handle_y, sb_w, handle_h, 0x007C4DFF, w, h);
            }
        }
        #[cfg(not(target_os = "windows"))]
        draw_text(buffer, w, &[], &editing.memo, s(240.0) as i32, input_memo_y + sc(10.0) as i32, sc(14.0), COLOR_TEXT_MAIN);
        
        if is_focused_memo {
            let memo_max_w = sc(480.0) as u32;
            let memo_view_h = sc(80.0);
            
            if let Some(sel_start_idx) = state.selection_start {
                let rects = crate::ui_primitives::get_selection_rects(
                    &editing.memo,
                    sc(14.0),
                    memo_max_w,
                    sel_start_idx,
                    state.cursor_pos,
                );
                for (rx, ry, rw, rh) in rects {
                    let draw_y_f = input_memo_y as f32 + sc(10.0) + ry + (state.memo_scroll_offset * scale);
                    let box_bottom = input_memo_y as f32 + sc(10.0) + memo_view_h;
                    if draw_y_f >= (input_memo_y as f32 + sc(10.0) - rh) && draw_y_f <= box_bottom {
                        crate::ui_primitives::draw_rect_alpha(
                            buffer, w,
                            s(240.0) as i32 + rx as i32,
                            draw_y_f as i32,
                            rw as u32,
                            rh as u32,
                            0x00AADDFF,
                            0.4,
                            w, h,
                        );
                    }
                }
            }

            let (px, py, ch) = crate::ui_primitives::get_xy_from_cursor_index(
                &editing.memo,
                sc(14.0),
                memo_max_w,
                state.cursor_pos,
            );
            let draw_y_f = input_memo_y as f32 + sc(10.0) + py as f32 + (state.memo_scroll_offset * scale);
            let draw_y_bottom = draw_y_f + ch as f32;
            let box_bottom = input_memo_y as f32 + sc(10.0) + memo_view_h;
            
            if draw_y_f >= (input_memo_y as f32 + sc(10.0) - ch as f32) && draw_y_f <= box_bottom {
                cursor_rect = Some((
                    s(240.0) as i32 + px as i32,
                    draw_y_f as i32,
                    sc(2.0) as u32,
                    sc(20.0) as u32,
                ));
            }
        }
        
        current_y += 150.0;
        
        // 6. Expiry Mode
        draw_text(buffer, w, &[], "Expiry Mode:", s(230.0) as i32, sy_val(current_y + 10.0) as i32, sc(16.0), COLOR_TEXT_SEC);
        let is_always_run = editing.expiry_minutes.is_none();
        
        // Always Run button
        let ar_btn_x = s(230.0) as i32;
        let ar_btn_y = sy_val(current_y + 35.0) as i32;
        draw_rounded_rect(buffer, w, ar_btn_x, ar_btn_y, sc(140.0) as u32, sc(40.0) as u32, 8, if is_always_run { COLOR_PRIMARY } else { COLOR_BORDER }, w, h);
        draw_rounded_rect(buffer, w, ar_btn_x + 1, ar_btn_y + 1, sc(140.0) as u32 - 2, sc(40.0) as u32 - 2, 7, if is_always_run { COLOR_PRIMARY } else { COLOR_BG_LIGHT }, w, h);
        draw_text(buffer, w, &[], "Always Run", ar_btn_x + sc(20.0) as i32, ar_btn_y + sc(10.0) as i32, sc(14.0), if is_always_run { 0x00FFFFFF } else { COLOR_TEXT_MAIN });
        
        // Expire After button
        let ea_btn_x = s(380.0) as i32;
        let ea_btn_y = sy_val(current_y + 35.0) as i32;
        draw_rounded_rect(buffer, w, ea_btn_x, ea_btn_y, sc(140.0) as u32, sc(40.0) as u32, 8, if !is_always_run { COLOR_PRIMARY } else { COLOR_BORDER }, w, h);
        draw_rounded_rect(buffer, w, ea_btn_x + 1, ea_btn_y + 1, sc(140.0) as u32 - 2, sc(40.0) as u32 - 2, 7, if !is_always_run { COLOR_PRIMARY } else { COLOR_BG_LIGHT }, w, h);
        draw_text(buffer, w, &[], "Expire After", ea_btn_x + sc(15.0) as i32, ea_btn_y + sc(10.0) as i32, sc(14.0), if !is_always_run { 0x00FFFFFF } else { COLOR_TEXT_MAIN });
        
        // Minutes input (Field 505) - only when Expire After is selected
        if !is_always_run {
            let min_x = s(540.0) as i32;
            let min_y = sy_val(current_y + 35.0) as i32;
            let is_focused_min = state.focused_field == Some(505);
            draw_rounded_rect(buffer, w, min_x, min_y, sc(120.0) as u32, sc(40.0) as u32, 8, if is_focused_min { COLOR_PRIMARY } else { COLOR_BORDER }, w, h);
            draw_rounded_rect(buffer, w, min_x + 1, min_y + 1, sc(120.0) as u32 - 2, sc(40.0) as u32 - 2, 7, COLOR_BG_LIGHT, w, h);
            
            let display_min = editing.expiry_minutes.unwrap_or(60).to_string();
            draw_text(buffer, w, &[], &display_min, min_x + sc(10.0) as i32, min_y + sc(10.0) as i32, sc(16.0), COLOR_TEXT_MAIN);
            draw_text(buffer, w, &[], "min", min_x + sc(85.0) as i32, min_y + sc(12.0) as i32, sc(12.0), COLOR_TEXT_SEC);
            if is_focused_min {
                let (px, _py, _ch) = crate::ui_primitives::get_xy_from_cursor_index(
                    &display_min, sc(16.0), sc(70.0) as u32, state.cursor_pos,
                );
                cursor_rect = Some((min_x + sc(10.0) as i32 + px as i32, min_y + sc(8.0) as i32, sc(2.0) as u32, sc(20.0) as u32));
            }
        }
        
        current_y += 90.0;
        
        // Actions
        // Cancel Button
        draw_rounded_rect(buffer, w, s(480.0) as i32, sy_val(current_y) as i32, sc(100.0) as u32, sc(40.0) as u32, 8, COLOR_BORDER, w, h);
        draw_text(buffer, w, &[], "Cancel", s(500.0) as i32, sy_val(current_y + 10.0) as i32, sc(16.0), COLOR_TEXT_MAIN);
        
        // Save Button
        draw_rounded_rect(buffer, w, s(600.0) as i32, sy_val(current_y) as i32, sc(130.0) as u32, sc(40.0) as u32, 8, COLOR_PRIMARY, w, h);
        draw_text(buffer, w, &[], "Save", s(645.0) as i32, sy_val(current_y + 10.0) as i32, sc(16.0), 0x00FFFFFF);
        
        return (600.0, current_y - state.scroll_offset + 100.0, cursor_rect);
    }
    
    // List View
    draw_rounded_rect(buffer, w, s(210.0) as i32, sy_val(current_y) as i32, sc(560.0) as u32, sc(50.0) as u32, 8, COLOR_PRIMARY, w, h);
    draw_text(buffer, w, &[], "+ Add New Routine", s(410.0) as i32, sy_val(current_y + 15.0) as i32, sc(18.0), 0x00FFFFFF);
    
    current_y += 70.0;
    
    for (i, routine) in state.config.routines.iter().enumerate() {
        let card_w = 560.0;
        let card_h = 100.0;
        let active_color = if routine.is_active { COLOR_PRIMARY } else { COLOR_TEXT_SEC };
        draw_rounded_rect(buffer, w, s(210.0) as i32, sy_val(current_y) as i32, sc(card_w) as u32, sc(card_h) as u32, 12, COLOR_BG_CARD, w, h);
        
        draw_text(buffer, w, &[], &routine.title, s(230.0) as i32, sy_val(current_y + 20.0) as i32, sc(20.0), COLOR_TEXT_MAIN);
        
        let type_str = match routine.schedule_type {
            ScheduleType::Daily => format!("Daily at {}", routine.time_of_day.as_deref().unwrap_or("00:00")),
            ScheduleType::Weekly => format!("Weekly on Day {} at {}", routine.day_of_week.unwrap_or(0), routine.time_of_day.as_deref().unwrap_or("00:00")),
            ScheduleType::Monthly => format!("Monthly on {}th at {}", routine.day_of_month.unwrap_or(1), routine.time_of_day.as_deref().unwrap_or("00:00")),
            ScheduleType::IntervalDays => format!("Every {} Days", routine.interval.unwrap_or(1)),
            ScheduleType::IntervalHours => format!("Every {} Hours", routine.interval.unwrap_or(1)),
            ScheduleType::IntervalMinutes => format!("Every {} Mins", routine.interval.unwrap_or(1)),
        };
        draw_text(buffer, w, &[], &type_str, s(230.0) as i32, sy_val(current_y + 55.0) as i32, sc(14.0), active_color);
        
        // Expiry info
        let expiry_str = match routine.expiry_minutes {
            Some(m) => format!("Expire: {}min", m),
            None => "Always Run".to_string(),
        };
        draw_text(buffer, w, &[], &expiry_str, s(230.0) as i32, sy_val(current_y + 75.0) as i32, sc(12.0), COLOR_TEXT_SEC);
        
        // Active Toggle
        let toggle_txt = if routine.is_active { "[ON]" } else { "[OFF]" };
        draw_text(buffer, w, &[], toggle_txt, s(550.0) as i32, sy_val(current_y + 40.0) as i32, sc(16.0), active_color);
        
        // Edit / Delete buttons
        draw_text(buffer, w, &[], "Edit", s(630.0) as i32, sy_val(current_y + 40.0) as i32, sc(16.0), COLOR_TEXT_MAIN);
        draw_text(buffer, w, &[], "Delete", s(690.0) as i32, sy_val(current_y + 40.0) as i32, sc(16.0), 0x00FF4444);
        
        current_y += 120.0;
    }
    
    (600.0, current_y - state.scroll_offset + 40.0, cursor_rect)
}
