mod anim;
mod bubble;
mod menu;
mod pet;
mod render;
mod settings_window;
mod types;

use settings_window::SettingsWindow;

use pet::Pet;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use types::{BehaviorMode, PetState, PreprocessedFrame};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, WindowLevel},
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowBuilderExtWindows;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, GWL_STYLE, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST,
};

fn main() {
    let event_loop = EventLoop::new().unwrap();

    // Load assets (Right-facing by default)
    let idle_frames_right = vec![
        anim::load_gif_processed("assets/gifs/idle1.gif"),
        anim::load_gif_processed("assets/gifs/idle2.gif"),
        anim::load_gif_processed("assets/gifs/idle3.gif"),
        anim::load_gif_processed("assets/gifs/idle4.gif"),
    ];
    let move_frames_right = vec![anim::load_gif_processed("assets/gifs/move.gif")];
    let drag_frames_right = vec![anim::load_gif_processed("assets/gifs/drag.gif")];

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

    let win_w = (max_pw as u32 + 40).max(bubble::BUBBLE_WIDTH as u32);
    let win_h = max_ph as u32 + bubble::BUBBLE_HEIGHT as u32 + 60; // More vertical space
    let pet_off_y = (win_h - max_ph as u32) as f64 - 15.0; // Stay slightly above bottom

    let mut pet = Pet::new(animation_map, (max_pw as f64, max_ph as f64));
    pet.state = PetState::Move;

    let window = Rc::new(
        WindowBuilder::new()
            .with_title("Ameath")
            .with_inner_size(winit::dpi::PhysicalSize::new(win_w, win_h))
            .with_decorations(false)
            .with_transparent(true)
            .with_visible(true)
            .with_skip_taskbar(true)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .build(&event_loop)
            .unwrap(),
    );

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
    {
        use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
        if let RawWindowHandle::Win32(handle) = window.raw_window_handle() {
            let hwnd = HWND(handle.hwnd as isize);
            unsafe {
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                let _ = SetWindowLongW(
                    hwnd,
                    GWL_EXSTYLE,
                    ex_style
                        | (WS_EX_LAYERED.0 as i32)
                        | (WS_EX_TOOLWINDOW.0 as i32)
                        | (WS_EX_TOPMOST.0 as i32),
                );
                let style = windows::Win32::UI::WindowsAndMessaging::WS_POPUP.0
                    | windows::Win32::UI::WindowsAndMessaging::WS_VISIBLE.0;
                let _ = SetWindowLongW(hwnd, GWL_STYLE, style as i32);
            }
        }
    }

    let mut bubble_manager = bubble::SpeechBubble::new();
    let mut menu_manager = menu::QuickMenu::new();
    let quotes = vec![
        "哎呀，被发现了！😆",
        "别戳我啦~",
        "今天天气真不错呢☀️",
        "要做点什么好呢？",
        "呼呼……💤",
        "你好呀，主人！✨",
        "在这里可以看到全世界哦~",
        "你会一直陪着我对吧？❤️",
        "肚子有点饿了……🍬",
        "在努力工作吗？加油！💪",
    ];

    let pet_off_x = 20.0; // Fixed left offset
    let mut last_update = Some(Instant::now());
    let mut last_cursor_pos: Option<PhysicalPosition<f64>> = None;

    // Click detection
    let mut click_start_time: Option<Instant> = None;
    let mut click_start_pos: Option<(f64, f64)> = None;
    let mut settings_cursor_pos: Option<PhysicalPosition<f64>> = None;

    let mut settings_win: Option<SettingsWindow> = None;
    let mut menu_visible_timer: Option<Instant> = None;
    let mut pet_off_x = 20.0;
    let mut pet_off_y = 50.0;

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            match event {
                Event::WindowEvent { event, window_id } => {
                    if window_id == window.id() {
                        match event {
                            WindowEvent::CloseRequested => elwt.exit(),
                            WindowEvent::RedrawRequested => {
                                let draw_scale = pet.scale.max(0.5); // Ensure safe scale
                                let (cur_pw, cur_ph) = pet.get_scaled_size();

                                // Update Menu Layout
                                menu_manager.update_layout(draw_scale);

                                // Menu visibility logic
                                let menu_w = if menu_manager.visible || menu_manager.opacity > 0.0 {
                                    menu_manager.menu_width as u32
                                } else {
                                    0
                                };

                                let bubble_w = if bubble_manager.is_visible() {
                                    bubble::BUBBLE_WIDTH as u32
                                } else {
                                    0
                                };
                                let bubble_h = if bubble_manager.is_visible() {
                                    bubble::BUBBLE_HEIGHT as u32
                                } else {
                                    0
                                };

                                // Layout Strategy: Stack from Bottom
                                let base_padding_bottom = 20u32;
                                let padding_gap = 10u32;

                                let menu_h = if menu_w > 0 {
                                    menu_manager.menu_height as u32
                                } else {
                                    0
                                };

                                // Calculate the padding needed at the bottom to fit the menu
                                // The menu starts at pet_off_y (pet top) and goes DOWN.
                                // We need (cur_ph + padding_bottom) >= menu_h to avoid truncation.
                                let padding_bottom = if menu_h > cur_ph as u32 {
                                    base_padding_bottom.max(menu_h - cur_ph as u32)
                                } else {
                                    base_padding_bottom
                                };

                                // Total content height in the pet column
                                let pet_col_h = cur_ph as u32
                                    + padding_bottom
                                    + if bubble_h > 0 {
                                        bubble_h + padding_gap
                                    } else {
                                        0
                                    };

                                let total_h = pet_col_h + 40;
                                let win_h = total_h;
                                let win_w = (cur_pw as u32 + 20 + menu_w + 40).max(bubble_w + 40);

                                // Update window size if changed
                                let current_size = window.inner_size();
                                if current_size.width != win_w || current_size.height != win_h {
                                    // PREVENT JUMP: Maintain pet screen position
                                    let old_off_x = pet_off_x;
                                    let old_off_y = pet_off_y;

                                    let new_off_x = 20.0;
                                    let new_off_y = (win_h as f64 - cur_ph - padding_bottom as f64);

                                    if let Ok(win_pos) = window.outer_position() {
                                        let screen_x = win_pos.x as f64 + old_off_x;
                                        let screen_y = win_pos.y as f64 + old_off_y;

                                        let _ = window.request_inner_size(
                                            winit::dpi::PhysicalSize::new(win_w, win_h),
                                        );
                                        // Update position to keep pet at same screen coordinates
                                        window.set_outer_position(PhysicalPosition::new(
                                            (screen_x - new_off_x) as i32,
                                            (screen_y - new_off_y) as i32,
                                        ));
                                    }

                                    pet_off_x = new_off_x;
                                    pet_off_y = new_off_y;
                                } else {
                                    pet_off_x = 20.0;
                                    pet_off_y = (win_h as f64 - cur_ph - padding_bottom as f64);
                                }

                                // Bubble is above pet
                                let bubble_y = pet_off_y - padding_gap as f64 - bubble_h as f64;

                                // Menu is to right of pet, aligned with pet top
                                let menu_x = (pet_off_x + cur_pw + 10.0) as i32;
                                let menu_y = pet_off_y as i32;

                                let mut composite_data =
                                    vec![0u8; win_w as usize * win_h as usize * 4usize];

                                // 1. Draw Pet
                                let frame = pet.current_frame();
                                for y in 0..(cur_ph as u32) {
                                    for x in 0..(cur_pw as u32) {
                                        let src_x = (x as f32 / draw_scale) as u32;
                                        let src_y = (y as f32 / draw_scale) as u32;
                                        if src_x >= frame.width as u32
                                            || src_y >= frame.height as u32
                                        {
                                            continue;
                                        }

                                        let src_idx = (src_y as usize * frame.width as usize
                                            + src_x as usize)
                                            * 4usize;
                                        let dest_x = (x as f64 + pet_off_x) as u32;
                                        let dest_y = (y as f64 + pet_off_y) as u32;

                                        if dest_x < win_w && dest_y < win_h {
                                            let dest_idx = (dest_y as usize * win_w as usize
                                                + dest_x as usize)
                                                * 4usize;
                                            let alpha = frame.data[src_idx + 3];
                                            if alpha > 0 {
                                                composite_data[dest_idx] = frame.data[src_idx];
                                                composite_data[dest_idx + 1] =
                                                    frame.data[src_idx + 1];
                                                composite_data[dest_idx + 2] =
                                                    frame.data[src_idx + 2];
                                                composite_data[dest_idx + 3] = alpha;
                                            }
                                        }
                                    }
                                }

                                // 2. Draw Bubble
                                if bubble_manager.is_visible() {
                                    let mut bubble_buf = vec![
                                        0u8;
                                        (bubble::BUBBLE_WIDTH * bubble::BUBBLE_HEIGHT * 4)
                                            as usize
                                    ];
                                    bubble_manager.render_to_buffer(bubble_buf.as_mut_ptr());

                                    let bx = (win_w as i32 - bubble::BUBBLE_WIDTH) / 2;
                                    let by = bubble_y as i32;

                                    if by >= 0 {
                                        for y in 0..bubble::BUBBLE_HEIGHT as usize {
                                            for x in 0..bubble::BUBBLE_WIDTH as usize {
                                                let src_idx = (y * bubble::BUBBLE_WIDTH as usize
                                                    + x)
                                                    * 4usize;
                                                let dest_x = bx as usize + x;
                                                let dest_y = by as usize + y;
                                                if dest_x < win_w as usize
                                                    && dest_y < win_h as usize
                                                {
                                                    let dest_idx =
                                                        (dest_y * win_w as usize + dest_x) * 4usize;
                                                    let alpha =
                                                        bubble_buf[src_idx + 3] as f32 / 255.0;
                                                    if alpha > 0.0 {
                                                        composite_data[dest_idx] =
                                                            bubble_buf[src_idx];
                                                        composite_data[dest_idx + 1] =
                                                            bubble_buf[src_idx + 1];
                                                        composite_data[dest_idx + 2] =
                                                            bubble_buf[src_idx + 2];
                                                        composite_data[dest_idx + 3] =
                                                            bubble_buf[src_idx + 3];
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
                                    use raw_window_handle::{HasRawWindowHandle, RawWindowHandle};
                                    if let RawWindowHandle::Win32(handle) =
                                        window.raw_window_handle()
                                    {
                                        let hwnd = HWND(handle.hwnd as isize);
                                        unsafe {
                                            render::update_layered_window_scaled(
                                                hwnd,
                                                &composite_data,
                                                win_w as i32,
                                                win_h as i32,
                                            );
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
                                                    // Adjust global_pos to be relative to the pet's top-left corner
                                                    let global_pos = (
                                                        win_pos.x as f64 + pos.x - pet_off_x,
                                                        win_pos.y as f64 + pos.y - pet_off_y,
                                                    );
                                                    // Don't call start_drag yet, just record start
                                                    click_start_time = Some(Instant::now());
                                                    click_start_pos = Some(global_pos);
                                                }
                                            }
                                        }
                                        ElementState::Released => {
                                            let mut is_click = false;
                                            if let (Some(start_time), Some(start_pos)) =
                                                (click_start_time, click_start_pos)
                                            {
                                                if start_time.elapsed() < Duration::from_millis(200)
                                                {
                                                    if let Some(pos) = last_cursor_pos {
                                                        if let Ok(win_pos) = window.outer_position()
                                                        {
                                                            let global_x = win_pos.x as f64 + pos.x
                                                                - pet_off_x;
                                                            let global_y = win_pos.y as f64 + pos.y
                                                                - pet_off_y;
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
                                                let _now = Instant::now();

                                                // Single Click handling for Menu
                                                if menu_manager.visible {
                                                    if let Some(pos) = last_cursor_pos {
                                                        let (cur_pw, _cur_ph) =
                                                            pet.get_scaled_size();
                                                        let menu_x =
                                                            (pet_off_x + cur_pw + 10.0) as i32;
                                                        let menu_y = pet_off_y as i32;

                                                        if let Some(action) = menu_manager
                                                            .check_hit(pos.x, pos.y, menu_x, menu_y)
                                                        {
                                                            match action.as_str() {
                                                                "chat" => {
                                                                    bubble_manager.show(
                                                                        "AI 对话功能即将上线！🤖",
                                                                        Duration::from_secs(3),
                                                                    );
                                                                }
                                                                "settings" => {
                                                                    if settings_win.is_none() {
                                                                        let sw =
                                                                            SettingsWindow::new(
                                                                                elwt,
                                                                            );
                                                                        sw.request_redraw();
                                                                        settings_win = Some(sw);
                                                                    } else {
                                                                        if let Some(sw) =
                                                                            &settings_win
                                                                        {
                                                                            sw.focus();
                                                                        }
                                                                    }
                                                                }
                                                                "pomodoro" => {
                                                                    bubble_manager.show(
                                                                        "开始 25 分钟专注！🍅",
                                                                        Duration::from_secs(3),
                                                                    );
                                                                    // TODO: Start timer
                                                                }
                                                                "music" => {
                                                                    bubble_manager.show(
                                                                        "音乐播放中... 🎵",
                                                                        Duration::from_secs(3),
                                                                    );
                                                                }
                                                                "exit" => elwt.exit(),
                                                                _ => {}
                                                            }
                                                        }
                                                    }
                                                }

                                                // Random Interaction
                                                if !is_click {
                                                    // Drag ended
                                                    pet.end_drag();
                                                } else {
                                                    // If not clicking menu (and menu checking didn't consume it?), play quote
                                                    // Logic: if menu hit, we handled it. If not, play quote?
                                                    // Simplified:
                                                    if let Some(&quote) =
                                                        rand::seq::SliceRandom::choose(
                                                            &quotes[..],
                                                            &mut rand::thread_rng(),
                                                        )
                                                    {
                                                        // Only show quote if we didn't just click a menu button?
                                                        // For now, let's allow overlapping interactions or refine later.
                                                        bubble_manager
                                                            .show(quote, Duration::from_secs(4));
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

                                    if pet.state == PetState::Drag {
                                        pet.update_drag((global_x, global_y));
                                    } else if pet.state == PetState::Clingy {
                                        pet.follow_mouse((global_x, global_y));
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
                                WindowEvent::CloseRequested => {
                                    settings_win = None;
                                }
                                WindowEvent::MouseInput {
                                    state: ElementState::Pressed,
                                    button: MouseButton::Left,
                                    ..
                                } => {
                                    if let Some(pos) = settings_cursor_pos {
                                        let action = sw.handle_click(pos.x, pos.y);
                                        match action {
                                            settings_window::SettingsAction::SetScale(s) => {
                                                pet.scale = s;
                                                sw.request_redraw();
                                                window.request_redraw();
                                            }
                                            settings_window::SettingsAction::SetMode(m) => {
                                                if pet.behavior_mode == BehaviorMode::Clingy
                                                    && m != BehaviorMode::Clingy
                                                {
                                                    pet.state = PetState::Idle;
                                                }
                                                pet.behavior_mode = m;
                                                sw.request_redraw();
                                            }
                                            settings_window::SettingsAction::None => {}
                                        }
                                    }
                                }
                                WindowEvent::CursorMoved { position, .. } => {
                                    settings_cursor_pos = Some(position);
                                }
                                WindowEvent::RedrawRequested => {
                                    let mode_str = match pet.behavior_mode {
                                        BehaviorMode::Quiet => "Quiet",
                                        BehaviorMode::Active => "Active",
                                        BehaviorMode::Clingy => "Clingy",
                                    };
                                    sw.redraw(pet.scale, mode_str);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Event::AboutToWait => {
                    let mut is_hovered = false;
                    #[cfg(target_os = "windows")]
                    unsafe {
                        use windows::Win32::Foundation::POINT;
                        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
                        let mut pt = POINT::default();
                        if GetCursorPos(&mut pt).is_ok() {
                            let mouse_x = pt.x as f64;
                            let mouse_y = pt.y as f64;

                            // Clingy Logic
                            if pet.state == PetState::Clingy
                                || pet.behavior_mode == BehaviorMode::Clingy
                            {
                                pet.follow_mouse((mouse_x, mouse_y));
                            }

                            // Hover Logic for Menu
                            // Pet Rect on Screen
                            let pet_screen_x = pet.position.0;
                            let pet_screen_y = pet.position.1;
                            let (sys_pw, sys_ph) = pet.get_scaled_size();

                            // Menu Rect (Dynamic)
                            let menu_on_right_x = pet_screen_x + sys_pw + 10.0;
                            let menu_w = menu_manager.menu_width as f64;
                            let menu_h = menu_manager.menu_height as f64;

                            // Check if mouse is over pet OR menu area
                            let over_pet = mouse_x >= pet_screen_x
                                && mouse_x <= pet_screen_x + sys_pw
                                && mouse_y >= pet_screen_y
                                && mouse_y <= pet_screen_y + sys_ph;

                            let over_menu = mouse_x >= menu_on_right_x
                                && mouse_x <= menu_on_right_x + menu_w
                                && mouse_y >= pet_screen_y
                                && mouse_y <= pet_screen_y + menu_h;

                            if over_pet || over_menu {
                                is_hovered = true;
                                menu_manager.visible = true;
                                menu_manager.opacity = (menu_manager.opacity + 0.1).min(1.0);
                                menu_visible_timer = Some(Instant::now());
                            } else {
                                let should_fade = match menu_visible_timer {
                                    Some(t) => t.elapsed() > Duration::from_secs(5),
                                    None => true,
                                };

                                if should_fade {
                                    menu_manager.opacity = (menu_manager.opacity - 0.05).max(0.0);
                                    if menu_manager.opacity <= 0.0 {
                                        menu_manager.visible = false;
                                        menu_visible_timer = None;
                                    }
                                }
                            }
                        }
                    }

                    if let Some(elapsed) = last_update.map(|t| t.elapsed().as_secs_f64()) {
                        pet.update_state(elapsed, is_hovered);
                        window.set_outer_position(PhysicalPosition::new(
                            (pet.position.0 - pet_off_x) as i32,
                            (pet.position.1 - pet_off_y) as i32,
                        ));
                    }
                    last_update = Some(Instant::now());
                    window.request_redraw();
                }
                _ => {}
            }
        })
        .unwrap();
}
