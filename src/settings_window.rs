use rusttype::{point, Font, Scale};
use softbuffer::{Context, Surface};
use std::fs;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::{event_loop::EventLoopWindowTarget, window::Window};

pub struct SettingsWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    font: Option<Font<'static>>,
    current_tab: usize,               // 0: Home, 1: General, 2: AI, 3: About
    pub focused_field: Option<usize>, // AI Tab focus: 0: Key, 1: URL, 2: Model, 3: ReAct, 4: L1 Thr, 5: L2 Thr
    show_api_key: bool,
    pub history: Vec<(String, String)>,
    pub scroll_offset: f32,
    pub history_scroll_states: Vec<f32>, // Per-item scroll offset
    pub content_height: f32,
    pub viewport_height: f32,

    pub system_prompt_scroll_offset: f32,
    pub active_sys_prompt_rect: Option<(f64, f64, f64, f64)>, // lx_min, ly_min, lx_max, ly_max
    pub active_sys_prompt_content_height: f32,
    pub available_monitors: Vec<(String, String)>,
    pub current_monitor_name: Option<String>,
    pub history_item_rects: Vec<(f64, f64, f64, f64)>, // Logical rects: x, y, w, h
    pub cursor_pos: usize,
    pub selection_start: Option<usize>,
    pub is_dragging_text: bool,
    pub last_cursor_action: std::time::Instant,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsAction {
    None,
    SetScale(f32),
    SetMode(crate::types::BehaviorMode),
    SetMusicPath(std::path::PathBuf),
    SetLayer(crate::types::WindowLayer),
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
}

impl SettingsWindow {
    pub fn handle_click(
        &mut self,
        x: f64,
        y: f64,
        is_right_click: bool,
        ai_config: &crate::types::AiConfig,
    ) -> SettingsAction {
        let size = self.window.inner_size();
        let w = size.width as f64;
        let h = size.height as f64;

        // Unified scaling and centering (Target: 800x750)
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let off_y = (h - 750.0 * scale) / 2.0;

        // Map screen coordinates to logical 800x750 coordinates
        let lx = (x - off_x) / scale;
        let ly = (y - off_y) / scale;

        // Sidebar Tab Selection
        if lx >= 0.0 && lx < 180.0 {
            for i in 0..5 {
                let my_min = 160.0 + i as f64 * 70.0 - 25.0;
                let my_max = 160.0 + i as f64 * 70.0 + 25.0;
                if ly >= my_min && ly <= my_max {
                    self.current_tab = i;
                    self.focused_field = None;
                    self.scroll_offset = 0.0;
                    self.cursor_pos = 0;
                    self.selection_start = None;
                    self.last_cursor_action = std::time::Instant::now(); // Reset cursor blink on tab change
                    self.window.request_redraw();
                    if i == 3 {
                        return SettingsAction::RequestHistory;
                    }
                    return SettingsAction::None;
                }
            }
        }

        // Ignore clicks in header area for content tabs (1, 2, 3)
        // Header is approx 120 logical pixels high
        if ly < 120.0 {
            return SettingsAction::None;
        }

        if self.current_tab == 3 {
            // History Tab Click Handling
            if lx > 0.0 && ly > 140.0 {
                // Use cached logical rects from Redraw (Guaranteed to be full list now)
                if self.history_item_rects.len() == self.history.len() {
                    for (_i, (_x, ly_start, _w, ly_end)) in
                        self.history_item_rects.iter().enumerate()
                    {
                        let y_start = *ly_start + self.scroll_offset as f64 / scale;
                        let y_end = *ly_end + self.scroll_offset as f64 / scale;

                        if ly >= y_start && ly <= y_end {
                            // Clicked history item i
                            // No action needed for fixed cards currently
                            // Maybe future: copy to clipboard?
                            return SettingsAction::None;
                        }
                    }
                } else {
                    // Cache mismatch or empty
                }
            }
            return SettingsAction::None;
        }

        if self.current_tab == 1 {
            // General Tab (Appearance)
            let scroll_y = self.scroll_offset as f64 / scale;
            let sly = ly - scroll_y; // Scrolled logical Y

            // Pet Scale Buttons (Visual: card1_y=120, buttons at 180)
            if sly >= 180.0 && sly <= 230.0 {
                let start_x = 220.0;
                let btn_w = 75.0;
                let gap = 10.0;
                let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
                for (i, &s) in scales.iter().enumerate() {
                    let btn_x = start_x + i as f64 * (btn_w + gap);
                    if lx >= btn_x && lx <= btn_x + btn_w {
                        return SettingsAction::SetScale(s);
                    }
                }
            }

            // Mode Buttons (Visual: card2_y=280, buttons at 340)
            if sly >= 340.0 && sly <= 400.0 {
                let btn_w = 150.0;
                let gap = 15.0;
                let start_x = 230.0;
                if lx >= start_x && lx <= start_x + btn_w {
                    return SettingsAction::SetMode(crate::types::BehaviorMode::Quiet);
                }
                let active_x = start_x + btn_w + gap;
                if lx >= active_x && lx <= active_x + btn_w {
                    return SettingsAction::SetMode(crate::types::BehaviorMode::Active);
                }
                let clingy_x = active_x + btn_w + gap;
                if lx >= clingy_x && lx <= clingy_x + btn_w {
                    return SettingsAction::SetMode(crate::types::BehaviorMode::Clingy);
                }
            }

            // Music Path Button (Visual: card3_y=440, buttons at 500)
            if sly >= 500.0 && sly <= 550.0 {
                let start_x = 230.0;
                let btn_w = 500.0;
                if lx >= start_x && lx <= start_x + btn_w {
                    // Temporarily disable AlwaysOnTop so the native dialog isn't hidden
                    self.window
                        .set_window_level(winit::window::WindowLevel::Normal);
                    let picked = rfd::FileDialog::new().pick_folder();
                    self.window
                        .set_window_level(winit::window::WindowLevel::AlwaysOnTop);

                    if let Some(path) = picked {
                        return SettingsAction::SetMusicPath(path);
                    }
                }
            }

            // Layer Buttons (Visual: card4_y=600, buttons at 660)
            if sly >= 660.0 && sly <= 720.0 {
                let btn_w = 150.0;
                let gap = 15.0;
                let start_x = 230.0;
                if lx >= start_x && lx <= start_x + btn_w {
                    return SettingsAction::SetLayer(crate::types::WindowLayer::Top);
                }
                let bottom_x = start_x + btn_w + gap;
                if lx >= bottom_x && lx <= bottom_x + btn_w {
                    return SettingsAction::SetLayer(crate::types::WindowLayer::Bottom);
                }
            }

            // Monitor Selection (Visual: card5_y=760, buttons at 820)
            // Dynamic height based on rows
            let rows = (self.available_monitors.len() + 2) / 3;
            let monitors_h = if rows > 0 { rows as f64 * 65.0 } else { 65.0 };

            if sly >= 820.0 && sly <= 820.0 + monitors_h {
                for (i, (name, _)) in self.available_monitors.iter().enumerate() {
                    let row = i / 3;
                    let col = i % 3;
                    // mx = 230 + col * 110
                    // my = 820 + row * 65
                    let btn_x = 230.0 + col as f64 * 110.0;
                    let btn_y = 820.0 + row as f64 * 65.0;
                    let btn_w = 100.0;

                    if lx >= btn_x && lx <= btn_x + btn_w && sly >= btn_y && sly <= btn_y + 55.0 {
                        return SettingsAction::SetMonitor(name.clone());
                    }
                }
            }
        } else if self.current_tab == 2 {
            // AI Tab
            let ai_y_start = 120.0;
            // Apply scroll offset to click check
            let scroll_y = self.scroll_offset as f64 / scale;
            println!(
                "AI Tab Click - ScrollOffset: {}, Scale: {}, CalcScrollY: {}, LY: {}",
                self.scroll_offset, scale, scroll_y, ly
            );

            let mut found_field = false;
            for i in 0..9 {
                let (fx, fy, fw) = match i {
                    0 => (230.0, 30.0, 500.0),
                    1 => (230.0, 130.0, 500.0),
                    2 => (230.0, 230.0, 500.0),
                    3 => (230.0, 330.0, 150.0), // Smaller numeric fields
                    4 => (405.0, 330.0, 150.0),
                    5 => (580.0, 330.0, 150.0),
                    8 => (230.0, 430.0, 150.0), // Interaction Frequency
                    6 => (230.0, 530.0, 500.0), // Tavily Key
                    7 => (230.0, 630.0, 500.0), // System Prompt
                    _ => (0.0, 0.0, 0.0),
                };

                // Match the drawing logic:
                // let fy_scaled = card_y + sc(fy) as u32;
                // card_y = sy_val(120) + scroll_y
                // So effective Y is 120 + fy + scroll_y (in unscaled coords)

                let effective_y = ai_y_start + fy + scroll_y;
                let input_y = effective_y + 25.0; // Label is at effective_y, input is +25
                let input_h = if i == 7 { 200.0 } else { 45.0 };

                if lx >= fx && lx <= fx + fw && ly >= input_y && ly <= input_y + input_h {
                    println!(
                        "Click Hit! Field: {}, LY: {}, InputY: {}, H: {}",
                        i, ly, input_y, input_h
                    );
                    found_field = true;
                    if is_right_click {
                        match i {
                            0 => return SettingsAction::SetAiApiKey("".to_string()),
                            1 => return SettingsAction::SetAiBaseUrl("".to_string()),
                            2 => return SettingsAction::SetAiModel("".to_string()),
                            3 => return SettingsAction::SetAiReactLimit(20),
                            4 => return SettingsAction::SetAiL1Threshold(10),
                            5 => return SettingsAction::SetAiL2Threshold(10),
                            8 => return SettingsAction::SetAiInteractionFrequency(20),
                            6 => return SettingsAction::SetAiTavilyKey("".to_string()),
                            7 => return SettingsAction::SetAiSystemPrompt("".to_string()),
                            _ => {}
                        }
                    } else {
                        if (i == 0 || i == 6) && lx >= fx + fw - 45.0 && ly <= input_y + 45.0 {
                            // Check for eye icon click for API Key and Tavily Key
                            self.show_api_key = !self.show_api_key;
                            self.window.request_redraw();
                            return SettingsAction::None;
                        }
                        self.focused_field = Some(i);
                        // Coordinate to cursor logic
                        let val = match i {
                            0 => ai_config.api_key.clone(),
                            1 => ai_config.base_url.clone(),
                            2 => ai_config.model.clone(),
                            3 => ai_config.react_limit.to_string(),
                            4 => ai_config.l1_summary_threshold.to_string(),
                            5 => ai_config.l2_merge_threshold.to_string(),
                            6 => ai_config.tavily_api_key.clone(),
                            7 => ai_config.system_prompt.clone(),
                            8 => ai_config.interaction_frequency.to_string(),
                            _ => String::new(),
                        };

                        if i == 7 {
                            // Multi-line system prompt needs more complex logic, for now place at end
                            self.cursor_pos = val.chars().count();
                        } else {
                            let text_x = lx - fx - 15.0; // 15.0 is horizontal padding
                            self.cursor_pos = self.get_cursor_from_x(&val, text_x, 1.0);
                        }

                        if !is_right_click {
                            self.selection_start = Some(self.cursor_pos);
                            self.is_dragging_text = true;
                        } else {
                            self.selection_start = None;
                            self.is_dragging_text = false;
                        }
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                }
            }

            if !found_field {
                self.focused_field = None;
                self.cursor_pos = 0;
                self.selection_start = None;
                self.window.request_redraw();
            }
        }

        SettingsAction::None
    }

    pub fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let window = Rc::new(
            winit::window::WindowBuilder::new()
                .with_title("Ameath Settings")
                .with_inner_size(winit::dpi::LogicalSize::new(800, 750))
                .with_resizable(true)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                .build(event_loop)
                .unwrap(),
        );
        window.set_ime_allowed(true); // Enable IME support explicitly

        let context = Context::new(window.clone()).unwrap();
        let surface = Surface::new(&context, window.clone()).unwrap();

        // Load Font (Microsoft YaHei)
        let font_path = "C:\\Windows\\Fonts\\msyh.ttc";
        let font = fs::read(font_path)
            .ok()
            .and_then(|data| Font::try_from_vec(data));

        if font.is_none() {
            eprintln!("Failed to load font from {}", font_path);
        }

        Self {
            window,
            context,
            surface,
            font,
            current_tab: 1, // Default to General (Appearance)
            focused_field: None,
            show_api_key: false,
            history: Vec::new(),
            scroll_offset: 0.0,
            history_scroll_states: Vec::new(),
            content_height: 0.0,
            viewport_height: 0.0,

            system_prompt_scroll_offset: 0.0,
            active_sys_prompt_rect: None,
            active_sys_prompt_content_height: 0.0,
            available_monitors: event_loop
                .available_monitors()
                .map(|m| (m.name().unwrap_or_default(), m.name().unwrap_or_default()))
                .collect(),
            current_monitor_name: None, // Will be set by main.rs on startup or selection
            history_item_rects: Vec::new(),
            cursor_pos: 0,
            selection_start: None,
            is_dragging_text: false,
            last_cursor_action: std::time::Instant::now(),
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        self.window.focus_window();
    }

    #[allow(dead_code)]
    pub fn window(&self) -> &Rc<Window> {
        &self.window
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    fn get_field_text(&self, idx: usize, ai_config: &crate::types::AiConfig) -> String {
        match idx {
            0 => ai_config.api_key.clone(),
            1 => ai_config.base_url.clone(),
            2 => ai_config.model.clone(),
            3 => ai_config.react_limit.to_string(),
            4 => ai_config.l1_summary_threshold.to_string(),
            5 => ai_config.l2_merge_threshold.to_string(),
            6 => ai_config.tavily_api_key.clone(),
            7 => ai_config.system_prompt.clone(),
            8 => ai_config.interaction_frequency.to_string(),
            _ => String::new(),
        }
    }

    fn set_field_text(&self, idx: usize, ai_config: &mut crate::types::AiConfig, text: String) {
        match idx {
            0 => ai_config.api_key = text,
            1 => ai_config.base_url = text,
            2 => ai_config.model = text,
            3 => ai_config.react_limit = text.parse().unwrap_or(0),
            4 => ai_config.l1_summary_threshold = text.parse().unwrap_or(0),
            5 => ai_config.l2_merge_threshold = text.parse().unwrap_or(0),
            6 => ai_config.tavily_api_key = text,
            7 => ai_config.system_prompt = text,
            8 => ai_config.interaction_frequency = text.parse().unwrap_or(0),
            _ => {}
        }
    }

    fn get_cursor_from_x(&self, text: &str, x_offset: f64, scale: f32) -> usize {
        if let Some(font) = &self.font {
            let sc = scale;
            let display_chars: Vec<char> = text.chars().collect();
            let mut best_idx = 0;
            let mut min_diff = x_offset.abs();

            for i in 1..=display_chars.len() {
                let sub: String = display_chars[..i].iter().collect();
                let glyphs = font
                    .layout(&sub, Scale::uniform(sc * 14.0), point(0.0, 0.0))
                    .collect::<Vec<_>>();
                let w = glyph_width(&glyphs) as f64;
                let diff = (x_offset - w).abs();
                if diff < min_diff {
                    min_diff = diff;
                    best_idx = i;
                }
            }
            best_idx
        } else {
            0
        }
    }

    pub fn handle_mouse_move(&mut self, x: f64, _y: f64, ai_config: &crate::types::AiConfig) {
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

        let size = self.window.inner_size();
        let w = size.width as f64;
        let h = size.height as f64;
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let lx = (x - off_x) / scale;

        let (fx, _, _) = match field_idx {
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

        let val = self.get_field_text(field_idx, ai_config);
        if field_idx == 7 {
            // Multi-line skip for now
        } else {
            let text_x = lx - fx - 15.0;
            self.cursor_pos = self.get_cursor_from_x(&val, text_x, 1.0);
            self.window.request_redraw();
        }
    }

    pub fn handle_mouse_up(&mut self) {
        if self.is_dragging_text {
            if let Some(start) = self.selection_start {
                if start == self.cursor_pos {
                    self.selection_start = None;
                }
            }
            self.is_dragging_text = false;
            self.window.request_redraw();
        }
    }

    pub fn handle_key_input(
        &mut self,
        event: &winit::event::KeyEvent,
        ai_config: &mut crate::types::AiConfig,
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

        use winit::keyboard::{Key, NamedKey};
        let is_pressed = event.state == winit::event::ElementState::Pressed;
        if !is_pressed {
            return false;
        }

        let has_ctrl = modifiers.control_key() || modifiers.super_key();
        let has_shift = modifiers.shift_key();

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
                    } else if self.cursor_pos > 0 {
                        chars.remove(self.cursor_pos - 1);
                        self.cursor_pos -= 1;
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
                    } else if self.cursor_pos < chars.len() {
                        chars.remove(self.cursor_pos);
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
                    // Numeric check
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

    fn draw_rect(
        buffer: &mut [u32],
        surface_w: u32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        color: u32,
        max_w: u32,
        max_h: u32,
    ) {
        let start_x = x.max(0);
        let start_y = y.max(0);
        let max_x = (x + width as i32).min(max_w as i32);
        let max_y = (y + height as i32).min(max_h as i32);

        if start_x >= max_x || start_y >= max_y {
            return;
        }

        for cy in start_y..max_y {
            for cx in start_x..max_x {
                let idx = (cy * surface_w as i32 + cx) as usize;
                if idx < buffer.len() {
                    buffer[idx] = color;
                }
            }
        }
    }

    fn draw_rounded_rect(
        buffer: &mut [u32],
        surface_w: u32,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        radius: u32,
        color: u32,
        max_w: u32,
        max_h: u32,
    ) {
        let start_x = x.max(0);
        let start_y = y.max(0);
        let max_x = (x + width as i32).min(max_w as i32);
        let max_y = (y + height as i32).min(max_h as i32);

        if start_x >= max_x || start_y >= max_y {
            return;
        }

        let r_i32 = radius as i32;
        let w_i32 = width as i32;
        let h_i32 = height as i32;
        let r_sq = r_i32 * r_i32;

        for cy in start_y..max_y {
            for cx in start_x..max_x {
                let mut in_corner = false;
                let mut dx = 0;
                let mut dy = 0;

                if cx < x + r_i32 && cy < y + r_i32 {
                    dx = (x + r_i32) - cx;
                    dy = (y + r_i32) - cy;
                    in_corner = true;
                } else if cx >= x + w_i32 - r_i32 && cy < y + r_i32 {
                    dx = cx - (x + w_i32 - r_i32);
                    dy = (y + r_i32) - cy;
                    in_corner = true;
                } else if cx < x + r_i32 && cy >= y + h_i32 - r_i32 {
                    dx = (x + r_i32) - cx;
                    dy = cy - (y + h_i32 - r_i32);
                    in_corner = true;
                } else if cx >= x + w_i32 - r_i32 && cy >= y + h_i32 - r_i32 {
                    dx = cx - (x + w_i32 - r_i32);
                    dy = cy - (y + h_i32 - r_i32);
                    in_corner = true;
                }

                if in_corner && dx * dx + dy * dy > r_sq {
                    continue;
                }

                let idx = (cy * surface_w as i32 + cx) as usize;
                if idx < buffer.len() {
                    buffer[idx] = color;
                }
            }
        }
    }

    fn draw_text(
        buffer: &mut [u32],
        surface_w: u32,
        font: &Font,
        text: &str,
        x: i32,
        y: i32,
        scale: f32,
        color: u32,
    ) {
        let scale = Scale::uniform(scale);
        let v_metrics = font.v_metrics(scale);

        // Pre-calculate foreground color components
        let fg_r = ((color >> 16) & 0xFF) as f32;
        let fg_g = ((color >> 8) & 0xFF) as f32;
        let fg_b = (color & 0xFF) as f32;

        for glyph in font.layout(text, scale, point(x as f32, y as f32 + v_metrics.ascent)) {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, v| {
                    let px_i = gx as i32 + bounding_box.min.x;
                    let py_i = gy as i32 + bounding_box.min.y;

                    if px_i >= 0 && px_i < surface_w as i32 && py_i >= 0 {
                        let px = px_i as u32;
                        let py = py_i as u32;
                        let idx = (py * surface_w + px) as usize;
                        if idx < buffer.len() {
                            let alpha = v;
                            if alpha > 0.0 {
                                let bg = buffer[idx];
                                let r = ((bg >> 16) & 0xFF) as f32;
                                let g = ((bg >> 8) & 0xFF) as f32;
                                let b = (bg & 0xFF) as f32;

                                // Blend
                                let out_r = r * (1.0 - alpha) + fg_r * alpha;
                                let out_g = g * (1.0 - alpha) + fg_g * alpha;
                                let out_b = b * (1.0 - alpha) + fg_b * alpha;

                                buffer[idx] =
                                    ((out_r as u32) << 16) | ((out_g as u32) << 8) | (out_b as u32);
                            }
                        }
                    }
                });
            }
        }
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
        if let Some(width) = NonZeroU32::new(size.width) {
            if let Some(height) = NonZeroU32::new(size.height) {
                let _ = self.surface.resize(width, height);
                let mut buffer = self.surface.buffer_mut().unwrap();
                let w = width.get();
                let h = height.get();

                // 1. Background
                let bg_color = 0x00F6F7F9;
                buffer.fill(bg_color);

                // Unified Scaling (Target: 800x750)
                let scale = (w as f32 / 800.0).min(h as f32 / 750.0);
                let off_x = (w as f32 - 800.0 * scale) / 2.0;
                let off_y = (h as f32 - 750.0 * scale) / 2.0;

                let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
                let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };
                let sc = |val: f32| -> f32 { val * scale };

                let card_bg = 0x00FFFFFF;
                let primary = 0x00FB7299;
                let text_main = 0x0018191C;
                let text_sec = 0x009499A0;

                // 2. Sidebar
                let sb_w = (180.0 * scale) as u32;
                Self::draw_rect(
                    &mut buffer,
                    w,
                    off_x as i32,
                    off_y as i32,
                    sb_w,
                    (750.0 * scale) as u32,
                    0xFFFFFFFF,
                    w,
                    h,
                );

                if let Some(font) = &self.font {
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        "Ame",
                        s(40) as i32,
                        sy_val(40) as i32,
                        sc(32.0),
                        primary,
                    );
                    let menu_items = vec!["Home", "General", "AI", "History", "About"];
                    for (i, item) in menu_items.iter().enumerate() {
                        let my = sy_val(160 + i as u32 * 70);
                        let is_active = i == self.current_tab;
                        let col = if is_active { primary } else { text_sec };
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            item,
                            s(40) as i32,
                            my as i32,
                            sc(20.0),
                            col,
                        );
                        if is_active {
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                (off_x) as i32,
                                my as i32 - sc(8.0) as i32,
                                sc(6.0) as u32,
                                sc(36.0) as u32,
                                sc(3.0) as u32,
                                primary,
                                w,
                                h,
                            );
                        }
                    }
                }

                // 3. Tab Content

                if self.current_tab == 3 {
                    // History Tab
                    if let Some(font) = &self.font {
                        let start_y = sy_val(140);

                        let mut current_y = start_y as f32 + self.scroll_offset;
                        let mut calculated_content_height = 0.0;

                        // Reset geometry fields
                        // Reset geometry fields
                        self.history_item_rects.clear();

                        // Resize scroll states if needed
                        if self.history_scroll_states.len() != self.history.len() {
                            self.history_scroll_states.resize(self.history.len(), 0.0);
                        }

                        let item_h_fixed = sc(180.0);

                        for (i, (role, content)) in self.history.iter().enumerate() {
                            // Store Logical Rect (Simple fixed height)
                            let logical_y = 140.0 + (i as f64 * 190.0); // 180 + 10 gap
                            let logical_h = 180.0;

                            self.history_item_rects.push((
                                230.0,
                                logical_y,
                                730.0,
                                logical_y + logical_h,
                            ));

                            let y_pos = current_y;
                            calculated_content_height += item_h_fixed + sc(10.0); // + gap
                            current_y += item_h_fixed + sc(10.0);

                            // Screen Bounds for Culling
                            let min_y = sy_val(120) as f32;
                            let max_y = h as f32;

                            if (y_pos + item_h_fixed) < min_y || y_pos > max_y {
                                continue;
                            }

                            let y_pos_i = y_pos as i32;

                            // Draw Card Background
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                s(230) as i32,
                                y_pos_i,
                                s(490),
                                item_h_fixed as u32,
                                8,
                                0xFFFFFFFF, // White card
                                w,
                                h,
                            );

                            // Draw Role Label
                            let role_col = if role == "user" {
                                0x00007ACC
                            } else {
                                0x002E8B57
                            };
                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                role,
                                s(240) as i32,
                                y_pos_i + sc(10.0) as i32,
                                sc(14.0),
                                role_col,
                            );

                            // Draw Content with Internal Scroll
                            let max_width = sc(450.0) as u32; // Allow space for scrollbar
                            let lines = wrap_text(
                                content,
                                font,
                                rusttype::Scale::uniform(sc(16.0)),
                                max_width,
                            );
                            let full_content_h = (lines.len() as f32 * sc(20.0)).max(sc(20.0));
                            let view_h = item_h_fixed - sc(40.0); // - top label/padding

                            let scroll = self.history_scroll_states[i];
                            let start_text_y = y_pos + sc(35.0);
                            let end_text_y = y_pos + item_h_fixed - sc(10.0);

                            for (li, line) in lines.iter().enumerate() {
                                let line_y =
                                    start_text_y + (li as f32 * sc(20.0)) + (scroll * scale);

                                // Clip to card content area
                                if line_y < start_text_y - sc(5.0) {
                                    continue;
                                }
                                if line_y > end_text_y - sc(15.0) {
                                    break;
                                }

                                // Clip to screen
                                if line_y < 0.0 || line_y > h as f32 {
                                    continue;
                                }

                                Self::draw_text(
                                    &mut buffer,
                                    w,
                                    font,
                                    line,
                                    s(240) as i32,
                                    line_y as i32,
                                    sc(16.0),
                                    text_main,
                                );
                            }

                            // Draw Scrollbar if needed
                            if full_content_h > view_h {
                                let sb_w = sc(4.0) as u32;
                                let sb_h = view_h;
                                let sb_x = s(230 + 480);
                                let sb_y_raw = start_text_y;

                                // Clip scrollbar container
                                if sb_y_raw + sb_h > 0.0 && sb_y_raw < h as f32 {
                                    // Draw track
                                    // Draw track
                                    Self::draw_rect(
                                        &mut buffer,
                                        w,
                                        sb_x as i32,
                                        sb_y_raw as i32,
                                        sb_w,
                                        sb_h as u32,
                                        0x00E3E5E7,
                                        w,
                                        h,
                                    );

                                    // Handle
                                    let ratio = view_h / full_content_h;
                                    let handle_h = (view_h * ratio).max(sc(20.0));
                                    let max_scroll = -(full_content_h - view_h);
                                    let progress = if max_scroll.abs() < 1.0 {
                                        0.0
                                    } else {
                                        scroll * scale / max_scroll
                                    };
                                    // scroll is negative, max_scroll is negative. progress 0..1

                                    let handle_y_rel = (view_h - handle_h) * progress;
                                    let handle_y = sb_y_raw + handle_y_rel;

                                    Self::draw_rect(
                                        &mut buffer,
                                        w,
                                        sb_x as i32,
                                        handle_y as i32,
                                        sb_w,
                                        handle_h as u32,
                                        0x00A0A0A0,
                                        w,
                                        h,
                                    );
                                }
                            }
                        }
                        self.content_height = calculated_content_height + sc(150.0); // Add bottom padding
                        self.viewport_height = (h as f32 - start_y as f32).max(0.0);
                    }
                }

                if self.current_tab == 1 {
                    // --- General Tab ---
                    let scroll_y = self.scroll_offset;
                    let card_w = (560.0 * scale) as u32;
                    let card1_y = (sy_val(120) as f32 + scroll_y) as i32;

                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210) as i32,
                        card1_y,
                        card_w,
                        (140.0 * scale) as u32,
                        12,
                        card_bg,
                        w,
                        h,
                    );
                    if let Some(font) = &self.font {
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Pet Scale",
                            s(230) as i32,
                            card1_y + sc(20.0) as i32,
                            sc(18.0),
                            text_main,
                        );
                    }
                    let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
                    let labels = vec!["0.5x", "0.75x", "1.0x", "1.25x", "1.5x"];
                    for (i, &val) in scales.iter().enumerate() {
                        let mx = s(220 + i as u32 * 85) as i32;
                        let my = card1_y + sc(60.0) as i32;
                        let is_active = (current_scale - val).abs() < 0.01;
                        let bg_col = if is_active { primary } else { 0x00F1F2F3 };
                        let text_col = if is_active { 0x00FFFFFF } else { text_main };
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            mx,
                            my,
                            sc(75.0) as u32,
                            sc(45.0) as u32,
                            8,
                            bg_col,
                            w,
                            h,
                        );
                        if let Some(font) = &self.font {
                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                labels[i],
                                mx + sc(12.0) as i32,
                                my + sc(12.0) as i32,
                                sc(14.0),
                                text_col,
                            );
                        }
                    }

                    let card2_y = (sy_val(280) as f32 + scroll_y) as i32;
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210) as i32,
                        card2_y,
                        card_w,
                        (140.0 * scale) as u32,
                        12,
                        card_bg,
                        w,
                        h,
                    );
                    if let Some(font) = &self.font {
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Behavior Mode",
                            s(230) as i32,
                            card2_y + sc(20.0) as i32,
                            sc(18.0),
                            text_main,
                        );
                        let modes = vec!["Quiet", "Active", "Clingy"];
                        for (i, mode) in modes.iter().enumerate() {
                            let mx = s(230 + i as u32 * 165) as i32;
                            let my = card2_y + sc(60.0) as i32;
                            let is_active = *mode == current_mode;
                            let b_col = if is_active { primary } else { 0x00E3E5E7 };
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx,
                                my,
                                sc(150.0) as u32,
                                sc(55.0) as u32,
                                8,
                                b_col,
                                w,
                                h,
                            );
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx + 2,
                                my + 2,
                                sc(150.0) as u32 - 4,
                                sc(55.0) as u32 - 4,
                                6,
                                card_bg,
                                w,
                                h,
                            );
                            if is_active {
                                Self::draw_rounded_rect(
                                    &mut buffer,
                                    w,
                                    mx + sc(125.0) as i32,
                                    my + 6,
                                    14,
                                    14,
                                    7,
                                    primary,
                                    w,
                                    h,
                                );
                            }
                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                mode,
                                mx + sc(25.0) as i32,
                                my + sc(18.0) as i32,
                                sc(15.0),
                                if is_active { primary } else { text_sec },
                            );
                        }
                    }

                    let card3_y = (sy_val(440) as f32 + scroll_y) as i32;
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210) as i32,
                        card3_y,
                        card_w,
                        (140.0 * scale) as u32,
                        12,
                        card_bg,
                        w,
                        h,
                    );
                    if let Some(font) = &self.font {
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Music Directory",
                            s(230) as i32,
                            card3_y + sc(20.0) as i32,
                            sc(18.0),
                            text_main,
                        );
                        let p_btn_y = card3_y + sc(60.0) as i32;
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            s(230) as i32,
                            p_btn_y,
                            sc(500.0) as u32,
                            sc(45.0) as u32,
                            8,
                            0x00E3E5E7,
                            w,
                            h,
                        );
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            s(230) as i32 + 1,
                            p_btn_y + 1,
                            sc(500.0) as u32 - 2,
                            sc(45.0) as u32 - 2,
                            7,
                            card_bg,
                            w,
                            h,
                        );
                        let path = current_music_path
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Click to select...".to_string());
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            &path,
                            s(245) as i32,
                            p_btn_y + sc(12.0) as i32,
                            sc(14.0),
                            text_sec,
                        );
                    }

                    let card4_y = (sy_val(600) as f32 + scroll_y) as i32;
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210) as i32,
                        card4_y,
                        card_w,
                        (140.0 * scale) as u32,
                        12,
                        card_bg,
                        w,
                        h,
                    );
                    if let Some(font) = &self.font {
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Window Layer",
                            s(230) as i32,
                            card4_y + sc(20.0) as i32,
                            sc(18.0),
                            text_main,
                        );
                        let layers = vec![
                            ("Always Top", crate::types::WindowLayer::Top),
                            ("Desktop", crate::types::WindowLayer::Bottom),
                        ];
                        for (i, (label, layer)) in layers.iter().enumerate() {
                            let mx = s(230 + i as u32 * 165) as i32;
                            let my = card4_y + sc(60.0) as i32;
                            let is_active = *layer == current_layer;
                            let b_col = if is_active { primary } else { 0x00E3E5E7 };
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx,
                                my,
                                sc(150.0) as u32,
                                sc(55.0) as u32,
                                8,
                                b_col,
                                w,
                                h,
                            );
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx + 2,
                                my + 2,
                                sc(150.0) as u32 - 4,
                                sc(55.0) as u32 - 4,
                                6,
                                card_bg,
                                w,
                                h,
                            );
                            if is_active {
                                Self::draw_rounded_rect(
                                    &mut buffer,
                                    w,
                                    mx + sc(125.0) as i32,
                                    my + 6,
                                    14,
                                    14,
                                    7,
                                    primary,
                                    w,
                                    h,
                                );
                            }
                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                label,
                                mx + sc(20.0) as i32,
                                my + sc(18.0) as i32,
                                sc(15.0),
                                if is_active { primary } else { text_sec },
                            );
                        }
                    }

                    // --- Monitor Selection (Card 5) ---
                    let card5_y = (sy_val(760) as f32 + scroll_y) as i32;
                    let mut card5_h = (140.0 * scale) as u32; // Base height
                                                              // Add rows for monitors
                    let rows = (self.available_monitors.len() + 2) / 3;
                    if rows > 1 {
                        card5_h += (rows as u32 - 1) * sc(70.0) as u32;
                    }

                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210) as i32,
                        card5_y,
                        card_w,
                        card5_h,
                        12,
                        card_bg,
                        w,
                        h,
                    );
                    if let Some(font) = &self.font {
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Monitor Selection",
                            s(230) as i32,
                            card5_y + sc(20.0) as i32,
                            sc(18.0),
                            text_main,
                        );
                        for (i, (name, _)) in self.available_monitors.iter().enumerate() {
                            let row = i / 3;
                            let col = i % 3;
                            let mx = s(230 + col as u32 * 110) as i32;
                            let my = card5_y + sc(60.0 + row as f32 * 65.0) as i32;
                            let is_active = self.current_monitor_name.as_ref() == Some(name);
                            let bg_col = if is_active { primary } else { 0x00F1F2F3 };
                            let text_col = if is_active { 0x00FFFFFF } else { text_main };

                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx,
                                my,
                                sc(100.0) as u32,
                                sc(55.0) as u32,
                                8,
                                bg_col,
                                w,
                                h,
                            );

                            // Truncate name if too long?
                            let disp_name = if name.len() > 10 { &name[..10] } else { name };
                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                disp_name,
                                mx + sc(10.0) as i32,
                                my + sc(18.0) as i32,
                                sc(13.0),
                                text_col,
                            );
                        }
                    }

                    // Set content height for scrolling
                    let bottom_y = 760.0 + (card5_h as f32 / scale); // Logical bottom of card5
                    self.content_height = (bottom_y + 50.0) * scale; // Physical content height
                    self.viewport_height = h as f32; // Physical viewport height
                } else if self.current_tab == 2 {
                    // --- AI Tab ---
                    let card_w = (560.0 * scale) as u32;
                    let card_h = (950.0 * scale) as u32; // Increased height to fit System Prompt + Note
                                                         // Apply scroll offset here!
                    let scroll_y = self.scroll_offset; // Use raw pixel offset

                    // Use i32 for Y calculations to handle negative values (off-screen top)
                    let card_y_raw = (sy_val(120) as f32 + scroll_y) as i32;

                    // Draw card background
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210) as i32,
                        card_y_raw,
                        card_w,
                        card_h,
                        12,
                        card_bg,
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
                            3 => (230.0, 330.0, 150.0), // Smaller numeric fields
                            4 => (405.0, 330.0, 150.0),
                            5 => (580.0, 330.0, 150.0),
                            8 => (230.0, 430.0, 150.0), // New Interval
                            6 => (230.0, 530.0, 500.0), // Tavily Key
                            7 => (230.0, 630.0, 500.0), // System Prompt
                            _ => (0.0, 0.0, 0.0),
                        };

                        let fy_scaled_raw = card_y_raw + sc(fy) as i32;

                        if let Some(font) = &self.font {
                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                label,
                                s(fx as u32) as i32,
                                fy_scaled_raw,
                                sc(14.0),
                                text_sec,
                            );
                        }

                        let input_y_raw = fy_scaled_raw + sc(25.0) as i32;
                        let input_w = sc(fw as f32) as u32;
                        let input_h = if i == 7 {
                            sc(200.0) as u32
                        } else {
                            sc(45.0) as u32
                        };

                        // Input Box
                        let is_focused = self.focused_field == Some(i);
                        let border_col = if is_focused { primary } else { 0x00E3E5E7 };
                        Self::draw_rounded_rect(
                            &mut buffer,
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
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            s(fx as u32) as i32 + 1,
                            input_y_raw + 1,
                            input_w - 2,
                            input_h.saturating_sub(2),
                            7,
                            card_bg,
                            w,
                            h,
                        );

                        // Text Drawing inside input
                        if let Some(font) = &self.font {
                            let val_chars: Vec<char> = val.chars().collect();
                            let is_masked =
                                (i == 0 || i == 6) && !val.is_empty() && !self.show_api_key;

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
                                0x00CCCCCC
                            } else {
                                text_main
                            };

                            if i == 0 || i == 6 {
                                let eye_x = s(fx as u32 + fw as u32 - 45) as i32;
                                let eye_y = input_y_raw + sc(12.0) as i32;
                                let eye_col = if self.show_api_key { primary } else { text_sec };
                                Self::draw_rect(
                                    &mut buffer,
                                    w,
                                    eye_x,
                                    eye_y + 4,
                                    16,
                                    16,
                                    eye_col,
                                    w,
                                    h,
                                );
                            }

                            if i == 7 {
                                // Multi-line rendering for System Prompt
                                let final_text: String = display_chars.iter().collect();
                                let max_width = sc(500.0 - 40.0) as u32;
                                let lines = wrap_text(
                                    &final_text,
                                    font,
                                    rusttype::Scale::uniform(sc(14.0)),
                                    max_width,
                                );

                                let sys_logical_y = input_y_raw as f64 / scale as f64;
                                let sys_logical_h = input_h as f64 / scale as f64;
                                let line_count = lines.len().max(1);
                                let full_content_h = sc(12.0 + line_count as f32 * 20.0) + sc(20.0);

                                self.active_sys_prompt_rect = Some((
                                    230.0,
                                    sys_logical_y,
                                    730.0,
                                    sys_logical_y + sys_logical_h,
                                ));
                                self.active_sys_prompt_content_height =
                                    full_content_h / scale as f32;

                                let start_text_raw = input_y_raw + sc(12.0) as i32;
                                let box_bottom_raw = input_y_raw + input_h as i32;

                                for (line_idx, line) in lines.iter().enumerate() {
                                    let line_offset = line_idx as f32 * sc(20.0);
                                    let draw_y_f = start_text_raw as f32
                                        + line_offset
                                        + (self.system_prompt_scroll_offset * scale);

                                    if draw_y_f < start_text_raw as f32 - sc(5.0) {
                                        continue;
                                    }
                                    if draw_y_f > (box_bottom_raw as f32 - sc(15.0)) {
                                        break;
                                    }
                                    if draw_y_f < 0.0 {
                                        continue;
                                    }
                                    if draw_y_f > h as f32 {
                                        break;
                                    }

                                    Self::draw_text(
                                        &mut buffer,
                                        w,
                                        font,
                                        line,
                                        s(fx as u32) as i32 + sc(15.0) as i32,
                                        draw_y_f as i32,
                                        sc(14.0),
                                        display_col,
                                    );
                                }

                                if is_focused {
                                    let mut temp_pos = 0;
                                    let mut cursor_found = false;
                                    for (line_idx, line) in lines.iter().enumerate() {
                                        let line_len = line.chars().count();
                                        if !cursor_found
                                            && (self.cursor_pos >= temp_pos
                                                && self.cursor_pos <= temp_pos + line_len)
                                        {
                                            let offset_in_line = self.cursor_pos - temp_pos;
                                            let line_chars: Vec<char> = line.chars().collect();
                                            let prefix: String =
                                                line_chars.iter().take(offset_in_line).collect();
                                            let lx = {
                                                let g = font
                                                    .layout(
                                                        &prefix,
                                                        Scale::uniform(sc(14.0)),
                                                        point(0.0, 0.0),
                                                    )
                                                    .collect::<Vec<_>>();
                                                glyph_width(&g)
                                            };
                                            let cursor_x =
                                                s(fx as u32) as i32 + sc(15.0) as i32 + lx as i32;
                                            let line_offset = line_idx as f32 * sc(20.0);
                                            let cursor_y = start_text_raw as f32
                                                + line_offset
                                                + (self.system_prompt_scroll_offset * scale);
                                            let cursor_visible = (std::time::Instant::now()
                                                - self.last_cursor_action)
                                                .as_millis()
                                                % 1000
                                                < 500;
                                            if cursor_y >= start_text_raw as f32
                                                && cursor_y <= (box_bottom_raw as f32 - sc(20.0))
                                                && cursor_visible
                                            {
                                                Self::draw_rect(
                                                    &mut buffer,
                                                    w,
                                                    cursor_x,
                                                    cursor_y as i32,
                                                    2,
                                                    sc(22.0) as u32,
                                                    primary,
                                                    w,
                                                    h,
                                                );
                                            }
                                            cursor_found = true;
                                        }
                                        temp_pos += line_len;
                                    }
                                }

                                // Scrollbar
                                let max_sys_visual_h = sc(200.0);
                                if full_content_h > max_sys_visual_h {
                                    let sb_w = sc(4.0) as u32;
                                    let sb_h = sc(190.0) as u32;
                                    let sb_x = s(fx as u32 + fw as u32 - 10) as i32;
                                    let sb_y = input_y_raw + sc(5.0) as i32;
                                    Self::draw_rect(
                                        &mut buffer,
                                        w,
                                        sb_x,
                                        sb_y,
                                        sb_w,
                                        sb_h,
                                        0x00E3E5E7,
                                        w,
                                        h,
                                    );

                                    let ratio = max_sys_visual_h / full_content_h;
                                    let handle_h = (sb_h as f32 * ratio).max(sc(20.0));
                                    let max_scroll = -(full_content_h - max_sys_visual_h);
                                    let progress = if max_scroll.abs() < 1.0 {
                                        0.0
                                    } else {
                                        self.system_prompt_scroll_offset * scale / max_scroll
                                    };
                                    let handle_y =
                                        sb_y as f32 + (sb_h as f32 - handle_h) * progress;
                                    Self::draw_rect(
                                        &mut buffer,
                                        w,
                                        sb_x,
                                        handle_y as i32,
                                        sb_w,
                                        handle_h as u32,
                                        0x00A0A0A0,
                                        w,
                                        h,
                                    );
                                }
                            } else {
                                // Single line
                                if !is_focused && display_chars.len() > 50 {
                                    display_chars =
                                        display_chars.iter().take(47).cloned().collect();
                                    display_chars.extend("...".chars());
                                }
                                let final_text: String = display_chars.iter().collect();
                                let text_start_x = s(fx as u32) as i32 + sc(15.0) as i32;
                                let text_start_y = input_y_raw as i32 + sc(12.0) as i32;

                                if is_focused {
                                    // Draw Selection
                                    if let Some(sel_start_idx) = self.selection_start {
                                        let min_idx = sel_start_idx
                                            .min(self.cursor_pos)
                                            .min(display_chars.len());
                                        let max_idx = sel_start_idx
                                            .max(self.cursor_pos)
                                            .min(display_chars.len());
                                        if min_idx != max_idx {
                                            let left_s: String =
                                                display_chars[..min_idx].iter().collect();
                                            let mid_s: String =
                                                display_chars[min_idx..max_idx].iter().collect();
                                            let lx = {
                                                let g = font
                                                    .layout(
                                                        &left_s,
                                                        Scale::uniform(sc(14.0)),
                                                        point(0.0, 0.0),
                                                    )
                                                    .collect::<Vec<_>>();
                                                glyph_width(&g)
                                            };
                                            let mx = {
                                                let g = font
                                                    .layout(
                                                        &mid_s,
                                                        Scale::uniform(sc(14.0)),
                                                        point(0.0, 0.0),
                                                    )
                                                    .collect::<Vec<_>>();
                                                glyph_width(&g)
                                            };
                                            Self::draw_rect(
                                                &mut buffer,
                                                w,
                                                text_start_x + lx as i32,
                                                text_start_y,
                                                mx,
                                                sc(22.0) as u32,
                                                0x00AADDFF,
                                                w,
                                                h,
                                            );
                                        }
                                    }

                                    Self::draw_text(
                                        &mut buffer,
                                        w,
                                        font,
                                        &final_text,
                                        text_start_x,
                                        text_start_y,
                                        sc(14.0),
                                        display_col,
                                    );

                                    // Draw Cursor
                                    let cur_idx = self.cursor_pos.min(display_chars.len());
                                    let left_s: String = display_chars[..cur_idx].iter().collect();
                                    let lx = {
                                        let g = font
                                            .layout(
                                                &left_s,
                                                Scale::uniform(sc(14.0)),
                                                point(0.0, 0.0),
                                            )
                                            .collect::<Vec<_>>();
                                        glyph_width(&g)
                                    };
                                    let cursor_x = text_start_x + lx as i32 + 1;
                                    let cursor_visible = (std::time::Instant::now()
                                        - self.last_cursor_action)
                                        .as_millis()
                                        % 1000
                                        < 500;
                                    if cursor_x < (s(fx as u32) + input_w) as i32 && cursor_visible
                                    {
                                        Self::draw_rect(
                                            &mut buffer,
                                            w,
                                            cursor_x,
                                            text_start_y,
                                            2,
                                            sc(22.0) as u32,
                                            primary,
                                            w,
                                            h,
                                        );
                                    }
                                } else {
                                    Self::draw_text(
                                        &mut buffer,
                                        w,
                                        font,
                                        &final_text,
                                        text_start_x,
                                        text_start_y,
                                        sc(14.0),
                                        display_col,
                                    );
                                }
                            }
                        }
                    }

                    // Update calculated content height for Tab 2
                    // Base content ends at fy=530 + input_h + padding
                    // fy=530 is where System Prompt starts.
                    // We need to calculate the bottom-most point relative to scroll_y=0
                    // Let's approximate based on the last drawn item (System Prompt)
                    // We need to recalculate the height of System Prompt to know where it ends relative to card start
                    // Use actual calculated height clamped to visual max (200.0)
                    let sys_prompt_h = sc(self.active_sys_prompt_content_height.min(200.0));
                    let content_bottom = sy_val(120) as f32 + sc(630.0) + sys_prompt_h + sc(100.0); // Extra padding
                    self.content_height = content_bottom - sy_val(120) as f32; // Height relative to start

                    self.viewport_height = h as f32;
                }
                // Draw Tab Header ON TOP
                if let Some(font) = &self.font {
                    let (title, sub) = match self.current_tab {
                        0 => ("Home", "Welcome to Ameath!"),
                        1 => ("Appearance", "Customize your pet's look"),
                        2 => ("AI Brain", "Connect Ameath to the cloud"),
                        3 => ("History", "Recent Local Memory (Last 50)"),
                        _ => ("About", "Ameath v0.1.0"),
                    };

                    // Draw Header Background to cover scrolled content
                    let header_h = sy_val(120);
                    Self::draw_rect(
                        &mut buffer,
                        w,
                        s(180) as i32, // Start after sidebar
                        0,
                        w - s(180),
                        header_h,
                        bg_color, // Use background color
                        w,
                        h,
                    );

                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        title,
                        s(220) as i32,
                        sy_val(40) as i32,
                        sc(32.0),
                        text_main,
                    );
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        sub,
                        s(220) as i32,
                        sy_val(85) as i32,
                        sc(16.0),
                        text_sec,
                    );
                }

                buffer.present().unwrap();
            }
        }
    }
    pub fn handle_ime(&mut self, text: &str, ai_config: &mut crate::types::AiConfig) -> bool {
        if self.current_tab != 2 {
            return false;
        }
        if let Some(idx) = self.focused_field {
            match idx {
                0 => ai_config.api_key.push_str(text),
                1 => ai_config.base_url.push_str(text),
                2 => ai_config.model.push_str(text),
                6 => ai_config.tavily_api_key.push_str(text),
                7 => ai_config.system_prompt.push_str(text),
                3 | 4 | 5 => {
                    // Numeric only, ignore IME
                }
                _ => {}
            }
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

                // Check History Items using cached rects
                if self.history_item_rects.len() == self.history.len() {
                    for (i, (_x, ly_start, _w, ly_end)) in
                        self.history_item_rects.iter().enumerate()
                    {
                        let y_start = *ly_start + self.scroll_offset as f64 / scale;
                        let y_end = *ly_end + self.scroll_offset as f64 / scale;

                        // Check Horizontal Bounds too (230 to 720)
                        if lx >= 230.0 && lx <= 720.0 && ly >= y_start && ly <= y_end {
                            // Hit item i
                            if let Some(font) = &self.font {
                                let content = &self.history[i].1;
                                let item_h_fixed_sc = 180.0 * scale as f32; // sc(180.0) approx
                                let max_width = (450.0 * scale) as u32; // sc(450.0)
                                let lines = wrap_text(
                                    content,
                                    font,
                                    rusttype::Scale::uniform(16.0 * scale as f32),
                                    max_width,
                                );

                                let line_h = 20.0 * scale as f32; // sc(20.0)
                                let full_h = (lines.len() as f32 * line_h).max(line_h);
                                let view_h = item_h_fixed_sc - (40.0 * scale as f32); // Header + Padding

                                if full_h > view_h {
                                    // Item is scrollable
                                    let current_log = self.history_scroll_states[i];
                                    let scroll_step_log = dy / scale as f32;
                                    let new_val_log = current_log + scroll_step_log;

                                    // Max scroll (negative value)
                                    // full_h and view_h are Screen Pixels.
                                    // scroll state is Logical Pixels (applied as state * scale in redraw)
                                    let max_scroll_log = -((full_h - view_h) / scale as f32);

                                    let clamped_log = new_val_log.clamp(max_scroll_log, 0.0);

                                    // Check if we actually scrolled
                                    if (clamped_log - current_log).abs() > 0.001 {
                                        self.history_scroll_states[i] = clamped_log;
                                        scrolled_item = true; // Consumed scroll
                                    } else {
                                        // We are at boundary.
                                        // If user is trying to scroll PAST boundary, we let it fall through to main list?
                                        // dy > 0: Scrolling UP (content moves down). If at 0.0, we are at top.
                                        // dy < 0: Scrolling DOWN (content moves up). If at max_scroll, we are at bottom.

                                        if dy > 0.0 && current_log >= 0.0 {
                                            // At top, trying to go up -> Scroll Main List
                                            scrolled_item = false;
                                        } else if dy < 0.0 && current_log <= max_scroll_log + 0.1 {
                                            // epsilon
                                            // At bottom, trying to go down -> Scroll Main List
                                            scrolled_item = false;
                                        } else {
                                            // Just stuck at boundary but not "pushing" past it?
                                            // Or maybe we just consume it.
                                            // Let's consume it to prevent jitter.
                                            scrolled_item = true;
                                        }

                                        // Actually the "Smart Scroll" requirement usually means:
                                        // If I scroll down and hit bottom, continue scrolling main list.
                                        if (dy > 0.0 && current_log >= -0.01)
                                            || (dy < 0.0 && current_log <= max_scroll_log + 0.01)
                                        {
                                            scrolled_item = false;
                                        }
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
                let content_h = if self.content_height > 0.0 {
                    self.content_height
                } else {
                    self.history.len() as f32 * 200.0 // approx
                };
                let viewport_h = if self.viewport_height > 0.0 {
                    self.viewport_height
                } else {
                    600.0
                };
                let min_offset = -(content_h - viewport_h).max(0.0);
                if self.scroll_offset < min_offset {
                    self.scroll_offset = min_offset;
                }
                if self.scroll_offset > 0.0 {
                    self.scroll_offset = 0.0;
                }
            }
        } else if self.current_tab == 2 {
            // AI Tab (Global Scroll)
            let mut scrolled_sys_prompt = false;

            // Check system prompt internal scroll
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
                        self.system_prompt_scroll_offset += dy;
                        // Clamp
                        let view_h = 200.0;
                        let content_h = self.active_sys_prompt_content_height;
                        let min_offset = -(content_h - view_h).max(0.0);

                        if self.system_prompt_scroll_offset < min_offset {
                            self.system_prompt_scroll_offset = min_offset;
                        }
                        if self.system_prompt_scroll_offset > 0.0 {
                            self.system_prompt_scroll_offset = 0.0;
                        }
                        scrolled_sys_prompt = true;
                    }
                }
            }

            if !scrolled_sys_prompt {
                self.scroll_offset += dy;

                let content_h = if self.content_height > 0.0 {
                    self.content_height
                } else {
                    1500.0
                };

                // Viewport is window height
                let viewport_h = 750.0;

                let min_offset = -(content_h - viewport_h).max(0.0);

                if self.scroll_offset < min_offset {
                    self.scroll_offset = min_offset;
                }
                if self.scroll_offset > 0.0 {
                    self.scroll_offset = 0.0;
                }
            }
        } else if self.current_tab == 1 {
            self.scroll_offset += dy;
            let content_h = if self.content_height > 0.0 {
                self.content_height
            } else {
                800.0
            };
            let viewport_h = if self.viewport_height > 0.0 {
                self.viewport_height
            } else {
                750.0
            };

            let min_offset = -(content_h - viewport_h).max(0.0);
            if self.scroll_offset < min_offset {
                self.scroll_offset = min_offset;
            }
            if self.scroll_offset > 0.0 {
                self.scroll_offset = 0.0;
            }
        }
    }
}

fn glyph_width(glyphs: &[rusttype::PositionedGlyph]) -> u32 {
    if glyphs.is_empty() {
        return 0;
    }
    let last = &glyphs[glyphs.len() - 1];
    let pos = last.pixel_bounding_box();
    if let Some(bb) = pos {
        bb.max.x as u32
    } else {
        last.position().x as u32
    }
}

fn wrap_text(
    text: &str,
    font: &rusttype::Font,
    scale: rusttype::Scale,
    max_width: u32,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0.0;

    for c in text.chars() {
        let g = font.glyph(c).scaled(scale);
        let h_metrics = g.h_metrics();
        let advance = h_metrics.advance_width;

        if current_width + advance > max_width as f32 {
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
                current_width = 0.0;
            }
        }

        current_line.push(c);
        current_width += advance;
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    lines
}
