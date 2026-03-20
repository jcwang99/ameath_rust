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
use crate::ui_primitives::{draw_rounded_rect_with_border, draw_circle};
use rusttype::PositionedGlyph;

#[derive(Clone)]
pub struct Thumbnail {
    pub pixels: Vec<u32>,
    pub width: u32,
    pub height: u32,
}

pub enum ImageStatus {
    Processing,
    Ready {
        data: crate::types::ImageData,
        thumb: Thumbnail,
    },
}

pub struct ImageSlot {
    pub id: u32,
    pub status: ImageStatus,
}

pub enum ImageAsyncMsg {
    RequestAddition(std::path::PathBuf),
    Finished(u32, crate::types::ImageData, Thumbnail),
    Failed(u32),
}

pub struct ChatWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    font: Font<'static>,
    input_text: String,
    is_visible: bool,
    cursor_blink_start: std::time::Instant,
    pub slots: Vec<ImageSlot>,
    next_slot_id: u32,
    plus_button_hovered: bool,
    hovered_thumb: Option<usize>,
    mouse_pos: (f64, f64),
    // Async channel for image results
    image_rx: std::sync::mpsc::Receiver<ImageAsyncMsg>,
    image_tx: std::sync::mpsc::Sender<ImageAsyncMsg>,
    proxy: winit::event_loop::EventLoopProxy<()>,
    cursor_byte_idx: usize,
    pub selection_start: Option<usize>,
    // Optimization: Cache layout
    cached_layout: Vec<Vec<PositionedGlyph<'static>>>,
    cached_line_heights: Vec<f32>,
    layout_valid: bool,
    text_buffer: Vec<u32>,
    text_buffer_w: u32,
    text_buffer_h: u32,
    ignore_next_char: bool,
    is_selecting: bool,
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
            cursor_blink_start: std::time::Instant::now(),
            slots: Vec::new(),
            next_slot_id: 0,
            plus_button_hovered: false,
            hovered_thumb: None,
            mouse_pos: (0.0, 0.0),
            image_tx,
            image_rx,
            proxy,
            cursor_byte_idx: 0,
            selection_start: None,
            cached_layout: Vec::new(),
            cached_line_heights: Vec::new(),
            layout_valid: false,
            text_buffer: Vec::new(),
            text_buffer_w: 0,
            text_buffer_h: 0,
            ignore_next_char: false,
            is_selecting: false,
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
        self.cursor_byte_idx = 0;
        self.selection_start = None;
        self.slots.clear();
        self.ignore_next_char = true; // Use this to swallow the hotkey leak
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
        while let Ok(msg) = self.image_rx.try_recv() {
            match msg {
                ImageAsyncMsg::RequestAddition(path) => {
                    self.add_image_from_path(path);
                    got_new_images = true;
                }
                ImageAsyncMsg::Finished(id, img_data, thumb) => {
                    if let Some(slot) = self.slots.iter_mut().find(|s| s.id == id) {
                        slot.status = ImageStatus::Ready {
                            data: img_data,
                            thumb,
                        };
                        got_new_images = true;
                    }
                }
                ImageAsyncMsg::Failed(id) => {
                    if let Some(pos) = self.slots.iter().position(|s| s.id == id) {
                        self.slots.remove(pos);
                        got_new_images = true;
                    }
                }
            }
        }
        if got_new_images {
            self.layout_valid = false;
            self.request_redraw();
        }

        match event {
            WindowEvent::Ime(ime) => match ime {
                winit::event::Ime::Commit(text) => {
                    if self.ignore_next_char && (text == "m" || text == "M") {
                        self.ignore_next_char = false;
                        return ChatAction::None;
                    }
                    self.ignore_next_char = false;
                    self.delete_selection();
                    self.input_text.insert_str(self.cursor_byte_idx, text);
                    self.cursor_byte_idx += text.len();
                    self.selection_start = None;
                    self.cursor_blink_start = std::time::Instant::now();
                    self.layout_valid = false;
                    self.request_redraw();
                }
                _ => {}
            },
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if *state == ElementState::Pressed {
                    self.is_selecting = true;
                    if self.plus_button_hovered {
                        self.trigger_upload();
                    } else if let Some(idx) = self.get_thumbnail_at_mouse() {
                        self.remove_image(idx);
                    } else {
                        // Check if click is in text area
                        let padding = 10.0;
                        let text_y_offset = if self.slots.is_empty() { 0.0 } else { 100.0 };
                        let (_mx, my) = self.mouse_pos;
                        let window_size = self.window.inner_size();
                        
                        // Button row height is 40. Text area is roughly between top+offset and bottom-40
                        if my > padding + text_y_offset && my < (window_size.height as f64 - 40.0) {
                            self.set_cursor_at_mouse();
                            self.selection_start = Some(self.cursor_byte_idx);
                        } else {
                            self.selection_start = None;
                            let _ = self.window.drag_window();
                        }
                    }
                } else {
                    // Released
                    self.is_selecting = false;
                    if let Some(start) = self.selection_start {
                        if start == self.cursor_byte_idx {
                            self.selection_start = None;
                        }
                    }
                }
                self.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x, position.y);
                self.update_hover_states();
                if self.is_selecting {
                    self.set_cursor_at_mouse();
                    self.request_redraw();
                }
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
                        let mut ready_images = Vec::new();
                        for slot in &self.slots {
                            if let ImageStatus::Ready { data, .. } = &slot.status {
                                ready_images.push(data.clone());
                            }
                        }

                        if !self.input_text.trim().is_empty() || !ready_images.is_empty() {
                            let msg = crate::types::ChatInput {
                                text: self.input_text.clone(),
                                images: ready_images,
                            };
                            self.input_text.clear();
                            self.cursor_byte_idx = 0;
                            self.slots.clear();
                            self.layout_valid = false;
                            self.request_redraw();
                            return ChatAction::Send(msg);
                        }
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.hide();
                        return ChatAction::Close;
                    }
                    Key::Named(NamedKey::Backspace) => {
                        if self.selection_start.is_some() && self.selection_start != Some(self.cursor_byte_idx) {
                            self.delete_selection();
                            self.selection_start = None;
                            self.cursor_blink_start = std::time::Instant::now();
                            self.layout_valid = false;
                            self.request_redraw();
                        } else if self.cursor_byte_idx > 0 {
                            // Find previous character start
                            if let Some((idx, _)) = self.input_text[..self.cursor_byte_idx]
                                .char_indices()
                                .next_back()
                            {
                                self.input_text.remove(idx);
                                self.cursor_byte_idx = idx;
                                self.cursor_blink_start = std::time::Instant::now();
                                self.layout_valid = false;
                                self.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::ArrowLeft) => {
                        if self.cursor_byte_idx > 0 {
                            if let Some((idx, _)) = self.input_text[..self.cursor_byte_idx]
                                .char_indices()
                                .next_back()
                            {
                                self.cursor_byte_idx = idx;
                                self.selection_start = None;
                                self.cursor_blink_start = std::time::Instant::now();
                                self.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::ArrowRight) => {
                        if self.cursor_byte_idx < self.input_text.len() {
                            if let Some((idx, _c)) = self.input_text[self.cursor_byte_idx..]
                                .char_indices()
                                .nth(1)
                            {
                                self.cursor_byte_idx += idx;
                            } else {
                                self.cursor_byte_idx = self.input_text.len();
                            }
                            self.selection_start = None;
                            self.cursor_blink_start = std::time::Instant::now();
                            self.request_redraw();
                        }
                    }
                    Key::Character(c) => {
                        let c_lower = c.to_lowercase();
                        
                        if self.ignore_next_char && (c_lower == "m") {
                            self.ignore_next_char = false;
                            return ChatAction::None;
                        }
                        self.ignore_next_char = false;

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

                        if c_lower == "a" && has_ctrl {
                            self.selection_start = Some(0);
                            self.cursor_byte_idx = self.input_text.len();
                            self.cursor_blink_start = std::time::Instant::now();
                            self.request_redraw();
                            return ChatAction::None;
                        }

                        // Filter control characters and Alt combinations (to prevent hotkey leakage)
                        if !c.chars().any(|ch| ch.is_control()) && !modifiers.alt_key() {
                            self.delete_selection();
                            self.input_text.insert_str(self.cursor_byte_idx, c);
                            self.cursor_byte_idx += c.len();
                            self.selection_start = None;
                            self.cursor_blink_start = std::time::Instant::now();
                            self.layout_valid = false;
                            self.request_redraw();
                        }
                    }
                    Key::Named(NamedKey::Space) => {
                        self.delete_selection();
                        self.input_text.insert(self.cursor_byte_idx, ' ');
                        self.cursor_byte_idx += 1;
                        self.selection_start = None;
                        self.cursor_blink_start = std::time::Instant::now();
                        self.layout_valid = false;
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
                // 1. Try Image directly
                if let Ok(image) = clipboard.get_image() {
                    let slot_id = self.next_slot_id;
                    self.next_slot_id += 1;
        self.slots.push(ImageSlot {
            id: slot_id,
            status: ImageStatus::Processing,
        });

                    let rgba_data = image.bytes.to_vec();
                    if let Some(img_buf) = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                        image.width as u32,
                        image.height as u32,
                        rgba_data,
                    ) {
                        if let Ok(data) = crate::screen_capture::compress_to_jpeg(&img_buf.into(), 80) {
                            let img_data = crate::types::ImageData {
                                data,
                                mime_type: "image/jpeg".to_string(),
                            };
                            Self::process_raw_image(
                                img_data,
                                slot_id,
                                self.image_tx.clone(),
                                self.proxy.clone(),
                            );
                        } else {
                            self.slots.pop(); // Revert if failed to encode PNG
                        }
                    } else {
                        self.slots.pop();
                    }
                    self.layout_valid = false;
                    self.request_redraw();
                    return;
                }

                // 2. Try DIB
                {
                    use clipboard_win::{formats, get_clipboard};
                    if let Ok(dib_data) =
                        get_clipboard::<Vec<u8>, _>(formats::RawData(formats::CF_DIB))
                    {
                        let mut bmp_file = Vec::with_capacity(14 + dib_data.len());
                        bmp_file.extend_from_slice(b"BM");
                        bmp_file.extend_from_slice(&((14 + dib_data.len()) as u32).to_le_bytes());
                        bmp_file.extend_from_slice(&0u16.to_le_bytes());
                        bmp_file.extend_from_slice(&0u16.to_le_bytes());
                        let header_size = if dib_data.len() >= 4 {
                            u32::from_le_bytes([dib_data[0], dib_data[1], dib_data[2], dib_data[3]])
                        } else {
                            40
                        };
                        bmp_file.extend_from_slice(&(14 + header_size).to_le_bytes());
                        bmp_file.extend_from_slice(&dib_data);

                        if let Ok(img) =
                            image::load_from_memory_with_format(&bmp_file, image::ImageFormat::Bmp)
                        {
                            let rgba = img.to_rgba8();
                            let slot_id = self.next_slot_id;
                            self.next_slot_id += 1;
                            self.slots.push(ImageSlot {
                                id: slot_id,
                                status: ImageStatus::Processing,
                            });

                            if let Ok(data) = crate::screen_capture::compress_to_jpeg(&img, 80) {
                                let img_data = crate::types::ImageData {
                                    data,
                                    mime_type: "image/jpeg".to_string(),
                                };
                                Self::process_raw_image(
                                    img_data,
                                    slot_id,
                                    self.image_tx.clone(),
                                    self.proxy.clone(),
                                );
                            } else {
                                self.slots.pop();
                            }
                            self.layout_valid = false;
                            self.request_redraw();
                            return;
                        }
                    }
                }

                // 3. Try Text
                if let Ok(text) = clipboard.get_text() {
                    let trimmed = text.trim();
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
                    self.input_text.insert_str(self.cursor_byte_idx, trimmed);
                    self.cursor_byte_idx += trimmed.len();
                    self.layout_valid = false;
                    self.request_redraw();
                }
            }
        }
    }

    fn get_thumbnail_at_mouse(&self) -> Option<usize> {
        if self.slots.is_empty() {
            return None;
        }
        let (mx, my) = self.mouse_pos;
        let start_y = 10.0;
        let start_x_base = 10.0;
        let spacing = 10.0;
        let slot_w = 80.0;

        for i in 0..self.slots.len() {
            let tx = start_x_base + (i as f64 * (slot_w + spacing));
            if mx >= tx && mx <= tx + slot_w && my >= start_y && my <= start_y + 80.0 {
                return Some(i);
            }
        }
        None
    }

    fn delete_selection(&mut self) {
        if let Some(start) = self.selection_start {
            if start != self.cursor_byte_idx {
                let min = start.min(self.cursor_byte_idx);
                let max = start.max(self.cursor_byte_idx);
                if min < self.input_text.len() {
                    let end = max.min(self.input_text.len());
                    self.input_text.replace_range(min..end, "");
                    self.cursor_byte_idx = min;
                }
            }
        }
    }

    fn remove_image(&mut self, index: usize) {
        if index < self.slots.len() {
            self.slots.remove(index);
            self.layout_valid = false;
            self.request_redraw();
        }
    }

    fn set_cursor_at_mouse(&mut self) {
        let (mx, my) = self.mouse_pos;
        let scale = Scale::uniform(24.0);
        let padding = 10.0;
        let v_metrics = self.font.v_metrics(scale);
        let line_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let text_y_offset = if self.slots.is_empty() { 0.0 } else { 100.0 };
        let max_width = 600.0 - (padding * 2.0);

        // Relative to text area
        let rx = mx as f32 - padding;
        let ry = my as f32 - padding - text_y_offset;

        if ry < 0.0 {
            self.cursor_byte_idx = 0;
            self.request_redraw();
            return;
        }

        let mut lines = Vec::new();
        let mut current_line_start = 0;
        let mut current_width = 0.0f32;
        for (i, c) in self.input_text.char_indices() {
            let glyph = self.font.glyph(c).scaled(scale);
            let advance = glyph.h_metrics().advance_width;
            if current_width + advance > max_width {
                lines.push(current_line_start..i);
                current_line_start = i;
                current_width = 0.0;
            }
            current_width += advance;
        }
        lines.push(current_line_start..self.input_text.len());

        let line_height_f32 = line_height as f32;
        let line_idx = (ry / line_height_f32).floor() as usize;
        let target_line_idx = if line_idx < lines.len() {
            line_idx
        } else {
            lines.len() - 1
        };
        let target_line_range = &lines[target_line_idx];

        // Find character in line
        let mut best_idx = target_line_range.start;
        let mut current_x = 0.0f32;
        let mut min_dist = rx.abs(); // Distance to start of line

        for (i, c) in self.input_text[target_line_range.clone()].char_indices() {
            let glyph = self.font.glyph(c).scaled(scale);
            let advance = glyph.h_metrics().advance_width;
            
            // Current character's right edge
            let next_x = current_x + advance;
            let dist = (rx - next_x).abs();
            if dist < min_dist {
                min_dist = dist;
                best_idx = target_line_range.start + i + c.len_utf8();
            }
            current_x = next_x;
        }
        
        self.cursor_byte_idx = best_idx;
        self.cursor_blink_start = std::time::Instant::now();
        self.request_redraw();
    }

    fn trigger_upload(&mut self) {
        let tx = self.image_tx.clone();
        let proxy = self.proxy.clone();

        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif"])
                .pick_files();

            if let Some(files) = picked {
                for path in files {
                    let _ = tx.send(ImageAsyncMsg::RequestAddition(path));
                    let _ = proxy.send_event(());
                }
            }
        });
    }

    fn add_image_from_path(&mut self, path: std::path::PathBuf) {
        let slot_id = self.next_slot_id;
        self.next_slot_id += 1;
        self.slots.push(ImageSlot {
            id: slot_id,
            status: ImageStatus::Processing,
        });
        Self::process_image_async(path, slot_id, self.image_tx.clone(), self.proxy.clone());
        self.layout_valid = false;
        self.request_redraw();
    }

    fn process_image_async(
        path: std::path::PathBuf,
        slot_id: u32,
        tx: std::sync::mpsc::Sender<ImageAsyncMsg>,
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
                let _ = tx.send(ImageAsyncMsg::Failed(slot_id));
                let _ = proxy.send_event(());
                return;
            }

            if let Ok(data) = std::fs::read(&path) {
                let mime = format!("image/{}", ext);
                let img_data = crate::types::ImageData {
                    data,
                    mime_type: mime,
                };
                Self::process_raw_image(img_data, slot_id, tx, proxy);
            } else {
                let _ = tx.send(ImageAsyncMsg::Failed(slot_id));
                let _ = proxy.send_event(());
            }
        });
    }

    fn process_raw_image(
        img_data: crate::types::ImageData,
        slot_id: u32,
        tx: std::sync::mpsc::Sender<ImageAsyncMsg>,
        proxy: winit::event_loop::EventLoopProxy<()>,
    ) {
        std::thread::spawn(move || {
            if let Ok(img) = image::load_from_memory(&img_data.data) {
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

                let mut processed_img_data = img_data;
                if let Ok(compressed_data) = crate::screen_capture::compress_to_jpeg(&img, 80) {
                    processed_img_data.data = compressed_data;
                    processed_img_data.mime_type = "image/jpeg".to_string();
                }

                let _ = tx.send(ImageAsyncMsg::Finished(slot_id, processed_img_data, thumb_obj));
                let _ = proxy.send_event(());
            } else {
                let _ = tx.send(ImageAsyncMsg::Failed(slot_id));
                let _ = proxy.send_event(());
            }
        });
    }

    fn redraw(&mut self) {
        let scale = Scale::uniform(24.0);
        let v_metrics = self.font.v_metrics(scale);
        let padding = 10.0;
        let line_height = v_metrics.ascent - v_metrics.descent + v_metrics.line_gap;
        let max_width = 600.0 - (padding * 2.0);

        // 1. Calculate layout & Render to text_buffer if invalid
        if !self.layout_valid {
            self.cached_layout.clear();
            self.cached_line_heights.clear();

            let text_y_offset = if self.slots.is_empty() { 0.0 } else { 100.0 };
            let mut current_line_glyphs = Vec::new();
            let mut current_width = 0.0f32;
            let mut line_y = padding + v_metrics.ascent + text_y_offset;

            // Simple wrapping & layout
            for c in self.input_text.chars() {
                let glyph = self.font.glyph(c).scaled(scale);
                let advance = glyph.h_metrics().advance_width;

                if current_width + advance > max_width && !current_line_glyphs.is_empty() {
                    self.cached_layout.push(current_line_glyphs);
                    current_line_glyphs = Vec::new();
                    current_width = 0.0;
                    line_y += line_height;
                }

                let offset = point(padding + current_width, line_y);
                current_line_glyphs.push(glyph.positioned(offset));
                current_width += advance;
            }
            self.cached_layout.push(current_line_glyphs);

            let text_h = (self.cached_layout.len() as f32 * line_height).max(line_height) as u32;
            let thumbnail_h = if self.slots.is_empty() { 0 } else { 100 };
            let button_row_h = 40;
            let total_padding = (padding * 2.0) as u32;
            let target_height = total_padding + thumbnail_h + text_h + button_row_h;

            // Prepare text_buffer for this layout
            self.text_buffer_w = 600;
            self.text_buffer_h = target_height;
            self.text_buffer.clear();
            self.text_buffer.resize((600 * target_height) as usize, 0);

            // Rasterize all glyphs into the buffer ONCE
            for line in &self.cached_layout {
                for glyph in line {
                    if let Some(bb) = glyph.pixel_bounding_box() {
                        glyph.draw(|x, y, v| {
                            let px = x as i32 + bb.min.x;
                            let py = y as i32 + bb.min.y;
                            if v > 0.0 && px >= 0 && px < 600 && py >= 0 && py < target_height as i32 {
                                let alpha = (v * 255.0) as u32;
                                if alpha > 0 {
                                    // Simple pre-multiplied-style or solid white with alpha in buffer
                                    // Here we store white (0xFFFFFF) and we can blend or just store alpha
                                    // Since background is solid, we'll store the final text color with alpha
                                    self.text_buffer[py as usize * 600 + px as usize] = (alpha << 24) | 0xFFFFFF;
                                }
                            }
                        });
                    }
                }
            }

            let current_size = self.window.inner_size();
            if current_size.height != target_height {
                let _ = self.window.request_inner_size(PhysicalSize::new(600, target_height));
            }
            // Always ensure surface matches target_height in layout pass
            let _ = self.surface.resize(
                NonZeroU32::new(600).unwrap(),
                NonZeroU32::new(target_height).unwrap(),
            );
            self.layout_valid = true;
        }

        let size = self.window.inner_size();
        let buf_w = size.width as usize;
        let buf_h = size.height as usize;

        let mut buffer = self.surface.buffer_mut().unwrap();
        
        // Safety check for resize lag
        if buffer.len() != buf_w * buf_h {
             buffer.present().unwrap();
             return;
        }

        // Colors
        let bg_color: u32 = 0xFF2D2D2D;
        let border_color: u32 = 0xFF444444;
        let text_color: u32 = 0xFFFFFFFF;
        let cursor_color: u32 = 0xFF00FF00;

        // Optimized Background Fill & Border with Anti-Aliasing
        buffer.fill(0); // Transparent outer
        
        draw_rounded_rect_with_border(
            &mut buffer,
            buf_w as u32,
            0,
            0,
            buf_w as u32,
            buf_h as u32,
            12,
            bg_color,
            border_color,
            1,
            buf_w as u32,
            buf_h as u32,
        );

        // Draw Thumbnails
        let mut thumb_x_cursor = 10;
        for (i, slot) in self.slots.iter().enumerate() {
            let is_hovered = self.hovered_thumb == Some(i);
            if let ImageStatus::Ready { thumb, .. } = &slot.status {
                let t_w = thumb.width as usize;
                let t_h = thumb.height as usize;
                let off_x = (80 - t_w) / 2;
                let off_y = (80 - t_h) / 2;

                for ty in 0..t_h {
                    for tx in 0..t_w {
                        let px = thumb_x_cursor + off_x + tx;
                        let py = 10 + off_y + ty;
                        if px < buf_w && py < buf_h {
                            let mut color = thumb.pixels[ty * t_w + tx];
                            if is_hovered {
                                let r_val = (((color >> 16) & 0xFF) as f32 * 0.5 + 127.0) as u32;
                                let g_val = (((color >> 8) & 0xFF) as f32 * 0.5) as u32;
                                let b_val = ((color & 0xFF) as f32 * 0.5) as u32;
                                color = (0xFF << 24) | (r_val << 16) | (g_val << 8) | b_val;
                            }
                            if (color >> 24) & 0xFF > 0 {
                                buffer[py as usize * buf_w + px as usize] = color;
                            }
                        }
                    }
                }
            }
            thumb_x_cursor += 90;
            if thumb_x_cursor + 80 > buf_w { break; }
        }

        // Draw Plus Button with AA Circle
        let _btn_size = 32;
        let btn_x = 10 + 16; // Center X
        let btn_y = buf_h as i32 - 10 - 16; // Center Y
        let plus_bg = if self.plus_button_hovered { 0xFF444444 } else { 0xFF3D3D3D };
        
        draw_circle(
            &mut buffer,
            buf_w as u32,
            btn_x,
            btn_y,
            16,
            plus_bg,
            buf_w as u32,
            buf_h as u32,
        );

        // Draw the plus symbol (+)
        for ty in 0..32 {
            for tx in 0..32 {
                let is_plus = (tx > 10 && tx < 22 && ty >= 15 && ty <= 16) || (ty > 10 && ty < 22 && tx >= 15 && tx <= 16);
                if is_plus {
                    let px = (10 + tx) as usize;
                    let py = (buf_h - 10 - 32 + ty) as usize;
                    if px < buf_w && py < buf_h {
                        buffer[py * buf_w + px] = 0xFFBBBBBB;
                    }
                }
            }
        }

        // Draw Selection Highlight
        if let Some(sel_start) = self.selection_start {
            if sel_start != self.cursor_byte_idx {
                let sel_min = sel_start.min(self.cursor_byte_idx);
                let sel_max = sel_start.max(self.cursor_byte_idx);
                
                let _text_y_base = if self.slots.is_empty() { 0.0 } else { 100.0 };
                let mut byte_offset = 0;
                let mut char_iter = self.input_text.chars();

                for line in &self.cached_layout {
                    let mut line_min_x = f32::MAX;
                    let mut line_max_x = f32::MIN;
                    let mut has_intersection = false;
                    let mut line_baseline_y = 0.0;

                    for glyph in line {
                        let char_len = if let Some(c) = char_iter.next() { c.len_utf8() } else { 0 };
                        let glyph_start = byte_offset;
                        let glyph_end = byte_offset + char_len;
                        byte_offset += char_len;

                        if glyph_start < sel_max && glyph_end > sel_min {
                            let pos = glyph.position();
                            let width = glyph.unpositioned().h_metrics().advance_width;
                            line_min_x = line_min_x.min(pos.x);
                            line_max_x = line_max_x.max(pos.x + width);
                            line_baseline_y = pos.y;
                            has_intersection = true;
                        }
                    }

                    if has_intersection {
                        let rx = line_min_x as i32;
                        let ry = (line_baseline_y - v_metrics.ascent) as i32;
                        let rw = (line_max_x - line_min_x) as u32;
                        let rh = (v_metrics.ascent - v_metrics.descent) as u32;

                        for sy in ry..(ry + rh as i32) {
                            for sx in rx..(rx + rw as i32) {
                                if sx >= 0 && sx < buf_w as i32 && sy >= 0 && sy < buf_h as i32 {
                                    let idx = sy as usize * buf_w + sx as usize;
                                    let bg = buffer[idx];
                                    let sel_color = 0x00AADDFF; // Selection blue
                                    let alpha = 120; // Semi-transparent
                                    
                                    let r = (((sel_color >> 16) & 0xFF) * alpha + ((bg >> 16) & 0xFF) * (255 - alpha)) / 255;
                                    let g = (((sel_color >> 8) & 0xFF) * alpha + ((bg >> 8) & 0xFF) * (255 - alpha)) / 255;
                                    let b = ((sel_color & 0xFF) * alpha + (bg & 0xFF) * (255 - alpha)) / 255;
                                    buffer[idx] = (0xFF << 24) | (r << 16) | (g << 8) | b;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw Text from Pre-rendered Buffer (Alpha Blending) - Region Optimized
        let text_y_start = (padding + (if self.slots.is_empty() { 0.0 } else { 100.0 })) as usize;
        let text_y_end = (text_y_start + self.text_buffer_h as usize).min(buf_h);

        for py in text_y_start..text_y_end {
            let row_start = py * buf_w;
            let src_row_start = py * 600;
            for px in 0..buf_w {
                let color_with_alpha = self.text_buffer[src_row_start + px];
                let alpha = (color_with_alpha >> 24) & 0xFF;
                if alpha > 0 {
                    if alpha == 255 {
                        buffer[row_start + px] = text_color;
                    } else {
                        // Blend with background
                        let bg = buffer[row_start + px];
                        let r = ((0xFF * alpha + ((bg >> 16) & 0xFF) * (255 - alpha)) / 255) as u32;
                        let g = ((0xFF * alpha + ((bg >> 8) & 0xFF) * (255 - alpha)) / 255) as u32;
                        let b = ((0xFF * alpha + (bg & 0xFF) * (255 - alpha)) / 255) as u32;
                        buffer[row_start + px] = (0xFF << 24) | (r << 16) | (g << 8) | b;
                    }
                }
            }
        }

        // Track cursor position from Cache
        let text_y_base = if self.slots.is_empty() { 0.0 } else { 100.0 };
        let mut cursor_pos = (padding as i32, (padding + text_y_base) as i32);
        let mut found_cursor = false;
        let mut byte_counter = 0;
        let mut char_iter = self.input_text.chars();

        for line in &self.cached_layout {
            for glyph in line {
                if byte_counter == self.cursor_byte_idx {
                    cursor_pos = (glyph.position().x as i32, (glyph.position().y - v_metrics.ascent) as i32);
                    found_cursor = true;
                }
                if let Some(c) = char_iter.next() {
                    byte_counter += c.len_utf8();
                }
            }
        }
        
        if !found_cursor && byte_counter == self.cursor_byte_idx {
            if let Some(last_line) = self.cached_layout.last() {
                if let Some(last_glyph) = last_line.last() {
                    cursor_pos = ((last_glyph.position().x + last_glyph.unpositioned().h_metrics().advance_width) as i32, 
                                  (last_glyph.position().y - v_metrics.ascent) as i32);
                }
            }
        }

        // Cursor Blink
        let elapsed = self.cursor_blink_start.elapsed().as_millis();
        if (elapsed % 1000) < 500 {
            let cx = cursor_pos.0 + 1;
            let cy = cursor_pos.1;
            for y in cy..(cy + 24) {
                for x in cx..(cx + 2) {
                    if x >= 0 && x < buf_w as i32 && y >= 0 && y < buf_h as i32 {
                        buffer[y as usize * buf_w + x as usize] = cursor_color;
                    }
                }
            }
        }

        // Use winit's IME positioning
        self.window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(cursor_pos.0 as f64, cursor_pos.1 as f64),
            winit::dpi::PhysicalSize::new(2.0, 24.0),
        );

        buffer.present().unwrap();
    }
}
