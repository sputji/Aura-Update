use serde::{Deserialize, Serialize};

use crate::commands::config::{self, AppState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryModeStatus {
    pub is_laptop: bool,
    pub battery_percent: Option<u8>,
    pub battery_charging: bool,
    pub mode: String,
}

#[tauri::command]
pub fn get_battery_mode_status(state: tauri::State<'_, AppState>) -> BatteryModeStatus {
    let vitals = super::health::get_system_vitals();
    let cfg = state.config.lock().unwrap();
    BatteryModeStatus {
        is_laptop: vitals.battery_percent.is_some(),
        battery_percent: vitals.battery_percent,
        battery_charging: vitals.battery_charging,
        mode: cfg.battery_mode.clone(),
    }
}

#[tauri::command]
pub async fn set_battery_mode(
    state: tauri::State<'_, AppState>,
    mode: String,
) -> Result<BatteryModeStatus, String> {
    let mode = mode.to_lowercase();
    if mode != "eco" && mode != "normal" && mode != "extreme" {
        return Err("Mode batterie invalide".into());
    }

    apply_mode_os(&mode).await?;

    {
        let mut cfg = state.config.lock().unwrap();
        cfg.battery_mode = mode.clone();
        config::save_config(&state.data_dir, &cfg);
    }

    let status = get_battery_mode_status(state);
    Ok(status)
}

#[cfg(windows)]
async fn apply_mode_os(mode: &str) -> Result<(), String> {
    use tokio::process::Command;

    let script = match mode {
        "eco" => {
            r#"
powercfg /setactive 381b4222-f694-41f0-9685-ff5bb260df2e | Out-Null
powercfg /setacvalueindex scheme_current sub_processor 893dee8e-2bef-41e0-89c6-b55d0929964c 5 | Out-Null
powercfg /setacvalueindex scheme_current sub_processor bc5038f7-23e0-4960-96da-33abaf5935ec 70 | Out-Null
sc.exe stop SysMain 2>$null | Out-Null
sc.exe stop WSearch 2>$null | Out-Null
"#
        }
        "normal" => {
            r#"
powercfg /setactive 381b4222-f694-41f0-9685-ff5bb260df2e | Out-Null
powercfg /setacvalueindex scheme_current sub_processor 893dee8e-2bef-41e0-89c6-b55d0929964c 10 | Out-Null
powercfg /setacvalueindex scheme_current sub_processor bc5038f7-23e0-4960-96da-33abaf5935ec 90 | Out-Null
sc.exe start SysMain 2>$null | Out-Null
sc.exe start WSearch 2>$null | Out-Null
"#
        }
        _ => {
            r#"
powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c | Out-Null
powercfg /setacvalueindex scheme_current sub_processor 893dee8e-2bef-41e0-89c6-b55d0929964c 100 | Out-Null
powercfg /setacvalueindex scheme_current sub_processor bc5038f7-23e0-4960-96da-33abaf5935ec 100 | Out-Null
sc.exe stop SysMain 2>$null | Out-Null
sc.exe stop WSearch 2>$null | Out-Null
"#
        }
    };

    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", script])
        .creation_flags(0x0800_0000)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Err(format!("Impossible d'appliquer le mode batterie: {}", stderr))
    }
}

#[cfg(target_os = "macos")]
async fn apply_mode_os(mode: &str) -> Result<(), String> {
    use tokio::process::Command;

    let args: Vec<&str> = match mode {
        "eco" => vec!["-a", "lowpowermode", "1"],
        "normal" => vec!["-a", "lowpowermode", "0"],
        _ => vec!["-a", "lowpowermode", "0"],
    };

    let out = Command::new("pmset")
        .args(args)
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err("pmset failed".into()) }
}

#[cfg(target_os = "linux")]
async fn apply_mode_os(mode: &str) -> Result<(), String> {
    use tokio::process::Command;

    let governor = match mode {
        "eco" => "powersave",
        "normal" => "schedutil",
        _ => "performance",
    };
    let cmd = format!("echo {} | tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor", governor);
    let _ = Command::new("sh").args(["-c", &cmd]).output().await;
    Ok(())
}
