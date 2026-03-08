use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::format::Writer;
use tracing_subscriber::fmt::time::FormatTime;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

struct LocalTimer;

impl FormatTime for LocalTimer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let now = chrono::Local::now();
        write!(w, "{}", now.format("%Y-%m-%dT%H:%M:%S%.3f%:z"))
    }
}

pub fn init_logging() {
    let log_dir = "logs";

    // Create logs directory if it doesn't exist
    if !std::path::Path::new(log_dir).exists() {
        let _ = std::fs::create_dir_all(log_dir);
    }

    // 1. File appender: daily rotation
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir, "ameath.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Leak the guard so it stays alive for the duration of the program
    Box::leak(Box::new(_guard));

    // 2. Formatters
    let file_layer = fmt::layer()
        .with_timer(LocalTimer)
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true);

    let console_layer = fmt::layer()
        .with_timer(LocalTimer)
        .with_target(false)
        .pretty();

    // 3. Register global subscriber
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with(console_layer)
        .with(file_layer)
        .init();

    // 4. Cleanup old logs (keep last 7 days)
    // Now called AFTER init() so we can see cleanup logs in the file
    cleanup_old_logs(log_dir, 7);

    // 5. Setup Panic Hook to ensure panics are stored in log files
    std::panic::set_hook(Box::new(|panic_info| {
        let msg = match panic_info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Box<dyn Any>",
            },
        };

        if let Some(location) = panic_info.location() {
            tracing::error!(
                "PANIC occurred at {}:{} - {}",
                location.file(),
                location.line(),
                msg
            );
        } else {
            tracing::error!("PANIC occurred - {}", msg);
        }
    }));

    tracing::info!("Logging initialized. Logs are stored in {}", log_dir);
}

fn cleanup_old_logs(dir: &str, keep_days: i64) {
    let now = chrono::Local::now().date_naive();
    tracing::info!(
        "Checking for logs older than {} days in directory: {}",
        keep_days,
        dir
    );

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };

            // Expected format: ameath.log.YYYY-MM-DD
            // We split by '.' and try to parse the last part
            let parts: Vec<&str> = file_name.split('.').collect();
            if parts.len() < 3 {
                continue;
            }

            let date_str = parts[parts.len() - 1];
            if let Ok(log_date) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                let days_old = (now - log_date).num_days();

                if days_old >= keep_days {
                    tracing::info!(
                        "Deleting old log file: {:?} (Log date: {}, {} days old)",
                        file_name,
                        log_date,
                        days_old
                    );
                    if let Err(e) = std::fs::remove_file(&path) {
                        tracing::error!("Failed to delete old log file {:?}: {}", file_name, e);
                    }
                }
            }
        }
    } else {
        tracing::error!("Failed to read log directory: {}", dir);
    }
}
