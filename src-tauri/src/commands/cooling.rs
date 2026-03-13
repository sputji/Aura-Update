use serde::{Deserialize, Serialize};

/// Result of a fan boost request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoolBoostResult {
    pub success: bool,
    pub message: String,
    /// Detailed log of what was attempted (vendor chain)
    #[serde(default)]
    pub log: Vec<String>,
}

/// Activate or deactivate fan boost.
///
/// Platform-specific:
/// - **Windows**: WMI / Win32_Fan + PowerShell fallback
/// - **macOS**: SMC fan override via `smc` CLI
/// - **Linux**: sysfs hwmon pwm
///
/// Wrapped in catch_unwind to prevent silent crashes.
#[tauri::command]
pub fn set_fan_boost(active: bool) -> CoolBoostResult {
    match std::panic::catch_unwind(|| {
        #[cfg(target_os = "windows")]
        { fan_boost_windows(active) }
        #[cfg(target_os = "macos")]
        { fan_boost_macos(active) }
        #[cfg(target_os = "linux")]
        { fan_boost_linux(active) }
    }) {
        Ok(result) => result,
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "Unknown panic in fan boost".into()
            };
            CoolBoostResult {
                success: false,
                message: "cool_error".into(),
                log: vec![format!("PANIC caught: {}", msg)],
            }
        }
    }
}

// ── Windows ──────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn fan_boost_windows(active: bool) -> CoolBoostResult {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut log: Vec<String> = Vec::new();

    // Step 1: Detect vendor from WMI baseboard (with timeout protection)
    let board_vendor = match Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "(Get-CimInstance Win32_BaseBoard).Manufacturer"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_lowercase(),
        Err(e) => {
            log.push(format!("Board vendor detection failed: {}", e));
            String::new()
        }
    };
    log.push(format!("Board vendor: {}", if board_vendor.is_empty() { "unknown" } else { &board_vendor }));

    // Step 2: Power plan switch (universal, always applied first)
    let mode = if active { "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c" } else { "381b4222-f694-41f0-9685-ff5bb260df2e" };
    let pp_result = Command::new("powercfg").args(["/setactive", mode]).creation_flags(CREATE_NO_WINDOW).output();
    let pp_ok = pp_result.map(|o| o.status.success()).unwrap_or(false);
    log.push(format!("Power plan ({}): {}", if active { "High Perf" } else { "Balanced" }, if pp_ok { "OK" } else { "FAIL" }));

    // Step 3: Multi-vendor EC fan control fallback chain with logging
    let vendors: Vec<(&str, &str, &str)> = vec![
        ("MSI", 
         "Set-WmiInstance -Namespace root/WMI -Class MSI_LaptopFanControl -Arguments @{FanSpeed=100} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class MSI_LaptopFanControl -Arguments @{FanSpeed=0} -ErrorAction Stop"),
        ("ASUS/ROG",
         "Set-WmiInstance -Namespace root/WMI -Class AsusATK -Arguments @{FanSpeedPercentage=100} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class AsusATK -Arguments @{FanSpeedPercentage=0} -ErrorAction Stop"),
        ("HP",
         "Set-WmiInstance -Namespace root/HP/InstrumentedBIOS -Class HP_BIOSSetting -Arguments @{Name='Fan Always On';Value='Enable'} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/HP/InstrumentedBIOS -Class HP_BIOSSetting -Arguments @{Name='Fan Always On';Value='Disable'} -ErrorAction Stop"),
        ("Lenovo",
         "(Get-WmiObject -Namespace root/WMI -Class Lenovo_SetBiosSetting -ErrorAction Stop).SetBiosSetting('FanControlOverride,1')",
         "(Get-WmiObject -Namespace root/WMI -Class Lenovo_SetBiosSetting -ErrorAction Stop).SetBiosSetting('FanControlOverride,0')"),
        ("Dell/Alienware",
         "Set-WmiInstance -Namespace root/WMI -Class AWCCWmiPrivApi -Arguments @{FanSpeedLevel=2} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class AWCCWmiPrivApi -Arguments @{FanSpeedLevel=0} -ErrorAction Stop"),
    ];

    let mut vendor_success = false;
    for (vendor_name, activate_cmd, deactivate_cmd) in &vendors {
        let script = if active { *activate_cmd } else { *deactivate_cmd };
        let result = Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match result {
            Ok(o) if o.status.success() => {
                log.push(format!("[{}] OK", vendor_name));
                vendor_success = true;
                break;
            }
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr);
                // Use char-based truncation to avoid a byte-boundary panic on
                // multi-byte characters (e.g. accented French output from PowerShell).
                let truncated_err: String = err.chars().take(80).collect();
                log.push(format!("[{}] SKIP: {}", vendor_name, truncated_err.trim()));
            }
            Err(e) => {
                log.push(format!("[{}] ERROR: {}", vendor_name, e));
            }
        }
    }

    if !vendor_success {
        log.push("Fan EC: No vendor matched → Power Plan mode only".into());
    }

    // If no vendor EC succeeded while activating, report Power Plan-only result so
    // the frontend can display an honest status instead of claiming full fan control.
    let message = if active {
        if vendor_success { "cool_boost_started".into() } else { "cool_boost_powerplan_only".into() }
    } else {
        "cool_boost_finished".into()
    };

    CoolBoostResult {
        success: true,
        message,
        log,
    }
}

// ── macOS ────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn fan_boost_macos(active: bool) -> CoolBoostResult {
    use std::process::Command;

    if active {
        let result = Command::new("smc")
            .args(["-k", "F0Mx", "-w", "ffff"])
            .output();

        match result {
            Ok(o) if o.status.success() => CoolBoostResult {
                success: true,
                message: "cool_boost_started".into(),
                log: vec!["smc F0Mx=ffff: OK".into()],
            },
            _ => CoolBoostResult {
                success: false,
                message: "cool_error".into(),
                log: vec!["smc not available".into()],
            },
        }
    } else {
        let result = Command::new("smc")
            .args(["-k", "F0Mx", "-w", "0000"])
            .output();

        match result {
            Ok(_) => CoolBoostResult {
                success: true,
                message: "cool_boost_finished".into(),
                log: vec!["smc F0Mx=0000: OK".into()],
            },
            _ => CoolBoostResult {
                success: false,
                message: "cool_error".into(),
                log: vec!["smc not available".into()],
            },
        }
    }
}

// ── Linux ────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn fan_boost_linux(active: bool) -> CoolBoostResult {
    use std::fs;
    use std::path::Path;

    let mut log: Vec<String> = Vec::new();
    let hwmon_base = Path::new("/sys/class/hwmon");
    if !hwmon_base.exists() {
        return CoolBoostResult {
            success: false,
            message: "cool_error".into(),
            log: vec!["/sys/class/hwmon not found".into()],
        };
    }

    let mut found = false;

    if let Ok(entries) = fs::read_dir(hwmon_base) {
        for entry in entries.flatten() {
            let pwm_enable = entry.path().join("pwm1_enable");
            let pwm_value = entry.path().join("pwm1");

            if pwm_enable.exists() && pwm_value.exists() {
                if active {
                    let _ = fs::write(&pwm_enable, "1");
                    let _ = fs::write(&pwm_value, "255");
                } else {
                    let _ = fs::write(&pwm_enable, "2");
                }
                log.push(format!("hwmon {}: OK", entry.path().display()));
                found = true;
            }
        }
    }

    if found {
        CoolBoostResult {
            success: true,
            message: if active { "cool_boost_started" } else { "cool_boost_finished" }.into(),
            log,
        }
    } else {
        CoolBoostResult {
            success: false,
            message: "cool_error".into(),
            log: vec!["No hwmon pwm nodes found".into()],
        }
    }
}
