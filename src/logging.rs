use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
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

/// Returns the newest bounded application log lines first.
///
/// The active file and rotated `ameath.log.*` files are considered. Each file
/// is read from its tail so a large historical log cannot block the settings UI.
pub fn read_recent_log_lines(max_lines: usize, max_bytes: usize) -> String {
    read_recent_log_lines_from_dir(Path::new("logs"), max_lines, max_bytes)
}

fn read_recent_log_lines_from_dir(dir: &Path, max_lines: usize, max_bytes: usize) -> String {
    if max_lines == 0 || max_bytes == 0 {
        return String::new();
    }

    let mut files: Vec<(bool, String, PathBuf)> = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() {
                return None;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name != "ameath.log" && !name.starts_with("ameath.log.") {
                return None;
            }
            let is_active = name == "ameath.log";
            Some((is_active, name.into_owned(), entry.path()))
        })
        .collect();

    // Rotated files contain an ISO date in their name, so lexical ordering is
    // chronological and remains deterministic even when filesystem timestamps
    // have coarse resolution. The active file is always the newest segment.
    files.sort_by(|(active_a, name_a, _), (active_b, name_b, _)| {
        active_a.cmp(active_b).then_with(|| name_a.cmp(name_b))
    });
    let mut combined = Vec::new();
    for (_, _, path) in files {
        let Ok(mut file) = std::fs::File::open(path) else {
            continue;
        };
        let Ok(length) = file.metadata().map(|metadata| metadata.len()) else {
            continue;
        };
        let start = length.saturating_sub(max_bytes as u64);
        if file.seek(SeekFrom::Start(start)).is_err() {
            continue;
        }
        let mut bytes = Vec::with_capacity((length - start) as usize);
        if file.read_to_end(&mut bytes).is_ok() {
            if !combined.is_empty() && !combined.ends_with(b"\n") {
                combined.push(b'\n');
            }
            combined.extend_from_slice(&bytes);
        }
    }

    if combined.len() > max_bytes {
        let start = combined.len() - max_bytes;
        combined.drain(..start);
    }

    let text = String::from_utf8_lossy(&combined);
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..]
        .iter()
        .rev()
        .copied()
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::read_recent_log_lines_from_dir;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn recent_logs_are_bounded_and_ignore_unrelated_files() {
        let dir = std::env::temp_dir().join(format!("ameath-log-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ameath.log.2026-11-15"), "old-1\nold-2\n").unwrap();
        fs::write(dir.join("ameath.log"), "new-1\nnew-2\nnew-3\n").unwrap();
        fs::write(dir.join("other.log"), "ignore-me\n").unwrap();

        let text = read_recent_log_lines_from_dir(&dir, 3, 1024);
        assert_eq!(text, "new-3\nnew-2\nnew-1");

        let _ = fs::remove_dir_all(PathBuf::from(dir));
    }

    #[test]
    fn missing_directory_and_zero_limits_are_safe() {
        let missing = std::env::temp_dir().join(format!("ameath-missing-{}", uuid::Uuid::new_v4()));
        assert!(read_recent_log_lines_from_dir(&missing, 100, 1024).is_empty());
        assert!(read_recent_log_lines_from_dir(&missing, 0, 1024).is_empty());
        assert!(read_recent_log_lines_from_dir(&missing, 100, 0).is_empty());
    }
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
