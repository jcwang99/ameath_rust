mod anim;
mod bubble;
mod pet;
mod render;
mod types;

use pet::Pet;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};
use types::{PetState, PreprocessedFrame};
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

    let win_w = (max_pw as u32 + 20).max(bubble::BUBBLE_WIDTH as u32);
    let win_h = (max_ph as u32 + bubble::BUBBLE_HEIGHT as u32 + 20);
    let pet_off_y = (win_h - max_ph as u32) as f64 - 10.0;

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
    let fps = refresh_rate_millihertz as f64 / 1000.0;

    if let Some(monitor) = window.current_monitor() {
        let size = monitor.size();
        pet.screen_size = (size.width as f64, size.height as f64);
    }

    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                let hwnd = HWND(handle.hwnd.get() as isize);
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
    }

    let mut bubble_manager = bubble::SpeechBubble::new();
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

    let pet_off_x = (win_w as f64 - max_pw as f64) / 2.0;
    let mut last_update = Some(Instant::now());
    let mut last_cursor_pos: Option<PhysicalPosition<f64>> = None;

    // Click detection
    let mut click_start_time: Option<Instant> = None;
    let mut click_start_pos: Option<(f64, f64)> = None;

    event_loop
        .run(move |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            match event {
                Event::WindowEvent { event, window_id } if window_id == window.id() => {
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::RedrawRequested => {
                            let pet_frame = pet.current_frame();

                            // Composite Buffer
                            let mut composite_data = vec![0u8; (win_w * win_h * 4) as usize];

                            // 1. Draw Pet Frame (Dynamic sizing)
                            let pw = pet_frame.width as usize;
                            let ph = pet_frame.height as usize;

                            // Center pet horizontally in its slot
                            let px = ((win_w as usize - pw) / 2).max(0);
                            let py = pet_off_y as usize;

                            for y in 0..ph {
                                for x in 0..pw {
                                    let src_idx = (y * pw + x) * 4;
                                    let dst_idx = ((py + y) * win_w as usize + (px + x)) * 4;
                                    if dst_idx + 3 < composite_data.len()
                                        && src_idx + 3 < pet_frame.data.len()
                                    {
                                        composite_data[dst_idx] = pet_frame.data[src_idx];
                                        composite_data[dst_idx + 1] = pet_frame.data[src_idx + 1];
                                        composite_data[dst_idx + 2] = pet_frame.data[src_idx + 2];
                                        composite_data[dst_idx + 3] = pet_frame.data[src_idx + 3];
                                    }
                                }
                            }

                            // 2. Draw Bubble if visible
                            if bubble_manager.is_visible() {
                                let mut bubble_buf = vec![
                                    0u8;
                                    (bubble::BUBBLE_WIDTH * bubble::BUBBLE_HEIGHT * 4)
                                        as usize
                                ];
                                bubble_manager.render_to_buffer(bubble_buf.as_mut_ptr());

                                let bx = ((win_w as i32 - bubble::BUBBLE_WIDTH) / 2) as usize;
                                let by = 30usize; // Move bubble down from very top
                                for y in 0..bubble::BUBBLE_HEIGHT as usize {
                                    for x in 0..bubble::BUBBLE_WIDTH as usize {
                                        let src_idx = (y * bubble::BUBBLE_WIDTH as usize + x) * 4;
                                        let dst_idx = ((by + y) * win_w as usize + (bx + x)) * 4;
                                        // Simple Alpha Blending
                                        let alpha = bubble_buf[src_idx + 3] as f32 / 255.0;
                                        if alpha > 0.0 {
                                            composite_data[dst_idx] = bubble_buf[src_idx];
                                            composite_data[dst_idx + 1] = bubble_buf[src_idx + 1];
                                            composite_data[dst_idx + 2] = bubble_buf[src_idx + 2];
                                            composite_data[dst_idx + 3] = bubble_buf[src_idx + 3];
                                        }
                                    }
                                }
                            }

                            #[cfg(target_os = "windows")]
                            {
                                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                                if let Ok(handle) = window.window_handle() {
                                    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                                        let hwnd = HWND(handle.hwnd.get() as isize);
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
                                            if start_time.elapsed() < Duration::from_millis(200) {
                                                if let Some(pos) = last_cursor_pos {
                                                    if let Ok(win_pos) = window.outer_position() {
                                                        let global_x =
                                                            win_pos.x as f64 + pos.x - pet_off_x;
                                                        let global_y =
                                                            win_pos.y as f64 + pos.y - pet_off_y;
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
                                            if let Some(&quote) = rand::seq::SliceRandom::choose(
                                                &quotes[..],
                                                &mut rand::thread_rng(),
                                            ) {
                                                bubble_manager.show(quote, Duration::from_secs(4));
                                            }
                                            pet.velocity = (0.0, 0.0);
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
                                // Adjust global_x/y to be relative to the pet's top-left corner
                                let global_x = win_pos.x as f64 + position.x - pet_off_x;
                                let global_y = win_pos.y as f64 + position.y - pet_off_y;

                                if pet.state == PetState::Drag {
                                    pet.update_drag((global_x, global_y));
                                } else if let Some(start_pos) = click_start_pos {
                                    // Potentially start drag if threshold exceeded
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
                }
                Event::AboutToWait => {
                    if let Some(elapsed) = last_update.map(|t| t.elapsed().as_secs_f64()) {
                        pet.update_state(elapsed);
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
