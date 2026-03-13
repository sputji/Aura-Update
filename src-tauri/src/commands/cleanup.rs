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

    let total = items.iter().map(|i| i.size_bytes).sum();
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

    let total = items.iter().map(|i| i.size_bytes).sum();
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

#[tauri::command]
pub async fn scan_browser_caches() -> Result<CleanupReport, String> {
    let mut items = Vec::new();

    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            collect_browser_items(&mut items, &[
                (local.join("Google\\Chrome\\User Data\\Default\\Cache"), "Chrome Cache"),
                (local.join("Google\\Chrome\\User Data\\Default\\Code Cache"), "Chrome Code Cache"),
                (local.join("Microsoft\\Edge\\User Data\\Default\\Cache"), "Edge Cache"),
                (local.join("Microsoft\\Edge\\User Data\\Default\\Code Cache"), "Edge Code Cache"),
                (local.join("BraveSoftware\\Brave-Browser\\User Data\\Default\\Cache"), "Brave Cache"),
                (local.join("Mozilla\\Firefox\\Profiles"), "Firefox Profiles Cache"),
                (local.join("Opera Software\\Opera Stable\\Cache"), "Opera Cache"),
                (local.join("Opera Software\\Opera GX Stable\\Cache"), "Opera GX Cache"),
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

    let total = items.iter().map(|i| i.size_bytes).sum();
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
        for &pkg in BLOATWARE_LIST {
            result.push(BloatwareInfo {
                package: pkg.to_string(),
                label: pkg.split('.').last().unwrap_or(pkg).to_string(),
                installed: false,
            });
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
    #[cfg(windows)]
    {
        // If empty selection, purge all known bloatwares
        let targets: Vec<&str> = if selection.is_empty() {
            BLOATWARE_LIST.to_vec()
        } else {
            // Only purge packages that are in our known list (security: whitelist)
            selection.iter()
                .filter(|s| BLOATWARE_LIST.contains(&s.as_str()))
                .map(|s| s.as_str())
                .collect()
        };

        let mut removed_count = 0;

        // Batch all removals into a single PowerShell call
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

        return Ok(format!("Purge terminée : {} applications supprimées.", removed_count));
    }

    #[cfg(not(windows))]
    {
        Err("La purge des bloatwares est spécifique à Windows.".into())
    }
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
/// Categories: "windows", "office", "vscode"
#[tauri::command]
pub async fn disable_telemetry_granular(category: String, disable: bool) -> Result<Vec<String>, String> {
    let mut results = Vec::new();

    #[cfg(windows)]
    {
        match category.as_str() {
            "windows" => {
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
            "vscode" => {
                // VS Code telemetry is in settings.json — modify user settings
                if let Some(appdata) = std::env::var_os("APPDATA") {
                    let settings_path = std::path::PathBuf::from(appdata)
                        .join("Code")
                        .join("User")
                        .join("settings.json");
                    if settings_path.exists() {
                        if let Ok(content) = std::fs::read_to_string(&settings_path) {
                            if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&content) {
                                let val = if disable { "off" } else { "all" };
                                json["telemetry.telemetryLevel"] = serde_json::Value::String(val.into());
                                if let Ok(out) = serde_json::to_string_pretty(&json) {
                                    let _ = std::fs::write(&settings_path, out);
                                    results.push(format!("VS Code telemetry: {val}"));
                                }
                            }
                        }
                    } else {
                        results.push("VS Code settings not found".into());
                    }
                }
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
