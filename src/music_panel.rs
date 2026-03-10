use crate::music_player::MusicPlayer;
use crate::ui_primitives;

pub const BASE_PANEL_WIDTH: i32 = 220;
pub const BASE_PANEL_HEIGHT: i32 = 100;
pub const BASE_LIST_ITEM_HEIGHT: i32 = 22;
pub const MAX_VISIBLE_ITEMS: usize = 8;

pub fn get_max_scroll_offset(songs_len: usize) -> f32 {
    let content_h = songs_len as f32 * BASE_LIST_ITEM_HEIGHT as f32;
    let visible_h = MAX_VISIBLE_ITEMS as f32 * BASE_LIST_ITEM_HEIGHT as f32;
    (content_h - visible_h).max(0.0)
}

#[derive(Debug, Clone, Copy)]
pub enum MusicPanelAction {
    PlayPause,
    Prev,
    Next,
    Seek(f32),
    ToggleList,
    SelectSong(usize),
}

pub fn render_music_panel(
    player: &MusicPlayer,
    buffer: &mut [u32],
    win_w: u32,
    win_h: u32,
    panel_x: i32,
    panel_y: i32,
    scale: f32,
    opacity: f32,
) {
    if !player.panel_enabled || opacity <= 0.0 {
        return;
    }

    let w = (BASE_PANEL_WIDTH as f32 * scale) as u32;
    let mut h = (BASE_PANEL_HEIGHT as f32 * scale) as u32;
    
    let songs = player.songs();
    let max_visible_items = 8;
    let visible_items = songs.len().min(max_visible_items);
    
    if player.list_visible && !songs.is_empty() {
        let list_h = (visible_items as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale) as u32;
        h += list_h; // 精简高度，移除多余的 5.0 * scale
    }

    // 1. Background
    let bg_color = ui_primitives::apply_opacity(0x1F1F1F, opacity);
    ui_primitives::draw_rounded_rect(
        buffer,
        win_w,
        panel_x,
        panel_y,
        w,
        h,
        (10.0 * scale) as u32,
        bg_color,
        win_w,
        win_h,
    );

    // 2. Cover Art (Rotating Disc) - APlayer Style
    let cover_size = (45.0 * scale) as u32;
    let cover_x = panel_x + (12.0 * scale) as i32;
    let cover_y = panel_y + (12.0 * scale) as i32;
    
    // Draw base disc (dark circle)
    ui_primitives::draw_rounded_rect(
        buffer,
        win_w,
        cover_x,
        cover_y,
        cover_size,
        cover_size,
        cover_size / 2,
        ui_primitives::apply_opacity(0x111111, opacity),
        win_w,
        win_h,
    );

    // Draw rotating "needle" or "reflection" to simulate spinning
    if player.is_playing() {
        use std::time::SystemTime;
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() as f64;
        let angle = (now / 20.0) % 360.0;
        let rad = angle.to_radians();
        let cx = (cover_x as f64 + cover_size as f64 / 2.0) as i32;
        let cy = (cover_y as f64 + cover_size as f64 / 2.0) as i32;
        let r = (cover_size as f64 / 2.0 - 5.0 * scale as f64) as f64;
        
        let tx = cx + (r * rad.cos()) as i32;
        let ty = cy + (r * rad.sin()) as i32;
        
        // Just a small highlight dot that rotates
        ui_primitives::draw_rounded_rect(
            buffer,
            win_w,
            tx - (2.0 * scale) as i32,
            ty - (2.0 * scale) as i32,
            (4.0 * scale) as u32,
            (4.0 * scale) as u32,
            (2.0 * scale) as u32,
            ui_primitives::apply_opacity(0xFB7299, opacity),
            win_w,
            win_h,
        );
    }
    
    // Center point of the disc
    ui_primitives::draw_rounded_rect(
        buffer,
        win_w,
        (cover_x as f64 + cover_size as f64 / 2.0 - 4.0 * scale as f64) as i32,
        (cover_y as f64 + cover_size as f64 / 2.0 - 4.0 * scale as f64) as i32,
        (8.0 * scale) as u32,
        (8.0 * scale) as u32,
        (4.0 * scale) as u32,
        ui_primitives::apply_opacity(0x333333, opacity),
        win_w,
        win_h,
    );

    // 3. Title (Next to cover) with Marquee effect
    let text_color = ui_primitives::apply_opacity(0xFFFFFF, opacity);
    let name = player.current_song_name().unwrap_or_else(|| "No Music".to_string());
    let title_x = cover_x + cover_size as i32 + (10.0 * scale) as i32;
    let title_max_w = (w as i32 - (title_x - panel_x) - (35.0 * scale) as i32).max(10) as u32;
    
    let font_size = 13.0 * scale;
    let text_w = ui_primitives::get_text_width(&name, font_size, false);
    
    let mut scroll_x = 0.0;
    if text_w > title_max_w as f32 {
        use std::time::SystemTime;
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() as f64;
        let speed = 40.0; 
        let text_overflow = text_w - title_max_w as f32;
        let pause_duration = 1.5; // seconds
        let move_duration = text_overflow / speed;
        let total_half_cycle = pause_duration + move_duration;
        let total_cycle = total_half_cycle * 2.0;
        
        let local_time = (now / 1000.0) % total_cycle as f64;
        
        scroll_x = if local_time < pause_duration as f64 {
            0.0
        } else if local_time < total_half_cycle as f64 {
            let t = (local_time - pause_duration as f64) as f32;
            -(t * speed)
        } else if local_time < (total_half_cycle + pause_duration) as f64 {
            -text_overflow
        } else {
            let t = (local_time - (total_half_cycle + pause_duration) as f64) as f32;
            -(text_overflow - t * speed)
        };
        
        scroll_x = scroll_x.min(0.0).max(-text_overflow);
    }

    ui_primitives::draw_text_dw_ex_nowrap(
        buffer,
        win_w,
        &name,
        title_x,
        panel_y + (15.0 * scale) as i32,
        font_size,
        text_color,
        title_max_w,
        25,
        0.0,
        scroll_x,
        2000,
    );

    // List toggle button (≡)
    let list_btn_text = "≡";
    ui_primitives::draw_text_dw_ex(
        buffer,
        win_w,
        list_btn_text,
        panel_x + w as i32 - (28.0 * scale) as i32,
        panel_y + (8.0 * scale) as i32,
        18.0 * scale,
        if player.list_visible { ui_primitives::apply_opacity(0xFB7299, opacity) } else { ui_primitives::apply_opacity(0x888888, opacity) },
        25,
        25,
        0.0,
        0.0,
        25,
    );

    // 4. Controls (Shifted right to make room for cover)
    let ctrl_y = panel_y + (45.0 * scale) as i32;
    let btn_gap = (35.0 * scale) as i32;
    let ctrl_start_x = title_x + (5.0 * scale) as i32;

    // Prev
    ui_primitives::draw_text_dw_ex(
        buffer,
        win_w,
        "⏮",
        ctrl_start_x,
        ctrl_y,
        16.0 * scale,
        ui_primitives::apply_opacity(0xCCCCCC, opacity),
        25,
        25,
        0.0,
        0.0,
        25,
    );
    // Play/Pause
    let pp_icon = if player.is_playing() { "⏸" } else { "▶" };
    ui_primitives::draw_text_dw_ex(
        buffer,
        win_w,
        pp_icon,
        ctrl_start_x + btn_gap,
        ctrl_y,
        16.0 * scale,
        ui_primitives::apply_opacity(0xFFFFFF, opacity),
        25,
        25,
        0.0,
        0.0,
        25,
    );
    // Next
    ui_primitives::draw_text_dw_ex(
        buffer,
        win_w,
        "⏭",
        ctrl_start_x + btn_gap * 2,
        ctrl_y,
        16.0 * scale,
        ui_primitives::apply_opacity(0xCCCCCC, opacity),
        25,
        25,
        0.0,
        0.0,
        25,
    );

    // 5. Progress Bar
    let prog_y = panel_y + (80.0 * scale) as i32;
    let prog_x = panel_x + (15.0 * scale) as i32;
    let prog_w = w - (30.0 * scale) as u32;
    let (progress, current_dur, total_dur) = player.get_progress();

    // Background line
    ui_primitives::draw_rect(
        buffer,
        win_w,
        prog_x,
        prog_y + (3.0 * scale) as i32,
        prog_w,
        (3.0 * scale) as u32,
        ui_primitives::apply_opacity(0x333333, opacity),
        win_w,
        win_h,
    );
    // Active line
    ui_primitives::draw_rect(
        buffer,
        win_w,
        prog_x,
        prog_y + (3.0 * scale) as i32,
        (prog_w as f32 * progress) as u32,
        (3.0 * scale) as u32,
        ui_primitives::apply_opacity(0xFB7299, opacity),
        win_w,
        win_h,
    );

    // Time text (Mini style: 01:23 / 03:45 at bottom right)
    let time_text = format!("{:02}:{:02}/{:02}:{:02}", 
        current_dur.as_secs() / 60, current_dur.as_secs() % 60,
        total_dur.as_secs() / 60, total_dur.as_secs() % 60);
    ui_primitives::draw_text_dw_ex(
        buffer,
        win_w,
        &time_text,
        panel_x + w as i32 - (75.0 * scale) as i32,
        prog_y + (10.0 * scale) as i32, // Moved below progress bar
        9.0 * scale,
        ui_primitives::apply_opacity(0x888888, opacity),
        70,
        15,
        0.0,
        0.0,
        70,
    );

    // 5. Playlist
    if player.list_visible && !songs.is_empty() {
        let max_visible_items = MAX_VISIBLE_ITEMS;
        let list_y = panel_y + (BASE_PANEL_HEIGHT as f32 * scale) as i32;
        let item_h = (BASE_LIST_ITEM_HEIGHT as f32 * scale) as i32;
        
        // Ensure scroll offset is clamped (safety)
        let max_offset = get_max_scroll_offset(songs.len());
        let current_offset = player.list_scroll_offset.clamp(0.0, max_offset);
        
        let start_idx = (current_offset / BASE_LIST_ITEM_HEIGHT as f32).floor() as usize;
        let end_idx = (start_idx + max_visible_items + 1).min(songs.len());

        for i in start_idx..end_idx {
            // High-precision Y calculation to prevent rounding errors causing mis-clipping
            let ry_f = (i as f32 * BASE_LIST_ITEM_HEIGHT as f32 - player.list_scroll_offset) * scale;
            let item_y = list_y + ry_f as i32;
            
            // Clip items outside of the list area with 2px buffer
            if item_y < list_y - 2 || item_y >= list_y + (visible_items as i32 * item_h) - 2 {
                continue;
            }

            let is_current = i == player.current_idx();
            
            if is_current {
                ui_primitives::draw_rect(
                    buffer,
                    win_w,
                    panel_x + (5.0 * scale) as i32,
                    item_y,
                    w - (10.0 * scale) as u32,
                    item_h as u32,
                    ui_primitives::apply_opacity(0x333333, opacity),
                    win_w,
                    win_h,
                );
            }

            let song_name = songs[i].file_name().and_then(|f| f.to_str()).unwrap_or("Unknown");
            let display_name = if let Some(dot_idx) = song_name.rfind('.') {
                &song_name[..dot_idx]
            } else {
                song_name
            };
            
            let item_text = format!("{:02}. {}", i + 1, display_name);
            let item_text_w = ui_primitives::get_text_width(&item_text, 11.0 * scale, false);
            let item_max_w = w - (40.0 * scale) as u32;
            
            let mut item_scroll_x = 0.0;
            // Marquee for current playing song if it's too long
            if is_current && item_text_w > item_max_w as f32 {
                use std::time::SystemTime;
                let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis() as f64;
                let speed = 30.0;
                let overflow = item_text_w - item_max_w as f32;
                let cycle = (overflow / speed + 2.0) * 2.0;
                let t = (now / 1000.0) % cycle as f64;
                item_scroll_x = if t < 1.0 { 0.0 } 
                               else if t < 1.0 + (overflow / speed) as f64 { -((t - 1.0) as f32 * speed) }
                               else if t < 2.0 + (overflow / speed) as f64 { -overflow }
                               else { -(overflow - (t - (2.0 + (overflow / speed) as f64)) as f32 * speed) };
                item_scroll_x = item_scroll_x.min(0.0).max(-overflow);
            }

            ui_primitives::draw_text_dw_ex_nowrap(
                buffer,
                win_w,
                &item_text,
                panel_x + (15.0 * scale) as i32,
                item_y + (4.0 * scale) as i32,
                11.0 * scale,
                if is_current { ui_primitives::apply_opacity(0xFB7299, opacity) } else { text_color },
                item_max_w,
                item_h as u32,
                0.0,
                item_scroll_x,
                2000,
            );
        }

        // 6. Scrollbar
        if songs.len() > max_visible_items {
            let bar_w = (4.0 * scale) as u32;
            let bar_x = panel_x + w as i32 - (6.0 * scale) as i32;
            
            let list_area_h = (visible_items as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale) as f32;
            let total_content_h = (songs.len() as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale) as f32;
            
            // 滑块长度比例
            let bar_h = (list_area_h * (list_area_h / total_content_h)).max(15.0 * scale);
            
            // 滑块位置：(当前滚动位 / 总溢出量) * (列表区域 - 滑块长度)
            let max_scroll = (songs.len() - visible_items) as f32 * BASE_LIST_ITEM_HEIGHT as f32;
            let scroll_ratio = if max_scroll > 0.0 { player.list_scroll_offset / max_scroll } else { 0.0 };
            let bar_y = list_y as f32 + scroll_ratio * (list_area_h - bar_h);

            ui_primitives::draw_rect(
                buffer,
                win_w,
                bar_x,
                bar_y as i32,
                bar_w,
                bar_h as u32,
                ui_primitives::apply_opacity(0xFB7299, opacity * 0.7),
                win_w,
                win_h,
            );
        }
    }
}

pub fn check_music_panel_hit(
    player: &MusicPlayer,
    mx: f64,
    my: f64,
    panel_x: i32,
    panel_y: i32,
    scale: f32,
) -> Option<MusicPanelAction> {
    if !player.panel_enabled {
        return None;
    }

    let w = (BASE_PANEL_WIDTH as f32 * scale) as i32;
    let mut h = (BASE_PANEL_HEIGHT as f32 * scale) as i32;
    let songs = player.songs();
    if player.list_visible && !songs.is_empty() {
        let list_h = (songs.len().min(8) as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale) as i32;
        h += list_h + (5.0 * scale) as i32;
    }

    let rx = mx as i32 - panel_x;
    let ry = my as i32 - panel_y;

    if rx < 0 || rx >= w || ry < 0 || ry >= h {
        return None;
    }

    // 1. List Toggle (≡) - Top right
    if rx >= w - (40.0 * scale) as i32 && ry < (35.0 * scale) as i32 {
        return Some(MusicPanelAction::ToggleList);
    }

    // 2. Controls - Right of cover art
    let cover_size = (45.0 * scale) as i32;
    let title_x = (12.0 * scale) as i32 + cover_size + (10.0 * scale) as i32;
    let ctrl_start_x = title_x + (5.0 * scale) as i32;
    let ctrl_y = (45.0 * scale) as i32;
    let btn_gap = (35.0 * scale) as i32;

    if ry >= ctrl_y - (5.0 * scale) as i32 && ry < ctrl_y + (25.0 * scale) as i32 {
        if rx >= ctrl_start_x && rx < ctrl_start_x + (25.0 * scale) as i32 {
            return Some(MusicPanelAction::Prev);
        } else if rx >= ctrl_start_x + btn_gap && rx < ctrl_start_x + btn_gap + (25.0 * scale) as i32 {
            return Some(MusicPanelAction::PlayPause);
        } else if rx >= ctrl_start_x + btn_gap * 2 && rx < ctrl_start_x + btn_gap * 2 + (25.0 * scale) as i32 {
            return Some(MusicPanelAction::Next);
        }
    }

    // 3. Progress Bar Seek - Bottom
    let prog_y = (80.0 * scale) as i32;
    if ry >= prog_y - (10.0 * scale) as i32 && ry < prog_y + (15.0 * scale) as i32 {
        let prog_x = (15.0 * scale) as i32;
        let prog_w = w - (30.0 * scale) as i32;
        if rx >= prog_x && rx < prog_x + prog_w {
            let frac = (rx - prog_x) as f32 / prog_w as f32;
            return Some(MusicPanelAction::Seek(frac));
        }
    }

    // 4. Playlist Selection
    if player.list_visible && ry >= (BASE_PANEL_HEIGHT as f32 * scale) as i32 {
        let list_ry = ry - (BASE_PANEL_HEIGHT as f32 * scale) as i32;
        let item_h_scaled = (BASE_LIST_ITEM_HEIGHT as f32 * scale).max(1.0);
        let clicked_idx = ((player.list_scroll_offset * scale + list_ry as f32) / item_h_scaled).floor() as usize;
        if clicked_idx < songs.len() {
            return Some(MusicPanelAction::SelectSong(clicked_idx));
        }
    }

    None
}
