use rusttype::{point, Font, Scale};
use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::{
    dpi::{LogicalPosition, PhysicalSize},
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::EventLoopWindowTarget,
    keyboard::{Key, NamedKey},
    window::{Window, WindowBuilder, WindowLevel},
};

pub struct ChatWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    font: Font<'static>,
    input_text: String,
    is_visible: bool,
    last_size: Option<(u32, u32)>,
    cursor_blink_start: std::time::Instant,
}

pub enum ChatAction {
    None,
    Send(String),
    Close,
}

impl ChatWindow {
    pub fn new<T>(
        event_loop: &EventLoopWindowTarget<T>,
        icon: Option<winit::window::Icon>,
    ) -> Self {
        let window = WindowBuilder::new()
            .with_title("Ameath Chat")
            .with_inner_size(PhysicalSize::new(600, 60)) // Wider size: 600
            .with_decorations(false) // No title bar
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_transparent(true)
            .with_window_icon(icon)
            .build(event_loop)
            .unwrap();

        // Enable IME once at start to avoid lag when toggling
        window.set_ime_allowed(true);

        let window = Rc::new(window);
        let context = Context::new(window.clone()).unwrap();
        let surface = Surface::new(&context, window.clone()).unwrap();

        // Load Font (Microsoft YaHei) like settings
        let font_data =
            std::fs::read("C:\\Windows\\Fonts\\msyh.ttc").expect("Failed to load msyh.ttc");
        let font = Font::try_from_vec(font_data).expect("Error constructing Font");

        Self {
            window,
            context,
            surface,
            font,
            input_text: String::new(),
            is_visible: false,
            last_size: None,
            cursor_blink_start: std::time::Instant::now(),
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    pub fn next_blink_at(&self) -> std::time::Instant {
        let elapsed_ms = self.cursor_blink_start.elapsed().as_millis();
        let current_step = elapsed_ms / 500;
        let next_step = current_step + 1;
        self.cursor_blink_start + std::time::Duration::from_millis((next_step * 500) as u64)
    }

    pub fn show(&mut self, position: LogicalPosition<f64>) {
        self.window.set_visible(true);
        self.window.focus_window();

        // Position near the pet
        self.window.set_outer_position(position);

        self.is_visible = true;
        self.input_text.clear();
        self.cursor_blink_start = std::time::Instant::now();
        self.request_redraw();
    }

    pub fn hide(&mut self) {
        self.window.set_visible(false);
        self.is_visible = false;
    }

    pub fn request_redraw(&self) {
        if self.is_visible {
            self.window.request_redraw();
        }
    }

    pub fn request_redraw_actual(&self) {
        self.window.request_redraw();
    }

    pub fn handle_event(
        &mut self,
        event: &WindowEvent,
        modifiers: winit::keyboard::ModifiersState,
    ) -> ChatAction {
        match event {
            WindowEvent::Ime(ime) => match ime {
                winit::event::Ime::Commit(text) => {
                    self.input_text.push_str(text);
                    self.cursor_blink_start = std::time::Instant::now();
                    self.request_redraw();
                }
                _ => {}
            },
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                let _ = self.window.drag_window();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        state: ElementState::Pressed,
                        logical_key,
                        ..
                    },
                ..
            } => {
                match logical_key {
                    Key::Named(NamedKey::Enter) => {
                        // Send message
                        if !self.input_text.trim().is_empty() {
                            let msg = self.input_text.clone();
                            self.input_text.clear();
                            // Don't close on send, keep open for more chat
                            // self.hide();
                            self.request_redraw();
                            return ChatAction::Send(msg);
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.hide();
                        return ChatAction::Close;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        self.input_text.pop();
                        self.cursor_blink_start = std::time::Instant::now();
                        self.request_redraw();
                    }
                    Key::Character(c) => {
                        let has_ctrl = modifiers.control_key() || modifiers.super_key();
                        if c == "v" && has_ctrl {
                            #[cfg(target_os = "windows")]
                            {
                                use arboard::Clipboard;
                                if let Ok(mut clipboard) = Clipboard::new() {
                                    if let Ok(text) = clipboard.get_text() {
                                        let trimmed = text.trim();
                                        self.input_text.push_str(trimmed);
                                        self.request_redraw();
                                    }
                                }
                            }
                            return ChatAction::None;
                        }

                        // Filter control characters and Alt combinations (to prevent hotkey leakage)
                        if !c.chars().any(|ch| ch.is_control()) && !modifiers.alt_key() {
                            self.input_text.push_str(c);
                            self.cursor_blink_start = std::time::Instant::now();
                            self.request_redraw();
                        }
                    }
                    Key::Named(NamedKey::Space) => {
                        self.input_text.push(' ');
                        self.cursor_blink_start = std::time::Instant::now();
                        self.request_redraw();
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            _ => {}
        }
        ChatAction::None
    }

    fn redraw(&mut self) {
        // 1. Calculate layout first to determine needed height
        let scale = Scale::uniform(24.0);
        let v_metrics = self.font.v_metrics(scale);
        let padding = 10.0;
        let line_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let max_width = 600.0 - (padding * 2.0); // 580.0

        // Wrap text
        let mut lines = Vec::new();
        let mut current_line = String::new();
        let mut current_width = 0.0;

        for c in self.input_text.chars() {
            let glyph = self.font.glyph(c).scaled(scale);
            let h_metrics = glyph.h_metrics();
            let advance = h_metrics.advance_width;

            if current_width + advance > max_width {
                lines.push(current_line);
                current_line = String::new();
                current_width = 0.0;
            }
            current_line.push(c);
            current_width += advance;
        }
        lines.push(current_line); // Push last line

        let num_lines = lines.len().max(1);
        let content_height = (num_lines as f32 * line_height) + (padding * 2.0);
        let target_height = content_height.max(60.0) as u32;

        // 2. Resize window if needed
        let current_size = self.window.inner_size();
        if current_size.height != target_height {
            let _ = self
                .window
                .request_inner_size(PhysicalSize::new(600, target_height));
            // Return early, let the next resize event trigger redraw to avoid flickering/race
            // or just continue drawing to the new surface size if immediate
            self.surface
                .resize(
                    NonZeroU32::new(600).unwrap(),
                    NonZeroU32::new(target_height).unwrap(),
                )
                .unwrap();
            self.last_size = Some((600, target_height));
        } else if self.last_size != Some((current_size.width, current_size.height)) {
            self.surface
                .resize(
                    NonZeroU32::new(current_size.width).unwrap(),
                    NonZeroU32::new(current_size.height).unwrap(),
                )
                .unwrap();
            self.last_size = Some((current_size.width, current_size.height));
        }

        let mut buffer = self.surface.buffer_mut().unwrap();
        let width = 600; // Fixed width logic for buffer
        let height = target_height as usize;

        if buffer.len() != width * height {
            // Surface resize might not have propagated to buffer len yet if we just resized?
            // Actually surface.resize should handle it. match width/height to buffer len just in case
            // Or trust the target_height
        }

        // Safety check for buffer size vs loop limits
        let buf_w = width;
        let buf_h = height;

        // Colors
        let bg_color = 0xFF2D2D2D; // Dark grey
        let border_color = 0xFFFFFFFF; // White
        let text_color = 0xFFFFFFFF;
        let cursor_color = 0xFF00FF00; // Green cursor

        // Fill background
        buffer.fill(0);

        for y in 0..buf_h {
            for x in 0..buf_w {
                // Border
                if x < 2 || x >= buf_w - 2 || y < 2 || y >= buf_h - 2 {
                    buffer[y * buf_w + x] = border_color;
                } else {
                    buffer[y * buf_w + x] = bg_color;
                }
            }
        }

        // Draw text
        for (i, line) in lines.iter().enumerate() {
            let y_pos = padding + v_metrics.ascent + (i as f32 * line_height);
            let offset = point(padding, y_pos);

            let glyphs: Vec<_> = self.font.layout(line, scale, offset).collect();
            for glyph in glyphs {
                if let Some(bb) = glyph.pixel_bounding_box() {
                    glyph.draw(|x, y, v| {
                        let px = x as i32 + bb.min.x;
                        let py = y as i32 + bb.min.y;
                        if v > 0.5 && px >= 0 && px < buf_w as i32 && py >= 0 && py < buf_h as i32 {
                            buffer[py as usize * buf_w + px as usize] = text_color;
                        }
                    });
                }
            }
        }

        // Draw blinking cursor at end of last line
        let last_line_idx = lines.len() - 1;
        let last_line = &lines[last_line_idx];

        // Calculate cursor_x correctly even for spaces
        let mut cursor_x_accum = padding;
        for c in last_line.chars() {
            let glyph = self.font.glyph(c).scaled(scale);
            cursor_x_accum += glyph.h_metrics().advance_width;
        }
        let cursor_x = cursor_x_accum as i32 + 2;

        let cursor_h = 24; // approx line height
        let cursor_y = (padding + (last_line_idx as f32 * line_height)) as i32;

        // Use winit's IME positioning
        self.window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(cursor_x as f64, cursor_y as f64),
            winit::dpi::PhysicalSize::new(2.0, cursor_h as f64),
        );

        let elapsed = self.cursor_blink_start.elapsed().as_millis();
        let cursor_visible = (elapsed % 1000) < 500;

        if cursor_visible {
            for y in cursor_y..(cursor_y + cursor_h) {
                for x in cursor_x..(cursor_x + 2) {
                    if x < buf_w as i32 && y < buf_h as i32 {
                        let idx = y as usize * buf_w + x as usize;
                        if idx < buffer.len() {
                            buffer[idx] = cursor_color;
                        }
                    }
                }
            }
        }

        buffer.present().unwrap();
    }
}
