#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anim;
mod bubble;
mod chat_window;
mod menu;
mod music_player;
mod pet;
mod pomodoro;
mod render;
mod settings;
mod types;
mod ai;
mod interaction;
mod theme;
mod ui_primitives;
mod screen_capture;


use chat_window::{ChatWindow, ChatAction};
use settings::SettingsWindow;
use rayon::prelude::*;

use pet::Pet;
use std::collections::HashMap;
use std::rc::Rc;
use rand::Rng;
use std::time::{Duration, Instant};
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuItem},
    TrayIconBuilder,
};
use types::{BehaviorMode, PetState, PreprocessedFrame, PersistentConfig};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::EventLoop,
    window::{WindowBuilder, WindowLevel},
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowBuilderExtWindows;

#[cfg(target_os = "windows")]
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HWND, POINT};

use image::GenericImageView;

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let event_loop = EventLoop::new().unwrap();
    let settings_proxy = event_loop.create_proxy();

    // Global Hotkey Setup
    let hotkey_manager = GlobalHotKeyManager::new().unwrap();
    let hotkey = HotKey::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyM);
    if let Err(e) = hotkey_manager.register(hotkey) {
        eprintln!("Warning: Failed to register global hotkey (Alt+Shift+M): {:?}. Another process might be using it.", e);
    }
    let hotkey_channel = GlobalHotKeyEvent::receiver();

    // Load assets (Right-facing by default)
    let idle_frames_right = vec![
        anim::load_gif_from_memory(include_bytes!("../assets/gifs/idle1.gif")),
        anim::load_gif_from_memory(include_bytes!("../assets/gifs/idle2.gif")),
        anim::load_gif_from_memory(include_bytes!("../assets/gifs/idle3.gif")),
        anim::load_gif_from_memory(include_bytes!("../assets/gifs/idle4.gif")),
    ];
    let move_frames_right = vec![anim::load_gif_from_memory(include_bytes!("../assets/gifs/move.gif"))];
    let drag_frames_right = vec![anim::load_gif_from_memory(include_bytes!("../assets/gifs/drag.gif"))];

    // Load Loading GIF and pre-decompress all frames
    let loading_frames = anim::load_gif_from_memory(include_bytes!("../assets/icons/loading.gif"));
    let loading_frames_decompressed: Vec<(i32, i32, Vec<u8>)> = loading_frames
        .iter()
        .map(|f| {
            let data = lz4_flex::decompress_size_prepended(&f.lz4_data).unwrap_or_default();
            (f.width, f.height, data)
        })
        .collect();

    // Generate Left-facing assets (DELETED mirroring - will flip on-the-fly)

    // Store variants
    let mut animation_map: HashMap<
        PetState,
        Vec<Vec<PreprocessedFrame>>,
    > = HashMap::new();
    animation_map.insert(
        PetState::Clingy,
        move_frames_right.clone(),
    );
    animation_map.insert(PetState::Idle, idle_frames_right);
    animation_map.insert(PetState::Move, move_frames_right);
    animation_map.insert(PetState::Drag, drag_frames_right);

    // Calculate dynamic "envelope" size based on max GIF dimensions
    let mut max_pw = 0;
    let mut max_ph = 0;
    for (_, variants) in &animation_map {
        for variant in variants {
            for frame in variant {
                max_pw = max_pw.max(frame.width);
                max_ph = max_ph.max(frame.height);
            }
        }
    }

    let win_w = (max_pw as u32 + 40).max(bubble::BASE_BUBBLE_WIDTH as u32);
    let win_h = max_ph as u32 + bubble::BASE_BUBBLE_HEIGHT as u32 + 60; // More vertical space

    // Extract Icon before animation_map is moved into Pet
    // Load Official Icon
    let icon_path = "assets/gifs/ameath.ico";
    let (winit_icon, tray_icon_handle) = if let Ok(img) = image::open(icon_path) {
        let rgba = img.to_rgba8();
        let (width, height) = img.dimensions();
        let w_icon = winit::window::Icon::from_rgba(rgba.clone().into_raw(), width, height).ok();
        let t_icon = tray_icon::Icon::from_rgba(rgba.into_raw(), width, height).ok();
        (w_icon, t_icon)
    } else {
        (None, None)
    };

    let mut pet = Pet::new(animation_map, (max_pw as f64, max_ph as f64));
    pet.state = PetState::Move;

    let mut ai_config = types::AiConfig::load();
    let mut window_config = types::WindowConfig::load(); // Load window config
    let mut modifier_state = winit::keyboard::ModifiersState::default();
    let mut chat_window = ChatWindow::new(&event_loop, winit_icon.clone());
    let mut is_thinking = false;
    let mut thinking_start: Option<Instant> = None;
    let mut monitor_offset = (0, 0); // Global offset of the current monitor

    let window = Rc::new(
        WindowBuilder::new()
            .with_title("Ameath")
            .with_inner_size(winit::dpi::PhysicalSize::new(win_w, win_h))
            .with_decorations(false)
            .with_transparent(true)
            .with_visible(false) // Start invisible to avoid jumping
            .with_skip_taskbar(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .build(&event_loop)
            .unwrap(),
    );

    // Initial positioning based on config
    if let Some(ref monitor_name) = window_config.monitor_name {
        let available = window.available_monitors();
        if let Some(monitor) = available.into_iter().find(|m| m.name().as_ref() == Some(monitor_name)) {
             let pos = monitor.position();
             let size = monitor.size();
             let center_x = pos.x + (size.width as i32 / 2) - (win_w as i32 / 2);
             let center_y = pos.y + (size.height as i32 / 2) - (win_h as i32 / 2);
             monitor_offset = (pos.x, pos.y);
             window.set_outer_position(winit::dpi::PhysicalPosition::new(center_x, center_y));
        }
    }
    window.set_visible(true);

    // Initial frame duration (will be updated dynamically based on monitor)
    let mut target_frame_duration = Duration::from_nanos(1_000_000_000 / 60);
    if let Some(monitor) = window.current_monitor() {
        if let Some(refresh) = monitor.refresh_rate_millihertz() {
            // millihertz to duration: 1_000_000_000_000 / mHz = nanos
            target_frame_duration = Duration::from_nanos(1_000_000_000_000 / refresh as u64);
            println!("Detected monitor refresh rate: {} Hz", refresh as f32 / 1000.0);
        }
    }

    if let Some(monitor) = window.current_monitor() {
        let size = monitor.size();
        pet.screen_size = (size.width as f64, size.height as f64);
    }

    #[cfg(target_os = "windows")]
    let mut render_ctx = {
        use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
        if let RawWindowHandle::Win32(handle) = window.raw_window_handle() {
            let hwnd = HWND(handle.hwnd as isize);
            apply_window_styles(hwnd, true); // Initial application
            Some(render::RenderContext::new(hwnd))
        } else {
            None
        }
    };

    let mut bubble_manager = bubble::SpeechBubble::new();
    let mut pomodoro_manager = pomodoro::Pomodoro::new();
    let mut menu_manager = menu::QuickMenu::new();
    let mut music_player = music_player::MusicPlayer::new();

    // AI Kernel & Channel
    let scheduler = interaction::ActionScheduler::new();
    let (ai_tx, ai_rx) = std::sync::mpsc::channel::<String>();
    let mut chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config, scheduler.clone()));
    let music_dir = window_config.music_path.clone().unwrap_or_else(|| std::path::PathBuf::from("assets/music"));
    if music_dir.exists() {
        music_player.set_path(music_dir);
    }
    let mut interaction_manager = interaction::InteractionManager::new(ai_config.clone(), scheduler.clone());
    let (path_tx, path_rx) = std::sync::mpsc::channel::<Option<std::path::PathBuf>>();

    let quotes = vec![
        "哎呀，被发现了！😆",
        "别戳我啦~",
        "今天天气真不错呢☀️",
        "要做点什么好呢？",
        "呼呼……💤",
        "你好呀，今天也是美好的一天！✨",
        "在这里可以看到全世界哦~",
        "你会一直陪着我对吧？❤️",
        "肚子有点饿了……🍬",
        "在努力工作吗？加油！💪",
    ];


    // Tray Icon Setup
    let tray_menu = Menu::new();
    let settings_item = MenuItem::new("Settings", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let settings_id = settings_item.id();
    let quit_id = quit_item.id();
    let _ = tray_menu.append_items(&[&settings_item, &quit_item]);

    // Use the first frame of idle as icon if available (MOVED UP)

    let mut _tray_icon = tray_icon_handle.map(|i| {
        TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("Ameath")
            .with_icon(i)
            .build()
            .unwrap()
    });

    let mut pet_off_x = 20.0;
    let mut pet_off_y = 50.0;
    let mut last_cursor_pos: Option<PhysicalPosition<f64>> = None;

    // Click detection
    let mut last_frame_idx = 0;
    let mut last_loading_frame_idx = 0;
    let mut last_render_pet_off = (0.0, 0.0);
    let mut last_state = PetState::Idle;
    let mut last_facing_right = true;
    let mut last_window_pos = POINT::default();
    let mut last_update: Option<Instant> = None;
    let mut needs_pet_redraw = false;
    let mut click_start_time: Option<Instant> = None;
    let mut click_start_pos: Option<(f64, f64)> = None;
    let mut settings_cursor_pos: Option<PhysicalPosition<f64>> = None;

    let mut settings_win: Option<SettingsWindow> = None;
    let mut menu_visible_timer: Option<Instant> = None;
    let mut current_layer = types::WindowLayer::Top;
    let mut bubble_rect: Option<(i32, i32, i32, i32)>;
    bubble_rect = None;

    let mut composite_data: Vec<u8> = Vec::new();
    let mut decompressed_frame_buffer: Vec<u8> = Vec::new();
    let mut pomodoro_data: Vec<u8> = Vec::new();
    
    // Frame cache: key = (PetState, variant, frame_idx), value = decompressed pixels
    // Reduced from 32 to 16 frames to save memory (max ~4MB instead of 8MB)
    let mut frame_cache: lru::LruCache<(PetState, usize, usize), Vec<u8>> = lru::LruCache::new(std::num::NonZeroUsize::new(16).unwrap());
    let mut is_hovered = false;

    event_loop
        .run(move |event, elwt| {
            // Control flow is managed in AboutToWait logic

            // Global Hotkey Trigger
            if let Ok(_hotkey_event) = hotkey_channel.try_recv() {
                // Always show/focus on hotkey (idempotent)
                if let Some(monitor) = window.current_monitor() {
                    let scale_factor = monitor.scale_factor();
                    let m_size = monitor.size();
                    let m_pos = monitor.position();
                    let chat_w = 600.0;
                    let chat_h = 60.0;
                    let m_w_logical = m_size.width as f64 / scale_factor;
                    let m_h_logical = m_size.height as f64 / scale_factor;
                    let m_x_logical = m_pos.x as f64 / scale_factor;
                    let m_y_logical = m_pos.y as f64 / scale_factor;
                    let center_x = m_x_logical + (m_w_logical / 2.0) - (chat_w / 2.0);
                    let center_y = m_y_logical + (m_h_logical / 2.0) - (chat_h / 2.0);
                    chat_window.show(winit::dpi::LogicalPosition::new(center_x, center_y));
                }
            }

            match event {
                Event::UserEvent(()) => {
                    if let Some(sw) = &settings_win {
                        sw.request_redraw();
                    }
                }
                Event::WindowEvent { window_id, event } => {
                    if let WindowEvent::ModifiersChanged(modifiers) = &event {
                        modifier_state = modifiers.state();
                    }

                    if chat_window.id() == window_id {
                         match chat_window.handle_event(&event, modifier_state) {
                             ChatAction::Send(msg) => {
                                 println!("User sent: {:?}", msg);
                                 is_thinking = true;
                                 thinking_start = Some(Instant::now());
                                 
                                 // Update kernel with current config if changed
                                 let kernel = chat_kernel.clone();
                                 let tx = ai_tx.clone();
                                 let input = msg;
                                 
                                 tokio::spawn(async move {
                                     let response = kernel.handle(input).await;
                                     let _ = tx.send(response);
                                 });

                                 window.request_redraw();
                             }
                             ChatAction::Close => {
                                 window.request_redraw();
                             }
                             ChatAction::None => {}
                         }
                    } else if window_id == window.id() {
                        match event {
                            WindowEvent::CloseRequested => elwt.exit(),
                            WindowEvent::RedrawRequested => {
                                needs_pet_redraw = true;
                            }
                            WindowEvent::MouseInput { state, button, .. } => {
                                if button == MouseButton::Left {
                                    match state {
                                        ElementState::Pressed => {
                                            if let Some(pos) = last_cursor_pos {
                                                if let Ok(win_pos) = window.outer_position() {
                                                    let global_pos = (
                                                        win_pos.x as f64 + pos.x - pet_off_x,
                                                        win_pos.y as f64 + pos.y - pet_off_y,
                                                    );
                                                    click_start_time = Some(Instant::now());
                                                    click_start_pos = Some(global_pos);
                                                }
                                            }
                                        }
                                        ElementState::Released => {
                                            let mut is_click = false;
                                            if let (Some(start_time), Some(start_pos)) = (click_start_time, click_start_pos) {
                                                if start_time.elapsed() < Duration::from_millis(200) {
                                                    if let Some(pos) = last_cursor_pos {
                                                        if let Ok(win_pos) = window.outer_position() {
                                                            let global_x = win_pos.x as f64 + pos.x - pet_off_x;
                                                            let global_y = win_pos.y as f64 + pos.y - pet_off_y;
                                                            let dx = global_x - start_pos.0;
                                                            let dy = global_y - start_pos.1;
                                                            if (dx * dx + dy * dy).sqrt() < 5.0 {
                                                                is_click = true;
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            if is_click {
                                                let mut handled = false;
                                                if menu_manager.visible {
                                                    if let Some(pos) = last_cursor_pos {
                                                        let (cur_pw, _cur_ph) = pet.get_scaled_size();
                                                        let menu_x = (pet_off_x + cur_pw + 10.0) as i32;
                                                        let menu_y = pet_off_y as i32;

                                                        if let Some(action) = menu_manager.check_hit(pos.x, pos.y, menu_x, menu_y) {
                                                            handled = true;
                                                            match action {
                                                                menu::MenuAction::Chat => {
                                                                    if let Some(monitor) = window.current_monitor() {
                                                                        let scale_factor = monitor.scale_factor();
                                                                        let m_size = monitor.size();
                                                                        let m_pos = monitor.position();
                                                                        let chat_w = 300.0;
                                                                        let chat_h = 60.0;
                                                                        let m_w_logical = m_size.width as f64 / scale_factor;
                                                                        let m_h_logical = m_size.height as f64 / scale_factor;
                                                                        let m_x_logical = m_pos.x as f64 / scale_factor;
                                                                        let m_y_logical = m_pos.y as f64 / scale_factor;
                                                                        let center_x = m_x_logical + (m_w_logical / 2.0) - (chat_w / 2.0);
                                                                        let center_y = m_y_logical + (m_h_logical / 2.0) - (chat_h / 2.0);
                                                                        chat_window.show(winit::dpi::LogicalPosition::new(center_x, center_y));
                                                                    }
                                                                }
                                                                menu::MenuAction::Settings => {
                                                                    if settings_win.is_none() {
                                                                         let mut sw = SettingsWindow::new(elwt, settings_proxy.clone(), winit_icon.clone());
                                                                         sw.current_monitor_name = window_config.monitor_name.clone();
                                                                         sw.request_redraw();
                                                                         settings_win = Some(sw);
                                                                    } else if let Some(sw) = &settings_win {
                                                                        sw.focus();
                                                                    }
                                                                }
                                                                menu::MenuAction::Pomodoro => {
                                                                    if let Some(msg) = pomodoro_manager.update() {
                                                                        bubble_manager.show(&msg, Duration::from_secs(4), pet.scale);
                                                                    }
                                                                    if pomodoro_manager.toggle() {
                                                                        bubble_manager.show("Pomodoro Started! 🍅", Duration::from_secs(2), pet.scale);
                                                                    } else {
                                                                        bubble_manager.show("Pomodoro Stopped.", Duration::from_secs(2), pet.scale);
                                                                    }
                                                                }
                                                                menu::MenuAction::Music => {
                                                                    music_player.toggle();
                                                                    if music_player.is_playing() {
                                                                        bubble_manager.show("Music Started! 🎵", Duration::from_secs(2), pet.scale);
                                                                    } else {
                                                                        bubble_manager.show("Music Paused ⏸️", Duration::from_secs(2), pet.scale);
                                                                    }
                                                                    window.request_redraw();
                                                                }
                                                                menu::MenuAction::Exit => elwt.exit(),
                                                            }
                                                        }
                                                    }
                                                }

                                                if !handled {
                                                    // Only show quote if clicking directly on pet body
                                                    if let Some(pos) = last_cursor_pos {
                                                        if let Ok(win_pos) = window.outer_position() {
                                                            let monitor_mx = win_pos.x as f64 + pos.x - monitor_offset.0 as f64;
                                                            let monitor_my = win_pos.y as f64 + pos.y - monitor_offset.1 as f64;
                                                            if pet.check_hit(monitor_mx, monitor_my) {
                                                                if let Some(&quote) = rand::seq::SliceRandom::choose(&quotes[..], &mut rand::thread_rng()) {
                                                                    bubble_manager.show(quote, Duration::from_secs(4), pet.scale);
                                                                    window.request_redraw();
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            } else {
                                                pet.end_drag();
                                            }
                                            click_start_time = None;
                                            click_start_pos = None;
                                        }
                                    }
                                }
                            }
                            WindowEvent::CursorMoved { position, .. } => {
                                last_cursor_pos = Some(position);
                                if let Ok(win_pos) = window.outer_position() {
                                    let global_x = win_pos.x as f64 + position.x - pet_off_x;
                                    let global_y = win_pos.y as f64 + position.y - pet_off_y;
                                    let local_x = global_x - monitor_offset.0 as f64;
                                    let local_y = global_y - monitor_offset.1 as f64;

                                    if pet.state == PetState::Drag {
                                        pet.update_drag((global_x, global_y));
                                    } else if pet.state == PetState::Clingy {
                                        pet.follow_mouse((local_x, local_y));
                                    } else if let Some(start_pos) = click_start_pos {
                                        let dx = global_x - start_pos.0;
                                        let dy = global_y - start_pos.1;
                                        if (dx * dx + dy * dy).sqrt() > 5.0 {
                                            pet.start_drag(start_pos);
                                            pet.update_drag((global_x, global_y));
                                        }
                                    }
                                }
                            }
                            WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. } => {
                                if let Some(monitor) = window.current_monitor() {
                                    if let Some(refresh) = monitor.refresh_rate_millihertz() {
                                        target_frame_duration = Duration::from_nanos(1_000_000_000_000 / refresh as u64);
                                        println!("Monitor changed, new refresh rate: {} Hz", refresh as f32 / 1000.0);
                                    }
                                }
                            }
                            WindowEvent::Focused(true) => {
                                ui_primitives::harvest_memory();
                            }
                            _ => {}
                        }
                    } else if let Some(sw) = &mut settings_win {
                        if window_id == sw.id() {
                            match event {
                                WindowEvent::CloseRequested => {
                                    settings_win = None;
                                    ui_primitives::harvest_memory();
                                }
                                WindowEvent::Focused(true) => {
                                    ui_primitives::harvest_memory();
                                }
                                WindowEvent::MouseInput { state, button: btn, .. } => {
                                    if state == ElementState::Pressed {
                                        if let Some(pos) = settings_cursor_pos {
                                            let is_right_click = btn == MouseButton::Right;
                                            let action = sw.handle_click(pos.x, pos.y, is_right_click, &ai_config);
                                            match action {
                                                settings::SettingsAction::SetScale(s) => {
                                                    pet.scale = s;
                                                    sw.request_redraw();
                                                    window.request_redraw();
                                                }
                                                settings::SettingsAction::SetMode(m) => {
                                                    if pet.behavior_mode == BehaviorMode::Clingy && m != BehaviorMode::Clingy {
                                                        pet.state = PetState::Idle;
                                                        let count = pet.animations[&PetState::Idle].len();
                                                        if count > 0 {
                                                            pet.current_anim_variant = rand::thread_rng().gen_range(0..count);
                                                        } else {
                                                            pet.current_anim_variant = 0;
                                                        }
                                                    }
                                                    pet.behavior_mode = m;
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SelectMusicPath => {
                                                    let tx = path_tx.clone();
                                                    sw.window().set_window_level(winit::window::WindowLevel::Normal);
                                                    std::thread::spawn(move || {
                                                        let picked = rfd::FileDialog::new().pick_folder();
                                                        let _ = tx.send(picked);
                                                    });
                                                }
                                                settings::SettingsAction::SetLayer(layer) => {
                                                    current_layer = layer;
                                                    let level = match layer {
                                                        types::WindowLayer::Top => WindowLevel::AlwaysOnTop,
                                                        types::WindowLayer::Bottom => WindowLevel::Normal,
                                                    };
                                                    window.set_window_level(level);
                                                    #[cfg(target_os = "windows")]
                                                    {
                                                        if let RawWindowHandle::Win32(handle) = window.raw_window_handle() {
                                                            let hwnd = HWND(handle.hwnd as isize);
                                                            apply_window_styles(hwnd, layer == types::WindowLayer::Top);
                                                        }
                                                    }
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetAiApiKey(key) => {
                                                    ai_config.api_key = key;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config, scheduler.clone()));
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetAiBaseUrl(url) => {
                                                    ai_config.base_url = url;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config, scheduler.clone()));
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetAiModel(model) => {
                                                    ai_config.model = model;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config, scheduler.clone()));
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetAiReactLimit(limit) => {
                                                    ai_config.react_limit = limit;
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetAiL1Threshold(t) => {
                                                    ai_config.l1_summary_threshold = t;
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetAiL2Threshold(val) => {
                                                    ai_config.l2_merge_threshold = val;
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetAiInteractionFrequency(val) => {
                                                    ai_config.interaction_frequency = val;
                                                    interaction_manager.update_config(ai_config.clone());
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetAiTavilyKey(key) => {
                                                    ai_config.tavily_api_key = key;
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::UpdateAiConfig(new_config) => {
                                                    ai_config = new_config;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config, scheduler.clone()));
                                                    interaction_manager.update_config(ai_config.clone());
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::SetMonitor(name) => {
                                                    let available = window.available_monitors();
                                                    if let Some(monitor) = available.into_iter().find(|m| m.name().as_ref() == Some(&name)) {
                                                        let pos = monitor.position();
                                                        let size = monitor.size();
                                                        let win_size = window.inner_size();
                                                        let center_x = pos.x + (size.width as i32 / 2) - (win_size.width as i32 / 2);
                                                        let center_y = pos.y + (size.height as i32 / 2) - (win_size.height as i32 / 2);
                                                        window.set_outer_position(winit::dpi::PhysicalPosition::new(center_x, center_y));
                                                        window_config.monitor_name = Some(name.clone());
                                                        window_config.save();
                                                        sw.current_monitor_name = Some(name);
                                                        pet.screen_size = (size.width as f64, size.height as f64);
                                                        monitor_offset = (pos.x, pos.y);
                                                        pet.position.0 = (size.width as f64 - pet.window_size.0) / 2.0;
                                                        pet.position.1 = (size.height as f64 - pet.window_size.1) / 2.0;
                                                    }
                                                }
                                                settings::SettingsAction::SetAiSystemPrompt(prompt) => {
                                                    ai_config.system_prompt = prompt;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config, scheduler.clone()));
                                                    sw.request_redraw();
                                                }
                                                settings::SettingsAction::RequestHistory => {
                                                    if let Ok(history) = chat_kernel.get_recent_history(50) {
                                                        sw.history = std::sync::Arc::new(history);
                                                        sw.request_redraw();
                                                    }
                                                }
                                                settings::SettingsAction::RequestGc => {
                                                    ui_primitives::harvest_memory();
                                                }
                                                _ => {}
                                            }
                                        }
                                    } else {
                                        sw.handle_mouse_up();
                                    }
                                }
                                WindowEvent::MouseWheel { delta, .. } => {
                                    let dy = match delta {
                                        winit::event::MouseScrollDelta::LineDelta(_, y) => y * 30.0,
                                        winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                                    };
                                    sw.handle_scroll(dy, settings_cursor_pos);
                                    sw.request_redraw();
                                }
                                WindowEvent::KeyboardInput { event: key_event, .. } => {
                                    if sw.handle_key_input(&key_event, &mut ai_config, modifier_state) {
                                        interaction_manager.update_config(ai_config.clone());
                                    }
                                }
                                WindowEvent::Ime(ime_event) => {
                                    if let winit::event::Ime::Commit(text) = ime_event {
                                        sw.handle_ime(&text, &mut ai_config);
                                        interaction_manager.update_config(ai_config.clone());
                                    }
                                }
                                WindowEvent::CursorMoved { position, .. } => {
                                    settings_cursor_pos = Some(position);
                                    sw.handle_mouse_move(position.x, position.y, &ai_config);
                                }
                                WindowEvent::RedrawRequested => {
                                    let mode_str = match pet.behavior_mode {
                                        BehaviorMode::Static => "Static",
                                        BehaviorMode::Quiet => "Quiet",
                                        BehaviorMode::Active => "Active",
                                        BehaviorMode::Clingy => "Clingy",
                                    };
                                    sw.redraw(pet.scale, mode_str, music_player.music_path.as_deref(), current_layer, &ai_config);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::AboutToWait => {
                    // Poll for settings window background updates fallback
                    if let Some(sw) = &settings_win {
                         if let Ok(guard) = sw.render_back_buffer.try_lock() {
                             if guard.is_some() {
                                 sw.request_redraw();
                             }
                         }
                    }

                    // Update target_frame_duration based on current monitor refresh rate
                    if let Some(monitor) = window.current_monitor() {
                        let refresh_rate_millihertz = monitor.refresh_rate_millihertz().unwrap_or(60000);
                        let refresh_rate = refresh_rate_millihertz as f64 / 1000.0;
                        target_frame_duration = Duration::from_nanos((1_000_000_000.0 / refresh_rate.max(30.0)) as u64);
                    }

                    if let Ok(response) = ai_rx.try_recv() {
                        is_thinking = false;
                        thinking_start = None;
                        bubble_manager.show(&response, Duration::from_secs(6), pet.scale);
                        needs_pet_redraw = true;
                    }

                    if let Ok(path_opt) = path_rx.try_recv() {
                        if let Some(path) = path_opt {
                            music_player.set_path(path.clone());
                            window_config.music_path = Some(path);
                            window_config.save();
                        }
                        if let Some(sw) = &mut settings_win {
                            sw.window().set_window_level(winit::window::WindowLevel::AlwaysOnTop);
                            sw.request_redraw();
                        }
                    }

                    if let Ok(event) = MenuEvent::receiver().try_recv() {
                        if event.id == settings_id {
                            if settings_win.is_none() {
                                let sw = SettingsWindow::new(elwt, settings_proxy.clone(), winit_icon.clone());
                                sw.request_redraw();
                                settings_win = Some(sw);
                            } else if let Some(sw) = &settings_win {
                                sw.focus();
                            }
                        } else if event.id == quit_id {
                            elwt.exit();
                        }
                    }

                    let dt = last_update.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0).min(0.05);
                    last_update = Some(Instant::now());

                    // 1. Get Mouse Position
                    #[cfg(target_os = "windows")]
                    let mut current_mouse = (0.0, 0.0);
                    #[cfg(target_os = "windows")]
                    unsafe {
                        use windows::Win32::Foundation::POINT;
                        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
                        let mut pt = POINT::default();
                        if GetCursorPos(&mut pt).is_ok() {
                            current_mouse = (pt.x as f64, pt.y as f64);
                        }
                    }

                    // 2. Pet Position / Physics (Update depends on previous frame is_hovered, which is fine)
                    if pet.state == PetState::Clingy || pet.behavior_mode == BehaviorMode::Clingy {
                        let local_mx = current_mouse.0 - monitor_offset.0 as f64;
                        let local_my = current_mouse.1 - monitor_offset.1 as f64;
                        pet.follow_mouse((local_mx, local_my));
                    }
                    
                    // Logic update pause check (one-frame lag for hover is intended for stability)
                    let is_paused = is_hovered && pet.state != PetState::Clingy && pet.behavior_mode != BehaviorMode::Clingy;
                    pet.update_state(dt, is_paused);

                    // 3. Triggers
                    if let Some(msg) = pomodoro_manager.update() {
                        bubble_manager.show(&msg, Duration::from_secs(4), pet.scale);
                    }
                    if let Some(msg) = music_player.update() {
                        bubble_manager.show(&msg, Duration::from_secs(4), pet.scale);
                    }
                    if !is_thinking {
                        if let Some(system_event) = interaction_manager.check_for_trigger() {
                            let kernel = chat_kernel.clone();
                            let tx = ai_tx.clone();
                            let input_struct = system_event;
                            tokio::spawn(async move {
                                let response = kernel.handle_system_event(input_struct).await;
                                let _ = tx.send(response);
                            });
                            is_thinking = true;
                            thinking_start = Some(Instant::now());
                        }
                    }

                    // 4. GLOBAL LAYOUT CALCULATION (Runs every frame to ensure perfect sync)
                    let draw_scale = pet.scale.max(0.5);
                    let (cur_pw, cur_ph) = pet.get_scaled_size();
                    
                    let mut loading_w_f = 0.0;
                    let mut loading_h_f = 0.0;
                    if is_thinking && !loading_frames_decompressed.is_empty() {
                        loading_w_f = 32.0 * draw_scale as f64;
                        loading_h_f = 32.0 * draw_scale as f64;
                    }

                    let current_bubble_w_f = if bubble_manager.is_visible() { bubble_manager.current_width as f64 } else { 0.0 };
                    let current_bubble_h_f = if bubble_manager.is_visible() { bubble_manager.current_height as f64 } else { 0.0 };
                    let current_pomodoro_w_f = if pomodoro_manager.visible { (pomodoro::BASE_POMODORO_WIDTH as f32 * pet.scale) as f64 } else { 0.0 };
                    let current_pomodoro_h_f = if pomodoro_manager.visible { (pomodoro::BASE_POMODORO_HEIGHT as f32 * pet.scale) as f64 } else { 0.0 };

                    let menu_w_f = if menu_manager.visible || menu_manager.opacity > 0.0 { menu_manager.menu_width as f64 } else { 0.0 };
                    let menu_h_f_val = if menu_manager.visible || menu_manager.opacity > 0.0 { menu_manager.menu_height as f64 } else { 0.0 };

                    let gap_between = 10.0 * pet.scale as f64;
                    let pet_cx = cur_pw / 2.0;
                    let b_left = pet_cx - current_bubble_w_f / 2.0;
                    let p_left = pet_cx - current_pomodoro_w_f / 2.0;
                    let min_left = 0.0f64.min(b_left).min(p_left);
                    
                    let padding_edge_v = 40.0;
                    pet_off_x = padding_edge_v - min_left;

                    let padding_top_v = 40.0;
                    let mut extras_h = 0.0;
                    if bubble_manager.is_visible() { extras_h += current_bubble_h_f + gap_between; }
                    if pomodoro_manager.visible { extras_h += current_pomodoro_h_f + gap_between; }
                    if is_thinking { extras_h += loading_h_f + gap_between; }

                    pet_off_y = padding_top_v + extras_h;

                    let loading_y_f = if is_thinking { pet_off_y - gap_between - loading_h_f } else { pet_off_y };
                    let bubble_y_f = if is_thinking { loading_y_f - gap_between - current_bubble_h_f } 
                                   else if bubble_manager.is_visible() { pet_off_y - gap_between - current_bubble_h_f } 
                                   else { pet_off_y };
                    let pomodoro_y_f = if bubble_manager.is_visible() { bubble_y_f - gap_between - current_pomodoro_h_f }
                                     else if is_thinking { loading_y_f - gap_between - current_pomodoro_h_f }
                                     else { pet_off_y - gap_between - current_pomodoro_h_f };

                    let loading_x_f = pet_off_x + cur_pw/2.0 - loading_w_f / 2.0;
                    let bx_f = pet_off_x + b_left;
                    let px_f = pet_off_x + p_left;
                    let menu_x_f = pet_off_x + cur_pw + gap_between;
                    let menu_y_f = pet_off_y;

                    let pet_right = pet_off_x + cur_pw;
                    let menu_area_right_f = pet_right + gap_between + menu_w_f + (20.0 * pet.scale as f64);
                    let bubble_right_f = pet_off_x + b_left + current_bubble_w_f + padding_edge_v;
                    let pomodoro_right_f = pet_off_x + p_left + current_pomodoro_w_f + padding_edge_v;
                    let win_w_f = (menu_area_right_f.max(bubble_right_f).max(pomodoro_right_f) + 20.0) as u32;

                    let base_padding_bottom_v = 20.0 * pet.scale as f64;
                    let padding_bottom_v = base_padding_bottom_v.max(menu_h_f_val - cur_ph);
                    let win_h_f = (pet_off_y + cur_ph + padding_bottom_v + 40.0) as u32;

                    let target_x = monitor_offset.0 + (pet.position.0 - pet_off_x) as i32;
                    let target_y = monitor_offset.1 + (pet.position.1 - pet_off_y) as i32;
                    let target_pos = POINT { x: target_x, y: target_y };

                    // 5. UPDATE CACHED RECTS & HIT DETECTION
                    if bubble_manager.is_visible() {
                        // bubble_rect not needed here
                    } else {
                        bubble_rect = None;
                    }

                    let mouse_x = current_mouse.0;
                    let mouse_y = current_mouse.1;
                    
                    let over_pet = pet.check_hit(mouse_x - monitor_offset.0 as f64, mouse_y - monitor_offset.1 as f64);
                    
                    let menu_on_right_x = target_x as f64 + menu_x_f;
                    let menu_on_top_y = target_y as f64 + menu_y_f;
                    let over_menu = mouse_x >= menu_on_right_x && mouse_x <= menu_on_right_x + menu_w_f && 
                                    mouse_y >= menu_on_top_y && mouse_y <= menu_on_top_y + menu_h_f_val;

                    let over_bubble = if let Some((bx, by, bw, bh)) = bubble_rect {
                        let b_screen_x = target_x as f64 + bx as f64;
                        let b_screen_y = target_y as f64 + by as f64;
                        mouse_x >= b_screen_x && mouse_x <= (b_screen_x + bw as f64) && 
                        mouse_y >= b_screen_y && mouse_y <= (b_screen_y + bh as f64)
                    } else { false };

                    is_hovered = over_pet || over_menu || over_bubble;
                    
                    if is_hovered {
                        bubble_manager.keep_alive();
                        if over_pet || over_menu {
                            if !menu_manager.visible || menu_manager.opacity < 1.0 {
                                needs_pet_redraw = true;
                            }
                            menu_manager.visible = true;
                            menu_manager.opacity = (menu_manager.opacity + 0.1).min(1.0);
                            menu_visible_timer = Some(Instant::now());
                        }
                    } else {
                        if menu_visible_timer.map_or(true, |t| t.elapsed() > Duration::from_secs(5)) {
                            if menu_manager.opacity > 0.0 {
                                needs_pet_redraw = true;
                                menu_manager.opacity = (menu_manager.opacity - 0.05).max(0.0);
                                if menu_manager.opacity <= 0.0 {
                                    menu_manager.visible = false;
                                    menu_visible_timer = None;
                                }
                            }
                        }
                    }

                    // 6. REDRAW STATUS CHECKS
                    let mut pos_changed = false;
                    #[cfg(target_os = "windows")]
                    {
                        if (target_x != last_window_pos.x) || (target_y != last_window_pos.y) {
                            pos_changed = true;
                        }
                    }
                    
                    let pet_frame_changed = pet.current_frame_idx != last_frame_idx 
                        || pet.state != last_state 
                        || pet.facing_right != last_facing_right;
                    
                    let layout_changed = (pet_off_x - last_render_pet_off.0).abs() > 0.1 || (pet_off_y - last_render_pet_off.1).abs() > 0.1 || pos_changed;
                    
                    let loading_frame_idx = if is_thinking && !loading_frames_decompressed.is_empty() {
                        (Instant::now().duration_since(thinking_start.unwrap_or(Instant::now())).as_millis() / 100) as usize % loading_frames_decompressed.len()
                    } else { 0 };
                    let loading_frame_changed = is_thinking && loading_frame_idx != last_loading_frame_idx;

                    if needs_pet_redraw || pet_frame_changed || layout_changed || loading_frame_changed || 
                       Instant::now() >= pet.next_frame_at() {
                        needs_pet_redraw = true;
                    }

                    
                    if needs_pet_redraw {
                        last_frame_idx = pet.current_frame_idx;
                        last_loading_frame_idx = loading_frame_idx;
                        last_render_pet_off = (pet_off_x, pet_off_y);
                        last_state = pet.state;
                        last_facing_right = pet.facing_right;
                        last_window_pos = target_pos;
                        
                        // --- SYNCHRONOUS RENDER START ---
                        menu_manager.update_layout(draw_scale);

                        let win_w = win_w_f;
                        let win_h = win_h_f;

                        let total_size = win_w as usize * win_h as usize * 4usize;
                        if composite_data.len() != total_size { composite_data.resize(total_size, 0); }
                        composite_data.fill(0);

                        let win_w_usize = win_w as usize;
                        let win_h_usize = win_h as usize;

                        let facing_right = pet.facing_right;
                        
                        // Frame cache lookup - get key values first
                        let cache_key = (pet.state, pet.current_anim_variant, pet.current_frame_idx);
                        if let Some(cached_data) = frame_cache.get(&cache_key) {
                            decompressed_frame_buffer.clone_from(cached_data);
                        } else {
                            // Cache miss: decompress and store
                            let frame = pet.current_frame();
                            if let Ok(data) = lz4_flex::decompress_size_prepended(&frame.lz4_data) {
                                frame_cache.put(cache_key, data.clone());
                                decompressed_frame_buffer = data;
                            } else {
                                // Fallback if decompression fails
                                decompressed_frame_buffer.resize((frame.width * frame.height * 4) as usize, 0);
                            }
                        }
                        
                        // Get frame reference for rendering (it's the same frame we just cached)
                        let frame = pet.current_frame();
                        
                        let dest_y_start = pet_off_y as usize;
                        let dest_x_start = pet_off_x as usize;
                        
                        if (draw_scale - 1.0).abs() < 0.001 {
                            let fw = frame.width as usize;
                            let fh = frame.height as usize;
                                        // Parallelized source copy using rayon
                                        composite_data
                                            .par_chunks_mut(win_w_usize * 4)
                                            .enumerate()
                                            .skip(dest_y_start)
                                            .take(fh)
                                            .for_each(|(dy, dest_row): (usize, &mut [u8])| {
                                                let y = dy - dest_y_start;
                                                let (start_x_unflipped, end_x_unflipped) = frame.opaque_rows[y];
                                                if start_x_unflipped < end_x_unflipped {
                                                    let src_row = &decompressed_frame_buffer[y * fw * 4..(y + 1) * fw * 4];
                                                    let (start_x, end_x) = if facing_right {
                                                        (start_x_unflipped, end_x_unflipped)
                                                    } else {
                                                        (fw - end_x_unflipped, fw - start_x_unflipped)
                                                    };
                                                    for x in start_x..end_x {
                                                        let src_x = if facing_right { x } else { fw - 1 - x };
                                                        let s_idx = src_x * 4;
                                                        if src_row[s_idx + 3] > 0 {
                                                            let d_idx = (dest_x_start + x) * 4;
                                                            if d_idx + 4 <= dest_row.len() {
                                                                dest_row[d_idx..d_idx+4].copy_from_slice(&src_row[s_idx..s_idx+4]);
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                        } else {
                            let fw = frame.width as usize;
                            let fh = frame.height as usize;
                            let inv_scale = 1.0 / draw_scale;
                            
                                // Parallelized scaled source copy using rayon
                                composite_data
                                    .par_chunks_mut(win_w_usize * 4)
                                    .enumerate()
                                    .for_each(|(dy, dest_row): (usize, &mut [u8])| {
                                        let y_f32 = (dy as f64 - pet_off_y) as f32;
                                        if y_f32 < 0.0 || y_f32 >= cur_ph as f32 { return; }
                                        let y = y_f32 as usize;
                                        let src_y = (y as f32 * inv_scale) as usize;
                                        if src_y >= fh { return; }

                                        let (start_x_src_unflipped, end_x_src_unflipped) = frame.opaque_rows[src_y];
                                        if start_x_src_unflipped >= end_x_src_unflipped { return; }

                                        let start_x_dest = 0;
                                        let end_x_dest = cur_pw as usize;
                                        let src_row_idx = src_y * fw * 4;

                                        for x in start_x_dest..end_x_dest {
                                            let src_x_f32 = x as f32 * inv_scale;
                                            let src_x = if facing_right {
                                                src_x_f32 as usize
                                            } else {
                                                (fw as f32 - 1.0 - src_x_f32) as usize
                                            };

                                            if src_x >= fw { continue; }
                                            let s_idx = src_row_idx + src_x * 4;
                                            let a = decompressed_frame_buffer[s_idx + 3];
                                            if a > 0 {
                                                let d_idx = (dest_x_start + x) * 4;
                                                if d_idx + 4 <= dest_row.len() {
                                                    dest_row[d_idx..d_idx+4].copy_from_slice(&decompressed_frame_buffer[s_idx..s_idx+4]);
                                                }
                                            }
                                        }
                                    });
                        }

                        // 1.5 Loading (uses pre-decompressed frames)
                        if is_thinking && !loading_frames_decompressed.is_empty() {
                             let f_idx = loading_frame_idx % loading_frames_decompressed.len();
                             let (f_width, f_height, loading_data) = &loading_frames_decompressed[f_idx];
                             
                             let ly = loading_y_f as i32;
                             let lw = loading_w_f as i32;
                             let lh = loading_h_f as i32;
                              if ly >= 0 && lw > 0 && lh > 0 {
                                  let sx = *f_width as f32 / lw as f32;
                                  let sy = *f_height as f32 / lh as f32;
                                  for y in 0..lh as usize {
                                      let src_y = (y as f32 * sy) as usize;
                                      if src_y >= *f_height as usize { continue; }
                                      let dy_i32 = ly + y as i32;
                                      if dy_i32 < 0 || dy_i32 >= win_h as i32 { continue; }
                                      
                                      let src_row_off = src_y * *f_width as usize * 4;
                                      let dest_row_off = dy_i32 as usize * win_w_usize * 4;
                                      
                                      for x in 0..lw as usize {
                                          let src_x = (x as f32 * sx) as usize;
                                          if src_x < *f_width as usize {
                                              let s_idx = src_row_off + src_x * 4;
                                              let dx_i32 = loading_x_f as i32 + x as i32;
                                              if dx_i32 >= 0 && dx_i32 < win_w as i32 {
                                                  let d_idx = dest_row_off + dx_i32 as usize * 4;
                                                  let alpha = loading_data[s_idx + 3];
                                                  if alpha > 0 {
                                                      composite_data[d_idx..d_idx+3].copy_from_slice(&loading_data[s_idx..s_idx+3]);
                                                      composite_data[d_idx + 3] = composite_data[d_idx + 3].saturating_add(alpha);
                                                  }
                                              }
                                          }
                                      }
                                  }
                              }
                        }

                        // 2. Bubble
                        if bubble_manager.is_visible() {
                            bubble_manager.render_to_buffer(std::ptr::null_mut(), pet.scale);
                            let by = bubble_y_f as i32;
                            let bx = bx_f as i32;
                            if by >= 0 {
                                if let Some(b_pixels) = bubble_manager.pixel_data() {
                                    // Use actual pixel data dimensions to avoid mismatch after render_to_buffer updates
                                    let bw = bubble_manager.current_width as u32;
                                    let bh = bubble_manager.current_height as u32;
                                    
                                    // Safety check: ensure pixel data size matches dimensions
                                    let expected_len = (bw * bh * 4) as usize;
                                    if b_pixels.len() == expected_len {
                                        let b_u32 = unsafe {
                                        std::slice::from_raw_parts(
                                            b_pixels.as_ptr() as *const u32,
                                            b_pixels.len() / 4,
                                        )
                                    };
                                    let comp_u32 = unsafe {
                                        std::slice::from_raw_parts_mut(
                                            composite_data.as_mut_ptr() as *mut u32,
                                            composite_data.len() / 4,
                                        )
                                    };

                                    ui_primitives::blit_32bit_premultiplied(
                                        comp_u32,
                                        win_w as u32,
                                        win_h as u32,
                                        b_u32,
                                        bx,
                                        by,
                                        bw,
                                        bh,
                                    );
                                    }
                                }
                            }
                        }

                        // 2.5 Pomodoro
                        if pomodoro_manager.visible {
                            let p_size = (current_pomodoro_w_f * current_pomodoro_h_f * 4.0) as usize;
                            if pomodoro_data.len() != p_size { pomodoro_data.resize(p_size, 0); }
                            pomodoro_manager.render_to_buffer(pomodoro_data.as_mut_ptr(), pet.scale);
                            let py = pomodoro_y_f as i32;
                            let px = px_f as i32;
                            if py >= 0 {
                                let pw = current_pomodoro_w_f as usize;
                                let ph = current_pomodoro_h_f as usize;
                                for y in 0..ph {
                                    let dy = py as usize + y;
                                    if dy < win_h_usize {
                                        let src_row_off = y * pw * 4;
                                        let dest_row_off = dy * win_w_usize * 4;
                                        for x in 0..pw {
                                            let s = src_row_off + x * 4;
                                            let a = pomodoro_data[s + 3];
                                            if a > 0 {
                                                let dx = px as usize + x;
                                                if dx < win_w_usize {
                                                    let d = dest_row_off + dx * 4;
                                                    composite_data[d..d+3].copy_from_slice(&pomodoro_data[s..s+3]);
                                                    composite_data[d+3] = (composite_data[d+3] as u16 + a as u16).min(255) as u8;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // 3. Menu
                        if menu_manager.visible || menu_manager.opacity > 0.0 {
                            menu_manager.render(composite_data.as_mut_slice(), win_w as i32, win_h as i32, menu_x_f as i32, menu_y_f as i32);
                        }

                        #[cfg(target_os = "windows")]
                        {
                            if let RawWindowHandle::Win32(_) = window.raw_window_handle() {
                                unsafe { if let Some(ctx) = &mut render_ctx { ctx.update(&composite_data, win_w as i32, win_h as i32, Some(target_pos)); } }
                            }
                        }
                        // --- SYNCHRONOUS RENDER END ---
                    } else if pos_changed {
                        // Only move if we didn't redraw (atomic move is handled in ctx.update above)
                        window.set_outer_position(PhysicalPosition::new(target_pos.x, target_pos.y));
                        last_window_pos = target_pos;
                    }
                    
                    // --- OPTIMIZATION: Precise scheduling ---
                    let now = Instant::now();
                    let mut next_deadline = now + Duration::from_secs(1);
                    
                    if pet.state != PetState::Drag {
                        next_deadline = next_deadline.min(pet.next_frame_at());
                    }
                    if is_thinking {
                        next_deadline = next_deadline.min(now + Duration::from_millis(100));
                    }

                    let needs_high_freq = pet.state == PetState::Move || 
                                         pet.state == PetState::Clingy || 
                                         pet.state == PetState::Drag ||
                                         menu_manager.opacity > 0.0 ||
                                         bubble_manager.is_visible() ||
                                         pomodoro_manager.visible;

                    if needs_high_freq {
                        next_deadline = next_deadline.min(now + target_frame_duration);
                    }

                    if chat_window.is_visible() {
                        let blink_deadline = chat_window.next_blink_at();
                        if now >= blink_deadline - Duration::from_millis(10) {
                             chat_window.request_redraw_actual();
                        }
                        // Only wake up for blink if we are close or if no animation is active
                        next_deadline = next_deadline.min(blink_deadline);
                    }

                    if let Some(sw) = &settings_win {
                        let blink_deadline = sw.next_blink_at();
                        if now >= blink_deadline - Duration::from_millis(10) {
                             sw.request_redraw_actual();
                        }
                        next_deadline = next_deadline.min(blink_deadline);
                    }

                    elwt.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_deadline));
                    needs_pet_redraw = false;
                }
                _ => {}
            }
        })
        .unwrap();
}

#[cfg(target_os = "windows")]
fn apply_window_styles(hwnd: HWND, top_most: bool) {
    unsafe {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, GWL_STYLE, WS_CAPTION, WS_EX_LAYERED,
            WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_THICKFRAME, WS_VISIBLE,
        };
        // STYLE: Remove Caption/ThickFrame, Force Popup + Visible
        let style = GetWindowLongW(hwnd, GWL_STYLE);
        let new_style = (style & !(WS_CAPTION.0 as i32 | WS_THICKFRAME.0 as i32))
            | WS_POPUP.0 as i32
            | WS_VISIBLE.0 as i32;
        SetWindowLongW(hwnd, GWL_STYLE, new_style);

        // EX_STYLE: Layered + ToolWindow + (Optional) TopMost
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let mut new_ex_style = ex_style | WS_EX_LAYERED.0 as i32 | WS_EX_TOOLWINDOW.0 as i32;

        if top_most {
            new_ex_style |= WS_EX_TOPMOST.0 as i32;
        } else {
            // If strictly needed to remove topmost, verify if winit handles it.
            // winit's set_window_level might toggle this bit.
            // We enforce it just in case.
            new_ex_style &= !(WS_EX_TOPMOST.0 as i32);
        }
        SetWindowLongW(hwnd, GWL_EXSTYLE, new_ex_style);
    }
}
