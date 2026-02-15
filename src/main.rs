#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod anim;
mod bubble;
mod chat_window;
mod menu;
mod music_player;
mod pet;
mod pomodoro;
mod render;
mod settings_window;
mod types;
mod ai;
mod interaction;


use chat_window::{ChatWindow, ChatAction};
use settings_window::SettingsWindow;

use pet::Pet;
use std::collections::HashMap;
use std::rc::Rc;
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
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, WindowLevel},
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowBuilderExtWindows;

#[cfg(target_os = "windows")]
use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;

use image::GenericImageView;

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();

    let event_loop = EventLoop::new().unwrap();

    // Global Hotkey Setup
    let hotkey_manager = GlobalHotKeyManager::new().unwrap();
    let hotkey = HotKey::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyM);
    hotkey_manager.register(hotkey).unwrap();
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

    // Load Loading GIF
    let loading_frames = anim::load_gif_from_memory(include_bytes!("../assets/icons/loading.gif"));

    // Helper to mirror variants
    let mirror_variants = |variants: &Vec<Vec<PreprocessedFrame>>| -> Vec<Vec<PreprocessedFrame>> {
        variants
            .iter()
            .map(|variant| {
                variant
                    .iter()
                    .map(|frame| anim::flip_frame_horizontal(frame))
                    .collect()
            })
            .collect()
    };

    // Generate Left-facing (Mirrored) assets
    let idle_frames_left = mirror_variants(&idle_frames_right);
    let move_frames_left = mirror_variants(&move_frames_right);
    let drag_frames_left = mirror_variants(&drag_frames_right);

    // Store as (Right, Left) pairs
    let mut animation_map: HashMap<
        PetState,
        (Vec<Vec<PreprocessedFrame>>, Vec<Vec<PreprocessedFrame>>),
    > = HashMap::new();
    animation_map.insert(
        PetState::Clingy,
        (move_frames_right.clone(), move_frames_left.clone()),
    );
    animation_map.insert(PetState::Idle, (idle_frames_right, idle_frames_left));
    animation_map.insert(PetState::Move, (move_frames_right, move_frames_left));
    animation_map.insert(PetState::Drag, (drag_frames_right, drag_frames_left));

    // Calculate dynamic "envelope" size based on max GIF dimensions
    let mut max_pw = 0;
    let mut max_ph = 0;
    for (_, (right, left)) in &animation_map {
        for variant in right {
            for frame in variant {
                max_pw = max_pw.max(frame.width);
                max_ph = max_ph.max(frame.height);
            }
        }
        for variant in left {
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

    // Determine refresh rate
    let monitor = window.current_monitor();
    let refresh_rate_millihertz = monitor
        .and_then(|m| m.refresh_rate_millihertz())
        .unwrap_or(60000);
    let _fps = refresh_rate_millihertz as f64 / 1000.0;

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
    let (ai_tx, ai_rx) = std::sync::mpsc::channel::<String>();
    let mut chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config));
    let music_dir = window_config.music_path.clone().unwrap_or_else(|| std::path::PathBuf::from("assets/music"));
    if music_dir.exists() {
        music_player.set_path(music_dir);
    }
    let mut interaction_manager = interaction::InteractionManager::new(ai_config.clone());
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
    let mut last_update = Some(Instant::now());
    let mut last_cursor_pos: Option<PhysicalPosition<f64>> = None;

    // Click detection
    let mut click_start_time: Option<Instant> = None;
    let mut click_start_pos: Option<(f64, f64)> = None;
    let mut settings_cursor_pos: Option<PhysicalPosition<f64>> = None;

    let mut settings_win: Option<SettingsWindow> = None;
    let mut menu_visible_timer: Option<Instant> = None;
    let mut current_layer = types::WindowLayer::Top;
    let mut bubble_rect: Option<(i32, i32, i32, i32)> = None;

    let mut composite_data: Vec<u8> = Vec::new();
    let mut pomodoro_data: Vec<u8> = Vec::new();

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

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
                Event::WindowEvent { window_id, event } => {
                    if let WindowEvent::ModifiersChanged(modifiers) = &event {
                        modifier_state = modifiers.state();
                    }

                    if chat_window.id() == window_id {
                         match chat_window.handle_event(&event, modifier_state) {
                             ChatAction::Send(msg) => {
                                 println!("User sent: {}", msg);
                                 is_thinking = true;
                                 thinking_start = Some(Instant::now());
                                 
                                 // Update kernel with current config if changed
                                 chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config));
                                 let kernel = chat_kernel.clone();
                                 let tx = ai_tx.clone();
                                 let input = msg.clone();
                                 
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
                                let draw_scale = pet.scale.max(0.5);
                                let (cur_pw, cur_ph) = pet.get_scaled_size();
                                menu_manager.update_layout(draw_scale);

                                // Update Thinking State (Polling)
                                if is_thinking {
                                    // Just keep it true until channel receives response
                                }

                                // Calc Loading GIF dimensions
                                let mut loading_w = 0;
                                let mut loading_h = 0;
                                if is_thinking && !loading_frames.is_empty() {
                                    // Cap loading size to roughly 32x32 scaled
                                    let target_size = 32.0 * draw_scale;
                                    loading_w = target_size as i32;
                                    loading_h = target_size as i32;
                                }

                                // Recalculate scaled dimensions for bubble and pomodoro
                                let current_bubble_w = bubble_manager.current_width;
                                let mut current_bubble_h = bubble_manager.current_height;
                                let current_pomodoro_w = (pomodoro::BASE_POMODORO_WIDTH as f32 * pet.scale) as i32;
                                let mut current_pomodoro_h = (pomodoro::BASE_POMODORO_HEIGHT as f32 * pet.scale) as i32;

                                // Menu visibility logic
                                let menu_w = if menu_manager.visible || menu_manager.opacity > 0.0 {
                                    menu_manager.menu_width as u32
                                } else {
                                    0
                                };

                                if !bubble_manager.is_visible() {
                                    current_bubble_h = 0;
                                }

                                if !pomodoro_manager.visible {
                                    current_pomodoro_h = 0;
                                }

                                // Layout Strategy
                                let padding_edge = 40.0;
                                let gap_between = 10.0 * pet.scale as f64;

                                let pet_cx = cur_pw / 2.0;
                                let b_left = pet_cx - current_bubble_w as f64 / 2.0;
                                let p_left = pet_cx - current_pomodoro_w as f64 / 2.0;

                                let min_left = 0.0f64.min(b_left).min(p_left);
                                let pet_x = padding_edge - min_left;

                                let pet_right = pet_x + cur_pw;
                                let menu_area_right = pet_right + gap_between + menu_w as f64 + (20.0 * pet.scale as f64);
                                let bubble_right = pet_x + b_left + current_bubble_w as f64 + padding_edge;
                                let pomodoro_right = pet_x + p_left + current_pomodoro_w as f64 + padding_edge;

                                let win_w = (menu_area_right.max(bubble_right).max(pomodoro_right) + 20.0) as u32;


                                let padding_top = 40.0;
                                let mut extras_h = 0.0;
                                
                                if bubble_manager.is_visible() {
                                    extras_h += current_bubble_h as f64 + gap_between;
                                }
                                if pomodoro_manager.visible {
                                    extras_h += current_pomodoro_h as f64 + gap_between;
                                }
                                if is_thinking {
                                    extras_h += loading_h as f64 + gap_between;
                                }

                                pet_off_x = pet_x;
                                pet_off_y = padding_top + extras_h;

                                let base_padding_bottom = 20.0 * pet.scale as f64;
                                let menu_h = if menu_manager.visible || menu_manager.opacity > 0.0 {
                                    menu_manager.menu_height as f64
                                } else {
                                    0.0
                                };
                                let padding_bottom = base_padding_bottom.max(menu_h - cur_ph);

                                let win_h = (pet_off_y + cur_ph + padding_bottom + 40.0) as u32;

                                // --- OPTIMIZATION: Only resize if dimensions change ---
                                let current_size = window.inner_size();
                                if current_size.width != win_w || current_size.height != win_h {
                                    let _ = window.request_inner_size(winit::dpi::PhysicalSize::new(win_w, win_h));
                                }

                                let loading_y = if is_thinking {
                                    pet_off_y - gap_between - loading_h as f64
                                } else {
                                    pet_off_y 
                                };

                                let bubble_y = if is_thinking {
                                    loading_y - gap_between - current_bubble_h as f64
                                } else if bubble_manager.is_visible() {
                                    pet_off_y - gap_between - current_bubble_h as f64
                                } else {
                                    pet_off_y
                                };

                                let pomodoro_y = if bubble_manager.is_visible() {
                                    bubble_y - gap_between - current_pomodoro_h as f64
                                } else if is_thinking {
                                    loading_y - gap_between - current_pomodoro_h as f64
                                } else {
                                    pet_off_y - gap_between - current_pomodoro_h as f64
                                };

                                let bx = (pet_off_x + b_left) as i32;
                                let px = (pet_off_x + p_left) as i32;
                                let menu_x = (pet_off_x + cur_pw + gap_between) as i32;
                                let menu_y = pet_off_y as i32;
                                let loading_x = (pet_off_x + cur_pw/2.0 - loading_w as f64 / 2.0) as i32;

                                let total_size = win_w as usize * win_h as usize * 4usize;
                                if composite_data.len() != total_size {
                                    composite_data.resize(total_size, 0);
                                }
                                composite_data.fill(0);

                                let win_w_usize = win_w as usize;
                                let win_h_usize = win_h as usize;

                                // 1. Draw Pet (Optimized with opaque_rows and row-based copy)
                                let frame = pet.current_frame();
                                let dest_y_start = pet_off_y as usize;
                                let dest_x_start = pet_off_x as usize;

                                if (draw_scale - 1.0).abs() < 0.001 {
                                    // FAST PATH: Direct row copies using opaque_rows
                                    let fw = frame.width as usize;
                                    let fh = frame.height as usize;
                                    for y in 0..fh {
                                        let dy = dest_y_start + y;
                                        if dy < win_h_usize {
                                            let (start_x, end_x) = frame.opaque_rows[y];
                                            if start_x < end_x {
                                                let src_row = &frame.data[y * fw * 4..(y + 1) * fw * 4];
                                                
                                                if dest_x_start + end_x <= win_w_usize {
                                                    // For fast path scale 1.0, we can copy the whole opaque segment if it's mostly opaque,
                                                    // but the frame might still have internal transparency.
                                                    // Let's at least bound the loop by opaque_rows.
                                                    for x in start_x..end_x {
                                                        let s_idx = x * 4;
                                                        let a = src_row[s_idx + 3];
                                                        if a > 0 {
                                                            let d_idx = (dy * win_w_usize + dest_x_start + x) * 4;
                                                            composite_data[d_idx..d_idx+4].copy_from_slice(&src_row[s_idx..s_idx+4]);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                } else {
                                    // Scaled path using opaque_rows to bound inner loop
                                    let fw = frame.width as usize;
                                    let fh = frame.height as usize;
                                    for y in 0..(cur_ph as u32) {
                                        let src_y = (y as f32 / draw_scale) as usize;
                                        if src_y >= fh { continue; }
                                        let dy = (y as f64 + pet_off_y) as usize;
                                        if dy >= win_h_usize { continue; }
                                        
                                        let (start_x_src, end_x_src) = frame.opaque_rows[src_y];
                                        if start_x_src >= end_x_src { continue; }

                                        let src_row_idx = src_y * fw * 4;
                                        let dest_row_start = dy * win_w_usize * 4;
                                        
                                        // Bound x loop by scaled opaque_rows
                                        let start_x_dest = (start_x_src as f32 * draw_scale) as u32;
                                        let end_x_dest = ((end_x_src as f32 * draw_scale) as u32).min(cur_pw as u32);

                                        for x in start_x_dest..end_x_dest {
                                            let src_x = (x as f32 / draw_scale) as usize;
                                            if src_x >= fw { continue; }
                                            let dx = (x as f64 + pet_off_x) as usize;
                                            if dx >= win_w_usize { continue; }
                                            let s_idx = src_row_idx + src_x * 4;
                                            let a = frame.data[s_idx + 3];
                                            if a > 0 {
                                                let d_idx = dest_row_start + dx * 4;
                                                composite_data[d_idx] = frame.data[s_idx];
                                                composite_data[d_idx + 1] = frame.data[s_idx + 1];
                                                composite_data[d_idx + 2] = frame.data[s_idx + 2];
                                                composite_data[d_idx + 3] = a;
                                            }
                                        }
                                    }
                                }

                                // 1.5 Draw Loading
                                if is_thinking && !loading_frames.is_empty() {
                                     let frame_idx = (Instant::now().duration_since(thinking_start.unwrap_or(Instant::now())).as_millis() / 100) as usize % loading_frames.len();
                                     let frame = &loading_frames[frame_idx];
                                     
                                     let ly = loading_y as i32;
                                     if ly >= 0 && loading_w > 0 && loading_h > 0 {
                                         let scale_x = frame.width as f32 / loading_w as f32;
                                         let scale_y = frame.height as f32 / loading_h as f32;
                                         let lw_usize = loading_w as usize;
                                         let lh_usize = loading_h as usize;

                                         for y in 0..lh_usize {
                                             for x in 0..lw_usize {
                                                 let src_x = (x as f32 * scale_x) as u32;
                                                 let src_y = (y as f32 * scale_y) as u32;
                                                 
                                                 if src_x >= frame.width as u32 || src_y >= frame.height as u32 { continue; }
                                                 
                                                 let src_idx = (src_y as usize * frame.width as usize + src_x as usize) * 4;
                                                 let dest_x_i32 = loading_x + x as i32;
                                                 let dest_y_i32 = ly + y as i32;
                                                 
                                                  if dest_x_i32 >= 0 && dest_x_i32 < win_w as i32 && dest_y_i32 >= 0 && dest_y_i32 < win_h as i32 {
                                                     let dest_x = dest_x_i32 as usize;
                                                     let dest_y = dest_y_i32 as usize;
                                                     let dest_idx = (dest_y * win_w_usize + dest_x) * 4;
                                                     let alpha = frame.data[src_idx + 3];
                                                     if alpha > 0 {
                                                         composite_data[dest_idx] = frame.data[src_idx];
                                                         composite_data[dest_idx + 1] = frame.data[src_idx + 1];
                                                         composite_data[dest_idx + 2] = frame.data[src_idx + 2];
                                                         let new_alpha = (composite_data[dest_idx + 3] as u16 + alpha as u16).min(255) as u8;
                                                         composite_data[dest_idx + 3] = new_alpha;
                                                     }
                                                  }
                                             }
                                         }
                                     }
                                }

                                // 2. Draw Bubble (Optimized with early alpha exit)
                                if bubble_manager.is_visible() {
                                    bubble_manager.render_to_buffer(std::ptr::null_mut(), pet.scale);

                                    let by = bubble_y as i32;
                                    bubble_rect = Some((bx, by, current_bubble_w, current_bubble_h));

                                    if by >= 0 {
                                        if let Some(b_pixels) = bubble_manager.pixel_data() {
                                            let bw = current_bubble_w as usize;
                                            let bh = current_bubble_h as usize;
                                            for y in 0..bh {
                                                let dy = (by + y as i32) as usize;
                                                if dy < win_h_usize {
                                                    let src_row_start = y * bw * 4;
                                                    let bx_usize = bx as usize;
                                                    let dest_row_start = (dy * win_w_usize + bx_usize) * 4;
                                                    for x in 0..bw {
                                                        let s = src_row_start + x * 4;
                                                        let a = b_pixels[s + 3];
                                                        if a > 0 {
                                                            let d = dest_row_start + x * 4;
                                                            if a == 255 {
                                                                composite_data[d..d+4].copy_from_slice(&b_pixels[s..s+4]);
                                                            } else {
                                                                // Blend for non-premultiplied or partial bubble edges
                                                                // But bubble_manager likely uses Direct2D which is premultiplied.
                                                                // If it's premultiplied, standard additive-like blend or copy.
                                                                composite_data[d..d+4].copy_from_slice(&b_pixels[s..s+4]);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // 2.5 Draw Pomodoro (Optimized with persistent buffer and row copies)
                                if pomodoro_manager.visible {
                                    let p_size = (current_pomodoro_w * current_pomodoro_h * 4) as usize;
                                    if pomodoro_data.len() != p_size {
                                        pomodoro_data.resize(p_size, 0);
                                    }
                                    pomodoro_manager.render_to_buffer(pomodoro_data.as_mut_ptr(), pet.scale);

                                    let py = pomodoro_y as i32;
                                    if py >= 0 {
                                        let pw = current_pomodoro_w as usize;
                                        let ph = current_pomodoro_h as usize;
                                        let px_usize = px as usize;
                                        for y in 0..ph {
                                            let dy = (py + y as i32) as usize;
                                            if dy < win_h_usize {
                                                let src_idx = y * pw * 4;
                                                let dest_idx = (dy * win_w_usize + px_usize) * 4;
                                                for x in 0..pw {
                                                    let s = src_idx + x * 4;
                                                    let a = pomodoro_data[s + 3];
                                                    if a > 0 {
                                                        let d = dest_idx + x * 4;
                                                        // Simple additive alpha for pomodoro if needed, or just copy
                                                        composite_data[d] = pomodoro_data[s];
                                                        composite_data[d+1] = pomodoro_data[s+1];
                                                        composite_data[d+2] = pomodoro_data[s+2];
                                                        let new_a = (composite_data[d+3] as u16 + a as u16).min(255) as u8;
                                                        composite_data[d+3] = new_a;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // 3. Draw Menu
                                if menu_manager.visible || menu_manager.opacity > 0.0 {
                                    menu_manager.render(
                                        composite_data.as_mut_slice(),
                                        win_w as i32,
                                        win_h as i32,
                                        menu_x,
                                        menu_y,
                                    );
                                }

                                #[cfg(target_os = "windows")]
                                {
                                    if let RawWindowHandle::Win32(_) = window.raw_window_handle() {
                                        unsafe {
                                            if let Some(ctx) = &mut render_ctx {
                                                ctx.update(&composite_data, win_w as i32, win_h as i32);
                                            }
                                        }
                                    }
                                }
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
                                                if menu_manager.visible {
                                                    if let Some(pos) = last_cursor_pos {
                                                        let (cur_pw, _cur_ph) = pet.get_scaled_size();
                                                        let menu_x = (pet_off_x + cur_pw + 10.0) as i32;
                                                        let menu_y = pet_off_y as i32;

                                                        if let Some(action) = menu_manager.check_hit(pos.x, pos.y, menu_x, menu_y) {
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
                                                                         let mut sw = SettingsWindow::new(elwt, winit_icon.clone());
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
                                                                }
                                                                menu::MenuAction::Exit => elwt.exit(),
                                                            }
                                                        }
                                                    }
                                                }

                                                if !is_click {
                                                    pet.end_drag();
                                                } else {
                                                    if let Some(&quote) = rand::seq::SliceRandom::choose(&quotes[..], &mut rand::thread_rng()) {
                                                        bubble_manager.show(quote, Duration::from_secs(4), pet.scale);
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
                            _ => {}
                        }
                    } else if let Some(sw) = &mut settings_win {
                        if window_id == sw.id() {
                            match event {
                                WindowEvent::CloseRequested => { settings_win = None; }
                                WindowEvent::MouseInput { state, button: btn, .. } => {
                                    if state == ElementState::Pressed {
                                        if let Some(pos) = settings_cursor_pos {
                                            let is_right_click = btn == MouseButton::Right;
                                            let action = sw.handle_click(pos.x, pos.y, is_right_click, &ai_config);
                                            match action {
                                                settings_window::SettingsAction::SetScale(s) => {
                                                    pet.scale = s;
                                                    sw.request_redraw();
                                                    window.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetMode(m) => {
                                                    if pet.behavior_mode == BehaviorMode::Clingy && m != BehaviorMode::Clingy {
                                                        pet.state = PetState::Idle;
                                                    }
                                                    pet.behavior_mode = m;
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SelectMusicPath => {
                                                    let tx = path_tx.clone();
                                                    sw.window().set_window_level(winit::window::WindowLevel::Normal);
                                                    std::thread::spawn(move || {
                                                        let picked = rfd::FileDialog::new().pick_folder();
                                                        let _ = tx.send(picked);
                                                    });
                                                }
                                                settings_window::SettingsAction::SetLayer(layer) => {
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
                                                settings_window::SettingsAction::SetAiApiKey(key) => {
                                                    ai_config.api_key = key;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config));
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetAiBaseUrl(url) => {
                                                    ai_config.base_url = url;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config));
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetAiModel(model) => {
                                                    ai_config.model = model;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config));
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetAiReactLimit(limit) => {
                                                    ai_config.react_limit = limit;
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetAiL1Threshold(t) => {
                                                    ai_config.l1_summary_threshold = t;
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetAiL2Threshold(val) => {
                                                    ai_config.l2_merge_threshold = val;
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetAiInteractionFrequency(val) => {
                                                    ai_config.interaction_frequency = val;
                                                    interaction_manager.update_config(ai_config.clone());
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetAiTavilyKey(key) => {
                                                    ai_config.tavily_api_key = key;
                                                    ai_config.save();
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::SetMonitor(name) => {
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
                                                settings_window::SettingsAction::SetAiSystemPrompt(prompt) => {
                                                    ai_config.system_prompt = prompt;
                                                    ai_config.save();
                                                    chat_kernel = std::sync::Arc::new(ai::kernel::ChatKernel::new(&ai_config));
                                                    sw.request_redraw();
                                                }
                                                settings_window::SettingsAction::RequestHistory => {
                                                    if let Ok(history) = chat_kernel.get_recent_history(50) {
                                                        sw.history = history;
                                                        sw.request_redraw();
                                                    }
                                                }
                                                settings_window::SettingsAction::None => {}
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
                    let mut needs_pet_redraw = false;
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

                    // Settings window redraw is handled by its own event signals, 
                    // removing the blanket sw.request_redraw() here for performance.

                    if let Ok(event) = MenuEvent::receiver().try_recv() {
                        if event.id == settings_id {
                            if settings_win.is_none() {
                                let sw = SettingsWindow::new(elwt, winit_icon.clone());
                                // Redraw only once on creation
                                sw.request_redraw();
                                settings_win = Some(sw);
                            } else if let Some(sw) = &settings_win {
                                sw.focus();
                            }
                        } else if event.id == quit_id {
                            elwt.exit();
                        }
                    }

                    let mut is_hovered = false;
                    #[cfg(target_os = "windows")]
                    unsafe {
                        use windows::Win32::Foundation::POINT;
                        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
                        let mut pt = POINT::default();
                        if GetCursorPos(&mut pt).is_ok() {
                            let mouse_x = pt.x as f64;
                            let mouse_y = pt.y as f64;

                            if pet.state == PetState::Clingy || pet.behavior_mode == BehaviorMode::Clingy {
                                pet.follow_mouse((mouse_x - monitor_offset.0 as f64, mouse_y - monitor_offset.1 as f64));
                            }

                            let pet_screen_x = pet.position.0 + monitor_offset.0 as f64;
                            let pet_screen_y = pet.position.1 + monitor_offset.1 as f64;
                            let (sys_pw, sys_ph) = pet.get_scaled_size();
                            let menu_on_right_x = pet_screen_x + sys_pw + 10.0;
                            let menu_w = menu_manager.menu_width as f64;
                            let menu_h = menu_manager.menu_height as f64;

                            let over_pet = mouse_x >= pet_screen_x && mouse_x <= pet_screen_x + sys_pw && mouse_y >= pet_screen_y && mouse_y <= pet_screen_y + sys_ph;
                            let over_menu = mouse_x >= menu_on_right_x && mouse_x <= menu_on_right_x + menu_w && mouse_y >= pet_screen_y && mouse_y <= pet_screen_y + menu_h;
                            let over_bubble = if let Some((bx, by, bw, bh)) = bubble_rect {
                                mouse_x >= bx as f64 && mouse_x <= (bx + bw) as f64 && mouse_y >= by as f64 && mouse_y <= (by + bh) as f64
                            } else { false };

                            if over_pet || over_menu || over_bubble {
                                is_hovered = true;
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
                        }
                    }

                    if let Some(elapsed) = last_update.map(|t| t.elapsed().as_secs_f64()) {
                        let dt = elapsed.min(0.05);
                        pet.update_state(dt, is_hovered);
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
                                let input = system_event;
                                tokio::spawn(async move {
                                    let response = kernel.handle_system_event(input).await;
                                    let _ = tx.send(response);
                                });
                                is_thinking = true;
                                thinking_start = Some(Instant::now());
                            }
                        }
                        window.set_outer_position(PhysicalPosition::new(
                            monitor_offset.0 + (pet.position.0 - pet_off_x) as i32,
                            monitor_offset.1 + (pet.position.1 - pet_off_y) as i32,
                        ));
                        
                        // Redraw if anything is active or animation frame changed
                        if is_thinking || pet.state != PetState::Idle || is_hovered || menu_manager.opacity > 0.0 || bubble_manager.is_visible() || Instant::now() >= pet.next_frame_at() {
                            needs_pet_redraw = true;
                        }
                    }
                    last_update = Some(Instant::now());
                    
                    if needs_pet_redraw {
                        window.request_redraw();
                    }
                    
                    // --- OPTIMIZATION: Precise scheduling ---
                    let mut next_deadline = Instant::now() + Duration::from_secs(1); // Default 1s wait if idle
                    
                    if is_thinking {
                        // Loading GIF is ~10fps (100ms)
                        next_deadline = next_deadline.min(Instant::now() + Duration::from_millis(100));
                    }
                    
                    if pet.state != PetState::Drag {
                        let pet_deadline = pet.next_frame_at();
                        next_deadline = next_deadline.min(pet_deadline);
                    }

                    // If menu is fading, we need high frequency
                    if menu_manager.opacity > 0.0 && menu_manager.opacity < 1.0 {
                        next_deadline = next_deadline.min(Instant::now() + Duration::from_millis(16));
                    }

                    if chat_window.is_visible() {
                        let blink_deadline = chat_window.next_blink_at();
                        // Request redraw slightly before or at deadline
                        if Instant::now().duration_since(blink_deadline - Duration::from_millis(5)) < Duration::from_millis(10) || Instant::now() >= blink_deadline {
                             chat_window.request_redraw_actual();
                        }
                        next_deadline = next_deadline.min(blink_deadline);
                    }

                    if let Some(sw) = &settings_win {
                        let blink_deadline = sw.next_blink_at();
                        if Instant::now().duration_since(blink_deadline - Duration::from_millis(5)) < Duration::from_millis(10) || Instant::now() >= blink_deadline {
                             sw.request_redraw_actual();
                        }
                        next_deadline = next_deadline.min(blink_deadline);
                    }

                    elwt.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(next_deadline));
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
