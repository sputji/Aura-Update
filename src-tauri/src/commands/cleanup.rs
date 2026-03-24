use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use std::time::Duration;
use rayon::prelude::*;

use super::config::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupItem {
    pub path: String,
    pub size_bytes: u64,
    pub category: String,   // "temp", "cache", "update_cache", "os_residue"
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupReport {
    pub items: Vec<CleanupItem>,
    pub total_bytes: u64,
}

// ── Scan temp / cache files ──────────────────────────────────────────
#[tauri::command]
pub async fn scan_cleanup() -> Result<CleanupReport, String> {
    let mut items = Vec::new();

    #[cfg(windows)]
    {
        scan_dir(&mut items, &std::env::temp_dir(), "temp", "Temporary files");
        if let Some(windir) = std::env::var_os("SystemRoot") {
            let wintemp = PathBuf::from(&windir).join("Temp");
            scan_dir(&mut items, &wintemp, "temp", "Windows Temp");
            let sw_dist = PathBuf::from(&windir).join("SoftwareDistribution").join("Download");
            scan_dir(&mut items, &sw_dist, "update_cache", "Windows Update cache");
            // Old Windows Update logs (safe to remove)
            let wu_logs = PathBuf::from(&windir).join("Logs").join("WindowsUpdate");
            scan_dir(&mut items, &wu_logs, "temp", "Windows Update logs");
        }
        // Thumbnail database files (Explorer folder — only .db files, not the whole dir)
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let explorer_dir = PathBuf::from(&local).join("Microsoft").join("Windows").join("Explorer");
            scan_files_matching(&mut items, &explorer_dir, "cache", "Thumbnail cache", |name| {
                name.starts_with("thumbcache_") && name.ends_with(".db")
            });
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            scan_dir(&mut items, &home.join("Library/Caches"), "cache", "User caches");
        }
        scan_dir(&mut items, &PathBuf::from("/tmp"), "temp", "System temp");
    }

    #[cfg(target_os = "linux")]
    {
        scan_dir(&mut items, &PathBuf::from("/tmp"), "temp", "System temp");
        scan_dir(&mut items, &PathBuf::from("/var/tmp"), "temp", "Var temp");
        if PathBuf::from("/var/cache/apt/archives").exists() {
            scan_dir(&mut items, &PathBuf::from("/var/cache/apt/archives"), "update_cache", "APT cache");
        }
        if let Some(home) = dirs::home_dir() {
            scan_dir(&mut items, &home.join(".cache"), "cache", "User cache");
        }
    }

    let total: u64 = items.iter().map(|i: &CleanupItem| i.size_bytes).sum();
    Ok(CleanupReport { items, total_bytes: total })
}

fn scan_dir(items: &mut Vec<CleanupItem>, dir: &Path, category: &str, desc: &str) {
    if !dir.exists() { return; }
    let size = dir_size(dir);
    if size > 0 {
        items.push(CleanupItem {
            path: dir.to_string_lossy().to_string(),
            size_bytes: size,
            category: category.to_string(),
            description: desc.to_string(),
        });
    }
}

/// Quick temp size check — returns total bytes in temp directories.
/// Used for the > 1 GB popup alert at startup.
#[tauri::command]
pub fn check_temp_size() -> u64 {
    let mut total: u64 = 0;

    #[cfg(windows)]
    {
        total += dir_size_safe(&std::env::temp_dir());
        if let Some(windir) = std::env::var_os("SystemRoot") {
            total += dir_size_safe(&PathBuf::from(&windir).join("Temp"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        total += dir_size_safe(&PathBuf::from("/tmp"));
        if let Some(home) = dirs::home_dir() {
            total += dir_size_safe(&home.join("Library/Caches"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        total += dir_size_safe(&PathBuf::from("/tmp"));
        total += dir_size_safe(&PathBuf::from("/var/tmp"));
    }

    total
}

fn dir_size_safe(path: &Path) -> u64 {
    if path.exists() { dir_size(path) } else { 0 }
}

/// Scan individual files in `dir` that match `predicate`, adding each as a separate item.
/// Used for directories that must not be deleted wholesale (e.g. thumbnail cache).
#[cfg(windows)]
fn scan_files_matching<F>(
    items: &mut Vec<CleanupItem>,
    dir: &Path,
    category: &str,
    desc: &str,
    predicate: F,
) where
    F: Fn(&str) -> bool,
{
    if !dir.exists() { return; }
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_file() {
                let name = p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                if predicate(&name) {
                    paths.push(p);
                }
            }
        }
        for p in paths {
            if let Ok(size) = p.metadata().map(|m| m.len()) {
                if size > 0 {
                    items.push(CleanupItem {
                        path: p.to_string_lossy().to_string(),
                        size_bytes: size,
                        category: category.to_string(),
                        description: desc.to_string(),
                    });
                }
            }
        }
    }
}

fn dir_size(path: &Path) -> u64 {
    let entries: Vec<_> = std::fs::read_dir(path)
        .map(|rd| rd.flatten().collect())
        .unwrap_or_default();

    entries.par_iter().map(|entry| {
        let meta = entry.metadata();
        if let Ok(m) = meta {
            if m.is_file() {
                m.len()
            } else if m.is_dir() {
                dir_size(&entry.path())
            } else { 0 }
        } else { 0 }
    }).sum()
}

// ── Clean temp / cache files ─────────────────────────────────────────
#[tauri::command]
pub async fn run_cleanup(state: tauri::State<'_, AppState>, paths: Vec<String>) -> Result<u64, String> {
    let mut freed: u64 = 0;
    for p in &paths {
        let path = PathBuf::from(p);
        if !path.exists() { continue; }
        // Individual file (e.g. thumbcache_*.db)
        if path.is_file() {
            let size = path.metadata().map(|m| m.len()).unwrap_or(0);
            if std::fs::remove_file(&path).is_ok() {
                freed += size;
            }
            continue;
        }
        // Directory — delete its contents, not the directory itself
        if let Ok(entries) = std::fs::read_dir(&path) {
            for entry in entries.flatten() {
                let size = if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                    dir_size(&entry.path())
                } else {
                    entry.metadata().map(|m| m.len()).unwrap_or(0)
                };
                if std::fs::remove_dir_all(entry.path()).is_ok()
                    || std::fs::remove_file(entry.path()).is_ok()
                {
                    freed += size;
                }
            }
        }
    }
    // Save to cleanup history
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.cleanup_history.push(freed);
        if cfg.cleanup_history.len() > 10 {
            let excess = cfg.cleanup_history.len() - 10;
            cfg.cleanup_history.drain(0..excess);
        }
        super::config::save_config(&state.data_dir, &cfg);
    }
    Ok(freed)
}

// ── OS residue scan ──────────────────────────────────────────────────
#[tauri::command]
pub async fn scan_os_residues() -> Result<CleanupReport, String> {
    let mut items = Vec::new();

    #[cfg(windows)]
    {
        // Check DISM component store (2-minute timeout for analysis)
        let dism_analyze = Command::new("dism")
            .args(["/online", "/cleanup-image", "/analyzecomponentstore", "/quiet"])
            .creation_flags(0x0800_0000)
            .output();
        if let Ok(Ok(o)) = tokio::time::timeout(Duration::from_secs(120), dism_analyze).await {
            let text = String::from_utf8_lossy(&o.stdout);
            if text.contains("Component Store Cleanup Recommended : Yes")
                || text.contains("Nettoyage du magasin de composants recommandé : Oui")
            {
                items.push(CleanupItem {
                    path: "dism-component-store".into(),
                    size_bytes: 0,
                    category: "os_residue".into(),
                    description: "Windows component store (old system files)".into(),
                });
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // APT autoremove
        if Command::new("which").arg("apt-get").output().await
            .map(|o| o.status.success()).unwrap_or(false)
        {
            let out = Command::new("apt-get")
                .args(["autoremove", "--dry-run"])
                .output()
                .await;
            if let Ok(o) = out {
                let text = String::from_utf8_lossy(&o.stdout);
                let count = text.lines()
                    .filter(|l| l.starts_with("Remv") || l.starts_with("Removing"))
                    .count();
                if count > 0 {
                    items.push(CleanupItem {
                        path: "apt-autoremove".into(),
                        size_bytes: 0,
                        category: "os_residue".into(),
                        description: format!("{count} orphaned packages (apt autoremove)"),
                    });
                }
            }
        }
        // Pacman orphans
        if Command::new("which").arg("pacman").output().await
            .map(|o| o.status.success()).unwrap_or(false)
        {
            let out = Command::new("pacman").args(["-Qtdq"]).output().await;
            if let Ok(o) = out {
                let text = String::from_utf8_lossy(&o.stdout);
                let count = text.lines().filter(|l| !l.trim().is_empty()).count();
                if count > 0 {
                    items.push(CleanupItem {
                        path: "pacman-orphans".into(),
                        size_bytes: 0,
                        category: "os_residue".into(),
                        description: format!("{count} orphaned packages (pacman)"),
                    });
                }
            }
        }
    }

    let total: u64 = items.iter().map(|i: &CleanupItem| i.size_bytes).sum();
    Ok(CleanupReport { items, total_bytes: total })
}

// ── Clean OS residues ────────────────────────────────────────────────
#[tauri::command]
pub async fn clean_os_residues(state: tauri::State<'_, AppState>, residues: Vec<String>) -> Result<String, String> {
    let mut results: Vec<String> = Vec::new();

    for r in &residues {
        match r.as_str() {
            #[cfg(windows)]
            "dism-component-store" => {
                // 3-minute timeout to prevent DISM from hanging the autopilot
                let dism_future = Command::new("dism")
                    .args(["/online", "/cleanup-image", "/startcomponentcleanup", "/quiet", "/norestart"])
                    .creation_flags(0x0800_0000)
                    .output();
                match tokio::time::timeout(Duration::from_secs(180), dism_future).await {
                    Ok(Ok(o)) if o.status.success() => results.push("DISM cleanup complete".to_string()),
                    Ok(Ok(o)) => results.push(format!("DISM error: {}", String::from_utf8_lossy(&o.stderr))),
                    Ok(Err(e)) => results.push(format!("DISM error: {e}")),
                    Err(_) => results.push("DISM cleanup timed out after 3 minutes".to_string()),
                }
            }
            #[cfg(target_os = "linux")]
            "apt-autoremove" => {
                let out = Command::new("apt-get")
                    .args(["autoremove", "-y"])
                    .output()
                    .await;
                match out {
                    Ok(o) if o.status.success() => results.push("apt autoremove complete".to_string()),
                    _ => results.push("apt autoremove failed".to_string()),
                }
            }
            #[cfg(target_os = "linux")]
            "pacman-orphans" => {
                let orphans = Command::new("pacman").args(["-Qtdq"]).output().await;
                if let Ok(o) = orphans {
                    let pkgs = String::from_utf8_lossy(&o.stdout);
                    let pkg_list: Vec<&str> = pkgs.lines().filter(|l| !l.trim().is_empty()).collect();
                    if !pkg_list.is_empty() {
                        let mut cmd = Command::new("pacman");
                        cmd.args(["-Rns", "--noconfirm"]);
                        for p in &pkg_list { cmd.arg(p); }
                        let _ = cmd.output().await;
                        results.push(format!("Removed {} orphaned packages", pkg_list.len()));
                    }
                }
            }
            _ => {}
        }
    }

    // Save to cleanup history (record a nominal 0 bytes for OS residue operations)
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.cleanup_history.push(0);
        if cfg.cleanup_history.len() > 10 {
            let excess = cfg.cleanup_history.len() - 10;
            cfg.cleanup_history.drain(0..excess);
        }
        super::config::save_config(&state.data_dir, &cfg);
    }

    Ok(results.join("\n"))
}

// ── Scan browser caches ──────────────────────────────────────────────
/// Collects browser cache items from a list of (path, description) pairs.
fn collect_browser_items(items: &mut Vec<CleanupItem>, browsers: &[(PathBuf, &str)]) {
    for (path, desc) in browsers {
        if path.exists() {
            let size = dir_size(path);
            if size > 0 {
                items.push(CleanupItem {
                    path: path.to_string_lossy().to_string(),
                    size_bytes: size,
                    category: "browser_cache".to_string(),
                    description: desc.to_string(),
                });
            }
        }
    }
}

/// Filter spec for granular browser cleanup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCleanupFilter {
    pub browser: String,    // "chrome", "edge", "firefox", "brave", "opera", "opera_gx"
    pub cache: bool,
    pub history: bool,
    pub cookies: bool,
    pub sessions: bool,
}

/// Detect which browsers are actually installed on this machine.
#[tauri::command]
pub fn detect_installed_browsers() -> Vec<String> {
    let all = vec!["chrome", "edge", "firefox", "brave", "opera", "opera_gx"];
    all.into_iter()
        .filter(|b| is_browser_installed(b))
        .map(|b| b.to_string())
        .collect()
}

fn is_browser_installed(browser: &str) -> bool {
    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
        let roaming = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();
        let pf = std::env::var_os("ProgramFiles").map(PathBuf::from).unwrap_or_default();
        let pf86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from).unwrap_or_default();
        match browser {
            "chrome" => local.join("Google\\Chrome\\User Data").exists()
                || pf.join("Google\\Chrome\\Application\\chrome.exe").exists()
                || pf86.join("Google\\Chrome\\Application\\chrome.exe").exists(),
            "edge" => local.join("Microsoft\\Edge\\User Data").exists()
                || pf.join("Microsoft\\Edge\\Application\\msedge.exe").exists()
                || pf86.join("Microsoft\\Edge\\Application\\msedge.exe").exists(),
            "firefox" => roaming.join("Mozilla\\Firefox\\Profiles").exists()
                || pf.join("Mozilla Firefox\\firefox.exe").exists()
                || pf86.join("Mozilla Firefox\\firefox.exe").exists(),
            "brave" => local.join("BraveSoftware\\Brave-Browser\\User Data").exists()
                || pf.join("BraveSoftware\\Brave-Browser\\Application\\brave.exe").exists(),
            "opera" => local.join("Opera Software\\Opera Stable").exists()
                || pf.join("Opera\\opera.exe").exists(),
            "opera_gx" => local.join("Opera Software\\Opera GX Stable").exists(),
            _ => false,
        }
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        match browser {
            "chrome" => std::path::Path::new("/Applications/Google Chrome.app").exists()
                || home.join("Library/Application Support/Google/Chrome").exists(),
            "edge" => std::path::Path::new("/Applications/Microsoft Edge.app").exists()
                || home.join("Library/Application Support/Microsoft Edge").exists(),
            "firefox" => std::path::Path::new("/Applications/Firefox.app").exists()
                || home.join("Library/Application Support/Firefox/Profiles").exists(),
            "brave" => std::path::Path::new("/Applications/Brave Browser.app").exists()
                || home.join("Library/Application Support/BraveSoftware/Brave-Browser").exists(),
            "opera" => std::path::Path::new("/Applications/Opera.app").exists()
                || home.join("Library/Application Support/com.operasoftware.Opera").exists(),
            "opera_gx" => std::path::Path::new("/Applications/Opera GX.app").exists(),
            _ => false,
        }
    }
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        match browser {
            "chrome" => std::path::Path::new("/usr/bin/google-chrome").exists()
                || std::path::Path::new("/usr/bin/google-chrome-stable").exists()
                || home.join(".config/google-chrome").exists(),
            "edge" => std::path::Path::new("/usr/bin/microsoft-edge").exists()
                || home.join(".config/microsoft-edge").exists(),
            "firefox" => std::path::Path::new("/usr/bin/firefox").exists()
                || home.join(".mozilla/firefox").exists(),
            "brave" => std::path::Path::new("/usr/bin/brave-browser").exists()
                || home.join(".config/BraveSoftware/Brave-Browser").exists(),
            "opera" => std::path::Path::new("/usr/bin/opera").exists()
                || home.join(".config/opera").exists(),
            "opera_gx" => false, // Opera GX n'existe pas sur Linux
            _ => false,
        }
    }
}

/// Scan browser data with granular filters (cache, history, cookies, sessions).
#[tauri::command]
pub async fn scan_browser_granular(filters: Vec<BrowserCleanupFilter>) -> Result<CleanupReport, String> {
    let mut items = Vec::new();

    for filter in &filters {
        let paths = get_browser_data_paths(&filter.browser, filter.cache, filter.history, filter.cookies, filter.sessions);
        for (path, desc) in paths {
            if path.exists() {
                let size = if path.is_file() {
                    path.metadata().map(|m| m.len()).unwrap_or(0)
                } else {
                    dir_size(&path)
                };
                if size > 0 {
                    items.push(CleanupItem {
                        path: path.to_string_lossy().to_string(),
                        size_bytes: size,
                        category: format!("browser_{}", filter.browser),
                        description: desc,
                    });
                }
            }
        }
    }

    let total: u64 = items.iter().map(|i| i.size_bytes).sum();
    Ok(CleanupReport { items, total_bytes: total })
}

fn get_browser_data_paths(browser: &str, cache: bool, history: bool, cookies: bool, sessions: bool) -> Vec<(PathBuf, String)> {
    let mut paths = Vec::new();

    #[cfg(windows)]
    {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_default();
        let roaming = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_default();

        match browser {
            "chrome" => {
                let base = local.join("Google\\Chrome\\User Data\\Default");
                if cache   { paths.push((base.join("Cache\\Cache_Data"), "Chrome — Cache".into())); paths.push((base.join("Code Cache"), "Chrome — Code Cache".into())); }
                if history { paths.push((base.join("History"), "Chrome — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Chrome — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Chrome — Sessions".into())); }
            }
            "edge" => {
                let base = local.join("Microsoft\\Edge\\User Data\\Default");
                if cache   { paths.push((base.join("Cache\\Cache_Data"), "Edge — Cache".into())); paths.push((base.join("Code Cache"), "Edge — Code Cache".into())); }
                if history { paths.push((base.join("History"), "Edge — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Edge — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Edge — Sessions".into())); }
            }
            "firefox" => {
                let profiles = roaming.join("Mozilla\\Firefox\\Profiles");
                if profiles.exists() {
                    if let Ok(entries) = std::fs::read_dir(&profiles) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if p.is_dir() {
                                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                                if cache   { paths.push((p.join("cache2"), format!("Firefox ({name}) — Cache"))); }
                                if history { paths.push((p.join("places.sqlite"), format!("Firefox ({name}) — Historique"))); }
                                if cookies { paths.push((p.join("cookies.sqlite"), format!("Firefox ({name}) — Cookies"))); }
                                if sessions { paths.push((p.join("sessionstore-backups"), format!("Firefox ({name}) — Sessions"))); }
                            }
                        }
                    }
                }
            }
            "brave" => {
                let base = local.join("BraveSoftware\\Brave-Browser\\User Data\\Default");
                if cache   { paths.push((base.join("Cache\\Cache_Data"), "Brave — Cache".into())); }
                if history { paths.push((base.join("History"), "Brave — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Brave — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Brave — Sessions".into())); }
            }
            "opera" => {
                let base = local.join("Opera Software\\Opera Stable");
                if cache   { paths.push((base.join("Cache\\Cache_Data"), "Opera — Cache".into())); }
                if history { paths.push((base.join("History"), "Opera — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Opera — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Opera — Sessions".into())); }
            }
            "opera_gx" => {
                let base = local.join("Opera Software\\Opera GX Stable");
                if cache   { paths.push((base.join("Cache\\Cache_Data"), "Opera GX — Cache".into())); }
                if history { paths.push((base.join("History"), "Opera GX — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Opera GX — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Opera GX — Sessions".into())); }
            }
            _ => {}
        }
    }

    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        match browser {
            "chrome" => {
                let base = home.join("Library/Application Support/Google/Chrome/Default");
                if cache   { paths.push((home.join("Library/Caches/Google/Chrome/Default/Cache"), "Chrome — Cache".into())); }
                if history { paths.push((base.join("History"), "Chrome — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Chrome — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Chrome — Sessions".into())); }
            }
            "edge" => {
                let base = home.join("Library/Application Support/Microsoft Edge/Default");
                if cache   { paths.push((home.join("Library/Caches/com.microsoft.edgemac"), "Edge — Cache".into())); }
                if history { paths.push((base.join("History"), "Edge — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Edge — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Edge — Sessions".into())); }
            }
            "firefox" => {
                let profiles = home.join("Library/Application Support/Firefox/Profiles");
                if profiles.exists() {
                    if let Ok(entries) = std::fs::read_dir(&profiles) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if p.is_dir() {
                                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                                if cache   { paths.push((p.join("cache2"), format!("Firefox ({name}) — Cache"))); }
                                if history { paths.push((p.join("places.sqlite"), format!("Firefox ({name}) — Historique"))); }
                                if cookies { paths.push((p.join("cookies.sqlite"), format!("Firefox ({name}) — Cookies"))); }
                                if sessions { paths.push((p.join("sessionstore-backups"), format!("Firefox ({name}) — Sessions"))); }
                            }
                        }
                    }
                }
            }
            "brave" => {
                let base = home.join("Library/Application Support/BraveSoftware/Brave-Browser/Default");
                if cache   { paths.push((home.join("Library/Caches/com.brave.Browser"), "Brave — Cache".into())); }
                if history { paths.push((base.join("History"), "Brave — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Brave — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Brave — Sessions".into())); }
            }
            "opera" => {
                let base = home.join("Library/Application Support/com.operasoftware.Opera");
                if cache   { paths.push((home.join("Library/Caches/com.operasoftware.Opera"), "Opera — Cache".into())); }
                if history { paths.push((base.join("History"), "Opera — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Opera — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Opera — Sessions".into())); }
            }
            _ => {}
        }
    }

    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir().unwrap_or_default();
        match browser {
            "chrome" => {
                let base = home.join(".config/google-chrome/Default");
                if cache   { paths.push((home.join(".cache/google-chrome/Default/Cache"), "Chrome — Cache".into())); }
                if history { paths.push((base.join("History"), "Chrome — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Chrome — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Chrome — Sessions".into())); }
            }
            "edge" => {
                let base = home.join(".config/microsoft-edge/Default");
                if cache   { paths.push((home.join(".cache/microsoft-edge/Default/Cache"), "Edge — Cache".into())); }
                if history { paths.push((base.join("History"), "Edge — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Edge — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Edge — Sessions".into())); }
            }
            "firefox" => {
                let profiles = home.join(".mozilla/firefox");
                if profiles.exists() {
                    if let Ok(entries) = std::fs::read_dir(&profiles) {
                        for entry in entries.flatten() {
                            let p = entry.path();
                            if p.is_dir() && p.to_string_lossy().contains(".default") {
                                let name = p.file_name().unwrap_or_default().to_string_lossy().to_string();
                                if cache   { paths.push((p.join("cache2"), format!("Firefox ({name}) — Cache"))); }
                                if history { paths.push((p.join("places.sqlite"), format!("Firefox ({name}) — Historique"))); }
                                if cookies { paths.push((p.join("cookies.sqlite"), format!("Firefox ({name}) — Cookies"))); }
                                if sessions { paths.push((p.join("sessionstore-backups"), format!("Firefox ({name}) — Sessions"))); }
                            }
                        }
                    }
                }
            }
            "brave" => {
                let base = home.join(".config/BraveSoftware/Brave-Browser/Default");
                if cache   { paths.push((home.join(".cache/BraveSoftware/Brave-Browser/Default/Cache"), "Brave — Cache".into())); }
                if history { paths.push((base.join("History"), "Brave — Historique".into())); }
                if cookies { paths.push((base.join("Cookies"), "Brave — Cookies".into())); }
                if sessions { paths.push((base.join("Sessions"), "Brave — Sessions".into())); }
            }
            "opera" => {
                if cache   { paths.push((home.join(".cache/opera"), "Opera — Cache".into())); }
                if history { paths.push((home.join(".config/opera/History"), "Opera — Historique".into())); }
                if cookies { paths.push((home.join(".config/opera/Cookies"), "Opera — Cookies".into())); }
                if sessions { paths.push((home.join(".config/opera/Sessions"), "Opera — Sessions".into())); }
            }
            _ => {}
        }
    }

    paths
}

#[tauri::command]
pub async fn scan_browser_caches() -> Result<CleanupReport, String> {
    let mut items = Vec::new();

    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            collect_browser_items(&mut items, &[
                (local.join("Google\\Chrome\\User Data\\Default\\Cache\\Cache_Data"), "Chrome Cache"),
                (local.join("Google\\Chrome\\User Data\\Default\\Code Cache"), "Chrome Code Cache"),
                (local.join("Microsoft\\Edge\\User Data\\Default\\Cache\\Cache_Data"), "Edge Cache"),
                (local.join("Microsoft\\Edge\\User Data\\Default\\Code Cache"), "Edge Code Cache"),
                (local.join("BraveSoftware\\Brave-Browser\\User Data\\Default\\Cache\\Cache_Data"), "Brave Cache"),
                (local.join("Mozilla\\Firefox\\Profiles"), "Firefox Profiles Cache"),
                (local.join("Opera Software\\Opera Stable\\Cache\\Cache_Data"), "Opera Cache"),
                (local.join("Opera Software\\Opera GX Stable\\Cache\\Cache_Data"), "Opera GX Cache"),
            ]);
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            collect_browser_items(&mut items, &[
                (home.join("Library/Caches/Google/Chrome/Default/Cache"), "Chrome Cache"),
                (home.join("Library/Caches/com.microsoft.edgemac"), "Edge Cache"),
                (home.join("Library/Caches/com.brave.Browser"), "Brave Cache"),
                (home.join("Library/Caches/Firefox/Profiles"), "Firefox Cache"),
                (home.join("Library/Caches/com.operasoftware.Opera"), "Opera Cache"),
            ]);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            collect_browser_items(&mut items, &[
                (home.join(".cache/google-chrome/Default/Cache"), "Chrome Cache"),
                (home.join(".cache/microsoft-edge/Default/Cache"), "Edge Cache"),
                (home.join(".cache/BraveSoftware/Brave-Browser/Default/Cache"), "Brave Cache"),
                (home.join(".cache/mozilla/firefox"), "Firefox Cache"),
                (home.join(".cache/opera"), "Opera Cache"),
            ]);
        }
    }

    let total: u64 = items.iter().map(|i: &CleanupItem| i.size_bytes).sum();
    Ok(CleanupReport { items, total_bytes: total })
}

// ── Known bloatware list ─────────────────────────────────────────────
const BLOATWARE_LIST: &[&str] = &[
    "Microsoft.BingNews",
    "Microsoft.BingWeather",
    "Microsoft.GetHelp",
    "Microsoft.Getstarted",
    "Microsoft.MicrosoftOfficeHub",
    "Microsoft.MicrosoftSolitaireCollection",
    "Microsoft.People",
    "Microsoft.SkypeApp",
    "Microsoft.WindowsFeedbackHub",
    "Microsoft.Xbox.TCUI",
    "Microsoft.XboxApp",
    "Microsoft.XboxGameOverlay",
    "Microsoft.XboxGamingOverlay",
    "Microsoft.XboxIdentityProvider",
    "Microsoft.XboxSpeechToTextOverlay",
    "Microsoft.YourPhone",
    "Microsoft.ZuneMusic",
    "Microsoft.ZuneVideo",
    "Microsoft.MixedReality.Portal",
    "Microsoft.WindowsMaps",
    "Microsoft.Todos",
    "Clipchamp.Clipchamp",
    "Microsoft.549981C3F5F10", // Cortana
    "king.com.CandyCrushSaga",
    "king.com.CandyCrushSodaSaga",
    "SpotifyAB.SpotifyMusic",
    "Disney.37853FC22B2CE",
    "BytedancePte.Ltd.TikTok",
];

/// macOS: known bloatware / preinstalled apps removable from /Applications
#[cfg(target_os = "macos")]
const BLOATWARE_MACOS: &[(&str, &str)] = &[
    ("GarageBand", "GarageBand.app"),
    ("iMovie", "iMovie.app"),
    ("Keynote", "Keynote.app"),
    ("Numbers", "Numbers.app"),
    ("Pages", "Pages.app"),
];

/// Linux: known snap/flatpak/apt bloatware
#[cfg(target_os = "linux")]
const BLOATWARE_LINUX: &[(&str, &str)] = &[
    ("gnome-games", "gnome-games"),
    ("aisleriot", "aisleriot"),
    ("gnome-mahjongg", "gnome-mahjongg"),
    ("gnome-mines", "gnome-mines"),
    ("gnome-sudoku", "gnome-sudoku"),
    ("thunderbird", "thunderbird"),
    ("libreoffice-common", "libreoffice-common"),
    ("rhythmbox", "rhythmbox"),
    ("totem", "totem"),
    ("cheese", "cheese"),
    ("shotwell", "shotwell"),
];

/// Return the list of known bloatwares with their install status.
#[tauri::command]
pub async fn list_bloatwares() -> Result<Vec<BloatwareInfo>, String> {
    let mut result = Vec::new();

    #[cfg(windows)]
    {
        // Batch all checks into a single PowerShell call (29 separate spawns → 1)
        let checks: Vec<String> = BLOATWARE_LIST.iter().map(|&pkg| {
            format!(
                "if (Get-AppxPackage -Name '*{}*' -ErrorAction SilentlyContinue) {{ Write-Output '{}=installed' }} else {{ Write-Output '{}=not_found' }}",
                pkg, pkg, pkg
            )
        }).collect();
        let script = checks.join("; ");
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .creation_flags(0x0800_0000)
            .output()
            .await;
        let output_text = out
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let installed_set: std::collections::HashSet<&str> = output_text
            .lines()
            .filter(|l| l.ends_with("=installed"))
            .filter_map(|l| l.strip_suffix("=installed"))
            .collect();

        for &pkg in BLOATWARE_LIST {
            result.push(BloatwareInfo {
                package: pkg.to_string(),
                label: pkg.split('.').last().unwrap_or(pkg).to_string(),
                installed: installed_set.contains(pkg),
            });
        }
    }

    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        {
            for &(label, app_name) in BLOATWARE_MACOS {
                let path = format!("/Applications/{}", app_name);
                let installed = std::path::Path::new(&path).exists();
                result.push(BloatwareInfo {
                    package: app_name.to_string(),
                    label: label.to_string(),
                    installed,
                });
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Check dpkg for installed packages
            let out = Command::new("dpkg")
                .args(["--get-selections"])
                .output()
                .await;
            let installed_text = out
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            for &(label, pkg) in BLOATWARE_LINUX {
                let installed = installed_text.lines().any(|l| l.starts_with(pkg) && l.contains("install"));
                result.push(BloatwareInfo {
                    package: pkg.to_string(),
                    label: label.to_string(),
                    installed,
                });
            }
        }
    }

    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloatwareInfo {
    pub package: String,
    pub label: String,
    pub installed: bool,
}

// ── Purge selected bloatwares ────────────────────────────────────────
#[tauri::command]
pub async fn purge_bloatwares(selection: Vec<String>) -> Result<String, String> {
    let mut removed_count = 0;

    #[cfg(windows)]
    {
        let targets: Vec<&str> = if selection.is_empty() {
            BLOATWARE_LIST.to_vec()
        } else {
            selection.iter()
                .filter(|s| BLOATWARE_LIST.contains(&s.as_str()))
                .map(|s| s.as_str())
                .collect()
        };

        let ps_script: String = targets.iter().map(|pkg| {
            format!("Get-AppxPackage -Name *{}* | Remove-AppxPackage -ErrorAction SilentlyContinue", pkg)
        }).collect::<Vec<_>>().join("; ");
        let out = Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps_script])
            .creation_flags(0x0800_0000)
            .output()
            .await;
        if out.map(|o| o.status.success()).unwrap_or(false) {
            removed_count = targets.len();
        }
    }

    #[cfg(target_os = "macos")]
    {
        let known: Vec<&str> = BLOATWARE_MACOS.iter().map(|&(_, app)| app).collect();
        let targets: Vec<&str> = if selection.is_empty() {
            known.clone()
        } else {
            selection.iter()
                .filter(|s| known.contains(&s.as_str()))
                .map(|s| s.as_str())
                .collect()
        };

        for app_name in &targets {
            let path = format!("/Applications/{}", app_name);
            if std::path::Path::new(&path).exists() {
                let out = Command::new("rm")
                    .args(["-rf", &path])
                    .output()
                    .await;
                if out.map(|o| o.status.success()).unwrap_or(false) {
                    removed_count += 1;
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let known: Vec<&str> = BLOATWARE_LINUX.iter().map(|&(_, pkg)| pkg).collect();
        let targets: Vec<&str> = if selection.is_empty() {
            known.clone()
        } else {
            selection.iter()
                .filter(|s| known.contains(&s.as_str()))
                .map(|s| s.as_str())
                .collect()
        };

        if !targets.is_empty() {
            let mut args = vec!["purge", "-y"];
            args.extend(targets.iter());
            let out = Command::new("sudo")
                .args(["apt-get"])
                .args(&args)
                .output()
                .await;
            if out.map(|o| o.status.success()).unwrap_or(false) {
                removed_count = targets.len();
            }
        }
    }

    Ok(format!("Purge terminée : {} applications supprimées.", removed_count))
}

// ── Disable telemetry services ───────────────────────────────────────
#[tauri::command]
pub async fn disable_telemetry() -> Result<Vec<String>, String> {
    let mut disabled = Vec::new();

    #[cfg(windows)]
    {
        let services = [
            "DiagTrack",           // Connected User Experiences and Telemetry
            "dmwappushservice",    // WAP Push Message Routing Service
            "diagnosticshub.standardcollector.service",
            "WerSvc",             // Windows Error Reporting
        ];
        for svc in &services {
            let mut c1 = Command::new("sc");
            c1.args(["stop", svc]);
            c1.creation_flags(0x0800_0000);
            let out = c1.output().await;
            let mut c2 = Command::new("sc");
            c2.args(["config", svc, "start=", "disabled"]);
            c2.creation_flags(0x0800_0000);
            let _ = c2.output().await;
            if let Ok(o) = out {
                if o.status.success() {
                    disabled.push(svc.to_string());
                }
            }
        }
    }

    Ok(disabled)
}

/// Granular telemetry control: disable/enable per-category.
/// Categories: "windows", "office", "nvidia", "browsers", "tracking"
#[tauri::command]
pub async fn disable_telemetry_granular(category: String, disable: bool) -> Result<Vec<String>, String> {
    let mut results: Vec<String> = Vec::new();

    #[cfg(windows)]
    {
        match category.as_str() {
            "windows" => {
                // ── Services de télémétrie Windows ──
                let services = ["DiagTrack", "dmwappushservice", "diagnosticshub.standardcollector.service", "WerSvc"];
                let action = if disable { "disabled" } else { "demand" };
                for svc in &services {
                    if disable {
                        let mut c = Command::new("sc");
                        c.args(["stop", svc]).creation_flags(0x0800_0000);
                        let _ = c.output().await;
                    }
                    let mut c = Command::new("sc");
                    c.args(["config", svc, "start=", action]).creation_flags(0x0800_0000);
                    if let Ok(o) = c.output().await {
                        if o.status.success() {
                            results.push(format!("{svc}: {}", if disable { "disabled" } else { "enabled" }));
                        }
                    }
                }
                // Registry: AllowTelemetry
                let reg_val = if disable { "0" } else { "3" };
                let _ = Command::new("reg")
                    .args(["add", r"HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection", "/v", "AllowTelemetry", "/t", "REG_DWORD", "/d", reg_val, "/f"])
                    .creation_flags(0x0800_0000)
                    .output().await;
                results.push(format!("AllowTelemetry: {reg_val}"));
            }
            "office" => {
                let scripts = if disable {
                    vec![
                        r"Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Office\Common\ClientTelemetry' -Name 'DisableTelemetry' -Value 1 -Type DWord -Force -ErrorAction SilentlyContinue",
                        r"Set-ItemProperty -Path 'HKCU:\Software\Microsoft\Office\16.0\Common\ClientTelemetry' -Name 'SendTelemetry' -Value 3 -Type DWord -Force -ErrorAction SilentlyContinue",
                    ]
                } else {
                    vec![
                        r"Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Office\Common\ClientTelemetry' -Name 'DisableTelemetry' -ErrorAction SilentlyContinue",
                        r"Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Office\16.0\Common\ClientTelemetry' -Name 'SendTelemetry' -ErrorAction SilentlyContinue",
                    ]
                };
                let script = scripts.join("; ");
                let _ = Command::new("powershell")
                    .args(["-NoProfile", "-Command", &script])
                    .creation_flags(0x0800_0000)
                    .output().await;
                results.push(format!("Office telemetry: {}", if disable { "disabled" } else { "enabled" }));
            }
            "nvidia" => {
                // ── NVIDIA Telemetry Container service ──
                let svc = "NvTelemetryContainer";
                let action = if disable { "disabled" } else { "demand" };
                if disable {
                    let mut c = Command::new("sc");
                    c.args(["stop", svc]).creation_flags(0x0800_0000);
                    let _ = c.output().await;
                }
                let mut c = Command::new("sc");
                c.args(["config", svc, "start=", action]).creation_flags(0x0800_0000);
                if let Ok(o) = c.output().await {
                    if o.status.success() {
                        results.push(format!("{svc}: {}", if disable { "disabled" } else { "enabled" }));
                    } else {
                        results.push(format!("{svc}: not found (skip)"));
                    }
                }
                // Disable NVIDIA scheduled telemetry tasks
                let tasks = ["NvTmMon_{B2FE1952-0186-46C3-BAEC-A80AA35AC5B8}", "NvTmRep_{B2FE1952-0186-46C3-BAEC-A80AA35AC5B8}"];
                for task in &tasks {
                    let act = if disable { "/Disable" } else { "/Enable" };
                    let _ = Command::new("schtasks")
                        .args(["/Change", "/TN", task, act])
                        .creation_flags(0x0800_0000)
                        .output().await;
                }
                // Registry: opt out of NVIDIA telemetry
                let val = if disable { "0" } else { "1" };
                let _ = Command::new("reg")
                    .args(["add", r"HKLM\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client", "/v", "OptInOrOutPreference", "/t", "REG_DWORD", "/d", val, "/f"])
                    .creation_flags(0x0800_0000)
                    .output().await;
                results.push(format!("NVIDIA telemetry: {}", if disable { "disabled" } else { "enabled" }));
            }
            "browsers" => {
                // ── Microsoft Edge telemetry ──
                let edge_keys = [
                    ("MetricsReportingEnabled", if disable { "0" } else { "1" }),
                    ("DiagnosticData", if disable { "0" } else { "2" }),
                    ("SendSiteInfoToImproveServices", if disable { "0" } else { "1" }),
                    ("PersonalizationReportingEnabled", if disable { "0" } else { "1" }),
                ];
                for (name, val) in &edge_keys {
                    let _ = Command::new("reg")
                        .args(["add", r"HKLM\SOFTWARE\Policies\Microsoft\Edge", "/v", name, "/t", "REG_DWORD", "/d", val, "/f"])
                        .creation_flags(0x0800_0000)
                        .output().await;
                }
                results.push(format!("Edge telemetry: {}", if disable { "disabled" } else { "enabled" }));

                // ── Google Chrome telemetry ──
                let chrome_keys = [
                    ("MetricsReportingEnabled", if disable { "0" } else { "1" }),
                    ("SafeBrowsingExtendedReportingEnabled", if disable { "0" } else { "1" }),
                    ("UrlKeyedAnonymizedDataCollectionEnabled", if disable { "0" } else { "1" }),
                    ("SpellCheckServiceEnabled", if disable { "0" } else { "1" }),
                ];
                for (name, val) in &chrome_keys {
                    let _ = Command::new("reg")
                        .args(["add", r"HKLM\SOFTWARE\Policies\Google\Chrome", "/v", name, "/t", "REG_DWORD", "/d", val, "/f"])
                        .creation_flags(0x0800_0000)
                        .output().await;
                }
                results.push(format!("Chrome telemetry: {}", if disable { "disabled" } else { "enabled" }));

                // ── Firefox telemetry ──
                let ff_script = if disable {
                    r#"$ffProfiles = Get-ChildItem "$env:APPDATA\Mozilla\Firefox\Profiles" -Directory -ErrorAction SilentlyContinue; foreach($p in $ffProfiles){ $f = Join-Path $p.FullName 'user.js'; Add-Content -Path $f -Value 'user_pref("toolkit.telemetry.enabled", false);' -ErrorAction SilentlyContinue; Add-Content -Path $f -Value 'user_pref("datareporting.healthreport.uploadEnabled", false);' -ErrorAction SilentlyContinue }"#
                } else {
                    r#"$ffProfiles = Get-ChildItem "$env:APPDATA\Mozilla\Firefox\Profiles" -Directory -ErrorAction SilentlyContinue; foreach($p in $ffProfiles){ $f = Join-Path $p.FullName 'user.js'; if(Test-Path $f){ (Get-Content $f) | Where-Object { $_ -notmatch 'toolkit.telemetry.enabled|datareporting.healthreport.uploadEnabled' } | Set-Content $f -ErrorAction SilentlyContinue }}"#
                };
                let _ = Command::new("powershell")
                    .args(["-NoProfile", "-Command", ff_script])
                    .creation_flags(0x0800_0000)
                    .output().await;
                results.push(format!("Firefox telemetry: {}", if disable { "disabled" } else { "enabled" }));
            }
            "tracking" => {
                // ── Identifiant publicitaire (Advertising ID) ──
                let ad_val = if disable { "0" } else { "1" };
                let _ = Command::new("reg")
                    .args(["add", r"HKCU\Software\Microsoft\Windows\CurrentVersion\AdvertisingInfo", "/v", "Enabled", "/t", "REG_DWORD", "/d", ad_val, "/f"])
                    .creation_flags(0x0800_0000)
                    .output().await;
                let ad_policy_val = if disable { "1" } else { "0" };
                let _ = Command::new("reg")
                    .args(["add", r"HKLM\SOFTWARE\Policies\Microsoft\Windows\AdvertisingInfo", "/v", "DisabledByGroupPolicy", "/t", "REG_DWORD", "/d", ad_policy_val, "/f"])
                    .creation_flags(0x0800_0000)
                    .output().await;
                results.push(format!("Advertising ID: {}", if disable { "disabled" } else { "enabled" }));

                // ── Historique d'activité (Activity History + Timeline) ──
                let activity_val = if disable { "0" } else { "1" };
                for key in ["EnableActivityFeed", "PublishUserActivities", "UploadUserActivities"] {
                    let _ = Command::new("reg")
                        .args(["add", r"HKLM\SOFTWARE\Policies\Microsoft\Windows\System", "/v", key, "/t", "REG_DWORD", "/d", activity_val, "/f"])
                        .creation_flags(0x0800_0000)
                        .output().await;
                }
                results.push(format!("Activity History: {}", if disable { "disabled" } else { "enabled" }));

                // ── Localisation (Location tracking) ──
                let loc_val = if disable { "1" } else { "0" };
                let _ = Command::new("reg")
                    .args(["add", r"HKLM\SOFTWARE\Policies\Microsoft\Windows\LocationAndSensors", "/v", "DisableLocation", "/t", "REG_DWORD", "/d", loc_val, "/f"])
                    .creation_flags(0x0800_0000)
                    .output().await;
                results.push(format!("Location tracking: {}", if disable { "disabled" } else { "enabled" }));

                // ── Cortana / Recherche connectée ──
                let cortana_val = if disable { "0" } else { "1" };
                for key in ["AllowCortana", "AllowSearchToUseLocation", "ConnectedSearchUseWeb"] {
                    let _ = Command::new("reg")
                        .args(["add", r"HKLM\SOFTWARE\Policies\Microsoft\Windows\Windows Search", "/v", key, "/t", "REG_DWORD", "/d", cortana_val, "/f"])
                        .creation_flags(0x0800_0000)
                        .output().await;
                }
                results.push(format!("Cortana/Connected Search: {}", if disable { "disabled" } else { "enabled" }));

                // ── Données d'écriture manuscrite et saisie (Inking & Typing) ──
                let ink_val = if disable { "0" } else { "1" };
                let _ = Command::new("reg")
                    .args(["add", r"HKCU\Software\Microsoft\Input\TIPC", "/v", "Enabled", "/t", "REG_DWORD", "/d", ink_val, "/f"])
                    .creation_flags(0x0800_0000)
                    .output().await;
                results.push(format!("Inking & Typing data: {}", if disable { "disabled" } else { "enabled" }));

                // ── Feedback & Diagnostics frequency ──
                let feedback_val = if disable { "0" } else { "1" };
                let _ = Command::new("reg")
                    .args(["add", r"HKCU\Software\Microsoft\Siuf\Rules", "/v", "NumberOfSIUFInPeriod", "/t", "REG_DWORD", "/d", feedback_val, "/f"])
                    .creation_flags(0x0800_0000)
                    .output().await;
                results.push(format!("Feedback frequency: {}", if disable { "disabled" } else { "enabled" }));
            }
            _ => return Err(format!("Unknown telemetry category: {category}")),
        }
    }

    #[cfg(not(windows))]
    {
        let _ = (category, disable);
        results.push("Telemetry control is Windows-specific".into());
    }

    Ok(results)
}

// ── Kill running browser processes to unlock cache files ─────────────
#[tauri::command]
pub async fn kill_browser_processes(browsers: Vec<String>) -> Result<Vec<String>, String> {
    let mut killed = Vec::new();

    #[cfg(windows)]
    {
        let all_targets = [
            ("chrome", "chrome.exe", "Chrome"),
            ("edge", "msedge.exe", "Edge"),
            ("firefox", "firefox.exe", "Firefox"),
            ("brave", "brave.exe", "Brave"),
            ("opera", "opera.exe", "Opera"),
            ("opera_gx", "opera.exe", "Opera GX"),
        ];
        for (key, proc, label) in &all_targets {
            if !browsers.iter().any(|b| b == key) { continue; }
            let output = Command::new("taskkill")
                .args(["/F", "/IM", proc])
                .creation_flags(0x0800_0000)
                .output()
                .await
                .map_err(|e| e.to_string())?;
            if output.status.success() {
                killed.push(format!("{label} fermé"));
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        let all_targets = [
            ("chrome", "Google Chrome", "Chrome"),
            ("edge", "Microsoft Edge", "Edge"),
            ("firefox", "firefox", "Firefox"),
            ("brave", "Brave Browser", "Brave"),
            ("opera", "Opera", "Opera"),
            ("opera_gx", "Opera", "Opera GX"),
        ];
        for (key, proc, label) in &all_targets {
            if !browsers.iter().any(|b| b == key) { continue; }
            let output = Command::new("pkill")
                .args(["-f", proc])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            if output.status.success() {
                killed.push(format!("{label} fermé"));
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let all_targets = [
            ("chrome", "chrome", "Chrome"),
            ("edge", "msedge", "Edge"),
            ("firefox", "firefox", "Firefox"),
            ("brave", "brave", "Brave"),
            ("opera", "opera", "Opera"),
            ("opera_gx", "opera", "Opera GX"),
        ];
        for (key, proc, label) in &all_targets {
            if !browsers.iter().any(|b| b == key) { continue; }
            let output = Command::new("pkill")
                .args(["-f", proc])
                .output()
                .await
                .map_err(|e| e.to_string())?;
            if output.status.success() {
                killed.push(format!("{label} fermé"));
            }
        }
    }

    Ok(killed)
}
