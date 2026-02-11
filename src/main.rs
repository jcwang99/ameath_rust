use image::codecs::gif::GifDecoder;
use image::AnimationDecoder;

use rand::Rng;
use std::collections::HashMap;
use std::fs::File;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::{
    dpi::PhysicalPosition,
    event::{ElementState, Event, MouseButton, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[cfg(target_os = "windows")]
use winit::platform::windows::WindowBuilderExtWindows;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS,
    HDC,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongW, SetWindowLongW, UpdateLayeredWindow, GWL_EXSTYLE, ULW_ALPHA, WS_EX_LAYERED,
};

// Physics Constants
const SPEED_PPS: f64 = 150.0; // Pixels per second
const GRAVITY: f64 = 1000.0;
const JUMP_SPEED: f64 = -400.0;

#[derive(Clone)]
struct PreprocessedFrame {
    width: i32,
    height: i32,
    data: Vec<u8>,
    delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PetState {
    Idle,
    Move,
    Drag,
    Interact,
}

fn flip_frame_horizontal(frame: &PreprocessedFrame) -> PreprocessedFrame {
    let mut new_data = frame.data.clone();
    let width = frame.width as usize;
    let height = frame.height as usize;
    let bpp = 4; // BGRA

    for y in 0..height {
        let row_start = y * width * bpp;
        for x in 0..(width / 2) {
            let left_idx = row_start + x * bpp;
            let right_idx = row_start + (width - 1 - x) * bpp;

            // Swap pixels (4 bytes)
            for i in 0..4 {
                let tmp = new_data[left_idx + i];
                new_data[left_idx + i] = new_data[right_idx + i];
                new_data[right_idx + i] = tmp;
            }
        }
    }

    PreprocessedFrame {
        width: frame.width,
        height: frame.height,
        data: new_data,
        delay: frame.delay,
    }
}

struct Pet {
    position: (f64, f64),
    velocity: (f64, f64),
    state: PetState,
    window_size: (f64, f64),
    screen_size: (f64, f64),

    // Animation
    // Stores (RightFrames, LeftFrames)
    animations: HashMap<PetState, (Vec<Vec<PreprocessedFrame>>, Vec<Vec<PreprocessedFrame>>)>,
    current_anim_variant: usize,
    current_frame_idx: usize,
    last_frame_time: Instant,
    facing_right: bool,

    // State Machine
    timer: Instant,
    state_duration: Duration,
    target_position: Option<(f64, f64)>,

    // Drag
    drag_start_offset: Option<(f64, f64)>,
}

impl Pet {
    fn new(
        animations: HashMap<PetState, (Vec<Vec<PreprocessedFrame>>, Vec<Vec<PreprocessedFrame>>)>,
        screen_size: (f64, f64),
    ) -> Self {
        // Use the first variant of Idle for initial size
        let first_frame = &animations[&PetState::Idle].0[0][0];
        let window_size = (first_frame.width as f64, first_frame.height as f64);

        Self {
            position: (200.0, 200.0),
            velocity: (0.0, 0.0),
            state: PetState::Idle,
            window_size,
            screen_size,
            animations,
            current_anim_variant: 0,
            current_frame_idx: 0,
            last_frame_time: Instant::now(),
            facing_right: true, // Default to right
            timer: Instant::now(),
            state_duration: Duration::from_secs(2),
            target_position: None,
            drag_start_offset: None,
        }
    }

    fn interact(&mut self) {
        if self.state == PetState::Drag {
            return;
        }
        self.state = PetState::Interact;
        self.velocity = (0.0, JUMP_SPEED); // Jump up
        self.state_duration = Duration::from_millis(800); // Max interaction time if logic fails
        self.timer = Instant::now();
        self.current_anim_variant = 0; // Use first variant (usually Drag frame 0)
    }

    fn update_state(&mut self, dt: f64) {
        if self.state == PetState::Drag {
            return;
        }

        if self.state == PetState::Interact {
            // Apply Gravity
            self.velocity.1 += GRAVITY * dt;
            self.position.0 += self.velocity.0 * dt;
            self.position.1 += self.velocity.1 * dt;

            // Ground collision
            let (_, h) = self.window_size;
            let (_, sh) = self.screen_size;

            if self.position.1 + h >= sh {
                self.position.1 = sh - h;
                self.velocity = (0.0, 0.0);
                // Landed, go back to Idle
                self.state = PetState::Idle;
                self.timer = Instant::now();
                self.state_duration = Duration::from_secs(2);
            }
            return;
        }

        // State Machine Transitions
        if self.timer.elapsed() >= self.state_duration {
            match self.state {
                PetState::Idle => {
                    // Switch to Move
                    self.state = PetState::Move;
                    self.current_anim_variant = 0;
                    self.timer = Instant::now();
                    self.state_duration = Duration::from_secs(rand::thread_rng().gen_range(3..7));

                    // Pick random target
                    let max_x = self.screen_size.0 - self.window_size.0;
                    let max_y = self.screen_size.1 - self.window_size.1;
                    let target_x = rand::thread_rng().gen_range(0.0..max_x.max(1.0));
                    let target_y = rand::thread_rng().gen_range(0.0..max_y.max(1.0));
                    self.target_position = Some((target_x, target_y));

                    let dx = target_x - self.position.0;
                    let dy = target_y - self.position.1;
                    let dist = (dx * dx + dy * dy).sqrt();

                    if dist > 0.0 {
                        self.velocity.0 = (dx / dist) * SPEED_PPS;
                        self.velocity.1 = (dy / dist) * SPEED_PPS;
                    } else {
                        self.velocity = (0.0, 0.0);
                    }
                }
                PetState::Move => {
                    // Switch to Idle
                    self.state = PetState::Idle;
                    self.timer = Instant::now();
                    self.state_duration = Duration::from_secs(rand::thread_rng().gen_range(2..5));
                    self.velocity = (0.0, 0.0);
                    self.target_position = None;

                    // Pick random idle animation
                    let count = self.animations[&PetState::Idle].0.len();
                    if count > 0 {
                        self.current_anim_variant = rand::thread_rng().gen_range(0..count);
                    } else {
                        self.current_anim_variant = 0;
                    }
                }
                _ => {}
            }
        }

        // Movement Logic
        if self.state == PetState::Move {
            self.position.0 += self.velocity.0 * dt;
            self.position.1 += self.velocity.1 * dt;

            // Update facing direction
            if self.velocity.0 > 0.1 {
                self.facing_right = true;
            } else if self.velocity.0 < -0.1 {
                self.facing_right = false;
            }

            if let Some((tx, ty)) = self.target_position {
                let dx = tx - self.position.0;
                let dy = ty - self.position.1;
                if dx * dx + dy * dy < 100.0 {
                    self.timer = Instant::now() - self.state_duration; // Force expire
                }
            }

            let (w, h) = self.window_size;
            let (sw, sh) = self.screen_size;

            if self.position.0 <= 0.0 {
                self.position.0 = 0.0;
                self.velocity.0 = self.velocity.0.abs();
            } else if self.position.0 + w >= sw {
                self.position.0 = sw - w;
                self.velocity.0 = -self.velocity.0.abs();
            }

            if self.position.1 <= 0.0 {
                self.position.1 = 0.0;
                self.velocity.1 = self.velocity.1.abs();
            } else if self.position.1 + h >= sh {
                self.position.1 = sh - h;
                self.velocity.1 = -self.velocity.1.abs();
            }
        }
    }

    fn current_frame(&mut self) -> &PreprocessedFrame {
        // For Interact, reuse Drag animation
        let (right_variants, left_variants) = if self.state == PetState::Interact {
            &self.animations[&PetState::Drag]
        } else {
            &self.animations[&self.state]
        };

        let variants = if self.facing_right {
            right_variants
        } else {
            left_variants
        };

        // Safety check for variant index
        let variant_idx = if self.current_anim_variant < variants.len() {
            self.current_anim_variant
        } else {
            0
        };
        let frames = &variants[variant_idx];

        let now = Instant::now();
        let frame = &frames[self.current_frame_idx % frames.len()];

        if now.duration_since(self.last_frame_time) >= frame.delay {
            self.current_frame_idx = (self.current_frame_idx + 1) % frames.len();
            self.last_frame_time = now;
        }

        &frames[self.current_frame_idx % frames.len()]
    }

    fn start_drag(&mut self, mouse_pos: (f64, f64)) {
        self.state = PetState::Drag;
        self.current_anim_variant = 0;
        self.drag_start_offset =
            Some((mouse_pos.0 - self.position.0, mouse_pos.1 - self.position.1));
        self.velocity = (0.0, 0.0);
    }

    fn end_drag(&mut self) {
        self.state = PetState::Idle;
        // Pick random idle on drop
        let count = self.animations[&PetState::Idle].0.len();
        if count > 0 {
            self.current_anim_variant = rand::thread_rng().gen_range(0..count);
        } else {
            self.current_anim_variant = 0;
        }
        self.timer = Instant::now();
        self.state_duration = Duration::from_secs(2);
        self.drag_start_offset = None;
    }

    fn update_drag(&mut self, mouse_pos: (f64, f64)) {
        if let Some((off_x, off_y)) = self.drag_start_offset {
            self.position.0 = mouse_pos.0 - off_x;
            self.position.1 = mouse_pos.1 - off_y;
        }
    }
}

#[cfg(target_os = "windows")]
unsafe fn update_layered_window(hwnd: HWND, frame: &PreprocessedFrame) {
    let screen_dc = GetDC(HWND(0));
    let mem_dc = CreateCompatibleDC(screen_dc);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: frame.width,
            biHeight: -frame.height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbitmap = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();

    let old_bitmap = SelectObject(mem_dc, hbitmap);

    // Fast copy: No loop!
    let pixel_data = std::slice::from_raw_parts_mut(bits as *mut u8, frame.data.len());
    pixel_data.copy_from_slice(&frame.data);

    let point_source = POINT { x: 0, y: 0 };
    let size_dest = SIZE {
        cx: frame.width,
        cy: frame.height,
    };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };

    UpdateLayeredWindow(
        hwnd,
        HDC::default(),
        None,
        Some(&size_dest as *const SIZE),
        mem_dc,
        Some(&point_source as *const POINT),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );

    SelectObject(mem_dc, old_bitmap);
    DeleteObject(hbitmap);
    DeleteDC(mem_dc);
    ReleaseDC(HWND(0), screen_dc);
}

fn preprocess_frames(frames: Vec<image::Frame>) -> Vec<PreprocessedFrame> {
    frames
        .into_iter()
        .map(|frame| {
            let (w, h) = frame.buffer().dimensions();
            let width = w as i32;
            let height = h as i32;
            let delay: Duration = frame.delay().into();

            let mut data = Vec::with_capacity((width * height * 4) as usize);

            for rgba in frame.buffer().pixels() {
                let r = rgba[0] as f32;
                let g = rgba[1] as f32;
                let b = rgba[2] as f32;
                let a = rgba[3] as f32;

                let alpha_factor = a / 255.0;

                // BGRA, Pre-multiplied
                data.push((b * alpha_factor) as u8);
                data.push((g * alpha_factor) as u8);
                data.push((r * alpha_factor) as u8);
                data.push(a as u8);
            }

            PreprocessedFrame {
                width,
                height,
                data,
                delay,
            }
        })
        .collect()
}

fn load_gif_processed(path: &str) -> Vec<PreprocessedFrame> {
    let file = File::open(path).expect("Failed to open GIF");
    let decoder = GifDecoder::new(file).expect("Failed to decode GIF");
    let frames = decoder
        .into_frames()
        .collect_frames()
        .expect("Failed to collect frames");
    preprocess_frames(frames)
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let screen_size = (1920.0, 1080.0); // Default estimate, winit provides monitor info

    // Load assets (Right-facing by default)
    let idle_frames_right = vec![
        load_gif_processed("assets/gifs/idle1.gif"),
        load_gif_processed("assets/gifs/idle2.gif"),
        load_gif_processed("assets/gifs/idle3.gif"),
        load_gif_processed("assets/gifs/idle4.gif"),
    ];
    let move_frames_right = vec![load_gif_processed("assets/gifs/move.gif")];
    let drag_frames_right = vec![load_gif_processed("assets/gifs/drag.gif")];

    // Helper to mirror variants (Vec<Vec<Frame>>)
    let mirror_variants = |variants: &Vec<Vec<PreprocessedFrame>>| -> Vec<Vec<PreprocessedFrame>> {
        variants
            .iter()
            .map(|variant| {
                variant
                    .iter()
                    .map(|frame| flip_frame_horizontal(frame))
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

    println!("Refresh Rate: {} Hz (Frame Delay: {:?})", fps, frame_delay);

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
                    SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | (WS_EX_LAYERED.0 as i32));
                }
            }
        }
    }

    let mut last_physics_update = Instant::now();
    let mut last_cursor_pos: Option<PhysicalPosition<f64>> = None;

    // Click detection variables
    let mut click_start_time: Option<Instant> = None;
    let mut click_start_pos: Option<(f64, f64)> = None;

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
                                            update_layered_window(hwnd, frame);
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
                                                        let global_x = win_pos.x as f64 + pos.x;
                                                        let global_y = win_pos.y as f64 + pos.y;
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
                                            pet.interact();
                                            // Ensure we don't drift after interact
                                            pet.velocity.0 = 0.0;
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
