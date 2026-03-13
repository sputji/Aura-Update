use serde::{Deserialize, Serialize};
use tauri::Manager;
use sysinfo::Components;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::Instant;

/// Cached full health score, updated by get_health_score().
/// Used by remote dashboard for accurate sync access.
static CACHED_HEALTH_SCORE: AtomicU8 = AtomicU8::new(0);

/// Battery cache: avoids spawning PowerShell every 5 s (refreshVitals interval).
static BATTERY_CACHE: Mutex<Option<(Instant, (Option<u8>, bool))>> = Mutex::new(None);

/// Components cache: avoids re-enumerating sensors every call.
static COMP_CACHE: Mutex<Option<Components>> = Mutex::new(None);

pub fn get_cached_health_score() -> Option<u8> {
    let v = CACHED_HEALTH_SCORE.load(Ordering::Relaxed);
    if v == 0 { None } else { Some(v) }
}

/// Health score breakdown sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthScore {
    pub total: u8,            // 0-100
    pub update_score: u8,     // up to 40 pts
    pub disk_score: u8,       // up to 20 pts
    pub startup_score: u8,    // up to 20 pts
    pub temp_score: u8,       // up to 20 pts
    pub pending_updates: usize,
    pub startup_count: usize,
    pub temp_size_mb: u64,
    pub disk_free_gb: u64,
}

#[tauri::command]
pub async fn get_health_score() -> Result<HealthScore, String> {
    // 1. Updates (40 pts)
    let updates = super::updates::check_updates().await.unwrap_or_default();
    let update_score = match updates.len() {
        0 => 40,
        1..=3 => 30,
        4..=10 => 15,
        _ => 0,
    };

    // 2. Disk free space (20 pts)
    let disk_free = get_disk_free_gb();
    let disk_score = if disk_free >= 50 {
        20
    } else if disk_free >= 20 {
        15
    } else if disk_free >= 10 {
        10
    } else if disk_free >= 5 {
        5
    } else {
        0
    };

    // 3. Startup items (20 pts)
    let startup_items = super::startup::get_startup_items().unwrap_or_default();
    let enabled = startup_items.iter().filter(|s| s.enabled).count();
    let startup_score = match enabled {
        0..=5 => 20,
        6..=10 => 15,
        11..=15 => 10,
        _ => 5,
    };

    // 4. Temp files (20 pts)
    let cleanup = super::cleanup::scan_cleanup().await.unwrap_or(
        super::cleanup::CleanupReport { items: vec![], total_bytes: 0 }
    );
    let temp_mb = cleanup.total_bytes / (1024 * 1024);
    let temp_score = match temp_mb {
        0..=100 => 20,
        101..=500 => 15,
        501..=2000 => 10,
        _ => 5,
    };

    let total = (update_score + disk_score + startup_score + temp_score).min(100);

    // Cache for remote dashboard
    CACHED_HEALTH_SCORE.store(total, Ordering::Relaxed);

    Ok(HealthScore {
        total,
        update_score,
        disk_score,
        startup_score,
        temp_score,
        pending_updates: updates.len(),
        startup_count: enabled,
        temp_size_mb: temp_mb,
        disk_free_gb: disk_free,
    })
}

fn get_disk_free_gb() -> u64 {
    use sysinfo::Disks;
    let disks = Disks::new_with_refreshed_list();
    // Take the main / root disk
    disks
        .iter()
        .filter(|d| {
            let mp = d.mount_point().to_string_lossy();
            mp == "/" || mp.starts_with("C:")
        })
        .map(|d| d.available_space() / (1024 * 1024 * 1024))
        .next()
        .unwrap_or(0)
}

/// Auto-pilot: runs updates + cleanup + returns new health score.
#[tauri::command]
pub async fn run_autopilot(app: tauri::AppHandle) -> Result<HealthScore, String> {
    use tauri::Emitter;

    app.emit("autopilot-progress", serde_json::json!({
        "step": "snapshot", "message": "Creating safety snapshot…"
    })).ok();

    // 1. Snapshot (best-effort)
    let _ = super::snapshot::create_snapshot("Aura Auto-Pilot".into()).await;

    // 2. Updates
    app.emit("autopilot-progress", serde_json::json!({
        "step": "updates", "message": "Checking for updates…"
    })).ok();
    let updates = super::updates::check_updates().await.unwrap_or_default();

    for pkg in &updates {
        app.emit("autopilot-progress", serde_json::json!({
            "step": "updates", "message": format!("Installing {}…", &pkg.name)
        })).ok();
        let _ = super::updates::install_update(app.clone(), pkg.clone()).await;
    }

    // 3. Cleanup
    app.emit("autopilot-progress", serde_json::json!({
        "step": "cleanup", "message": "Cleaning temporary files…"
    })).ok();
    let report = super::cleanup::scan_cleanup().await.unwrap_or(
        super::cleanup::CleanupReport { items: vec![], total_bytes: 0 }
    );
    let paths: Vec<String> = report.items.iter().map(|i| i.path.clone()).collect();
    let state: tauri::State<'_, crate::AppState> = app.state();
    let _ = super::cleanup::run_cleanup(state, paths).await;

    // 4. OS residues
    app.emit("autopilot-progress", serde_json::json!({
        "step": "residues", "message": "Cleaning OS residues…"
    })).ok();
    let residues = super::cleanup::scan_os_residues().await.unwrap_or(
        super::cleanup::CleanupReport { items: vec![], total_bytes: 0 }
    );
    let residue_ids: Vec<String> = residues.items.iter().map(|i| i.path.clone()).collect();
    let state2: tauri::State<'_, crate::AppState> = app.state();
    let _ = super::cleanup::clean_os_residues(state2, residue_ids).await;

    // 5. New score
    app.emit("autopilot-progress", serde_json::json!({
        "step": "done", "message": "Auto-pilot complete!"
    })).ok();

    get_health_score().await
}

/// Tauri command: returns CPU/GPU temps as JSON for the frontend.
#[tauri::command]
pub fn get_vitals() -> Result<serde_json::Value, String> {
    let mut lock = COMP_CACHE.lock().unwrap();
    if lock.is_none() {
        *lock = Some(Components::new_with_refreshed_list());
    } else {
        lock.as_mut().unwrap().refresh_list();
    }

    let mut cpu = 0.0_f32;
    let mut gpu = 0.0_f32;
    if let Some(comps) = lock.as_ref() {
        for c in comps.iter() {
            let label = c.label().to_lowercase();
            if (label.contains("cpu") || label.contains("core") || label.contains("package")) && c.temperature() > 0.0 {
                cpu = c.temperature();
            } else if (label.contains("gpu") || label.contains("nvidia") || label.contains("amdgpu")) && c.temperature() > 0.0 {
                gpu = c.temperature();
            }
        }
    }
    drop(lock);

    // Windows fallback: WMI thermal zone + nvidia-smi
    #[cfg(target_os = "windows")]
    {
        if cpu == 0.0 {
            if let Some(t) = wmi_cpu_temp() { cpu = t; }
        }
        if gpu == 0.0 {
            if let Some(t) = nvidia_smi_gpu_temp() { gpu = t; }
        }
    }

    let (batt_pct, batt_charging) = get_battery_info();
    Ok(serde_json::json!({ "cpu_temp": cpu, "gpu_temp": gpu, "battery_percent": batt_pct, "battery_charging": batt_charging }))
}

/// Lightweight sync helper: returns (cpu_temp, gpu_temp) from sysinfo.
/// Reuses COMP_CACHE to avoid allocating a new Components list on every call.
/// Falls back to WMI/nvidia-smi on Windows when sysinfo returns no sensors.
fn get_vitals_internal() -> (Option<f32>, Option<f32>) {
    let mut lock = COMP_CACHE.lock().unwrap();
    if lock.is_none() {
        *lock = Some(Components::new_with_refreshed_list());
    } else {
        lock.as_mut().unwrap().refresh_list();
    }

    let mut cpu: Option<f32> = None;
    let mut gpu: Option<f32> = None;
    if let Some(comps) = lock.as_ref() {
        for c in comps.iter() {
            let label = c.label().to_lowercase();
            let temp = c.temperature();
            if cpu.is_none() && (label.contains("cpu") || label.contains("core") || label.contains("tctl") || label.contains("package")) {
                cpu = Some(temp);
            }
            if gpu.is_none() && (label.contains("gpu") || label.contains("edge") || label.contains("junction") || label.contains("nvidia") || label.contains("amdgpu")) {
                gpu = Some(temp);
            }
        }
    }
    // Release the lock before calling potentially long-running PowerShell commands below.
    drop(lock);

    // Windows fallback: WMI thermal zone + nvidia-smi
    #[cfg(target_os = "windows")]
    {
        if cpu.is_none() {
            cpu = wmi_cpu_temp();
        }
        if gpu.is_none() {
            gpu = nvidia_smi_gpu_temp();
        }
    }
    (cpu, gpu)
}

/// WMI fallback for CPU temperature on Windows.
/// Queries MSAcpi_ThermalZoneTemperature (returns tenths of Kelvin).
#[cfg(target_os = "windows")]
fn wmi_cpu_temp() -> Option<f32> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "(Get-CimInstance -Namespace root/WMI -ClassName MSAcpi_ThermalZoneTemperature -ErrorAction SilentlyContinue | Select-Object -First 1).CurrentTemperature"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout);
    let raw: f32 = text.trim().parse().ok()?;
    // Convert tenths of Kelvin to Celsius
    let celsius = raw / 10.0 - 273.15;
    if celsius > 0.0 && celsius < 150.0 { Some(celsius) } else { None }
}

/// nvidia-smi fallback for GPU temperature on Windows.
#[cfg(target_os = "windows")]
fn nvidia_smi_gpu_temp() -> Option<f32> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() { return None; }
    let text = String::from_utf8_lossy(&output.stdout);
    text.trim().lines().next()?.trim().parse::<f32>().ok()
}

/// Sync health score estimation based on disk + temps only (no async I/O).
/// Returns 0–100. Used by the remote dashboard when async is unavailable.
pub fn get_health_score_sync() -> u8 {
    let disk_free = get_disk_free_gb();
    let disk_score: u8 = if disk_free >= 50 { 20 } else if disk_free >= 20 { 15 } else if disk_free >= 10 { 10 } else if disk_free >= 5 { 5 } else { 0 };

    let (cpu, gpu) = get_vitals_internal();
    let max_temp = cpu.unwrap_or(0.0).max(gpu.unwrap_or(0.0));
    let temp_score: u8 = if max_temp < 60.0 { 20 } else if max_temp < 75.0 { 15 } else if max_temp < 85.0 { 10 } else { 5 };

    // Without updates/startup info, scale the 2 available axes to 0-100
    // disk (20) + temp (20) = 40 available, scale: score * 100 / 40
    let raw = (disk_score + temp_score) as u16 * 100 / 40;
    raw.min(100) as u8
}

/// System vitals: temperatures, battery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemVitals {
    pub cpu_temp: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub battery_percent: Option<u8>,
    pub battery_charging: bool,
    pub components: Vec<ComponentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentInfo {
    pub label: String,
    pub temperature: f32,
}

#[tauri::command]
pub fn get_system_vitals() -> SystemVitals {
    let mut lock = COMP_CACHE.lock().unwrap();
    if lock.is_none() {
        *lock = Some(Components::new_with_refreshed_list());
    } else {
        lock.as_mut().unwrap().refresh_list();
    }

    let mut cpu_temp: Option<f32> = None;
    let mut gpu_temp: Option<f32> = None;
    let mut comp_list = Vec::new();

    if let Some(components) = lock.as_ref() {
        for c in components.iter() {
            let label = c.label().to_lowercase();
            let temp = c.temperature();

            comp_list.push(ComponentInfo {
                label: c.label().to_string(),
                temperature: temp,
            });

            if cpu_temp.is_none() && (label.contains("cpu") || label.contains("core") || label.contains("tctl") || label.contains("package")) {
                cpu_temp = Some(temp);
            }
            if gpu_temp.is_none() && (label.contains("gpu") || label.contains("edge") || label.contains("junction") || label.contains("nvidia") || label.contains("amdgpu")) {
                gpu_temp = Some(temp);
            }
        }
    }
    // Release the lock before calling potentially long-running PowerShell/process commands below.
    drop(lock);

    // Windows fallback: WMI thermal zone + nvidia-smi
    #[cfg(target_os = "windows")]
    {
        if cpu_temp.is_none() {
            if let Some(t) = wmi_cpu_temp() {
                cpu_temp = Some(t);
                comp_list.push(ComponentInfo { label: "WMI ThermalZone".into(), temperature: t });
            }
        }
        if gpu_temp.is_none() {
            if let Some(t) = nvidia_smi_gpu_temp() {
                gpu_temp = Some(t);
                comp_list.push(ComponentInfo { label: "nvidia-smi GPU".into(), temperature: t });
            }
        }
    }

    // Battery (sysinfo doesn't expose battery directly; use platform-specific)
    let (battery_percent, battery_charging) = get_battery_info();

    SystemVitals {
        cpu_temp,
        gpu_temp,
        battery_percent,
        battery_charging,
        components: comp_list,
    }
}

#[cfg(windows)]
fn get_battery_info() -> (Option<u8>, bool) {
    // Return cached value if fresh (< 60 s) to avoid spawning PowerShell too often
    if let Ok(cache) = BATTERY_CACHE.lock() {
        if let Some((ts, val)) = &*cache {
            if ts.elapsed().as_secs() < 60 {
                return *val;
            }
        }
    }

    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command",
            "(Get-CimInstance Win32_Battery | Select-Object EstimatedChargeRemaining, BatteryStatus | ConvertTo-Json)"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(o) = output {
        if o.status.success() {
            let text = String::from_utf8_lossy(&o.stdout);
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(text.trim()) {
                let pct = val["EstimatedChargeRemaining"].as_u64().map(|v| v.min(100) as u8);
                let charging = val["BatteryStatus"].as_u64().unwrap_or(0) == 2;
                let result = (pct, charging);
                if let Ok(mut c) = BATTERY_CACHE.lock() {
                    *c = Some((Instant::now(), result));
                }
                return result;
            }
        }
    }
    (None, false)
}

#[cfg(not(windows))]
fn get_battery_info() -> (Option<u8>, bool) {
    // Try reading from /sys/class/power_supply/BAT0
    let capacity = std::fs::read_to_string("/sys/class/power_supply/BAT0/capacity")
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok());
    let charging = std::fs::read_to_string("/sys/class/power_supply/BAT0/status")
        .ok()
        .map(|s| s.trim().to_lowercase().contains("charging"))
        .unwrap_or(false);
    (capacity, charging)
}

// ── System Info Command ──────────────────────────────────────────────

/// System identification info for the badge row and log header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub cpu: String,
    pub gpu: String,
    pub ram_total_gb: f32,
    pub ram_used_gb: f32,
}

/// Retrieve OS, CPU, GPU and RAM identification in a single call.
/// Used by the frontend badge row and embedded in log headers.
#[tauri::command]
pub fn get_system_info() -> SystemInfo {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_list(sysinfo::CpuRefreshKind::everything());

    let os_name = System::name().unwrap_or_else(|| std::env::consts::OS.to_string());
    let os_version = System::long_os_version().unwrap_or_default();
    let os = format!("{} {}", os_name, os_version).trim().to_string();

    let cpu_count = sys.cpus().len();
    let cpu = sys
        .cpus()
        .first()
        .map(|c| {
            let brand = c.brand().trim().to_string();
            if cpu_count > 1 {
                format!("{} ({} Threads)", brand, cpu_count)
            } else {
                brand
            }
        })
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let total_bytes = sys.total_memory();
    let used_bytes = sys.used_memory();
    let ram_total_gb = (total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)) as f32;
    let ram_used_gb = (used_bytes as f64 / (1024.0 * 1024.0 * 1024.0)) as f32;

    let gpu = detect_gpu_name();

    SystemInfo {
        os,
        cpu,
        gpu,
        ram_total_gb: (ram_total_gb * 10.0).round() / 10.0,
        ram_used_gb: (ram_used_gb * 10.0).round() / 10.0,
    }
}

/// Best-effort GPU name detection. Returns an empty string when unavailable.
fn detect_gpu_name() -> String {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        if let Ok(out) = Command::new("powershell")
            .args(["-NoProfile", "-Command",
                "(Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | Select-Object -First 1 -ExpandProperty Name)"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        {
            let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !name.is_empty() && out.status.success() {
                return name;
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("system_profiler")
            .args(["SPDisplaysDataType", "-json"])
            .output()
        {
            if let Ok(txt) = std::str::from_utf8(&out.stdout) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(txt) {
                    if let Some(name) = val["SPDisplaysDataType"][0]["sppci_model"].as_str() {
                        return name.to_string();
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(out) = std::process::Command::new("lspci").output() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                let lower = line.to_lowercase();
                if lower.contains("vga") || lower.contains("3d controller") || lower.contains("display") {
                    // Extract the part after the colon following the class
                    if let Some(pos) = line.find(": ") {
                        return line[pos + 2..].trim().to_string();
                    }
                }
            }
        }
    }

    String::new()
}
