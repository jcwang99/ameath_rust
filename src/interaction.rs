use arboard::Clipboard;
use chrono::{DateTime, Local, NaiveDate, NaiveTime, Timelike, Datelike};
use rand::Rng;
use std::time::{Duration, Instant};
use sysinfo::{Components, Disks, Networks, System};

use crate::types::{AiConfig, PersistentConfig, RemindersConfig, ScheduledItemDef, RoutinesConfig, RoutineStateConfig, ScheduleType};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct ActionScheduler {
    pub config: Arc<Mutex<RemindersConfig>>,
}

impl ActionScheduler {
    pub fn new() -> Self {
        Self {
            config: Arc::new(Mutex::new(RemindersConfig::load())),
        }
    }

    pub fn schedule(&self, minutes: u32, memo: String) -> Result<String, String> {
        let mut cfg = self.config.lock().unwrap();
        if cfg.items.len() >= 5 {
            return Err(
                "Too many active reminders (max 5). Please wait for some to trigger.".to_string(),
            );
        }

        let mins = minutes.max(1);
        let trigger_time = Local::now() + chrono::Duration::minutes(mins as i64);

        cfg.items.push(ScheduledItemDef {
            id: uuid::Uuid::new_v4().to_string(),
            time: trigger_time.to_rfc3339(),
            memo: memo.clone(),
        });

        cfg.items.sort_by(|a, b| {
            let t_a = DateTime::parse_from_rfc3339(&a.time).unwrap_or_default();
            let t_b = DateTime::parse_from_rfc3339(&b.time).unwrap_or_default();
            t_a.cmp(&t_b)
        });

        cfg.save();

        Ok(format!("Scheduled: '{}' in {} minutes.", memo, mins))
    }

    pub fn poll(&self) -> Option<String> {
        let mut cfg = self.config.lock().unwrap();
        if cfg.items.is_empty() {
            return None;
        }

        let now = Local::now();
        let target_time = DateTime::parse_from_rfc3339(&cfg.items[0].time).unwrap_or_default();
        
        if now.with_timezone(&target_time.timezone()) >= target_time {
            let item = cfg.items.remove(0);
            cfg.save();
            return Some(item.memo);
        }
        None
    }
}

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::LPARAM;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
#[cfg(target_os = "windows")]
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
#[cfg(target_os = "windows")]
use windows::Win32::System::Registry::{
    RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ,
};
#[cfg(target_os = "windows")]
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

pub struct Senses {
    sys: System,
    disks: Disks,
    networks: Networks,
    components: Components,
    clipboard: Option<Clipboard>,

    // Net rate tracking
    last_net_time: Instant,
    last_tx_total: u64,
    last_rx_total: u64,
    net_up_kbps: f64,
    net_down_kbps: f64,
}

impl Senses {
    pub fn new() -> Self {
        let sys = System::new();
        let networks = Networks::new();
        let mut total_tx = 0;
        let mut total_rx = 0;
        for (_, data) in &networks {
            total_tx += data.transmitted();
            total_rx += data.received();
        }

        Self {
            sys,
            disks: Disks::new(),
            networks,
            components: Components::new(),
            clipboard: Clipboard::new().ok(),
            last_net_time: Instant::now(),
            last_tx_total: total_tx,
            last_rx_total: total_rx,
            net_up_kbps: 0.0,
            net_down_kbps: 0.0,
        }
    }

    pub fn refresh(&mut self) {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.disks.refresh(false);
        self.networks.refresh(false);
        self.components.refresh(false);

        // Update net rates
        let now = Instant::now();
        let delta = now.duration_since(self.last_net_time).as_secs_f64();
        if delta > 0.5 {
            let mut total_tx = 0;
            let mut total_rx = 0;
            for (_, data) in &self.networks {
                total_tx += data.transmitted();
                total_rx += data.received();
            }

            let tx_diff = total_tx.saturating_sub(self.last_tx_total);
            let rx_diff = total_rx.saturating_sub(self.last_rx_total);

            self.net_up_kbps = (tx_diff as f64 / 1024.0) / delta;
            self.net_down_kbps = (rx_diff as f64 / 1024.0) / delta;

            self.last_tx_total = total_tx;
            self.last_rx_total = total_rx;
            self.last_net_time = now;
        }
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

    pub fn get_total_memory(&self) -> u64 {
        self.sys.total_memory() / 1024 / 1024 // MB
    }

    pub fn get_battery_info(&self) -> String {
        #[cfg(target_os = "windows")]
        unsafe {
            let mut status = SYSTEM_POWER_STATUS::default();
            if GetSystemPowerStatus(&mut status).is_ok() {
                let life = status.BatteryLifePercent;
                let state = if status.ACLineStatus == 1 {
                    "Charging"
                } else {
                    "On Battery"
                };
                if life == 255 {
                    return "Desktop/Unknown".to_string();
                }
                return format!("{life}% ({state})");
            }
        }
        "Unknown".to_string()
    }

    pub fn get_idle_time(&self) -> Duration {
        #[cfg(target_os = "windows")]
        unsafe {
            let mut lii = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            let res = GetLastInputInfo(&mut lii);
            // GetLastInputInfo returns BOOL (struct with .0)
            if res.as_bool() {
                // Changed from res.0 != 0 to res.as_bool()
                let tick = windows::Win32::System::SystemInformation::GetTickCount();
                let diff = tick.wrapping_sub(lii.dwTime);
                return Duration::from_millis(diff as u64);
            }
        }
        Duration::from_secs(0)
    }

    pub fn get_uptime(&self) -> String {
        let uptime_secs = System::uptime();
        let hours = uptime_secs / 3600;
        let mins = (uptime_secs % 3600) / 60;
        format!("{hours}h {mins}m")
    }

    pub fn get_theme(&self) -> String {
        #[cfg(target_os = "windows")]
        unsafe {
            let mut hkey = HKEY::default();
            let path = windows::core::w!(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"
            );
            if RegOpenKeyExW(HKEY_CURRENT_USER, path, 0, KEY_READ, &mut hkey).is_ok() {
                let mut value = 0u32;
                let mut size = std::mem::size_of::<u32>() as u32;
                let name = windows::core::w!("AppsUseLightTheme");
                if RegQueryValueExW(
                    hkey,
                    name,
                    None,
                    None,
                    Some(&mut value as *mut u32 as *mut u8),
                    Some(&mut size),
                )
                .is_ok()
                {
                    return if value == 0 {
                        "Dark".to_string()
                    } else {
                        "Light".to_string()
                    };
                }
            }
        }
        "Unknown".to_string()
    }

    pub fn get_disk_info(&self) -> String {
        if let Some(main_disk) = self.disks.iter().next() {
            let free = main_disk.available_space() / 1024 / 1024 / 1024; // GB
            let total = main_disk.total_space() / 1024 / 1024 / 1024; // GB
            let pct = (free as f64 / total as f64 * 100.0) as u32;
            return format!("{free}GB Free ({pct}%)");
        }
        "Unknown".to_string()
    }

    pub fn get_network_rates(&self) -> String {
        format!(
            "Up: {:.1}KB/s, Down: {:.1}KB/s",
            self.net_up_kbps, self.net_down_kbps
        )
    }

    pub fn get_monitor_info(&self) -> String {
        #[cfg(target_os = "windows")]
        unsafe {
            let mut monitors = Vec::new();
            unsafe extern "system" fn monitor_enum_proc(
                hmonitor: HMONITOR,
                _hdc: HDC,
                _rect: *mut windows::Win32::Foundation::RECT,
                data: LPARAM,
            ) -> windows::Win32::Foundation::BOOL {
                let monitors = &mut *(data.0 as *mut Vec<String>);
                let mut info = MONITORINFO::default();
                info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
                if GetMonitorInfoW(hmonitor, &mut info).as_bool() {
                    let w = info.rcMonitor.right - info.rcMonitor.left;
                    let h = info.rcMonitor.bottom - info.rcMonitor.top;
                    monitors.push(format!("{}x{}", w, h));
                }
                true.into()
            }

            let _ = EnumDisplayMonitors(
                HDC::default(),
                None,
                Some(monitor_enum_proc),
                LPARAM(&mut monitors as *mut Vec<String> as isize),
            );

            if !monitors.is_empty() {
                return format!("{} Monitors ({})", monitors.len(), monitors.join(", "));
            }
        }
        "Unknown".to_string()
    }

    pub fn get_hardware_info(&self) -> String {
        let cpu_name = self
            .sys
            .cpus()
            .get(0)
            .map(|c| c.brand())
            .unwrap_or("Unknown CPU");
        format!("CPU: {}", cpu_name)
    }

    pub fn get_temps(&self) -> String {
        let mut temps = Vec::new();
        for comp in &self.components {
            if comp.label().to_lowercase().contains("cpu") {
                if let Some(t) = comp.temperature() {
                    temps.push(format!("{:.0}C", t));
                }
            }
        }
        if temps.is_empty() {
            "N/A".to_string()
        } else {
            temps.join(", ")
        }
    }

    pub fn get_interesting_apps(&self) -> String {
        let mut apps = Vec::new();
        let targets = [
            "steam", "spotify", "discord", "code", "browser", "chrome", "firefox",
        ];
        for proc in self.sys.processes().values() {
            let name = proc.name().to_string_lossy().to_lowercase();
            for t in targets {
                if name.contains(t) && !apps.contains(&t.to_string()) {
                    apps.push(t.to_string());
                }
            }
        }
        if apps.is_empty() {
            "None".to_string()
        } else {
            apps.join(", ")
        }
    }

    pub fn get_clipboard_preview(&mut self) -> String {
        if let Some(cb) = self.clipboard.as_mut() {
            // Changed from ref mut cb to cb = self.clipboard.as_mut()
            // Arboard doesn't have a stable way to check type without getting.
            // We just try to get text.
            // CLIPBOARD can be slow/blocking, so we use a small timeout or just skip if it fails.
            if let Ok(text) = cb.get_text() {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return "Empty".to_string();
                }
                let preview = if trimmed.chars().count() > 15 {
                    format!("{}...", trimmed.chars().take(12).collect::<String>())
                } else {
                    trimmed.to_string()
                };
                return format!("\"{}\"", preview);
            }
        }
        "None/Binary".to_string()
    }

    pub fn get_context_snapshot(&mut self) -> String {
        self.refresh();
        let now = Local::now();
        let date = now.format("%Y-%m-%d").to_string();
        let day = now.format("%A").to_string();
        let time = now.format("%H:%M:%S").to_string();

        let cpu = self.get_cpu_usage();
        let hardware = self.get_hardware_info();
        let monitors = self.get_monitor_info();

        let used_mem = self.get_memory_usage();
        let total_mem = self.get_total_memory();
        let app = self.get_active_window_title();

        let battery = self.get_battery_info();
        let uptime = self.get_uptime();
        let idle = self.get_idle_time();
        let idle_str = if idle.as_secs() > 60 {
            format!("{}m", idle.as_secs() / 60)
        } else {
            format!("{}s", idle.as_secs())
        };

        let theme = self.get_theme();
        let disk = self.get_disk_info();
        let net = self.get_network_rates();
        let temp = self.get_temps();
        let procs = self.get_interesting_apps();
        let clip = self.get_clipboard_preview();

        format!(
            "[{hardware}] Date: {date} ({day}), Time: {time}, Theme: {theme}, Displays: {monitors}, Active App: {app}, CPU: {cpu:.1}%, Temp: {temp}, RAM: {used_mem}/{total_mem}MB, Disk: {disk}, Net: {net}, Battery: {battery}, Uptime: {uptime}, User Idle: {idle_str}, Interesting Apps: {procs}, Clipboard: {clip}"
        )
    }
}

fn get_last_cleanup_date() -> Option<NaiveDate> {
    let mut path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    path.push("data");
    path.push("last_factboard_cleanup.txt");
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(date) = NaiveDate::parse_from_str(content.trim(), "%Y-%m-%d") {
            return Some(date);
        }
    }
    None
}

fn set_last_cleanup_date(date: NaiveDate) {
    let mut path = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    path.push("data");
    let _ = std::fs::create_dir_all(&path);
    path.push("last_factboard_cleanup.txt");
    let _ = std::fs::write(&path, date.format("%Y-%m-%d").to_string());
}

pub struct InteractionManager {
    senses: Senses,
    last_interaction: Instant,
    config: AiConfig,
    base_interval: Duration,
    scheduler: ActionScheduler,
    first_run: bool,
    routines: std::sync::Arc<std::sync::Mutex<RoutinesConfig>>,
    routine_states: RoutineStateConfig,
}

impl InteractionManager {
    pub fn new(config: AiConfig, scheduler: ActionScheduler, shared_routines: std::sync::Arc<std::sync::Mutex<RoutinesConfig>>) -> Self {
        let base_interval = Duration::from_secs(config.interaction_frequency * 60);

        Self {
            senses: Senses::new(),
            last_interaction: Instant::now(),
            config,
            base_interval,
            scheduler,
            first_run: true,
            routines: shared_routines,
            routine_states: RoutineStateConfig::load(),
        }
    }

    pub fn update_config(&mut self, config: AiConfig) {
        self.config = config;
        // Enforce 1 minute minimum to avoid accidental spam during typing
        self.base_interval = Duration::from_secs(self.config.interaction_frequency.max(1) * 60);
    }

    pub fn update_routines(&mut self, routines: RoutinesConfig) {
        let mut cfg = self.routines.lock().unwrap();
        *cfg = routines;
    }

    pub fn check_for_trigger(&mut self) -> Option<crate::types::ChatInput> {
        if !self.config.active_interaction_enabled || self.config.interaction_frequency == 0 {
            return None;
        }

        let now = Instant::now();

        // 0. Poll Scheduler FIRST (Explicit User/AI Requests take precedence)
        // Scheduler items are text-only for now
        if let Some(memo) = self.scheduler.poll() {
            self.last_interaction = now; // Reset routine timer too
            return Some(crate::types::ChatInput {
                text: format!("[SYSTEM_EVENT] Scheduled Reminder triggered: '{}'. If you deem this reminder critical or the user won't notice your speech bubble, use 'send_notification' to alert them.", memo),
                images: vec![],
            });
        }

        // 0.5 Poll Routines
        let routines_snapshot = self.routines.lock().unwrap().routines.clone();
        for routine in &routines_snapshot {
            if !routine.is_active {
                continue;
            }

            let now_local = Local::now();
            let last_run_str = self.routine_states.last_executions.get(&routine.id);
            let last_run = last_run_str
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Local));

            let should_run = match routine.schedule_type {
                ScheduleType::Daily => {
                    // Run if last_run is before today, OR if time_of_day is set and passed today but not run today yet
                    if let Some(lr) = last_run {
                        if lr.date_naive() < now_local.date_naive() {
                            if let Some(ref t) = routine.time_of_day {
                                if let Ok(time) = NaiveTime::parse_from_str(t, "%H:%M") {
                                    now_local.time() >= time
                                } else {
                                    true
                                }
                            } else {
                                true
                            }
                        } else {
                            false
                        }
                    } else {
                        // Never run before
                        if let Some(ref t) = routine.time_of_day {
                            if let Ok(time) = NaiveTime::parse_from_str(t, "%H:%M") {
                                now_local.time() >= time
                            } else {
                                true
                            }
                        } else {
                            true
                        }
                    }
                }
                ScheduleType::Weekly => {
                    let target_day = routine.day_of_week.unwrap_or(0);
                    let current_day = now_local.weekday().num_days_from_monday() as u32;
                    if current_day == target_day {
                        if let Some(lr) = last_run {
                            if lr.date_naive() < now_local.date_naive() {
                                if let Some(ref t) = routine.time_of_day {
                                    if let Ok(time) = NaiveTime::parse_from_str(t, "%H:%M") {
                                        now_local.time() >= time
                                    } else {
                                        true
                                    }
                                } else {
                                    true
                                }
                            } else {
                                false
                            }
                        } else {
                            if let Some(ref t) = routine.time_of_day {
                                if let Ok(time) = NaiveTime::parse_from_str(t, "%H:%M") {
                                    now_local.time() >= time
                                } else {
                                    true
                                }
                            } else {
                                true
                            }
                        }
                    } else {
                        false
                    }
                }
                ScheduleType::Monthly => {
                    let target_date = routine.day_of_month.unwrap_or(1);
                    if now_local.day() == target_date {
                        if let Some(lr) = last_run {
                            if lr.date_naive() < now_local.date_naive() {
                                if let Some(ref t) = routine.time_of_day {
                                    if let Ok(time) = NaiveTime::parse_from_str(t, "%H:%M") {
                                        now_local.time() >= time
                                    } else {
                                        true
                                    }
                                } else {
                                    true
                                }
                            } else {
                                false
                            }
                        } else {
                            if let Some(ref t) = routine.time_of_day {
                                if let Ok(time) = NaiveTime::parse_from_str(t, "%H:%M") {
                                    now_local.time() >= time
                                } else {
                                    true
                                }
                            } else {
                                true
                            }
                        }
                    } else {
                        false
                    }
                }
                ScheduleType::IntervalDays => {
                    let interval = routine.interval.unwrap_or(1);
                    if let Some(lr) = last_run {
                        let duration = now_local.signed_duration_since(lr);
                        duration.num_days() >= interval as i64
                    } else {
                        true
                    }
                }
                ScheduleType::IntervalHours => {
                    let interval = routine.interval.unwrap_or(1);
                    if let Some(lr) = last_run {
                        let duration = now_local.signed_duration_since(lr);
                        duration.num_hours() >= interval as i64
                    } else {
                        true
                    }
                }
                ScheduleType::IntervalMinutes => {
                    let interval = routine.interval.unwrap_or(1);
                    if let Some(lr) = last_run {
                        let duration = now_local.signed_duration_since(lr);
                        duration.num_minutes() >= interval as i64
                    } else {
                        true
                    }
                }
            };

            if should_run {
                self.routine_states.last_executions.insert(routine.id.clone(), now_local.to_rfc3339());
                self.routine_states.save();
                self.last_interaction = now; // Reset random interaction timer
                return Some(crate::types::ChatInput {
                    text: routine.memo.clone(),
                    images: vec![],
                });
            }
        }

        let elapsed = now.duration_since(self.last_interaction);

        // 1. Basic Timer Check with Randomness
        let mut is_triggered = self.first_run;

        if !is_triggered && elapsed > self.base_interval {
            // Add some randomness +/- 20%
            let mut rng = rand::thread_rng();
            let random_factor: f32 = rng.gen_range(0.8..1.2);
            let threshold = self.base_interval.mul_f32(random_factor);

            if elapsed > threshold {
                is_triggered = true;
            }
        }

        if is_triggered {
            self.first_run = false;
            self.last_interaction = now;
                let context = self.senses.get_context_snapshot();

                let mut images = Vec::new();
                if self.config.active_interaction_screenshots_enabled {
                    match crate::screen_capture::capture_all_monitors() {
                        Ok(imgs) => {
                            for img in imgs {
                                 if let Ok(data) = crate::screen_capture::compress_to_jpeg(&img, 80) {
                                     images.push(crate::types::ImageData {
                                         data,
                                         mime_type: "image/jpeg".to_string(),
                                     });
                                 }
                            }
                        }
                        Err(e) => {
                            tracing::error!("Screenshot failed: {}", e);
                        }
                    }
                }

                return Some(crate::types::ChatInput {
                    text: format!("[SYSTEM_EVENT] Routine Check. Context: {}. Observe current activities and decide if you need to use tools or send a system notification for important findings.", context),
                    images,
                });
        }

        None
    }
}
