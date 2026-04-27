use super::command_runner::{request_cancel, run_logged_command, RunSpec};

#[tauri::command]
pub fn maintenance_cancel_task(task: String) -> Result<bool, String> {
    Ok(request_cancel(&task))
}

// ── Mise à jour du moteur Git ─────────────────────────────────────────
#[tauri::command]
pub async fn maintenance_update_git(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(windows)]
    {
        let result = run_logged_command(
            &app,
            RunSpec {
                task: "update_git".to_string(),
                action: "maintenance_update_git".to_string(),
                step: "Mise à jour Git CLI".to_string(),
                program: "git".to_string(),
                args: vec!["update-git-for-windows".to_string(), "-y".to_string()],
                timeout_secs: 120,
                start_percent: 5,
                done_percent: 100,
            },
        )
        .await?;
        return Ok(result.output);
    }

    #[cfg(target_os = "macos")]
    {
        let result = run_logged_command(
            &app,
            RunSpec {
                task: "update_git".to_string(),
                action: "maintenance_update_git".to_string(),
                step: "Mise à jour Git CLI".to_string(),
                program: "brew".to_string(),
                args: vec!["upgrade".to_string(), "git".to_string()],
                timeout_secs: 300,
                start_percent: 5,
                done_percent: 100,
            },
        )
        .await?;
        return Ok(result.output);
    }

    #[cfg(target_os = "linux")]
    {
        let result = run_logged_command(
            &app,
            RunSpec {
                task: "update_git".to_string(),
                action: "maintenance_update_git".to_string(),
                step: "Mise à jour Git CLI".to_string(),
                program: "sudo".to_string(),
                args: vec!["apt".to_string(), "upgrade".to_string(), "git".to_string(), "-y".to_string()],
                timeout_secs: 300,
                start_percent: 5,
                done_percent: 100,
            },
        )
        .await?;
        return Ok(result.output);
    }

    #[allow(unreachable_code)]
    Err("Plateforme non supportée".into())
}

// ── Mise à jour des applications ──────────────────────────────────────
#[tauri::command]
pub async fn maintenance_update_apps(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(windows)]
    {
        let result = run_logged_command(
            &app,
            RunSpec {
                task: "update_apps".to_string(),
                action: "maintenance_update_apps".to_string(),
                step: "Mise à jour des applications".to_string(),
                program: "winget".to_string(),
                args: vec![
                    "upgrade".to_string(),
                    "--all".to_string(),
                    "--force".to_string(),
                    "--include-unknown".to_string(),
                    "--accept-package-agreements".to_string(),
                    "--accept-source-agreements".to_string(),
                ],
                timeout_secs: 120,
                start_percent: 5,
                done_percent: 100,
            },
        )
        .await?;
        return Ok(result.output);
    }

    #[cfg(target_os = "macos")]
    {
        let update_text = run_logged_command(
            &app,
            RunSpec {
                task: "update_apps".to_string(),
                action: "maintenance_update_apps".to_string(),
                step: "brew update".to_string(),
                program: "brew".to_string(),
                args: vec!["update".to_string()],
                timeout_secs: 180,
                start_percent: 15,
                done_percent: 45,
            },
        )
        .await?
        .output;
        let upgrade_text = run_logged_command(
            &app,
            RunSpec {
                task: "update_apps".to_string(),
                action: "maintenance_update_apps".to_string(),
                step: "brew upgrade".to_string(),
                program: "brew".to_string(),
                args: vec!["upgrade".to_string()],
                timeout_secs: 300,
                start_percent: 55,
                done_percent: 100,
            },
        )
        .await?
        .output;
        return Ok(format!("{}\n---\n{}", update_text, upgrade_text));
    }

    #[cfg(target_os = "linux")]
    {
        let update_text = run_logged_command(
            &app,
            RunSpec {
                task: "update_apps".to_string(),
                action: "maintenance_update_apps".to_string(),
                step: "apt update".to_string(),
                program: "sudo".to_string(),
                args: vec!["apt".to_string(), "update".to_string()],
                timeout_secs: 180,
                start_percent: 15,
                done_percent: 45,
            },
        )
        .await?
        .output;
        let upgrade_text = run_logged_command(
            &app,
            RunSpec {
                task: "update_apps".to_string(),
                action: "maintenance_update_apps".to_string(),
                step: "apt full-upgrade".to_string(),
                program: "sudo".to_string(),
                args: vec!["apt".to_string(), "full-upgrade".to_string(), "-y".to_string()],
                timeout_secs: 600,
                start_percent: 55,
                done_percent: 100,
            },
        )
        .await?
        .output;
        return Ok(format!("{}\n---\n{}", update_text, upgrade_text));
    }

    #[allow(unreachable_code)]
    Err("Plateforme non supportée".into())
}

// ── Analyse & Réparation Système ──────────────────────────────────────
#[tauri::command]
pub async fn maintenance_repair_system(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(windows)]
    {
        let dism = run_logged_command(
            &app,
            RunSpec {
                task: "repair_system".to_string(),
                action: "maintenance_repair_system".to_string(),
                step: "DISM RestoreHealth".to_string(),
                program: "dism".to_string(),
                args: vec!["/online".to_string(), "/cleanup-image".to_string(), "/restorehealth".to_string()],
                timeout_secs: 1800,
                start_percent: 10,
                done_percent: 65,
            },
        )
        .await?;

        let sfc = run_logged_command(
            &app,
            RunSpec {
                task: "repair_system".to_string(),
                action: "maintenance_repair_system".to_string(),
                step: "SFC ScanNow".to_string(),
                program: "sfc".to_string(),
                args: vec!["/scannow".to_string()],
                timeout_secs: 1800,
                start_percent: 70,
                done_percent: 100,
            },
        )
        .await?;

        return Ok(format!(
            "=== DISM (run_id: {}) ===\n{}\n\n=== SFC (run_id: {}) ===\n{}",
            dism.run_id, dism.output, sfc.run_id, sfc.output
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let result = run_logged_command(
            &app,
            RunSpec {
                task: "repair_system".to_string(),
                action: "maintenance_repair_system".to_string(),
                step: "diskutil verifyVolume".to_string(),
                program: "diskutil".to_string(),
                args: vec!["verifyVolume".to_string(), "/".to_string()],
                timeout_secs: 1200,
                start_percent: 10,
                done_percent: 100,
            },
        )
        .await?;
        return Ok(result.output);
    }

    #[cfg(target_os = "linux")]
    {
        let result = run_logged_command(
            &app,
            RunSpec {
                task: "repair_system".to_string(),
                action: "maintenance_repair_system".to_string(),
                step: "dpkg --configure -a".to_string(),
                program: "sudo".to_string(),
                args: vec!["dpkg".to_string(), "--configure".to_string(), "-a".to_string()],
                timeout_secs: 900,
                start_percent: 10,
                done_percent: 100,
            },
        )
        .await?;
        return Ok(result.output);
    }

    #[allow(unreachable_code)]
    Err("Plateforme non supportée".into())
}

// ── Nettoyage du Système ─────────────────────────────────────────────
#[tauri::command]
pub async fn maintenance_clean_system(app: tauri::AppHandle) -> Result<String, String> {
    #[cfg(windows)]
    {
        let _ = run_logged_command(
            &app,
            RunSpec {
                task: "clean_system".to_string(),
                action: "maintenance_clean_system".to_string(),
                step: "cleanmgr sageset".to_string(),
                program: "cleanmgr".to_string(),
                args: vec!["/sageset:1".to_string()],
                timeout_secs: 120,
                start_percent: 5,
                done_percent: 15,
            },
        )
        .await;

        let result = run_logged_command(
            &app,
            RunSpec {
                task: "clean_system".to_string(),
                action: "maintenance_clean_system".to_string(),
                step: "cleanmgr sagerun".to_string(),
                program: "cleanmgr".to_string(),
                args: vec!["/sagerun:1".to_string()],
                timeout_secs: 900,
                start_percent: 20,
                done_percent: 100,
            },
        )
        .await?;
        return Ok(result.output);
    }

    #[cfg(target_os = "macos")]
    {
        let result = run_logged_command(
            &app,
            RunSpec {
                task: "clean_system".to_string(),
                action: "maintenance_clean_system".to_string(),
                step: "brew cleanup".to_string(),
                program: "brew".to_string(),
                args: vec!["cleanup".to_string()],
                timeout_secs: 300,
                start_percent: 15,
                done_percent: 100,
            },
        )
        .await?;
        return Ok(result.output);
    }

    #[cfg(target_os = "linux")]
    {
        let autoremove_text = run_logged_command(
            &app,
            RunSpec {
                task: "clean_system".to_string(),
                action: "maintenance_clean_system".to_string(),
                step: "apt autoremove".to_string(),
                program: "sudo".to_string(),
                args: vec!["apt".to_string(), "autoremove".to_string(), "-y".to_string()],
                timeout_secs: 600,
                start_percent: 20,
                done_percent: 60,
            },
        )
        .await?
        .output;
        let clean_text = run_logged_command(
            &app,
            RunSpec {
                task: "clean_system".to_string(),
                action: "maintenance_clean_system".to_string(),
                step: "apt clean".to_string(),
                program: "sudo".to_string(),
                args: vec!["apt".to_string(), "clean".to_string()],
                timeout_secs: 300,
                start_percent: 70,
                done_percent: 100,
            },
        )
        .await?
        .output;
        return Ok(format!("{}\n---\n{}", autoremove_text, clean_text));
    }

    #[allow(unreachable_code)]
    Err("Plateforme non supportée".into())
}

// Note: l'émission de progression est désormais pilotée par command_runner::run_logged_command.
