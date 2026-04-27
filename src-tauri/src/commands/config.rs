use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

// ── Shared application state ─────────────────────────────────────────
pub struct AppState {
    pub data_dir: PathBuf,
    pub config: Mutex<Config>,
    pub remote_port: Mutex<Option<u16>>,
    pub app_update_in_progress: Mutex<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TurboProfile {
    pub name: String,
    pub whitelist: Vec<String>,
    pub blacklist: Vec<String>,
}

impl Default for TurboProfile {
    fn default() -> Self {
        Self {
            name: "Jeux".into(),
            whitelist: vec!["discord".into()],
            blacklist: vec!["chrome".into(), "teams".into(), "spotify".into()],
        }
    }
}

// ── Configuration model ──────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub language: String,
    pub theme: String,
    pub ai_enabled: bool,
    pub ai_endpoint: String,
    #[serde(default)]
    pub ai_api_key: String,
    #[serde(default)]
    pub ai_app_key: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    pub ai_consent_given: bool,
    pub scheduler_enabled: bool,
    pub scheduler_interval: String,
    pub auto_snapshot: bool,
    #[serde(default)]
    pub cleanup_history: Vec<u64>,
    #[serde(default = "default_true")]
    pub first_run: bool,
    /// Startup mode: "visible", "minimized", or "tray"
    #[serde(default = "default_startup_mode")]
    pub startup_mode: String,
    /// Whether system tray icon is enabled
    #[serde(default = "default_true")]
    pub tray_enabled: bool,
    /// Granular telemetry: Windows telemetry services
    #[serde(default = "default_true")]
    pub telemetry_windows: bool,
    /// Granular telemetry: Office telemetry
    #[serde(default = "default_true")]
    pub telemetry_office: bool,
    /// Granular telemetry: NVIDIA telemetry
    #[serde(default = "default_true")]
    pub telemetry_nvidia: bool,
    /// Granular telemetry: Browser telemetry (Edge/Chrome/Firefox)
    #[serde(default = "default_true")]
    pub telemetry_browsers: bool,
    /// Granular telemetry: Ad tracking / Activity History / Location
    #[serde(default = "default_true")]
    pub telemetry_tracking: bool,
    /// Custom backup directory (if user wants to avoid C:)
    #[serde(default)]
    pub backup_dir: String,
    /// Close to tray instead of quitting
    #[serde(default)]
    pub close_to_tray: bool,
    /// Auto-clean scheduler enabled
    #[serde(default)]
    pub auto_clean_enabled: bool,
    /// Auto-clean interval: "disabled", "daily", "weekly", "monthly"
    #[serde(default = "default_disabled")]
    pub auto_clean_interval: String,
    /// Check Aura Update app updates on startup
    #[serde(default = "default_true")]
    pub auto_update_on_startup: bool,
    /// Active battery mode: eco | normal | extreme
    #[serde(default = "default_battery_mode")]
    pub battery_mode: String,
    /// Stored turbo profiles (whitelist/blacklist)
    #[serde(default = "default_turbo_profiles")]
    pub turbo_profiles: Vec<TurboProfile>,
    /// Active turbo profile name
    #[serde(default = "default_active_turbo_profile")]
    pub active_turbo_profile: String,
}

fn default_true() -> bool { true }
fn default_startup_mode() -> String { "visible".into() }
fn default_disabled() -> String { "disabled".into() }
fn default_ai_model() -> String { "llama3".into() }
fn default_battery_mode() -> String { "normal".into() }
fn default_active_turbo_profile() -> String { "Jeux".into() }
fn default_turbo_profiles() -> Vec<TurboProfile> {
    vec![
        TurboProfile {
            name: "Jeux".into(),
            whitelist: vec!["discord".into()],
            blacklist: vec!["chrome".into(), "teams".into(), "spotify".into(), "onedrive".into()],
        },
        TurboProfile {
            name: "Live".into(),
            whitelist: vec!["obs64".into(), "discord".into()],
            blacklist: vec!["chrome".into(), "teams".into(), "adobe".into()],
        },
        TurboProfile {
            name: "Work".into(),
            whitelist: vec!["teams".into(), "outlook".into()],
            blacklist: vec!["spotify".into(), "xbox".into(), "gamebar".into()],
        },
    ]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: "fr".into(),
            theme: "dark".into(),
            ai_enabled: false,
            ai_endpoint: "http://localhost:11434".into(),
            ai_api_key: String::new(),
            ai_app_key: "aura_aura_update_mmkzgiz4".into(),
            ai_model: default_ai_model(),
            ai_consent_given: false,
            scheduler_enabled: false,
            scheduler_interval: "disabled".into(),
            auto_snapshot: true,
            cleanup_history: Vec::new(),
            first_run: true,
            startup_mode: "visible".into(),
            tray_enabled: true,
            telemetry_windows: true,
            telemetry_office: true,
            telemetry_nvidia: true,
            telemetry_browsers: true,
            telemetry_tracking: true,
            backup_dir: String::new(),
            close_to_tray: false,
            auto_clean_enabled: false,
            auto_clean_interval: "disabled".into(),
            auto_update_on_startup: false,
            battery_mode: default_battery_mode(),
            turbo_profiles: default_turbo_profiles(),
            active_turbo_profile: default_active_turbo_profile(),
        }
    }
}

// ── Portable directory ───────────────────────────────────────────────
pub fn get_portable_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_dir = exe.parent().unwrap_or_else(|| std::path::Path::new("."));
    let portable_dir = exe_dir.join("aura_data");

    if fs::create_dir_all(&portable_dir).is_ok() && fs::create_dir_all(portable_dir.join("cache")).is_ok() {
        return portable_dir;
    }

    // Fallback when install directory is not writable.
    let fallback_base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|_| std::env::current_dir())
        .unwrap_or_else(|_| PathBuf::from("."));
    let fallback_dir = fallback_base.join("Aura.Update").join("aura_data");
    let _ = fs::create_dir_all(&fallback_dir);
    let _ = fs::create_dir_all(fallback_dir.join("cache"));
    fallback_dir
}

pub fn load_config(data_dir: &PathBuf) -> Config {
    let path = data_dir.join("config.json");
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<Config>(&data) {
                return cfg;
            }
        }
    }
    Config::default()
}

/// Persist config to disk. Usable from any module via `super::config::save_config`.
pub(crate) fn save_config(data_dir: &PathBuf, config: &Config) {
    let path = data_dir.join("config.json");
    if let Ok(json) = serde_json::to_string_pretty(config) {
        fs::write(path, json).ok();
    }
}

// ── Commands ─────────────────────────────────────────────────────────
#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> Config {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn set_config_value(
    state: tauri::State<'_, AppState>,
    key: String,
    value: serde_json::Value,
) -> Result<bool, String> {
    let mut cfg = state.config.lock().unwrap();
    match key.as_str() {
        "language" => cfg.language = value.as_str().unwrap_or("fr").into(),
        "theme" => cfg.theme = value.as_str().unwrap_or("dark").into(),
        "aiEnabled" | "ai_enabled" => cfg.ai_enabled = value.as_bool().unwrap_or(false),
        "aiEndpoint" | "ai_endpoint" => cfg.ai_endpoint = value.as_str().unwrap_or("").into(),
        "aiApiKey" | "ai_api_key" => cfg.ai_api_key = value.as_str().unwrap_or("").into(),
        "aiAppKey" | "ai_app_key" => cfg.ai_app_key = value.as_str().unwrap_or("").into(),
        "aiModel" | "ai_model" => cfg.ai_model = value.as_str().unwrap_or("aura-ia").into(),
        "aiConsentGiven" | "ai_consent_given" => {
            cfg.ai_consent_given = value.as_bool().unwrap_or(false)
        }
        "scheduler_enabled" => cfg.scheduler_enabled = value.as_bool().unwrap_or(false),
        "scheduler_interval" => cfg.scheduler_interval = value.as_str().unwrap_or("disabled").into(),
        "auto_snapshot" => cfg.auto_snapshot = value.as_bool().unwrap_or(true),
        "first_run" => cfg.first_run = value.as_bool().unwrap_or(false),
        "startup_mode" => cfg.startup_mode = value.as_str().unwrap_or("visible").into(),
        "tray_enabled" => cfg.tray_enabled = value.as_bool().unwrap_or(true),
        "telemetry_windows" => cfg.telemetry_windows = value.as_bool().unwrap_or(true),
        "telemetry_office" => cfg.telemetry_office = value.as_bool().unwrap_or(true),
        "telemetry_nvidia" => cfg.telemetry_nvidia = value.as_bool().unwrap_or(true),
        "telemetry_browsers" => cfg.telemetry_browsers = value.as_bool().unwrap_or(true),
        "telemetry_tracking" => cfg.telemetry_tracking = value.as_bool().unwrap_or(true),
        "backup_dir" => cfg.backup_dir = value.as_str().unwrap_or("").into(),
        "close_to_tray" => cfg.close_to_tray = value.as_bool().unwrap_or(false),
        "auto_clean_enabled" => cfg.auto_clean_enabled = value.as_bool().unwrap_or(false),
        "auto_clean_interval" => cfg.auto_clean_interval = value.as_str().unwrap_or("disabled").into(),
        "auto_update_on_startup" => cfg.auto_update_on_startup = value.as_bool().unwrap_or(true),
        "battery_mode" => cfg.battery_mode = value.as_str().unwrap_or("normal").into(),
        "active_turbo_profile" => cfg.active_turbo_profile = value.as_str().unwrap_or("Jeux").into(),
        _ => return Err(format!("Unknown config key: {key}")),

    }
    save_config(&state.data_dir, &cfg);
    Ok(true)
}

#[tauri::command]
pub fn get_translations(lang: String) -> Result<serde_json::Value, String> {
    let json = match lang.as_str() {
        "en" => include_str!("../../../frontend/locales/en.json"),
        _ => include_str!("../../../frontend/locales/fr.json"),
    };
    serde_json::from_str(json).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Predict next cleanup gain from history using simple moving average.
#[tauri::command]
pub fn get_predicted_cleanup_gain(state: tauri::State<'_, AppState>) -> u64 {
    let cfg = state.config.lock().unwrap();
    let history = &cfg.cleanup_history;
    if history.is_empty() {
        return 0;
    }
    let recent: Vec<&u64> = history.iter().rev().take(10).collect();
    let sum: u64 = recent.iter().copied().sum();
    sum / recent.len() as u64
}

/// Open a URL in the system default browser.
/// Only allows https:// and http:// URLs for security.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    const STRICT_PRIVACY_MODE: bool = true;
    if STRICT_PRIVACY_MODE {
        return Err("Mode confidentialité stricte: liens externes désactivés".into());
    }

    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("Only http/https URLs are allowed".into());
    }
    open::that(&url).map_err(|e| format!("Failed to open URL: {}", e))
}

/// Get free disk space (in bytes) for a given path (defaults to C:\ on Windows).
#[tauri::command]
pub fn get_disk_free_space(path: Option<String>) -> Result<u64, String> {
    use sysinfo::Disks;
    let target = path.unwrap_or_else(|| {
        #[cfg(windows)]
        { "C:\\".into() }
        #[cfg(not(windows))]
        { "/".into() }
    });
    let disks = Disks::new_with_refreshed_list();
    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        if target.starts_with(&mount) || mount.starts_with(&target) {
            return Ok(disk.available_space());
        }
    }
    Err("Disk not found".into())
}
