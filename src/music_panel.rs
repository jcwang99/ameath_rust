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

pub const BASE_BUTTON_SIZE: i32 = 24;

#[derive(Debug, Clone)]
pub struct MusicRenderState {
    pub current_song_name: String,
    pub songs: Vec<String>,
    pub is_playing: bool,
    pub list_visible: bool,
    pub list_scroll_offset: f32,
    pub current_song_idx: usize,
    pub current_duration: std::time::Duration,
    pub total_duration: std::time::Duration,
    pub panel_enabled: bool,
    pub play_mode: crate::music_player::PlayMode,
    pub scale: f32,
    pub opacity: f32,
    pub mx: f64,
    pub my: f64,
    pub timestamp: f64, // For synced animation if needed
}

impl MusicRenderState {
    pub fn new(player: &MusicPlayer, scale: f32, opacity: f32, mx: f64, my: f64) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as f64;
        
        let (_, current_dur, total_dur) = player.get_progress();
        
        Self {
            current_song_name: player.current_song_name().unwrap_or_else(|| "No Music".to_string()),
            songs: player.songs().iter().map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).collect(),
            is_playing: player.is_playing(),
            list_visible: player.list_visible,
            list_scroll_offset: player.list_scroll_offset,
            current_song_idx: player.current_idx(),
            current_duration: current_dur,
            total_duration: total_dur,
            panel_enabled: player.panel_enabled,
            play_mode: player.play_mode,
            scale,
            opacity,
            mx,
            my,
            timestamp: now,
        }
    }
}

pub struct MusicRenderResult {
    pub buffer: Vec<u32>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum MusicPanelAction {
    PlayPause,
    Prev,
    Next,
    Seek(f32),
    ToggleList,
    SelectSong(usize),
    ToggleMode,
}

pub fn render_music_panel(
    state: &MusicRenderState,
    buffer: &mut [u32],
    win_w: u32,
    win_h: u32,
    panel_x: i32,
    panel_y: i32,
) {
    if !state.panel_enabled || state.opacity <= 0.0 {
        return;
    }

    let scale = state.scale;
    let opacity = state.opacity;
    let mx = state.mx;
    let my = state.my;

    let w = (BASE_PANEL_WIDTH as f32 * scale) as u32;
    let mut h = (BASE_PANEL_HEIGHT as f32 * scale) as u32;
    
    let songs = &state.songs;
    let max_visible_items = 8;
    let visible_items = songs.len().min(max_visible_items);
    
    if state.list_visible && !songs.is_empty() {
        let list_h = (visible_items as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale) as u32;
        h += list_h; 
    }

    // 1. Background
    let bg_color = ui_primitives::apply_opacity(0x151515, opacity * 0.85);
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

    // 2. Cover Art (Rotating Disc)
    let cover_size = (45.0 * scale) as u32;
    let cover_x = panel_x + (12.0 * scale) as i32;
    let cover_y = panel_y + (12.0 * scale) as i32;
    
    ui_primitives::draw_rounded_rect(
        buffer,
        win_w,
        cover_x,
        cover_y,
        cover_size,
        cover_size,
        cover_size / 2,
        ui_primitives::apply_opacity(0x0A0A0A, opacity),
        win_w,
        win_h,
    );

    if state.is_playing {
        let now = state.timestamp;
        let angle = (now / 20.0) % 360.0;
        let rad = angle.to_radians();
        let cx = (cover_x as f64 + cover_size as f64 / 2.0) as i32;
        let cy = (cover_y as f64 + cover_size as f64 / 2.0) as i32;
        let r = (cover_size as f64 / 2.0 - 5.0 * scale as f64) as f64;
        
        let tx = cx + (r * rad.cos()) as i32;
        let ty = cy + (r * rad.sin()) as i32;
        
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

    // 3. Title with Marquee effect
    let text_color = ui_primitives::apply_opacity(0xFFFFFF, opacity);
    let name = &state.current_song_name;
    let title_x = cover_x + cover_size as i32 + (10.0 * scale) as i32;
    let title_max_w = (w as i32 - (title_x - panel_x) - (35.0 * scale) as i32).max(10) as u32;
    
    let font_size = 13.0 * scale;
    let text_w = ui_primitives::get_text_width(name, font_size, false);
    
    let mut scroll_x = 0.0;
    if text_w > title_max_w as f32 {
        let now = state.timestamp;
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
        (25.0 * scale) as u32,
        0.0,
        -scroll_x,
        (2000.0 * scale) as u32,
    );

    // List toggle button (≡)
    let list_btn_text = "≡";
    let list_btn_x = panel_x + w as i32 - (28.0 * scale) as i32;
    let list_btn_y = panel_y + (8.0 * scale) as i32;
    let rx_list = mx as i32 - panel_x;
    let ry_list = my as i32 - panel_y;
    let list_hovered = rx_list >= w as i32 - (40.0 * scale) as i32 && ry_list < (35.0 * scale) as i32;
    
    let list_font_scale = if list_hovered { 1.2 } else { 1.0 };
    let list_color = if list_hovered { 
        ui_primitives::apply_opacity(0xFFFFFF, opacity) 
    } else if state.list_visible { 
        ui_primitives::apply_opacity(0xFB7299, opacity) 
    } else { 
        ui_primitives::apply_opacity(0x888888, opacity) 
    };

    ui_primitives::draw_text_dw_ex(
        buffer,
        win_w,
        list_btn_text,
        list_btn_x,
        list_btn_y,
        (18.0 * scale) * list_font_scale,
        list_color,
        (25.0 * scale * list_font_scale) as u32,
        (25.0 * scale * list_font_scale) as u32,
        0.0,
        0.0,
        (25.0 * scale * list_font_scale) as u32,
    );

    // 4. Controls (Shifted right to make room for cover, and added Mode Toggle on left)
    let ctrl_y = panel_y + (45.0 * scale) as i32;
    let btn_gap = (30.0 * scale) as i32; // slightly reduced gap to fit 4 buttons
    let mut ctrl_start_x = title_x + (2.0 * scale) as i32;

    // Check hit for controls (using absolute coords)
    let mx_i = mx as i32;
    let my_i = my as i32;
    
    let in_ctrl_row = my_i >= ctrl_y - (5.0 * scale) as i32 && my_i < ctrl_y + (25.0 * scale) as i32;

    // Mode Toggle
    let mode_hovered = in_ctrl_row && mx_i >= ctrl_start_x && mx_i < ctrl_start_x + (25.0 * scale) as i32;
    let mode_icon = match state.play_mode {
        crate::music_player::PlayMode::Sequential => "⮂", // Rightwards Arrow Over Leftwards Arrow
        crate::music_player::PlayMode::LoopSingle => "↻",  // Clockwise Open Circle Arrow
        crate::music_player::PlayMode::Random => "⤮",       // Rightwards Arrow with Lower Hook
    };
    let mode_font_scale = if mode_hovered { 1.2 } else { 1.0 };
    let mode_color = if mode_hovered { ui_primitives::apply_opacity(0xFFFFFF, opacity) } else { ui_primitives::apply_opacity(0x999999, opacity) };
    ui_primitives::draw_text_dw_ex(
        buffer,
        win_w,
        mode_icon,
        ctrl_start_x,
        ctrl_y, 
        (16.0 * scale) * mode_font_scale,
        mode_color,
        (25.0 * scale * mode_font_scale) as u32,
        (25.0 * scale * mode_font_scale) as u32,
        0.0,
        0.0,
        (25.0 * scale * mode_font_scale) as u32,
    );
    
    ctrl_start_x += btn_gap;

    // Prev
    let prev_hovered = in_ctrl_row && mx_i >= ctrl_start_x && mx_i < ctrl_start_x + (25.0 * scale) as i32;
    let prev_font_scale = if prev_hovered { 1.2 } else { 1.0 };
    let prev_color = if prev_hovered { ui_primitives::apply_opacity(0xFFFFFF, opacity) } else { ui_primitives::apply_opacity(0xCCCCCC, opacity) };
    let prev_w = (14.0 * scale * prev_font_scale) as u32;
    let prev_h = (14.0 * scale * prev_font_scale) as u32;
    // draw two left-pointing triangles
    ui_primitives::draw_triangle(
        buffer, win_w,
        ctrl_start_x + (2.0 * scale) as i32, ctrl_y + (5.0 * scale) as i32,
        prev_w / 2, prev_h,
        prev_color, false,
        win_w, buffer.len() as u32 / win_w ,
    );
    ui_primitives::draw_triangle(
        buffer, win_w,
        ctrl_start_x + (10.0 * scale) as i32, ctrl_y + (5.0 * scale) as i32,
        prev_w / 2, prev_h,
        prev_color, false,
        win_w, buffer.len() as u32 / win_w ,
    );
    
    // Play/Pause
    let pp_hovered = in_ctrl_row && mx_i >= ctrl_start_x + btn_gap && mx_i < ctrl_start_x + btn_gap + (25.0 * scale) as i32;
    let pp_font_scale = if pp_hovered { 1.2 } else { 1.0 };
    let pp_color = if pp_hovered { ui_primitives::apply_opacity(0xFFFFFF, opacity) } else { ui_primitives::apply_opacity(0xEEEEEE, opacity) };
    let pp_w = (16.0 * scale * pp_font_scale) as u32;
    let pp_h = (16.0 * scale * pp_font_scale) as u32;
    if state.is_playing {
        // draw two vertical bars
        ui_primitives::draw_rounded_rect(
            buffer, win_w,
            ctrl_start_x + btn_gap + (2.0 * scale) as i32, ctrl_y + (4.0 * scale) as i32,
            pp_w / 3, pp_h, 
            0, pp_color, win_w, buffer.len() as u32 / win_w 
        );
        ui_primitives::draw_rounded_rect(
            buffer, win_w,
            ctrl_start_x + btn_gap + (10.0 * scale) as i32, ctrl_y + (4.0 * scale) as i32,
            pp_w / 3, pp_h, 
            0, pp_color, win_w, buffer.len() as u32 / win_w 
        );
    } else {
        // draw one right-pointing triangle
        ui_primitives::draw_triangle(
            buffer, win_w,
            ctrl_start_x + btn_gap + (4.0 * scale) as i32, ctrl_y + (4.0 * scale) as i32,
            pp_w, pp_h,
            pp_color, true,
            win_w, buffer.len() as u32 / win_w ,
        );
    }
    
    // Next
    let next_hovered = in_ctrl_row && mx_i >= ctrl_start_x + btn_gap * 2 && mx_i < ctrl_start_x + btn_gap * 2 + (25.0 * scale) as i32;
    let next_font_scale = if next_hovered { 1.2 } else { 1.0 };
    let next_color = if next_hovered { ui_primitives::apply_opacity(0xFFFFFF, opacity) } else { ui_primitives::apply_opacity(0xCCCCCC, opacity) };
    let next_w = (14.0 * scale * next_font_scale) as u32;
    let next_h = (14.0 * scale * next_font_scale) as u32;
    // draw two right-pointing triangles
    ui_primitives::draw_triangle(
        buffer, win_w,
        ctrl_start_x + btn_gap * 2 + (5.0 * scale) as i32, ctrl_y + (5.0 * scale) as i32,
        next_w / 2, next_h,
        next_color, true,
        win_w, buffer.len() as u32 / win_w ,
    );
    ui_primitives::draw_triangle(
        buffer, win_w,
        ctrl_start_x + btn_gap * 2 + (13.0 * scale) as i32, ctrl_y + (5.0 * scale) as i32,
        next_w / 2, next_h,
        next_color, true,
        win_w, buffer.len() as u32 / win_w ,
    );

    // 5. Progress Bar
    let prog_y = panel_y + (75.0 * scale) as i32;
    let prog_x = panel_x + (12.0 * scale) as i32;
    let prog_w = w - (24.0 * scale) as u32;
    let current_dur = state.current_duration;
    let total_dur = state.total_duration;
    let progress = if total_dur.as_secs_f32() > 0.0 {
        current_dur.as_secs_f32() / total_dur.as_secs_f32()
    } else { 0.0 };

    // --- Dynamic Hover & Glow Logic ---
    let mut prog_hovered = false;
    let rel_mx = mx as i32 - prog_x;
    let rel_my = my as i32 - prog_y;
    // Expanded hit area for better touch/mouse feel (-10 to +15 pixels vertically)
    if rel_mx >= -5 && rel_mx <= prog_w as i32 + 5 {
        if rel_my >= (-10.0 * scale) as i32 && rel_my <= (15.0 * scale) as i32 {
            prog_hovered = true;
        }
    }

    let bar_h = if prog_hovered { (4.0 * scale).max(1.0) as u32 } else { (2.0 * scale).max(1.0) as u32 };
    let bar_y_offset = if prog_hovered { (2.0 * scale) as i32 } else { (3.0 * scale) as i32 };
    let active_w = (prog_w as f32 * progress) as u32;
    let bar_radius = bar_h / 2;

    // 5.1 Glow Layer (rendered only when hovered for the neon effect)
    // 5.1 Subtle Rounded Bloom (only when hovered)
    if prog_hovered && active_w > 0 {
        let bloom_margin = (2.0 * scale) as i32;
        ui_primitives::draw_rounded_rect(
            buffer,
            win_w,
            prog_x - bloom_margin,
            prog_y + bar_y_offset - bloom_margin,
            active_w + (bloom_margin * 2) as u32,
            bar_h + (bloom_margin * 2) as u32,
            bar_radius + bloom_margin as u32,
            ui_primitives::apply_opacity(0xFB7299, 0.1 * opacity),
            win_w,
            win_h,
        );
    }

    // 5.2 Background line (Track)
    ui_primitives::draw_rounded_rect(
        buffer,
        win_w,
        prog_x,
        prog_y + bar_y_offset,
        prog_w,
        bar_h,
        bar_radius,
        ui_primitives::apply_opacity(if prog_hovered { 0x4A4A4A } else { 0x333333 }, opacity), // Slightly lighter track on hover
        win_w,
        win_h,
    );

    // 5.3 Active line (Fill)
    if active_w > 0 {
        ui_primitives::draw_rounded_rect(
            buffer,
            win_w,
            prog_x,
            prog_y + bar_y_offset,
            active_w,
            bar_h,
            bar_radius,
            ui_primitives::apply_opacity(0xFB7299, opacity),
            win_w,
            win_h,
        );
    }

    // 5.4 Thumb Indicator (rendered only when hovered, at the tip of the fill)
    if prog_hovered {
        let thumb_r = (4.0 * scale).max(1.0) as u32;
        let thumb_d = thumb_r * 2;
        // Position thumb centered perfectly at the end of active line
        let thumb_x = prog_x + active_w as i32 - thumb_r as i32;
        let thumb_y = prog_y + bar_y_offset + (bar_h as i32 / 2) - thumb_r as i32;
        
        // Draw the white circular head
        ui_primitives::draw_rounded_rect(
            buffer,
            win_w,
            thumb_x,
            thumb_y,
            thumb_d,
            thumb_d,
            thumb_r,
            ui_primitives::apply_opacity(0xFFFFFF, opacity),
            win_w,
            win_h,
        );
    }

    let current_dur: std::time::Duration = state.current_duration;
    let total_dur: std::time::Duration = state.total_duration;
    let time_text = format!("{:02}:{:02} / {:02}:{:02}",
        current_dur.as_secs() / 60, current_dur.as_secs() % 60,
        total_dur.as_secs() / 60, total_dur.as_secs() % 60);
    ui_primitives::draw_text_dw_ex(
        buffer,
        win_w,
        &time_text,
        panel_x + w as i32 - (75.0 * scale) as i32,
        prog_y + (6.0 * scale) as i32, // Moved closer to bar from 10
        9.0 * scale,
        ui_primitives::apply_opacity(0x888888, opacity),
        (70.0 * scale) as u32,
        (15.0 * scale) as u32,
        0.0,
        0.0,
        (70.0 * scale) as u32,
    );

    // 5. Playlist
    let songs = &state.songs;
    if state.list_visible && !songs.is_empty() {
        let max_visible_items = MAX_VISIBLE_ITEMS;
        // BASE_PANEL_HEIGHT (100) is the background height of the control area. 
        // We start list slightly earlier to merge the visual gap.
        let list_y = panel_y + ((BASE_PANEL_HEIGHT as f32 - 4.0) * scale) as i32;
        let item_h = (BASE_LIST_ITEM_HEIGHT as f32 * scale) as i32;
        
        // Ensure scroll offset is clamped (safety)
        let max_offset = get_max_scroll_offset(songs.len());
        let current_offset = state.list_scroll_offset.clamp(0.0, max_offset);
        
        let start_idx = (current_offset / BASE_LIST_ITEM_HEIGHT as f32).floor() as usize;
        let end_idx = (start_idx + max_visible_items + 1).min(songs.len());

        for i in start_idx..end_idx {
            // High-precision Y calculation to prevent rounding errors causing mis-clipping
            let ry_f = (i as f32 * BASE_LIST_ITEM_HEIGHT as f32 - state.list_scroll_offset) * scale;
            let item_y = list_y + ry_f as i32;
            
            // Clip items outside of the list area with small buffer
            let clip_buffer = (2.0 * scale) as i32;
            if item_y < list_y - clip_buffer || item_y >= list_y + (visible_items as i32 * item_h) - clip_buffer {
                continue;
            }

            let is_current = i == state.current_song_idx;
            let mut is_hovered = false;
            
            // Check if mouse is hovering over this list item
            let rel_x = mx as i32 - panel_x;
            let rel_y = my as i32 - item_y;
            if rel_x >= (5.0 * scale) as i32 && rel_x < (w as i32 - (5.0 * scale) as i32) {
                if rel_y >= 0 && rel_y < item_h {
                    is_hovered = true;
                }
            }
            
            // Removed: background draw_rect for is_current/is_hovered
            // We now rely purely on text stylings based on visual cues

            let song_path = std::path::Path::new(&songs[i]);
            let song_name = song_path.file_name().and_then(|f| f.to_str()).unwrap_or("Unknown");
            let display_name = if let Some(dot_idx) = song_name.rfind('.') {
                &song_name[..dot_idx]
            } else {
                song_name
            };
            
            let _item_text = format!("{}. {}", i + 1, display_name);
            
            let _item_max_w = (w as i32 - (20.0 * scale) as i32).max(10) as u32;
            let item_h = (BASE_LIST_ITEM_HEIGHT as f32 * scale) as i32;

            // Highlight bar
            if is_hovered || is_current {
                let bar_color = if is_current { 
                    ui_primitives::apply_opacity(0xFB7299, opacity * 0.2) 
                } else { 
                    ui_primitives::apply_opacity(0x333333, opacity * 0.3) 
                };
                ui_primitives::draw_rounded_rect(
                    buffer, win_w, panel_x + (5.0 * scale) as i32, item_y + (2.0 * scale) as i32,
                    (w as f32 - 10.0 * scale) as u32, (item_h as f32 - 4.0 * scale) as u32,
                    (4.0 * scale) as u32, bar_color, win_w, win_h
                );
            }

            let mut target_font_size = 11.0 * scale;
            let mut target_color = text_color;

            if is_current {
                target_font_size = if is_hovered { 12.0 * scale } else { 11.5 * scale };
                target_color = ui_primitives::apply_opacity(0xFB7299, opacity);
            } else if is_hovered {
                target_font_size = 12.0 * scale;
                target_color = ui_primitives::apply_opacity(0xFFFFFF, opacity * 1.5);
            }

            let index_text = format!("{}.", i + 1);
            let index_w = ui_primitives::get_text_width(&index_text, target_font_size, false);
            let gap = 4.0 * scale;
            
            let item_max_w = (w as i32 - (25.0 * scale) as i32).max(10) as u32;
            let name_max_w = (item_max_w as f32 - index_w - gap).max(10.0) as u32;
            let item_h = (BASE_LIST_ITEM_HEIGHT as f32 * scale) as i32;

            // Draw Index (Fixed)
            ui_primitives::draw_text_dw_ex_nowrap(
                buffer, win_w, &index_text, panel_x + (12.0 * scale) as i32, item_y + (4.0 * scale) as i32,
                target_font_size, target_color, index_w as u32 + 2, item_h as u32, 0.0, 0.0, 2000
            );

            // Text Marquee for Song Name
            let mut item_scroll_x = 0.0;
            let name_text_w = ui_primitives::get_text_width(display_name, target_font_size, false);
            if (is_hovered || is_current) && name_text_w > name_max_w as f32 {
                let now = state.timestamp;
                let text_overflow = name_text_w - name_max_w as f32;
                let speed = 35.0; 
                let pause_duration = 1.5;
                let move_duration = text_overflow / speed;
                let total_half_cycle = pause_duration + move_duration;
                let total_cycle = total_half_cycle * 2.0;

                let local_time = (now / 1000.0) % total_cycle as f64;
                
                item_scroll_x = if local_time < pause_duration as f64 {
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
                item_scroll_x = item_scroll_x.min(0.0).max(-text_overflow);
            }

            // Draw Song Name (Marquee)
            ui_primitives::draw_text_dw_ex_nowrap(
                buffer, win_w, display_name, 
                panel_x + (12.0 * scale) as i32 + (index_w + gap) as i32, 
                item_y + (4.0 * scale) as i32,
                target_font_size, target_color, name_max_w, item_h as u32, 0.0, -item_scroll_x, 2000
            );
        }

        let visible_items = 8;

        // 6. Scrollbar
        if songs.len() > visible_items {
            let bar_w = (4.0 * scale) as u32;
            let bar_x = panel_x + w as i32 - (6.0 * scale) as i32;
            let list_area_h = (visible_items as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale) as f32;
            let total_content_h = (songs.len() as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale) as f32;
            let bar_h = (list_area_h * (list_area_h / total_content_h)).max(15.0 * scale);
            let max_scroll = (songs.len() - visible_items) as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale;
            let scroll_ratio = if max_scroll > 0.0 { state.list_scroll_offset / max_scroll } else { 0.0 };
            let bar_y = list_y as f32 + scroll_ratio * (list_area_h - bar_h);

            ui_primitives::draw_rect(
                buffer, win_w, bar_x, bar_y as i32, bar_w, bar_h as u32,
                ui_primitives::apply_opacity(0xFB7299, opacity * 0.7), win_w, win_h
            );
        }
    }
}


// --- ASYNC RENDERER IMPLEMENTATION ---
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, Arc};
use std::thread;

pub struct MusicRenderer {
    tx: Sender<MusicRenderState>,
    result: Arc<Mutex<Option<MusicRenderResult>>>,
}

impl MusicRenderer {
    pub fn new() -> Self {
        let (tx, rx) = channel::<MusicRenderState>();
        let result = Arc::new(Mutex::new(None));
        let result_clone = result.clone();

        thread::spawn(move || {
            let _scratchpad = crate::ui_primitives::ScratchpadRenderer::new();
            let mut last_processed_time = std::time::Instant::now();

            loop {
                // Determine wait strategy: if playing, we want 60fps (~16ms). If not, we can wait longer.
                let mut current_state = None;
                
                // Drain everything from channel to get the LATEST state
                while let Ok(s) = rx.try_recv() {
                    current_state = Some(s);
                }

                if current_state.is_none() {
                    // Block with timeout to handle animations (rotating disc) even if no input state changes
                    if let Ok(s) = rx.recv_timeout(std::time::Duration::from_millis(16)) {
                        current_state = Some(s);
                    }
                }

                if let Some(state) = current_state {
                    let scale = state.scale;
                    let w = (BASE_PANEL_WIDTH as f32 * scale) as u32;
                    let mut h = (BASE_PANEL_HEIGHT as f32 * scale) as u32;
                    if state.list_visible && !state.songs.is_empty() {
                        h += (state.songs.len().min(8) as f32 * BASE_LIST_ITEM_HEIGHT as f32 * scale) as u32;
                    }

                    if w > 0 && h > 0 {
                        let mut buffer = vec![0u32; (w * h) as usize];
                        render_music_panel(&state, &mut buffer, w, h, 0, 0);
                        
                        let mut res = result_clone.lock().unwrap();
                        *res = Some(MusicRenderResult {
                            buffer,
                            width: w,
                            height: h,
                        });
                    }
                } else if last_processed_time.elapsed() >= std::time::Duration::from_millis(16) {
                    // Placeholder for animation-only update if needed
                }
                last_processed_time = std::time::Instant::now();
            }
        });

        Self { tx, result }
    }

    pub fn update_state(&self, state: MusicRenderState) {
        let _ = self.tx.send(state);
    }

    pub fn get_latest_render(&self) -> Option<MusicRenderResult> {
        let mut res = self.result.lock().unwrap();
        res.take() // Consume the frame
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
    let ctrl_start_x = title_x + (2.0 * scale) as i32; // Mode Toggle shift
    let ctrl_y = (45.0 * scale) as i32;
    let btn_gap = (30.0 * scale) as i32;

    if ry >= ctrl_y - (5.0 * scale) as i32 && ry < ctrl_y + (25.0 * scale) as i32 {
        if rx >= ctrl_start_x && rx < ctrl_start_x + (25.0 * scale) as i32 {
            return Some(MusicPanelAction::ToggleMode);
        } else if rx >= ctrl_start_x + btn_gap && rx < ctrl_start_x + btn_gap + (25.0 * scale) as i32 {
            return Some(MusicPanelAction::Prev);
        } else if rx >= ctrl_start_x + btn_gap * 2 && rx < ctrl_start_x + btn_gap * 2 + (25.0 * scale) as i32 {
            return Some(MusicPanelAction::PlayPause);
        } else if rx >= ctrl_start_x + btn_gap * 3 && rx < ctrl_start_x + btn_gap * 3 + (25.0 * scale) as i32 {
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
