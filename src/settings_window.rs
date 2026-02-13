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
    pub expanded_history_index: Option<usize>,
    pub content_height: f32,
    pub viewport_height: f32,
    pub expanded_scroll_offset: f32,
    pub active_expanded_rect: Option<(f64, f64, f64, f64)>, // lx_min, ly_min, lx_max, ly_max
    pub active_expanded_content_height: f32,
    pub system_prompt_scroll_offset: f32,
    pub active_sys_prompt_rect: Option<(f64, f64, f64, f64)>,
    pub active_sys_prompt_content_height: f32,
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
    RequestHistory,
}

impl SettingsWindow {
    pub fn handle_click(&mut self, x: f64, y: f64, is_right_click: bool) -> SettingsAction {
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
                    self.expanded_scroll_offset = 0.0;
                    self.window.request_redraw();
                    if i == 3 {
                        return SettingsAction::RequestHistory;
                    }
                    return SettingsAction::None;
                }
            }
        }

        if self.current_tab == 3 {
            // History Tab Click Handling
            if lx > 0.0 && ly > 140.0 {
                let start_y = 140.0; // Logical start Y
                let mut current_y = start_y + self.scroll_offset as f64;

                // Use unscaled height logic for hit testing since lx/ly are logical
                // Base height 60.0
                // Expansion logic: 40.0 + lines * 20.0

                for (i, (_role, content)) in self.history.iter().enumerate() {
                    let is_expanded = self.expanded_history_index == Some(i);
                    let item_h = if is_expanded {
                        let chars: Vec<char> = content.chars().collect();
                        let mut line_chars = 0;
                        let max_line_chars = 50;
                        let mut line_count = 1;
                        for c in chars {
                            line_chars += 1;
                            if c == '\n' {
                                line_chars = 0;
                                line_count += 1;
                            } else if line_chars >= max_line_chars {
                                line_count += 1;
                                line_chars = 0;
                            }
                        }
                        40.0 + (line_count as f64 * 20.0)
                    } else {
                        60.0
                    };

                    if ly >= current_y && ly <= (current_y + item_h) {
                        if is_expanded {
                            self.expanded_history_index = None;
                        } else {
                            self.expanded_history_index = Some(i);
                        }
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                    current_y += item_h;
                }
            }
            return SettingsAction::None;
        }

        if self.current_tab == 1 {
            // General Tab (Appearance)
            // Pet Scale Buttons (Visual: card1_y=120, buttons at 180)
            if ly >= 180.0 && ly <= 230.0 {
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
            if ly >= 340.0 && ly <= 400.0 {
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
            if ly >= 500.0 && ly <= 550.0 {
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
            if ly >= 660.0 && ly <= 720.0 {
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
        } else if self.current_tab == 2 {
            // AI Tab
            let ai_y_start = 120.0;
            // Apply scroll offset to click check
            let scroll_y = self.scroll_offset as f64;

            let mut found_field = false;
            for i in 0..8 {
                let (fx, fy, fw) = match i {
                    0 => (230.0, 30.0, 500.0),
                    1 => (230.0, 130.0, 500.0),
                    2 => (230.0, 230.0, 500.0),
                    3 => (230.0, 330.0, 150.0), // Smaller numeric fields
                    4 => (405.0, 330.0, 150.0),
                    5 => (580.0, 330.0, 150.0),
                    6 => (230.0, 430.0, 500.0), // Tavily Key
                    7 => (230.0, 530.0, 500.0), // System Prompt
                    _ => (0.0, 0.0, 0.0),
                };

                // Match the drawing logic:
                // let fy_scaled = card_y + sc(fy) as u32;
                // card_y = sy_val(120) + scroll_y
                // So effective Y is 120 + fy + scroll_y (in unscaled coords)

                let effective_y = ai_y_start + fy + scroll_y;
                let input_y = effective_y + 25.0; // Label is at effective_y, input is +25
                let input_h = if i == 7 { 150.0 } else { 45.0 };

                if lx >= fx && lx <= fx + fw && ly >= input_y && ly <= input_y + input_h {
                    found_field = true;
                    if is_right_click {
                        match i {
                            0 => return SettingsAction::SetAiApiKey("".to_string()),
                            1 => return SettingsAction::SetAiBaseUrl("".to_string()),
                            2 => return SettingsAction::SetAiModel("".to_string()),
                            3 => return SettingsAction::SetAiReactLimit(20),
                            4 => return SettingsAction::SetAiL1Threshold(10),
                            5 => return SettingsAction::SetAiL2Threshold(10),
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
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                }
            }

            if !found_field {
                self.focused_field = None;
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
            expanded_history_index: None,
            content_height: 0.0,
            viewport_height: 0.0,
            expanded_scroll_offset: 0.0,
            active_expanded_rect: None,
            active_expanded_content_height: 0.0,
            system_prompt_scroll_offset: 0.0,
            active_sys_prompt_rect: None,
            active_sys_prompt_content_height: 0.0,
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

    pub fn handle_key_input(
        &mut self,
        event: &winit::event::KeyEvent,
        ai_config: &mut crate::types::AiConfig,
        modifiers: winit::keyboard::ModifiersState,
    ) -> bool {
        if self.current_tab != 2 {
            return false;
        }

        let field_idx = match self.focused_field {
            Some(i) => i,
            None => return false,
        };

        use winit::keyboard::{Key, NamedKey};
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                if event.state == winit::event::ElementState::Pressed {
                    match field_idx {
                        0 => {
                            ai_config.api_key.pop();
                        }
                        1 => {
                            ai_config.base_url.pop();
                        }
                        2 => {
                            ai_config.model.pop();
                        }
                        6 => {
                            ai_config.tavily_api_key.pop();
                        }
                        7 => {
                            ai_config.system_prompt.pop();
                        }
                        _ => {}
                    }
                    ai_config.save();
                    self.window.request_redraw();
                    return true;
                }
            }
            Key::Character(c) => {
                if event.state == winit::event::ElementState::Pressed {
                    let has_ctrl = modifiers.control_key() || modifiers.super_key();
                    if c == "v" && has_ctrl {
                        #[cfg(target_os = "windows")]
                        {
                            use arboard::Clipboard;
                            if let Ok(mut clipboard) = Clipboard::new() {
                                if let Ok(text) = clipboard.get_text() {
                                    let limit = if field_idx == 7 { 5000 } else { 500 };
                                    let trimmed: String = text.trim().chars().take(limit).collect();
                                    match field_idx {
                                        0 => ai_config.api_key = trimmed,
                                        1 => ai_config.base_url = trimmed,
                                        2 => ai_config.model = trimmed,
                                        3 => ai_config.react_limit = trimmed.parse().unwrap_or(20),
                                        4 => {
                                            ai_config.l1_summary_threshold =
                                                trimmed.parse().unwrap_or(10)
                                        }
                                        5 => {
                                            ai_config.l2_merge_threshold =
                                                trimmed.parse().unwrap_or(10)
                                        }
                                        6 => ai_config.tavily_api_key = trimmed,
                                        7 => ai_config.system_prompt = trimmed,
                                        _ => {}
                                    }
                                    ai_config.save();
                                    self.window.request_redraw();
                                    return true;
                                }
                            }
                        }
                    }

                    if !c.chars().any(|ch| ch.is_control()) {
                        match field_idx {
                            0 => ai_config.api_key.push_str(c),
                            1 => ai_config.base_url.push_str(c),
                            2 => ai_config.model.push_str(c),
                            6 => ai_config.tavily_api_key.push_str(c),
                            7 => ai_config.system_prompt.push_str(c),
                            3 | 4 | 5 => {
                                if c.chars().all(|ch| ch.is_ascii_digit()) {
                                    let mut val_str = match field_idx {
                                        3 => ai_config.react_limit.to_string(),
                                        4 => ai_config.l1_summary_threshold.to_string(),
                                        5 => ai_config.l2_merge_threshold.to_string(),
                                        _ => String::new(),
                                    };
                                    val_str.push_str(c);
                                    let val = val_str.parse().unwrap_or(0);
                                    match field_idx {
                                        3 => ai_config.react_limit = val,
                                        4 => ai_config.l1_summary_threshold = val,
                                        5 => ai_config.l2_merge_threshold = val,
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                        ai_config.save();
                        self.window.request_redraw();
                        return true;
                    }
                }
            }
            Key::Named(NamedKey::Tab) => {
                if event.state == winit::event::ElementState::Pressed {
                    self.focused_field = Some((field_idx + 1) % 8); // Changed from %7 to %8
                    self.window.request_redraw();
                    return true;
                }
            }
            _ => {}
        }

        false
    }

    fn draw_rect(
        buffer: &mut [u32],
        surface_w: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        color: u32,
        max_w: u32,
        max_h: u32,
    ) {
        let max_y = (y + height).min(max_h);
        let max_x = (x + width).min(max_w);
        for cy in y..max_y {
            for cx in x..max_x {
                let idx = (cy * surface_w + cx) as usize;
                if idx < buffer.len() {
                    buffer[idx] = color;
                }
            }
        }
    }

    fn draw_rounded_rect(
        buffer: &mut [u32],
        surface_w: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        radius: u32,
        color: u32,
        max_w: u32,
        max_h: u32,
    ) {
        let max_y = (y + height).min(max_h);
        let max_x = (x + width).min(max_w);
        let r_sq = (radius * radius) as i32;

        for cy in y..max_y {
            for cx in x..max_x {
                let mut in_corner = false;
                let mut dx = 0;
                let mut dy = 0;

                if cx < x + radius && cy < y + radius {
                    dx = (x + radius) as i32 - cx as i32;
                    dy = (y + radius) as i32 - cy as i32;
                    in_corner = true;
                } else if cx >= x + width - radius && cy < y + radius {
                    dx = cx as i32 - (x + width - radius) as i32;
                    dy = (y + radius) as i32 - cy as i32;
                    in_corner = true;
                } else if cx < x + radius && cy >= y + height - radius {
                    dx = (x + radius) as i32 - cx as i32;
                    dy = cy as i32 - (y + height - radius) as i32;
                    in_corner = true;
                } else if cx >= x + width - radius && cy >= y + height - radius {
                    dx = cx as i32 - (x + width - radius) as i32;
                    dy = cy as i32 - (y + height - radius) as i32;
                    in_corner = true;
                }

                if in_corner && dx * dx + dy * dy > r_sq {
                    continue;
                }

                let idx = (cy * surface_w + cx) as usize;
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
        x: u32,
        y: u32,
        scale: f32,
        color: u32,
    ) {
        let scale = Scale::uniform(scale);
        let v_metrics = font.v_metrics(scale);
        for glyph in font.layout(text, scale, point(x as f32, y as f32 + v_metrics.ascent)) {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, v| {
                    let px = gx + bounding_box.min.x as u32;
                    let py = gy + bounding_box.min.y as u32;
                    if px < surface_w {
                        let idx = (py * surface_w + px) as usize;
                        if idx < buffer.len() {
                            let alpha = v;
                            if alpha > 0.0 {
                                let bg = buffer[idx];
                                let mut r = ((bg >> 16) & 0xFF) as f32;
                                let mut g = ((bg >> 8) & 0xFF) as f32;
                                let mut b = (bg & 0xFF) as f32;
                                let fg_r = ((color >> 16) & 0xFF) as f32;
                                let fg_g = ((color >> 8) & 0xFF) as f32;
                                let fg_b = (color & 0xFF) as f32;
                                r = r * (1.0 - alpha) + fg_r * alpha;
                                g = g * (1.0 - alpha) + fg_g * alpha;
                                b = b * (1.0 - alpha) + fg_b * alpha;
                                buffer[idx] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
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
                    off_x as u32,
                    off_y as u32,
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
                        s(40),
                        sy_val(40),
                        sc(32.0),
                        primary,
                    );
                    let menu_items = vec!["Home", "General", "AI", "History", "About"];
                    for (i, item) in menu_items.iter().enumerate() {
                        let my = sy_val(160 + i as u32 * 70);
                        let is_active = i == self.current_tab;
                        let col = if is_active { primary } else { text_sec };
                        Self::draw_text(&mut buffer, w, font, item, s(40), my, sc(20.0), col);
                        if is_active {
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                (off_x) as u32,
                                my - sc(8.0) as u32,
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
                if let Some(font) = &self.font {
                    let (title, sub) = match self.current_tab {
                        0 => ("Home", "Welcome to Ameath!"),
                        1 => ("Appearance", "Customize your pet's look"),
                        2 => ("AI Brain", "Connect Ameath to the cloud"),
                        3 => ("History", "Recent Local Memory (Last 50)"),
                        _ => ("About", "Ameath v0.1.0"),
                    };
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        title,
                        s(220),
                        sy_val(40),
                        sc(32.0),
                        text_main,
                    );
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        sub,
                        s(220),
                        sy_val(85),
                        sc(16.0),
                        text_sec,
                    );
                }

                if self.current_tab == 3 {
                    // History Tab
                    if let Some(font) = &self.font {
                        let start_y = sy_val(140);

                        let mut current_y = start_y as f32 + self.scroll_offset;
                        let mut calculated_content_height = 0.0;

                        // Reset geometry fields for this frame (will be set if expanded item exists and is drawn)
                        self.active_expanded_rect = None;
                        self.active_expanded_content_height = 0.0;

                        for (i, (role, content)) in self.history.iter().enumerate() {
                            let is_expanded = self.expanded_history_index == Some(i);

                            // Calculate display content and height
                            let (display_content, item_h, full_content_h) = if is_expanded {
                                let max_width = sc(500.0) as u32;
                                let lines = wrap_text(
                                    content,
                                    font,
                                    rusttype::Scale::uniform(sc(16.0)),
                                    max_width,
                                );
                                let wrapped = lines.join("\n");
                                let line_count = lines.len();
                                let full_h = sc(40.0) + (line_count as f32 * sc(20.0));
                                // Clamp visual height
                                let max_h = sc(400.0); // Max height for expanded item
                                let h = full_h.min(max_h);
                                (wrapped, h, full_h)
                            } else {
                                let summary = if content.chars().count() > 30 {
                                    let substr: String = content.chars().take(30).collect();
                                    format!("{}...", substr.replace("\n", " "))
                                } else {
                                    content.replace("\n", " ")
                                };
                                (summary, sc(60.0), sc(60.0))
                            };

                            calculated_content_height += item_h;
                            let y_pos = current_y;
                            current_y += item_h;

                            let min_y = sy_val(140) as f32;
                            let max_y = h as f32;

                            // Culling
                            if (y_pos + item_h) < min_y || y_pos > max_y {
                                continue;
                            }

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
                                s(230),
                                y_pos as u32,
                                sc(14.0),
                                role_col,
                            );

                            if is_expanded {
                                // Draw multiline with internal scrolling

                                // Store geometry
                                // We need logical Y. current_y is screen-scaled Y relative to window top?
                                // start_y = sy_val(140). sy_val uses scale. So current_y is scaled.
                                // We need to convert back to logical for mouse detection if we want logical.
                                // But active_expanded_rect stores LOGICAL coords.
                                // y_pos is SCALED Y.
                                // Logical Y = (y_pos - off_y) / scale.
                                // Wait, scale is 'scale'. off_y is not available nicely here without calc.
                                // But we know logic: y_pos is strictly: (140.0 * scale + off_y) + (index * h * scale) - scroll * scale?
                                // Actually, 'sy_val' does (val * scale) + off_y?
                                // Let's check sy_val definition. It is likely a macro or closure.
                                // It seems to be a closure closing over 'scale' and 'off_y'.
                                // So y_pos is purely screen Y.
                                // We better store SCREEN RECT in active_expanded_rect OR pass screen coords to handle_scroll.
                                // Passing screen coords (lx, ly) to handle_scroll (which converts to logical) means we should store LOGICAL rect.
                                // Logical Y = (y_pos / scale) ?? No, (y_pos - off_y) / scale.
                                // We can't access off_y here easily!
                                // Actually, we CAN Calculate logical rect "roughly" or just use the logic in handle_scroll to match geometry.
                                // BUT: handle_scroll converts mouse to logical.
                                // So we should store logical rect.
                                // How to get logical Y from y_pos?
                                // y_pos = start_y + offsets.
                                // start_y = (140 * scale) + off_y (presumably).
                                // So logical_y = 140.0 + (offsets / scale).
                                // Let's try to deduce logical Y from knowing start_y is logical 140.
                                let logical_y_rel = (y_pos - start_y as f32) / scale;
                                let item_logical_y = 140.0 + logical_y_rel;
                                let item_logical_h = item_h / scale;

                                self.active_expanded_rect = Some((
                                    230.0,
                                    item_logical_y as f64,
                                    730.0,
                                    (item_logical_y + item_logical_h) as f64,
                                ));
                                self.active_expanded_content_height = full_content_h / scale;

                                // Clip drawing
                                let start_text_y = y_pos + sc(20.0);
                                let end_text_y = y_pos + item_h;

                                for (li, line) in display_content.lines().enumerate() {
                                    let line_offset = li as f32 * sc(20.0);
                                    let draw_y = start_text_y
                                        + line_offset
                                        + (self.expanded_scroll_offset * scale);

                                    // Clip
                                    if draw_y < start_text_y - sc(5.0) {
                                        continue;
                                    } // Too high
                                    if draw_y > end_text_y - sc(20.0) {
                                        break;
                                    } // Too low

                                    Self::draw_text(
                                        &mut buffer,
                                        w,
                                        font,
                                        line,
                                        s(230),
                                        draw_y as u32,
                                        sc(16.0),
                                        text_main,
                                    );
                                }

                                // Draw Scrollbar for expanded item if needed
                                if item_h < full_content_h {
                                    let sb_w = sc(4.0) as u32;
                                    let sb_h = item_h as u32 - sc(10.0) as u32;
                                    let sb_x = s(230 + 500 - 10);
                                    let sb_y = y_pos as u32 + sc(5.0) as u32;

                                    // Background
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

                                    // Handle
                                    let ratio = item_h / full_content_h;
                                    let handle_h = (sb_h as f32 * ratio).max(sc(20.0));
                                    // scroll_offset is negative. range is 0 to -(content - view)
                                    // progress = scroll / max_scroll (0 to 1)
                                    let max_scroll = -(full_content_h - item_h);
                                    let progress = if max_scroll.abs() < 1.0 {
                                        0.0
                                    } else {
                                        self.expanded_scroll_offset * scale / max_scroll
                                    };
                                    let handle_y =
                                        sb_y as f32 + (sb_h as f32 - handle_h) * progress;

                                    Self::draw_rect(
                                        &mut buffer,
                                        w,
                                        sb_x,
                                        handle_y as u32,
                                        sb_w,
                                        handle_h as u32,
                                        0x00A0A0A0,
                                        w,
                                        h,
                                    );
                                }
                            } else {
                                Self::draw_text(
                                    &mut buffer,
                                    w,
                                    font,
                                    &display_content,
                                    s(230),
                                    y_pos as u32 + sc(20.0) as u32,
                                    sc(16.0),
                                    text_main,
                                );
                            }

                            // Divider
                            Self::draw_rect(
                                &mut buffer,
                                w,
                                s(230),
                                y_pos as u32 + item_h as u32 - 1,
                                (500.0 * scale) as u32,
                                1,
                                0x00E3E5E7,
                                w,
                                h,
                            );
                        }
                        self.content_height = calculated_content_height + sc(50.0); // Add bottom padding
                        self.viewport_height = (h as f32 - start_y as f32).max(0.0);
                    }
                }

                if self.current_tab == 1 {
                    // --- General Tab ---
                    let card_w = (560.0 * scale) as u32;
                    let card1_y = sy_val(120);
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210),
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
                            s(230),
                            card1_y + sc(20.0) as u32,
                            sc(18.0),
                            text_main,
                        );
                    }
                    let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
                    let labels = vec!["0.5x", "0.75x", "1.0x", "1.25x", "1.5x"];
                    for (i, &val) in scales.iter().enumerate() {
                        let mx = s(220 + i as u32 * 85);
                        let my = card1_y + sc(60.0) as u32;
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
                                mx + sc(12.0) as u32,
                                my + sc(12.0) as u32,
                                sc(14.0),
                                text_col,
                            );
                        }
                    }

                    let card2_y = sy_val(280);
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210),
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
                            s(230),
                            card2_y + sc(20.0) as u32,
                            sc(18.0),
                            text_main,
                        );
                        let modes = vec!["Quiet", "Active", "Clingy"];
                        for (i, mode) in modes.iter().enumerate() {
                            let mx = s(230 + i as u32 * 165);
                            let my = card2_y + sc(60.0) as u32;
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
                                    mx + sc(125.0) as u32,
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
                                mx + sc(25.0) as u32,
                                my + sc(18.0) as u32,
                                sc(15.0),
                                if is_active { primary } else { text_sec },
                            );
                        }
                    }

                    let card3_y = sy_val(440);
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210),
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
                            s(230),
                            card3_y + sc(20.0) as u32,
                            sc(18.0),
                            text_main,
                        );
                        let p_btn_y = card3_y + sc(60.0) as u32;
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            s(230),
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
                            s(230) + 1,
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
                            s(245),
                            p_btn_y + sc(12.0) as u32,
                            sc(14.0),
                            text_sec,
                        );
                    }

                    let card4_y = sy_val(600);
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210),
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
                            s(230),
                            card4_y + sc(20.0) as u32,
                            sc(18.0),
                            text_main,
                        );
                        let layers = vec![
                            ("Always Top", crate::types::WindowLayer::Top),
                            ("Desktop", crate::types::WindowLayer::Bottom),
                        ];
                        for (i, (label, layer)) in layers.iter().enumerate() {
                            let mx = s(230 + i as u32 * 165);
                            let my = card4_y + sc(60.0) as u32;
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
                                    mx + sc(125.0) as u32,
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
                                mx + sc(20.0) as u32,
                                my + sc(18.0) as u32,
                                sc(15.0),
                                if is_active { primary } else { text_sec },
                            );
                        }
                    }
                } else if self.current_tab == 2 {
                    // --- AI Tab ---
                    let card_w = (560.0 * scale) as u32;
                    let card_h = (750.0 * scale) as u32; // Increased height for multi-line field
                                                         // Apply scroll offset here!
                    let scroll_y = self.scroll_offset; // Use raw pixel offset
                    let card_y = (sy_val(120) as f32 + scroll_y) as u32;

                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(210),
                        card_y,
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
                    ];
                    for (i, (label, val)) in fields.iter().enumerate() {
                        let (fx, fy, fw) = match i {
                            0 => (230.0, 30.0, 500.0),
                            1 => (230.0, 130.0, 500.0),
                            2 => (230.0, 230.0, 500.0),
                            3 => (230.0, 330.0, 150.0), // Smaller numeric fields
                            4 => (405.0, 330.0, 150.0),
                            5 => (580.0, 330.0, 150.0),
                            6 => (230.0, 430.0, 500.0), // Tavily Key
                            7 => (230.0, 530.0, 500.0), // System Prompt
                            _ => (0.0, 0.0, 0.0),
                        };

                        let fy_scaled = card_y + sc(fy) as u32;
                        if let Some(font) = &self.font {
                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                label,
                                s(fx as u32),
                                fy_scaled,
                                sc(14.0),
                                text_sec,
                            );
                        }
                        let input_y = fy_scaled + sc(25.0) as u32;
                        let input_w = sc(fw as f32) as u32;
                        let input_h = if i == 7 {
                            sc(150.0) as u32
                        } else {
                            sc(45.0) as u32
                        };
                        let is_focused = self.focused_field == Some(i);
                        let border_col = if is_focused { primary } else { 0x00E3E5E7 };
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            s(fx as u32),
                            input_y,
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
                            s(fx as u32) + 1,
                            input_y + 1,
                            input_w - 2,
                            input_h - 2,
                            7,
                            card_bg,
                            w,
                            h,
                        );

                        if let Some(font) = &self.font {
                            let display_val = if val.is_empty() {
                                if is_focused {
                                    ""
                                } else {
                                    "None"
                                }
                            } else {
                                val
                            };
                            let display_col = if val.is_empty() {
                                0x00CCCCCC
                            } else {
                                text_main
                            };
                            let mut final_text =
                                if (i == 0 || i == 6) && !val.is_empty() && !self.show_api_key {
                                    let mask_char = if is_focused { "•" } else { "*" };
                                    mask_char.repeat(val.len().min(32))
                                } else {
                                    display_val.to_string()
                                };

                            if i == 0 || i == 6 {
                                let eye_x = s(fx as u32 + fw as u32 - 45);
                                let eye_y = input_y + sc(12.0) as u32;
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

                            // SAFE TRUNCATION (Fixes panic) - Skip for System Prompt (7)
                            if i != 7 && final_text.chars().count() > 50 {
                                final_text =
                                    final_text.chars().take(47).collect::<String>() + "...";
                            }

                            if i == 7 {
                                // Multi-line rendering for System Prompt
                                let max_width = sc(500.0 - 40.0) as u32; // Allow padding
                                let lines = wrap_text(
                                    &final_text,
                                    font,
                                    rusttype::Scale::uniform(sc(14.0)),
                                    max_width,
                                );

                                // Calculate actual height needed
                                let line_count = lines.len().max(1);
                                let full_content_h = sc(12.0 + line_count as f32 * 20.0) + sc(20.0);

                                // Limit visual height for System Prompt
                                let max_sys_visual_h = sc(200.0);
                                let actual_input_h = if full_content_h > max_sys_visual_h {
                                    max_sys_visual_h as u32
                                } else {
                                    full_content_h as u32
                                };

                                // Store geometry for scrolling
                                // approximate logical Y.
                                // input_y is scaled.
                                // we need consistent logic with handle_scroll.
                                // Let's store SCREEN coordinates in rect for handle_scroll,
                                // effectively bypassing the logical conversion issue if we pass screen pos to handle_scroll?
                                // No, handle_scroll receives logical pos via our manual conversion OR raw screen pos.
                                // In handle_scroll I did manual conversion.
                                // So here we must store LOGICAL rect.
                                // input_y = fy_scaled + ... = card_y + ...
                                // logical_y = fy + offset

                                // Let's just store the logical rect based on known layout
                                // System prompt is index 7.
                                // fy for 7 is 530.0.
                                // card starts at 120.0 (logical)
                                // so logical y start = 120 + 530 + 25 = 675.0 ?
                                // logic: fy=530, card_y_logical=120.
                                // input_y_logical = 120 + 530 + 25 = 675.0.
                                // visual height = actual_input_h / scale.

                                let sys_logical_y = input_y as f64 / scale as f64;
                                let sys_logical_h = actual_input_h as f64 / scale as f64;

                                self.active_sys_prompt_rect = Some((
                                    230.0,
                                    sys_logical_y,
                                    730.0,
                                    sys_logical_y + sys_logical_h,
                                ));
                                self.active_sys_prompt_content_height =
                                    full_content_h / scale as f32;

                                // Re-draw background with correct height
                                let is_focused = self.focused_field == Some(i);
                                let border_col = if is_focused { primary } else { 0x00E3E5E7 };
                                Self::draw_rounded_rect(
                                    &mut buffer,
                                    w,
                                    s(fx as u32),
                                    input_y,
                                    input_w,
                                    actual_input_h,
                                    8,
                                    border_col,
                                    w,
                                    h,
                                );
                                Self::draw_rounded_rect(
                                    &mut buffer,
                                    w,
                                    s(fx as u32) + 1,
                                    input_y + 1,
                                    input_w - 2,
                                    actual_input_h - 2,
                                    7,
                                    card_bg,
                                    w,
                                    h,
                                );

                                // Helper for clipping
                                let start_text_y = input_y + sc(12.0) as u32;
                                let end_text_y = input_y + actual_input_h - sc(10.0) as u32;

                                for (line_idx, line) in lines.iter().enumerate() {
                                    let line_offset = line_idx as f32 * sc(20.0);
                                    let draw_y_f = start_text_y as f32
                                        + line_offset
                                        + (self.system_prompt_scroll_offset * scale);

                                    // Clip
                                    if draw_y_f < start_text_y as f32 - sc(15.0) {
                                        continue;
                                    }
                                    if draw_y_f > end_text_y as f32 {
                                        break;
                                    }

                                    Self::draw_text(
                                        &mut buffer,
                                        w,
                                        font,
                                        line,
                                        s(fx as u32) + sc(15.0) as u32,
                                        draw_y_f as u32,
                                        sc(14.0),
                                        display_col,
                                    );
                                }

                                // Draw Scrollbar if needed
                                if full_content_h > max_sys_visual_h {
                                    let sb_w = sc(4.0) as u32;
                                    let sb_h = actual_input_h - sc(10.0) as u32;
                                    let sb_x = s(fx as u32 + fw as u32 - 10);
                                    let sb_y = input_y + sc(5.0) as u32;

                                    // Background
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

                                    // Handle
                                    let ratio = max_sys_visual_h / full_content_h;
                                    let handle_h = (sb_h as f32 * ratio).max(sc(20.0));

                                    let max_scroll = -(full_content_h - max_sys_visual_h); // negative
                                                                                           // progress 0..1
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
                                        handle_y as u32,
                                        sb_w,
                                        handle_h as u32,
                                        0x00A0A0A0,
                                        w,
                                        h,
                                    );
                                }

                                // Simple Cursor (at end of last line)
                                if is_focused {
                                    let last_line = lines.last().cloned().unwrap_or_default();
                                    let line_count = lines.len().max(1);
                                    let glyphs: Vec<_> = font
                                        .layout(
                                            &last_line,
                                            Scale::uniform(sc(14.0)),
                                            point(0.0, 0.0),
                                        )
                                        .collect();
                                    let tw = glyph_width(&glyphs);

                                    let cursor_x = s(fx as u32) + sc(15.0) as u32 + tw + 2;
                                    let cursor_y =
                                        input_y + sc(12.0 + (line_count - 1) as f32 * 20.0) as u32;

                                    if cursor_y < input_y + input_h {
                                        Self::draw_rect(
                                            &mut buffer,
                                            w,
                                            cursor_x,
                                            cursor_y,
                                            2,
                                            sc(22.0) as u32,
                                            primary,
                                            w,
                                            h,
                                        );
                                    }
                                }
                            } else {
                                // Single-line rendering for others
                                Self::draw_text(
                                    &mut buffer,
                                    w,
                                    font,
                                    &final_text,
                                    s(fx as u32) + sc(15.0) as u32,
                                    input_y + sc(12.0) as u32,
                                    sc(14.0),
                                    display_col,
                                );
                                if is_focused {
                                    let glyphs: Vec<_> = font
                                        .layout(
                                            &final_text,
                                            Scale::uniform(sc(14.0)),
                                            point(0.0, 0.0),
                                        )
                                        .collect();
                                    let tw = if val.is_empty() {
                                        0
                                    } else {
                                        glyph_width(&glyphs)
                                    };
                                    let cursor_x = s(fx as u32) + sc(15.0) as u32 + tw + 2;
                                    if cursor_x < s(fx as u32) + input_w {
                                        Self::draw_rect(
                                            &mut buffer,
                                            w,
                                            cursor_x,
                                            input_y + sc(12.0) as u32,
                                            2,
                                            sc(22.0) as u32,
                                            primary,
                                            w,
                                            h,
                                        );
                                    }
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
                    let sys_prompt_lines = ai_config.system_prompt.len() / 60 + 1; // Approx
                    let sys_prompt_h = sc(12.0 + sys_prompt_lines as f32 * 20.0) + sc(50.0);
                    let content_bottom = sy_val(120) as f32 + sc(530.0) + sys_prompt_h + sc(100.0); // Extra padding
                    self.content_height = content_bottom - sy_val(120) as f32; // Height relative to start
                    self.content_height += 1000.0; // Add plenty of extra scroll space just in case
                    self.viewport_height = h as f32;

                    if let Some(font) = &self.font {
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Note: More AI features coming soon!",
                            s(230),
                            card_y + sc(340.0) as u32,
                            sc(11.0),
                            text_sec,
                        );
                    }
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
            let mut scrolled_expanded = false;

            // Check if we should scroll the expanded item (internal scroll)
            if let Some((min_x, min_y, max_x, max_y)) = self.active_expanded_rect {
                if let Some(pos) = cursor_pos {
                    // Map cursor to logical coordinates
                    let size = self.window.inner_size();
                    let w = size.width as f64;
                    let h = size.height as f64;
                    let scale = (w / 800.0).min(h / 750.0);
                    let off_x = (w - 800.0 * scale) / 2.0;
                    let off_y = (h - 750.0 * scale) / 2.0;
                    let lx = (pos.x - off_x) / scale;
                    let ly = (pos.y - off_y) / scale;

                    if lx >= min_x && lx <= max_x && ly >= min_y && ly <= max_y {
                        // Scroll expanded item
                        self.expanded_scroll_offset += dy;
                        // Clamp
                        // Visual height is fixed at 400.0 (must match redraw)
                        let view_h = 400.0;
                        let content_h = self.active_expanded_content_height;
                        // Scroll range: 0.0 to -(content_h - view_h)
                        let min_offset = -(content_h - view_h).max(0.0);

                        if self.expanded_scroll_offset < min_offset {
                            self.expanded_scroll_offset = min_offset;
                        }
                        if self.expanded_scroll_offset > 0.0 {
                            self.expanded_scroll_offset = 0.0;
                        }
                        scrolled_expanded = true;
                    }
                }
            }

            if !scrolled_expanded {
                self.scroll_offset += dy;
                let content_h = if self.content_height > 0.0 {
                    self.content_height
                } else {
                    self.history.len() as f32 * 60.0
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

    for c in text.chars() {
        let mut test_line = current_line.clone();
        test_line.push(c);

        let glyphs: Vec<_> = font
            .layout(&test_line, scale, rusttype::point(0.0, 0.0))
            .collect();
        let width = glyph_width(&glyphs);

        if width > max_width && !current_line.is_empty() {
            lines.push(current_line);
            current_line = c.to_string();
        } else {
            current_line = test_line;
        }
    }
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
