mod anim;
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
    window::WindowBuilder,
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowBuilderExtWindows;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HWND;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WS_EX_LAYERED,
};

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let screen_size = (1920.0, 1080.0); // Default estimate

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

    let mut pet = Pet::new(animation_map, screen_size);
    pet.state = PetState::Move; // Start moving

    let window = Rc::new(
        WindowBuilder::new()
            .with_title("Ameath Rust")
            .with_inner_size(winit::dpi::LogicalSize::new(
                pet.window_size.0,
                pet.window_size.1,
            ))
            .with_decorations(false)
            .with_transparent(true)
            .with_skip_taskbar(true)
            .build(&event_loop)
            .unwrap(),
    );

    // Determine refresh rate
    let monitor = window.current_monitor();
    let refresh_rate_millihertz = monitor
        .and_then(|m| m.refresh_rate_millihertz())
        .unwrap_or(60000);
    let fps = refresh_rate_millihertz as f64 / 1000.0;
    let frame_delay = Duration::from_secs_f64(1.0 / fps);

    if let Some(monitor) = window.current_monitor() {
        let size = monitor.size();
        pet.screen_size = (size.width as f64, size.height as f64);
    }

    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                let hwnd = HWND(handle.hwnd.get());
                unsafe {
                    let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
                    let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_LAYERED.0 as i32));
                }
            }
        }
    }

    let mut last_physics_update = Instant::now();
    let mut last_cursor_pos: Option<PhysicalPosition<f64>> = None;

    event_loop
        .run(move |event, elwt| {
            let now = Instant::now();

            // Physics Loop (Dynamic Timestep)
            let time_since_last_update = now.duration_since(last_physics_update);
            if time_since_last_update >= frame_delay {
                let dt = time_since_last_update.as_secs_f64();
                pet.update_state(dt);

                window.set_outer_position(PhysicalPosition::new(
                    pet.position.0 as i32,
                    pet.position.1 as i32,
                ));

                window.request_redraw();
                last_physics_update = now;
            }

            // Control Flow
            let next_update = last_physics_update + frame_delay;
            elwt.set_control_flow(ControlFlow::WaitUntil(next_update));

            match event {
                Event::WindowEvent { event, window_id } if window_id == window.id() => {
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::RedrawRequested => {
                            let frame = pet.current_frame();
                            #[cfg(target_os = "windows")]
                            {
                                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                                if let Ok(handle) = window.window_handle() {
                                    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                                        let hwnd = HWND(handle.hwnd.get());
                                        unsafe {
                                            render::update_layered_window(hwnd, frame);
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
                                                let global_x = win_pos.x as f64 + pos.x;
                                                let global_y = win_pos.y as f64 + pos.y;
                                                let global_pos = (global_x, global_y);

                                                pet.start_drag(global_pos);
                                            }
                                        }
                                    }
                                    ElementState::Released => {
                                        pet.end_drag();
                                    }
                                }
                            }
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            last_cursor_pos = Some(position);
                            if let Ok(win_pos) = window.outer_position() {
                                let global_x = win_pos.x as f64 + position.x;
                                let global_y = win_pos.y as f64 + position.y;

                                if pet.state == PetState::Drag {
                                    pet.update_drag((global_x, global_y));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        })
        .unwrap();
}
