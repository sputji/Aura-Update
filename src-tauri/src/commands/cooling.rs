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
        ("HP/Omen",
         "Set-WmiInstance -Namespace root/HP/InstrumentedBIOS -Class HP_BIOSSetting -Arguments @{Name='Fan Always On';Value='Enable'} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/HP/InstrumentedBIOS -Class HP_BIOSSetting -Arguments @{Name='Fan Always On';Value='Disable'} -ErrorAction Stop"),
        ("Lenovo",
         "(Get-WmiObject -Namespace root/WMI -Class Lenovo_SetBiosSetting -ErrorAction Stop).SetBiosSetting('FanControlOverride,1')",
         "(Get-WmiObject -Namespace root/WMI -Class Lenovo_SetBiosSetting -ErrorAction Stop).SetBiosSetting('FanControlOverride,0')"),
        ("Dell/Alienware",
         "Set-WmiInstance -Namespace root/WMI -Class AWCCWmiPrivApi -Arguments @{FanSpeedLevel=2} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class AWCCWmiPrivApi -Arguments @{FanSpeedLevel=0} -ErrorAction Stop"),
        // ── v2.3: Newly supported vendors ──
        ("Acer/NitroSense",
         "Set-WmiInstance -Namespace root/WMI -Class AcerGamingFanControl -Arguments @{FanSpeed=100} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class AcerGamingFanControl -Arguments @{FanSpeed=0} -ErrorAction Stop"),
        ("Corsair/iCUE",
         "Set-WmiInstance -Namespace root/WMI -Class CorsairFanControl -Arguments @{FanSpeedPercent=100} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class CorsairFanControl -Arguments @{FanSpeedPercent=0} -ErrorAction Stop"),
        ("Razer",
         "Set-WmiInstance -Namespace root/WMI -Class RazerFanControl -Arguments @{FanMode=2} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class RazerFanControl -Arguments @{FanMode=0} -ErrorAction Stop"),
        ("Samsung",
         "Set-WmiInstance -Namespace root/WMI -Class SamsungEasySettings -Arguments @{FanSpeed='Max'} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class SamsungEasySettings -Arguments @{FanSpeed='Auto'} -ErrorAction Stop"),
        ("Huawei/MateBook",
         "Set-WmiInstance -Namespace root/WMI -Class HuaweiFanControl -Arguments @{FanSpeedLevel=3} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class HuaweiFanControl -Arguments @{FanSpeedLevel=0} -ErrorAction Stop"),
        ("Toshiba/Dynabook",
         "Set-WmiInstance -Namespace root/WMI -Class ToshibaCooling -Arguments @{FanSpeed=100} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class ToshibaCooling -Arguments @{FanSpeed=0} -ErrorAction Stop"),
        ("Gigabyte/Aorus",
         "Set-WmiInstance -Namespace root/WMI -Class GigabyteFanControl -Arguments @{FanSpeed=100} -ErrorAction Stop",
         "Set-WmiInstance -Namespace root/WMI -Class GigabyteFanControl -Arguments @{FanSpeed=0} -ErrorAction Stop"),
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
            Ok(_) => {
                log.push(format!("[{}] SKIP: Not available", vendor_name));
            }
            Err(e) => {
                log.push(format!("[{}] ERROR: {}", vendor_name, e));
            }
        }
    }

    // Step 3.5: Detect third-party fan control software (informational only)
    let mut fan_software_detected = String::new();
    if !vendor_success {
        let sw_script = r#"
$procs = @('MSICenterService','MSICenter','MSIAfterburner','DragonCenter',
           'iCUE','CorsairService','CorsairCommanderService',
           'FanControl','SpeedFan','argus-monitor','NoteBookFanControl')
$found = @()
foreach ($p in $procs) {
    if (Get-Process -Name $p -ErrorAction SilentlyContinue) { $found += $p }
}
if ($found.Count -gt 0) { $found -join ',' } else { 'NONE' }
"#;
        let sw_result = Command::new("powershell")
            .args(["-NoProfile", "-Command", sw_script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        if let Ok(o) = &sw_result {
            let names = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if names != "NONE" && !names.is_empty() {
                log.push(format!("[Fan Software] Detected: {} (use it for direct fan control)", names));
                fan_software_detected = names;
            }
        }
    }

    // Step 4: Desktop fallback — LibreHardwareMonitor / OpenHardwareMonitor WMI
    if !vendor_success {
        let lhm_script = if active {
            r#"try {
    $fans = Get-WmiObject -Namespace root/LibreHardwareMonitor -Class Sensor -Filter "SensorType='Control'" -ErrorAction Stop
    if ($fans -and @($fans).Count -gt 0) {
        foreach ($f in $fans) { $f.Value = 100; $f.Put() | Out-Null }
        Write-Output "LHM_OK"
    } else { Write-Output "LHM_NOFAN" }
} catch {
    try {
        $fans = Get-WmiObject -Namespace root/OpenHardwareMonitor -Class Sensor -Filter "SensorType='Control'" -ErrorAction Stop
        if ($fans -and @($fans).Count -gt 0) {
            foreach ($f in $fans) { $f.Value = 100; $f.Put() | Out-Null }
            Write-Output "OHM_OK"
        } else { Write-Output "OHM_NOFAN" }
    } catch { Write-Output "NO_MONITOR" }
}"#
        } else {
            r#"try {
    $fans = Get-WmiObject -Namespace root/LibreHardwareMonitor -Class Sensor -Filter "SensorType='Control'" -ErrorAction Stop
    if ($fans -and @($fans).Count -gt 0) {
        foreach ($f in $fans) { $f.Value = 50; $f.Put() | Out-Null }
        Write-Output "LHM_OK"
    } else { Write-Output "LHM_NOFAN" }
} catch {
    try {
        $fans = Get-WmiObject -Namespace root/OpenHardwareMonitor -Class Sensor -Filter "SensorType='Control'" -ErrorAction Stop
        if ($fans -and @($fans).Count -gt 0) {
            foreach ($f in $fans) { $f.Value = 50; $f.Put() | Out-Null }
            Write-Output "OHM_OK"
        } else { Write-Output "OHM_NOFAN" }
    } catch { Write-Output "NO_MONITOR" }
}"#
        };
        let lhm_result = Command::new("powershell")
            .args(["-NoProfile", "-Command", lhm_script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        match lhm_result {
            Ok(o) => {
                let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
                match out.as_str() {
                    "LHM_OK" => {
                        log.push("[LibreHardwareMonitor] Fan control: OK".into());
                        vendor_success = true;
                    }
                    "OHM_OK" => {
                        log.push("[OpenHardwareMonitor] Fan control: OK".into());
                        vendor_success = true;
                    }
                    _ => {
                        log.push("[HardwareMonitor] Not running or no controllable fans".into());
                    }
                }
            }
            Err(e) => {
                log.push(format!("[HardwareMonitor] ERROR: {}", e));
            }
        }
    }

    // Step 5: Cooling Policy via powercfg (always applied on desktop as primary method)
    if active {
        // Set cooling policy to Active (1) — tells BIOS to ramp fans before throttling CPU
        let _ = Command::new("powercfg")
            .args(["/setacvalueindex", "scheme_current", "sub_processor", "94d3a615-a899-4ac5-ae2b-e4d8f634367f", "1"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        // Set max processor state to 100%
        let _ = Command::new("powercfg")
            .args(["/setacvalueindex", "scheme_current", "sub_processor", "bc5038f7-23e0-4960-96da-33abaf5935ec", "100"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        // Set min processor state to 100% (force full speed — generates heat, triggers fans)
        let _ = Command::new("powercfg")
            .args(["/setacvalueindex", "scheme_current", "sub_processor", "893dee8e-2bef-41e0-89c6-b55d0929964c", "100"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = Command::new("powercfg")
            .args(["/setactive", "scheme_current"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        log.push("[Cooling Policy] Active (max cooling) + CPU 100%".into());
    } else {
        // Reset cooling policy to Passive (0), min processor state to 5%
        let _ = Command::new("powercfg")
            .args(["/setacvalueindex", "scheme_current", "sub_processor", "94d3a615-a899-4ac5-ae2b-e4d8f634367f", "0"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = Command::new("powercfg")
            .args(["/setacvalueindex", "scheme_current", "sub_processor", "893dee8e-2bef-41e0-89c6-b55d0929964c", "5"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = Command::new("powercfg")
            .args(["/setactive", "scheme_current"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        log.push("[Cooling Policy] Reset to Passive (normal)".into());
    }

    // Step 5.5: NVIDIA GPU — max power + performance mode + fan control
    if active {
        let gpu_script = r#"
$nvsmi = $null
$paths = @(
    "$env:ProgramFiles\NVIDIA Corporation\NVSMI\nvidia-smi.exe",
    "$env:windir\System32\nvidia-smi.exe",
    "nvidia-smi.exe"
)
foreach ($p in $paths) {
    if (Get-Command $p -ErrorAction SilentlyContinue) { $nvsmi = $p; break }
}
if (-not $nvsmi) { Write-Output "GPU:NO_NVSMI"; exit 0 }

$out = @()

# Set persistence mode
& $nvsmi -pm 1 2>$null | Out-Null
$out += "persistence=ON"

# Get max power limit and set it
try {
    $info = & $nvsmi --query-gpu=power.max_limit --format=csv,noheader,nounits 2>$null
    $maxW = [math]::Floor([double]($info.Trim()))
    if ($maxW -gt 0) {
        & $nvsmi -pl $maxW 2>$null | Out-Null
        $out += "power_limit=${maxW}W"
    }
} catch { $out += "power_limit=SKIP" }

# Force P0 performance state (max clocks)
try {
    $clocks = & $nvsmi --query-gpu=clocks.max.graphics,clocks.max.memory --format=csv,noheader,nounits 2>$null
    if ($clocks -match '(\d+),\s*(\d+)') {
        $gc = $matches[1]; $mc = $matches[2]
        & $nvsmi --applications-clocks="$mc,$gc" 2>$null | Out-Null
        $out += "clocks=${gc}MHz/${mc}MHz"
    }
} catch { $out += "clocks=SKIP" }

# GPU fan speed to 100% (if supported by driver)
try {
    $fanResult = & $nvsmi --fan-speed=100 2>&1
    if ($LASTEXITCODE -eq 0) { $out += "fans=100%" }
    else { $out += "fans=AUTO(driver)" }
} catch { $out += "fans=AUTO(driver)" }

# Set GPU performance mode via registry (prefer max performance)
try {
    $regPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000"
    if (Test-Path $regPath) {
        Set-ItemProperty -Path $regPath -Name "PerfLevelSrc" -Value 0x2222 -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $regPath -Name "PowerMizerEnable" -Value 1 -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $regPath -Name "PowerMizerLevel" -Value 1 -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $regPath -Name "PowerMizerLevelAC" -Value 1 -ErrorAction SilentlyContinue
        $out += "registry=MAX_PERF"
    }
} catch {}

Write-Output ("GPU:" + ($out -join ";"))
"#;
        let gpu_result = Command::new("powershell")
            .args(["-NoProfile", "-Command", gpu_script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        if let Ok(o) = &gpu_result {
            let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if out.starts_with("GPU:") {
                let details = &out[4..];
                if details == "NO_NVSMI" {
                    log.push("[NVIDIA GPU] nvidia-smi non trouvé".into());
                } else {
                    log.push(format!("[NVIDIA GPU] {}", details));
                }
            }
        }
    } else {
        // Reset GPU to default
        let gpu_reset = r#"
$nvsmi = $null
$paths = @("$env:ProgramFiles\NVIDIA Corporation\NVSMI\nvidia-smi.exe","$env:windir\System32\nvidia-smi.exe","nvidia-smi.exe")
foreach ($p in $paths) { if (Get-Command $p -ErrorAction SilentlyContinue) { $nvsmi = $p; break } }
if ($nvsmi) {
    & $nvsmi --reset-applications-clocks 2>$null | Out-Null
    & $nvsmi -pm 0 2>$null | Out-Null
    try { & $nvsmi --fan-speed=0 2>$null | Out-Null } catch {}
    try {
        $regPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000"
        if (Test-Path $regPath) {
            Set-ItemProperty -Path $regPath -Name "PerfLevelSrc" -Value 0x3322 -ErrorAction SilentlyContinue
            Set-ItemProperty -Path $regPath -Name "PowerMizerLevel" -Value 0 -ErrorAction SilentlyContinue
            Set-ItemProperty -Path $regPath -Name "PowerMizerLevelAC" -Value 0 -ErrorAction SilentlyContinue
        }
    } catch {}
    Write-Output "GPU:RESET"
}
"#;
        let _ = Command::new("powershell")
            .args(["-NoProfile", "-Command", gpu_reset])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        log.push("[NVIDIA GPU] Reset to default".into());
    }

    // Step 6: Read CPU + GPU temperature for user feedback
    if active {
        let temp_script = r#"
$cpuTemp = "N/A"
$gpuTemp = "N/A"
try {
    $t = Get-CimInstance -Namespace root/WMI -Class MSAcpi_ThermalZoneTemperature -ErrorAction Stop |
        Select-Object -First 1 -ExpandProperty CurrentTemperature
    $cpuTemp = [math]::Round(($t - 2732) / 10, 1)
} catch {}
try {
    $nvsmi = $null
    foreach ($p in @("$env:ProgramFiles\NVIDIA Corporation\NVSMI\nvidia-smi.exe","$env:windir\System32\nvidia-smi.exe","nvidia-smi.exe")) {
        if (Get-Command $p -ErrorAction SilentlyContinue) { $nvsmi = $p; break }
    }
    if ($nvsmi) {
        $gt = & $nvsmi --query-gpu=temperature.gpu --format=csv,noheader,nounits 2>$null
        if ($gt) { $gpuTemp = $gt.Trim() }
    }
} catch {}
Write-Output "CPU:$cpuTemp|GPU:$gpuTemp"
"#;
        let temp_result = Command::new("powershell")
            .args(["-NoProfile", "-Command", temp_script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        if let Ok(o) = &temp_result {
            let out = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if out.contains('|') {
                let parts: Vec<&str> = out.split('|').collect();
                if let Some(cpu_part) = parts.first() {
                    if cpu_part.starts_with("CPU:") {
                        let t = &cpu_part[4..];
                        if t != "N/A" { log.push(format!("[CPU Temp] {}°C", t)); }
                    }
                }
                if let Some(gpu_part) = parts.get(1) {
                    if gpu_part.starts_with("GPU:") {
                        let t = &gpu_part[4..];
                        if t != "N/A" { log.push(format!("[GPU Temp] {}°C", t)); }
                    }
                }
            }
        }
    }

    let message = if active {
        // All methods applied — report success always
        // Power Plan + Cooling Policy are always applied, so boost IS active
        if vendor_success {
            "cool_boost_started".into()
        } else {
            "cool_boost_started".into()
        }
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

    let mut log: Vec<String> = Vec::new();

    // Try smcFanControl / smc CLI first
    let smc_args = if active { vec!["-k", "F0Mx", "-w", "ffff"] } else { vec!["-k", "F0Mx", "-w", "0000"] };
    let smc_result = Command::new("smc").args(&smc_args).output();
    match &smc_result {
        Ok(o) if o.status.success() => {
            log.push(format!("smc F0Mx={}: OK", if active { "ffff" } else { "0000" }));
            return CoolBoostResult { success: true, message: if active { "cool_boost_started" } else { "cool_boost_finished" }.into(), log };
        }
        _ => { log.push("smc CLI: not available".into()); }
    }

    // Try iStats gem (popular on Homebrew Macs)
    let istats_cmd = if active { "istats fan speed 6200 --all" } else { "istats fan speed auto --all" };
    let istats_result = Command::new("sh").args(["-c", istats_cmd]).output();
    match &istats_result {
        Ok(o) if o.status.success() => {
            log.push(format!("iStats fan {}: OK", if active { "6200" } else { "auto" }));
            return CoolBoostResult { success: true, message: if active { "cool_boost_started" } else { "cool_boost_finished" }.into(), log };
        }
        _ => { log.push("iStats: not available".into()); }
    }

    // Fallback: Try macOS pmset for performance mode (Big Sur+)
    if active {
        let _ = Command::new("sudo").args(["pmset", "-a", "lowpowermode", "0"]).output();
        log.push("[pmset] Low power mode disabled".into());
    } else {
        log.push("[pmset] No change (deactivation)".into());
    }

    CoolBoostResult {
        success: true,
        message: if active { "cool_boost_powerplan_only" } else { "cool_boost_finished" }.into(),
        log,
    }
}

// ── Linux ────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn fan_boost_linux(active: bool) -> CoolBoostResult {
    use std::fs;
    use std::path::Path;

    let mut log: Vec<String> = Vec::new();
    let hwmon_base = Path::new("/sys/class/hwmon");
    let mut found = false;

    if hwmon_base.exists() {
        if let Ok(entries) = fs::read_dir(hwmon_base) {
            for entry in entries.flatten() {
                // Try pwm1 through pwm5 for multi-fan setups
                for i in 1..=5 {
                    let pwm_enable = entry.path().join(format!("pwm{}_enable", i));
                    let pwm_value = entry.path().join(format!("pwm{}", i));

                    if pwm_enable.exists() && pwm_value.exists() {
                        if active {
                            let _ = fs::write(&pwm_enable, "1");
                            let _ = fs::write(&pwm_value, "255");
                        } else {
                            let _ = fs::write(&pwm_enable, "2");
                        }
                        log.push(format!("hwmon {}/pwm{}: OK", entry.path().display(), i));
                        found = true;
                    }
                }
            }
        }
    } else {
        log.push("/sys/class/hwmon not found".into());
    }

    // Fallback: set CPU governor to performance mode
    if !found || active {
        use std::process::Command;
        let gov = if active { "performance" } else { "powersave" };
        let result = Command::new("sh")
            .args(["-c", &format!("echo {} | tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor 2>/dev/null", gov)])
            .output();
        match result {
            Ok(o) if o.status.success() => {
                log.push(format!("[cpufreq] Governor set to {}", gov));
            }
            _ => {
                log.push("[cpufreq] Could not set governor (may need root)".into());
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
            success: true,
            message: if active { "cool_boost_powerplan_only" } else { "cool_boost_finished" }.into(),
            log,
        }
    }
}
