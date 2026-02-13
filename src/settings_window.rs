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
                    self.window.request_redraw();
                    if i == 3 {
                        return SettingsAction::RequestHistory;
                    }
                    return SettingsAction::None;
                }
            }
        }

        if self.current_tab == 3 {
            // History Tab Click Handling (if any)
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

            let mut found_field = false;
            for i in 0..8 {
                // Changed from 0..7 to 0..8
                let (fx, fy, fw) = match i {
                    0 => (230.0, ai_y_start + 55.0, 500.0),
                    1 => (230.0, ai_y_start + 155.0, 500.0),
                    2 => (230.0, ai_y_start + 255.0, 500.0),
                    3 => (230.0, ai_y_start + 355.0, 150.0),
                    4 => (405.0, ai_y_start + 355.0, 150.0),
                    5 => (580.0, ai_y_start + 355.0, 150.0),
                    6 => (230.0, ai_y_start + 455.0, 500.0), // Tavily Key
                    7 => (230.0, ai_y_start + 555.0, 500.0), // System Prompt
                    _ => (0.0, 0.0, 0.0),
                };

                if lx >= fx && lx <= fx + fw && ly >= fy && ly <= fy + 45.0 {
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
                        if (i == 0 || i == 6) && lx >= fx + fw - 45.0 {
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
                let btn_w = 120.0;
                let btn_h = 35.0;
                let btn_x = 210.0 + 560.0 - btn_w - 20.0;

                // The Tavily Key field is below this, so the button Y needs to be adjusted.
                // Let's assume the save button is now below the 7th field.
                let save_btn_y = ai_y_start + 500.0 + 55.0 - btn_h - 15.0; // 500.0 is the fy for Tavily Key, 55.0 is height of input field + label gap

                if lx >= btn_x
                    && lx <= btn_x + btn_w
                    && ly >= save_btn_y
                    && ly <= save_btn_y + btn_h
                {
                    self.focused_field = None;
                    self.window.request_redraw();
                    return SettingsAction::None;
                }

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
                                    let mut trimmed = text.trim().to_string();
                                    if trimmed.len() > 500 {
                                        trimmed.truncate(500);
                                    }
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
                        let item_h = sc(60.0) as u32; // Height per history item

                        for (i, (role, content)) in self.history.iter().enumerate() {
                            let item_h_f32 = item_h as f32;
                            let y_pos =
                                start_y as f32 + self.scroll_offset + (i as f32 * item_h_f32);
                            let min_y = sy_val(140) as f32;
                            let max_y = h as f32;

                            // Simple culling
                            if (y_pos + item_h_f32) < min_y || y_pos > max_y {
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

                            let display_content = if content.chars().count() > 30 {
                                let substr: String = content.chars().take(30).collect();
                                format!("{}...", substr.replace("\n", " "))
                            } else {
                                content.replace("\n", " ")
                            };

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

                            // Divider
                            Self::draw_rect(
                                &mut buffer,
                                w,
                                s(230),
                                y_pos as u32 + item_h - 5,
                                (500.0 * scale) as u32,
                                1,
                                0x00E3E5E7,
                                w,
                                h,
                            );
                        }
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
                    let card_h = (550.0 * scale) as u32; // Increased height to accommodate new field
                    let card_y = sy_val(120);
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
                        let input_h = sc(45.0) as u32;
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

                            // SAFE TRUNCATION (Fixes panic)
                            if final_text.chars().count() > 50 {
                                final_text =
                                    final_text.chars().take(47).collect::<String>() + "...";
                            }

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
                                    .layout(&final_text, Scale::uniform(sc(14.0)), point(0.0, 0.0))
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
                    if let Some(font) = &self.font {
                        let bw = sc(120.0) as u32;
                        let bh = sc(35.0) as u32;
                        let bx = s(210 + 560 - 120 - 20);
                        let by = card_y + (400.0 * scale) as u32 - bh - sc(15.0) as u32;
                        Self::draw_rounded_rect(&mut buffer, w, bx, by, bw, bh, 6, primary, w, h);
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Save",
                            bx + sc(35.0) as u32,
                            by + sc(8.0) as u32,
                            sc(14.0),
                            0xFFFFFFFF,
                        );
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
    pub fn handle_scroll(&mut self, dy: f32) {
        if self.current_tab != 3 {
            return;
        }

        // Scroll speed
        self.scroll_offset += dy;

        // Clamp
        // Content height ~ history.len() * 60
        // Viewport ~ 600
        let item_h = 60.0; // unscaled estimate for logic
        let content_h = self.history.len() as f32 * item_h;
        let viewport_h = 600.0;

        let min_offset = -(content_h - viewport_h).max(0.0);
        let max_offset = 0.0;

        if self.scroll_offset < min_offset {
            self.scroll_offset = min_offset;
        }
        if self.scroll_offset > max_offset {
            self.scroll_offset = max_offset;
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
