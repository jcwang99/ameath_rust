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
}

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsAction {
    None,
    SetScale(f32),
    SetMode(crate::types::BehaviorMode),
    SetMusicPath(std::path::PathBuf),
    SetLayer(crate::types::WindowLayer),
}

impl SettingsWindow {
    pub fn handle_click(&self, x: f64, y: f64) -> SettingsAction {
        let size = self.window.inner_size();
        let scale_x = size.width as f64 / 600.0;
        let scale_y = size.height as f64 / 700.0;

        // Scale Buttons (Replacing Slider)
        // Card1 Y=90. Buttons Y=140 (~90+50).
        let scale_y_min = 150.0 * scale_y;
        let scale_y_max = 190.0 * scale_y; // 40px height

        if y >= scale_y_min && y <= scale_y_max {
            let start_x = 120.0 * scale_x;
            let btn_w = 60.0 * scale_x;
            let gap = 10.0 * scale_x;

            // 5 options: 0.5, 0.75, 1.0, 1.25, 1.5
            let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];

            for (i, &s) in scales.iter().enumerate() {
                let btn_x = start_x + i as f64 * (btn_w + gap);
                if x >= btn_x && x <= btn_x + btn_w {
                    return SettingsAction::SetScale(s);
                }
            }
        }

        // Mode Buttons: Card2 Y=210. Buttons Y=260.
        let mode_y_min = 290.0 * scale_y;
        let mode_y_max = 340.0 * scale_y;

        if y >= mode_y_min && y <= mode_y_max {
            let btn_w = 120.0 * scale_x;
            let gap = 12.0 * scale_x;
            let start_x = 140.0 * scale_x;

            // Quiet
            if x >= start_x && x <= start_x + btn_w {
                return SettingsAction::SetMode(crate::types::BehaviorMode::Quiet);
            }
            // Active
            let active_x = start_x + btn_w + gap;
            if x >= active_x && x <= active_x + btn_w {
                return SettingsAction::SetMode(crate::types::BehaviorMode::Active);
            }
            // Clingy
            let clingy_x = active_x + btn_w + gap;
            if x >= clingy_x && x <= clingy_x + btn_w {
                return SettingsAction::SetMode(crate::types::BehaviorMode::Clingy);
            }
        }

        // Music Path Button: Card3 Y=380. Button Y=430.
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

        // Layer Buttons: Card4 Y=520. Buttons Y=570.
        let layer_y_min = 570.0 * scale_y;
        let layer_y_max = 610.0 * scale_y;
        if y >= layer_y_min && y <= layer_y_max {
            let btn_w = 120.0 * scale_x;
            let gap = 12.0 * scale_x;
            let start_x = 140.0 * scale_x;

            // Top
            if x >= start_x && x <= start_x + btn_w {
                return SettingsAction::SetLayer(crate::types::WindowLayer::Top);
            }
            // Bottom
            let bottom_x = start_x + btn_w + gap;
            if x >= bottom_x && x <= bottom_x + btn_w {
                return SettingsAction::SetLayer(crate::types::WindowLayer::Bottom);
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
                // Keep some vertical things proportional to height, but clamp aspect ratio distortion manually if needed
                let sy_val = |val: u32| -> u32 { (val as f32 * sy) as u32 };

                // Colors
                let bg_color = 0x00F6F7F9; // Light Grey/Blue
                let card_bg = 0x00FFFFFF;
                let primary = 0x00FB7299; // Pink
                let text_main = 0x0018191C;
                let text_sec = 0x009499A0;
                let sb_bg = 0x00FFFFFF; // Sidebar White

                // 1. Background
                buffer.fill(bg_color);

                // 2. Sidebar (Left 80px -> Scaled)
                let sb_w = s(100);
                Self::draw_rect(&mut buffer, w, 0, 0, sb_w, h, sb_bg, w, h);

                // Sidebar Text Labels
                if let Some(font) = &self.font {
                    // Logo/Brand area
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
                    let menu_items = vec!["Home", "General", "About"];
                    for (i, item) in menu_items.iter().enumerate() {
                        let my = sy_val(100 + i as u32 * 60);
                        let col = if i == 1 { primary } else { text_sec }; // Mock active state
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

                        if i == 1 {
                            // Active indicator line
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

                // 3. Header (Top)
                // Title
                if let Some(font) = &self.font {
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        "Appearance",
                        s(120),
                        sy_val(30),
                        24.0 * sx,
                        text_main,
                    );
                    Self::draw_text(
                        &mut buffer,
                        w,
                        font,
                        "Customize your pet's look",
                        s(120),
                        sy_val(65),
                        14.0 * sx,
                        text_sec,
                    );
                }

                // 4. Content Cards

                // Card 1: Scale
                let card1_y = sy_val(100);
                let card_w = s(440);
                let card_h = sy_val(120); // Height increased for buttons
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

                // Scale Options (Discrete Buttons)
                let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
                let scale_labels = vec!["0.5x", "0.75x", "1.0x", "1.25x", "1.5x"];
                // Align with hit testing logic: starts at 120 (scaled)
                let s_btn_y = card1_y + sy_val(50);
                let s_btn_w = s(60);
                let s_btn_h = sy_val(40);
                let s_gap = s(10);

                for (i, &val) in scales.iter().enumerate() {
                    let mx = s(120) + i as u32 * (s_btn_w + s_gap);
                    let is_active = (current_scale - val).abs() < 0.01;

                    let bg_col = if is_active { primary } else { 0x00F1F2F3 }; // Active pink, inactive light grey
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
                        // Center text roughly
                        let label = scale_labels[i];
                        let text_x = mx + (8.0 * sx) as u32; // manual tweak
                        let text_y = s_btn_y + (10.0 * sy) as u32;
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            label,
                            text_x,
                            text_y,
                            14.0 * sx,
                            text_col,
                        );
                    }
                }

                // Card 2: Behavior Mode
                let card2_y = sy_val(240);
                let card2_h = sy_val(120);
                Self::draw_rounded_rect(
                    &mut buffer,
                    w,
                    s(120),
                    card2_y,
                    card_w,
                    card2_h,
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
                }

                // Mode Options
                let modes = vec!["Quiet", "Active", "Clingy"];
                let btn_w = s(120);
                let btn_h = sy_val(50);
                let gap = s(12);

                for (i, mode) in modes.iter().enumerate() {
                    let mx = s(140) + i as u32 * (btn_w + gap);
                    let my = card2_y + sy_val(50);

                    let is_active = *mode == current_mode;
                    let border_col = if is_active { primary } else { 0x00E3E5E7 };

                    // Border
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        mx,
                        my,
                        btn_w,
                        btn_h,
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
                        btn_w - 4,
                        btn_h - 4,
                        6,
                        card_bg,
                        w,
                        h,
                    );

                    if is_active {
                        // Checkmark
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            mx + btn_w - 20,
                            my + 5,
                            12,
                            12,
                            6,
                            primary,
                            w,
                            h,
                        );
                    }

                    let text_col = if is_active { primary } else { text_sec };
                    if let Some(font) = &self.font {
                        let text_x = mx + (20.0 * sx) as u32;
                        let text_y = my + (18.0 * sy) as u32;
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            mode,
                            text_x,
                            text_y,
                            14.0 * sx,
                            text_col,
                        );
                    }
                }

                // Card 3: Music Path
                let card3_y = sy_val(380);
                let card3_h = sy_val(120);
                Self::draw_rounded_rect(
                    &mut buffer,
                    w,
                    s(120),
                    card3_y,
                    card_w,
                    card3_h,
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
                }

                // Path selection button
                let p_btn_y = card3_y + sy_val(50);
                let p_btn_w = s(400);
                let p_btn_h = sy_val(40);
                Self::draw_rounded_rect(
                    &mut buffer,
                    w,
                    s(140),
                    p_btn_y,
                    p_btn_w,
                    p_btn_h,
                    8,
                    0x00E3E5E7, // Slightly darker, more "clickable"
                    w,
                    h,
                );
                // Add border
                Self::draw_rounded_rect(
                    &mut buffer,
                    w,
                    s(140) + 1,
                    p_btn_y + 1,
                    p_btn_w - 2,
                    p_btn_h - 2,
                    7,
                    card_bg,
                    w,
                    h,
                );

                let path_text = current_music_path
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Click to select a music folder...".to_string());
                if let Some(font) = &self.font {
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

                // Card 4: Window Layer
                let card4_y = sy_val(520);
                let card4_h = sy_val(120);
                Self::draw_rounded_rect(
                    &mut buffer,
                    w,
                    s(120),
                    card4_y,
                    card_w,
                    card4_h,
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
                }

                let layers = vec![
                    ("Always Top", crate::types::WindowLayer::Top),
                    ("Desktop", crate::types::WindowLayer::Bottom),
                ];
                let btn_w = s(120);
                let btn_h = sy_val(50);
                let gap = s(12);

                for (i, (label, layer)) in layers.iter().enumerate() {
                    let mx = s(140) + i as u32 * (btn_w + gap);
                    let my = card4_y + sy_val(50);

                    let is_active = *layer == current_layer;
                    let border_col = if is_active { primary } else { 0x00E3E5E7 };

                    // Border
                    Self::draw_rounded_rect(
                        &mut buffer,
                        w,
                        mx,
                        my,
                        btn_w,
                        btn_h,
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
                        btn_w - 4,
                        btn_h - 4,
                        6,
                        card_bg,
                        w,
                        h,
                    );

                    if is_active {
                        Self::draw_rounded_rect(
                            &mut buffer,
                            w,
                            mx + btn_w - 20,
                            my + 5,
                            12,
                            12,
                            6,
                            primary,
                            w,
                            h,
                        );
                    }

                    let text_col = if is_active { primary } else { text_sec };
                    if let Some(font) = &self.font {
                        Self::draw_text(
                            &mut buffer,
                            w,
                            font,
                            label,
                            mx + (20.0 * sx) as u32,
                            my + (18.0 * sy) as u32,
                            14.0 * sx,
                            text_col,
                        );
                    }
                }

                buffer.present().unwrap();
            }
        }
    }
}
