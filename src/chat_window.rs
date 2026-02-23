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

#[derive(Clone)]
pub struct Thumbnail {
    pub pixels: Vec<u32>,
    pub width: u32,
    pub height: u32,
}

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
    pub pending_images: Vec<crate::types::ImageData>,
    pub thumbnails: Vec<Thumbnail>, // Fixed: Store explicit dimensions
    plus_button_hovered: bool,
    hovered_thumb: Option<usize>,
    mouse_pos: (f64, f64),
    // Async channel for image results
    image_rx: std::sync::mpsc::Receiver<(crate::types::ImageData, Thumbnail)>,
    image_tx: std::sync::mpsc::Sender<(crate::types::ImageData, Thumbnail)>,
    proxy: winit::event_loop::EventLoopProxy<()>,
}

pub enum ChatAction {
    None,
    Send(crate::types::ChatInput),
    Close,
}

impl ChatWindow {
    pub fn new<T>(
        event_loop: &EventLoopWindowTarget<T>,
        proxy: winit::event_loop::EventLoopProxy<()>,
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

        let (image_tx, image_rx) = std::sync::mpsc::channel();

        Self {
            window,
            context,
            surface,
            font,
            input_text: String::new(),
            is_visible: false,
            last_size: None,
            cursor_blink_start: std::time::Instant::now(),
            pending_images: Vec::new(),
            thumbnails: Vec::new(),
            plus_button_hovered: false,
            hovered_thumb: None,
            mouse_pos: (0.0, 0.0),
            image_tx,
            image_rx,
            proxy,
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
        self.pending_images.clear();
        self.thumbnails.clear();
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
        // Poll for async image results
        let mut got_new_images = false;
        while let Ok((img_data, thumb)) = self.image_rx.try_recv() {
            self.pending_images.push(img_data);
            self.thumbnails.push(thumb);
            got_new_images = true;
        }
        if got_new_images {
            self.request_redraw();
        }

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
                if self.plus_button_hovered {
                    self.trigger_upload();
                } else if let Some(idx) = self.get_thumbnail_at_mouse() {
                    self.remove_image(idx);
                } else {
                    let _ = self.window.drag_window();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x, position.y);
                self.update_hover_states();
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
                        if !self.input_text.trim().is_empty() || !self.pending_images.is_empty() {
                            let msg = crate::types::ChatInput {
                                text: self.input_text.clone(),
                                images: self.pending_images.clone(),
                            };
                            self.input_text.clear();
                            self.pending_images.clear();
                            self.thumbnails.clear();
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
                        let c_lower = c.to_lowercase();
                        let has_ctrl = modifiers.control_key() || modifiers.super_key();
                        if c_lower == "v" && has_ctrl {
                            self.handle_paste();
                            self.cursor_blink_start = std::time::Instant::now(); // Add blink reset to paste
                            return ChatAction::None;
                        }

                        if c_lower == "u" && has_ctrl {
                            self.trigger_upload();
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
            WindowEvent::HoveredFile(_) => {
                // Potential visual feedback for drop target could go here
                self.request_redraw();
            }
            WindowEvent::DroppedFile(path) => {
                self.add_image_from_path(path.clone());
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
            }
            _ => {}
        }
        ChatAction::None
    }

    fn update_hover_states(&mut self) {
        let (mx, my) = self.mouse_pos;
        let window_height = self.window.inner_size().height as f64;

        // Plus button is now at bottom-left, below text
        let btn_size = 32.0;
        let btn_x = 10.0;
        let btn_y = window_height - 5.0 - btn_size; // Margin from bottom

        self.plus_button_hovered =
            mx >= btn_x && mx <= btn_x + btn_size && my >= btn_y && my <= btn_y + btn_size;

        self.hovered_thumb = self.get_thumbnail_at_mouse();

        self.request_redraw();
    }

    fn handle_paste(&mut self) {
        #[cfg(target_os = "windows")]
        {
            use arboard::Clipboard;
            if let Ok(mut clipboard) = Clipboard::new() {
                println!("[Clipboard Debug] Attempting to paste...");

                // 1. Try Image (arboard)
                match clipboard.get_image() {
                    Ok(img) => {
                        println!(
                            "[Clipboard Debug] arboard success: {}x{}",
                            img.width, img.height
                        );
                        self.process_raw_image(
                            img.width as u32,
                            img.height as u32,
                            img.bytes.to_vec(),
                        );
                        return;
                    }
                    Err(e) => println!("[Clipboard Debug] arboard image check failed: {:?}", e),
                }

                #[cfg(target_os = "windows")]
                {
                    use clipboard_win::{formats, get_clipboard};
                    // Try DIB (Device Independent Bitmap) which is what PixPin uses
                    if let Ok(dib_data) =
                        get_clipboard::<Vec<u8>, _>(formats::RawData(formats::CF_DIB))
                    {
                        println!(
                            "[Clipboard Debug] clipboard-win found DIB data, size={}",
                            dib_data.len()
                        );

                        // DIB data starts with BITMAPINFOHEADER (usually 40 bytes)
                        // We wrap it in a 14-byte BMP file header to make it a valid .bmp image
                        let mut bmp_file = Vec::with_capacity(14 + dib_data.len());
                        bmp_file.extend_from_slice(b"BM");
                        // File size
                        bmp_file.extend_from_slice(&((14 + dib_data.len()) as u32).to_le_bytes());
                        bmp_file.extend_from_slice(&0u16.to_le_bytes()); // Reserved
                        bmp_file.extend_from_slice(&0u16.to_le_bytes()); // Reserved

                        // Offset to pixel data
                        let header_size = if dib_data.len() >= 4 {
                            u32::from_le_bytes([dib_data[0], dib_data[1], dib_data[2], dib_data[3]])
                        } else {
                            40
                        };

                        // For DIB, the color table (if any) follows the header.
                        // For 24/32bpp, there's usually no color table.
                        bmp_file.extend_from_slice(&(14 + header_size).to_le_bytes());
                        bmp_file.extend_from_slice(&dib_data);

                        if let Ok(img) =
                            image::load_from_memory_with_format(&bmp_file, image::ImageFormat::Bmp)
                        {
                            println!("[Clipboard Debug] Successfully decoded PixPin DIB");
                            let rgba = img.to_rgba8();
                            self.process_raw_image(rgba.width(), rgba.height(), rgba.into_raw());
                            return;
                        }
                    }
                }

                // 3. Try Text (Path or raw text)
                match clipboard.get_text() {
                    Ok(text) => {
                        println!("[Clipboard Debug] Success: Found text (len={})", text.len());
                        let trimmed = text.trim();

                        // Path detection
                        let path = std::path::Path::new(trimmed);
                        if path.exists() && path.is_file() {
                            let ok_exts = ["png", "jpg", "jpeg", "webp", "gif"];
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            if ok_exts.contains(&ext.as_str()) {
                                self.add_image_from_path(path.to_path_buf());
                                return;
                            }
                        }

                        self.input_text.push_str(trimmed);
                        self.cursor_blink_start = std::time::Instant::now();
                        self.request_redraw();
                        return; // Return after text is processed
                    }
                    Err(e) => println!("[Clipboard Debug] Text check failed: {:?}", e),
                }

                println!("[Clipboard Debug] No supported format found after full check.");
            }
        }
    }

    fn process_raw_image(&mut self, w: u32, h: u32, bytes: Vec<u8>) {
        let tx = self.image_tx.clone();
        let proxy = self.proxy.clone();

        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(img_buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, bytes) {
                let mut cursor = std::io::Cursor::new(&mut buffer);
                if img_buf
                    .write_to(&mut cursor, image::ImageFormat::Png)
                    .is_ok()
                {
                    let img_data = crate::types::ImageData {
                        data: buffer,
                        mime_type: "image/png".to_string(),
                    };

                    let thumb = image::imageops::thumbnail(&img_buf, 80, 80);
                    let thumb_u32 = thumb
                        .pixels()
                        .map(|p| {
                            ((p[3] as u32) << 24)
                                | ((p[0] as u32) << 16)
                                | ((p[1] as u32) << 8)
                                | (p[2] as u32)
                        })
                        .collect();
                    let thumb_obj = Thumbnail {
                        pixels: thumb_u32,
                        width: thumb.width(),
                        height: thumb.height(),
                    };

                    let _ = tx.send((img_data, thumb_obj));
                    let _ = proxy.send_event(());
                }
            }
        });
    }

    fn get_thumbnail_at_mouse(&self) -> Option<usize> {
        if self.thumbnails.is_empty() {
            return None;
        }
        let (mx, my) = self.mouse_pos;
        let start_y = 10.0;
        let start_x = 10.0; // This is where the thumbnails usually start if they were on top
                            // Actually, in the current layout, start_x for thumbnails is start_x += T_SIZE + 10;?
                            // No, thumbnails are at top: start_y = 10, start_x = 10.

        let spacing = 10.0;

        for i in 0..self.thumbnails.len() {
            let thumb = &self.thumbnails[i];
            let t_w = thumb.width as f64;
            let t_h = thumb.height as f64;
            let tx = start_x + (i as f64 * (80.0 + spacing)); // Use 80.0 as fixed visual slot width
            if mx >= tx && mx <= tx + t_w && my >= start_y && my <= start_y + t_h {
                return Some(i);
            }
        }
        None
    }

    fn remove_image(&mut self, index: usize) {
        if index < self.pending_images.len() {
            self.pending_images.remove(index);
            self.thumbnails.remove(index);
            self.request_redraw();
        }
    }

    fn trigger_upload(&mut self) {
        let tx = self.image_tx.clone();
        let proxy = self.proxy.clone();

        // Spawn FileDialog in a background thread because it's a blocking call on Windows
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
                .pick_files();

            if let Some(files) = picked {
                for path in files {
                    Self::process_image_async(path, tx.clone(), proxy.clone());
                }
            }
        });
    }

    fn add_image_from_path(&mut self, path: std::path::PathBuf) {
        Self::process_image_async(path, self.image_tx.clone(), self.proxy.clone());
    }

    fn process_image_async(
        path: std::path::PathBuf,
        tx: std::sync::mpsc::Sender<(crate::types::ImageData, Thumbnail)>,
        proxy: winit::event_loop::EventLoopProxy<()>,
    ) {
        std::thread::spawn(move || {
            let ok_exts = ["png", "jpg", "jpeg", "webp", "gif"];
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            if !ok_exts.contains(&ext.as_str()) {
                return;
            }

            if let Ok(data) = std::fs::read(&path) {
                let mime = format!("image/{}", ext);
                let img_data = crate::types::ImageData {
                    data: data.clone(),
                    mime_type: mime,
                };

                // Create thumbnail from data
                if let Ok(img) = image::load_from_memory(&data) {
                    let thumb = img.thumbnail(80, 80);
                    let thumb_rgba = thumb.to_rgba8();
                    let thumb_u32 = thumb_rgba
                        .pixels()
                        .map(|p| {
                            ((p[3] as u32) << 24)
                                | ((p[0] as u32) << 16)
                                | ((p[1] as u32) << 8)
                                | (p[2] as u32)
                        })
                        .collect();
                    let thumb_obj = Thumbnail {
                        pixels: thumb_u32,
                        width: thumb_rgba.width(),
                        height: thumb_rgba.height(),
                    };

                    let _ = tx.send((img_data, thumb_obj));
                    // Wake up the event loop to poll results
                    let _ = proxy.send_event(());
                }
            }
        });
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

        // Dynamic height calculation
        let text_h = (lines.len() as f32 * line_height).max(line_height) as u32;
        let thumbnail_h = if self.thumbnails.is_empty() { 0 } else { 100 };
        let button_row_h = 40; // Explicit space for the plus button at the bottom
        let padding_total = (padding * 2.0) as u32;

        let target_height = padding_total + thumbnail_h + text_h + button_row_h;

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
        let border_color = 0xFF444444; // Subtle dark border
        let text_color = 0xFFFFFFFF;
        let cursor_color = 0xFF00FF00; // Green cursor

        // Fill background
        buffer.fill(0);

        // Draw rounded window background
        let win_r: i32 = 12;
        let win_r_sq = win_r * win_r;
        for y in 0..buf_h {
            for x in 0..buf_w {
                let mut draw_bg = true;
                let dx = if x < win_r as usize {
                    (win_r as usize - x) as i32
                } else if x > (buf_w - win_r as usize - 1) {
                    (x - (buf_w - win_r as usize - 1)) as i32
                } else {
                    0
                };
                let dy = if y < win_r as usize {
                    (win_r as usize - y) as i32
                } else if y > (buf_h - win_r as usize - 1) {
                    (y - (buf_h - win_r as usize - 1)) as i32
                } else {
                    0
                };

                if dx > 0 && dy > 0 && dx * dx + dy * dy > win_r_sq {
                    draw_bg = false;
                }

                if draw_bg {
                    // Subtle border (1px)
                    if x == 0 || x == buf_w - 1 || y == 0 || y == buf_h - 1 {
                        buffer[y * buf_w + x] = border_color;
                    } else {
                        buffer[y * buf_w + x] = bg_color;
                    }
                }
            }
        }

        // Draw thumbnails (at top if present)
        let start_y = 10;
        let mut start_x = 10;

        if !self.thumbnails.is_empty() {
            for (i, thumb_obj) in self.thumbnails.iter().enumerate() {
                let t_w = thumb_obj.width;
                let t_h = thumb_obj.height;

                // Draw thumbnail with rounding
                let r: i32 = 8;
                let r_sq = r * r;
                let is_hovered = self.hovered_thumb == Some(i);

                for ty in 0..t_h {
                    for tx in 0..t_w {
                        let px = start_x + tx as usize;
                        let py = start_y + ty as usize;

                        let mut draw = true;
                        let dx = if tx < r as u32 {
                            r as i32 - tx as i32
                        } else if tx > (t_w - r as u32 - 1) {
                            tx as i32 - (t_w as i32 - r as i32 - 1)
                        } else {
                            0
                        };
                        let dy = if ty < r as u32 {
                            r as i32 - ty as i32
                        } else if ty > (t_h - r as u32 - 1) {
                            ty as i32 - (t_h as i32 - r as i32 - 1)
                        } else {
                            0
                        };

                        if dx > 0 && dy > 0 && dx * dx + dy * dy > r_sq {
                            draw = false;
                        }

                        if draw && px < buf_w && py < buf_h {
                            let mut color =
                                thumb_obj.pixels[ty as usize * t_w as usize + tx as usize];

                            // Visual feedback for deleting
                            if is_hovered {
                                let mut r_val = (color >> 16) & 0xFF;
                                let mut g_val = (color >> 8) & 0xFF;
                                let mut b_val = color & 0xFF;

                                // Make it more red and translucent
                                r_val = (r_val as f32 * 0.5 + 255.0 * 0.5) as u32;
                                g_val = (g_val as f32 * 0.5) as u32;
                                b_val = (b_val as f32 * 0.5) as u32;
                                color = (0xFF << 24) | (r_val << 16) | (g_val << 8) | b_val;

                                // Draw an "X" mark (centered in whatever the actual thumb size is)
                                let is_x = (tx as i32 - ty as i32 + (t_h as i32 - t_w as i32) / 2)
                                    .abs()
                                    < 2
                                    || (tx as i32 + ty as i32 - ((t_w + t_h) as i32 / 2 - 1)).abs()
                                        < 2;

                                if is_x
                                    && tx > t_w / 4
                                    && tx < 3 * t_w / 4
                                    && ty > t_h / 4
                                    && ty < 3 * t_h / 4
                                {
                                    color = 0xFFFFFFFF;
                                }
                            }

                            let alpha = (color >> 24) & 0xFF;
                            if alpha > 0 {
                                buffer[py * buf_w + px] = color;
                            }
                        }
                    }
                }
                start_x += t_w as usize + 10;
                if start_x + 80 > buf_w {
                    // Use a safe estimate of 80 for wrap check
                    break;
                }
            }
        }

        // Draw Plus Button (at bottom-left) - Circular, borderless
        let btn_size = 32;
        let btn_x = 10;
        let btn_y = buf_h - 10 - btn_size;

        let plus_bg = if self.plus_button_hovered {
            0xFF444444
        } else {
            0xFF3D3D3D
        };

        let radius = btn_size as i32 / 2;
        let r_sq = radius * radius;
        let cx = btn_size as i32 / 2;
        let cy = btn_size as i32 / 2;

        for ty in 0..btn_size {
            for tx in 0..btn_size {
                let dx = tx as i32 - cx;
                let dy = ty as i32 - cy;

                if dx * dx + dy * dy <= r_sq {
                    let px = btn_x + tx;
                    let py = btn_y + ty;
                    if px < buf_w && py < buf_h {
                        // Plus sign centered in circle
                        let is_plus = (tx > 10 && tx < 22 && ty >= 15 && ty <= 16)
                            || (ty > 10 && ty < 22 && tx >= 15 && tx <= 16);

                        if is_plus {
                            buffer[py * buf_w + px] = 0xFFBBBBBB; // Slightly greyish plus as in reference
                        } else {
                            buffer[py * buf_w + px] = plus_bg;
                        }
                    }
                }
            }
        }

        // Draw text (no horizontal offset anymore, starts at padding)
        let text_y_offset = if self.thumbnails.is_empty() {
            0.0
        } else {
            100.0
        };
        let text_x_offset = 0.0;
        for (i, line) in lines.iter().enumerate() {
            let y_pos = padding + v_metrics.ascent + (i as f32 * line_height) + text_y_offset;
            let offset = point(padding + text_x_offset, y_pos);

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
        let mut cursor_x_accum = padding + text_x_offset;
        for c in last_line.chars() {
            let glyph = self.font.glyph(c).scaled(scale);
            cursor_x_accum += glyph.h_metrics().advance_width;
        }
        let cursor_x = cursor_x_accum as i32 + 2;

        let cursor_h = 24; // approx line height
        let cursor_y = (padding + (last_line_idx as f32 * line_height) + text_y_offset) as i32;

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
