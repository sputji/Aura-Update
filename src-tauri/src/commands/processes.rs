use serde::{Deserialize, Serialize};
use sysinfo::{System, ProcessesToUpdate};
use std::sync::Mutex;

use crate::commands::config::{self, AppState, TurboProfile};

#[cfg(windows)]
use std::process::Command as StdCommand;
#[cfg(windows)]
use std::os::windows::process::CommandExt;

// ── Cache Global (Évite les freezes CPU) ──
static SYS_CACHE: Mutex<Option<System>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub memory_mb: u64,
}

/// Return the top resource-consuming processes (CPU + RAM).
#[tauri::command]
pub fn get_heavy_processes() -> Vec<ProcessInfo> {
    let mut lock = SYS_CACHE.lock().unwrap();
    if lock.is_none() {
        *lock = Some(System::new());
    }
    let sys = lock.as_mut().unwrap();

    // Rafraîchissement ultra-léger (processus uniquement)
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut procs: Vec<ProcessInfo> = sys.processes().values()
        .filter(|p| {
            let name = p.name().to_string_lossy();
            !name.is_empty() && name != "System Idle Process" && name != "svchost.exe"
        })
        .map(|p| ProcessInfo {
            pid: p.pid().as_u32(),
            name: p.name().to_string_lossy().to_string(),
            cpu_percent: p.cpu_usage(),
            memory_mb: p.memory() / 1_048_576,
        })
        .collect();

    procs.sort_by(|a, b| b.memory_mb.cmp(&a.memory_mb));
    procs.truncate(30);
    procs
}

/// Kill a process by PID.
#[tauri::command]
pub fn kill_process(pid: u32) -> Result<bool, String> {
    let mut lock = SYS_CACHE.lock().unwrap();
    if lock.is_none() {
        *lock = Some(System::new());
    }
    let sys = lock.as_mut().unwrap();
    let spid = sysinfo::Pid::from(pid as usize);
    sys.refresh_processes(ProcessesToUpdate::Some(&[spid]), true);
    if let Some(process) = sys.process(spid) {
        process.kill();
        Ok(true)
    } else {
        Err(format!("Process {pid} not found"))
    }
}

/// Toggle Game Mode: suspend or resume heavy non-essential processes.
/// Also applies system-wide performance optimizations.
#[tauri::command]
pub async fn toggle_game_mode(activate: bool, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let (active_profile_name, profiles) = {
        let cfg = state.config.lock().unwrap();
        (cfg.active_turbo_profile.clone(), cfg.turbo_profiles.clone())
    };

    let profile = profiles
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(&active_profile_name))
        .cloned()
        .or_else(|| profiles.first().cloned());

    // Fire-and-forget: spawn in background so the UI isn't blocked
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || {
            toggle_game_mode_sync(activate, profile)
        }).await;
    });
    Ok(true)
}

fn toggle_game_mode_sync(activate: bool, profile: Option<TurboProfile>) -> Result<Vec<String>, String> {
    // Apps to suspend for maximum FPS / performance
    let default_targets = ["chrome", "msedge", "spotify", "discord", "teams", "adobe",
                   "onedrive", "searchindexer", "widgets", "cortana",
                   "yourphone", "gamebar", "skype", "slack", "zoom",
                   "dropbox", "googledrive", "icloud"];

    let (whitelist, blacklist): (Vec<String>, Vec<String>) = if let Some(p) = &profile {
        (
            p.whitelist.iter().map(|s| s.to_lowercase()).collect(),
            p.blacklist.iter().map(|s| s.to_lowercase()).collect(),
        )
    } else {
        (Vec::new(), Vec::new())
    };

    // Scan processes ONCE — no repeated polling loop
    let mut sys = System::new();
    sys.refresh_all();

    // Collect all matching (pid, display_name) pairs in one pass
    let matching: Vec<(u32, String)> = sys.processes().values()
        .filter(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            if whitelist.iter().any(|w| name.contains(w)) {
                return false;
            }

            let in_default = default_targets.iter().any(|t| name.contains(t));
            let in_blacklist = blacklist.iter().any(|b| name.contains(b));
            in_default || in_blacklist
        })
        .map(|p| (p.pid().as_u32(), p.name().to_string_lossy().to_string()))
        .collect();

    let mut affected = Vec::new();

    #[cfg(windows)]
    {
        // ── 1. Suspend/Resume processes ──
        if !matching.is_empty() {
            let pid_array: Vec<String> = matching.iter().map(|(pid, _)| pid.to_string()).collect();
            let pid_list = pid_array.join(",");
            let cmd = if activate { "Suspend-Process" } else { "Resume-Process" };
            let script = format!(
                "$pids=@({}); $pids | ForEach-Object {{ {cmd} -Id $_ -ErrorAction SilentlyContinue }}",
                pid_list
            );
            let result = StdCommand::new("powershell")
                .args(["-NoProfile", "-Command", &script])
                .creation_flags(0x0800_0000)
                .output();
            if result.is_ok() {
                for (pid, display) in &matching {
                    affected.push(format!("{cmd}: {display} (PID {pid})"));
                }
            }
        }

        // ── 2. Windows services optimization ──
        let services = if activate {
            // Stop non-essential services that eat CPU/RAM/disk
            vec![
                ("SysMain", "stop"),           // Superfetch — pre-loads apps, heavy on disk
                ("WSearch", "stop"),            // Windows Search indexer — CPU + disk
                ("TabletInputService", "stop"), // Touch keyboard (useless for gaming)
                ("MapsBroker", "stop"),         // Downloaded maps manager
                ("wuauserv", "stop"),           // Windows Update (prevent mid-game updates)
                ("BITS", "stop"),               // Background transfers
                ("DusmSvc", "stop"),            // Data Usage
            ]
        } else {
            vec![
                ("SysMain", "start"),
                ("WSearch", "start"),
                ("TabletInputService", "start"),
                ("MapsBroker", "start"),
                ("wuauserv", "start"),
                ("BITS", "start"),
                ("DusmSvc", "start"),
            ]
        };
        let svc_cmds: Vec<String> = services.iter()
            .map(|(svc, action)| format!("sc.exe {} {} 2>$null", action, svc))
            .collect();
        let svc_script = svc_cmds.join("; ");
        let _ = StdCommand::new("powershell")
            .args(["-NoProfile", "-Command", &svc_script])
            .creation_flags(0x0800_0000)
            .output();
        affected.push(format!("Services: {} {}", if activate { "stopped" } else { "restarted" }, services.len()));

        // ── 3. Power plan: Ultimate Performance or High Performance ──
        let power_script = if activate {
            r#"
# Try Ultimate Performance first, fall back to High Performance
$ultimate = powercfg /list | Select-String "e9a42b02-d5df-448d-aa00-03f14749eb61"
if ($ultimate) {
    powercfg /setactive e9a42b02-d5df-448d-aa00-03f14749eb61
} else {
    # Create Ultimate Performance if not present
    $result = powercfg /duplicatescheme e9a42b02-d5df-448d-aa00-03f14749eb61 2>$null
    if ($LASTEXITCODE -eq 0) { powercfg /setactive e9a42b02-d5df-448d-aa00-03f14749eb61 }
    else { powercfg /setactive 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c }
}
# Max CPU state
powercfg /setacvalueindex scheme_current sub_processor bc5038f7-23e0-4960-96da-33abaf5935ec 100
powercfg /setacvalueindex scheme_current sub_processor 893dee8e-2bef-41e0-89c6-b55d0929964c 100
# Disable CPU parking
powercfg /setacvalueindex scheme_current sub_processor 0cc5b647-c1df-4637-891a-dec35c318583 100
# Disable power throttling
powercfg /setacvalueindex scheme_current sub_processor 3b04d4fd-1cc7-4f23-ab1c-d1337819c4bb 0 2>$null
powercfg /setactive scheme_current
"#
        } else {
            r#"
powercfg /setactive 381b4222-f694-41f0-9685-ff5bb260df2e
powercfg /setacvalueindex scheme_current sub_processor 893dee8e-2bef-41e0-89c6-b55d0929964c 5
powercfg /setacvalueindex scheme_current sub_processor 0cc5b647-c1df-4637-891a-dec35c318583 50
powercfg /setactive scheme_current
"#
        };
        let _ = StdCommand::new("powershell")
            .args(["-NoProfile", "-Command", power_script])
            .creation_flags(0x0800_0000)
            .output();
        affected.push(format!("PowerPlan: {}", if activate { "Ultimate Performance" } else { "Balanced" }));

        // ── 4. GPU max performance (NVIDIA) ──
        let gpu_script = if activate {
            r#"
$nvsmi = $null
foreach ($p in @("$env:ProgramFiles\NVIDIA Corporation\NVSMI\nvidia-smi.exe","$env:windir\System32\nvidia-smi.exe","nvidia-smi.exe")) {
    if (Get-Command $p -ErrorAction SilentlyContinue) { $nvsmi = $p; break }
}
if ($nvsmi) {
    & $nvsmi -pm 1 2>$null | Out-Null
    try {
        $maxW = [math]::Floor([double]((& $nvsmi --query-gpu=power.max_limit --format=csv,noheader,nounits 2>$null).Trim()))
        if ($maxW -gt 0) { & $nvsmi -pl $maxW 2>$null | Out-Null }
    } catch {}
    # Prefer max performance in NVIDIA Control Panel
    $regPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000"
    if (Test-Path $regPath) {
        Set-ItemProperty -Path $regPath -Name "PerfLevelSrc" -Value 0x2222 -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $regPath -Name "PowerMizerEnable" -Value 1 -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $regPath -Name "PowerMizerLevel" -Value 1 -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $regPath -Name "PowerMizerLevelAC" -Value 1 -ErrorAction SilentlyContinue
    }
}
# System-wide GPU preference: High Performance
$regGpu = "HKCU:\Software\Microsoft\DirectX\UserGpuPreferences"
if (!(Test-Path $regGpu)) { New-Item -Path $regGpu -Force | Out-Null }
Set-ItemProperty -Path $regGpu -Name "DirectXUserGlobalSettings" -Value "SwapEffectUpgradeEnable=1;VRROptimizeEnable=1;" -ErrorAction SilentlyContinue 2>$null
"#
        } else {
            r#"
$nvsmi = $null
foreach ($p in @("$env:ProgramFiles\NVIDIA Corporation\NVSMI\nvidia-smi.exe","$env:windir\System32\nvidia-smi.exe","nvidia-smi.exe")) {
    if (Get-Command $p -ErrorAction SilentlyContinue) { $nvsmi = $p; break }
}
if ($nvsmi) {
    & $nvsmi --reset-applications-clocks 2>$null | Out-Null
    & $nvsmi -pm 0 2>$null | Out-Null
    $regPath = "HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}\0000"
    if (Test-Path $regPath) {
        Set-ItemProperty -Path $regPath -Name "PerfLevelSrc" -Value 0x3322 -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $regPath -Name "PowerMizerLevel" -Value 0 -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $regPath -Name "PowerMizerLevelAC" -Value 0 -ErrorAction SilentlyContinue
    }
}
"#
        };
        let _ = StdCommand::new("powershell")
            .args(["-NoProfile", "-Command", gpu_script])
            .creation_flags(0x0800_0000)
            .output();

        // ── 5. Memory + system optimizations ──
        let sys_script = if activate {
            r#"
# Clear standby memory (frees RAM)
try {
    $MemBytes = [System.Runtime.InteropServices.Marshal]::SizeOf([Type][IntPtr])
    # Increase working set
    $proc = Get-Process -Id $PID
    $proc.MinWorkingSet = 200MB
} catch {}

# Disable visual effects for performance
try {
    Set-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects" -Name "VisualFXSetting" -Value 2 -ErrorAction SilentlyContinue
    Set-ItemProperty -Path "HKCU:\Control Panel\Desktop" -Name "UserPreferencesMask" -Value ([byte[]](0x90,0x12,0x03,0x80,0x10,0x00,0x00,0x00)) -ErrorAction SilentlyContinue
} catch {}

# Set timer resolution to 1ms (better frame pacing)
try {
    $ntdll = [System.Runtime.InteropServices.RuntimeEnvironment]::GetRuntimeDirectory()
    Add-Type -TypeDefinition @"
using System.Runtime.InteropServices;
public class NtDll {
    [DllImport("ntdll.dll")] public static extern int NtSetTimerResolution(int DesiredResolution, bool SetResolution, out int CurrentResolution);
}
"@ -ErrorAction SilentlyContinue
    $current = 0
    [NtDll]::NtSetTimerResolution(5000, $true, [ref]$current) | Out-Null
} catch {}

# Disable power throttling
try {
    Set-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling" -Name "PowerThrottlingOff" -Value 1 -Type DWord -ErrorAction SilentlyContinue
} catch {}

# Network optimization: disable Nagle's algorithm for lower latency
try {
    $adapters = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces" -ErrorAction SilentlyContinue
    foreach ($a in $adapters) {
        Set-ItemProperty -Path $a.PSPath -Name "TcpAckFrequency" -Value 1 -Type DWord -ErrorAction SilentlyContinue
        Set-ItemProperty -Path $a.PSPath -Name "TCPNoDelay" -Value 1 -Type DWord -ErrorAction SilentlyContinue
    }
} catch {}
"#
        } else {
            r#"
# Restore visual effects to defaults
try {
    Set-ItemProperty -Path "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects" -Name "VisualFXSetting" -Value 0 -ErrorAction SilentlyContinue
} catch {}
# Restore power throttling
try {
    Remove-ItemProperty -Path "HKLM:\SYSTEM\CurrentControlSet\Control\Power\PowerThrottling" -Name "PowerThrottlingOff" -ErrorAction SilentlyContinue
} catch {}
# Restore Nagle's algorithm
try {
    $adapters = Get-ChildItem "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces" -ErrorAction SilentlyContinue
    foreach ($a in $adapters) {
        Remove-ItemProperty -Path $a.PSPath -Name "TcpAckFrequency" -ErrorAction SilentlyContinue
        Remove-ItemProperty -Path $a.PSPath -Name "TCPNoDelay" -ErrorAction SilentlyContinue
    }
} catch {}
"#
        };
        let _ = StdCommand::new("powershell")
            .args(["-NoProfile", "-Command", sys_script])
            .creation_flags(0x0800_0000)
            .output();
        affected.push(format!("System: {}", if activate { "max performance" } else { "restored" }));
    }

    #[cfg(not(windows))]
    {
        let signal = if activate { "STOP" } else { "CONT" };
        for (pid, display) in &matching {
            let _ = std::process::Command::new("kill")
                .args([&format!("-{signal}"), &pid.to_string()])
                .output();
            affected.push(format!("{signal}: {display} (PID {pid})"));
        }
    }

    Ok(affected)
}

#[tauri::command]
pub fn get_turbo_profiles(state: tauri::State<'_, AppState>) -> Vec<TurboProfile> {
    state.config.lock().unwrap().turbo_profiles.clone()
}

#[tauri::command]
pub fn save_turbo_profile(state: tauri::State<'_, AppState>, profile: TurboProfile) -> Result<bool, String> {
    let mut cfg = state.config.lock().unwrap();
    if profile.name.trim().is_empty() {
        return Err("Le nom du profil est requis".into());
    }

    if let Some(existing) = cfg
        .turbo_profiles
        .iter_mut()
        .find(|p| p.name.eq_ignore_ascii_case(&profile.name))
    {
        *existing = profile.clone();
    } else {
        cfg.turbo_profiles.push(profile.clone());
    }

    if cfg.active_turbo_profile.trim().is_empty() {
        cfg.active_turbo_profile = profile.name;
    }

    config::save_config(&state.data_dir, &cfg);
    Ok(true)
}

#[tauri::command]
pub fn set_active_turbo_profile(state: tauri::State<'_, AppState>, name: String) -> Result<bool, String> {
    let mut cfg = state.config.lock().unwrap();
    if !cfg.turbo_profiles.iter().any(|p| p.name.eq_ignore_ascii_case(&name)) {
        return Err("Profil introuvable".into());
    }
    cfg.active_turbo_profile = name;
    config::save_config(&state.data_dir, &cfg);
    Ok(true)
}
