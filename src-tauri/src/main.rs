#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // 1. FIX CRITIQUE : Force le dossier de travail sur celui de l'exécutable
    // Empêche le bug ERR_CONNECTION_REFUSED en mode Admin
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let _ = std::env::set_current_dir(exe_dir);
        }
    }

    // 2. FORCER L'ADMIN DÈS LE DÉMARRAGE (Uniquement Windows)
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        let is_admin = std::process::Command::new("net")
            .args(["session"])
            .creation_flags(0x0800_0000)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !is_admin {
            if let Ok(exe) = std::env::current_exe() {
                let script = format!(
                    "Start-Process -FilePath '{}' -WorkingDirectory '{}' -Verb RunAs",
                    exe.display(),
                    exe.parent().unwrap().display()
                );
                let _ = std::process::Command::new("powershell.exe")
                    .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
                    .spawn();
                std::process::exit(0);
            }
        }
    }

    aura_update_lib::run();
}
