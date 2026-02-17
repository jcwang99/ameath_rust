pub mod tabs;

use crate::theme::*;
use crate::types::{AiConfig, BehaviorMode, PersistentConfig, WindowLayer};
use crate::ui_primitives::*;
use softbuffer::{Context, Surface};
// use windows::core::ComInterface;
// use windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget;
use std::rc::Rc;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::Window;

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum SettingsAction {
    None,
    SetScale(f32),
    SetMode(BehaviorMode),
    SetLayer(WindowLayer),
    SetAiApiKey(String),
    SetAiBaseUrl(String),
    SetAiModel(String),
    SetAiReactLimit(usize),
    SetAiL1Threshold(usize),
    SetAiL2Threshold(usize),
    SetAiTavilyKey(String),
    SetAiSystemPrompt(String),
    SetAiInteractionFrequency(u64),
    RequestHistory,
    SetMonitor(String),
    SelectMusicPath,
}

pub struct SettingsWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    pub current_tab: usize,
    pub scroll_offset: f32,
    pub content_height: f32,
    pub viewport_height: f32,

    // AI Tab State
    pub focused_field: Option<usize>,
    pub show_api_key: bool,
    pub cursor_pos: usize,
    pub selection_start: Option<usize>,
    pub last_cursor_action: std::time::Instant,
    pub is_dragging_text: bool,
    pub system_prompt_scroll_offset: f32,
    pub active_sys_prompt_content_height: f32,
    pub active_sys_prompt_rect: Option<(f64, f64, f64, f64)>,

    // History Tab State
    pub history: Vec<(String, String)>,
    pub history_scroll_states: Vec<f32>,
    pub history_item_rects: Vec<(f64, f64, f64, f64)>,
    pub history_hashes: Vec<u64>,
    pub history_metrics_cache: Vec<f32>, // Cached heights
    pub dragging_history_idx: Option<usize>,
    pub dragging_sys_prompt: bool,
    pub system_prompt_hash: u64,
    pub system_prompt_metrics_cache: f32,
    pub config_dirty: bool,

    // Layout
    pub is_dragging_scrollbar: bool,
    pub last_size: (u32, u32),
    pub available_monitors: Vec<(String, String)>,
    pub current_monitor_name: Option<String>,

    pub is_dirty: bool,
    pub last_state_hash: u64,
    pub last_config_hash: u64,

    // Layered Rendering Caches
    pub static_layer_buffer: Vec<u32>,
    pub skeleton_layer_buffer: Vec<u32>,
    pub last_base_state_hash: u64,
    pub last_skeleton_hash: u64,
    pub cursor_cache: Option<(i32, i32, u32, u32)>,
}

impl SettingsWindow {
    pub fn new(event_loop: &EventLoopWindowTarget<()>, icon: Option<winit::window::Icon>) -> Self {
        let window = Rc::new(
            winit::window::WindowBuilder::new()
                .with_title("Ameath Settings")
                .with_inner_size(winit::dpi::LogicalSize::new(800, 750))
                .with_resizable(true)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                .with_window_icon(icon)
                .build(event_loop)
                .unwrap(),
        );
        window.set_ime_allowed(true);

        let context = Context::new(window.clone()).unwrap();
        let surface = Surface::new(&context, window.clone()).unwrap();

        Self {
            window,
            context,
            surface,
            current_tab: 0,
            scroll_offset: 0.0,
            content_height: 0.0,
            viewport_height: 0.0,
            focused_field: None,
            show_api_key: false,
            cursor_pos: 0,
            selection_start: None,
            last_cursor_action: std::time::Instant::now(),
            is_dragging_text: false,
            system_prompt_scroll_offset: 0.0,
            active_sys_prompt_content_height: 0.0,
            active_sys_prompt_rect: None,
            history: Vec::new(),
            history_scroll_states: Vec::new(),
            history_item_rects: Vec::new(),
            history_hashes: Vec::new(),
            history_metrics_cache: Vec::new(),
            system_prompt_hash: 0,
            system_prompt_metrics_cache: 0.0,
            config_dirty: true,
            is_dragging_scrollbar: false,
            available_monitors: event_loop
                .available_monitors()
                .map(|m| (m.name().unwrap_or_default(), m.name().unwrap_or_default()))
                .collect(),
            current_monitor_name: None,
            last_size: (800, 750),
            is_dirty: true,
            last_state_hash: 0,
            last_config_hash: 0,
            static_layer_buffer: Vec::new(),
            skeleton_layer_buffer: Vec::new(),
            last_base_state_hash: 0,
            last_skeleton_hash: 0,
            cursor_cache: None,
            dragging_history_idx: None,
            dragging_sys_prompt: false,
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        self.window.focus_window();
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn request_redraw_actual(&self) {
        self.window.request_redraw();
    }

    pub fn next_blink_at(&self) -> std::time::Instant {
        if self.focused_field.is_none() {
            // Hibernate for 1 hour if nothing to blink
            return std::time::Instant::now() + std::time::Duration::from_secs(3600);
        }
        let elapsed_ms = self.last_cursor_action.elapsed().as_millis();
        let current_step = elapsed_ms / 500;
        let next_step = current_step + 1;
        self.last_cursor_action + std::time::Duration::from_millis((next_step * 500) as u64)
    }

    pub fn window(&self) -> &Rc<Window> {
        &self.window
    }

    pub fn redraw(
        &mut self,
        current_scale: f32,
        current_mode: &str,
        current_music_path: Option<&std::path::Path>,
        current_layer: crate::types::WindowLayer,
        ai_config: &crate::types::AiConfig,
    ) {
        let size = self.window.inner_size();
        let w = size.width;
        let h = size.height;
        if w == 0 || h == 0 {
            return;
        }

        self.current_monitor_name = self.window.current_monitor().and_then(|m| m.name());

        if self.last_size != (w, h) {
            self.surface
                .resize(
                    std::num::NonZeroU32::new(w).unwrap(),
                    std::num::NonZeroU32::new(h).unwrap(),
                )
                .unwrap();
            self.last_size = (w, h);
            self.history_hashes.clear(); // Force re-calculation of everything
            self.history_metrics_cache.clear();
            self.system_prompt_metrics_cache = 0.0;
            self.is_dirty = true;
        }

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // 0. Efficient Config Hashing
        if self.config_dirty || self.last_config_hash == 0 {
            let mut config_hasher = DefaultHasher::new();
            ai_config.api_key.hash(&mut config_hasher);
            ai_config.base_url.hash(&mut config_hasher);
            ai_config.model.hash(&mut config_hasher);
            // Use existing system_prompt_hash if possible
            if self.system_prompt_hash == 0 {
                let mut s_hasher = DefaultHasher::new();
                ai_config.system_prompt.hash(&mut s_hasher);
                self.system_prompt_hash = s_hasher.finish();
            }
            self.system_prompt_hash.hash(&mut config_hasher);
            ai_config.tavily_api_key.hash(&mut config_hasher);
            ai_config.brave_api_key.hash(&mut config_hasher);
            ai_config.firecrawl_api_key.hash(&mut config_hasher);
            ai_config.firecrawl_url.hash(&mut config_hasher);
            ai_config.interaction_frequency.hash(&mut config_hasher);
            ai_config.l1_summary_threshold.hash(&mut config_hasher);
            ai_config.l2_merge_threshold.hash(&mut config_hasher);
            ai_config.react_limit.hash(&mut config_hasher);
            self.last_config_hash = config_hasher.finish();
            self.config_dirty = false;
        }

        // 1. Skeleton Hash (Static UI Chrome: Sidebar, Header)
        let mut skeleton_hasher = DefaultHasher::new();
        w.hash(&mut skeleton_hasher);
        h.hash(&mut skeleton_hasher);
        self.current_tab.hash(&mut skeleton_hasher);
        let skeleton_hash = skeleton_hasher.finish();

        // 2. Base Hash (Everything except cursor blink)
        let mut base_hasher = DefaultHasher::new();
        skeleton_hash.hash(&mut base_hasher);
        self.scroll_offset.to_bits().hash(&mut base_hasher);
        self.focused_field.hash(&mut base_hasher);
        self.cursor_pos.hash(&mut base_hasher);
        self.history.len().hash(&mut base_hasher);
        self.last_config_hash.hash(&mut base_hasher);
        self.system_prompt_scroll_offset
            .to_bits()
            .hash(&mut base_hasher);
        for offset in &self.history_scroll_states {
            offset.to_bits().hash(&mut base_hasher);
        }
        let base_state_hash = base_hasher.finish();

        // 3. Transient Hash (Includes cursor blink ONLY if focused)
        let mut transient_hasher = DefaultHasher::new();
        transient_hasher.write_u64(base_state_hash);
        if self.focused_field.is_some() {
            let elapsed_ms = self.last_cursor_action.elapsed().as_millis();
            let is_cursor_on = (elapsed_ms / 500) % 2 == 0;
            is_cursor_on.hash(&mut transient_hasher);
        }
        let current_hash = transient_hasher.finish();

        if !self.is_dirty && self.last_state_hash == current_hash {
            // ZERO-IDLE: Skip presentation if nothing changed
            return;
        }

        let needs_skeleton_redraw = self.is_dirty
            || self.last_skeleton_hash != skeleton_hash
            || self.skeleton_layer_buffer.len() != (w * h) as usize;
        let needs_static_redraw = needs_skeleton_redraw
            || self.last_base_state_hash != base_state_hash
            || self.static_layer_buffer.len() != (w * h) as usize;

        // Scaling (Target 800x750)
        let scale = (w as f32 / 800.0).min(h as f32 / 750.0);
        let off_x = (w as f32 - 800.0 * scale) / 2.0;
        let off_y = (h as f32 - 750.0 * scale) / 2.0;

        let sc = |val: f32| -> f32 { val * scale };
        let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
        let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };

        // REDRAW SKELETON LAYER (SIDEBAR + HEADER)
        if needs_skeleton_redraw {
            self.skeleton_layer_buffer.resize((w * h) as usize, 0);
            self.skeleton_layer_buffer.fill(COLOR_BG_APP);
            let skel_buf = &mut self.skeleton_layer_buffer;

            // Sidebar
            draw_rect(skel_buf, w, 0, 0, s(180), h, COLOR_BG_SIDEBAR, w, h);
            let icons = ["🏠", "🎨", "🧠", "📜", "ℹ️"];
            for i in 0..5 {
                let color = if self.current_tab == i {
                    COLOR_PRIMARY
                } else {
                    COLOR_TEXT_SEC
                };
                draw_text(
                    skel_buf,
                    w,
                    &[],
                    icons[i],
                    s(75) as i32,
                    sy_val(60 + i as u32 * 80) as i32,
                    sc(32.0),
                    color,
                );
            }

            // Header Background & Static Text
            let (title, sub) = match self.current_tab {
                0 => ("Home", "Welcome to Ameath!"),
                1 => ("Appearance", "Customize your pet's look"),
                2 => ("AI Brain", "Connect Ameath to the cloud"),
                3 => ("History", "Recent Local Memory (Last 50)"),
                _ => ("About", "Ameath v0.1.0"),
            };
            let header_h = sy_val(120);
            draw_rect(
                skel_buf,
                w,
                s(180) as i32,
                0,
                w - s(180),
                header_h,
                COLOR_BG_APP,
                w,
                h,
            );
            draw_text(
                skel_buf,
                w,
                &[],
                title,
                s(220) as i32,
                sy_val(40) as i32,
                sc(32.0),
                COLOR_TEXT_MAIN,
            );
            draw_text(
                skel_buf,
                w,
                &[],
                sub,
                s(220) as i32,
                sy_val(85) as i32,
                sc(16.0),
                COLOR_TEXT_SEC,
            );

            self.last_skeleton_hash = skeleton_hash;
        }

        // REDRAW STATIC LAYER (TAB CONTENT + SKELETON)
        if needs_static_redraw {
            self.static_layer_buffer.resize((w * h) as usize, 0);
            self.static_layer_buffer
                .copy_from_slice(&self.skeleton_layer_buffer);
            let static_buf = &mut self.static_layer_buffer;

            // Tab Content
            match self.current_tab {
                0 => {
                    let (vh, ch, _) = tabs::home::draw(static_buf, w, h, scale, off_x, off_y);
                    self.viewport_height = vh;
                    self.content_height = ch;
                }
                1 => {
                    let mut gen_state = tabs::general::GeneralTabState {
                        current_scale,
                        current_mode,
                        current_music_path,
                        current_layer,
                        scroll_offset: self.scroll_offset,
                        available_monitors: &self.available_monitors,
                        current_monitor_name: self.current_monitor_name.as_deref(),
                    };
                    let (vh, ch, _) =
                        tabs::general::draw(static_buf, w, h, scale, off_x, off_y, &mut gen_state);
                    self.viewport_height = vh;
                    self.content_height = ch;
                }
                2 => {
                    let mut ai_state = tabs::ai::AiTabState {
                        focused_field: self.focused_field,
                        show_api_key: self.show_api_key,
                        cursor_pos: self.cursor_pos,
                        selection_start: self.selection_start,
                        last_cursor_action: self.last_cursor_action,
                        system_prompt_scroll_offset: self.system_prompt_scroll_offset,
                        active_sys_prompt_content_height: &mut self
                            .active_sys_prompt_content_height,
                        active_sys_prompt_rect: &mut self.active_sys_prompt_rect,
                        system_prompt_metrics_cache: &mut self.system_prompt_metrics_cache,
                        system_prompt_hash: self.system_prompt_hash,
                        config_hash: self.last_config_hash,
                        draw_cursor: false,
                    };
                    let (vh, ch, cursor_rect) = tabs::ai::draw(
                        static_buf,
                        w,
                        h,
                        scale,
                        off_x,
                        off_y,
                        self.scroll_offset,
                        ai_config,
                        &mut ai_state,
                    );
                    self.viewport_height = vh;
                    self.content_height = ch;
                    self.cursor_cache = cursor_rect;
                }
                3 => {
                    // Sync metadata for History tab once (On-demand caching)
                    if self.history_hashes.len() != self.history.len() {
                        let old_len = self.history_hashes.len();
                        self.history_hashes.resize(self.history.len(), 0);
                        self.history_metrics_cache.resize(self.history.len(), 0.0);
                        let max_text_w = sc(450.0) as u32;

                        for i in old_len..self.history.len() {
                            let (_, content) = &self.history[i];
                            let mut h_hasher = DefaultHasher::new();
                            content.hash(&mut h_hasher);
                            self.history_hashes[i] = h_hasher.finish();
                            let (_, mh) =
                                crate::ui_primitives::get_metrics_dw(content, sc(16.0), max_text_w);
                            self.history_metrics_cache[i] = mh;
                        }
                    }
                    let mut history_state = tabs::history::HistoryTabState {
                        history: &self.history,
                        history_hashes: &self.history_hashes,
                        history_metrics_cache: &self.history_metrics_cache,
                        history_scroll_states: &mut self.history_scroll_states,
                        history_item_rects: &mut self.history_item_rects,
                        scroll_offset: self.scroll_offset * scale,
                    };
                    let (vh, ch, _) = tabs::history::draw(
                        static_buf,
                        w,
                        h,
                        scale,
                        off_x,
                        off_y,
                        &mut history_state,
                    );
                    self.viewport_height = vh;
                    self.content_height = ch;
                }
                4 => {
                    let (vh, ch, _) = tabs::about::draw(static_buf, w, h, scale, off_x, off_y);
                    self.viewport_height = vh;
                    self.content_height = ch;
                }
                _ => {}
            }

            // Global Scrollbar
            if self.content_height > self.viewport_height {
                let sb_w = sc(6.0) as u32;
                let sb_h = sc(600.0);
                let sb_x = s(785) as i32;
                let sb_y = sy_val(130);
                draw_rounded_rect(
                    static_buf,
                    w,
                    sb_x,
                    sb_y as i32,
                    sb_w,
                    sb_h as u32,
                    3,
                    COLOR_BG_LIGHT,
                    w,
                    h,
                );
                let ratio = (self.viewport_height / self.content_height).clamp(0.0, 1.0);
                let hh = (sb_h * ratio).max(sc(30.0));
                let max_sc = -(self.content_height - self.viewport_height);
                let prog = if max_sc.abs() < 1.0 {
                    0.0
                } else {
                    (self.scroll_offset / max_sc).clamp(0.0, 1.0)
                };
                let hy = sb_y as f32 + (sb_h - hh) * prog;
                draw_rounded_rect(
                    static_buf, w, sb_x, hy as i32, sb_w, hh as u32, 3, 0x00CCCCCC, w, h,
                );
            }
            self.last_base_state_hash = base_state_hash;
        }

        let mut buffer = self.surface.buffer_mut().unwrap();

        // 5. Layer Composition (Dirty Optimization)
        let only_cursor_blink = !needs_static_redraw && !self.is_dirty;

        if only_cursor_blink {
            // SURGICAL RESTORE: If ONLY blinking, don't copy all 600k pixels.
            // Just restore the old cursor area from the static cache.
            if let Some((cx, cy, cw, ch)) = self.cursor_cache {
                let surface_w = w as usize;
                let static_buf = &self.static_layer_buffer;
                let surface_h = h as usize;

                for row in 0..ch {
                    let target_y = cy + row as i32;
                    if target_y < 0 || target_y >= surface_h as i32 {
                        continue;
                    }
                    let y_idx = target_y as usize;
                    let row_start = y_idx * surface_w;

                    for col in 0..cw {
                        let target_x = cx + col as i32;
                        if target_x < 0 || target_x >= surface_w as i32 {
                            continue;
                        }
                        let x_idx = target_x as usize;
                        let idx = row_start + x_idx;

                        if idx < buffer.len() && idx < static_buf.len() {
                            buffer[idx] = static_buf[idx];
                        }
                    }
                }
            }
        } else {
            // Full copy if something else changed or first frame
            buffer.copy_from_slice(&self.static_layer_buffer);
        }

        self.last_state_hash = current_hash;
        self.is_dirty = false;

        // Rendering params (Already calculated in outer scope)

        // 6. Transient Layer (Cursor)
        let elapsed_ms = self.last_cursor_action.elapsed().as_millis();
        let is_cursor_on = (elapsed_ms / 500) % 2 == 0;

        if is_cursor_on && self.focused_field.is_some() {
            if let Some((cx, cy, cw, ch)) = self.cursor_cache {
                // MICRO-REDRAW: Just draw the primary color rect over the buffer
                // Use absolute bounds check
                if cx >= 0
                    && cy >= 0
                    && (cx + cw as i32) <= w as i32
                    && (cy + ch as i32) <= h as i32
                {
                    draw_rect(&mut buffer, w, cx, cy, cw, ch, COLOR_PRIMARY, w, h);
                }
            }
        }

        buffer.present().unwrap();
    }

    pub fn handle_click(
        &mut self,
        x: f64,
        y: f64,
        _is_right_click: bool,
        ai_config: &crate::types::AiConfig,
    ) -> SettingsAction {
        let size = self.window.inner_size();
        let w = size.width as f64;
        let h = size.height as f64;
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let off_y = (h - 750.0 * scale) / 2.0;

        self.is_dirty = true;
        let lx = (x - off_x) / scale;
        let ly = (y - off_y) / scale;
        let dlx = lx;
        let dly = ly - self.scroll_offset as f64;

        // Sidebar
        if lx >= 0.0 && lx <= 180.0 {
            for i in 0..5 {
                let ty = 60.0 + i as f64 * 80.0;
                if ly >= ty - 15.0 && ly <= ty + 45.0 {
                    self.current_tab = i;
                    self.scroll_offset = 0.0;
                    self.focused_field = None;
                    if i == 3 {
                        return SettingsAction::RequestHistory;
                    }
                    self.window.request_redraw();
                    return SettingsAction::None;
                }
            }
        }

        // Scrollbar Drag & Jump
        if self.content_height > self.viewport_height {
            if lx >= 770.0 && lx <= 810.0 && ly >= 130.0 && ly <= 730.0 {
                self.is_dragging_scrollbar = true;
                // Immediate jump
                let track_ly_start = 130.0;
                let track_ly_end = 730.0;
                let progress =
                    ((ly - track_ly_start) / (track_ly_end - track_ly_start)).clamp(0.0, 1.0);
                let max_scroll = -(self.content_height - self.viewport_height);
                self.scroll_offset = progress as f32 * max_scroll;
                self.window.request_redraw();
                return SettingsAction::None;
            }
        }

        match self.current_tab {
            1 => {
                // Tab 1: General
                let card_w = 560.0;
                let scroll_y = self.scroll_offset as f64;
                let card1_y = 120.0 + scroll_y;
                let card2_y = 280.0 + scroll_y;
                let card3_y = 505.0 + scroll_y;
                let card4_y = 665.0 + scroll_y;

                if lx >= 210.0 && lx <= 210.0 + card_w {
                    // Pet Scale
                    if ly >= card1_y + 60.0 && ly <= card1_y + 105.0 {
                        let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
                        for (i, &val) in scales.iter().enumerate() {
                            let mx = 220.0 + i as f64 * 85.0;
                            if lx >= mx && lx <= mx + 75.0 {
                                return SettingsAction::SetScale(val);
                            }
                        }
                    }
                    // Behavior
                    let modes = vec![
                        BehaviorMode::Static,
                        BehaviorMode::Quiet,
                        BehaviorMode::Active,
                        BehaviorMode::Clingy,
                    ];
                    for (i, mode) in modes.into_iter().enumerate() {
                        let row = i / 2;
                        let col = i % 2;
                        let mx = 230.0 + col as f64 * 165.0;
                        let my = card2_y + 60.0 + row as f64 * 65.0;
                        if lx >= mx && lx <= mx + 150.0 && ly >= my && ly <= my + 55.0 {
                            return SettingsAction::SetMode(mode);
                        }
                    }
                    // Music
                    if ly >= card3_y + 60.0 && ly <= card3_y + 105.0 {
                        if lx >= 230.0 && lx <= 730.0 {
                            return SettingsAction::SelectMusicPath;
                        }
                    }
                    // Layer
                    if ly >= card4_y + 60.0 && ly <= card4_y + 115.0 {
                        if lx >= 230.0 && lx <= 430.0 {
                            return SettingsAction::SetLayer(WindowLayer::Top);
                        }
                        if lx >= 440.0 && lx <= 640.0 {
                            return SettingsAction::SetLayer(WindowLayer::Bottom);
                        }
                    }
                }

                // Monitor selection
                let card5_y = 825.0 + scroll_y;
                let rows = (self.available_monitors.len() + 2) / 3;
                let card5_h = 60.0 + (rows as f64 * 65.0);
                if dlx >= 210.0
                    && dlx <= 210.0 + card_w
                    && dly >= card5_y + 60.0
                    && dly <= card5_y + card5_h
                {
                    for (i, (name, _)) in self.available_monitors.iter().enumerate() {
                        let row = i / 3;
                        let col = i % 3;
                        let mx = 230.0 + col as f64 * 110.0;
                        let my = card5_y + 60.0 + row as f64 * 65.0;
                        if dlx >= mx && dlx <= mx + 100.0 && dly >= my && dly <= my + 55.0 {
                            return SettingsAction::SetMonitor(name.clone());
                        }
                    }
                }
            }
            2 => {
                // Tab 2: AI
                let design_card_y = 120.0;

                // Priority: Sub-scrollbar
                if lx >= 230.0 + 480.0 && lx <= 230.0 + 480.0 + 8.0 {
                    let input_y = design_card_y + 930.0 + 25.0;
                    if dly >= input_y && dly <= input_y + 250.0 {
                        self.dragging_sys_prompt = true;
                        let progress = ((dly - input_y) / 250.0).clamp(0.0, 1.0);
                        let view_h = 250.0;
                        let content_h = self.active_sys_prompt_content_height;
                        let max_scroll = -(content_h - view_h).max(0.0);
                        self.system_prompt_scroll_offset = progress as f32 * max_scroll;
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                }

                let fields = vec![
                    (230.0, 30.0, 500.0),  // 0: Key
                    (230.0, 130.0, 500.0), // 1: URL
                    (230.0, 230.0, 500.0), // 2: Model
                    (230.0, 330.0, 150.0), // 3: Steps
                    (405.0, 330.0, 150.0), // 4: L1
                    (580.0, 330.0, 150.0), // 5: L2
                    (230.0, 430.0, 150.0), // 6: Interval
                    (230.0, 530.0, 500.0), // 7: Tavily
                    (230.0, 630.0, 500.0), // 8: Brave
                    (230.0, 730.0, 500.0), // 9: FC URL
                    (230.0, 830.0, 500.0), // 10: FC Key
                    (230.0, 930.0, 500.0), // 11: System
                ];

                for (i, (fx, fy, fw)) in fields.iter().enumerate() {
                    let input_y = design_card_y + fy + 25.0;
                    let input_h = if i == 11 { 250.0 } else { 45.0 };

                    if dlx >= *fx && dlx <= *fx + *fw && dly >= input_y && dly <= input_y + input_h
                    {
                        self.focused_field = Some(i);
                        self.last_cursor_action = std::time::Instant::now();
                        let text = self.get_field_text(i, ai_config);

                        if i == 11 {
                            // System prompt multi-line
                            let text_x = lx - fx - 15.0;
                            let text_y =
                                dly - input_y - 12.0 - self.system_prompt_scroll_offset as f64;
                            self.cursor_pos =
                                self.get_cursor_from_xy(&text, text_x, text_y, scale as f32);

                            if !_is_right_click {
                                self.selection_start = Some(self.cursor_pos);
                                self.is_dragging_text = true;
                            }
                        } else {
                            if !_is_right_click {
                                if lx >= *fx + *fw - 45.0 && (i == 0 || i == 7 || i == 8 || i == 10)
                                {
                                    self.show_api_key = !self.show_api_key;
                                    self.config_dirty = true;
                                } else {
                                    let text_x = lx - fx - 15.0;
                                    self.cursor_pos =
                                        self.get_cursor_from_x(&text, text_x, scale as f32);
                                    self.selection_start = Some(self.cursor_pos);
                                    self.is_dragging_text = true;
                                }
                            }
                        }
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                }

                self.focused_field = None;
                self.selection_start = None;
                self.window.request_redraw();
                return SettingsAction::None;
            }
            3 => {
                // Tab 3: History
                if dlx >= 230.0 + 480.0 && dlx <= 230.0 + 480.0 + 8.0 {
                    for (i, (_rx_start, ry_start, _rx_end, ry_end)) in
                        self.history_item_rects.iter().enumerate()
                    {
                        if dly >= *ry_start && dly <= *ry_end {
                            // Hit a history item row's X range?
                            // Actually history.rs draws scrollbar at s(230 + 480)
                            let track_y_start = *ry_start + 35.0;
                            let track_h = 140.0;
                            if dly >= track_y_start && dly <= track_y_start + track_h {
                                self.dragging_history_idx = Some(i);
                                let progress = ((dly - track_y_start) / track_h).clamp(0.0, 1.0);
                                let content = &self.history[i].1;
                                let max_width = (450.0 * scale) as u32;
                                let (_, full_h) =
                                    get_metrics_dw(content, 16.0 * scale as f32, max_width);
                                let full_h_logical = full_h / scale as f32;
                                let max_scroll = -(full_h_logical - 140.0).max(0.0);
                                self.history_scroll_states[i] = progress as f32 * max_scroll;
                                self.window.request_redraw();
                                return SettingsAction::None;
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        SettingsAction::None
    }

    fn get_field_text(&self, idx: usize, ai_config: &AiConfig) -> String {
        match idx {
            0 => ai_config.api_key.clone(),
            1 => ai_config.base_url.clone(),
            2 => ai_config.model.clone(),
            3 => {
                if ai_config.react_limit == 0 {
                    String::new()
                } else {
                    ai_config.react_limit.to_string()
                }
            }
            4 => {
                if ai_config.l1_summary_threshold == 0 {
                    String::new()
                } else {
                    ai_config.l1_summary_threshold.to_string()
                }
            }
            5 => {
                if ai_config.l2_merge_threshold == 0 {
                    String::new()
                } else {
                    ai_config.l2_merge_threshold.to_string()
                }
            }
            6 => {
                if ai_config.interaction_frequency == 0 {
                    String::new()
                } else {
                    ai_config.interaction_frequency.to_string()
                }
            }
            7 => ai_config.tavily_api_key.clone(),
            8 => ai_config.brave_api_key.clone(),
            9 => ai_config.firecrawl_url.clone(),
            10 => ai_config.firecrawl_api_key.clone(),
            11 => ai_config.system_prompt.clone(),
            _ => String::new(),
        }
    }

    fn set_field_text(&mut self, idx: usize, ai_config: &mut AiConfig, text: String) {
        self.config_dirty = true;
        match idx {
            0 => ai_config.api_key = text,
            1 => ai_config.base_url = text,
            2 => ai_config.model = text,
            3 => {
                ai_config.react_limit = text.parse().unwrap_or(0);
            }
            4 => {
                ai_config.l1_summary_threshold = text.parse().unwrap_or(0);
            }
            5 => {
                ai_config.l2_merge_threshold = text.parse().unwrap_or(0);
            }
            6 => {
                ai_config.interaction_frequency = text.parse().unwrap_or(0);
            }
            7 => ai_config.tavily_api_key = text,
            8 => ai_config.brave_api_key = text,
            9 => ai_config.firecrawl_url = text,
            10 => ai_config.firecrawl_api_key = text,
            11 => {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut hasher = DefaultHasher::new();
                text.hash(&mut hasher);
                self.system_prompt_hash = hasher.finish();
                ai_config.system_prompt = text;
            }
            _ => {}
        }
    }

    fn get_cursor_from_x(&self, text: &str, x: f64, scale: f32) -> usize {
        get_cursor_index_from_xy(
            text,
            14.0 * scale,
            10000,
            (x as f32 * scale).max(0.0),
            7.0 * scale,
        )
    }

    fn get_cursor_from_xy(&self, text: &str, lx: f64, ly: f64, scale: f32) -> usize {
        let field_idx = self.focused_field.unwrap_or(0);
        let base_w = if field_idx == 11 { 460.0 } else { 500.0 };
        let max_width = base_w * scale;
        get_cursor_index_from_xy(
            text,
            14.0 * scale,
            max_width as u32,
            (lx as f32 * scale).max(0.0),
            (ly as f32 * scale).max(0.0),
        )
    }

    fn get_xy_from_cursor(&self, text: &str, cursor_pos: usize, scale: f32) -> (f64, f64) {
        let field_idx = self.focused_field.unwrap_or(0);
        let base_w = if field_idx == 11 { 460.0 } else { 500.0 };
        let max_width = base_w * scale;
        let (px, py) = get_xy_from_cursor_index(text, 14.0 * scale, max_width as u32, cursor_pos);
        (px as f64 / scale as f64, py as f64 / scale as f64)
    }

    pub fn handle_key_input(
        &mut self,
        event: &winit::event::KeyEvent,
        ai_config: &mut AiConfig,
        modifiers: winit::keyboard::ModifiersState,
    ) -> bool {
        let size = self.window.inner_size();
        let scale = ((size.width as f64 / 800.0).min(size.height as f64 / 750.0)) as f32;
        self.last_cursor_action = std::time::Instant::now();
        if self.current_tab != 2 {
            return false;
        }

        let field_idx = match self.focused_field {
            Some(i) => i,
            None => return false,
        };

        let text = self.get_field_text(field_idx, ai_config);
        let mut chars: Vec<char> = text.chars().collect();

        if self.cursor_pos > chars.len() {
            self.cursor_pos = chars.len();
        }

        // use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
        use winit::keyboard::{Key, NamedKey};
        let is_pressed = event.state == winit::event::ElementState::Pressed;
        if !is_pressed {
            return false;
        }

        let has_ctrl = modifiers.control_key() || modifiers.super_key();
        let has_shift = modifiers.shift_key();

        if let Key::Named(NamedKey::ArrowUp) = &event.logical_key {
            if field_idx == 11 {
                let (lx, ly) = self.get_xy_from_cursor(&text, self.cursor_pos, scale);
                let line_height = 20.0;
                self.cursor_pos = self.get_cursor_from_xy(&text, lx, ly - line_height + 5.0, scale);
                if !has_shift {
                    self.selection_start = None;
                }
                self.window.request_redraw();
                return true;
            }
        }
        if let Key::Named(NamedKey::ArrowDown) = &event.logical_key {
            if field_idx == 11 {
                let (lx, ly) = self.get_xy_from_cursor(&text, self.cursor_pos, scale);
                let line_height = 20.0;
                self.cursor_pos = self.get_cursor_from_xy(&text, lx, ly + line_height + 5.0, scale);
                if !has_shift {
                    self.selection_start = None;
                }
                self.window.request_redraw();
                return true;
            }
        }

        match &event.logical_key {
            Key::Named(NamedKey::ArrowLeft) => {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::ArrowRight) => {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                if self.cursor_pos < chars.len() {
                    self.cursor_pos += 1;
                }
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::Home) => {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                self.cursor_pos = 0;
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::End) => {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                self.cursor_pos = chars.len();
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(start) = self.selection_start {
                    let min = start.min(self.cursor_pos);
                    let max = start.max(self.cursor_pos);
                    if min != max {
                        chars.drain(min..max);
                        self.cursor_pos = min;
                        self.selection_start = None;
                    } else {
                        self.selection_start = None;
                        if self.cursor_pos > 0 {
                            chars.remove(self.cursor_pos - 1);
                            self.cursor_pos -= 1;
                        }
                    }
                } else if self.cursor_pos > 0 {
                    chars.remove(self.cursor_pos - 1);
                    self.cursor_pos -= 1;
                }
                self.set_field_text(field_idx, ai_config, chars.iter().collect());
                ai_config.save();
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::Delete) => {
                if let Some(start) = self.selection_start {
                    let min = start.min(self.cursor_pos);
                    let max = start.max(self.cursor_pos);
                    if min != max {
                        chars.drain(min..max);
                        self.cursor_pos = min;
                        self.selection_start = None;
                    } else {
                        self.selection_start = None;
                        if self.cursor_pos < chars.len() {
                            chars.remove(self.cursor_pos);
                        }
                    }
                } else if self.cursor_pos < chars.len() {
                    chars.remove(self.cursor_pos);
                }
                self.set_field_text(field_idx, ai_config, chars.iter().collect());
                ai_config.save();
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::Enter) => {
                if field_idx == 11 {
                    if let Some(start) = self.selection_start {
                        let min = start.min(self.cursor_pos);
                        let max = start.max(self.cursor_pos);
                        chars.drain(min..max);
                        self.cursor_pos = min;
                        self.selection_start = None;
                    }
                    chars.insert(self.cursor_pos, '\n');
                    self.cursor_pos += 1;
                    self.set_field_text(field_idx, ai_config, chars.iter().collect());
                    ai_config.save();
                    self.window.request_redraw();
                    return true;
                }
                return false;
            }
            Key::Character(c) => {
                if has_ctrl {
                    if c == "a" {
                        self.selection_start = Some(0);
                        self.cursor_pos = chars.len();
                        self.window.request_redraw();
                        return true;
                    } else if c == "c" {
                        if let Some(start) = self.selection_start {
                            let min = start.min(self.cursor_pos);
                            let max = start.max(self.cursor_pos);
                            if min != max {
                                let selected: String = chars[min..max].iter().collect();
                                use arboard::Clipboard;
                                if let Ok(mut cb) = Clipboard::new() {
                                    let _ = cb.set_text(selected);
                                }
                            }
                        }
                        return true;
                    } else if c == "v" {
                        use arboard::Clipboard;
                        if let Ok(mut cb) = Clipboard::new() {
                            if let Ok(pasted) = cb.get_text() {
                                let trimmed = pasted.trim();
                                let p_chars: Vec<char> = trimmed.chars().collect();
                                if let Some(start) = self.selection_start {
                                    let min = start.min(self.cursor_pos);
                                    let max = start.max(self.cursor_pos);
                                    chars.splice(min..max, p_chars.iter().cloned());
                                    self.cursor_pos = min + p_chars.len();
                                    self.selection_start = None;
                                } else {
                                    chars.splice(
                                        self.cursor_pos..self.cursor_pos,
                                        p_chars.iter().cloned(),
                                    );
                                    self.cursor_pos += p_chars.len();
                                }
                                self.set_field_text(field_idx, ai_config, chars.iter().collect());
                                ai_config.save();
                                self.window.request_redraw();
                            }
                        }
                        return true;
                    }
                }

                if !c.chars().any(|ch| ch.is_control()) {
                    let input_chars: Vec<char> = c.chars().collect();
                    if (field_idx == 3 || field_idx == 4 || field_idx == 5 || field_idx == 8)
                        && !input_chars.iter().all(|ch| ch.is_ascii_digit())
                    {
                        return true;
                    }

                    if let Some(start) = self.selection_start {
                        let min = start.min(self.cursor_pos);
                        let max = start.max(self.cursor_pos);
                        chars.splice(min..max, input_chars.iter().cloned());
                        self.cursor_pos = min + input_chars.len();
                        self.selection_start = None;
                    } else {
                        chars.splice(
                            self.cursor_pos..self.cursor_pos,
                            input_chars.iter().cloned(),
                        );
                        self.cursor_pos += input_chars.len();
                    }
                    self.set_field_text(field_idx, ai_config, chars.iter().collect());
                    ai_config.save();
                    self.window.request_redraw();
                    return true;
                }
            }
            Key::Named(NamedKey::Tab) => {
                self.focused_field = Some((field_idx + 1) % 9);
                self.cursor_pos = 0;
                self.selection_start = None;
                self.window.request_redraw();
                return true;
            }
            _ => {}
        }
        false
    }

    pub fn handle_ime(&mut self, text: &str, ai_config: &mut AiConfig) -> bool {
        if self.current_tab != 2 {
            return false;
        }
        if let Some(idx) = self.focused_field {
            let val = self.get_field_text(idx, ai_config);
            let mut chars: Vec<char> = val.chars().collect();

            if self.cursor_pos > chars.len() {
                self.cursor_pos = chars.len();
            }

            let input_chars: Vec<char> = text.chars().collect();

            // Support selection replacement in IME too
            if let Some(start) = self.selection_start {
                let min = start.min(self.cursor_pos);
                let max = start.max(self.cursor_pos);
                chars.splice(min..max, input_chars.iter().cloned());
                self.cursor_pos = min + input_chars.len();
                self.selection_start = None;
            } else {
                chars.splice(
                    self.cursor_pos..self.cursor_pos,
                    input_chars.iter().cloned(),
                );
                self.cursor_pos += input_chars.len();
            }

            self.set_field_text(idx, ai_config, chars.iter().collect());
            self.config_dirty = true;
            if idx == 11 {
                // Invalidate system prompt metrics cache
                self.system_prompt_hash = 0;
                self.system_prompt_metrics_cache = 0.0;
            }
            ai_config.save();
            self.last_cursor_action = std::time::Instant::now();
            self.window.request_redraw();
            self.is_dirty = true; // Added for handle_key (IME is a form of key input)
            return true;
        }
        false
    }

    pub fn handle_scroll(
        &mut self,
        dy: f32,
        cursor_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    ) {
        self.is_dirty = true;
        let size = self.window.inner_size();
        let w = size.width as f64;
        let h = size.height as f64;
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let off_y = (h - 750.0 * scale) / 2.0;
        let dy_logical = dy / scale as f32;

        let (lx, ly) = if let Some(pos) = cursor_pos {
            ((pos.x - off_x) / scale, (pos.y - off_y) / scale)
        } else {
            (-1000.0, -1000.0)
        };

        // Design-space coordinates for hit detection
        let dlx = lx;
        let dly = ly - self.scroll_offset as f64;

        if self.current_tab == 3 {
            // History Tab
            let mut scrolled_item = false;
            if self.history_item_rects.len() == self.history.len() {
                for (i, (rx_start, ry_start, rx_end, ry_end)) in
                    self.history_item_rects.iter().enumerate()
                {
                    if dlx >= *rx_start && dlx <= *rx_end && dly >= *ry_start && dly <= *ry_end {
                        let content = &self.history[i].1;
                        let item_h_fixed_sc = 180.0 * scale as f32;
                        let max_width = (450.0 * scale) as u32;
                        let (_, full_h) = get_metrics_dw(content, 16.0 * scale as f32, max_width);
                        let full_h = full_h.max(20.0 * scale as f32);
                        let view_h = item_h_fixed_sc - (40.0 * scale as f32);

                        if full_h > view_h {
                            let current_log = self.history_scroll_states[i];
                            let scroll_step_log = dy_logical;
                            let new_val_log = current_log + scroll_step_log;
                            let max_scroll_log = -(full_h - view_h) / scale as f32;
                            let clamped_log = new_val_log.clamp(max_scroll_log, 0.0);

                            if (clamped_log - current_log).abs() > 0.001 {
                                self.history_scroll_states[i] = clamped_log;
                                scrolled_item = true;
                            } else {
                                if (dy_logical > 0.0 && current_log >= -0.01)
                                    || (dy_logical < 0.0 && current_log <= max_scroll_log + 0.01)
                                {
                                    scrolled_item = false;
                                } else {
                                    scrolled_item = true;
                                }
                            }
                        }
                        break;
                    }
                }
            }

            if !scrolled_item {
                self.scroll_offset += dy_logical;
                let min_offset = -(self.content_height - self.viewport_height).max(0.0);
                self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
            }
        } else if self.current_tab == 2 {
            // AI Brain Tab
            let mut scrolled_sys_prompt = false;
            if let Some((min_x, min_y, max_x, max_y)) = self.active_sys_prompt_rect {
                if dlx >= min_x && dlx <= max_x && dly >= min_y && dly <= max_y {
                    let old_off = self.system_prompt_scroll_offset;
                    self.system_prompt_scroll_offset += dy_logical;
                    let view_h = 250.0;
                    let content_h = self.active_sys_prompt_content_height;
                    let min_offset = -(content_h - view_h).max(0.0);
                    self.system_prompt_scroll_offset =
                        self.system_prompt_scroll_offset.clamp(min_offset, 0.0);

                    if (self.system_prompt_scroll_offset - old_off).abs() > 0.01 {
                        scrolled_sys_prompt = true;
                    } else {
                        if (dy_logical > 0.0 && self.system_prompt_scroll_offset >= -0.01)
                            || (dy_logical < 0.0
                                && self.system_prompt_scroll_offset <= min_offset + 0.01)
                        {
                            scrolled_sys_prompt = false;
                        } else {
                            scrolled_sys_prompt = true;
                        }
                    }
                }
            }

            if !scrolled_sys_prompt {
                self.scroll_offset += dy_logical;
                let min_offset = -(self.content_height - self.viewport_height).max(0.0);
                self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
            }
        } else {
            self.scroll_offset += dy_logical;
            let min_offset = -(self.content_height - self.viewport_height).max(0.0);
            self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
        }
        self.window.request_redraw();
    }

    pub fn handle_mouse_move(&mut self, x: f64, y: f64, ai_config: &crate::types::AiConfig) {
        let size = self.window.inner_size();
        let w = size.width as f64;
        let h = size.height as f64;
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let off_y = (h - 750.0 * scale) / 2.0;

        let lx = (x - off_x) / scale;
        let ly = (y - off_y) / scale;

        // Sidebar hover
        if lx >= 0.0 && lx <= 180.0 {
            for i in 0..5 {
                let ty = 60.0 + i as f64 * 80.0;
                if ly >= ty - 15.0 && ly <= ty + 45.0 {
                    if self.current_tab != i {
                        // We don't have a hover state for sidebar yet, but we could trigger redraw here
                        // if we want hover effects.
                    }
                }
            }
        }
        let dlx = lx;
        let dly = ly - self.scroll_offset as f64;

        if self.is_dragging_scrollbar {
            if self.content_height > self.viewport_height {
                let track_ly_start = 130.0;
                let track_ly_end = 730.0;
                let progress =
                    ((ly - track_ly_start) / (track_ly_end - track_ly_start)).clamp(0.0, 1.0);
                let max_scroll = -(self.content_height - self.viewport_height);
                self.scroll_offset = progress as f32 * max_scroll;
                self.window.request_redraw();
            }
            return;
        }

        if let Some(idx) = self.dragging_history_idx {
            if idx < self.history.len() {
                let content = &self.history[idx].1;
                let view_h = 140.0;
                let max_width = (450.0 * scale) as u32;
                let (_, full_h) = get_metrics_dw(content, 16.0 * scale as f32, max_width);
                let full_h_logical = full_h / scale as f32;

                if self.history_item_rects.len() > idx {
                    let (_, ry_start, _, _) = self.history_item_rects[idx];
                    let track_y_start = ry_start + 35.0;
                    let track_h = view_h as f64;
                    let progress = ((dly - track_y_start) / track_h).clamp(0.0, 1.0);
                    let max_scroll = -(full_h_logical - view_h as f32).max(0.0);
                    self.history_scroll_states[idx] = progress as f32 * max_scroll;
                    self.window.request_redraw();
                }
            }
            return;
        }

        if self.dragging_sys_prompt {
            if let Some((_, ry_start, _, ry_end)) = self.active_sys_prompt_rect {
                let track_h = (ry_end - ry_start).max(1.0);
                let progress = ((dly - ry_start) / track_h).clamp(0.0, 1.0);
                let view_h = track_h as f32;
                let content_h = self.active_sys_prompt_content_height;
                let max_scroll = -(content_h - view_h).max(0.0);
                self.system_prompt_scroll_offset = progress as f32 * max_scroll;
                self.window.request_redraw();
                return;
            } else {
                // Fallback to hardcoded if rect not set yet
                let track_h = 250.0;
                let track_y_start = 120.0 + 930.0 + 25.0;
                let progress = ((dly - track_y_start) / track_h).clamp(0.0, 1.0);
                let view_h = 250.0f32;
                let content_h = self.active_sys_prompt_content_height;
                let max_scroll = -(content_h - view_h).max(0.0);
                self.system_prompt_scroll_offset = progress as f32 * max_scroll;
                self.window.request_redraw();
                return;
            }
        }

        if !self.is_dragging_text {
            return;
        }
        self.last_cursor_action = std::time::Instant::now();
        let field_idx = match self.focused_field {
            Some(i) => i,
            None => {
                self.is_dragging_text = false;
                return;
            }
        };

        let val = self.get_field_text(field_idx, ai_config);
        let fields = vec![
            (230.0, 30.0),  // 0: Key
            (230.0, 130.0), // 1: URL
            (230.0, 230.0), // 2: Model
            (230.0, 330.0), // 3: Steps
            (405.0, 330.0), // 4: L1
            (580.0, 330.0), // 5: L2
            (230.0, 430.0), // 6: Interval
            (230.0, 530.0), // 7: Tavily
            (230.0, 630.0), // 8: Brave
            (230.0, 730.0), // 9: FC URL
            (230.0, 830.0), // 10: FC Key
            (230.0, 930.0), // 11: System
        ];

        let (fx, fy) = fields[field_idx];
        let design_card_y = 120.0;
        let input_y = design_card_y + fy + 25.0;
        let text_x = dlx - fx - 15.0;

        if !text_x.is_finite() || !dly.is_finite() {
            return;
        }

        if field_idx == 11 {
            // Multi-line cursor drag for System Prompt
            let text_y = dly - input_y - 12.0 - self.system_prompt_scroll_offset as f64;
            if text_y.is_finite() {
                self.cursor_pos = self.get_cursor_from_xy(&val, text_x, text_y, scale as f32);
            }
        } else {
            self.cursor_pos = self.get_cursor_from_x(&val, text_x, scale as f32);
        }
        self.window.request_redraw();
        self.last_cursor_action = std::time::Instant::now();
        self.is_dirty = true;
    }

    pub fn handle_mouse_up(&mut self) {
        if self.is_dragging_text {
            if let Some(start) = self.selection_start {
                if start == self.cursor_pos {
                    self.selection_start = None;
                }
            }
            self.is_dragging_text = false;
        }
        self.is_dragging_scrollbar = false;
        self.dragging_history_idx = None;
        self.dragging_sys_prompt = false;
        self.window.request_redraw();
    }
}
