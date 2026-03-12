#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Crash reporter — save panic context to disk for next-launch detection.
    // With panic="abort" (release), the hook executes then the process aborts.
    let exe = std::env::current_exe().unwrap_or_default();
    let data_dir = exe
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("aura_data");

    std::panic::set_hook(Box::new(move |info| {
        let crash_info = info.to_string().replace('\\', "\\\\").replace('"', "\\\"");
        let os = std::env::consts::OS;
        let json = format!(
            "{{\"panic\":\"{}\",\"os\":\"{}\"}}",
            crash_info, os
        );
        let _ = std::fs::create_dir_all(&data_dir);
        let _ = std::fs::write(data_dir.join("crash_report.json"), json);
    }));

    aura_update_lib::run();
}
