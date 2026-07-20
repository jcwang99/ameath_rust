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
use crate::ui_primitives::draw_circle;

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
    SkinLoaded(Thumbnail),
}

pub struct ChatWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
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
    layout_valid: bool,
    ignore_next_char: bool,
    is_selecting: bool,
    is_dragging_scrollbar: bool,
    scroll_y: f32,
    skin_image: Option<Thumbnail>,
    cached_text_h: f32,
    cached_max_scroll: f32,
    cached_text_area_h: f32,
    cached_text_y_base: f32,
    alpha_buffer: Vec<u8>,
    base_ui_buffer: Vec<u32>,
    base_ui_valid: bool,
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

        let (image_tx, image_rx) = std::sync::mpsc::channel();

        // Spawn background thread to load skin image
        let tx_skin = image_tx.clone();
        let proxy_skin = proxy.clone();
        std::thread::spawn(move || {
            let img_data = include_bytes!("../assets/skin/skin1.jpg");
            if let Ok(img) = image::load_from_memory(img_data) {
                // OPTIMIZATION: Downscale skin to a reasonable size to save memory (max 900px)
                let img = img.thumbnail(900, 900);
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let thumb = Thumbnail {
                    width: w,
                    height: h,
                    pixels: rgba.pixels().map(|p| {
                        let a = p[3] as u32;
                        // PRE-APPLY 60% factor to save CPU in every frame
                        let r = (p[0] as u32 * 60) / 100;
                        let g = (p[1] as u32 * 60) / 100;
                        let b = (p[2] as u32 * 60) / 100;
                        (a << 24) | (r << 16) | (g << 8) | b
                    }).collect(),
                };
                let _ = tx_skin.send(ImageAsyncMsg::SkinLoaded(thumb));
                let _ = proxy_skin.send_event(());
            }
        });

        Self {
            window,
            context,
            surface,
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
            layout_valid: false,
            ignore_next_char: false,
            is_selecting: false,
            is_dragging_scrollbar: false,
            scroll_y: 0.0,
            skin_image: None,
            cached_text_h: 0.0,
            cached_max_scroll: 0.0,
            cached_text_area_h: 0.0,
            cached_text_y_base: 0.0,
            alpha_buffer: Vec::new(),
            base_ui_buffer: Vec::new(),
            base_ui_valid: false,
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
                ImageAsyncMsg::SkinLoaded(thumb) => {
                    self.skin_image = Some(thumb);
                    got_new_images = true;
                }
            }
        }
        if got_new_images {
            self.layout_valid = false;
            self.base_ui_valid = false;
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
                    let text = text.replace("\r\n", "\n");
                    self.input_text.insert_str(self.cursor_byte_idx, &text);
                    self.cursor_byte_idx += text.len();
                    self.selection_start = None;
                    self.cursor_blink_start = std::time::Instant::now();
                    self.layout_valid = false;
                    self.ensure_cursor_visible();
                    self.request_redraw();
                }
                _ => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => *y as f32 * 30.0,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                self.scroll_y -= dy;
                self.request_redraw();
            }
            WindowEvent::MouseInput {
                state,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if *state == ElementState::Pressed {
                    let (mx, my) = self.mouse_pos;
                    let size = self.window.inner_size();
                    let buf_w = size.width as f32;
                    let padding = 10.0;
                    
                    let max_scroll = self.cached_max_scroll;
                    let text_area_h = self.cached_text_area_h;
                    let text_y_base = self.cached_text_y_base;

                    // 1. Check Scrollbar Hit (Expanded hit area for easier dragging)
                    if max_scroll > 0.0 && mx >= (buf_w - 20.0) as f64 {
                        self.is_dragging_scrollbar = true;
                        let sb_track_y = padding + text_y_base;
                        let relative_y = (my as f32 - sb_track_y).clamp(0.0, text_area_h);
                        self.scroll_y = (relative_y / text_area_h) * max_scroll;
                    } else if self.plus_button_hovered {
                        self.trigger_upload();
                    } else if let Some(idx) = self.get_thumbnail_at_mouse() {
                        self.remove_image(idx);
                    } else {
                        // Check if click is in text area
                        if my > (padding + text_y_base) as f64 && my < (size.height as f64 - 40.0) {
                            self.is_selecting = true;
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
                    self.is_dragging_scrollbar = false;
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
                
                let padding = 10.0;
                let text_y_base = self.cached_text_y_base;
                let text_area_h = self.cached_text_area_h;
                let max_scroll = self.cached_max_scroll;

                if self.is_dragging_scrollbar && max_scroll > 0.0 {
                    let sb_track_y = padding + text_y_base;
                    let relative_y = (position.y as f32 - sb_track_y).clamp(0.0, text_area_h);
                    self.scroll_y = (relative_y / text_area_h) * max_scroll;
                    self.request_redraw();
                } else if self.is_selecting {
                    // Only update cursor here, continuous scrolling is handled in redraw()
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
                                self.ensure_cursor_visible();
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
                                self.ensure_cursor_visible();
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
                            self.ensure_cursor_visible();
                            self.request_redraw();
                        }
                    }
                    Key::Named(NamedKey::Delete) => {
                        if self.selection_start.is_some() && self.selection_start != Some(self.cursor_byte_idx) {
                            self.delete_selection();
                            self.selection_start = None;
                            self.cursor_blink_start = std::time::Instant::now();
                            self.layout_valid = false;
                            self.request_redraw();
                        } else if self.cursor_byte_idx < self.input_text.len() {
                            if self.input_text[self.cursor_byte_idx..]
                                .char_indices()
                                .next()
                                .is_some()
                            {
                                self.input_text.remove(self.cursor_byte_idx);
                                self.cursor_blink_start = std::time::Instant::now();
                                self.layout_valid = false;
                                self.request_redraw();
                            }
                        }
                    }
                    Key::Named(NamedKey::Home) => {
                        self.cursor_byte_idx = 0;
                        self.selection_start = None;
                        self.cursor_blink_start = std::time::Instant::now();
                        self.ensure_cursor_visible();
                        self.request_redraw();
                    }
                    Key::Named(NamedKey::End) => {
                        self.cursor_byte_idx = self.input_text.len();
                        self.selection_start = None;
                        self.cursor_blink_start = std::time::Instant::now();
                        self.ensure_cursor_visible();
                        self.request_redraw();
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
                            self.ensure_cursor_visible();
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
                        self.ensure_cursor_visible();
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
            // 1. Try DIB (More reliable on Windows for many apps)
            {
                use clipboard_win::{formats, get_clipboard};
                if let Ok(dib_data) = get_clipboard::<Vec<u8>, _>(formats::RawData(formats::CF_DIB)) {
                    if dib_data.len() >= 40 {
                        let mut bmp_file = Vec::with_capacity(14 + dib_data.len());
                        bmp_file.extend_from_slice(b"BM");
                        bmp_file.extend_from_slice(&((14 + dib_data.len()) as u32).to_le_bytes());
                        bmp_file.extend_from_slice(&0u16.to_le_bytes());
                        bmp_file.extend_from_slice(&0u16.to_le_bytes());
                        let header_size = u32::from_le_bytes([dib_data[0], dib_data[1], dib_data[2], dib_data[3]]);
                        bmp_file.extend_from_slice(&(14 + header_size).to_le_bytes());
                        bmp_file.extend_from_slice(&dib_data);

                        if let Ok(img) = image::load_from_memory_with_format(&bmp_file, image::ImageFormat::Bmp) {
                            let slot_id = self.next_slot_id;
                            self.next_slot_id += 1;
                            self.slots.push(ImageSlot { id: slot_id, status: ImageStatus::Processing });
                            if let Ok(data) = crate::screen_capture::compress_to_jpeg(&img, 80) {
                                let img_data = crate::types::ImageData { data, mime_type: "image/jpeg".to_string() };
                                Self::process_raw_image(img_data, slot_id, self.image_tx.clone(), self.proxy.clone());
                                self.layout_valid = false;
                                self.ensure_cursor_visible();
                                self.request_redraw();
                                return;
                            } else {
                                self.slots.pop();
                            }
                        }
                    }
                }
            }

            // 2. Try Arboard (Standard images)
            use arboard::Clipboard;
            if let Ok(mut clipboard) = Clipboard::new() {
                if let Ok(image) = clipboard.get_image() {
                    if image.width > 0 && image.height > 0 {
                        let rgba_data = image.bytes.to_vec();
                        if let Some(img_buf) = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
                            image.width as u32, image.height as u32, rgba_data
                        ) {
                            let dynamic_img = image::DynamicImage::ImageRgba8(img_buf);
                            if let Ok(data) = crate::screen_capture::compress_to_jpeg(&dynamic_img, 80) {
                                let slot_id = self.next_slot_id;
                                self.next_slot_id += 1;
                                self.slots.push(ImageSlot { id: slot_id, status: ImageStatus::Processing });
                                let img_data = crate::types::ImageData { data, mime_type: "image/jpeg".to_string() };
                                Self::process_raw_image(img_data, slot_id, self.image_tx.clone(), self.proxy.clone());
                                self.layout_valid = false;
                                self.ensure_cursor_visible();
                                self.request_redraw();
                                return;
                            }
                        }
                    }
                }

                // 3. Try Text/Path
                if let Ok(text) = clipboard.get_text() {
                    let trimmed = text.trim();
                    let path = std::path::Path::new(trimmed);
                    if path.exists() && path.is_file() {
                        let ok_exts = ["png", "jpg", "jpeg", "webp", "gif"];
                        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        if ok_exts.contains(&ext.as_str()) {
                            self.add_image_from_path(path.to_path_buf());
                            return;
                        }
                    }
                    let normalized = trimmed.replace("\r\n", "\n");
                    self.input_text.insert_str(self.cursor_byte_idx, &normalized);
                    self.cursor_byte_idx += normalized.len();
                    self.layout_valid = false;
                    self.ensure_cursor_visible();
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
            self.ensure_cursor_visible();
            self.request_redraw();
        }
    }

    fn set_cursor_at_mouse(&mut self) {
        let (mx, my) = self.mouse_pos;
        let padding = 10.0;
        let text_y_offset = if self.slots.is_empty() { 0.0 } else { 100.0 };
        let max_width = 600.0 - (padding * 2.0);

        // Relative to text area (account for scroll offset)
        let rx = mx as f32 - padding as f32;
        let ry = my as f32 - padding as f32 - text_y_offset as f32 + self.scroll_y;

        if ry < 0.0 {
            self.cursor_byte_idx = 0;
            self.request_redraw();
            return;
        }

        let char_idx = crate::ui_primitives::get_cursor_index_from_xy(
            &self.input_text, 18.0, max_width as u32, rx, ry
        );
        let mut byte_idx = 0;
        for (i, c) in self.input_text.char_indices().take(char_idx) {
            byte_idx = i + c.len_utf8();
        }
        // Fallback for edge cases
        if char_idx > self.input_text.chars().count() {
            byte_idx = self.input_text.len();
        }

        self.cursor_byte_idx = byte_idx;
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
        self.ensure_cursor_visible();
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
        let font_size = 18.0;
        let padding = 10.0;
        let max_width = 600.0 - (padding * 2.0);

        // 1. Calculate target size based on DirectWrite text measurement (Only when invalid)
        if !self.layout_valid {
            let (_, th) = crate::ui_primitives::get_metrics_dw(&self.input_text, font_size, max_width as u32);
            self.cached_text_h = (th as f32).max(font_size + 8.0);
            
            let thumbnail_h = if self.slots.is_empty() { 0.0 } else { 100.0 };
            let button_row_h = 40.0;
            let total_padding = padding * 2.0;
            
            let raw_text_area_height = self.cached_text_h;
            let raw_target_height = total_padding + thumbnail_h + self.cached_text_h + button_row_h;
            let target_height = raw_target_height.min(400.0);
            
            self.cached_text_area_h = (target_height - (total_padding + thumbnail_h + button_row_h)).max(0.0);
            self.cached_max_scroll = (raw_text_area_height - self.cached_text_area_h).max(0.0);
            self.cached_text_y_base = thumbnail_h;

            let current_size = self.window.inner_size();
            if current_size.height != target_height as u32 {
                let _ = self.window.request_inner_size(PhysicalSize::new(600, target_height as u32));
            }
            // Always ensure surface matches target_height in layout pass
            let _ = self.surface.resize(
                NonZeroU32::new(600).unwrap(),
                NonZeroU32::new(target_height as u32).unwrap(),
            );
            self.layout_valid = true;
            self.base_ui_valid = false; // Size change invalidates static BG
        }

        let max_scroll = self.cached_max_scroll;
        let text_area_h = self.cached_text_area_h;
        let text_y_base = self.cached_text_y_base;

        // Auto-scroll logic (Continuous)
        if self.is_selecting && max_scroll > 0.0 {
            let my = self.mouse_pos.1 as f32;
            let top_bound = padding + text_y_base;
            let bottom_bound = (self.window.inner_size().height as f32) - 40.0 - padding;
            
            let mut scrolled = false;
            if my < top_bound {
                let dist = (top_bound - my).min(100.0);
                let speed = 2.0 + (dist / 10.0).powf(1.5);
                self.scroll_y = (self.scroll_y - speed).max(0.0);
                scrolled = true;
            } else if my > bottom_bound {
                let dist = (my - bottom_bound).min(100.0);
                let speed = 2.0 + (dist / 10.0).powf(1.5);
                self.scroll_y = (self.scroll_y + speed).min(max_scroll);
                scrolled = true;
            }

            if scrolled {
                self.set_cursor_at_mouse();
                self.request_redraw();
            }
        }

        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);

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
        let border_color: u32 = 0xFF444444;
        let text_color: u32 = 0xFFFFFFFF;
        let cursor_color: u32 = 0xFF00FF00;

        // OPTIMIZATION: Reuse or rebuild Base UI Cache (BG, Skin, Border)
        if !self.base_ui_valid || self.base_ui_buffer.len() != buf_w * buf_h {
            self.base_ui_buffer.resize(buf_w * buf_h, 0);
            self.base_ui_buffer.fill(0);
            
            let base_buffer = &mut self.base_ui_buffer[..];
            let bg_color: u32 = 0xFF2D2D2D;

            crate::ui_primitives::draw_rounded_rect(
                base_buffer,
                buf_w as u32,
                0,
                0,
                buf_w as u32,
                buf_h as u32,
                12.min(buf_h as u32 / 2).max(1),
                bg_color,
                buf_w as u32,
                buf_h as u32,
            );

            if let Some(skin) = &self.skin_image {
                let img_w = skin.width as usize;
                let img_h = skin.height as usize;
                let scale_x = buf_w as f32 / img_w as f32;
                let scale_y = buf_h as f32 / img_h as f32;
                let scale = scale_x.max(scale_y);
                let draw_w = (img_w as f32 * scale) as usize;
                let draw_h = (img_h as f32 * scale) as usize;
                let off_x = (draw_w.saturating_sub(buf_w)) / 2;
                let off_y = ((draw_h.saturating_sub(buf_h)) as f32 * 0.25) as usize;
                
                use rayon::prelude::*;
                base_buffer.par_chunks_mut(buf_w).enumerate().for_each(|(y, row)| {
                    for (x, pixel) in row.iter_mut().enumerate() {
                        let ex_a = (*pixel >> 24) & 0xFF;
                        if ex_a > 0 { 
                            let tx = ((x + off_x) as f32 / scale) as usize;
                            let ty = ((y + off_y) as f32 / scale) as usize;
                            if tx < img_w && ty < img_h {
                                let skin_px = skin.pixels[ty * img_w + tx];
                                if ex_a == 255 {
                                    // Opaque background: Darken skin to 60% for readability
                                    let r = ((skin_px >> 16) & 0xFF) * 160 / 255;
                                    let g = ((skin_px >> 8) & 0xFF) * 160 / 255;
                                    let b = (skin_px & 0xFF) * 160 / 255;
                                    *pixel = (0xFF << 24) | (r << 16) | (g << 8) | b;
                                } else {
                                    let sr = (skin_px >> 16) & 0xFF;
                                    let sg = (skin_px >> 8) & 0xFF;
                                    let sb = skin_px & 0xFF;
                                    
                                    // Combine AA alpha with 60% brightness darkening
                                    let dr = (sr * ex_a * 160) / (255 * 255);
                                    let dg = (sg * ex_a * 160) / (255 * 255);
                                    let db = (sb * ex_a * 160) / (255 * 255);
                                    *pixel = (ex_a << 24) | (dr << 16) | (dg << 8) | db;
                                }
                            }
                        }
                    }
                });
            }

            self.alpha_buffer.resize(buf_w * buf_h, 0);
            self.alpha_buffer.fill(0);
            let border_r = 12.min(buf_h as u32 / 2).max(1);
            crate::ui_primitives::draw_rounded_rect_border_alpha_internal(
                &mut self.alpha_buffer, buf_w as u32, buf_w as u32, buf_h as u32, border_r, 1
            );
            crate::ui_primitives::blit_alpha(
                base_buffer,
                buf_w as u32,
                0,
                0,
                buf_h as u32,
                &self.alpha_buffer,
                border_color,
                buf_w as u32,
                buf_h as u32,
            );
            self.base_ui_valid = true;
        }

        // Extremely fast copy from cache to hardware surface
        buffer.copy_from_slice(&self.base_ui_buffer);

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
                
                let char_sel_start = self.input_text[..sel_min].chars().count();
                let char_sel_end = self.input_text[..sel_max].chars().count();
                
                let rects = crate::ui_primitives::get_selection_rects(
                    &self.input_text,
                    font_size,
                    max_width as u32,
                    char_sel_start,
                    char_sel_end,
                );
                
                for (rx, ry, _rw, _rh) in rects {
                    let draw_y = padding as f32 + text_y_base + ry - self.scroll_y;
                    // Only draw if within text area bounds
                    if draw_y + _rh >= padding + text_y_base && draw_y <= padding + text_y_base + text_area_h {
                        let clip_y = draw_y.max(padding + text_y_base);
                        let clip_bottom = (draw_y + _rh).min(padding + text_y_base + text_area_h);
                        let clip_h = clip_bottom - clip_y;
                        
                        if clip_h > 0.0 {
                            crate::ui_primitives::draw_rect(
                                &mut buffer,
                                buf_w as u32,
                                (padding as f32 + rx) as i32,
                                clip_y as i32,
                                _rw as u32,
                                clip_h as u32,
                                0x7700AADD, // Semi-transparent blue for selection
                                buf_w as u32,
                                buf_h as u32,
                            );
                        }
                    }
                }
            }
        }

        // Draw Text from DirectWrite
        crate::ui_primitives::draw_text_dw_ex(
            &mut buffer,
            buf_w as u32,
            &self.input_text,
            padding as i32,
            (padding as f32 + text_y_base) as i32,
            font_size,
            text_color,
            max_width as u32,
            text_area_h as u32, // Clip height to text area
            self.scroll_y,      // scroll_offset (Arg 10)
            0.0,                // scroll_x (Arg 11)
            max_width as u32,
        );

        // Track cursor position
        let char_cursor = self.input_text[..self.cursor_byte_idx].chars().count();
        let (cx, cy, ch) = crate::ui_primitives::get_xy_from_cursor_index(
            &self.input_text,
            font_size,
            max_width as u32,
            char_cursor,
        );
        
        let cursor_x = (padding as f32 + cx) as i32;
        let cursor_y = (padding as f32 + text_y_base + cy - self.scroll_y) as i32;
        let cursor_pos = (cursor_x, cursor_y);

        // Cursor Blink (only if within view)
        let elapsed = self.cursor_blink_start.elapsed().as_millis();
        if (elapsed % 1000) < 500 {
            if cursor_pos.1 >= (padding + text_y_base) as i32 && (cursor_pos.1 + ch as i32) <= (padding + text_y_base + text_area_h) as i32 {
                crate::ui_primitives::draw_rect(
                    &mut buffer,
                    buf_w as u32,
                    cursor_pos.0,
                    cursor_pos.1,
                    2,
                    ch as u32,
                    cursor_color,
                    buf_w as u32,
                    buf_h as u32,
                );
            }
        }

        // Draw Scrollbar if contents overflow
        if max_scroll > 0.0 {
            let sb_width = 4;
            let sb_x = buf_w - 6;
            let sb_h = (text_area_h * (text_area_h / self.cached_text_h)) as u32;
            let sb_y = (padding + text_y_base) + (self.scroll_y / max_scroll) * (text_area_h - sb_h as f32);
            
            crate::ui_primitives::draw_rounded_rect(
                &mut buffer,
                buf_w as u32,
                sb_x as i32,
                sb_y as i32,
                sb_width as u32,
                sb_h.max(10),
                2,
                0x88AAAAAA, // Semi-transparent grey
                buf_w as u32,
                buf_h as u32,
            );
        }

        // Use winit's IME positioning
        self.window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(cursor_pos.0 as f64, cursor_pos.1 as f64),
            winit::dpi::PhysicalSize::new(2.0, font_size as f64),
        );
        buffer.present().unwrap();
    }

    pub fn ensure_cursor_visible(&mut self) {
        let font_size = 18.0;
        let padding = 10.0;
        let max_width = 600.0 - (padding * 2.0);

        // Safety: ensure index is within bounds to prevent panics during slicing
        self.cursor_byte_idx = self.cursor_byte_idx.min(self.input_text.len());

        let char_cursor = self.input_text[..self.cursor_byte_idx].chars().count();
        let (_cx, cy, ch) = crate::ui_primitives::get_xy_from_cursor_index(
            &self.input_text,
            font_size,
            max_width as u32,
            char_cursor,
        );

        let text_area_h = self.cached_text_area_h;
        
        // If cursor is below viewport
        if cy + ch > self.scroll_y + text_area_h {
            self.scroll_y = cy + ch - text_area_h;
        }
        // If cursor is above viewport
        else if cy < self.scroll_y {
            self.scroll_y = cy;
        }

        // Clamp to valid range
        self.scroll_y = self.scroll_y.clamp(0.0, self.cached_max_scroll);
    }
}
