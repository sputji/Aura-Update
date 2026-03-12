use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[tauri::command]
pub fn is_admin() -> bool {
    is_admin_impl()
}

#[cfg(windows)]
fn is_admin_impl() -> bool {
    Command::new("net")
        .args(["session"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn is_admin_impl() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false)
}

#[tauri::command]
pub fn elevate() -> Result<bool, String> {
    elevate_impl()
}

#[cfg(windows)]
fn elevate_impl() -> Result<bool, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();

    // On passe l'argument --admin-relaunch
    let script = format!(
        "Start-Process -FilePath \"{}\" -ArgumentList \"--admin-relaunch\" -WorkingDirectory \"{}\" -Verb RunAs",
        exe.display(),
        dir
    );

    let status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-ExecutionPolicy", "Bypass",
            "-WindowStyle", "Hidden",
            "-Command",
            &script
        ])
        .creation_flags(0x0800_0000)
        .status()
        .map_err(|e| format!("Erreur système : {}", e))?;

    if status.success() {
        // L'utilisateur a dit "Oui", on tue cette instance INSTANTANÉMENT
        std::process::exit(0);
    } else {
        Err("L'utilisateur a refusé l'élévation UAC.".into())
    }
}

#[cfg(target_os = "macos")]
fn elevate_impl() -> Result<bool, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let script = format!(
        "do shell script \"\\\"{}\\\" --admin-relaunch\" with administrator privileges",
        exe.display()
    );
    let status = Command::new("osascript")
        .args(["-e", &script])
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        std::process::exit(0);
    } else {
        Err("Refusé".into())
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn elevate_impl() -> Result<bool, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let status = Command::new("pkexec")
        .arg(exe)
        .arg("--admin-relaunch")
        .status()
        .map_err(|e| e.to_string())?;

    if status.success() {
        std::process::exit(0);
    } else {
        Err("Refusé".into())
    }
}
