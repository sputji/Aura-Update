use serde::{Deserialize, Serialize};
use sysinfo::{System, ProcessesToUpdate};
use std::sync::Mutex;

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
/// Targets user apps that consume CPU/RAM to free resources for gaming / pro use.
#[tauri::command]
pub async fn toggle_game_mode(activate: bool) -> Result<bool, String> {
    // Fire-and-forget: spawn in background so the UI isn't blocked
    tokio::spawn(async move {
        let _ = tokio::task::spawn_blocking(move || {
            toggle_game_mode_sync(activate)
        }).await;
    });
    Ok(true)
}

fn toggle_game_mode_sync(activate: bool) -> Result<Vec<String>, String> {
    // Apps to suspend for maximum FPS / performance
    let targets = ["chrome", "msedge", "spotify", "discord", "teams", "adobe",
                   "onedrive", "searchindexer", "widgets", "cortana"];

    // Scan processes ONCE — no repeated polling loop
    let mut sys = System::new();
    sys.refresh_all();

    // Collect all matching (pid, display_name) pairs in one pass
    let matching: Vec<(u32, String)> = sys.processes().values()
        .filter(|p| {
            let name = p.name().to_string_lossy().to_lowercase();
            targets.iter().any(|t| name.contains(t))
        })
        .map(|p| (p.pid().as_u32(), p.name().to_string_lossy().to_string()))
        .collect();

    let mut affected = Vec::new();
    if matching.is_empty() {
        return Ok(affected);
    }

    #[cfg(windows)]
    {
        // Single PowerShell invocation for ALL matched processes.
        // This eliminates the per-process startup cost (previously N × ~500 ms).
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
