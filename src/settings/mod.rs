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

    // Layout
    pub is_dragging_scrollbar: bool,
    pub last_size: (u32, u32),
    pub available_monitors: Vec<(String, String)>,
    pub current_monitor_name: Option<String>,
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
            is_dragging_scrollbar: false,
            available_monitors: event_loop
                .available_monitors()
                .map(|m| (m.name().unwrap_or_default(), m.name().unwrap_or_default()))
                .collect(),
            current_monitor_name: None,
            last_size: (800, 750),
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
        }

        let mut buffer = self.surface.buffer_mut().unwrap();
        buffer.fill(0);

        // Scaling (Target 800x750)
        let scale = (w as f32 / 800.0).min(h as f32 / 750.0);
        let off_x = (w as f32 - 800.0 * scale) / 2.0;
        let off_y = (h as f32 - 750.0 * scale) / 2.0;

        let sc = |val: f32| -> f32 { val * scale };
        let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
        let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };

        // 1. Background
        draw_rect(&mut buffer, w, 0, 0, w, h, COLOR_BG_APP, w, h);

        // 2. Sidebar
        draw_rect(
            &mut buffer,
            w,
            off_x as i32,
            off_y as i32,
            s(180) - off_x as u32,
            (750.0 * scale) as u32,
            COLOR_BG_SIDEBAR,
            w,
            h,
        );

        let menu_items = vec!["Home", "General", "AI", "History", "About"];
        for (i, item) in menu_items.iter().enumerate() {
            let ty = sy_val(120 + i as u32 * 60);
            let is_active = self.current_tab == i;

            if is_active {
                draw_rounded_rect(
                    &mut buffer,
                    w,
                    s(20) as i32,
                    ty as i32,
                    s(140) - s(20),
                    sc(45.0) as u32,
                    8,
                    COLOR_PRIMARY,
                    w,
                    h,
                );
            }

            draw_text(
                &mut buffer,
                w,
                &[],
                item,
                s(40) as i32,
                ty as i32 + sc(12.0) as i32,
                sc(16.0),
                if is_active {
                    0xFFFFFFFF
                } else {
                    COLOR_TEXT_SEC
                },
            );
        }

        // 3. Tab Content
        match self.current_tab {
            0 => {
                let (vh, ch) = tabs::home::draw(&mut buffer, w, h, scale, off_x, off_y);
                self.viewport_height = vh;
                self.content_height = ch;
            }
            1 => {
                let (vh, ch) = tabs::general::draw(
                    &mut buffer,
                    w,
                    h,
                    scale,
                    off_x,
                    off_y,
                    self.scroll_offset,
                    current_scale,
                    current_mode,
                    current_music_path,
                    current_layer,
                    &self.available_monitors,
                    self.current_monitor_name.as_ref(),
                );
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
                    active_sys_prompt_content_height: &mut self.active_sys_prompt_content_height,
                    active_sys_prompt_rect: &mut self.active_sys_prompt_rect,
                };
                let (vh, ch) = tabs::ai::draw(
                    &mut buffer,
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
            }
            3 => {
                let mut history_state = tabs::history::HistoryTabState {
                    history: &self.history,
                    history_scroll_states: &mut self.history_scroll_states,
                    history_item_rects: &mut self.history_item_rects,
                    scroll_offset: self.scroll_offset,
                };
                let (vh, ch) =
                    tabs::history::draw(&mut buffer, w, h, scale, off_x, off_y, &mut history_state);
                self.viewport_height = vh;
                self.content_height = ch;
            }
            4 => {
                let (vh, ch) = tabs::about::draw(&mut buffer, w, h, scale, off_x, off_y);
                self.viewport_height = vh;
                self.content_height = ch;
            }
            _ => {}
        }

        // Draw Tab Header
        let (title, sub) = match self.current_tab {
            0 => ("Home", "Welcome to Ameath!"),
            1 => ("Appearance", "Customize your pet's look"),
            2 => ("AI Brain", "Connect Ameath to the cloud"),
            3 => ("History", "Recent Local Memory (Last 50)"),
            _ => ("About", "Ameath v0.1.0"),
        };

        let header_h = sy_val(120);
        draw_rect(
            &mut buffer,
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
            &mut buffer,
            w,
            &[],
            title,
            s(220) as i32,
            sy_val(40) as i32,
            sc(32.0),
            COLOR_TEXT_MAIN,
        );
        draw_text(
            &mut buffer,
            w,
            &[],
            sub,
            s(220) as i32,
            sy_val(85) as i32,
            sc(16.0),
            COLOR_TEXT_SEC,
        );

        // Draw Global Scrollbar
        if self.content_height > self.viewport_height {
            let sb_w = sc(6.0) as u32;
            let sb_h = sc(600.0);
            let sb_x = s(785) as i32;
            let sb_y = sy_val(130);

            draw_rounded_rect(
                &mut buffer,
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
                &mut buffer,
                w,
                sb_x,
                hy as i32,
                sb_w,
                hh as u32,
                3,
                0x00CCCCCC,
                w,
                h,
            );
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

        let lx = (x - off_x) / scale;
        let ly = (y - off_y) / scale;

        // Sidebar
        if lx >= 0.0 && lx <= 180.0 {
            for i in 0..5 {
                let ty = 120.0 + i as f64 * 60.0;
                if ly >= ty && ly <= ty + 45.0 {
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
                let scroll_y = self.scroll_offset as f64 / scale;
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
                if lx >= 210.0
                    && lx <= 210.0 + card_w
                    && ly >= card5_y + 60.0
                    && ly <= card5_y + card5_h
                {
                    for (i, (name, _)) in self.available_monitors.iter().enumerate() {
                        let row = i / 3;
                        let col = i % 3;
                        let mx = 230.0 + col as f64 * 110.0;
                        let my = card5_y + 60.0 + row as f64 * 65.0;
                        if lx >= mx && lx <= mx + 100.0 && ly >= my && ly <= my + 55.0 {
                            return SettingsAction::SetMonitor(name.clone());
                        }
                    }
                }
            }
            2 => {
                // Tab 2: AI
                let scroll_y = self.scroll_offset as f64 / scale;
                let card_y = 120.0 + scroll_y;

                self.focused_field = None;
                self.selection_start = None;

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
                    let input_y = card_y + fy + 25.0;
                    let input_h = if i == 11 { 250.0 } else { 45.0 };

                    if lx >= *fx && lx <= *fx + *fw && ly >= input_y && ly <= input_y + input_h {
                        self.focused_field = Some(i);
                        self.last_cursor_action = std::time::Instant::now();

                        // The line `let (_, layout_h) = get_metrics_dw(&final_text, sc(14.0), max_width);` was not found in the original document.
                        // Assuming the instruction implies removing `max_width` from a similar call if it were present.
                        // Since it's not present, no change is made here regarding `max_width` in `get_metrics_dw`.

                        let text = self.get_field_text(i, ai_config);
                        if i == 11 {
                            // System prompt multi-line
                            let text_x = lx - fx - 15.0;
                            let text_y =
                                ly - input_y - 12.0 - self.system_prompt_scroll_offset as f64;
                            self.cursor_pos =
                                self.get_cursor_from_xy(&text, text_x, text_y, scale as f32);

                            if !_is_right_click {
                                self.selection_start = Some(self.cursor_pos);
                                self.is_dragging_text = true;
                            } else {
                                self.selection_start = None;
                            }
                        } else {
                            if _is_right_click {
                                // Right click handled in SettingsAction usually but we might want it here too
                            } else {
                                if lx >= *fx + *fw - 45.0 && (i == 0 || i == 7 || i == 8 || i == 10)
                                {
                                    self.show_api_key = !self.show_api_key;
                                    self.selection_start = None;
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

    fn set_field_text(&self, idx: usize, ai_config: &mut AiConfig, text: String) {
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
            11 => ai_config.system_prompt = text,
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
        let max_width = (540.0 - 80.0) * scale;
        get_cursor_index_from_xy(
            text,
            14.0 * scale,
            max_width as u32,
            (lx as f32 * scale).max(0.0),
            (ly as f32 * scale).max(0.0),
        )
    }

    fn get_xy_from_cursor(&self, text: &str, cursor_pos: usize, scale: f32) -> (f64, f64) {
        let max_width = (540.0 - 80.0) * scale;
        let (px, py) = get_xy_from_cursor_index(text, 14.0 * scale, max_width as u32, cursor_pos);
        (px as f64, py as f64)
    }

    pub fn handle_key_input(
        &mut self,
        event: &winit::event::KeyEvent,
        ai_config: &mut AiConfig,
        modifiers: winit::keyboard::ModifiersState,
    ) -> bool {
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
                let (lx, ly) = self.get_xy_from_cursor(&text, self.cursor_pos, 1.0);
                let line_height = 20.0;
                self.cursor_pos = self.get_cursor_from_xy(&text, lx, ly - line_height + 5.0, 1.0);
                if !has_shift {
                    self.selection_start = None;
                }
                self.window.request_redraw();
                return true;
            }
        }
        if let Key::Named(NamedKey::ArrowDown) = &event.logical_key {
            if field_idx == 11 {
                let (lx, ly) = self.get_xy_from_cursor(&text, self.cursor_pos, 1.0);
                let line_height = 20.0;
                self.cursor_pos = self.get_cursor_from_xy(&text, lx, ly + line_height + 5.0, 1.0);
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
            ai_config.save();
            self.last_cursor_action = std::time::Instant::now();
            self.window.request_redraw();
            return true;
        }
        false
    }

    pub fn handle_scroll(
        &mut self,
        dy: f32,
        cursor_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    ) {
        if self.current_tab == 3 {
            // History Tab
            let mut scrolled_item = false;
            if let Some(pos) = cursor_pos {
                let size = self.window.inner_size();
                let w = size.width as f64;
                let h = size.height as f64;
                let scale = (w / 800.0).min(h / 750.0);
                let off_x = (w - 800.0 * scale) / 2.0;
                let off_y = (h - 750.0 * scale) / 2.0;
                let lx = (pos.x - off_x) / scale;
                let ly = (pos.y - off_y) / scale;

                if self.history_item_rects.len() == self.history.len() {
                    for (i, (_x, ly_start, _w, ly_end)) in
                        self.history_item_rects.iter().enumerate()
                    {
                        let y_start = *ly_start + self.scroll_offset as f64 / scale;
                        let y_end = *ly_end + self.scroll_offset as f64 / scale;

                        if lx >= 230.0 && lx <= 720.0 && ly >= y_start && ly <= y_end {
                            let content = &self.history[i].1;
                            let item_h_fixed_sc = 180.0 * scale as f32;
                            let max_width = (450.0 * scale) as u32;
                            let (_, full_h) =
                                get_metrics_dw(content, 16.0 * scale as f32, max_width);
                            let full_h = full_h.max(20.0 * scale as f32);
                            let view_h = item_h_fixed_sc - (40.0 * scale as f32);

                            if full_h > view_h {
                                let current_log = self.history_scroll_states[i];
                                let scroll_step_log = dy / scale as f32;
                                let new_val_log = current_log + scroll_step_log;
                                let max_scroll_log = -((full_h - view_h) / scale as f32);
                                let clamped_log = new_val_log.clamp(max_scroll_log, 0.0);

                                if (clamped_log - current_log).abs() > 0.001 {
                                    self.history_scroll_states[i] = clamped_log;
                                    scrolled_item = true;
                                } else {
                                    if (dy > 0.0 && current_log >= -0.01)
                                        || (dy < 0.0 && current_log <= max_scroll_log + 0.01)
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
            }

            if !scrolled_item {
                self.scroll_offset += dy;
                let min_offset = -(self.content_height - self.viewport_height).max(0.0);
                self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
            }
        } else if self.current_tab == 2 {
            let mut scrolled_sys_prompt = false;
            if let Some((min_x, min_y, max_x, max_y)) = self.active_sys_prompt_rect {
                if let Some(pos) = cursor_pos {
                    let size = self.window.inner_size();
                    let w = size.width as f64;
                    let h = size.height as f64;
                    let scale = (w / 800.0).min(h / 750.0);
                    let off_x = (w - 800.0 * scale) / 2.0;
                    let off_y = (h - 750.0 * scale) / 2.0;
                    let lx = (pos.x - off_x) / scale;
                    let ly = (pos.y - off_y) / scale;

                    if lx >= min_x && lx <= max_x && ly >= min_y && ly <= max_y {
                        let old_off = self.system_prompt_scroll_offset;
                        self.system_prompt_scroll_offset += dy / scale as f32;
                        let view_h = 250.0; // Updated from 200.0 to 250.0
                        let content_h = self.active_sys_prompt_content_height;
                        let min_offset = -(content_h - view_h).max(0.0);
                        self.system_prompt_scroll_offset =
                            self.system_prompt_scroll_offset.clamp(min_offset, 0.0);

                        // If we reached the boundary, allow the global scroll to take over
                        if (self.system_prompt_scroll_offset - old_off).abs() > 0.1 {
                            scrolled_sys_prompt = true;
                        } else {
                            // Already at boundary: only eat scroll if we're trying to scroll FURTHER into the boundary
                            if (dy > 0.0 && self.system_prompt_scroll_offset >= -0.1)
                                || (dy < 0.0
                                    && self.system_prompt_scroll_offset <= min_offset + 0.1)
                            {
                                scrolled_sys_prompt = false;
                            } else {
                                scrolled_sys_prompt = true;
                            }
                        }
                    }
                }
            }

            if !scrolled_sys_prompt {
                self.scroll_offset += dy;
                let min_offset = -(self.content_height - self.viewport_height).max(0.0);
                self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
            }
        } else {
            self.scroll_offset += dy;
            let min_offset = -(self.content_height - self.viewport_height).max(0.0);
            self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
        }
        self.window.request_redraw();
    }

    pub fn handle_mouse_move(&mut self, x: f64, y: f64, ai_config: &AiConfig) {
        let size = self.window.inner_size();
        let w = size.width as f64;
        let h = size.height as f64;
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let off_y = (h - 750.0 * scale) / 2.0;
        let lx = (x - off_x) / scale;
        let ly = (y - off_y) / scale;

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
            (230.0, 30.0),  // Key
            (230.0, 130.0), // URL
            (230.0, 230.0), // Model
            (230.0, 330.0), // Steps
            (405.0, 330.0), // L1
            (580.0, 330.0), // L2
            (230.0, 530.0), // Tavily
            (230.0, 630.0), // System
            (230.0, 430.0), // Interaction Frequency
        ];

        let (fx, fy) = fields[field_idx];
        let scroll_y = self.scroll_offset as f64 / scale;
        let card_y = 120.0 + scroll_y;
        let input_y = card_y + fy + 25.0;
        let text_x = lx - fx - 15.0;

        if field_idx == 7 {
            // Multi-line cursor drag
            let text_y = ly - input_y - 12.0 - self.system_prompt_scroll_offset as f64;
            self.cursor_pos = self.get_cursor_from_xy(&val, text_x, text_y, scale as f32);
        } else {
            self.cursor_pos = self.get_cursor_from_x(&val, text_x, scale as f32);
        }
        self.window.request_redraw();
        self.last_cursor_action = std::time::Instant::now();
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
        self.window.request_redraw();
    }
}
