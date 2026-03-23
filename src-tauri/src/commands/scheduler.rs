use tokio::process::Command;

/// Get the current scheduled check configuration.
#[tauri::command]
pub fn get_schedule(state: tauri::State<'_, super::config::AppState>) -> (bool, String) {
    let cfg = state.config.lock().unwrap();
    (cfg.scheduler_enabled, cfg.scheduler_interval.clone())
}

/// Set up (or remove) a scheduled task for automatic update checking.
#[tauri::command]
pub async fn set_schedule(
    state: tauri::State<'_, super::config::AppState>,
    enabled: bool,
    interval: String,
) -> Result<String, String> {
    // Persist in config
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.scheduler_enabled = enabled;
        cfg.scheduler_interval = interval.clone();
        super::config::save_config(&state.data_dir, &cfg);
    }

    if !enabled || interval == "disabled" {
        remove_schedule().await?;
        return Ok("Schedule removed".into());
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    create_schedule(&exe.to_string_lossy(), &interval).await
}

/// Set up (or remove) a scheduled task for automatic cleanup.
#[tauri::command]
pub async fn set_auto_clean_schedule(
    state: tauri::State<'_, super::config::AppState>,
    enabled: bool,
    interval: String,
) -> Result<String, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.auto_clean_enabled = enabled;
        cfg.auto_clean_interval = interval.clone();
        super::config::save_config(&state.data_dir, &cfg);
    }

    if !enabled || interval == "disabled" {
        remove_clean_schedule().await?;
        return Ok("Clean schedule removed".into());
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    create_clean_schedule(&exe.to_string_lossy(), &interval).await
}

/// Get the current auto-clean schedule configuration.
#[tauri::command]
pub fn get_auto_clean_schedule(state: tauri::State<'_, super::config::AppState>) -> (bool, String) {
    let cfg = state.config.lock().unwrap();
    (cfg.auto_clean_enabled, cfg.auto_clean_interval.clone())
}

// ── Windows: Task Scheduler ──────────────────────────────────────────
#[cfg(windows)]
async fn create_schedule(exe_path: &str, interval: &str) -> Result<String, String> {
    // Remove old task first
    let _ = Command::new("schtasks")
        .args(["/delete", "/tn", "AuraUpdateCheck", "/f"])
        .creation_flags(0x0800_0000)
        .output()
        .await;

    let (sc, mo) = match interval {
        "daily" => ("DAILY", "1"),
        "weekly" => ("WEEKLY", "1"),
        "monthly" => ("MONTHLY", "1"),
        "startup" => ("ONLOGON", ""),
        _ => return Err("Invalid interval".into()),
    };

    let mut args = vec![
        "/create".to_string(),
        "/tn".to_string(), "AuraUpdateCheck".to_string(),
        "/tr".to_string(), format!("\"{}\" --auto-check", exe_path),
        "/sc".to_string(), sc.to_string(),
        "/f".to_string(),
    ];
    if sc != "ONLOGON" {
        args.extend(["/st".to_string(), "03:00".to_string()]);
        if !mo.is_empty() {
            args.extend(["/mo".to_string(), mo.to_string()]);
        }
    }

    let out = Command::new("schtasks")
        .args(&args)
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(format!("Scheduled task created ({interval})"))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[cfg(windows)]
async fn remove_schedule() -> Result<(), String> {
    let _ = Command::new("schtasks")
        .args(["/delete", "/tn", "AuraUpdateCheck", "/f"])
        .creation_flags(0x0800_0000)
        .output()
        .await;
    Ok(())
}

// ── Linux: crontab ───────────────────────────────────────────────────
#[cfg(target_os = "linux")]
async fn create_schedule(exe_path: &str, interval: &str) -> Result<String, String> {
    let cron_expr = match interval {
        "daily" => "0 3 * * *",
        "weekly" => "0 3 * * 0",
        "startup" => "@reboot",
        _ => return Err("Invalid interval".into()),
    };

    // Read existing crontab, filter out our entries, add new one
    let existing = Command::new("crontab").arg("-l").output().await
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let filtered: String = existing
        .lines()
        .filter(|l| !l.contains("AuraUpdateCheck"))
        .collect::<Vec<_>>()
        .join("\n");

    let new_crontab = format!(
        "{filtered}\n{cron_expr} {exe_path} --auto-check # AuraUpdateCheck\n"
    );

    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    if let Some(stdin) = child.stdin.as_mut() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(new_crontab.as_bytes()).await.map_err(|e| e.to_string())?;
    }
    child.wait().await.map_err(|e| e.to_string())?;

    Ok(format!("Cron job created ({interval})"))
}

#[cfg(target_os = "linux")]
async fn remove_schedule() -> Result<(), String> {
    let existing = Command::new("crontab").arg("-l").output().await
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let filtered: String = existing
        .lines()
        .filter(|l| !l.contains("AuraUpdateCheck"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "Failed to update crontab".to_string())?;

    if let Some(stdin) = child.stdin.as_mut() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(filtered.as_bytes()).await.ok();
    }
    child.wait().await.ok();
    Ok(())
}

// ── macOS: launchd plist ─────────────────────────────────────────────
#[cfg(target_os = "macos")]
async fn create_schedule(exe_path: &str, interval: &str) -> Result<String, String> {
    let plist_dir = dirs::home_dir()
        .ok_or("Cannot find home dir")?
        .join("Library/LaunchAgents");
    let plist_path = plist_dir.join("fr.auraneo.auraupdate.check.plist");

    let interval_seconds = match interval {
        "daily" => 86400,
        "weekly" => 604800,
        "startup" => 0,
        _ => return Err("Invalid interval".into()),
    };

    let run_at_load = interval == "startup";
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>fr.auraneo.auraupdate.check</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe_path}</string>
        <string>--auto-check</string>
    </array>
    <key>RunAtLoad</key>
    <{run_at_load}/>
    {interval_key}
</dict>
</plist>"#,
        interval_key = if interval_seconds > 0 {
            format!("<key>StartInterval</key>\n    <integer>{interval_seconds}</integer>")
        } else {
            String::new()
        }
    );

    std::fs::write(&plist_path, &plist).map_err(|e| e.to_string())?;

    Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("LaunchAgent created ({interval})"))
}

#[cfg(target_os = "macos")]
async fn remove_schedule() -> Result<(), String> {
    let plist_path = dirs::home_dir()
        .ok_or("Cannot find home dir")?
        .join("Library/LaunchAgents/fr.auraneo.auraupdate.check.plist");

    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .output()
            .await;
        std::fs::remove_file(&plist_path).ok();
    }
    Ok(())
}

// ══════════════════════════════════════════════════════════════
// Auto-Clean Scheduled Task (v2.3)
// ══════════════════════════════════════════════════════════════

#[cfg(windows)]
async fn create_clean_schedule(exe_path: &str, interval: &str) -> Result<String, String> {
    let _ = Command::new("schtasks")
        .args(["/delete", "/tn", "AuraUpdateAutoClean", "/f"])
        .creation_flags(0x0800_0000)
        .output()
        .await;

    let (sc, mo) = match interval {
        "daily" => ("DAILY", "1"),
        "weekly" => ("WEEKLY", "1"),
        "monthly" => ("MONTHLY", "1"),
        _ => return Err("Invalid interval".into()),
    };

    let mut args = vec![
        "/create".to_string(),
        "/tn".to_string(), "AuraUpdateAutoClean".to_string(),
        "/tr".to_string(), format!("\"{}\" --auto-clean", exe_path),
        "/sc".to_string(), sc.to_string(),
        "/f".to_string(),
        "/st".to_string(), "04:00".to_string(),
    ];
    if !mo.is_empty() {
        args.extend(["/mo".to_string(), mo.to_string()]);
    }

    let out = Command::new("schtasks")
        .args(&args)
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(format!("Auto-clean scheduled task created ({interval})"))
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[cfg(windows)]
async fn remove_clean_schedule() -> Result<(), String> {
    let _ = Command::new("schtasks")
        .args(["/delete", "/tn", "AuraUpdateAutoClean", "/f"])
        .creation_flags(0x0800_0000)
        .output()
        .await;
    Ok(())
}

#[cfg(target_os = "linux")]
async fn create_clean_schedule(exe_path: &str, interval: &str) -> Result<String, String> {
    let cron_expr = match interval {
        "daily" => "0 4 * * *",
        "weekly" => "0 4 * * 0",
        "monthly" => "0 4 1 * *",
        _ => return Err("Invalid interval".into()),
    };

    let existing = Command::new("crontab").arg("-l").output().await
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let filtered: String = existing
        .lines()
        .filter(|l| !l.contains("AuraUpdateAutoClean"))
        .collect::<Vec<_>>()
        .join("\n");

    let new_crontab = format!(
        "{filtered}\n{cron_expr} {exe_path} --auto-clean # AuraUpdateAutoClean\n"
    );

    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    if let Some(stdin) = child.stdin.as_mut() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(new_crontab.as_bytes()).await.map_err(|e| e.to_string())?;
    }
    child.wait().await.map_err(|e| e.to_string())?;

    Ok(format!("Auto-clean cron job created ({interval})"))
}

#[cfg(target_os = "linux")]
async fn remove_clean_schedule() -> Result<(), String> {
    let existing = Command::new("crontab").arg("-l").output().await
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let filtered: String = existing
        .lines()
        .filter(|l| !l.contains("AuraUpdateAutoClean"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "Failed to update crontab".to_string())?;

    if let Some(stdin) = child.stdin.as_mut() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(filtered.as_bytes()).await.ok();
    }
    child.wait().await.ok();
    Ok(())
}

#[cfg(target_os = "macos")]
async fn create_clean_schedule(exe_path: &str, interval: &str) -> Result<String, String> {
    let plist_dir = dirs::home_dir()
        .ok_or("Cannot find home dir")?
        .join("Library/LaunchAgents");
    let plist_path = plist_dir.join("fr.auraneo.auraupdate.autoclean.plist");

    let interval_seconds = match interval {
        "daily" => 86400,
        "weekly" => 604800,
        "monthly" => 2592000,
        _ => return Err("Invalid interval".into()),
    };

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>fr.auraneo.auraupdate.autoclean</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe_path}</string>
        <string>--auto-clean</string>
    </array>
    <key>RunAtLoad</key>
    <false/>
    <key>StartInterval</key>
    <integer>{interval_seconds}</integer>
</dict>
</plist>"#
    );

    std::fs::write(&plist_path, &plist).map_err(|e| e.to_string())?;

    Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .output()
        .await
        .map_err(|e| e.to_string())?;

    Ok(format!("Auto-clean LaunchAgent created ({interval})"))
}

#[cfg(target_os = "macos")]
async fn remove_clean_schedule() -> Result<(), String> {
    let plist_path = dirs::home_dir()
        .ok_or("Cannot find home dir")?
        .join("Library/LaunchAgents/fr.auraneo.auraupdate.autoclean.plist");

    if plist_path.exists() {
        let _ = Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .output()
            .await;
        std::fs::remove_file(&plist_path).ok();
    }
    Ok(())
}
