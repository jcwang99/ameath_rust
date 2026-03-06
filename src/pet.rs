use crate::types::{BehaviorMode, PetState, PreprocessedFrame};
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const SPEED_PPS: f64 = 150.0;

pub struct Pet {
    pub position: (f64, f64),
    pub velocity: (f64, f64),
    pub state: PetState,
    pub window_size: (f64, f64),
    pub screen_size: (f64, f64),

    // Animation
    pub animations: HashMap<PetState, Vec<Vec<PreprocessedFrame>>>,
    pub current_anim_variant: usize,
    pub current_frame_idx: usize,
    pub last_frame_time: Instant,
    pub facing_right: bool,

    // State Machine
    pub timer: Instant,
    pub state_duration: Duration,
    pub target_position: Option<(f64, f64)>,

    // Drag
    pub drag_start_offset: Option<(f64, f64)>,

    // Layout
    pub scale: f32,
    pub behavior_mode: crate::types::BehaviorMode,
}

impl Pet {
    pub fn new(
        animations: HashMap<PetState, Vec<Vec<PreprocessedFrame>>>,
        max_size: (f64, f64),
    ) -> Self {
        Self {
            position: (200.0, 200.0),
            velocity: (0.0, 0.0),
            state: PetState::Idle,
            window_size: max_size,
            screen_size: (1920.0, 1080.0), // Default, will be updated
            animations,
            current_anim_variant: 0,
            current_frame_idx: 0,
            last_frame_time: Instant::now(),
            facing_right: true,
            timer: Instant::now(),
            state_duration: Duration::from_secs(2),
            target_position: None,
            drag_start_offset: None,
            scale: 1.0,
            behavior_mode: crate::types::BehaviorMode::Active,
        }
    }

    pub fn get_scaled_size(&self) -> (f64, f64) {
        (
            self.window_size.0 * self.scale as f64,
            self.window_size.1 * self.scale as f64,
        )
    }

    pub fn start_drag(&mut self, mouse_pos: (f64, f64)) {
        self.state = PetState::Drag;
        self.drag_start_offset =
            Some((mouse_pos.0 - self.position.0, mouse_pos.1 - self.position.1));
        self.velocity = (0.0, 0.0);
    }

    pub fn update_drag(&mut self, mouse_pos: (f64, f64)) {
        if let Some(offset) = self.drag_start_offset {
            self.position.0 = mouse_pos.0 - offset.0;
            self.position.1 = mouse_pos.1 - offset.1;
        }
    }

    pub fn end_drag(&mut self) {
        if self.state == PetState::Drag {
            self.state = PetState::Idle;
            self.drag_start_offset = None;
            self.timer = Instant::now();
            self.state_duration = Duration::from_secs(2);
        }
    }

    pub fn update_state(&mut self, dt: f64, is_paused: bool) {
        // ALWAYS advance animation, even if logic/movement is paused
        self.advance_animation();

        if self.state == PetState::Drag {
            return;
        }

        if is_paused {
            // Logic/movement is skipped, but animation already advanced above
            return;
        }

        if self.timer.elapsed() >= self.state_duration {
            match self.state {
                PetState::Idle => {
                    if self.behavior_mode == BehaviorMode::Clingy {
                        self.state = PetState::Clingy;
                        self.current_anim_variant = 0;
                    } else if self.behavior_mode == BehaviorMode::Static {
                        self.state = PetState::Idle;
                        let count = self.animations[&PetState::Idle].len();
                        if count > 0 {
                            self.current_anim_variant = rand::thread_rng().gen_range(0..count);
                        }
                    } else {
                        self.state = PetState::Move;
                        self.current_anim_variant = 0;
                    }
                    self.current_frame_idx = 0;
                    self.last_frame_time = Instant::now();
                    self.timer = Instant::now();

                    self.state_duration = match self.behavior_mode {
                        BehaviorMode::Static => {
                            Duration::from_secs(rand::thread_rng().gen_range(3..7))
                        }
                        BehaviorMode::Quiet => {
                            Duration::from_secs(rand::thread_rng().gen_range(2..4))
                        }
                        BehaviorMode::Active => {
                            Duration::from_secs(rand::thread_rng().gen_range(5..10))
                        }
                        BehaviorMode::Clingy => Duration::from_secs(2),
                    };

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
                    self.state = PetState::Idle;
                    if self.behavior_mode == BehaviorMode::Static {
                        let count = self.animations[&PetState::Idle].len();
                        if count > 0 {
                            self.current_anim_variant = rand::thread_rng().gen_range(0..count);
                        }
                    } else {
                        self.current_anim_variant = 0;
                    }
                    self.timer = Instant::now();
                    self.current_frame_idx = 0;
                    self.last_frame_time = Instant::now();

                    self.state_duration = match self.behavior_mode {
                        BehaviorMode::Static => {
                            Duration::from_secs(rand::thread_rng().gen_range(3..7))
                        }
                        BehaviorMode::Quiet => {
                            Duration::from_secs(rand::thread_rng().gen_range(5..10))
                        }
                        BehaviorMode::Active => {
                            Duration::from_secs(rand::thread_rng().gen_range(1..3))
                        }
                        BehaviorMode::Clingy => Duration::from_secs(2),
                    };

                    self.velocity = (0.0, 0.0);
                    self.target_position = None;

                    let count = self.animations[&PetState::Idle].len();
                    if count > 0 {
                        self.current_anim_variant = rand::thread_rng().gen_range(0..count);
                    } else {
                        self.current_anim_variant = 0;
                    }
                }
                PetState::Clingy => {
                    self.timer = Instant::now();
                }
                _ => {}
            }
        }

        // Movement Logic
        if self.state == PetState::Move || self.state == PetState::Clingy {
            if self.state == PetState::Clingy {
                if let Some(target) = self.target_position {
                    let dx = target.0 - self.position.0;
                    let dy = target.1 - self.position.1;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist > 5.0 {
                        self.velocity.0 = (dx / dist) * SPEED_PPS;
                        self.velocity.1 = (dy / dist) * SPEED_PPS;
                    } else {
                        self.velocity = (0.0, 0.0);
                    }
                }
            }

            self.position.0 += self.velocity.0 * dt;
            self.position.1 += self.velocity.1 * dt;

            if self.velocity.0 > 0.1 {
                if !self.facing_right {
                    self.facing_right = true;
                    self.current_frame_idx = 0;
                    self.last_frame_time = Instant::now();
                }
            } else if self.velocity.0 < -0.1 {
                if self.facing_right {
                    self.facing_right = false;
                    self.current_frame_idx = 0;
                    self.last_frame_time = Instant::now();
                }
            }

            if let Some((tx, ty)) = self.target_position {
                let dx = tx - self.position.0;
                let dy = ty - self.position.1;
                let dist_sq = dx * dx + dy * dy;

                if self.state == PetState::Move && dist_sq < 100.0 {
                    self.timer = Instant::now() - self.state_duration;
                } else if self.state == PetState::Clingy && dist_sq < 25.0 {
                    self.velocity = (0.0, 0.0);
                }
            }
        }

        let (w, h) = self.get_scaled_size();
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

    pub fn advance_animation(&mut self) {
        let variants = &self.animations[&self.state];
        if variants.is_empty() {
            return;
        }

        let variant_idx = self.current_anim_variant.min(variants.len() - 1);
        let frames = &variants[variant_idx];
        if frames.is_empty() {
            return;
        }

        let now = Instant::now();
        let mut frame = &frames[self.current_frame_idx % frames.len()];

        // Catch up with frames (handles skips if dt was large or redraws slow)
        while now.duration_since(self.last_frame_time) >= frame.delay {
            self.last_frame_time += frame.delay;
            self.current_frame_idx = (self.current_frame_idx + 1) % frames.len();
            frame = &frames[self.current_frame_idx % frames.len()];

            // Safety break to prevent infinite loop if delay is 0
            if frame.delay.as_millis() == 0 {
                break;
            }
        }
    }

    pub fn follow_mouse(&mut self, mouse_pos: (f64, f64)) {
        self.state = PetState::Clingy;
        self.target_position = Some(mouse_pos);

        let dx = mouse_pos.0 - self.position.0;
        let dy = mouse_pos.1 - self.position.1;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist > 5.0 {
            self.velocity.0 = (dx / dist) * SPEED_PPS;
            self.velocity.1 = (dy / dist) * SPEED_PPS;
        } else {
            self.velocity = (0.0, 0.0);
        }
    }

    pub fn current_frame(&mut self) -> &PreprocessedFrame {
        let variants = &self.animations[&self.state];

        let variant_idx = if self.current_anim_variant < variants.len() {
            self.current_anim_variant
        } else {
            0
        };
        let frames = &variants[variant_idx];
        &frames[self.current_frame_idx % frames.len()]
    }

    pub fn next_frame_at(&self) -> Instant {
        let variants = &self
            .animations
            .get(&self.state)
            .unwrap_or(&self.animations[&PetState::Idle]);
        if variants.is_empty() {
            return Instant::now() + Duration::from_millis(16);
        }
        let variant_idx = self.current_anim_variant.min(variants.len() - 1);
        let frames = &variants[variant_idx];
        if frames.is_empty() {
            return Instant::now() + Duration::from_millis(16);
        }
        let frame = &frames[self.current_frame_idx % frames.len()];
        self.last_frame_time + frame.delay
    }

    pub fn check_hit(&mut self, mouse_x: f64, mouse_y: f64) -> bool {
        let (scaled_w, scaled_h) = self.get_scaled_size();
        let pos_x = self.position.0;
        let pos_y = self.position.1;
        let p_scale = self.scale as f64;
        let p_facing_right = self.facing_right;

        let local_x = mouse_x - pos_x;
        let local_y = mouse_y - pos_y;

        if local_x >= 0.0 && local_x < scaled_w && local_y >= 0.0 && local_y < scaled_h {
            let px = (local_x / p_scale) as usize;
            let py = (local_y / p_scale) as usize;

            let frame = self.current_frame();
            if px < frame.width as usize && py < frame.height as usize {
                let actual_x = if p_facing_right {
                    px
                } else {
                    frame.width as usize - 1 - px
                };
                let (start_x, end_x) = frame.opaque_rows[py];
                return actual_x >= start_x && actual_x < end_x;
            }
        }
        false
    }

    pub fn hit_test_bubble(
        &self,
        mouse_x: f64,
        mouse_y: f64,
        bubble_rect: (i32, i32, i32, i32),
    ) -> bool {
        let (bx, by, bw, bh) = bubble_rect;
        mouse_x >= bx as f64
            && mouse_x <= (bx + bw) as f64
            && mouse_y >= by as f64
            && mouse_y <= (by + bh) as f64
    }
}
