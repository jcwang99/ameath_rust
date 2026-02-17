use crate::theme::*;
use crate::ui_primitives::*;

pub struct HistoryTabState<'a> {
    pub history: &'a [(String, String)],
    pub history_hashes: &'a [u64],
    pub history_metrics_cache: &'a [f32],
    pub history_scroll_states: &'a mut Vec<f32>,
    pub history_item_rects: &'a mut Vec<(f64, f64, f64, f64)>,
    pub scroll_offset: f32,
}

pub fn draw(
    buffer: &mut [u32],
    w: u32,
    h: u32,
    scale: f32,
    off_x: f32,
    off_y: f32,
    state: &mut HistoryTabState,
) -> (f32, f32, Option<(i32, i32, u32, u32)>) {
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };
    let sc = |val: f32| -> f32 { val * scale };

    let start_y = sy_val(140);
    let current_y = start_y as f32 + state.scroll_offset;

    state.history_item_rects.clear();
    if state.history_scroll_states.len() != state.history.len() {
        state.history_scroll_states.resize(state.history.len(), 0.0);
    }

    let item_h_fixed = sc(180.0);
    let spacing = sc(10.0);
    let total_item_h = item_h_fixed + spacing;

    // Process items (O(Visible) for drawing)
    let min_y_vis = sy_val(120) as f32;
    let max_y_vis = h as f32;

    for i in 0..state.history.len() {
        let (role, content) = &state.history[i];
        let is_user = role == "user";
        let card_color = if is_user { 0x003A3A42 } else { 0x002D2D35 };
        let text_color = if is_user {
            COLOR_TEXT_MAIN
        } else {
            COLOR_TEXT_SEC
        };

        let logical_y = 140.0 + (i as f64 * 190.0);
        let logical_h = 180.0;

        // Add to rects for click detection
        state
            .history_item_rects
            .push((230.0, logical_y, 720.0, logical_y + logical_h));

        let y_pos = current_y + (i as f32 * total_item_h);
        let y_pos_i = y_pos as i32;

        // Visibility Check
        if (y_pos + item_h_fixed) < min_y_vis || y_pos > max_y_vis {
            continue;
        }

        let scroll = state.history_scroll_states[i];
        let full_content_h = state.history_metrics_cache[i].max(sc(20.0));
        let view_h = item_h_fixed - sc(40.0);
        let start_text_y = y_pos + sc(35.0);
        let card_w = sc(490.0) as u32;
        let card_h = item_h_fixed as u32;

        // WHOLE CARD CACHING
        let mut card_hasher = std::collections::hash_map::DefaultHasher::new();
        use std::hash::{Hash, Hasher};
        state.history_hashes[i].hash(&mut card_hasher);
        (scroll as i32).hash(&mut card_hasher);
        let card_key = LayoutKey {
            text_hash: card_hasher.finish(),
            font_size_bits: sc(16.0).to_bits(),
            max_w: card_w,
            font_family_hash: (is_user as u64),
            is_bold: is_user,
            is_centered: false,
        };

        let card_blit_success = {
            let cache = get_raster_cache().read().unwrap();
            if let Some(entry) = cache.map.get(&card_key) {
                blit_opaque(
                    buffer,
                    w,
                    s(230) as i32,
                    y_pos_i,
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

        if !card_blit_success {
            // Render the whole card into a temporary buffer
            let mut card_buffer = vec![COLOR_BG_APP; (card_w * card_h) as usize];

            // 1. Background
            draw_rounded_rect_internal(
                &mut card_buffer,
                card_w,
                0,
                0,
                card_w,
                card_h,
                8,
                card_color,
            );

            // 2. Role
            draw_text_dw_ex(
                &mut card_buffer,
                card_w,
                role,
                sc(10.0) as i32,
                sc(10.0) as i32,
                sc(14.0),
                text_color,
                card_w,
                sc(30.0) as u32,
                0.0,
            );

            // 3. Content (Inside the card buffer)
            draw_text_dw_h(
                &mut card_buffer,
                card_w,
                content,
                state.history_hashes[i],
                sc(10.0) as i32,
                sc(35.0) as i32,
                sc(16.0),
                COLOR_TEXT_MAIN,
                sc(450.0) as u32,
                view_h as u32,
                scroll * scale,
            );

            // Blit to main surface
            blit_opaque(
                buffer,
                w,
                s(230) as i32,
                y_pos_i,
                card_w as i32,
                card_h as i32,
                &card_buffer,
                w,
                h,
                0,
            );

            // Store in cache
            let pixel_count = card_buffer.len();
            let mut cache = get_raster_cache().write().unwrap();
            // Evict if over 12MB (increased for whole cards)
            while cache.total_pixels + pixel_count > (12 * 1024 * 1024 / 4)
                && !cache.order.is_empty()
            {
                let oldest = cache.order.remove(0);
                if let Some(entry) = cache.map.remove(&oldest) {
                    cache.total_pixels -= entry.pixel_count;
                }
            }
            cache.order.push(card_key.clone());
            cache.total_pixels += pixel_count;
            cache.map.insert(
                card_key,
                RasterEntry {
                    pixels: card_buffer,
                    tw: card_w as i32,
                    th: card_h as i32,
                    pixel_count,
                },
            );
        }

        if full_content_h > view_h {
            let sb_w = sc(4.0) as u32;
            let sb_h = view_h;
            let sb_x = s(230 + 480);
            let sb_y_raw = start_text_y;

            draw_rect(
                buffer,
                w,
                sb_x as i32,
                sb_y_raw as i32,
                sb_w,
                sb_h as u32,
                COLOR_BORDER,
                w,
                h,
            );
            let ratio = view_h / full_content_h;
            let h_h = (view_h * ratio).max(sc(20.0));
            let max_scroll = -(full_content_h - view_h);
            let progress = if max_scroll.abs() < 1.0 {
                0.0
            } else {
                (scroll * scale / max_scroll).clamp(0.0, 1.0)
            };
            let h_y = sb_y_raw + (view_h - h_h) * progress;
            draw_rect(
                buffer,
                w,
                sb_x as i32,
                h_y as i32,
                sb_w,
                h_h as u32,
                0x00A0A0A0,
                w,
                h,
            );
        }
    }

    let viewport_h = 600.0;
    let content_h = (state.history.len() as f32 * 190.0).max(1.0);
    (viewport_h, content_h, None)
}
