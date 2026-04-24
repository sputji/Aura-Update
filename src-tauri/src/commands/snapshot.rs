use serde::{Deserialize, Serialize};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub description: String,
    pub date: String,
}

/// Check whether the current platform supports snapshots.
#[tauri::command]
pub async fn has_snapshot_support() -> bool {
    has_support_impl().await
}

/// Create a system snapshot / restore point before major operations.
#[tauri::command]
pub async fn create_snapshot(label: String) -> Result<String, String> {
    create_impl(&label).await
}

/// List existing system snapshots / restore points.
#[tauri::command]
pub async fn list_snapshots() -> Result<Vec<SnapshotInfo>, String> {
    list_impl().await
}

/// Get default backup directory (next to executable)
#[tauri::command]
pub fn get_default_backup_dir() -> String {
    default_backup_dir()
}

/// Create a local file-based backup in the custom backup directory.
/// Copies important user config/settings into a timestamped subfolder.
#[tauri::command]
pub async fn create_local_backup(backup_dir: String, label: String) -> Result<String, String> {
    let dir = if backup_dir.is_empty() {
        default_backup_dir()
    } else {
        backup_dir
    };

    let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let safe_label: String = label.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ' ').collect();
    let folder_name = format!("backup_{}_{}", ts, safe_label.replace(' ', "_"));
    let dest = std::path::Path::new(&dir).join(&folder_name);

    std::fs::create_dir_all(&dest).map_err(|e| format!("Cannot create backup dir: {e}"))?;

    // Save config snapshot
    if let Some(config_dir) = dirs::config_dir() {
        let app_config = config_dir.join("aura-update");
        if app_config.exists() {
            let dest_config = dest.join("config");
            copy_dir_recursive(&app_config, &dest_config)?;
        }
    }

    // Write metadata
    let meta = format!("label: {}\ndate: {}\nplatform: {}\n", label, ts, std::env::consts::OS);
    std::fs::write(dest.join("backup_meta.txt"), meta).map_err(|e| e.to_string())?;

    Ok(format!("Backup créé dans : {}", dest.display()))
}

/// List local backups from the custom backup directory
#[tauri::command]
pub fn list_local_backups(backup_dir: String) -> Result<Vec<SnapshotInfo>, String> {
    let dir = if backup_dir.is_empty() {
        default_backup_dir()
    } else {
        backup_dir
    };
    let path = std::path::Path::new(&dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    let entries = std::fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && entry.file_name().to_string_lossy().starts_with("backup_") {
            let meta_path = p.join("backup_meta.txt");
            let (desc, date) = if meta_path.exists() {
                let content = std::fs::read_to_string(&meta_path).unwrap_or_default();
                let label = content.lines().find(|l| l.starts_with("label:"))
                    .map(|l| l.trim_start_matches("label:").trim().to_string())
                    .unwrap_or_default();
                let date = content.lines().find(|l| l.starts_with("date:"))
                    .map(|l| l.trim_start_matches("date:").trim().to_string())
                    .unwrap_or_default();
                (label, date)
            } else {
                (entry.file_name().to_string_lossy().to_string(), String::new())
            };
            backups.push(SnapshotInfo {
                id: entry.file_name().to_string_lossy().to_string(),
                description: desc,
                date,
            });
        }
    }
    backups.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(backups)
}

fn default_backup_dir() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("System Backups").to_string_lossy().to_string()))
        .unwrap_or_else(|| "System Backups".to_string())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let dest_path = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ── Windows: System Restore ──────────────────────────────────────────
#[cfg(windows)]
async fn has_support_impl() -> bool {
    // Check if System Restore is enabled
    Command::new("powershell")
        .args(["-NoProfile", "-Command", "Get-ComputerRestorePoint -ErrorAction SilentlyContinue | Out-Null; $?"])
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "True" || o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
async fn create_impl(label: &str) -> Result<String, String> {
    // Check admin privileges first — Checkpoint-Computer requires elevation
    let admin_check = Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "[bool](([System.Security.Principal.WindowsIdentity]::GetCurrent()).groups -match 'S-1-5-32-544')"])
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if String::from_utf8_lossy(&admin_check.stdout).trim() != "True" {
        return Err("Administrator privileges required to create a restore point. Click 'Elevate' first.".into());
    }

    let ps = format!(
        "Checkpoint-Computer -Description '{}' -RestorePointType 'MODIFY_SETTINGS' -ErrorAction Stop",
        label.replace('\'', "''")
    );
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps])
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok("Windows restore point created".into())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        // Windows limits restore point creation to once every 24h
        if err.contains("1111") || err.contains("frequency") {
            Ok("Restore point skipped (one already exists within 24h)".into())
        } else {
            Err(format!("Restore point failed: {err}"))
        }
    }
}

// ── Linux: timeshift / snapper ───────────────────────────────────────
#[cfg(target_os = "linux")]
async fn has_support_impl() -> bool {
    let ts = Command::new("which").arg("timeshift").output().await
        .map(|o| o.status.success()).unwrap_or(false);
    let sn = Command::new("which").arg("snapper").output().await
        .map(|o| o.status.success()).unwrap_or(false);
    ts || sn
}

#[cfg(target_os = "linux")]
async fn create_impl(label: &str) -> Result<String, String> {
    // Try timeshift first
    if Command::new("which").arg("timeshift").output().await
        .map(|o| o.status.success()).unwrap_or(false)
    {
        let out = Command::new("timeshift")
            .args(["--create", "--comments", label])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return if out.status.success() {
            Ok("Timeshift snapshot created".into())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        };
    }

    // Try snapper
    if Command::new("which").arg("snapper").output().await
        .map(|o| o.status.success()).unwrap_or(false)
    {
        let out = Command::new("snapper")
            .args(["create", "-d", label, "--type", "single"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        return if out.status.success() {
            Ok("Snapper snapshot created".into())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).to_string())
        };
    }

    Err("No snapshot tool available (install timeshift or snapper)".into())
}

// ── macOS: APFS snapshot ─────────────────────────────────────────────
#[cfg(target_os = "macos")]
async fn has_support_impl() -> bool {
    // macOS always has tmutil (Time Machine utility)
    true
}

#[cfg(target_os = "macos")]
async fn create_impl(_label: &str) -> Result<String, String> {
    let out = Command::new("tmutil")
        .args(["localsnapshot"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok("APFS local snapshot created".into())
    } else {
        Err(format!(
            "Snapshot failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

// ── List snapshots ───────────────────────────────────────────────────
#[cfg(windows)]
async fn list_impl() -> Result<Vec<SnapshotInfo>, String> {
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "Get-ComputerRestorePoint | Select-Object SequenceNumber, Description, @{N='Date';E={$_.ConvertToDateTime($_.CreationTime).ToString('yyyy-MM-dd HH:mm')}} | ConvertTo-Json -Compress"])
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !out.status.success() {
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let text = text.trim();
    if text.is_empty() { return Ok(Vec::new()); }

    // PowerShell returns a single object (not array) when only one result
    let parsed: serde_json::Value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
    let items = match &parsed {
        serde_json::Value::Array(arr) => arr.clone(),
        serde_json::Value::Object(_) => vec![parsed],
        _ => Vec::new(),
    };

    Ok(items.iter().map(|v| SnapshotInfo {
        id: v["SequenceNumber"].to_string(),
        description: v["Description"].as_str().unwrap_or("").to_string(),
        date: v["Date"].as_str().unwrap_or("").to_string(),
    }).collect())
}

#[cfg(target_os = "linux")]
async fn list_impl() -> Result<Vec<SnapshotInfo>, String> {
    // Try timeshift
    if let Ok(out) = Command::new("timeshift").args(["--list"]).output().await {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            let snapshots: Vec<SnapshotInfo> = text.lines()
                .filter(|l| l.contains("Snapshot"))
                .enumerate()
                .map(|(i, l)| SnapshotInfo {
                    id: i.to_string(),
                    description: l.trim().to_string(),
                    date: String::new(),
                })
                .collect();
            return Ok(snapshots);
        }
    }
    Ok(Vec::new())
}

#[cfg(target_os = "macos")]
async fn list_impl() -> Result<Vec<SnapshotInfo>, String> {
    let out = Command::new("tmutil")
        .args(["listlocalsnapshots", "/"])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !out.status.success() { return Ok(Vec::new()); }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text.lines()
        .filter(|l| l.contains("com.apple.TimeMachine"))
        .enumerate()
        .map(|(i, l)| SnapshotInfo {
            id: i.to_string(),
            description: l.trim().to_string(),
            date: String::new(),
        })
        .collect())
}
