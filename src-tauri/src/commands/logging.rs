use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::commands::config::AppState;

/// Global log file handle — write-only, flushed after each entry.
static LOG_FILE: Mutex<Option<fs::File>> = Mutex::new(None);
/// Structured session log handle — JSONL.
static SESSION_FILE: Mutex<Option<fs::File>> = Mutex::new(None);
/// Last structured action (for crash payload correlation).
static LAST_ACTION: Mutex<Option<String>> = Mutex::new(None);
/// Session start instant for uptime diagnostics.
static SESSION_START: OnceLock<Instant> = OnceLock::new();

/// Initialize the rotating log system.
/// - Creates a `logs/` directory in `data_dir`
/// - Opens a new log file named `aura_update_YYYY-MM-DD.log`
/// - Purges log files older than the 3 most recent
/// - Writes a system-identification header for crash diagnostics
pub fn init_logging(data_dir: &Path) {
    let logs_dir = data_dir.join("logs");
    fs::create_dir_all(&logs_dir).ok();
    SESSION_START.get_or_init(Instant::now);

    // Purge old logs — keep only the 3 most recent
    purge_old_logs(&logs_dir, 5);

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
            write_system_dump_header();
        }
        Err(e) => {
            eprintln!("[logging] Failed to open log file: {}", e);
        }
    }

    // Open today's structured session log
    let session_path = logs_dir.join(format!("session_{}.jsonl", today));
    if let Ok(file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&session_path)
    {
        *SESSION_FILE.lock().unwrap() = Some(file);
    }
}

/// Write a structured system identification block at the top of each log session.
/// This allows crash reports to carry full hardware context automatically.
fn write_system_dump_header() {
    use sysinfo::System;

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_list(sysinfo::CpuRefreshKind::everything());

    let os = format!(
        "{} ({})",
        System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string()),
        System::kernel_version().unwrap_or_default()
    );

    let cpu_count = sys.cpus().len();
    let cpu = sys
        .cpus()
        .first()
        .map(|c| format!("{} ({} Threads)", c.brand().trim(), cpu_count))
        .unwrap_or_else(|| "Unknown".to_string());

    let ram_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);

    let is_admin = {
        #[cfg(windows)]
        {
            std::process::Command::new("net")
                .args(["session"])
                .creation_flags(0x0800_0000)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        }
        #[cfg(unix)]
        {
            std::process::Command::new("id")
                .arg("-u")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
                .unwrap_or(false)
        }
    };

    let version = env!("CARGO_PKG_VERSION");

    log_info(&format!("=== Aura Update started — {} ===", now));
    log_info(&format!("OS: {}", os));
    log_info(&format!("CPU: {}", cpu));
    log_info(&format!("RAM: {:.1} GB Total", ram_gb));
    log_info(&format!("App Version: {} (Admin Mode: {})", version, is_admin));
    log_info("=================================================");

    log_action_event(
        "session",
        "app",
        "startup",
        "start",
        Some("boot"),
        None,
        None,
        Some(0),
        None,
        false,
        &format!("os={os}; cpu={cpu}; ram_gb={:.1}; admin={is_admin}; version={version}", ram_gb),
    );
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
#[allow(dead_code)]
pub fn log_warn(message: &str) {
    write_log("WARN", message);
}

/// Write an ERROR-level log entry.
#[allow(dead_code)]
pub fn log_error(message: &str) {
    write_log("ERROR", message);
}

/// Write a PANIC-level log entry (used by crash reporter).
#[allow(dead_code)]
pub fn log_panic(message: &str) {
    write_log("PANIC", message);
}

pub fn log_action_event(
    run_id: &str,
    module: &str,
    action: &str,
    event: &str,
    step: Option<&str>,
    pid: Option<u32>,
    exit_code: Option<i32>,
    duration_ms: Option<u128>,
    timeout_ms: Option<u64>,
    user_cancelled: bool,
    message: &str,
) {
    write_log(
        "INFO",
        &format!(
            "[{}] {}:{}:{} {}",
            run_id,
            module,
            action,
            event,
            message
        ),
    );

    let record = serde_json::json!({
        "ts": chrono::Local::now().to_rfc3339(),
        "level": "INFO",
        "run_id": run_id,
        "module": module,
        "action": action,
        "event": event,
        "step": step,
        "pid": pid,
        "exit_code": exit_code,
        "duration_ms": duration_ms,
        "timeout_ms": timeout_ms,
        "user_cancelled": user_cancelled,
        "message": message,
    });

    *LAST_ACTION.lock().unwrap() = Some(record.to_string());

    if let Ok(mut guard) = SESSION_FILE.lock() {
        if let Some(ref mut file) = *guard {
            let _ = file.write_all(record.to_string().as_bytes());
            let _ = file.write_all(b"\n");
            let _ = file.flush();
        }
    }
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

pub fn get_latest_session_log_path(data_dir: &Path) -> Option<PathBuf> {
    let logs_dir = data_dir.join("logs");
    let mut files: Vec<PathBuf> = Vec::new();

    if let Ok(entries) = fs::read_dir(&logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl")
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map_or(false, |n| n.starts_with("session_"))
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files.last().cloned()
}

fn tail_lines(path: &Path, max_lines: usize) -> String {
    if let Ok(content) = fs::read_to_string(path) {
        let lines: Vec<&str> = content.lines().collect();
        let start = lines.len().saturating_sub(max_lines);
        return lines[start..].join("\n");
    }
    String::new()
}

fn session_uptime_sec() -> u64 {
    SESSION_START
        .get()
        .map(|s| s.elapsed().as_secs())
        .unwrap_or(0)
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

    // Read latest text log tail
    let log_tail = if let Some(log_path) = get_latest_log_path(&data_dir) {
        tail_lines(&log_path, 120)
    } else {
        String::new()
    };

    // Read latest structured log tail
    let structured_tail = if let Some(path) = get_latest_session_log_path(&data_dir) {
        tail_lines(&path, 200)
    } else {
        String::new()
    };

    let last_action = LAST_ACTION.lock().unwrap().clone();

    const CRASH_ENDPOINT: &str = "https://api.auraneo.fr/aura-update/v1/crash-report";
    if !CRASH_ENDPOINT.starts_with("https://") {
        return Err("Crash endpoint must use HTTPS".into());
    }

    let payload = serde_json::json!({
        "crash_data": crash_data,
        "user_message": user_message,
        "os": std::env::consts::OS,
        "app_version": env!("CARGO_PKG_VERSION"),
        "uptime_sec": session_uptime_sec(),
        "last_action": last_action,
        "log_tail": log_tail,
        "structured_tail": structured_tail,
    });

    let client = reqwest::Client::builder()
        .https_only(true)
        .min_tls_version(reqwest::tls::Version::TLS_1_2)
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("Failed to initialize secure HTTP client: {}", e))?;

    let result = client
        .post(CRASH_ENDPOINT)
        .header("X-Aura-Token", "aura_update_crash_2026")
        .json(&payload)
        .send()
        .await;

    match result {
        Ok(_) => {
            fs::remove_file(&crash_path).ok();
            Ok(true)
        }
        Err(e) => Err(format!("Failed to send crash report: {}", e)),
    }
}

/// Dismiss the crash report without sending — just deletes the file.
#[tauri::command]
pub fn clear_crash_report(state: tauri::State<'_, AppState>) -> bool {
    let crash_path = state.data_dir.join("crash_report.json");
    fs::remove_file(&crash_path).is_ok()
}
