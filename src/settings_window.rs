use rusttype::{point, Font, Scale};
use softbuffer::{Context, Surface};
use std::fs;
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::{dpi::PhysicalSize, event_loop::EventLoopWindowTarget, window::Window};

pub struct SettingsWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    font: Option<Font<'static>>,
    current_tab: usize,               // 0: Home, 1: General, 2: AI, 3: About
    pub focused_field: Option<usize>, // AI Tab focus: 0: Key, 1: URL, 2: Model
    show_api_key: bool,
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
}

impl SettingsWindow {
    pub fn handle_click(&mut self, x: f64, y: f64, is_right_click: bool) -> SettingsAction {
        let size = self.window.inner_size();
        let scale_x = size.width as f64 / 600.0;
        let scale_y = size.height as f64 / 700.0;

        // Sidebar Tab Selection
        let sb_w = 100.0 * scale_x;
        if x < sb_w {
            for i in 0..4 {
                let my_min = (100.0 + i as f64 * 60.0 - 20.0) * scale_y;
                let my_max = (100.0 + i as f64 * 60.0 + 20.0) * scale_y;
                if y >= my_min && y <= my_max {
                    self.current_tab = i;
                    self.window.request_redraw();
                    return SettingsAction::None;
                }
            }
        }

        if self.current_tab == 1 {
            // General Tab (Existing Appearance Settings)
            // Scale Buttons
            let scale_y_min = 150.0 * scale_y;
            let scale_y_max = 190.0 * scale_y;
            if y >= scale_y_min && y <= scale_y_max {
                let start_x = 120.0 * scale_x;
                let btn_w = 60.0 * scale_x;
                let gap = 10.0 * scale_x;
                let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
                for (i, &s) in scales.iter().enumerate() {
                    let btn_x = start_x + i as f64 * (btn_w + gap);
                    if x >= btn_x && x <= btn_x + btn_w {
                        return SettingsAction::SetScale(s);
                    }
                }
            }

            // Mode Buttons
            let mode_y_min = 290.0 * scale_y;
            let mode_y_max = 340.0 * scale_y;
            if y >= mode_y_min && y <= mode_y_max {
                let btn_w = 120.0 * scale_x;
                let gap = 12.0 * scale_x;
                let start_x = 140.0 * scale_x;
                if x >= start_x && x <= start_x + btn_w {
                    return SettingsAction::SetMode(crate::types::BehaviorMode::Quiet);
                }
                let active_x = start_x + btn_w + gap;
                if x >= active_x && x <= active_x + btn_w {
                    return SettingsAction::SetMode(crate::types::BehaviorMode::Active);
                }
                let clingy_x = active_x + btn_w + gap;
                if x >= clingy_x && x <= clingy_x + btn_w {
                    return SettingsAction::SetMode(crate::types::BehaviorMode::Clingy);
                }
            }

            // Music Path Button
            let music_y_min = 430.0 * scale_y;
            let music_y_max = 470.0 * scale_y;
            if y >= music_y_min && y <= music_y_max {
                let start_x = 140.0 * scale_x;
                let btn_w = 400.0 * scale_x;
                if x >= start_x && x <= start_x + btn_w {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        return SettingsAction::SetMusicPath(path);
                    }
                }
            }

            // Layer Buttons
            let layer_y_min = 570.0 * scale_y;
            let layer_y_max = 610.0 * scale_y;
            if y >= layer_y_min && y <= layer_y_max {
                let btn_w = 120.0 * scale_x;
                let gap = 12.0 * scale_x;
                let start_x = 140.0 * scale_x;
                if x >= start_x && x <= start_x + btn_w {
                    return SettingsAction::SetLayer(crate::types::WindowLayer::Top);
                }
                let bottom_x = start_x + btn_w + gap;
                if x >= bottom_x && x <= bottom_x + btn_w {
                    return SettingsAction::SetLayer(crate::types::WindowLayer::Bottom);
                }
            }
        } else if self.current_tab == 2 {
            // AI Tab
            let ai_y_start = 100.0 * scale_y;
            let field_gap = 90.0 * scale_y;
            let input_x = 140.0 * scale_x;
            let input_w = 400.0 * scale_x;

            let mut found_field = false;
            for i in 0..3 {
                let fy = ai_y_start + 20.0 * scale_y + (i as f64 * field_gap);
                let input_y = fy + 25.0 * scale_y;
                let input_h = 40.0 * scale_y;

                if x >= input_x && x <= input_x + input_w && y >= input_y && y <= input_y + input_h
                {
                    found_field = true;
                    if is_right_click {
                        match i {
                            0 => return SettingsAction::SetAiApiKey("".to_string()),
                            1 => return SettingsAction::SetAiBaseUrl("".to_string()),
                            2 => return SettingsAction::SetAiModel("".to_string()),
                            _ => {}
                        }
                    } else {
                        // Check for eye toggle click (API Key field only)
                        if i == 0 {
                            let eye_x = input_x + input_w - 35.0 * scale_x;
                            if x >= eye_x {
                                self.show_api_key = !self.show_api_key;
                                self.window.request_redraw();
                                return SettingsAction::None;
                            }
                        }

                        self.focused_field = Some(i);
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                }
            }

            if !found_field {
                // Check for Save button click
                let btn_w = 100.0 * scale_x;
                let btn_h = 30.0 * scale_y;
                let btn_x = 120.0 * scale_x + 440.0 * scale_x - btn_w - 20.0 * scale_x;
                let btn_y = ai_y_start + 360.0 * scale_y - btn_h - 15.0 * scale_y;

                if x >= btn_x && x <= btn_x + btn_w && y >= btn_y && y <= btn_y + btn_h {
                    // Manual save trigger (confirmation via redraw)
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
                .with_inner_size(PhysicalSize::new(600, 700))
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
                        _ => {}
                    }
                    ai_config.save();
                    self.window.request_redraw();
                    return true;
                }
            }
            Key::Character(c) => {
                if event.state == winit::event::ElementState::Pressed {
                    // Check for CTRL+V
                    let is_v = c == "v" || c == "V";
                    let has_ctrl = modifiers.control_key() || modifiers.super_key();

                    if is_v && has_ctrl {
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
                    self.focused_field = Some((field_idx + 1) % 3);
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
                // Check corners
                let mut in_corner = false;
                let dx;
                let dy;

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
                } else {
                    dx = 0;
                    dy = 0;
                }

                if in_corner {
                    if dx * dx + dy * dy > r_sq {
                        continue;
                    }
                }

                let idx = (cy * surface_w + cx) as usize;
                if idx < buffer.len() {
                    // Simple alpha blending if color has alpha (not implemented fully here, assumes opaque rect for now)
                    // But our color format is 0x00RRGGBB usually for softbuffer on windows?
                    // Actually softbuffer expects 00RRGGBB where top byte is ignored or 0.
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

        // Loop through glyphs
        for glyph in font.layout(text, scale, point(x as f32, y as f32 + v_metrics.ascent)) {
            if let Some(bounding_box) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, v| {
                    let px = gx + bounding_box.min.x as u32;
                    let py = gy + bounding_box.min.y as u32;

                    if px < surface_w {
                        let idx = (py * surface_w + px) as usize;
                        if idx < buffer.len() {
                            // Alpha blending
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
                // Resize surface if needed
                let _ = self.surface.resize(width, height);

                let mut buffer = self.surface.buffer_mut().unwrap();
                let w = width.get();
                let h = height.get();

                // Scaling Factors (Base 600x700)
                let sx = w as f32 / 600.0;
                let sy = h as f32 / 700.0;

                // Helper to scale coordinates
                let s = |val: u32| -> u32 { (val as f32 * sx) as u32 };
                let sy_val = |val: u32| -> u32 { (val as f32 * sy) as u32 };

                // Colors
                let bg_color = 0x00F6F7F9;
                let card_bg = 0x00FFFFFF;
                let primary = 0x00FB7299;
                let text_main = 0x0018191C;
                let text_sec = 0x009499A0;
                let sb_bg = 0x00FFFFFF;

                // 1. Background
                buffer.fill(bg_color);

                // 2. Sidebar
                let sb_w = s(100);
                Self::draw_rect(&mut buffer, w, 0, 0, sb_w, h, sb_bg, w, h);

                if let Some(font) = &self.font {
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        "Ame",
                        (100.0 * sx * 0.3) as u32,
                        sy_val(30),
                        24.0 * sx,
                        primary,
                    );

                    // Menu Items
                    let menu_items = vec!["Home", "General", "AI", "About"];
                    for (i, item) in menu_items.iter().enumerate() {
                        let my = sy_val(100 + i as u32 * 60);
                        let is_active = i == self.current_tab;
                        let col = if is_active { primary } else { text_sec };
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            item,
                            (100.0 * sx * 0.2) as u32,
                            my,
                            16.0 * sx,
                            col,
                        );

                        if is_active {
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                0,
                                my - 5,
                                4,
                                (30.0 * sy) as u32,
                                2,
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
                        _ => ("About", "Ameath v0.1.0"),
                    };
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        title,
                        s(120),
                        sy_val(30),
                        24.0 * sx,
                        text_main,
                    );
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        sub,
                        s(120),
                        sy_val(65),
                        14.0 * sx,
                        text_sec,
                    );
                }

                if self.current_tab == 1 {
                    // --- General Tab: Same as before ---
                    // Card 1: Scale
                    let card1_y = sy_val(100);
                    let card_w = s(440);
                    let card_h = sy_val(120);
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(120),
                        card1_y,
                        card_w,
                        card_h,
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
                            s(140),
                            card1_y + sy_val(20),
                            16.0 * sx,
                            text_main,
                        );
                    }
                    let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
                    let scale_labels = vec!["0.5x", "0.75x", "1.0x", "1.25x", "1.5x"];
                    let s_btn_y = card1_y + sy_val(50);
                    let s_btn_w = s(60);
                    let s_btn_h = sy_val(40);
                    let s_gap = s(10);
                    for (i, &val) in scales.iter().enumerate() {
                        let mx = s(120) + i as u32 * (s_btn_w + s_gap);
                        let is_active = (current_scale - val).abs() < 0.01;
                        let bg_col = if is_active { primary } else { 0x00F1F2F3 };
                        let text_col = if is_active { 0x00FFFFFF } else { text_main };
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            mx,
                            s_btn_y,
                            s_btn_w,
                            s_btn_h,
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
                                scale_labels[i],
                                mx + (8.0 * sx) as u32,
                                s_btn_y + (10.0 * sy) as u32,
                                14.0 * sx,
                                text_col,
                            );
                        }
                    }

                    // Card 2: Behavior Mode
                    let card2_y = sy_val(240);
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(120),
                        card2_y,
                        card_w,
                        sy_val(120),
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
                            s(140),
                            card2_y + sy_val(20),
                            16.0 * sx,
                            text_main,
                        );
                        let modes = vec!["Quiet", "Active", "Clingy"];
                        for (i, mode) in modes.iter().enumerate() {
                            let mx = s(140) + i as u32 * (s(120) + s(12));
                            let my = card2_y + sy_val(50);
                            let is_active = *mode == current_mode;
                            let border_col = if is_active { primary } else { 0x00E3E5E7 };
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx,
                                my,
                                s(120),
                                sy_val(50),
                                8,
                                border_col,
                                w,
                                h,
                            );
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx + 2,
                                my + 2,
                                s(120) - 4,
                                sy_val(50) - 4,
                                6,
                                card_bg,
                                w,
                                h,
                            );
                            if is_active {
                                Self::draw_rounded_rect(
                                    &mut buffer,
                                    w,
                                    mx + s(120) - 20,
                                    my + 5,
                                    12,
                                    12,
                                    6,
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
                                mx + (20.0 * sx) as u32,
                                my + (18.0 * sy) as u32,
                                14.0 * sx,
                                if is_active { primary } else { text_sec },
                            );
                        }
                    }

                    // Card 3: Music Path
                    let card3_y = sy_val(380);
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(120),
                        card3_y,
                        card_w,
                        sy_val(120),
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
                            s(140),
                            card3_y + sy_val(20),
                            16.0 * sx,
                            text_main,
                        );
                        let p_btn_y = card3_y + sy_val(50);
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            s(140),
                            p_btn_y,
                            s(400),
                            sy_val(40),
                            8,
                            0x00E3E5E7,
                            w,
                            h,
                        );
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            s(140) + 1,
                            p_btn_y + 1,
                            s(400) - 2,
                            sy_val(40) - 2,
                            7,
                            card_bg,
                            w,
                            h,
                        );
                        let path_text = current_music_path
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|| "Click to select...".to_string());
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            &path_text,
                            s(155),
                            p_btn_y + sy_val(10),
                            12.0 * sx,
                            text_sec,
                        );
                    }

                    // Card 4: Layer
                    let card4_y = sy_val(520);
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(120),
                        card4_y,
                        card_w,
                        sy_val(120),
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
                            s(140),
                            card4_y + sy_val(20),
                            16.0 * sx,
                            text_main,
                        );
                        let layers = vec![
                            ("Always Top", crate::types::WindowLayer::Top),
                            ("Desktop", crate::types::WindowLayer::Bottom),
                        ];
                        for (i, (label, layer)) in layers.iter().enumerate() {
                            let mx = s(140) + i as u32 * (s(120) + s(12));
                            let my = card4_y + sy_val(50);
                            let is_active = *layer == current_layer;
                            let border_col = if is_active { primary } else { 0x00E3E5E7 };
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx,
                                my,
                                s(120),
                                sy_val(50),
                                8,
                                border_col,
                                w,
                                h,
                            );
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                mx + 2,
                                my + 2,
                                s(120) - 4,
                                sy_val(50) - 4,
                                6,
                                card_bg,
                                w,
                                h,
                            );
                            if is_active {
                                Self::draw_rounded_rect(
                                    &mut buffer,
                                    w,
                                    mx + s(120) - 20,
                                    my + 5,
                                    12,
                                    12,
                                    6,
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
                                mx + (20.0 * sx) as u32,
                                my + (18.0 * sy) as u32,
                                14.0 * sx,
                                if is_active { primary } else { text_sec },
                            );
                        }
                    }
                } else if self.current_tab == 2 {
                    // --- AI Tab ---
                    let card_w = s(440);
                    let card_h = sy_val(400);
                    let card_start_y = sy_val(100);

                    // Card: API Settings
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        s(120),
                        card_start_y,
                        card_w,
                        card_h,
                        12,
                        card_bg,
                        w,
                        h,
                    );

                    if let Some(font) = &self.font {
                        let fields = vec![
                            ("API Key", &ai_config.api_key, "Enter your key..."),
                            (
                                "Base URL",
                                &ai_config.base_url,
                                "https://api.deepseek.com/v1",
                            ),
                            ("Model", &ai_config.model, "deepseek-chat"),
                        ];

                        for (i, (label, val, placeholder)) in fields.iter().enumerate() {
                            let fy = card_start_y + sy_val(20 + i as u32 * 90);
                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                label,
                                s(140),
                                fy,
                                14.0 * sx,
                                text_main,
                            );
                            let input_y = fy + sy_val(25);
                            let input_w = s(400);
                            let input_h = sy_val(40);

                            let is_focused = self.focused_field == Some(i);
                            let border_col = if is_focused { primary } else { 0x00F1F2F3 };

                            // Draw input background/border
                            Self::draw_rounded_rect(
                                &mut buffer,
                                w,
                                s(140),
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
                                s(140) + 1,
                                input_y + 1,
                                input_w - 2,
                                input_h - 2,
                                7,
                                if is_focused { card_bg } else { 0x00F1F2F3 },
                                w,
                                h,
                            );

                            let display_val = if val.is_empty() { *placeholder } else { val };
                            let display_col = if val.is_empty() { text_sec } else { text_main };

                            // Mask API key logic
                            let mut final_text = if i == 0 && !val.is_empty() {
                                if self.show_api_key {
                                    val.to_string()
                                } else {
                                    let mask_char = if is_focused { "•" } else { "*" };
                                    mask_char.repeat(val.len().min(24))
                                }
                            } else {
                                display_val.to_string()
                            };

                            // Draw "Show/Hide" icon placeholder for API Key
                            if i == 0 {
                                let eye_x = s(140) + input_w - s(30);
                                let eye_y = input_y + sy_val(10);
                                let eye_col = if self.show_api_key { primary } else { text_sec };
                                // Simple dot/square for icon for now
                                Self::draw_rect(
                                    &mut buffer,
                                    w,
                                    eye_x,
                                    eye_y + 4,
                                    12,
                                    12,
                                    eye_col,
                                    w,
                                    h,
                                );
                            }

                            // --- Better "Scrolling" Truncation ---
                            let max_chars = 40;
                            if final_text.len() > max_chars {
                                if is_focused && !val.is_empty() && i != 0 {
                                    let mut start_offset = final_text.len() - max_chars + 3;
                                    while !final_text.is_char_boundary(start_offset) {
                                        start_offset += 1;
                                    }
                                    final_text = format!("...{}", &final_text[start_offset..]);
                                } else {
                                    let mut end_offset = max_chars - 3;
                                    while !final_text.is_char_boundary(end_offset) {
                                        end_offset -= 1;
                                    }
                                    final_text = format!("{}...", &final_text[..end_offset]);
                                }
                            }

                            Self::draw_text(
                                &mut buffer,
                                w,
                                font,
                                &final_text,
                                s(150),
                                input_y + sy_val(10),
                                12.0 * sx,
                                display_col,
                            );

                            // Draw cursor if focused
                            if is_focused {
                                let font_size = 12.0 * sx;
                                let glyphs: Vec<_> = font
                                    .layout(&final_text, Scale::uniform(font_size), point(0.0, 0.0))
                                    .collect();

                                let text_w = if val.is_empty() {
                                    0
                                } else {
                                    glyph_width(&glyphs)
                                };

                                let cursor_x = s(150) + text_w + 2;
                                let cursor_y_top = input_y + sy_val(10);
                                let cursor_h = sy_val(20);
                                if cursor_x < s(150) + input_w - 5 {
                                    Self::draw_rect(
                                        &mut buffer,
                                        w,
                                        cursor_x,
                                        cursor_y_top,
                                        2,
                                        cursor_h,
                                        primary,
                                        w,
                                        h,
                                    );
                                }
                            }
                        }

                        // --- Save Button at the Bottom ---
                        let btn_w = s(100);
                        let btn_h = sy_val(30);
                        let btn_x = s(120) + card_w - btn_w - s(20);
                        let btn_y = card_start_y + card_h - btn_h - sy_val(15);

                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            btn_x,
                            btn_y,
                            btn_w,
                            btn_h,
                            6,
                            primary,
                            w,
                            h,
                        );
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Save",
                            btn_x + s(30),
                            btn_y + sy_val(6),
                            12.0 * sx,
                            0xFFFFFFFF,
                        );

                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            "Note: More AI features coming soon!",
                            s(140),
                            card_start_y + sy_val(280),
                            12.0 * sx,
                            text_sec,
                        );
                    }
                }

                buffer.present().unwrap();
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
