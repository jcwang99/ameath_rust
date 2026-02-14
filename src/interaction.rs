use crate::types::AiConfig;
use chrono::Local;
use rand::Rng;
use std::time::{Duration, Instant};
use sysinfo::System;

#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

pub struct Senses {
    sys: System,
}

impl Senses {
    pub fn new() -> Self {
        Self {
            sys: System::new_all(),
        }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_all();
    }

    pub fn get_cpu_usage(&self) -> f32 {
        self.sys.global_cpu_usage()
    }

    pub fn get_memory_usage(&self) -> u64 {
        self.sys.used_memory() / 1024 / 1024 // MB
    }

    pub fn get_active_window_title(&self) -> String {
        #[cfg(target_os = "windows")]
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0 == 0 {
                return "Unknown".to_string();
            }
            let mut buf = [0u16; 512];
            let len = GetWindowTextW(hwnd, &mut buf);
            if len > 0 {
                String::from_utf16_lossy(&buf[..len as usize])
            } else {
                "Unknown".to_string()
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            "Unknown (Non-Windows)".to_string()
        }
    }

    pub fn get_context_snapshot(&mut self) -> String {
        self.refresh();
        let time = Local::now().format("%H:%M").to_string();
        let cpu = self.get_cpu_usage();
        let mem = self.get_memory_usage();
        let app = self.get_active_window_title();

        format!("Time: {time}, Active App: {app}, CPU: {cpu:.1}%, RAM: {mem}MB")
    }
}

pub struct InteractionManager {
    senses: Senses,
    last_interaction: Instant,
    config: AiConfig,
    base_interval: Duration,
}

impl InteractionManager {
    pub fn new(config: AiConfig) -> Self {
        let base_interval = Duration::from_secs(config.interaction_frequency * 60);

        Self {
            senses: Senses::new(),
            last_interaction: Instant::now(),
            config,
            base_interval,
        }
    }

    pub fn update_config(&mut self, config: AiConfig) {
        self.config = config;
        // Enforce 1 minute minimum to avoid accidental spam during typing
        self.base_interval = Duration::from_secs(self.config.interaction_frequency.max(1) * 60);
    }

    pub fn check_for_trigger(&mut self) -> Option<String> {
        if !self.config.active_interaction_enabled {
            return None;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_interaction);

        // 1. Basic Timer Check with Randomness
        if elapsed > self.base_interval {
            // Add some randomness +/- 20%
            let mut rng = rand::thread_rng();
            let random_factor: f32 = rng.gen_range(0.8..1.2);
            let threshold = self.base_interval.mul_f32(random_factor);

            if elapsed > threshold {
                self.last_interaction = now;
                let context = self.senses.get_context_snapshot();
                return Some(format!(
                    "[SYSTEM_EVENT] Routine Check. Context: {}",
                    context
                ));
            }
        }

        // 2. High Priority Events (e.g. High CPU) - Check more frequently?
        // For now, let's keep it simple. Future: Check status every 10s.

        None
    }
}
