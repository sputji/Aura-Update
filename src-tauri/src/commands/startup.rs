use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::collections::HashSet;

// Cache to avoid spamming logs for already-warned missing startup paths
static WARNED_PATHS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupItem {
    pub name: String,
    pub path: String,
    pub enabled: bool,
    pub source: String,   // "registry", "folder", "desktop", "launchagent"
}

/// Cleans a registry path: strips surrounding quotes and trailing arguments.
/// e.g. `"C:\Program Files\app.exe" /start` → `C:\Program Files\app.exe`
#[cfg(windows)]
fn clean_registry_path(raw: &str) -> String {
    let mut cleaned = raw.trim().to_string();
    // Handle quoted path: extract content between first pair of quotes
    if cleaned.starts_with('"') {
        if let Some(end) = cleaned[1..].find('"') {
            cleaned = cleaned[1..1 + end].to_string();
        }
    }
    // Truncate at .exe/.bat/.cmd to strip trailing arguments
    let lower = cleaned.to_lowercase();
    for ext in &[".exe", ".bat", ".cmd"] {
        if let Some(pos) = lower.find(ext) {
            let candidate = cleaned[..pos + ext.len()].to_string();
            // Path Intelligence: if the path doesn't exist, try to find it
            if !std::path::Path::new(&candidate).exists() {
                if let Some(resolved) = resolve_missing_exe(&candidate) {
                    return resolved;
                }
                // Log broken path only ONCE per candidate to avoid spam
                let mut guard = WARNED_PATHS.lock().unwrap();
                let set = guard.get_or_insert_with(HashSet::new);
                if set.insert(candidate.clone()) {
                    super::logging::log_warn(&format!(
                        "Startup path not found (os error 2): {}",
                        candidate
                    ));
                }
            }
            return candidate;
        }
    }
    cleaned
}

/// Path Intelligence: recursively search common directories for a missing executable.
/// Handles "os error 2" (file not found) by scanning Program Files and user paths.
#[cfg(windows)]
fn resolve_missing_exe(original: &str) -> Option<String> {
    let path = std::path::Path::new(original);
    let filename = path.file_name()?.to_string_lossy().to_string();
    let filename_lower = filename.to_lowercase();

    // Common search roots
    let mut search_roots: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(pf) = std::env::var("ProgramFiles") {
        search_roots.push(std::path::PathBuf::from(pf));
    }
    if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
        search_roots.push(std::path::PathBuf::from(pf86));
    }
    if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
        search_roots.push(std::path::PathBuf::from(localappdata));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        search_roots.push(std::path::PathBuf::from(appdata));
    }

    for root in &search_roots {
        if let Some(found) = search_exe_recursive(root, &filename_lower, 3) {
            return Some(found.to_string_lossy().to_string());
        }
    }
    None
}

/// Recursively search for an executable by name, limited to max_depth levels.
#[cfg(windows)]
fn search_exe_recursive(dir: &std::path::Path, target_lower: &str, max_depth: u8) -> Option<std::path::PathBuf> {
    if max_depth == 0 || !dir.exists() { return None; }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_file() {
            if let Some(name) = entry_path.file_name() {
                if name.to_string_lossy().to_lowercase() == *target_lower {
                    return Some(entry_path);
                }
            }
        } else if entry_path.is_dir() {
            if let Some(found) = search_exe_recursive(&entry_path, target_lower, max_depth - 1) {
                return Some(found);
            }
        }
    }
    None
}

// ── Get startup items ────────────────────────────────────────────────
#[tauri::command]
pub fn get_startup_items() -> Result<Vec<StartupItem>, String> {
    get_items_impl()
}

#[cfg(windows)]
fn get_items_impl() -> Result<Vec<StartupItem>, String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    let mut items = Vec::new();

    // Registry: HKCU\...\Run  and HKLM\...\Run  (enabled items)
    let paths = [
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"),
    ];

    for (root, subkey) in &paths {
        if let Ok(key) = RegKey::predef(*root).open_subkey_with_flags(subkey, KEY_READ) {
            for value in key.enum_values().flatten() {
                let (name, data) = value;
                let raw_path = format!("{}", data);
                let path = clean_registry_path(&raw_path);
                items.push(StartupItem {
                    name: name.clone(),
                    path,
                    enabled: true,
                    source: "registry".into(),
                });
            }
        }
    }

    // Items we previously disabled (stored in our backup registry key)
    if let Ok(backup) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"SOFTWARE\AuraUpdate\DisabledStartup", KEY_READ)
    {
        for value in backup.enum_values().flatten() {
            let (name, data) = value;
            let raw_path = format!("{}", data);
            let path = clean_registry_path(&raw_path);
            // Only add if not already in the enabled list
            if !items.iter().any(|i| i.name == name) {
                items.push(StartupItem {
                    name: name.clone(),
                    path,
                    enabled: false,
                    source: "registry".into(),
                });
            }
        }
    }

    // Startup folder
    if let Some(startup) = dirs::data_dir() {
        let folder = startup
            .parent()
            .unwrap_or(&startup)
            .join("Roaming")
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup");
        if folder.exists() {
            if let Ok(entries) = std::fs::read_dir(&folder) {
                // Junk files that are never real startup items
                const JUNK_NAMES: &[&str] = &["desktop.ini", "thumbs.db"];
                const JUNK_EXTS: &[&str] = &[".ini", ".db", ".txt"];

                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') { continue; }

                    let lower = name.to_lowercase();
                    if JUNK_NAMES.iter().any(|j| lower == *j) { continue; }
                    if JUNK_EXTS.iter().any(|ext| lower.ends_with(ext)) { continue; }

                    // Validate the file actually exists on disk
                    let entry_path = entry.path();
                    if !entry_path.exists() { continue; }

                    // Case-insensitive check for .disabled / .lnk extensions
                    let is_disabled = lower.ends_with(".disabled");
                    let display_name = {
                        let mut n = name.clone();
                        // Strip .disabled (case-insensitive)
                        if lower.ends_with(".disabled") {
                            n = n[..n.len() - ".disabled".len()].to_string();
                        }
                        // Strip .lnk (case-insensitive)
                        let nl = n.to_lowercase();
                        if nl.ends_with(".lnk") {
                            n = n[..n.len() - ".lnk".len()].to_string();
                        }
                        n
                    };
                    items.push(StartupItem {
                        name: display_name,
                        path: entry_path.to_string_lossy().to_string(),
                        enabled: !is_disabled,
                        source: "folder".into(),
                    });
                }
            }
        }
    }

    Ok(items)
}

#[cfg(target_os = "linux")]
fn get_items_impl() -> Result<Vec<StartupItem>, String> {
    let mut items = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let autostart = home.join(".config/autostart");
        if autostart.exists() {
            if let Ok(entries) = std::fs::read_dir(&autostart) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "desktop").unwrap_or(false) {
                        let content = std::fs::read_to_string(&path).unwrap_or_default();
                        let name = content
                            .lines()
                            .find(|l| l.starts_with("Name="))
                            .map(|l| l.strip_prefix("Name=").unwrap_or("").to_string())
                            .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
                        let hidden = content
                            .lines()
                            .any(|l| l.trim() == "Hidden=true" || l.trim() == "X-GNOME-Autostart-enabled=false");
                        items.push(StartupItem {
                            name,
                            path: path.to_string_lossy().to_string(),
                            enabled: !hidden,
                            source: "desktop".into(),
                        });
                    }
                }
            }
        }
    }
    Ok(items)
}

#[cfg(target_os = "macos")]
fn get_items_impl() -> Result<Vec<StartupItem>, String> {
    let mut items = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let agents = home.join("Library/LaunchAgents");
        if agents.exists() {
            if let Ok(entries) = std::fs::read_dir(&agents) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "plist").unwrap_or(false) {
                        let name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let content = std::fs::read_to_string(&path).unwrap_or_default();
                        let disabled = content.contains("<key>Disabled</key>")
                            && content.contains("<true/>");
                        items.push(StartupItem {
                            name,
                            path: path.to_string_lossy().to_string(),
                            enabled: !disabled,
                            source: "launchagent".into(),
                        });
                    }
                }
            }
        }
    }
    Ok(items)
}

// ── Toggle startup item ──────────────────────────────────────────────
#[tauri::command]
pub fn toggle_startup_item(name: String, enabled: bool, source: String) -> Result<bool, String> {
    toggle_impl(&name, enabled, &source)
}

#[cfg(windows)]
fn toggle_impl(name: &str, enabled: bool, source: &str) -> Result<bool, String> {
    match source {
        "registry" => toggle_registry_item(name, enabled),
        "folder" => {
            if let Some(folder) = get_startup_folder() {
                toggle_folder_item(&folder, name, enabled)
            } else {
                Err("Startup folder not found".into())
            }
        }
        _ => Err(format!("Unknown source type: {}", source)),
    }
}

#[cfg(windows)]
fn toggle_registry_item(name: &str, enabled: bool) -> Result<bool, String> {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS, KEY_READ};
    use winreg::RegKey;

    let run_key = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";

    if !enabled {
        // Move from Run to a disabled backup key
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(run_key, KEY_ALL_ACCESS)
            .map_err(|e| e.to_string())?;

        let value: String = key.get_value(name).map_err(|e| e.to_string())?;

        let (backup, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(r"SOFTWARE\AuraUpdate\DisabledStartup")
            .map_err(|e| e.to_string())?;
        backup.set_value(name, &value).map_err(|e| e.to_string())?;

        key.delete_value(name).map_err(|e| e.to_string())?;
    } else {
        // Restore from backup
        let backup = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(r"SOFTWARE\AuraUpdate\DisabledStartup", KEY_READ)
            .map_err(|e| e.to_string())?;
        let value: String = backup.get_value(name).map_err(|e| e.to_string())?;

        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(run_key, KEY_ALL_ACCESS)
            .map_err(|e| e.to_string())?;
        key.set_value(name, &value).map_err(|e| e.to_string())?;
    }

    Ok(true)
}

#[cfg(windows)]
fn get_startup_folder() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| {
        d.parent()
            .unwrap_or(&d)
            .join("Roaming")
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
    })
}

#[cfg(windows)]
fn toggle_folder_item(folder: &std::path::Path, name: &str, enabled: bool) -> Result<bool, String> {
    if !folder.exists() {
        return Err(format!("Startup folder not found"));
    }

    let entries = std::fs::read_dir(folder).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        let lower = file_name.to_lowercase();

        // Case-insensitive strip of .disabled then .lnk to get the base name
        let mut base = file_name.clone();
        if lower.ends_with(".disabled") {
            base = base[..base.len() - ".disabled".len()].to_string();
        }
        let base_lower = base.to_lowercase();
        if base_lower.ends_with(".lnk") {
            base = base[..base.len() - ".lnk".len()].to_string();
        }

        if base == name {
            // Resolve the real path for Windows short/long name issues
            let path = entry.path().canonicalize().unwrap_or_else(|_| entry.path());
            if !enabled {
                // Disable: append .disabled
                let disabled_path = path.with_file_name(
                    format!("{}.disabled", path.file_name().unwrap_or_default().to_string_lossy())
                );
                std::fs::rename(&path, &disabled_path).map_err(|e| e.to_string())?;
            } else {
                // Enable: remove .disabled suffix
                let fname = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let fl = fname.to_lowercase();
                if fl.ends_with(".disabled") {
                    let new_name = &fname[..fname.len() - ".disabled".len()];
                    let enabled_path = path.with_file_name(new_name);
                    std::fs::rename(&path, &enabled_path).map_err(|e| e.to_string())?;
                }
            }
            return Ok(true);
        }
    }

    Err(format!("Startup item '{}' not found in startup folder", name))
}

#[cfg(target_os = "linux")]
fn toggle_impl(name: &str, enabled: bool, _source: &str) -> Result<bool, String> {
    if let Some(home) = dirs::home_dir() {
        let autostart = home.join(".config/autostart");
        if let Ok(entries) = std::fs::read_dir(&autostart) {
            for entry in entries.flatten() {
                let path = entry.path();
                let content = std::fs::read_to_string(&path).unwrap_or_default();
                let item_name = content
                    .lines()
                    .find(|l| l.starts_with("Name="))
                    .map(|l| l.strip_prefix("Name=").unwrap_or("").to_string())
                    .unwrap_or_default();

                if item_name == name || entry.file_name().to_string_lossy().contains(name) {
                    let new_content = if enabled {
                        content
                            .replace("Hidden=true", "Hidden=false")
                            .replace("X-GNOME-Autostart-enabled=false", "X-GNOME-Autostart-enabled=true")
                    } else if content.contains("Hidden=") {
                        content.replace("Hidden=false", "Hidden=true")
                    } else {
                        format!("{content}\nHidden=true\n")
                    };
                    std::fs::write(&path, new_content).map_err(|e| e.to_string())?;
                    return Ok(true);
                }
            }
        }
    }
    Err("Startup item not found".into())
}

#[cfg(target_os = "macos")]
fn toggle_impl(name: &str, enabled: bool, _source: &str) -> Result<bool, String> {
    if let Some(home) = dirs::home_dir() {
        let agents = home.join("Library/LaunchAgents");
        if let Ok(entries) = std::fs::read_dir(&agents) {
            for entry in entries.flatten() {
                if entry.file_name().to_string_lossy().contains(name) {
                    let path = entry.path();
                    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;

                    let new_content = if enabled {
                        content.replace(
                            "<key>Disabled</key>\n\t<true/>",
                            "<key>Disabled</key>\n\t<false/>",
                        )
                    } else {
                        if content.contains("<key>Disabled</key>") {
                            content.replace(
                                "<key>Disabled</key>\n\t<false/>",
                                "<key>Disabled</key>\n\t<true/>",
                            )
                        } else {
                            content.replace(
                                "</dict>",
                                "<key>Disabled</key>\n\t<true/>\n</dict>",
                            )
                        }
                    };
                    std::fs::write(&path, new_content).map_err(|e| e.to_string())?;
                    return Ok(true);
                }
            }
        }
    }
    Err("Launch agent not found".into())
}
