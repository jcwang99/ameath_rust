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
    // Stores (RightFrames, LeftFrames)
    pub animations: HashMap<PetState, (Vec<Vec<PreprocessedFrame>>, Vec<Vec<PreprocessedFrame>>)>,
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
        animations: HashMap<PetState, (Vec<Vec<PreprocessedFrame>>, Vec<Vec<PreprocessedFrame>>)>,
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
        if self.state == PetState::Drag {
            return;
        }

        if is_paused {
            // Animation continues, but position/velocity logic is skipped
            return;
        }

        if self.timer.elapsed() >= self.state_duration {
            match self.state {
                PetState::Idle => {
                    if self.behavior_mode == BehaviorMode::Clingy {
                        self.state = PetState::Clingy;
                    } else {
                        self.state = PetState::Move;
                    }
                    self.current_anim_variant = 0;
                    self.timer = Instant::now();

                    self.state_duration = match self.behavior_mode {
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
                    self.timer = Instant::now();

                    self.state_duration = match self.behavior_mode {
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

                    let count = self.animations[&PetState::Idle].0.len();
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
                self.facing_right = true;
            } else if self.velocity.0 < -0.1 {
                self.facing_right = false;
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
        let (right_variants, left_variants) = &self.animations[&self.state];

        let variants = if self.facing_right {
            right_variants
        } else {
            left_variants
        };

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
}
