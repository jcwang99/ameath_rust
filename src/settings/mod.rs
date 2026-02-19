pub mod tabs;

use crate::theme::*;
use crate::types::{AiConfig, BehaviorMode, PersistentConfig, WindowLayer};
use crate::ui_primitives::*;
use softbuffer::{Context, Surface};
// use windows::core::ComInterface;
// use windows::Win32::Graphics::Direct2D::ID2D1DCRenderTarget;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use winit::event_loop::{EventLoopProxy, EventLoopWindowTarget};
use winit::window::Window;

pub struct SettingsRenderInput {
    pub w: u32,
    pub h: u32,
    pub current_tab: usize,
    pub scroll_offset: f32,
    pub focused_field: Option<usize>,
    pub show_api_key: bool,
    pub cursor_pos: usize,
    pub selection_start: Option<usize>,
    pub last_cursor_action: std::time::Instant,
    pub system_prompt_scroll_offset: f32,
    pub history: std::sync::Arc<Vec<(String, String)>>,
    pub history_scroll_states: Vec<f32>,
    pub system_prompt_hash: u64,
    pub system_prompt_metrics_cache: f32,
    pub current_scale: f32,
    pub current_mode: String,
    pub current_music_path: Option<std::path::PathBuf>,
    pub current_layer: crate::types::WindowLayer,
    pub ai_config: crate::types::AiConfig,
    pub mouse_pos: (f32, f32),
    pub pressed_btn: Option<usize>, // 0-4 for profile buttons, 100+ for fields
    pub show_delete_dialog: bool,
    pub notification: Option<(String, std::time::Instant)>,
    pub field_scroll_offsets: [f32; 14],
    pub available_monitors: Vec<(String, String)>,
    pub current_monitor_name: Option<String>,
}

pub struct RenderResult {
    pub pixels: std::sync::Arc<Vec<u32>>, // Use Arc to avoid deep copy when cloning
    pub vh: f32,
    pub ch: f32,
    pub cursor_rect: Option<(i32, i32, u32, u32)>,
    pub w: u32,
    pub h: u32,
    pub hash: u64,
    pub active_sys_prompt_rect: Option<(f64, f64, f64, f64)>,
    pub active_sys_prompt_content_height: f32,
    pub history_item_rects: Vec<(f64, f64, f64, f64)>,
}

pub struct RenderRequest {
    pub input: SettingsRenderInput,
    pub hash: u64,
    pub buffer: Vec<u32>,
}

fn render_internal(buffer: &mut [u32], input: SettingsRenderInput, hash: u64) -> RenderResult {
    let w = input.w;
    let h = input.h;
    let mut vh = 0.0;
    let mut ch = 0.0;
    let mut cursor_rect = None;
    let mut active_sys_prompt_rect = None;
    let mut active_sys_prompt_content_height = 0.0f32;
    let mut history_item_rects = Vec::new();

    buffer.fill(COLOR_BG_APP);

    let scale = (w as f32 / 800.0).min(h as f32 / 750.0);
    let off_x = (w as f32 - 800.0 * scale) / 2.0;
    let off_y = (h as f32 - 750.0 * scale) / 2.0;

    let sc = |val: f32| -> f32 { val * scale };
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };

    // Sidebar
    draw_rect(buffer, w, 0, 0, s(180), h, COLOR_BG_SIDEBAR, w, h);
    let icons = ["🏠", "🎨", "🧠", "📜", "ℹ️"];
    for i in 0..5 {
        let color = if input.current_tab == i {
            COLOR_PRIMARY
        } else {
            COLOR_TEXT_SEC
        };
        draw_text(
            buffer,
            w,
            &[],
            icons[i],
            s(75) as i32,
            sy_val(60 + i as u32 * 80) as i32,
            sc(32.0),
            color,
        );
    }

    // Header
    let (title, sub) = match input.current_tab {
        0 => ("Home", "Welcome to Ameath!"),
        1 => ("Appearance", "Customize your pet's look"),
        2 => ("AI Brain", "Connect Ameath to the cloud"),
        3 => ("History", "Recent Local Memory (Last 50)"),
        _ => ("About", "Ameath v0.1.0"),
    };
    let header_h = sy_val(120);
    draw_rect(
        buffer,
        w,
        s(180) as i32,
        0,
        w - s(180),
        header_h,
        COLOR_BG_APP,
        w,
        h,
    );
    draw_text(
        buffer,
        w,
        &[],
        title,
        s(220) as i32,
        sy_val(40) as i32,
        sc(32.0),
        COLOR_TEXT_MAIN,
    );
    draw_text(
        buffer,
        w,
        &[],
        sub,
        s(220) as i32,
        sy_val(85) as i32,
        sc(16.0),
        COLOR_TEXT_SEC,
    );

    // Tab Content
    match input.current_tab {
        0 => {
            let (v, c, _) = tabs::home::draw(buffer, w, h, scale, off_x, off_y);
            vh = v;
            ch = c;
        }
        1 => {
            let mut gen_state = tabs::general::GeneralTabState {
                current_scale: input.current_scale,
                current_mode: &input.current_mode,
                current_music_path: input.current_music_path.as_deref(),
                current_layer: input.current_layer,
                scroll_offset: input.scroll_offset,
                available_monitors: &input.available_monitors,
                current_monitor_name: input.current_monitor_name.as_deref(),
            };
            let (v, c, _) = tabs::general::draw(buffer, w, h, scale, off_x, off_y, &mut gen_state);
            vh = v;
            ch = c;
        }
        2 => {
            let mut sys_metrics = input.system_prompt_metrics_cache;
            let mut local_sys_rect = None;
            let mut local_sys_content_h = 0.0f32;
            let lx = (input.mouse_pos.0 as f32 - off_x) / scale;
            let ly = (input.mouse_pos.1 as f32 - off_y) / scale;
            let dly = ly - 120.0 - input.scroll_offset;

            let mut ai_state = tabs::ai::AiTabState {
                focused_field: input.focused_field,
                show_api_key: input.show_api_key,
                cursor_pos: input.cursor_pos,
                selection_start: input.selection_start,
                last_cursor_action: input.last_cursor_action,
                system_prompt_scroll_offset: input.system_prompt_scroll_offset,
                active_sys_prompt_content_height: &mut local_sys_content_h,
                active_sys_prompt_rect: &mut local_sys_rect,
                system_prompt_metrics_cache: &mut sys_metrics,
                system_prompt_hash: input.system_prompt_hash,
                draw_cursor: false,
                mouse_pos: (lx, ly),
                content_mouse_pos: (lx, dly),
                pressed_btn: input.pressed_btn,
                show_delete_dialog: input.show_delete_dialog,
                notification: input.notification,
                field_scroll_offsets: input.field_scroll_offsets,
            };
            let (v, c, rect) = tabs::ai::draw(
                buffer,
                w,
                h,
                scale,
                off_x,
                off_y,
                input.scroll_offset,
                &input.ai_config,
                &mut ai_state,
            );
            vh = v;
            ch = c;
            cursor_rect = rect;
            active_sys_prompt_rect = local_sys_rect;
            active_sys_prompt_content_height = local_sys_content_h;
        }
        3 => {
            let mut scroll_states = input.history_scroll_states.clone();
            let mut local_rects = Vec::new();
            let mut history_state = tabs::history::HistoryTabState {
                history: &input.history,
                history_scroll_states: &mut scroll_states,
                history_item_rects: &mut local_rects,
                scroll_offset: input.scroll_offset * scale,
            };
            let (v, c, _) =
                tabs::history::draw(buffer, w, h, scale, off_x, off_y, &mut history_state);
            vh = v;
            ch = c;
            history_item_rects = local_rects;
        }
        4 => {
            let (v, c, _) = tabs::about::draw(buffer, w, h, scale, off_x, off_y);
            vh = v;
            ch = c;
        }
        _ => {}
    }

    // Scrollbar (Relocated to main thread)

    RenderResult {
        pixels: std::sync::Arc::new(buffer.to_vec()),
        vh,
        ch,
        cursor_rect,
        w,
        h,
        hash,
        active_sys_prompt_rect,
        active_sys_prompt_content_height,
        history_item_rects,
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum SettingsAction {
    None,
    SetScale(f32),
    SetMode(BehaviorMode),
    SetLayer(WindowLayer),
    SetAiApiKey(String),
    SetAiBaseUrl(String),
    SetAiModel(String),
    SetAiReactLimit(usize),
    SetAiL1Threshold(usize),
    SetAiL2Threshold(usize),
    SetAiTavilyKey(String),
    SetAiSystemPrompt(String),
    SetAiInteractionFrequency(u64),
    UpdateAiConfig(AiConfig),
    RequestHistory,
    SetMonitor(String),
    SelectMusicPath,
    RequestGc,
}

pub struct SettingsWindow {
    window: Rc<Window>,
    #[allow(dead_code)]
    context: Context<Rc<Window>>,
    surface: Surface<Rc<Window>, Rc<Window>>,
    pub current_tab: usize,
    pub scroll_offset: f32,
    pub content_height: f32,
    pub viewport_height: f32,

    // AI Tab State
    pub focused_field: Option<usize>,
    pub show_api_key: bool,
    pub cursor_pos: usize,
    pub selection_start: Option<usize>,
    pub last_cursor_action: std::time::Instant,
    pub is_dragging_text: bool,
    pub system_prompt_scroll_offset: f32,
    pub active_sys_prompt_content_height: f32,
    pub active_sys_prompt_rect: Option<(f64, f64, f64, f64)>,

    // History Tab State
    pub history: std::sync::Arc<Vec<(String, String)>>,
    pub history_scroll_states: Vec<f32>,
    pub history_item_rects: Vec<(f64, f64, f64, f64)>,
    pub history_hashes: Vec<u64>,
    pub history_metrics_cache: Vec<f32>, // Cached heights
    pub dragging_history_idx: Option<usize>,
    pub dragging_sys_prompt: bool,
    pub system_prompt_hash: u64,
    pub system_prompt_metrics_cache: f32,
    pub config_dirty: bool,

    // Layout
    pub is_dragging_scrollbar: bool,
    pub last_size: (u32, u32),
    pub last_render_scale: f32,
    pub available_monitors: Vec<(String, String)>,
    pub current_monitor_name: Option<String>,

    pub is_dirty: bool,
    pub last_state_hash: u64,
    pub last_config_hash: u64,
    pub mouse_pos: (f32, f32),
    pub pressed_btn: Option<usize>,
    pub show_delete_dialog: bool,
    pub notification: Option<(String, std::time::Instant)>,
    pub field_scroll_offsets: [f32; 14],

    // Layered Rendering Caches (Removed for memory savings)
    pub cursor_cache: Option<(i32, i32, u32, u32)>,
    pub cursor_save_under: Vec<u32>,
    pub last_base_state_hash: u64,
    pub last_sent_hash: u64,

    // Multithreaded Buffer
    pub render_back_buffer: Arc<Mutex<Option<RenderResult>>>,
    pub render_in_progress: Arc<AtomicBool>,
    pub idle_buffers: Arc<Mutex<Vec<Vec<u32>>>>,
    pub last_background_pixels: std::sync::Arc<Vec<u32>>,
    pub render_tx: Sender<RenderRequest>,
    pub _proxy: EventLoopProxy<()>,
}

impl SettingsWindow {
    pub fn new(
        event_loop: &EventLoopWindowTarget<()>,
        proxy: EventLoopProxy<()>,
        icon: Option<winit::window::Icon>,
    ) -> Self {
        let window = Rc::new(
            winit::window::WindowBuilder::new()
                .with_title("Ameath Settings")
                .with_inner_size(winit::dpi::LogicalSize::new(800, 750))
                .with_resizable(true)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                .with_window_icon(icon)
                .build(event_loop)
                .unwrap(),
        );
        window.set_ime_allowed(true);

        let context = Context::new(window.clone()).unwrap();
        let surface = Surface::new(&context, window.clone()).unwrap();

        let available_monitors: Vec<(String, String)> = event_loop
            .available_monitors()
            .map(|m| (m.name().unwrap_or_default(), m.name().unwrap_or_default()))
            .collect();

        let render_back_buffer = Arc::new(Mutex::new(None));
        let render_in_progress = Arc::new(AtomicBool::new(false));
        let idle_buffers = Arc::new(Mutex::new(Vec::with_capacity(2)));
        let (render_tx, render_rx) = mpsc::channel::<RenderRequest>();

        let rb_ptr = render_back_buffer.clone();
        let rip_ptr = render_in_progress.clone();
        let idle_buffers_ptr = idle_buffers.clone();
        let p_ptr = proxy.clone();

        std::thread::spawn(move || {
            while let Ok(mut req) = render_rx.recv() {
                // Drain any pending requests and only process the latest one
                // This is the "Frame Skipping" mechanism
                while let Ok(next_req) = render_rx.try_recv() {
                    // Return previous request's buffer to idle pool
                    let mut idle = idle_buffers_ptr.lock().unwrap();
                    if idle.len() < 2 {
                        idle.push(req.buffer);
                    }
                    req = next_req;
                }

                let res = render_internal(&mut req.buffer, req.input, req.hash);
                {
                    let mut lock = rb_ptr.lock().unwrap();
                    *lock = Some(res);
                }
                rip_ptr.store(false, Ordering::SeqCst);
                let _ = p_ptr.send_event(());
            }
        });

        Self {
            window,
            context,
            surface,
            current_tab: 0,
            scroll_offset: 0.0,
            content_height: 0.0,
            viewport_height: 0.0,
            focused_field: None,
            show_api_key: false,
            cursor_pos: 0,
            selection_start: None,
            last_cursor_action: std::time::Instant::now(),
            is_dragging_text: false,
            system_prompt_scroll_offset: 0.0,
            active_sys_prompt_content_height: 0.0,
            active_sys_prompt_rect: None,
            history: std::sync::Arc::new(Vec::new()),
            history_scroll_states: Vec::new(),
            history_item_rects: Vec::new(),
            history_hashes: Vec::new(),
            history_metrics_cache: Vec::new(),
            system_prompt_hash: 0,
            system_prompt_metrics_cache: 0.0,
            config_dirty: true,
            is_dragging_scrollbar: false,
            available_monitors,
            current_monitor_name: None,
            last_size: (800, 750),
            last_render_scale: 1.0,
            is_dirty: true,
            last_state_hash: 0,
            last_config_hash: 0,
            mouse_pos: (0.0, 0.0),
            pressed_btn: None,
            show_delete_dialog: false,
            notification: None,
            cursor_cache: None,
            cursor_save_under: Vec::new(),
            last_base_state_hash: 0,
            last_sent_hash: 0,
            dragging_history_idx: None,
            dragging_sys_prompt: false,
            field_scroll_offsets: [0.0; 14],
            render_back_buffer,
            render_in_progress,
            idle_buffers,
            last_background_pixels: std::sync::Arc::new(Vec::new()),
            render_tx,
            _proxy: proxy,
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        self.window.focus_window();
    }

    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn request_redraw_actual(&self) {
        self.window.request_redraw();
    }

    pub fn next_blink_at(&self) -> std::time::Instant {
        if self.focused_field.is_none() {
            // Hibernate for 1 hour if nothing to blink
            return std::time::Instant::now() + std::time::Duration::from_secs(3600);
        }
        let elapsed_ms = self.last_cursor_action.elapsed().as_millis();
        let current_step = elapsed_ms / 500;
        let next_step = current_step + 1;
        self.last_cursor_action + std::time::Duration::from_millis((next_step * 500) as u64)
    }

    pub fn window(&self) -> &Rc<Window> {
        &self.window
    }

    fn create_render_input(
        &self,
        current_scale: f32,
        current_mode: &str,
        current_music_path: Option<&std::path::Path>,
        current_layer: crate::types::WindowLayer,
        ai_config: &crate::types::AiConfig,
    ) -> SettingsRenderInput {
        let size = self.window.inner_size();
        SettingsRenderInput {
            w: size.width,
            h: size.height,
            current_tab: self.current_tab,
            scroll_offset: self.scroll_offset,
            focused_field: self.focused_field,
            show_api_key: self.show_api_key,
            cursor_pos: self.cursor_pos,
            selection_start: self.selection_start,
            last_cursor_action: self.last_cursor_action,
            system_prompt_scroll_offset: self.system_prompt_scroll_offset,
            history: self.history.clone(),
            history_scroll_states: self.history_scroll_states.clone(),
            system_prompt_hash: self.system_prompt_hash,
            system_prompt_metrics_cache: self.system_prompt_metrics_cache,
            current_scale,
            current_mode: current_mode.to_string(),
            current_music_path: current_music_path.map(|p| p.to_path_buf()),
            current_layer,
            ai_config: ai_config.clone(),
            mouse_pos: self.mouse_pos,
            pressed_btn: self.pressed_btn,
            show_delete_dialog: self.show_delete_dialog,
            notification: self.notification.clone(),
            field_scroll_offsets: self.field_scroll_offsets,
            available_monitors: self.available_monitors.clone(),
            current_monitor_name: self.current_monitor_name.clone(),
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
        let w = size.width;
        let h = size.height;
        if w == 0 || h == 0 {
            return;
        }

        self.current_monitor_name = self.window.current_monitor().and_then(|m| m.name());

        if self.last_size != (w, h) || (current_scale - self.last_render_scale).abs() > 0.01 {
            self.surface
                .resize(
                    std::num::NonZeroU32::new(w).unwrap(),
                    std::num::NonZeroU32::new(h).unwrap(),
                )
                .unwrap();
            self.last_size = (w, h);
            self.last_render_scale = current_scale;
            self.history_hashes.clear();
            self.history_metrics_cache.clear();
            self.system_prompt_metrics_cache = 0.0;
            self.is_dirty = true;
        }

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        // 0. Config Hashing (Main Thread)
        if self.config_dirty || self.last_config_hash == 0 {
            let mut config_hasher = DefaultHasher::new();
            // Hash legacy fields for safety
            ai_config.api_key.hash(&mut config_hasher);
            ai_config.base_url.hash(&mut config_hasher);
            ai_config.model.hash(&mut config_hasher);

            // Hash Profiles
            ai_config.active_profile_index.hash(&mut config_hasher);
            for profile in &ai_config.profiles {
                profile.name.hash(&mut config_hasher);
                profile.api_key.hash(&mut config_hasher);
                profile.base_url.hash(&mut config_hasher);
                profile.model.hash(&mut config_hasher);
                profile.is_multimodal.hash(&mut config_hasher);
            }

            if self.system_prompt_hash == 0 {
                let mut s_hasher = DefaultHasher::new();
                ai_config.system_prompt.hash(&mut s_hasher);
                self.system_prompt_hash = s_hasher.finish();
            }
            self.system_prompt_hash.hash(&mut config_hasher);
            ai_config.tavily_api_key.hash(&mut config_hasher);
            ai_config.brave_api_key.hash(&mut config_hasher);
            ai_config.firecrawl_api_key.hash(&mut config_hasher);
            ai_config.firecrawl_url.hash(&mut config_hasher);
            ai_config.interaction_frequency.hash(&mut config_hasher);
            ai_config.l1_summary_threshold.hash(&mut config_hasher);
            ai_config.l2_merge_threshold.hash(&mut config_hasher);
            ai_config.react_limit.hash(&mut config_hasher);
            self.last_config_hash = config_hasher.finish();
            self.config_dirty = false;
        }

        let mut base_hasher = DefaultHasher::new();
        w.hash(&mut base_hasher);
        h.hash(&mut base_hasher);
        self.current_tab.hash(&mut base_hasher);
        self.show_api_key.hash(&mut base_hasher);
        // scroll_offset excluded from hash for instant main-thread scrollbar feedback
        self.focused_field.hash(&mut base_hasher);
        self.cursor_pos.hash(&mut base_hasher);
        self.selection_start.hash(&mut base_hasher);
        self.history.len().hash(&mut base_hasher);
        self.last_config_hash.hash(&mut base_hasher);
        current_scale.to_bits().hash(&mut base_hasher);
        current_mode.hash(&mut base_hasher);
        current_music_path.hash(&mut base_hasher);
        current_layer.hash(&mut base_hasher);
        self.system_prompt_scroll_offset
            .to_bits()
            .hash(&mut base_hasher);
        self.scroll_offset.to_bits().hash(&mut base_hasher);
        self.mouse_pos.0.to_bits().hash(&mut base_hasher);
        self.mouse_pos.1.to_bits().hash(&mut base_hasher);
        self.pressed_btn.hash(&mut base_hasher);
        self.show_delete_dialog.hash(&mut base_hasher);
        if let Some((text, time)) = &self.notification {
            text.hash(&mut base_hasher);
            time.hash(&mut base_hasher);
        }
        for offset in &self.history_scroll_states {
            offset.to_bits().hash(&mut base_hasher);
        }
        for offset in &self.field_scroll_offsets {
            offset.to_bits().hash(&mut base_hasher);
        }
        let base_state_hash = base_hasher.finish();

        let mut transient_hasher = DefaultHasher::new();
        transient_hasher.write_u64(base_state_hash);
        self.scroll_offset.to_bits().hash(&mut transient_hasher);
        let elapsed_ms = self.last_cursor_action.elapsed().as_millis();
        let is_cursor_on = (elapsed_ms / 500) % 2 == 0;
        if self.focused_field.is_some() {
            is_cursor_on.hash(&mut transient_hasher);
        }
        let current_hash = transient_hasher.finish();

        // 1. Check Background Result
        let mut consumed_background = false;
        {
            let mut back_buffer = self.render_back_buffer.lock().unwrap();
            if let Some(res) = back_buffer.take() {
                if res.w == w && res.h == h {
                    let mut buffer = self.surface.buffer_mut().unwrap();
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            res.pixels.as_ptr(),
                            buffer.as_mut_ptr(),
                            (w * h) as usize,
                        );
                    }
                    self.last_background_pixels = res.pixels.clone(); // Arc clone (shallow copy)
                    self.viewport_height = res.vh;
                    self.content_height = res.ch;
                    self.cursor_cache = res.cursor_rect;
                    self.active_sys_prompt_rect = res.active_sys_prompt_rect;
                    self.active_sys_prompt_content_height = res.active_sys_prompt_content_height;
                    self.history_item_rects = res.history_item_rects;

                    if res.hash == base_state_hash {
                        self.last_base_state_hash = base_state_hash;
                        self.is_dirty = false;
                    }
                    consumed_background = true;

                    // Recycle pixels back to idle pool if possible
                    // Since pixels is Arc, we can only recycle if we're the only owner
                    let mut idle = self.idle_buffers.lock().unwrap();
                    if idle.len() < 2 {
                        if let Ok(pixels_vec) = std::sync::Arc::try_unwrap(res.pixels) {
                            idle.push(pixels_vec);
                        }
                    }
                }
            }
        }

        if !consumed_background && !self.is_dirty && self.last_state_hash == current_hash {
            return;
        }

        // 3. Trigger Async Redraw if needed
        let hash_mismatch = base_state_hash != self.last_base_state_hash;
        if (self.is_dirty || hash_mismatch) && base_state_hash != self.last_sent_hash {
            // Update history metadata before spawning if needed (Main Thread)
            if self.current_tab == 3 && self.history_metrics_cache.len() != self.history.len() {
                self.history_metrics_cache.resize(self.history.len(), 0.0);
                self.history_hashes.resize(self.history.len(), 0);
                self.history_scroll_states.resize(self.history.len(), 0.0);
                let scale = (w as f32 / 800.0).min(h as f32 / 750.0);
                let max_text_w = (450.0 * scale) as u32;
                for i in 0..self.history.len() {
                    if self.history_metrics_cache[i] == 0.0 {
                        let (_, content) = &self.history[i];
                        let (_, mh) =
                            crate::ui_primitives::get_metrics_dw(content, 16.0 * scale, max_text_w);
                        self.history_metrics_cache[i] = mh;
                    }
                }
            }

            self.render_in_progress.store(true, Ordering::SeqCst);
            self.last_sent_hash = base_state_hash;
            let input = self.create_render_input(
                current_scale,
                current_mode,
                current_music_path,
                current_layer,
                ai_config,
            );

            let mut pixels = {
                let mut idle = self.idle_buffers.lock().unwrap();
                idle.pop().unwrap_or_else(|| vec![0u32; (w * h) as usize])
            };
            if pixels.len() != (w * h) as usize {
                pixels = vec![0u32; (w * h) as usize];
            }

            let _ = self.render_tx.send(RenderRequest {
                input,
                hash: base_state_hash,
                buffer: pixels,
            });
        }

        let mut buffer = self.surface.buffer_mut().unwrap();

        // 1.5 Restore background if no new background frame was just copied
        // (This happens during smooth scrolling dragging between worker frames)
        if base_state_hash == self.last_base_state_hash && !self.last_background_pixels.is_empty() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    self.last_background_pixels.as_ptr(),
                    buffer.as_mut_ptr(),
                    (w * h) as usize,
                );
            }
        }

        // 2. Surgical Cursor Blink (Synchronous)
        let only_blink = !self.is_dirty && base_state_hash == self.last_base_state_hash;
        if only_blink {
            if let Some((cx, cy, cw, ch)) = self.cursor_cache {
                if cx >= 0
                    && cy >= 0
                    && (cx + cw as i32) <= w as i32
                    && (cy + ch as i32) <= h as i32
                {
                    let mut buffer = self.surface.buffer_mut().unwrap();
                    // Restore
                    if !self.cursor_save_under.is_empty()
                        && self.cursor_save_under.len() == (cw * ch) as usize
                    {
                        let mut idx = 0;
                        for row in 0..ch {
                            let y_idx = (cy + row as i32) as usize * w as usize;
                            for col in 0..cw {
                                buffer[y_idx + (cx + col as i32) as usize] =
                                    self.cursor_save_under[idx];
                                idx += 1;
                            }
                        }
                    } else {
                        self.cursor_save_under.clear();
                    }
                    // Draw
                    if is_cursor_on && self.focused_field.is_some() {
                        self.cursor_save_under.clear();
                        for row in 0..ch {
                            let y_idx = (cy + row as i32) as usize * w as usize;
                            for col in 0..cw {
                                self.cursor_save_under
                                    .push(buffer[y_idx + (cx + col as i32) as usize]);
                            }
                        }
                        draw_rect(&mut buffer, w, cx, cy, cw, ch, COLOR_PRIMARY, w, h);
                    } else {
                        self.cursor_save_under.clear();
                    }
                    self.last_state_hash = current_hash;
                    // Draw scrollbar even in cursor-only update
                    draw_main_scrollbar(
                        &mut buffer,
                        w,
                        h,
                        self.viewport_height,
                        self.content_height,
                        self.scroll_offset,
                    );
                    buffer.present().unwrap();
                    return;
                }
            }
        }

        // 4. Final Composition (Scrollbar + Cursor)
        // Always redraw scrollbar on top of whatever background we have
        draw_main_scrollbar(
            &mut buffer,
            w,
            h,
            self.viewport_height,
            self.content_height,
            self.scroll_offset,
        );

        // If background matches, we can safely draw the surgical cursor
        if base_state_hash == self.last_base_state_hash {
            self.cursor_save_under.clear();
            if is_cursor_on && self.focused_field.is_some() {
                if let Some((cx, cy, cw, ch)) = self.cursor_cache {
                    if cx >= 0
                        && cy >= 0
                        && (cx + cw as i32) <= w as i32
                        && (cy + ch as i32) <= h as i32
                    {
                        for row in 0..ch {
                            let y_idx = (cy + row as i32) as usize * w as usize;
                            for col in 0..cw {
                                self.cursor_save_under
                                    .push(buffer[y_idx + (cx + col as i32) as usize]);
                            }
                        }
                        draw_rect(&mut buffer, w, cx, cy, cw, ch, COLOR_PRIMARY, w, h);
                    }
                }
            }
        }

        self.last_state_hash = current_hash;
        buffer.present().unwrap();
    }

    pub fn handle_click(
        &mut self,
        x: f64,
        y: f64,
        _is_right_click: bool,
        ai_config: &crate::types::AiConfig,
    ) -> SettingsAction {
        let size = self.window.inner_size();
        let w = size.width as f32;
        let h = size.height as f32;
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let off_y = (h - 750.0 * scale) / 2.0;

        let lx = ((x as f32 - off_x) / scale) as f64;
        let ly = ((y as f32 - off_y) / scale) as f64;

        // DIALOG HANDLING
        if self.show_delete_dialog && self.current_tab == 2 {
            let dialog_w = 300.0;
            let dialog_h = 150.0;
            let dx = (800.0 - dialog_w) / 2.0;
            let dy = (750.0 - dialog_h) / 2.0;

            if lx >= dx && lx <= dx + dialog_w && ly >= dy && ly <= dy + dialog_h {
                // Inside dialog - check buttons
                let btn_y = dy + 85.0;
                let btn_w = 80.0;
                let btn_h = 35.0;

                // NO button
                let no_x = dx + 50.0;
                if lx >= no_x && lx <= no_x + btn_w && ly >= btn_y && ly <= btn_y + btn_h {
                    self.show_delete_dialog = false;
                    self.window.request_redraw();
                    return SettingsAction::None;
                }

                // YES button
                let yes_x = dx + 170.0;
                if lx >= yes_x && lx <= yes_x + btn_w && ly >= btn_y && ly <= btn_y + btn_h {
                    println!("[AI Settings] Delete Profile confirmed via dialog");
                    self.show_delete_dialog = false;
                    let mut config = ai_config.clone();
                    if config.profiles.len() > 1 {
                        config.profiles.remove(config.active_profile_index);
                        config.active_profile_index =
                            config.active_profile_index.min(config.profiles.len() - 1);
                        self.config_dirty = true;
                        self.notification =
                            Some(("Delete Success".to_string(), std::time::Instant::now()));
                        self.window.request_redraw();
                        return SettingsAction::UpdateAiConfig(config);
                    }
                    self.window.request_redraw();
                    return SettingsAction::None;
                }
                return SettingsAction::None; // Clicks inside dialog but not on buttons
            } else {
                // Click outside dialog closes it
                self.show_delete_dialog = false;
                self.window.request_redraw();
                return SettingsAction::None;
            }
        }

        self.is_dirty = true;
        let dlx = lx;
        let dly = ly - self.scroll_offset as f64;

        // Sidebar
        if lx >= 0.0 && lx <= 180.0 {
            for i in 0..5 {
                let ty = 60.0 + i as f64 * 80.0;
                if ly >= ty - 15.0 && ly <= ty + 45.0 {
                    self.current_tab = i;
                    self.scroll_offset = 0.0;
                    self.focused_field = None;
                    if i == 3 {
                        return SettingsAction::RequestHistory;
                    }
                    self.window.request_redraw();
                    return SettingsAction::RequestGc;
                }
            }
        }

        // Scrollbar Drag & Jump
        if self.content_height > self.viewport_height {
            if lx >= 770.0 && lx <= 810.0 && ly >= 130.0 && ly <= 730.0 {
                self.is_dragging_scrollbar = true;
                // Immediate jump
                let track_ly_start = 130.0;
                let track_ly_end = 730.0;
                let progress =
                    ((ly - track_ly_start) / (track_ly_end - track_ly_start)).clamp(0.0, 1.0);
                let max_scroll = -(self.content_height - self.viewport_height);
                self.scroll_offset = progress as f32 * max_scroll;
                self.window.request_redraw();
                return SettingsAction::None;
            }
        }

        match self.current_tab {
            1 => {
                // Tab 1: General
                let card_w = 560.0;
                let scroll_y = self.scroll_offset as f64;
                let card1_y = 120.0 + scroll_y;
                let card2_y = 280.0 + scroll_y;
                let card3_y = 505.0 + scroll_y;
                let card4_y = 665.0 + scroll_y;

                if lx >= 210.0 && lx <= 210.0 + card_w {
                    // Pet Scale
                    if ly >= card1_y + 60.0 && ly <= card1_y + 105.0 {
                        let scales = vec![0.5, 0.75, 1.0, 1.25, 1.5];
                        for (i, &val) in scales.iter().enumerate() {
                            let mx = 220.0 + i as f64 * 85.0;
                            if lx >= mx && lx <= mx + 75.0 {
                                return SettingsAction::SetScale(val);
                            }
                        }
                    }
                    // Behavior
                    let modes = vec![
                        BehaviorMode::Static,
                        BehaviorMode::Quiet,
                        BehaviorMode::Active,
                        BehaviorMode::Clingy,
                    ];
                    for (i, mode) in modes.into_iter().enumerate() {
                        let row = i / 2;
                        let col = i % 2;
                        let mx = 230.0 + col as f64 * 165.0;
                        let my = card2_y + 60.0 + row as f64 * 65.0;
                        if let Some((rx, ry, rw, rh)) = self.active_sys_prompt_rect {
                            if lx >= rx && lx <= (rx + rw) && ly >= ry && ly <= (ry + rh) {
                                self.is_dragging_text = true;
                                self.dragging_sys_prompt = true;
                            }
                        }
                        if lx >= mx && lx <= mx + 150.0 && ly >= my && ly <= my + 55.0 {
                            return SettingsAction::SetMode(mode);
                        }
                    }
                    // Music
                    if ly >= card3_y + 60.0 && ly <= card3_y + 105.0 {
                        if lx >= 230.0 && lx <= 730.0 {
                            return SettingsAction::SelectMusicPath;
                        }
                    }
                    // Layer
                    if ly >= card4_y + 60.0 && ly <= card4_y + 115.0 {
                        if lx >= 230.0 && lx <= 430.0 {
                            return SettingsAction::SetLayer(WindowLayer::Top);
                        }
                        if lx >= 440.0 && lx <= 640.0 {
                            return SettingsAction::SetLayer(WindowLayer::Bottom);
                        }
                    }
                }

                // Monitor selection
                let card5_y = 825.0 + scroll_y;
                let rows = (self.available_monitors.len() + 2) / 3;
                let card5_h = 60.0 + (rows as f64 * 65.0);
                if dlx >= 210.0
                    && dlx <= 210.0 + card_w
                    && dly >= card5_y + 60.0
                    && dly <= card5_y + card5_h
                {
                    for (i, (name, _)) in self.available_monitors.iter().enumerate() {
                        let row = i / 3;
                        let col = i % 3;
                        let mx = 230.0 + col as f64 * 110.0;
                        let my = card5_y + 60.0 + row as f64 * 65.0;
                        if dlx >= mx && dlx <= mx + 100.0 && dly >= my && dly <= my + 55.0 {
                            return SettingsAction::SetMonitor(name.clone());
                        }
                    }
                }
            }
            2 => {
                // Tab 2: AI
                let design_card_y = 120.0;

                // Priority: Sub-scrollbar
                // Note: dly already includes scroll_offset adjustment (dly = ly - scroll_offset)
                // So we need to compare with design-space coordinates directly
                if lx >= 230.0 + 480.0 && lx <= 230.0 + 480.0 + 8.0 {
                    let input_y = design_card_y + 930.0 + 25.0; // Design-space Y
                    let track_h = 250.0;
                    if dly >= input_y && dly <= input_y + track_h {
                        self.dragging_sys_prompt = true;
                        // Calculate progress (0.0 at top, 1.0 at bottom)
                        let progress = ((dly - input_y) / track_h).clamp(0.0, 1.0);
                        // Calculate scroll offset (negative value, 0 at top, -max at bottom)
                        let max_scroll = (self.active_sys_prompt_content_height - 250.0).max(0.0);
                        self.system_prompt_scroll_offset = -(progress * max_scroll as f64) as f32;
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                }

                let fields = vec![
                    (265.0, 30.0, 160.0),   // 0: Profile Name (Redesigned row)
                    (565.0, 30.0, 45.0),    // 1: Multimodal Toggle
                    (230.0, 130.0, 500.0),  // 2: Key
                    (230.0, 230.0, 500.0),  // 3: URL
                    (230.0, 330.0, 500.0),  // 4: Model
                    (230.0, 430.0, 150.0),  // 5: Steps
                    (405.0, 430.0, 150.0),  // 6: L1
                    (580.0, 430.0, 150.0),  // 7: L2
                    (230.0, 530.0, 150.0),  // 8: Interval
                    (230.0, 630.0, 500.0),  // 9: Tavily
                    (230.0, 730.0, 500.0),  // 10: Brave
                    (230.0, 830.0, 500.0),  // 11: FC URL
                    (230.0, 930.0, 500.0),  // 12: FC Key
                    (230.0, 1030.0, 500.0), // 13: System
                ];

                // Profile Management Buttons (Standardized Row)
                let btn_y_start = design_card_y + 30.0 + 25.0;
                let btn_y_end = btn_y_start + 45.0;

                // [<] Prev Profile (at 230)
                if dlx >= 230.0 && dlx <= 260.0 && dly >= btn_y_start && dly <= btn_y_end {
                    println!("[AI Settings] Prev Profile clicked");
                    self.pressed_btn = Some(0);
                    let mut config = ai_config.clone();
                    if config.active_profile_index > 0 {
                        config.active_profile_index -= 1;
                    } else if !config.profiles.is_empty() {
                        config.active_profile_index = config.profiles.len() - 1;
                    }
                    self.config_dirty = true;
                    self.window.request_redraw();
                    return SettingsAction::UpdateAiConfig(config);
                }

                // [>] Next Profile (at 430)
                if dlx >= 430.0 && dlx <= 460.0 && dly >= btn_y_start && dly <= btn_y_end {
                    println!("[AI Settings] Next Profile clicked");
                    self.pressed_btn = Some(1);
                    let mut config = ai_config.clone();
                    if !config.profiles.is_empty() {
                        config.active_profile_index =
                            (config.active_profile_index + 1) % config.profiles.len();
                    }
                    self.config_dirty = true;
                    self.window.request_redraw();
                    return SettingsAction::UpdateAiConfig(config);
                }

                // [+] Add Profile (at 480)
                if dlx >= 480.0 && dlx <= 515.0 && dly >= btn_y_start && dly <= btn_y_end {
                    println!("[AI Settings] Add Profile clicked");
                    self.show_delete_dialog = false;
                    self.pressed_btn = Some(2);
                    let mut config = ai_config.clone();
                    // Ensure unique name for the new profile
                    let base_name = "New Profile".to_string();
                    let mut final_name = base_name.clone();
                    let mut counter = 2;
                    while config.profiles.iter().any(|p| p.name == final_name) {
                        final_name = format!("{} ({})", base_name, counter);
                        counter += 1;
                    }

                    let mut new_profile = crate::types::AiProfile::default();
                    new_profile.name = final_name;
                    config.profiles.push(new_profile);
                    config.active_profile_index = config.profiles.len() - 1;
                    self.config_dirty = true;
                    self.notification =
                        Some(("Add Success".to_string(), std::time::Instant::now()));
                    self.window.request_redraw();
                    return SettingsAction::UpdateAiConfig(config);
                }

                // [-] Delete Profile (at 525)
                if dlx >= 525.0 && dlx <= 560.0 && dly >= btn_y_start && dly <= btn_y_end {
                    println!("[AI Settings] Delete Profile clicked (showing dialog)");
                    self.show_delete_dialog = true;
                    self.pressed_btn = Some(3);
                    self.window.request_redraw();
                    return SettingsAction::None;
                }

                for (i, (fx, fy, fw)) in fields.iter().enumerate() {
                    let input_y = design_card_y + fy + 25.0;
                    let input_h = if i == 13 { 250.0 } else { 45.0 };

                    if dlx >= *fx && dlx <= *fx + *fw && dly >= input_y && dly <= input_y + input_h
                    {
                        if i == 1 {
                            println!("[AI Settings] Multimodal Toggle clicked");
                            self.pressed_btn = Some(101); // Special code for multimodal
                                                          // Toggle Multimodal
                            let mut config = ai_config.clone();
                            let profile = config.active_profile_mut();
                            profile.is_multimodal = !profile.is_multimodal;
                            self.config_dirty = true;
                            self.window.request_redraw();
                            return SettingsAction::UpdateAiConfig(config);
                        }

                        self.focused_field = Some(i);
                        self.last_cursor_action = std::time::Instant::now();
                        let text = self.get_field_text(i, ai_config);

                        if i == 13 {
                            // System prompt multi-line
                            let scale_f32 = scale as f32;
                            let scroll_y = self.system_prompt_scroll_offset * scale_f32;
                            let layout_x = ((lx - fx - 15.0) * scale as f64) as f32;
                            let layout_y =
                                ((dly - input_y - 12.0) * scale as f64) as f32 - scroll_y;
                            self.cursor_pos =
                                self.get_cursor_from_xy(&text, layout_x, layout_y, scale_f32);

                            if !_is_right_click {
                                self.selection_start = Some(self.cursor_pos);
                                self.is_dragging_text = true;
                            }
                        } else {
                            if !_is_right_click {
                                if lx >= *fx + *fw - 45.0
                                    && (i == 2 || i == 9 || i == 10 || i == 12)
                                {
                                    self.show_api_key = !self.show_api_key;
                                    self.config_dirty = true;
                                } else {
                                    let scale_f32 = scale as f32;
                                    let scroll_x = self.field_scroll_offsets[i];
                                    let layout_x =
                                        ((lx - fx - 15.0) * scale as f64) as f32 - scroll_x;
                                    self.cursor_pos =
                                        self.get_cursor_from_x(&text, layout_x, scale_f32);
                                    self.selection_start = Some(self.cursor_pos);
                                    self.is_dragging_text = true;
                                }
                            }
                        }
                        let scale_f32 = scale as f32;
                        self.ensure_cursor_visible(i, scale_f32, ai_config);
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                }

                self.focused_field = None;
                self.selection_start = None;
                self.window.request_redraw();
                return SettingsAction::None;
            }
            3 => {
                // Tab 3: History
                if dlx >= 230.0 + 480.0 && dlx <= 230.0 + 480.0 + 8.0 {
                    for (i, (rx_start, ry_start, rx_end, ry_end)) in
                        self.history_item_rects.iter().enumerate()
                    {
                        if dlx >= *rx_start && dlx <= *rx_end && dly >= *ry_start && dly <= *ry_end
                        {
                            // Hit a history item row's X range?
                            // Actually history.rs draws scrollbar at s(230 + 480)
                            let track_y_start = *ry_start + 35.0;
                            let track_h = 140.0;
                            if dly >= track_y_start && dly <= track_y_start + track_h {
                                self.dragging_history_idx = Some(i);
                                let progress = ((dly - track_y_start) / track_h).clamp(0.0, 1.0);
                                let content_h_logical = if self.history_metrics_cache.len() > i {
                                    self.history_metrics_cache[i] / scale as f32
                                } else {
                                    0.0
                                };
                                let max_scroll = -(content_h_logical - 140.0).max(0.0);
                                self.history_scroll_states[i] = progress as f32 * max_scroll;
                                self.window.request_redraw();
                                return SettingsAction::None;
                            }
                        }
                    }
                }
            }
            _ => {}
        }

        SettingsAction::None
    }

    fn get_field_text(&self, idx: usize, ai_config: &AiConfig) -> String {
        let active_profile = ai_config.active_profile();
        match idx {
            0 => active_profile.name.clone(),
            1 => String::new(), // Toggle handled via click
            2 => active_profile.api_key.clone(),
            3 => active_profile.base_url.clone(),
            4 => active_profile.model.clone(),
            5 => {
                if ai_config.react_limit == 0 {
                    String::new()
                } else {
                    ai_config.react_limit.to_string()
                }
            }
            6 => {
                if ai_config.l1_summary_threshold == 0 {
                    String::new()
                } else {
                    ai_config.l1_summary_threshold.to_string()
                }
            }
            7 => {
                if ai_config.l2_merge_threshold == 0 {
                    String::new()
                } else {
                    ai_config.l2_merge_threshold.to_string()
                }
            }
            8 => {
                if ai_config.interaction_frequency == 0 {
                    String::new()
                } else {
                    ai_config.interaction_frequency.to_string()
                }
            }
            9 => ai_config.tavily_api_key.clone(),
            10 => ai_config.brave_api_key.clone(),
            11 => ai_config.firecrawl_url.clone(),
            12 => ai_config.firecrawl_api_key.clone(),
            13 => ai_config.system_prompt.clone(),
            _ => String::new(),
        }
    }

    fn set_field_text(&mut self, idx: usize, ai_config: &mut AiConfig, text: String) {
        self.config_dirty = true;
        match idx {
            0 | 2 | 3 | 4 => {
                if idx == 0 {
                    // Check for duplicate name BEFORE mutable borrow
                    let is_duplicate = ai_config
                        .profiles
                        .iter()
                        .enumerate()
                        .any(|(i, p)| i != ai_config.active_profile_index && p.name == text);
                    if is_duplicate {
                        self.notification =
                            Some(("Name already exists".to_string(), std::time::Instant::now()));
                        return;
                    }
                }

                let profile = ai_config.active_profile_mut();
                match idx {
                    0 => profile.name = text,
                    2 => profile.api_key = text,
                    3 => profile.base_url = text,
                    4 => profile.model = text,
                    _ => {}
                }
            }
            5 => {
                ai_config.react_limit = text.parse().unwrap_or(0);
            }
            6 => {
                ai_config.l1_summary_threshold = text.parse().unwrap_or(0);
            }
            7 => {
                ai_config.l2_merge_threshold = text.parse().unwrap_or(0);
            }
            8 => {
                ai_config.interaction_frequency = text.parse().unwrap_or(0);
            }
            9 => ai_config.tavily_api_key = text,
            10 => ai_config.brave_api_key = text,
            11 => ai_config.firecrawl_url = text,
            12 => ai_config.firecrawl_api_key = text,
            13 => {
                self.system_prompt_hash = 0; // Force re-hash/re-render in ai.rs
                ai_config.system_prompt = text;
            }
            _ => {}
        }
    }

    fn ensure_cursor_visible(&mut self, field_idx: usize, scale: f32, ai_config: &AiConfig) {
        if field_idx >= 14 {
            return;
        }

        if field_idx == 13 {
            let text = &ai_config.system_prompt;
            let mut measurement_text = text.clone();
            if measurement_text.ends_with('\n') {
                measurement_text.push(' ');
            }
            let (_, py, ch) = self.get_xy_from_cursor(&measurement_text, self.cursor_pos, scale);
            let py_logical = py as f32; // Relative to text start (scaled)
            let ch_scaled = ch as f32; // Line height (scaled)

            // Text viewport is smaller than box (padded by 12px top/bottom)
            let viewport_h_scaled = (250.0 - 24.0) * scale;
            let mut current_scroll = self.system_prompt_scroll_offset * scale;

            let top_y = py_logical + current_scroll;
            let bottom_y = top_y + ch_scaled;

            // Pad by 10/30px to keep cursor from hitting edges
            if top_y < 10.0 {
                current_scroll = (10.0 - py_logical).min(0.0);
            } else if bottom_y > viewport_h_scaled - 10.0 {
                current_scroll = (viewport_h_scaled - 10.0 - (py_logical + ch_scaled)).min(0.0);
            }

            // Content-aware clamping
            let (_, mh): (f32, f32) =
                get_metrics_dw(&measurement_text, 14.0 * scale, (460.0 * scale) as u32);
            let content_h = mh + 24.0 * scale;
            let min_scroll = (250.0 * scale - content_h).min(0.0f32);
            current_scroll = current_scroll.max(min_scroll).min(0.0);

            self.system_prompt_scroll_offset = current_scroll / scale;
            return;
        }

        // Single-line fields
        if field_idx >= 14 {
            return;
        } // Skip multiline or invalid

        let fields = vec![
            (265.0, 30.0, 160.0),   // 0: Profile Name
            (565.0, 30.0, 45.0),    // 1: Multimodal
            (230.0, 130.0, 500.0),  // 2: Key
            (230.0, 230.0, 500.0),  // 3: URL
            (230.0, 330.0, 500.0),  // 4: Model
            (230.0, 430.0, 150.0),  // 5: Steps
            (405.0, 430.0, 150.0),  // 6: L1
            (580.0, 430.0, 150.0),  // 7: L2
            (230.0, 530.0, 150.0),  // 8: Int Freq
            (230.0, 630.0, 500.0),  // 9: Tavily Key
            (230.0, 730.0, 500.0),  // 10: Brave Key
            (230.0, 830.0, 500.0),  // 11: FC URL
            (230.0, 930.0, 500.0),  // 12: FC Key
            (230.0, 1030.0, 500.0), // 13: System
        ];

        if field_idx >= fields.len() {
            return;
        }
        let (_fx, _, fw) = fields[field_idx];
        let text = self.get_field_text(field_idx, ai_config);
        let font_size = 14.0 * scale;
        let (total_text_w, _): (f32, f32) = get_metrics_dw(&text, font_size, 1000000);

        let viewport_w_scaled = (fw - 30.0) as f32 * scale;
        let (px, _, _) = self.get_xy_from_cursor(&text, self.cursor_pos, scale);
        let cx_logical = px as f32; // This is physical pixels relative to text start

        let mut current_scroll = self.field_scroll_offsets[field_idx];

        // 1. Ensure cursor visibility
        let visible_x = cx_logical + current_scroll;
        if visible_x < 5.0 {
            current_scroll = (5.0f32 - cx_logical).min(0.0);
        } else if visible_x > viewport_w_scaled - 5.0 {
            current_scroll = (viewport_w_scaled - 5.0f32 - cx_logical).min(0.0);
        }

        // 2. Snap-back logic: Ensure we don't show empty space at the end if the text is longer than viewport
        let min_scroll = (viewport_w_scaled - total_text_w).min(0.0f32);
        current_scroll = current_scroll.max(min_scroll).min(0.0);

        self.field_scroll_offsets[field_idx] = current_scroll;
    }

    fn get_cursor_from_x(&self, text: &str, layout_x: f32, scale: f32) -> usize {
        get_cursor_index_from_xy(text, 14.0 * scale, 1000000, layout_x, 7.0 * scale)
    }

    fn get_cursor_from_xy(&self, text: &str, layout_x: f32, layout_y: f32, scale: f32) -> usize {
        let field_idx = self.focused_field.unwrap_or(0);
        let max_width = if field_idx == 13 {
            460.0 * scale
        } else {
            1000000.0
        };
        get_cursor_index_from_xy(text, 14.0 * scale, max_width as u32, layout_x, layout_y)
    }

    fn get_xy_from_cursor(&self, text: &str, cursor_pos: usize, scale: f32) -> (f64, f64, f64) {
        let field_idx = self.focused_field.unwrap_or(0);
        let max_width = if field_idx == 13 {
            460.0 * scale
        } else {
            1000000.0
        };
        let (px, py, ch) =
            get_xy_from_cursor_index(text, 14.0 * scale, max_width as u32, cursor_pos);
        (px as f64, py as f64, ch as f64)
    }

    pub fn handle_key_input(
        &mut self,
        event: &winit::event::KeyEvent,
        ai_config: &mut AiConfig,
        modifiers: winit::keyboard::ModifiersState,
    ) -> bool {
        let size = self.window.inner_size();
        let scale = ((size.width as f32 / 800.0).min(size.height as f32 / 750.0)) as f32;
        self.last_cursor_action = std::time::Instant::now();
        if self.current_tab != 2 {
            return false;
        }

        let field_idx = match self.focused_field {
            Some(i) => i,
            None => return false,
        };

        let text = self.get_field_text(field_idx, ai_config);
        let mut chars: Vec<char> = text.chars().collect();

        if self.cursor_pos > chars.len() {
            self.cursor_pos = chars.len();
        }

        // use windows::Win32::Graphics::Gdi::{GetDC, ReleaseDC};
        use winit::keyboard::{Key, NamedKey};
        let is_pressed = event.state == winit::event::ElementState::Pressed;
        if !is_pressed {
            return false;
        }

        let has_ctrl = modifiers.control_key() || modifiers.super_key();
        let has_shift = modifiers.shift_key();

        if let Key::Named(NamedKey::ArrowUp) = &event.logical_key {
            if field_idx == 13 {
                let (lx, ly, _) = self.get_xy_from_cursor(&text, self.cursor_pos, scale);
                let line_height = 20.0 * scale; // pixels
                self.cursor_pos = self.get_cursor_from_xy(
                    &text,
                    lx as f32,
                    ly as f32 - line_height + 5.0 * scale,
                    scale,
                );
                if !has_shift {
                    self.selection_start = None;
                }
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                self.window.request_redraw();
                return true;
            }
        }
        if let Key::Named(NamedKey::ArrowDown) = &event.logical_key {
            if field_idx == 13 {
                let (lx, ly, _) = self.get_xy_from_cursor(&text, self.cursor_pos, scale);
                let line_height = 20.0 * scale; // pixels
                self.cursor_pos = self.get_cursor_from_xy(
                    &text,
                    lx as f32,
                    ly as f32 + line_height + 5.0 * scale,
                    scale,
                );
                if !has_shift {
                    self.selection_start = None;
                }
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                self.window.request_redraw();
                return true;
            }
        }

        match &event.logical_key {
            Key::Named(NamedKey::ArrowLeft) => {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                if self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                }
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::ArrowRight) => {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                if self.cursor_pos < chars.len() {
                    self.cursor_pos += 1;
                }
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::Home) => {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                self.cursor_pos = 0;
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::End) => {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                self.cursor_pos = chars.len();
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(start) = self.selection_start {
                    let min = start.min(self.cursor_pos);
                    let max = start.max(self.cursor_pos);
                    if min != max {
                        chars.drain(min..max);
                        self.cursor_pos = min;
                        self.selection_start = None;
                    } else {
                        self.selection_start = None;
                        if self.cursor_pos > 0 {
                            chars.remove(self.cursor_pos - 1);
                            self.cursor_pos -= 1;
                        }
                    }
                } else if self.cursor_pos > 0 {
                    chars.remove(self.cursor_pos - 1);
                    self.cursor_pos -= 1;
                }
                self.set_field_text(field_idx, ai_config, chars.iter().collect());
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                ai_config.save();
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::Delete) => {
                if let Some(start) = self.selection_start {
                    let min = start.min(self.cursor_pos);
                    let max = start.max(self.cursor_pos);
                    if min != max {
                        chars.drain(min..max);
                        self.cursor_pos = min;
                        self.selection_start = None;
                    } else {
                        self.selection_start = None;
                        if self.cursor_pos < chars.len() {
                            chars.remove(self.cursor_pos);
                        }
                    }
                } else if self.cursor_pos < chars.len() {
                    chars.remove(self.cursor_pos);
                }
                self.set_field_text(field_idx, ai_config, chars.iter().collect());
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                ai_config.save();
                self.window.request_redraw();
                return true;
            }
            Key::Named(NamedKey::Enter) => {
                if field_idx == 13 {
                    if let Some(start) = self.selection_start {
                        let min = start.min(self.cursor_pos);
                        let max = start.max(self.cursor_pos);
                        chars.drain(min..max);
                        self.cursor_pos = min;
                        self.selection_start = None;
                    }
                    chars.insert(self.cursor_pos, '\n');
                    self.cursor_pos += 1;
                    self.set_field_text(field_idx, ai_config, chars.iter().collect());
                    self.ensure_cursor_visible(field_idx, scale, ai_config);
                    ai_config.save();
                    self.window.request_redraw();
                    return true;
                }
                return false;
            }
            Key::Character(c) => {
                if has_ctrl {
                    if c == "a" {
                        self.selection_start = Some(0);
                        self.cursor_pos = chars.len();
                        self.window.request_redraw();
                        return true;
                    } else if c == "c" {
                        if let Some(start) = self.selection_start {
                            let min = start.min(self.cursor_pos);
                            let max = start.max(self.cursor_pos);
                            if min != max {
                                let selected: String = chars[min..max].iter().collect();
                                use arboard::Clipboard;
                                if let Ok(mut cb) = Clipboard::new() {
                                    let _ = cb.set_text(selected);
                                }
                            }
                        }
                        return true;
                    } else if c == "v" {
                        use arboard::Clipboard;
                        if let Ok(mut cb) = Clipboard::new() {
                            if let Ok(pasted) = cb.get_text() {
                                let trimmed = pasted.trim();
                                let p_chars: Vec<char> = trimmed.chars().collect();
                                if let Some(start) = self.selection_start {
                                    let min = start.min(self.cursor_pos);
                                    let max = start.max(self.cursor_pos);
                                    chars.splice(min..max, p_chars.iter().cloned());
                                    self.cursor_pos = min + p_chars.len();
                                    self.selection_start = None;
                                } else {
                                    chars.splice(
                                        self.cursor_pos..self.cursor_pos,
                                        p_chars.iter().cloned(),
                                    );
                                    self.cursor_pos += p_chars.len();
                                }
                                self.set_field_text(field_idx, ai_config, chars.iter().collect());
                                self.ensure_cursor_visible(field_idx, scale, ai_config);
                                ai_config.save();
                                self.window.request_redraw();
                            }
                        }
                        return true;
                    }
                }

                if !c.chars().any(|ch| ch.is_control()) {
                    let input_chars: Vec<char> = c.chars().collect();
                    if (field_idx == 5 || field_idx == 6 || field_idx == 7 || field_idx == 8)
                        && !input_chars.iter().all(|ch| ch.is_ascii_digit())
                    {
                        return true;
                    }

                    if let Some(start) = self.selection_start {
                        let min = start.min(self.cursor_pos);
                        let max = start.max(self.cursor_pos);
                        chars.splice(min..max, input_chars.iter().cloned());
                        self.cursor_pos = min + input_chars.len();
                        self.selection_start = None;
                    } else {
                        chars.splice(
                            self.cursor_pos..self.cursor_pos,
                            input_chars.iter().cloned(),
                        );
                        self.cursor_pos += input_chars.len();
                    }
                    self.set_field_text(field_idx, ai_config, chars.iter().collect());
                    self.ensure_cursor_visible(field_idx, scale, ai_config);
                    ai_config.save();
                    self.window.request_redraw();
                    return true;
                }
            }
            Key::Named(NamedKey::Tab) => {
                self.focused_field = Some((field_idx + 1) % 14);
                self.cursor_pos = 0;
                self.selection_start = None;
                self.window.request_redraw();
                return true;
            }
            _ => {}
        }
        false
    }

    pub fn handle_ime(&mut self, text: &str, ai_config: &mut AiConfig) -> bool {
        if self.current_tab != 2 {
            return false;
        }
        if let Some(idx) = self.focused_field {
            let val = self.get_field_text(idx, ai_config);
            let mut chars: Vec<char> = val.chars().collect();

            if self.cursor_pos > chars.len() {
                self.cursor_pos = chars.len();
            }

            let input_chars: Vec<char> = text.chars().collect();

            // Support selection replacement in IME too
            if let Some(start) = self.selection_start {
                let min = start.min(self.cursor_pos);
                let max = start.max(self.cursor_pos);
                chars.splice(min..max, input_chars.iter().cloned());
                self.cursor_pos = min + input_chars.len();
                self.selection_start = None;
            } else {
                chars.splice(
                    self.cursor_pos..self.cursor_pos,
                    input_chars.iter().cloned(),
                );
                self.cursor_pos += input_chars.len();
            }

            self.set_field_text(idx, ai_config, chars.iter().collect());
            self.config_dirty = true;
            if idx == 13 {
                // Invalidate system prompt metrics cache
                self.system_prompt_hash = 0;
                self.system_prompt_metrics_cache = 0.0;
            }
            ai_config.save();
            let size = self.window.inner_size();
            let scale = ((size.width as f32 / 800.0).min(size.height as f32 / 750.0)) as f32;
            self.ensure_cursor_visible(idx, scale, ai_config);
            self.last_cursor_action = std::time::Instant::now();
            self.window.request_redraw();
            self.is_dirty = true; // Added for handle_key (IME is a form of key input)
            return true;
        }
        false
    }

    pub fn handle_scroll(
        &mut self,
        dy: f32,
        cursor_pos: Option<winit::dpi::PhysicalPosition<f64>>,
    ) {
        self.is_dirty = true;
        let size = self.window.inner_size();
        let w = size.width as f64;
        let h = size.height as f64;
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let off_y = (h - 750.0 * scale) / 2.0;
        let dy_logical = dy / scale as f32;

        let (lx, ly) = if let Some(pos) = cursor_pos {
            ((pos.x - off_x) / scale, (pos.y - off_y) / scale)
        } else {
            (-1000.0, -1000.0)
        };

        // Design-space coordinates for hit detection
        let dlx = lx;
        let dly = ly - self.scroll_offset as f64;

        if self.current_tab == 3 {
            // History Tab
            let mut scrolled_item = false;
            if self.history_item_rects.len() == self.history.len()
                && self.history_metrics_cache.len() == self.history.len()
                && self.history_scroll_states.len() == self.history.len()
            {
                for (i, (rx_start, ry_start, rx_end, ry_end)) in
                    self.history_item_rects.iter().enumerate()
                {
                    if dlx >= *rx_start && dlx <= *rx_end && dly >= *ry_start && dly <= *ry_end {
                        let item_h_fixed_sc = 180.0 * scale as f32;
                        let full_h = self.history_metrics_cache[i].max(20.0 * scale as f32);
                        let view_h = item_h_fixed_sc - (40.0 * scale as f32);

                        if full_h > view_h {
                            let current_log = self.history_scroll_states[i];
                            let scroll_step_log = dy_logical;
                            let new_val_log = current_log + scroll_step_log;
                            let max_scroll_log = -(full_h - view_h) / scale as f32;
                            let clamped_log = new_val_log.clamp(max_scroll_log, 0.0);

                            if (clamped_log - current_log).abs() > 0.001 {
                                self.history_scroll_states[i] = clamped_log;
                                scrolled_item = true;
                            } else {
                                if (dy_logical > 0.0 && current_log >= -0.01)
                                    || (dy_logical < 0.0 && current_log <= max_scroll_log + 0.01)
                                {
                                    scrolled_item = false;
                                } else {
                                    scrolled_item = true;
                                }
                            }
                        }
                        break;
                    }
                }
            }

            if !scrolled_item {
                self.scroll_offset += dy_logical;
                let min_offset = -(self.content_height - self.viewport_height).max(0.0);
                self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
            }
        } else if self.current_tab == 2 {
            // AI Brain Tab
            let mut scrolled_sys_prompt = false;
            if let Some((min_x, min_y, max_x, max_y)) = self.active_sys_prompt_rect {
                if dlx >= min_x && dlx <= max_x && dly >= min_y && dly <= max_y {
                    let old_off = self.system_prompt_scroll_offset;
                    self.system_prompt_scroll_offset += dy_logical;
                    let view_h = 250.0;
                    let content_h = self.active_sys_prompt_content_height;
                    let min_offset = -(content_h - view_h).max(0.0);
                    self.system_prompt_scroll_offset =
                        self.system_prompt_scroll_offset.clamp(min_offset, 0.0);

                    if (self.system_prompt_scroll_offset - old_off).abs() > 0.01 {
                        scrolled_sys_prompt = true;
                    } else {
                        if (dy_logical > 0.0 && self.system_prompt_scroll_offset >= -0.01)
                            || (dy_logical < 0.0
                                && self.system_prompt_scroll_offset <= min_offset + 0.01)
                        {
                            scrolled_sys_prompt = false;
                        } else {
                            scrolled_sys_prompt = true;
                        }
                    }
                }
            }

            if !scrolled_sys_prompt {
                self.scroll_offset += dy_logical;
                let min_offset = -(self.content_height - self.viewport_height).max(0.0);
                self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
            }
        } else {
            self.scroll_offset += dy_logical;
            let min_offset = -(self.content_height - self.viewport_height).max(0.0);
            self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
        }
        self.window.request_redraw();
    }

    pub fn handle_mouse_move(&mut self, x: f64, y: f64, ai_config: &crate::types::AiConfig) {
        let size = self.window.inner_size();
        let w = size.width as f64;
        let h = size.height as f64;
        let scale = (w / 800.0).min(h / 750.0);
        let off_x = (w - 800.0 * scale) / 2.0;
        let off_y = (h - 750.0 * scale) / 2.0;

        let lx = (x - off_x) / scale;
        let ly = (y - off_y) / scale;

        // Sidebar hover
        if lx >= 0.0 && lx <= 180.0 {
            for i in 0..5 {
                let ty = 60.0 + i as f64 * 80.0;
                if ly >= ty - 15.0 && ly <= ty + 45.0 {
                    if self.current_tab != i {
                        // For sidebar hover
                    }
                }
            }
        }
        let dlx = lx as f32;
        let dly = (ly - self.scroll_offset as f64) as f32;
        self.mouse_pos = (x as f32, y as f32); // Store RAW coordinates for renderer

        if self.is_dragging_scrollbar {
            if self.content_height > self.viewport_height {
                let track_ly_start = 130.0;
                let track_ly_end = 730.0;
                let progress =
                    ((ly - track_ly_start) / (track_ly_end - track_ly_start)).clamp(0.0, 1.0);
                let max_scroll = -(self.content_height - self.viewport_height);
                self.scroll_offset = progress as f32 * max_scroll;
                self.window.request_redraw();
            }
            return;
        }

        if let Some(idx) = self.dragging_history_idx {
            if idx < self.history.len() {
                let view_h = 140.0;
                let full_h_logical = if self.history_metrics_cache.len() > idx {
                    self.history_metrics_cache[idx] / scale as f32
                } else {
                    0.0
                };

                if self.history_item_rects.len() > idx {
                    let (_, ry_start, _, _) = self.history_item_rects[idx];
                    let track_y_start = ry_start + 35.0;
                    let track_h = view_h as f64;
                    let progress = ((dly as f64 - track_y_start) / track_h).clamp(0.0, 1.0);
                    let max_scroll = -(full_h_logical - view_h as f32).max(0.0);
                    self.history_scroll_states[idx] = progress as f32 * max_scroll;
                    self.window.request_redraw();
                }
            }
            return;
        }

        if self.dragging_sys_prompt {
            if let Some((_, ry_start, _, ry_end)) = self.active_sys_prompt_rect {
                let track_h = (ry_end - ry_start).max(1.0);
                let progress = ((dly as f64 - ry_start) / track_h).clamp(0.0, 1.0);
                let view_h = track_h as f32;
                let content_h = self.active_sys_prompt_content_height;
                let max_scroll = -(content_h - view_h).max(0.0);
                self.system_prompt_scroll_offset = progress as f32 * max_scroll;
                self.window.request_redraw();
                return;
            } else {
                // Fallback to hardcoded if rect not set yet
                let track_h = 250.0;
                let track_y_start = 120.0 + 930.0 + 25.0;
                let progress = ((dly - track_y_start) / track_h).clamp(0.0, 1.0);
                let view_h = 250.0f32;
                let content_h = self.active_sys_prompt_content_height;
                let max_scroll = -(content_h - view_h).max(0.0);
                self.system_prompt_scroll_offset = progress as f32 * max_scroll;
                self.window.request_redraw();
                return;
            }
        }

        if !self.is_dragging_text {
            // Trigger redraw for hover effects in AI tab content area
            if self.current_tab == 2 && lx > 180.0 {
                self.window.request_redraw();
            }
            return;
        }

        self.last_cursor_action = std::time::Instant::now();
        let field_idx = match self.focused_field {
            Some(i) => i,
            None => {
                self.is_dragging_text = false;
                return;
            }
        };

        let val = self.get_field_text(field_idx, ai_config);
        let fields = vec![
            (270.0, 30.0),   // 0: Profile Name
            (685.0, 30.0),   // 1: Multimodal
            (230.0, 130.0),  // 2: Key
            (230.0, 230.0),  // 3: URL
            (230.0, 330.0),  // 4: Model
            (230.0, 430.0),  // 5: Steps
            (405.0, 430.0),  // 6: L1
            (580.0, 430.0),  // 7: L2
            (230.0, 530.0),  // 8: Interval
            (230.0, 630.0),  // 9: Tavily
            (230.0, 730.0),  // 10: Brave
            (230.0, 830.0),  // 11: FC URL
            (230.0, 930.0),  // 12: FC Key
            (230.0, 1030.0), // 13: System
        ];

        let (fx, fy) = fields[field_idx];
        let design_card_y = 120.0;
        let input_y = design_card_y + fy + 25.0;
        let text_x = dlx as f64 - fx - 15.0;

        if !text_x.is_finite() || !dly.is_finite() {
            return;
        }

        if field_idx == 13 {
            // Multi-line cursor drag for System Prompt
            let scale_f32 = scale as f32;
            let scroll_y = self.system_prompt_scroll_offset * scale_f32;
            let layout_x = (text_x as f32) * scale_f32;
            let layout_y = (dly - input_y - 12.0) as f32 * scale_f32 - scroll_y;
            if layout_y.is_finite() {
                self.cursor_pos = self.get_cursor_from_xy(&val, layout_x, layout_y, scale_f32);
            }
        } else if field_idx != 1 {
            // Skip multimodal toggle
            let scale_f32 = scale as f32;
            let scroll_x = self.field_scroll_offsets[field_idx];
            let layout_x = (text_x as f32) * scale_f32 - scroll_x;
            self.cursor_pos = self.get_cursor_from_x(&val, layout_x, scale_f32);
        }
        let scale_f32 = scale as f32;
        self.ensure_cursor_visible(field_idx, scale_f32, ai_config);
        self.window.request_redraw();
        self.last_cursor_action = std::time::Instant::now();
        self.is_dirty = true;
    }

    pub fn handle_mouse_up(&mut self) {
        if self.is_dragging_text {
            if let Some(start) = self.selection_start {
                if start == self.cursor_pos {
                    self.selection_start = None;
                }
            }
            self.is_dragging_text = false;
        }
        self.is_dragging_scrollbar = false;
        self.dragging_history_idx = None;
        self.dragging_sys_prompt = false;
        self.pressed_btn = None;
        self.window.request_redraw();
    }
}

fn draw_main_scrollbar(
    buffer: &mut [u32],
    w: u32,
    h: u32,
    viewport_height: f32,
    content_height: f32,
    scroll_offset: f32,
) {
    if content_height > viewport_height {
        let scale = (w as f32 / 800.0).min(h as f32 / 750.0);
        let off_x = (w as f32 - 800.0 * scale) / 2.0;
        let off_y = (h as f32 - 750.0 * scale) / 2.0;

        let sc = |val: f32| -> f32 { val * scale };
        let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
        let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };

        let sb_w = sc(6.0) as u32;
        let sb_h = sc(600.0);
        let sb_x = s(785) as i32;
        let sb_y = sy_val(130);

        // Track Background
        draw_rounded_rect(
            buffer,
            w,
            sb_x,
            sb_y as i32,
            sb_w,
            sb_h as u32,
            3,
            COLOR_BG_LIGHT,
            w,
            h,
        );

        // Thumb
        let ratio = (viewport_height / content_height).clamp(0.0, 1.0);
        let hh = (sb_h * ratio).max(sc(30.0));
        let max_sc = -(content_height - viewport_height);
        let prog = if max_sc.abs() < 1.0 {
            0.0
        } else {
            (scroll_offset / max_sc).clamp(0.0, 1.0)
        };
        let hy = sb_y as f32 + (sb_h - hh) * prog;
        draw_rounded_rect(
            buffer, w, sb_x, hy as i32, sb_w, hh as u32, 3, 0x00CCCCCC, // Light grey thumb
            w, h,
        );
    }
}
