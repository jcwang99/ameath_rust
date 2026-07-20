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
    pub history_selection_idx: Option<usize>,
    pub history_selection_start: Option<usize>,
    pub history_cursor_pos: usize,
    pub system_prompt_metrics_cache: f32,
    pub current_scale: f32,
    pub current_mode: String,
    pub current_music_path: Option<std::path::PathBuf>,
    pub current_layer: crate::types::WindowLayer,
    pub run_on_startup: bool,
    pub ai_config: crate::types::AiConfig,
    pub mouse_pos: (f32, f32),
    pub pressed_btn: Option<usize>, // 0-4 for profile buttons, 100+ for fields
    pub show_delete_dialog: bool,
    pub notification: Option<(String, std::time::Instant)>,
    pub field_scroll_offsets: [f32; 19],
    pub available_monitors: Vec<(String, String)>,
    pub current_monitor_name: Option<String>,
    pub routines_config: crate::types::RoutinesConfig,
    pub editing_routine: Option<crate::types::RoutineDef>,
    pub routine_memo_scroll_offset: f32,
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
    pub routine_memo_rect: Option<(f64, f64, f64, f64)>,
    pub routine_memo_content_height: f32,
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
    let mut routine_memo_rect = None;
    let mut routine_memo_content_height = 0.0f32;

    buffer.fill(COLOR_BG_APP);

    let scale = (w as f32 / 800.0).min(h as f32 / 750.0);
    let off_x = (w as f32 - 800.0 * scale) / 2.0;
    let off_y = (h as f32 - 750.0 * scale) / 2.0;

    let sc = |val: f32| -> f32 { val * scale };
    let s = |val: u32| -> u32 { (val as f32 * scale + off_x) as u32 };
    let sy_val = |val: u32| -> u32 { (val as f32 * scale + off_y) as u32 };

    // Sidebar
    draw_rect(buffer, w, 0, 0, s(180), h, COLOR_BG_SIDEBAR, w, h);
    let icons = ["🏠", "🎨", "🧠", "📜", "ℹ️", "⏰"];
    for i in 0..6 {
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
        4 => ("About", "Ameath v0.1.0"),
        5 => ("Routines", "Manage your scheduled tasks"),
        _ => ("", ""),
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
                run_on_startup: input.run_on_startup,
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
                selection_idx: input.history_selection_idx,
                selection_start: input.history_selection_start,
                cursor_pos: input.history_cursor_pos,
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
        5 => {
            let mut routines_state = tabs::routines::RoutinesTabState {
                config: &input.routines_config,
                editing_routine: &input.editing_routine,
                focused_field: input.focused_field,
                cursor_pos: input.cursor_pos,
                scroll_offset: input.scroll_offset,
                memo_scroll_offset: input.routine_memo_scroll_offset,
                memo_rect: &mut routine_memo_rect,
                memo_content_height: &mut routine_memo_content_height,
                selection_start: input.selection_start,
            };
            let (v, c, rect) = tabs::routines::draw(buffer, w, h, scale, off_x, off_y, &mut routines_state);
            vh = v;
            ch = c;
            cursor_rect = rect;
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
        routine_memo_rect,
        routine_memo_content_height,
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum SettingsAction {
    None,
    DragWindow,
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
    SelectTtsRefAudio,
    RequestGc,
    ToggleAutoStart,
    SaveWindowConfig,
    OpenWorkingDirectory,
    UpdateRoutinesConfig(crate::types::RoutinesConfig),
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
    pub history_selection_idx: Option<usize>,
    pub history_selection_start: Option<usize>,
    pub history_cursor_pos: usize,
    pub history_hashes: Vec<u64>,
    pub history_metrics_cache: Vec<f32>, // Cached heights
    pub dragging_history_idx: Option<usize>,
    pub dragging_sys_prompt: bool,
    pub system_prompt_hash: u64,
    pub system_prompt_metrics_cache: f32,
    pub config_dirty: bool,
    pub routine_memo_scroll_offset: f32,
    pub routine_memo_rect: Option<(f64, f64, f64, f64)>,
    pub routine_memo_content_height: f32,

    // Layout
    pub is_dragging_scrollbar: bool,
    pub last_size: (u32, u32),
    pub last_render_scale: f32,
    pub available_monitors: Vec<(String, String)>,
    pub current_monitor_name: Option<String>,
    pub is_dragging_pet_scale: bool,

    pub is_dirty: bool,
    pub last_state_hash: u64,
    pub last_config_hash: u64,
    pub mouse_pos: (f32, f32),
    pub pressed_btn: Option<usize>,
    pub show_delete_dialog: bool,
    pub notification: Option<(String, std::time::Instant)>,
    pub field_scroll_offsets: [f32; 19],

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
    
    pub routines_config: crate::types::RoutinesConfig,
    pub editing_routine: Option<crate::types::RoutineDef>,
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

        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
            use windows::Win32::Foundation::HWND;
            use windows::Win32::Graphics::Dwm::{
                DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE,
            };

            if let RawWindowHandle::Win32(handle) = window.raw_window_handle() {
                let hwnd = HWND(handle.hwnd as isize);
                let dark_mode = 1;
                unsafe {
                    let _ = DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_USE_IMMERSIVE_DARK_MODE,
                        &dark_mode as *const _ as *const _,
                        std::mem::size_of::<i32>() as u32,
                    );
                }
            }
        }
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
            history_selection_idx: None,
            history_selection_start: None,
            history_cursor_pos: 0,
            history_hashes: Vec::new(),
            history_metrics_cache: Vec::new(),
            dragging_history_idx: None,
            dragging_sys_prompt: false,
            system_prompt_hash: 0,
            system_prompt_metrics_cache: 0.0,
            config_dirty: false,
            routine_memo_scroll_offset: 0.0,
            routine_memo_rect: None,
            routine_memo_content_height: 0.0,
            is_dragging_scrollbar: false,
            available_monitors,
            current_monitor_name: None,
            is_dragging_pet_scale: false,
            last_size: (0, 0),
            last_render_scale: 0.0,
            is_dirty: true,
            last_state_hash: 0,
            last_config_hash: 0,
            mouse_pos: (0.0, 0.0),
            pressed_btn: None,
            show_delete_dialog: false,
            notification: None,
            field_scroll_offsets: [0.0; 19],
            cursor_cache: None,
            cursor_save_under: Vec::new(),
            last_base_state_hash: 0,
            last_sent_hash: 0,
            render_back_buffer,
            render_in_progress,
            idle_buffers,
            last_background_pixels: std::sync::Arc::new(Vec::new()),
            render_tx,
            _proxy: proxy,
            routines_config: crate::types::RoutinesConfig::load(),
            editing_routine: None,
        }
    }

    pub fn id(&self) -> winit::window::WindowId {
        self.window.id()
    }

    pub fn focus(&self) {
        #[cfg(target_os = "windows")]
        {
            use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
            use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};
            use windows::Win32::Foundation::HWND;

            if let RawWindowHandle::Win32(handle) = self.window.raw_window_handle() {
                let hwnd = HWND(handle.hwnd as isize);
                unsafe {
                    // 如果窗口被最小化了，先恢复它
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    // 强制设为前台窗口
                    let _ = SetForegroundWindow(hwnd);
                }
            }
        }
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
        run_on_startup: bool,
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
            history_selection_idx: self.history_selection_idx,
            history_selection_start: self.history_selection_start,
            history_cursor_pos: self.history_cursor_pos,
            system_prompt_metrics_cache: self.system_prompt_metrics_cache,
            current_scale,
            current_mode: current_mode.to_string(),
            current_music_path: current_music_path.map(|p| p.to_path_buf()),
            current_layer,
            run_on_startup,
            ai_config: ai_config.clone(),
            mouse_pos: self.mouse_pos,
            pressed_btn: self.pressed_btn,
            show_delete_dialog: self.show_delete_dialog,
            notification: self.notification.clone(),
            field_scroll_offsets: self.field_scroll_offsets,
            available_monitors: self.available_monitors.clone(),
            current_monitor_name: self.current_monitor_name.clone(),
            routines_config: self.routines_config.clone(),
            editing_routine: self.editing_routine.clone(),
            routine_memo_scroll_offset: self.routine_memo_scroll_offset,
        }
    }

    pub fn redraw(
        &mut self,
        current_scale: f32,
        current_mode: &str,
        current_music_path: Option<&std::path::Path>,
        current_layer: crate::types::WindowLayer,
        run_on_startup: bool,
        ai_config: &crate::types::AiConfig,
    ) {
        let size = self.window.inner_size();
        let w = size.width;
        let h = size.height;
        if w == 0 || h == 0 {
            return;
        }

        if self.last_size != (w, h) || (current_scale - self.last_render_scale).abs() > 0.01 {
            if let (Some(nz_w), Some(nz_h)) =
                (std::num::NonZeroU32::new(w), std::num::NonZeroU32::new(h))
            {
                if let Err(e) = self.surface.resize(nz_w, nz_h) {
                    tracing::error!("Failed to resize Softbuffer surface: {:?}", e);
                    return;
                } else {
                    self.last_size = (w, h);
                    self.last_render_scale = current_scale;
                    self.history_hashes.clear();
                    self.history_metrics_cache.clear();
                    self.system_prompt_metrics_cache = 0.0;
                    self.is_dirty = true;
                }
            } else {
                tracing::warn!(
                    "Invalid resize dimensions for SettingsWindow: w={}, h={}",
                    w,
                    h
                );
            }
        }

        if self.last_size == (0, 0) {
            return;
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
                profile.use_responses_api.hash(&mut config_hasher);
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
            ai_config.tts_enabled.hash(&mut config_hasher);
            ai_config.tts_reference_audio.hash(&mut config_hasher);
            ai_config.tts_prompt_text.hash(&mut config_hasher);
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
        self.current_monitor_name.hash(&mut base_hasher);
        current_scale.to_bits().hash(&mut base_hasher);
        current_mode.hash(&mut base_hasher);
        current_music_path.hash(&mut base_hasher);
        current_layer.hash(&mut base_hasher);
        run_on_startup.hash(&mut base_hasher);
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
                    let mut buffer = match self.surface.buffer_mut() {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::error!("Failed to get surface buffer from background result: {}. Skipping frame.", e);
                            return;
                        }
                    };
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
                    self.routine_memo_rect = res.routine_memo_rect;
                    self.routine_memo_content_height = res.routine_memo_content_height;

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
                run_on_startup,
                ai_config,
            );

            let mut pixels = {
                let mut idle = self.idle_buffers.lock().unwrap();
                idle.pop().unwrap_or_else(|| {
                    tracing::debug!("Creating new pixel buffer for settings window ({}x{})", w, h);
                    vec![0u32; (w * h) as usize]
                })
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

        let mut buffer = match self.surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                tracing::error!("Failed to get surface buffer for composition: {}. Skipping frame.", e);
                return;
            }
        };

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
                    let mut buffer = match self.surface.buffer_mut() {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::error!("Failed to get surface buffer for cursor restoration: {}. Skipping frame.", e);
                            return;
                        }
                    };
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
                    tracing::info!("Delete Profile confirmed via dialog");
                    self.show_delete_dialog = false;
                    let mut config = ai_config.clone();
                    if config.profiles.len() > 1 {
                        config.profiles.remove(config.active_profile_index);
                        config.active_profile_index =
                            config.active_profile_index.min(config.profiles.len() - 1);
                        config.active_interaction_screenshots_enabled = false;
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

        if !self.show_delete_dialog && ly < 120.0 && lx > 180.0 {
            return SettingsAction::DragWindow;
        }

        // Sidebar
        if lx >= 0.0 && lx <= 180.0 {
            for i in 0..6 {
                let ty = 60.0 + i as f64 * 80.0;
                if ly >= ty - 15.0 && ly <= ty + 45.0 {
                    self.current_tab = i;
                    self.scroll_offset = 0.0;
                    self.focused_field = None;
                    self.history_selection_idx = None;
                    self.history_selection_start = None;
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
            0 => {
                // Tab 0: Home
                // Check "Open Working Directory" button hit
                // Coordinates from home.rs: s(230), sy_val(415), w: 200, h: 45
                if lx >= 230.0 && lx <= 230.0 + 200.0 && ly >= 415.0 && ly <= 415.0 + 45.0 {
                    return SettingsAction::OpenWorkingDirectory;
                }
            }
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
                        if lx >= 220.0 && lx <= 540.0 {
                            self.is_dragging_pet_scale = true;
                            let progress = ((lx - 230.0) / 300.0).clamp(0.0, 1.0);
                            let scale = 0.1 + progress * 2.9;
                            return SettingsAction::SetScale(scale as f32);
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

                // Calculate rows for monitor section since Auto-Start depends on its height
                let rows = (self.available_monitors.len() + 2) / 3;

                // Auto-Start
                let card6_y = 825.0 + scroll_y + 60.0 + (rows as f64 * 65.0) + 20.0;
                let toggle_x = 210.0 + card_w - 80.0;
                let toggle_y = card6_y + 25.0;
                if lx >= toggle_x
                    && lx <= toggle_x + 44.0
                    && ly >= toggle_y
                    && ly <= toggle_y + 24.0
                {
                    return SettingsAction::ToggleAutoStart;
                }

                // Monitor selection
                let card5_y = 825.0 + scroll_y;
                let card5_h = 60.0 + (rows as f64 * 65.0);
                if lx >= 210.0
                    && lx <= 210.0 + card_w
                    && ly >= card5_y + 60.0
                    && ly <= card5_y + card5_h
                {
                    for (i, (name, _)) in self.available_monitors.iter().enumerate() {
                        let row = i / 3;
                        let col = i % 3;
                        let mx = 230.0 + col as f64 * 110.0;
                        let my = card5_y + 60.0 + row as f64 * 65.0;
                        if lx >= mx && lx <= mx + 100.0 && ly >= my && ly <= my + 55.0 {
                            tracing::info!("Monitor {} clicked", name);
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
                    (405.0, 542.5, 350.0),  // 14: Screen Capture (530 + 12.5 offset)
                    (230.0, 1330.0, 45.0),  // 15: TTS Toggle
                    (230.0, 1430.0, 500.0), // 16: TTS Ref Path
                    (230.0, 1530.0, 500.0), // 17: TTS Prompt Text
                    (650.0, 30.0, 45.0),    // 18: Responses API Toggle
                ];

                // Profile Management Buttons (Standardized Row)
                let btn_y_start = design_card_y + 30.0 + 25.0;
                let btn_y_end = btn_y_start + 45.0;

                // [<] Prev Profile (at 230)
                if dlx >= 230.0 && dlx <= 260.0 && dly >= btn_y_start && dly <= btn_y_end {
                    tracing::debug!("Prev Profile clicked");
                    self.pressed_btn = Some(0);
                    let mut config = ai_config.clone();
                    if config.active_profile_index > 0 {
                        config.active_profile_index -= 1;
                    } else if !config.profiles.is_empty() {
                        config.active_profile_index = config.profiles.len() - 1;
                    }
                    config.active_interaction_screenshots_enabled = false;
                    self.config_dirty = true;
                    self.window.request_redraw();
                    return SettingsAction::UpdateAiConfig(config);
                }

                // [>] Next Profile (at 430)
                if dlx >= 430.0 && dlx <= 460.0 && dly >= btn_y_start && dly <= btn_y_end {
                    tracing::debug!("Next Profile clicked");
                    self.pressed_btn = Some(1);
                    let mut config = ai_config.clone();
                    if !config.profiles.is_empty() {
                        config.active_profile_index =
                            (config.active_profile_index + 1) % config.profiles.len();
                    }
                    config.active_interaction_screenshots_enabled = false;
                    self.config_dirty = true;
                    self.window.request_redraw();
                    return SettingsAction::UpdateAiConfig(config);
                }

                // [+] Add Profile (at 480)
                if dlx >= 480.0 && dlx <= 515.0 && dly >= btn_y_start && dly <= btn_y_end {
                    tracing::info!("Add Profile clicked");
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
                    config.active_interaction_screenshots_enabled = false;
                    self.config_dirty = true;
                    self.notification =
                        Some(("Add Success".to_string(), std::time::Instant::now()));
                    self.window.request_redraw();
                    return SettingsAction::UpdateAiConfig(config);
                }

                // [-] Delete Profile (at 525)
                if dlx >= 525.0 && dlx <= 560.0 && dly >= btn_y_start && dly <= btn_y_end {
                    tracing::info!("Delete Profile clicked (showing dialog)");
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
                            tracing::info!("Multimodal Toggle clicked");
                            self.pressed_btn = Some(101); // Special code for multimodal
                                                          // Toggle Multimodal
                            let mut config = ai_config.clone();
                            let profile = config.active_profile_mut();
                            profile.is_multimodal = !profile.is_multimodal;
                            // If Multimodal is disabled, enforce Screen Capture off
                            if !profile.is_multimodal {
                                config.active_interaction_screenshots_enabled = false;
                            }
                            self.config_dirty = true;
                            self.window.request_redraw();
                            return SettingsAction::UpdateAiConfig(config);
                        }
                        if i == 14 {
                            if ai_config.active_profile().is_multimodal {
                                tracing::info!("Screen Capture Toggle clicked");
                                self.pressed_btn = Some(102); // Special code for screen capture
                                let mut config = ai_config.clone();
                                config.active_interaction_screenshots_enabled =
                                    !config.active_interaction_screenshots_enabled;
                                self.config_dirty = true;
                                self.window.request_redraw();
                                return SettingsAction::UpdateAiConfig(config);
                            }
                        }
                        if i == 15 {
                            tracing::info!("TTS Toggle clicked");
                            self.pressed_btn = Some(103);
                            let mut config = ai_config.clone();
                            config.tts_enabled = !config.tts_enabled;
                            self.config_dirty = true;
                            self.window.request_redraw();
                            return SettingsAction::UpdateAiConfig(config);
                        }
                        if i == 18 {
                            tracing::info!("Responses API Toggle clicked");
                            self.pressed_btn = Some(104);
                            let mut config = ai_config.clone();
                            let profile = config.active_profile_mut();
                            profile.use_responses_api = !profile.use_responses_api;
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
                                } else if i == 15 {
                                    let mut config = ai_config.clone();
                                    config.tts_enabled = !config.tts_enabled;
                                    self.config_dirty = true;
                                    return SettingsAction::UpdateAiConfig(config);
                                } else if i == 16 {
                                    return SettingsAction::SelectTtsRefAudio;
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
                
                {
                    for (i, (_, ry_start, _, ry_end)) in self.history_item_rects.iter().enumerate() {
                        if dly >= *ry_start && dly <= *ry_end {
                            // Hit this item. Is it inside the text area?
                            let content_y_base = ry_start + 35.0;
                            let content_x_base = 240.0;
                            
                            // Let's do a simple bounds check
                            if lx >= content_x_base && lx <= content_x_base + 450.0 && dly >= content_y_base {
                                if let Some((_, content_text)) = self.history.get(i) {
                                    self.history_selection_idx = Some(i);
                                    let scale_f32 = scale as f32;
                                    let content_scroll = self.history_scroll_states.get(i).copied().unwrap_or(0.0);
                                    let layout_x = (lx - content_x_base) as f32 * scale;
                                    // Y within the text block
                                    let layout_y = ((ly - content_y_base) as f32 * scale) - (content_scroll * scale_f32);
                                    self.history_cursor_pos = get_cursor_index_from_xy(content_text, 16.0 * scale_f32, (450.0 * scale_f32) as u32, layout_x, layout_y);
                                    if !_is_right_click {
                                        self.history_selection_start = Some(self.history_cursor_pos);
                                        self.is_dragging_text = true;
                                    }
                                    self.window.request_redraw();
                                    return SettingsAction::None;
                                }
                            }
                        }
                    }
                    self.history_selection_idx = None;
                    self.history_selection_start = None;
                    self.window.request_redraw();
                }
            }
            5 => {
                let mut current_y = 120.0;
                if self.editing_routine.is_some() {
                    // Title
                    current_y += 60.0;
                    let input_title_y = current_y + 35.0;
                    if dlx >= 230.0 && dlx <= 730.0 && dly >= input_title_y && dly <= input_title_y + 40.0 {
                        self.focused_field = Some(501);
                        let text = self.get_field_text(501, ai_config);
                        self.cursor_pos = self.get_cursor_from_x(&text, ((lx - 240.0) * scale as f64) as f32, scale as f32);
                        self.selection_start = Some(self.cursor_pos);
                        self.is_dragging_text = true;
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                    current_y += 90.0;
                    
                    // Type Selection
                    current_y += 35.0;
                    let types = [crate::types::ScheduleType::Daily, crate::types::ScheduleType::Weekly, crate::types::ScheduleType::Monthly, crate::types::ScheduleType::IntervalDays, crate::types::ScheduleType::IntervalHours, crate::types::ScheduleType::IntervalMinutes];
                    for (i, t) in types.iter().enumerate() {
                        let row = i / 3;
                        let col = i % 3;
                        let btn_x = 230.0 + col as f64 * 110.0;
                        let btn_y = current_y + row as f64 * 50.0;
                        if dlx >= btn_x && dlx <= btn_x + 100.0 && dly >= btn_y && dly <= btn_y + 40.0 {
                            if let Some(mut r) = self.editing_routine.clone() {
                                r.schedule_type = t.clone();
                                self.editing_routine = Some(r);
                                self.window.request_redraw();
                            }
                            return SettingsAction::None;
                        }
                    }
                    current_y += 110.0;
                    
                    // Dynamic value
                    if !matches!(self.editing_routine.as_ref().unwrap().schedule_type, crate::types::ScheduleType::Daily) {
                        let input_val_y = current_y + 35.0;
                        if dlx >= 230.0 && dlx <= 430.0 && dly >= input_val_y && dly <= input_val_y + 40.0 {
                            self.focused_field = Some(502);
                            let text = self.get_field_text(502, ai_config);
                            self.cursor_pos = self.get_cursor_from_x(&text, ((lx - 240.0) * scale as f64) as f32, scale as f32);
                            self.selection_start = Some(self.cursor_pos);
                            self.is_dragging_text = true;
                            self.window.request_redraw();
                            return SettingsAction::None;
                        }
                    }
                    if matches!(self.editing_routine.as_ref().unwrap().schedule_type, crate::types::ScheduleType::Daily | crate::types::ScheduleType::Weekly | crate::types::ScheduleType::Monthly) {
                        let input_time_y = current_y + 35.0;
                        if dlx >= 450.0 && dlx <= 650.0 && dly >= input_time_y && dly <= input_time_y + 40.0 {
                            self.focused_field = Some(503);
                            let text = self.get_field_text(503, ai_config);
                            self.cursor_pos = self.get_cursor_from_x(&text, ((lx - 460.0) * scale as f64) as f32, scale as f32);
                            self.selection_start = Some(self.cursor_pos);
                            self.is_dragging_text = true;
                            self.window.request_redraw();
                            return SettingsAction::None;
                        }
                    }
                    current_y += 90.0;
                    
                    // Memo
                    let input_memo_y = current_y + 35.0;
                    if dlx >= 230.0 && dlx <= 730.0 && dly >= input_memo_y && dly <= input_memo_y + 100.0 {
                        self.focused_field = Some(504);
                        let text = self.get_field_text(504, ai_config);
                        let scale_f32 = scale as f32;
                        let layout_x = ((lx - 240.0) * scale as f64) as f32;
                        // Use scroll_offset correctly for click hit testing inside text
                        let scroll_y = self.routine_memo_scroll_offset * scale_f32;
                        let layout_y = ((dly - input_memo_y - 10.0) * scale as f64) as f32 - scroll_y;
                        self.cursor_pos = self.get_cursor_from_xy(&text, layout_x, layout_y, scale_f32);
                        self.selection_start = Some(self.cursor_pos);
                        self.is_dragging_text = true;
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                    current_y += 150.0;
                    
                    // Expiry Mode buttons
                    let expiry_btn_y = current_y + 35.0;
                    // Always Run button
                    if dlx >= 230.0 && dlx <= 370.0 && dly >= expiry_btn_y && dly <= expiry_btn_y + 40.0 {
                        if let Some(mut r) = self.editing_routine.clone() {
                            r.expiry_minutes = None;
                            self.editing_routine = Some(r);
                            self.focused_field = None;
                            self.window.request_redraw();
                        }
                        return SettingsAction::None;
                    }
                    // Expire After button
                    if dlx >= 380.0 && dlx <= 520.0 && dly >= expiry_btn_y && dly <= expiry_btn_y + 40.0 {
                        if let Some(mut r) = self.editing_routine.clone() {
                            if r.expiry_minutes.is_none() {
                                r.expiry_minutes = Some(60); // Default 60 min
                            }
                            self.editing_routine = Some(r);
                            self.window.request_redraw();
                        }
                        return SettingsAction::None;
                    }
                    // Minutes input (Field 505)
                    if self.editing_routine.as_ref().map_or(false, |r| r.expiry_minutes.is_some()) {
                        if dlx >= 540.0 && dlx <= 660.0 && dly >= expiry_btn_y && dly <= expiry_btn_y + 40.0 {
                            self.focused_field = Some(505);
                            let text = self.get_field_text(505, ai_config);
                            self.cursor_pos = self.get_cursor_from_x(&text, ((lx - 550.0) * scale as f64) as f32, scale as f32);
                            self.selection_start = Some(self.cursor_pos);
                            self.is_dragging_text = true;
                            self.window.request_redraw();
                            return SettingsAction::None;
                        }
                    }
                    current_y += 90.0;
                    
                    // Cancel
                    if dlx >= 480.0 && dlx <= 580.0 && dly >= current_y && dly <= current_y + 40.0 {
                        self.editing_routine = None;
                        self.focused_field = None;
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                    // Save
                    if dlx >= 600.0 && dlx <= 730.0 && dly >= current_y && dly <= current_y + 40.0 {
                        if let Some(r) = self.editing_routine.take() {
                            let mut cfg = self.routines_config.clone();
                            if let Some(idx) = cfg.routines.iter().position(|x| x.id == r.id) {
                                cfg.routines[idx] = r;
                            } else {
                                cfg.routines.push(r);
                            }
                            self.routines_config = cfg.clone();
                            self.focused_field = None;
                            self.window.request_redraw();
                            return SettingsAction::UpdateRoutinesConfig(cfg);
                        }
                    }
                    
                    self.focused_field = None;
                    self.window.request_redraw();
                    return SettingsAction::None;
                } else {
                    // List View
                    if dlx >= 210.0 && dlx <= 770.0 && dly >= current_y && dly <= current_y + 50.0 {
                        let new_routine = crate::types::RoutineDef {
                            id: uuid::Uuid::new_v4().to_string(),
                            title: "New Routine".to_string(),
                            schedule_type: crate::types::ScheduleType::Daily,
                            day_of_week: Some(0),
                            day_of_month: Some(1),
                            interval: Some(1),
                            time_of_day: Some("12:00".to_string()),
                            memo: "".to_string(),
                            is_active: true,
                            expiry_minutes: None,
                        };
                        self.editing_routine = Some(new_routine);
                        self.window.request_redraw();
                        return SettingsAction::None;
                    }
                    current_y += 70.0;
                    
                    let mut action = SettingsAction::None;
                    for (i, r) in self.routines_config.routines.iter().enumerate() {
                        let card_y = current_y;
                        if dlx >= 550.0 && dlx <= 620.0 && dly >= card_y + 30.0 && dly <= card_y + 60.0 {
                            let mut cfg = self.routines_config.clone();
                            cfg.routines[i].is_active = !cfg.routines[i].is_active;
                            self.routines_config = cfg.clone();
                            self.window.request_redraw();
                            action = SettingsAction::UpdateRoutinesConfig(cfg);
                            break;
                        }
                        if dlx >= 630.0 && dlx <= 680.0 && dly >= card_y + 30.0 && dly <= card_y + 60.0 {
                            self.editing_routine = Some(r.clone());
                            self.window.request_redraw();
                            break;
                        }
                        if dlx >= 690.0 && dlx <= 760.0 && dly >= card_y + 30.0 && dly <= card_y + 60.0 {
                            let mut cfg = self.routines_config.clone();
                            cfg.routines.remove(i);
                            self.routines_config = cfg.clone();
                            self.window.request_redraw();
                            action = SettingsAction::UpdateRoutinesConfig(cfg);
                            break;
                        }
                        current_y += 120.0;
                    }
                    if action != SettingsAction::None {
                        return action;
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
            16 => ai_config.tts_reference_audio.to_string_lossy().into_owned(),
            17 => ai_config.tts_prompt_text.clone(),
            14 | 15 | 18 => String::new(), // Toggles handled via click
            501 => {
                if let Some(r) = &self.editing_routine { r.title.clone() } else { "".to_string() }
            }
            502 => {
                if let Some(r) = &self.editing_routine {
                    match r.schedule_type {
                        crate::types::ScheduleType::Weekly => r.day_of_week.unwrap_or(0).to_string(),
                        crate::types::ScheduleType::Monthly => r.day_of_month.unwrap_or(1).to_string(),
                        _ => r.interval.unwrap_or(1).to_string(),
                    }
                } else { "".to_string() }
            }
            503 => {
                if let Some(r) = &self.editing_routine { r.time_of_day.clone().unwrap_or("00:00".to_string()) } else { "".to_string() }
            }
            504 => {
                if let Some(r) = &self.editing_routine { r.memo.clone() } else { "".to_string() }
            }
            505 => {
                if let Some(r) = &self.editing_routine { r.expiry_minutes.unwrap_or(60).to_string() } else { "".to_string() }
            }
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
                    4 => {
                        profile.model = text;
                        ai_config.active_interaction_screenshots_enabled = false;
                    }
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
            16 => {
                ai_config.tts_reference_audio = std::path::PathBuf::from(text);
            }
            17 => {
                ai_config.tts_prompt_text = text;
            }
            501..=505 => {
                if let Some(mut r) = self.editing_routine.clone() {
                    match idx {
                        501 => r.title = text,
                        502 => {
                            let v: u32 = text.parse().unwrap_or(1);
                            match r.schedule_type {
                                crate::types::ScheduleType::Weekly => r.day_of_week = Some(v),
                                crate::types::ScheduleType::Monthly => r.day_of_month = Some(v),
                                _ => r.interval = Some(v),
                            }
                        }
                        503 => r.time_of_day = Some(text),
                        504 => r.memo = text,
                        505 => {
                            let v: u32 = text.parse().unwrap_or(60).max(1);
                            r.expiry_minutes = Some(v);
                        }
                        _ => {}
                    }
                    self.editing_routine = Some(r);
                }
            }
            _ => {}
        }
    }

    fn ensure_cursor_visible(&mut self, field_idx: usize, scale: f32, ai_config: &AiConfig) {
        if (field_idx >= 19 && field_idx < 500) || field_idx > 505 {
            return;
        }

        if field_idx == 14 || field_idx == 15 || field_idx == 16 || field_idx == 18 {
            return;
        }

        if field_idx == 13 || field_idx == 504 {
            let text = self.get_field_text(field_idx, ai_config);
            let mut measurement_text = text.clone();
            if measurement_text.ends_with('\n') {
                measurement_text.push(' ');
            }
            let (_, py, ch) = self.get_xy_from_cursor(&measurement_text, self.cursor_pos, scale);
            let py_logical = py as f32; // Relative to text start (scaled)
            let ch_scaled = ch as f32; // Line height (scaled)

            // Text viewport is smaller than box
            let viewport_h_scaled = if field_idx == 13 { (250.0 - 24.0) * scale } else { 80.0 * scale };
            let mut current_scroll = if field_idx == 13 { self.system_prompt_scroll_offset * scale } else { self.routine_memo_scroll_offset * scale };

            let top_y = py_logical + current_scroll;
            let bottom_y = top_y + ch_scaled;

            // Pad by 10/30px to keep cursor from hitting edges
            let pad = 10.0 * scale;
            if top_y < pad {
                current_scroll = (pad - py_logical).min(0.0);
            } else if bottom_y > viewport_h_scaled - pad {
                current_scroll = (viewport_h_scaled - pad - (py_logical + ch_scaled)).min(0.0);
            }

            // Content-aware clamping
            let max_w_design = if field_idx == 13 { 460.0 } else { 480.0 };
            let (_, mh): (f32, f32) =
                get_metrics_dw(&measurement_text, 14.0 * scale, (max_w_design * scale) as u32);
            let content_h = mh + if field_idx == 13 { 24.0 * scale } else { 20.0 * scale };
            let min_scroll = (viewport_h_scaled - content_h).min(0.0f32);
            current_scroll = current_scroll.max(min_scroll).min(0.0);

            if field_idx == 13 {
                self.system_prompt_scroll_offset = current_scroll / scale;
            } else {
                self.routine_memo_scroll_offset = current_scroll / scale;
            }
            return;
        }

        // Single-line fields
        if field_idx >= 501 && field_idx <= 503 || field_idx == 505 {
            return; // Scroll offsets not used for routine small fields yet
        }

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
            (405.0, 530.0, 20.0),   // 14: Screen Capture (Toggles don't scroll but index exists)
            (230.0, 1330.0, 20.0),  // 15: TTS Toggle
            (230.0, 1430.0, 500.0), // 16: Ref Path
            (230.0, 1530.0, 500.0), // 17: TTS Prompt Text
            (650.0, 30.0, 45.0),    // 18: Responses API Toggle
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
        } else if field_idx == 504 {
            480.0 * scale
        } else {
            1000000.0
        };
        get_cursor_index_from_xy(text, 14.0 * scale, max_width as u32, layout_x, layout_y)
    }

    fn get_xy_from_cursor(&self, text: &str, cursor_pos: usize, scale: f32) -> (f64, f64, f64) {
        let field_idx = self.focused_field.unwrap_or(0);
        let max_width = if field_idx == 13 {
            460.0 * scale
        } else if field_idx == 504 {
            480.0 * scale
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
        if self.current_tab != 2 && self.current_tab != 3 && self.current_tab != 5 {
            return false;
        }

        if self.current_tab == 3 {
            // History tab specific handling (Ctrl+C for copy)
            use winit::keyboard::Key;
            let is_pressed = event.state == winit::event::ElementState::Pressed;
            if !is_pressed {
                return false;
            }
            let has_ctrl = modifiers.control_key() || modifiers.super_key();
            if let Key::Character(c) = &event.logical_key {
                if has_ctrl && c == "c" {
                    if let Some(idx) = self.history_selection_idx {
                        if let Some(item) = self.history.get(idx) {
                            if let Some(start) = self.history_selection_start {
                                let min = start.min(self.history_cursor_pos);
                                let max = start.max(self.history_cursor_pos);
                                if min != max {
                                    let chars: Vec<char> = item.1.chars().collect();
                                    if min < chars.len() && max <= chars.len() {
                                        let selected: String = chars[min..max].iter().collect();
                                        use arboard::Clipboard;
                                        if let Ok(mut cb) = Clipboard::new() {
                                            let _ = cb.set_text(selected);
                                            tracing::info!("Copied text from history selection.");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    return true;
                }
            }
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
            if field_idx == 13 || field_idx == 504 {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                let (lx, ly, _) = self.get_xy_from_cursor(&text, self.cursor_pos, scale);
                let line_height = 20.0 * scale; // pixels
                self.cursor_pos = self.get_cursor_from_xy(
                    &text,
                    lx as f32,
                    ly as f32 - line_height + 5.0 * scale,
                    scale,
                );
                self.ensure_cursor_visible(field_idx, scale, ai_config);
                self.window.request_redraw();
                return true;
            }
        }
        if let Key::Named(NamedKey::ArrowDown) = &event.logical_key {
            if field_idx == 13 || field_idx == 504 {
                if has_shift {
                    if self.selection_start.is_none() {
                        self.selection_start = Some(self.cursor_pos);
                    }
                } else {
                    self.selection_start = None;
                }
                let (lx, ly, _) = self.get_xy_from_cursor(&text, self.cursor_pos, scale);
                let line_height = 20.0 * scale; // pixels
                self.cursor_pos = self.get_cursor_from_xy(
                    &text,
                    lx as f32,
                    ly as f32 + line_height + 5.0 * scale,
                    scale,
                );
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
                if field_idx == 13 || field_idx == 504 {
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
                self.focused_field = Some((field_idx + 1) % 18);
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
        if self.current_tab != 2 && self.current_tab != 5 {
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
                        self.window.request_redraw();
                    } else {
                        // At boundary
                        scrolled_sys_prompt = false;
                    }
                }
            }

            if !scrolled_sys_prompt {
                self.scroll_offset += dy_logical;
                let min_offset = -(self.content_height - self.viewport_height).max(0.0);
                self.scroll_offset = self.scroll_offset.clamp(min_offset, 0.0);
            }
        } else if self.current_tab == 5 {
            // Routines Tab
            let mut scrolled_memo = false;
            if let Some((min_x, min_y, max_x, max_y)) = self.routine_memo_rect {
                // Here, dlx/dly are used but min_x/min_y are returned from routines.rs with absolute coords (sy_val).
                // Wait, sy_val includes scroll_offset in routines.rs. Let's use physical x/y for check
                if cursor_pos.is_some() {
                    let pos = cursor_pos.unwrap();
                    if pos.x >= min_x && pos.x <= max_x && pos.y >= min_y && pos.y <= max_y {
                        let old_off = self.routine_memo_scroll_offset;
                        self.routine_memo_scroll_offset += dy_logical;
                        let view_h = 80.0;
                        let content_h = (self.routine_memo_content_height / scale as f32) + 20.0;
                        let min_offset = -(content_h - view_h).max(0.0);
                        self.routine_memo_scroll_offset = self.routine_memo_scroll_offset.clamp(min_offset, 0.0);
                        
                        if (self.routine_memo_scroll_offset - old_off).abs() > 0.01 {
                            scrolled_memo = true;
                            self.window.request_redraw();
                        }
                    }
                }
            }
            if !scrolled_memo {
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

    pub fn handle_mouse_move(
        &mut self,
        x: f64,
        y: f64,
        ai_config: &crate::types::AiConfig,
    ) -> Option<SettingsAction> {
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
                self.window.request_redraw();
            }
            return None;
        }

        if self.is_dragging_pet_scale {
            let progress = ((lx - 230.0) / 300.0).clamp(0.0, 1.0);
            let scale_val = 0.1 + progress * 2.9;
            self.window.request_redraw();
            return Some(SettingsAction::SetScale(scale_val as f32));
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
            return None;
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
                return None;
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
                return None;
            }
        }

        if !self.is_dragging_text {
            // Trigger redraw for hover effects in AI tab content area
            if self.current_tab == 2 && lx > 180.0 {
                self.window.request_redraw();
            }
            return None;
        }

        self.last_cursor_action = std::time::Instant::now();
        let field_idx = match self.focused_field {
            Some(i) => i,
            None => {
                // If dragging in history tab
                if self.current_tab == 3 {
                    if let Some(idx) = self.history_selection_idx {
                        if let Some((_, text)) = self.history.get(idx) {
                            let content_x_base = 240.0;
                            // Find logical Y from rect cache
                            let ry_start = if idx < self.history_item_rects.len() {
                                self.history_item_rects[idx].1
                            } else {
                                140.0 + idx as f64 * 190.0
                            };
                            let content_y_base = ry_start + 35.0;
                            
                            let scale_f32 = scale as f32;
                            let scroll_y = self.history_scroll_states.get(idx).copied().unwrap_or(0.0);
                            let layout_x = (lx as f32 - content_x_base as f32) * scale_f32;
                            let layout_y = (ly as f32 - content_y_base as f32) * scale_f32 - (scroll_y * scale_f32);
                            self.history_cursor_pos = get_cursor_index_from_xy(text, 16.0 * scale_f32, (450.0 * scale_f32) as u32, layout_x, layout_y);
                            self.window.request_redraw();
                        }
                        return None;
                    }
                }
                self.is_dragging_text = false;
                return None;
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
            (405.0, 530.0),  // 14: Screen Capture
            (230.0, 1330.0), // 15: TTS Toggle
            (230.0, 1430.0), // 16: Ref Path
            (230.0, 1530.0), // 17: TTS Prompt Text
        ];

        if field_idx >= 501 {
            let scale_f32 = scale as f32;
            let fx = if field_idx == 503 { 450.0 } else { 230.0 };
            let text_x = dlx as f64 - fx - 10.0;
            if text_x.is_finite() {
                let layout_x = (text_x as f32) * scale_f32;
                if field_idx == 504 {
                    // Base Y of the memo field:
                    // 120 + 60 + 90 + 35 + 110 + 90 = 505
                    // input_memo_y = 505 + 35 = 540
                    let input_memo_y = 540.0;
                    let scroll_y = self.routine_memo_scroll_offset * scale_f32;
                    let layout_y = (dly as f32 - input_memo_y - 10.0) * scale_f32 - scroll_y;
                    self.cursor_pos = self.get_cursor_from_xy(&val, layout_x, layout_y, scale_f32);
                } else {
                    self.cursor_pos = self.get_cursor_from_x(&val, layout_x, scale_f32);
                }
                self.window.request_redraw();
            }
            return None;
        }

        let (fx, fy) = fields[field_idx];
        let design_card_y = 120.0;
        let input_y = design_card_y + fy + 25.0;
        let text_x = dlx as f64 - fx - 15.0;

        if !text_x.is_finite() || !dly.is_finite() {
            return None;
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
        None
    }

    pub fn handle_mouse_up(&mut self) -> Option<SettingsAction> {
        if self.is_dragging_text {
            if self.current_tab == 2 {
                if let Some(start) = self.selection_start {
                    if start == self.cursor_pos {
                        self.selection_start = None;
                    }
                }
            } else if self.current_tab == 3 {
                if let Some(start) = self.history_selection_start {
                    if start == self.history_cursor_pos {
                        self.history_selection_start = None;
                    }
                }
            }
            self.is_dragging_text = false;
        }
        self.is_dragging_scrollbar = false;
        self.dragging_history_idx = None;
        self.dragging_sys_prompt = false;
        if self.is_dragging_pet_scale {
            self.is_dragging_pet_scale = false;
            self.pressed_btn = None;
            self.window.request_redraw();
            return Some(SettingsAction::SaveWindowConfig);
        }
        self.pressed_btn = None;
        self.window.request_redraw();
        None
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
