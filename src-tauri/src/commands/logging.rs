use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::commands::config::AppState;

/// Global log file handle — write-only, flushed after each entry.
static LOG_FILE: Mutex<Option<fs::File>> = Mutex::new(None);

/// Initialize the rotating log system.
/// - Creates a `logs/` directory in `data_dir`
/// - Opens a new log file named `aura_update_YYYY-MM-DD.log`
/// - Purges log files older than the 3 most recent
pub fn init_logging(data_dir: &Path) {
    let logs_dir = data_dir.join("logs");
    fs::create_dir_all(&logs_dir).ok();

    // Purge old logs — keep only the 3 most recent
    purge_old_logs(&logs_dir, 3);

    // Open today's log file (append mode)
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let log_path = logs_dir.join(format!("aura_update_{}.log", today));

    match fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(file) => {
            *LOG_FILE.lock().unwrap() = Some(file);
            log_info(&format!("=== Aura Update started — {} ===", chrono::Local::now().format("%Y-%m-%d %H:%M:%S")));
        }
        Err(e) => {
            eprintln!("[logging] Failed to open log file: {}", e);
        }
    }
}

/// Purge old log files, keeping only the `keep` most recent.
fn purge_old_logs(logs_dir: &Path, keep: usize) {
    let mut log_files: Vec<PathBuf> = Vec::new();

    if let Ok(entries) = fs::read_dir(logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("log")
                && path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n.starts_with("aura_update_"))
            {
                log_files.push(path);
            }
        }
    }

    // Sort by name (date-based names sort chronologically)
    log_files.sort();

    // Remove oldest files beyond the retention limit
    if log_files.len() > keep {
        let to_remove = log_files.len() - keep;
        for path in log_files.iter().take(to_remove) {
            fs::remove_file(path).ok();
        }
    }
}

/// Write an INFO-level log entry.
pub fn log_info(message: &str) {
    write_log("INFO", message);
}

/// Write a WARN-level log entry.
pub fn log_warn(message: &str) {
    write_log("WARN", message);
}

/// Write an ERROR-level log entry.
pub fn log_error(message: &str) {
    write_log("ERROR", message);
}

/// Write a PANIC-level log entry (used by crash reporter).
pub fn log_panic(message: &str) {
    write_log("PANIC", message);
}

fn write_log(level: &str, message: &str) {
    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
    let line = format!("[{}] [{}] {}\n", timestamp, level, message);

    // Write to file
    if let Ok(mut guard) = LOG_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }
    }

    // Also write to stderr for dev mode
    eprint!("{}", line);
}

/// Get the path to the most recent log file (for crash reports).
pub fn get_latest_log_path(data_dir: &Path) -> Option<PathBuf> {
    let logs_dir = data_dir.join("logs");
    let mut log_files: Vec<PathBuf> = Vec::new();

    if let Ok(entries) = fs::read_dir(&logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("log")
                && path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n.starts_with("aura_update_"))
            {
                log_files.push(path);
            }
        }
    }

    log_files.sort();
    log_files.last().cloned()
}

// ── Crash Report Commands ────────────────────────────────────────────

/// Check if a crash report file exists from a previous session.
/// Returns the JSON content of the crash file, or None.
#[tauri::command]
pub fn check_pending_crash(state: tauri::State<'_, AppState>) -> Option<String> {
    let crash_path = state.data_dir.join("crash_report.json");
    if crash_path.exists() {
        fs::read_to_string(&crash_path).ok()
    } else {
        None
    }
}

/// Send the crash report to the Aura Néo API, then delete the crash file.
#[tauri::command]
pub async fn send_crash_report(
    state: tauri::State<'_, AppState>,
    user_message: String,
) -> Result<bool, String> {
    let data_dir = state.data_dir.clone();
    let crash_path = data_dir.join("crash_report.json");

    let crash_data = if crash_path.exists() {
        fs::read_to_string(&crash_path).unwrap_or_default()
    } else {
        String::new()
    };

    // Read last 50 lines of the latest log file
    let log_tail = if let Some(log_path) = get_latest_log_path(&data_dir) {
        if let Ok(content) = fs::read_to_string(&log_path) {
            let lines: Vec<&str> = content.lines().collect();
            let start = if lines.len() > 50 { lines.len() - 50 } else { 0 };
            lines[start..].join("\n")
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let payload = serde_json::json!({
        "crash_data": crash_data,
        "user_message": user_message,
        "os": std::env::consts::OS,
        "app_version": env!("CARGO_PKG_VERSION"),
        "log_tail": log_tail,
    });

    let result = reqwest::Client::new()
        .post("https://api.auraneo.fr/aura-update/v1/crash-report")
        .header("X-Aura-Token", "aura_update_crash_2026")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;

    // Always clean up crash file
    fs::remove_file(&crash_path).ok();

    match result {
        Ok(_) => Ok(true),
        Err(e) => Err(format!("Failed to send crash report: {}", e)),
    }
}

/// Dismiss the crash report without sending — just deletes the file.
#[tauri::command]
pub fn clear_crash_report(state: tauri::State<'_, AppState>) -> bool {
    let crash_path = state.data_dir.join("crash_report.json");
    fs::remove_file(&crash_path).is_ok()
}
