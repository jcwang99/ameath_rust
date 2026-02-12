use softbuffer::{Context, Surface};
use std::num::NonZeroU32;
use std::rc::Rc;
use winit::{dpi::PhysicalSize, event_loop::EventLoopWindowTarget, window::Window};

pub struct SettingsWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
}

impl SettingsWindow {
    pub fn new(event_loop: &EventLoopWindowTarget<()>) -> Self {
        let window = Rc::new(
            winit::window::WindowBuilder::new()
                .with_title("Ameath Settings")
                .with_inner_size(PhysicalSize::new(400, 300))
                .with_resizable(false)
                .build(event_loop)
                .unwrap(),
        );

        let context = Context::new(window.clone()).unwrap();
        let surface = Surface::new(&context, window.clone()).unwrap();

        Self {
            window,
            context,
            surface,
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        self.window.focus_window();
    }

    pub fn redraw(&mut self) {
        let size = self.window.inner_size();
        if let Some(width) = NonZeroU32::new(size.width) {
            if let Some(height) = NonZeroU32::new(size.height) {
                // Resize surface if needed
                let _ = self.surface.resize(width, height);

                let mut buffer = self.surface.buffer_mut().unwrap();

                // Fill with white background
                buffer.fill(0xFFFFFFFF);

                // Simple UI rendering (Placeholder)
                // Draw a header bar
                let w = width.get();
                // let h = height.get();

                for i in 0..buffer.len() {
                    let y = i as u32 / w;
                    // let x = i as u32 % w;

                    if y < 40 {
                        // Header: Dark Purple
                        buffer[i] = 0x00412C5E;
                    } else {
                        // Body: Lavender/White
                        buffer[i] = 0x00FAF5FF;
                    }
                }

                // TODO: Add buttons for configuration

                buffer.present().unwrap();
            }
        }
    }
}
