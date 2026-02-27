use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init_logging() {
    let log_dir = "logs";

    // Create logs directory if it doesn't exist
    if !std::path::Path::new(log_dir).exists() {
        let _ = std::fs::create_dir_all(log_dir);
    }

    // Cleanup old logs (keep last 7 days)
    cleanup_old_logs(log_dir, 7);

    // 1. File appender: daily rotation
    let file_appender = RollingFileAppender::new(Rotation::DAILY, log_dir, "ameath.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Leak the guard so it stays alive for the duration of the program
    // In a GUI app, we usually don't have a clean "exit" but if we do,
    // we might want a global guard.
    Box::leak(Box::new(_guard));

    // 2. Formatters
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false) // Disable ANSI colors in file
        .with_target(true)
        .with_thread_ids(true);

    let console_layer = fmt::layer().with_target(false).pretty();

    // 3. Register global subscriber
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with(console_layer)
        .with(file_layer)
        .init();

    tracing::info!("Logging initialized. Logs are stored in {}", log_dir);
}

fn cleanup_old_logs(dir: &str, keep_days: i64) {
    let now = chrono::Local::now();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    let modified: chrono::DateTime<chrono::Local> = modified.into();
                    if now.signed_duration_since(modified).num_days() > keep_days {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}
