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
}

pub enum ChatAction {
    None,
    Send(String),
    Close,
}

impl ChatWindow {
    pub fn new<T>(event_loop: &EventLoopWindowTarget<T>) -> Self {
        let window = WindowBuilder::new()
            .with_title("Ameath Chat")
            .with_inner_size(PhysicalSize::new(300, 60)) // Compact size
            .with_decorations(false) // No title bar
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_transparent(true)
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
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn show(&mut self, position: LogicalPosition<f64>) {
        self.window.set_visible(true);
        self.window.focus_window();

        // Position near the pet
        self.window.set_outer_position(position);

        self.is_visible = true;
        self.input_text.clear();
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

    pub fn handle_event(&mut self, event: &WindowEvent) -> ChatAction {
        match event {
            WindowEvent::Ime(ime) => match ime {
                winit::event::Ime::Commit(text) => {
                    self.input_text.push_str(text);
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
                            self.hide();
                            return ChatAction::Send(msg);
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.hide();
                        return ChatAction::Close;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        self.input_text.pop();
                        self.request_redraw();
                    }
                    Key::Character(c) => {
                        // Filter control characters
                        if !c.chars().any(|ch| ch.is_control()) {
                            self.input_text.push_str(c);
                            self.request_redraw();
                        }
                    }
                    Key::Named(NamedKey::Space) => {
                        self.input_text.push(' ');
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
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }

        let new_size = (size.width, size.height);
        if self.last_size != Some(new_size) {
            self.surface
                .resize(
                    NonZeroU32::new(size.width).unwrap(),
                    NonZeroU32::new(size.height).unwrap(),
                )
                .unwrap();
            self.last_size = Some(new_size);
        }

        let mut buffer = self.surface.buffer_mut().unwrap();
        let width = size.width as usize;
        let height = size.height as usize;

        // Colors
        let bg_color = 0xFF2D2D2D; // Dark grey
        let border_color = 0xFFFFFFFF; // White
        let text_color = 0xFFFFFFFF;
        let cursor_color = 0xFF00FF00; // Green cursor

        // Fill background
        buffer.fill(0);

        for y in 0..height {
            for x in 0..width {
                // Border
                if x < 2 || x >= width - 2 || y < 2 || y >= height - 2 {
                    buffer[y * width + x] = border_color;
                } else {
                    buffer[y * width + x] = bg_color;
                }
            }
        }

        // Draw text
        let scale = Scale::uniform(24.0);
        let v_metrics = self.font.v_metrics(scale);
        let offset = point(10.0, v_metrics.ascent + 10.0);

        // Draw input text
        let glyphs: Vec<_> = self.font.layout(&self.input_text, scale, offset).collect();
        for glyph in glyphs {
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|x, y, v| {
                    let px = x as i32 + bb.min.x;
                    let py = y as i32 + bb.min.y;
                    if v > 0.5 && px >= 0 && px < width as i32 && py >= 0 && py < height as i32 {
                        buffer[py as usize * width + px as usize] = text_color;
                    }
                });
            }
        }

        // Draw blinking cursor (simple implementation: always draw for now)
        // Would handle blinking in main loop via timer
        let cursor_x = if let Some(last) = self.font.layout(&self.input_text, scale, offset).last()
        {
            last.pixel_bounding_box().map(|bb| bb.max.x).unwrap_or(10) + 2
        } else {
            10
        };

        let cursor_h = 20;
        let cursor_y = 15;

        // Use winit's IME positioning
        self.window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(cursor_x as f64, cursor_y as f64),
            winit::dpi::PhysicalSize::new(2.0, cursor_h as f64),
        );

        for y in cursor_y..(cursor_y + cursor_h) {
            for x in cursor_x..(cursor_x + 2) {
                if x < width as i32 && y < height as i32 {
                    buffer[y as usize * width + x as usize] = cursor_color;
                }
            }
        }

        buffer.present().unwrap();
    }
}
