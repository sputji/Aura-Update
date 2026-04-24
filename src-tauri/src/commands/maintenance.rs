use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceProgress {
    pub task: String,      // identifiant de la tâche
    pub status: String,    // "running" | "done" | "error"
    pub output: String,    // stdout/stderr accumulé
    pub percent: u8,       // 0-100 (approximatif)
}

// ── Mise à jour du moteur Git ─────────────────────────────────────────
#[tauri::command]
pub async fn maintenance_update_git(app: tauri::AppHandle) -> Result<String, String> {
    emit_progress(&app, "update_git", "running", "Démarrage…", 0);

    #[cfg(windows)]
    {
        let out = Command::new("git")
            .args(["update-git-for-windows", "-y"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let text = combine_output(&out);
        emit_progress(&app, "update_git", "done", &text, 100);
        return Ok(text);
    }

    #[cfg(target_os = "macos")]
    {
        // brew peut demander un long temps — on streame stdout
        let out = Command::new("brew")
            .args(["upgrade", "git"])
            .output()
            .await
            .map_err(|e| format!("brew non disponible : {e}"))?;
        let text = combine_output(&out);
        emit_progress(&app, "update_git", "done", &text, 100);
        return Ok(text);
    }

    #[cfg(target_os = "linux")]
    {
        let out = Command::new("sudo")
            .args(["apt", "upgrade", "git", "-y"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let text = combine_output(&out);
        emit_progress(&app, "update_git", "done", &text, 100);
        return Ok(text);
    }

    #[allow(unreachable_code)]
    Err("Plateforme non supportée".into())
}

// ── Mise à jour des applications ──────────────────────────────────────
#[tauri::command]
pub async fn maintenance_update_apps(app: tauri::AppHandle) -> Result<String, String> {
    emit_progress(&app, "update_apps", "running", "Mise à jour des applications…", 5);

    #[cfg(windows)]
    {
        let out = Command::new("winget")
            .args([
                "upgrade",
                "--all",
                "--accept-package-agreements",
                "--accept-source-agreements",
            ])
            .output()
            .await
            .map_err(|e| format!("winget non disponible : {e}"))?;
        let text = combine_output(&out);
        emit_progress(&app, "update_apps", "done", &text, 100);
        return Ok(text);
    }

    #[cfg(target_os = "macos")]
    {
        emit_progress(&app, "update_apps", "running", "brew update…", 20);
        let update_out = Command::new("brew")
            .arg("update")
            .output()
            .await
            .map_err(|e| format!("brew non disponible : {e}"))?;
        let update_text = combine_output(&update_out);

        emit_progress(&app, "update_apps", "running", "brew upgrade…", 60);
        let upgrade_out = Command::new("brew")
            .arg("upgrade")
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let upgrade_text = combine_output(&upgrade_out);

        let text = format!("{}\n---\n{}", update_text, upgrade_text);
        emit_progress(&app, "update_apps", "done", &text, 100);
        return Ok(text);
    }

    #[cfg(target_os = "linux")]
    {
        emit_progress(&app, "update_apps", "running", "apt update…", 20);
        let update_out = Command::new("sudo")
            .args(["apt", "update"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let update_text = combine_output(&update_out);

        emit_progress(&app, "update_apps", "running", "apt full-upgrade…", 60);
        let upgrade_out = Command::new("sudo")
            .args(["apt", "full-upgrade", "-y"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let upgrade_text = combine_output(&upgrade_out);

        let text = format!("{}\n---\n{}", update_text, upgrade_text);
        emit_progress(&app, "update_apps", "done", &text, 100);
        return Ok(text);
    }

    #[allow(unreachable_code)]
    Err("Plateforme non supportée".into())
}

// ── Analyse & Réparation Système ──────────────────────────────────────
#[tauri::command]
pub async fn maintenance_repair_system(app: tauri::AppHandle) -> Result<String, String> {
    emit_progress(&app, "repair_system", "running", "Analyse du système…", 5);

    #[cfg(windows)]
    {
        // DISM RestoreHealth en premier (peut être long ~10-20 min)
        emit_progress(&app, "repair_system", "running", "DISM RestoreHealth en cours (peut prendre plusieurs minutes)…", 10);
        let dism_out = Command::new("dism")
            .args(["/online", "/cleanup-image", "/restorehealth"])
            .creation_flags(0x0800_0000)
            .output()
            .await
            .map_err(|e| format!("DISM non disponible : {e}"))?;
        let dism_text = combine_output(&dism_out);

        emit_progress(&app, "repair_system", "running", "SFC ScanNow en cours…", 70);
        let sfc_out = Command::new("sfc")
            .arg("/scannow")
            .creation_flags(0x0800_0000)
            .output()
            .await
            .map_err(|e| format!("SFC non disponible : {e}"))?;
        let sfc_text = combine_output(&sfc_out);

        let text = format!("=== DISM ===\n{}\n\n=== SFC ===\n{}", dism_text, sfc_text);
        emit_progress(&app, "repair_system", "done", &text, 100);
        return Ok(text);
    }

    #[cfg(target_os = "macos")]
    {
        let out = Command::new("diskutil")
            .args(["verifyVolume", "/"])
            .output()
            .await
            .map_err(|e| format!("diskutil non disponible : {e}"))?;
        let text = combine_output(&out);
        emit_progress(&app, "repair_system", "done", &text, 100);
        return Ok(text);
    }

    #[cfg(target_os = "linux")]
    {
        let out = Command::new("sudo")
            .args(["dpkg", "--configure", "-a"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let text = combine_output(&out);
        emit_progress(&app, "repair_system", "done", &text, 100);
        return Ok(text);
    }

    #[allow(unreachable_code)]
    Err("Plateforme non supportée".into())
}

// ── Nettoyage du Système ─────────────────────────────────────────────
#[tauri::command]
pub async fn maintenance_clean_system(app: tauri::AppHandle) -> Result<String, String> {
    emit_progress(&app, "clean_system", "running", "Nettoyage du système…", 5);

    #[cfg(windows)]
    {
        // Enregistrer le sagerun:1 si pas encore fait (silent — ignorer les erreurs)
        let _ = Command::new("cleanmgr")
            .args(["/sageset:1"])
            .creation_flags(0x0800_0000)
            .output()
            .await;

        let out = Command::new("cleanmgr")
            .args(["/sagerun:1"])
            .creation_flags(0x0800_0000)
            .output()
            .await
            .map_err(|e| format!("cleanmgr non disponible : {e}"))?;
        let text = combine_output(&out);
        emit_progress(&app, "clean_system", "done", &text, 100);
        return Ok(text);
    }

    #[cfg(target_os = "macos")]
    {
        let out = Command::new("brew")
            .arg("cleanup")
            .output()
            .await
            .map_err(|e| format!("brew non disponible : {e}"))?;
        let text = combine_output(&out);
        emit_progress(&app, "clean_system", "done", &text, 100);
        return Ok(text);
    }

    #[cfg(target_os = "linux")]
    {
        emit_progress(&app, "clean_system", "running", "apt autoremove…", 30);
        let autoremove_out = Command::new("sudo")
            .args(["apt", "autoremove", "-y"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let autoremove_text = combine_output(&autoremove_out);

        emit_progress(&app, "clean_system", "running", "apt clean…", 70);
        let clean_out = Command::new("sudo")
            .args(["apt", "clean"])
            .output()
            .await
            .map_err(|e| e.to_string())?;
        let clean_text = combine_output(&clean_out);

        let text = format!("{}\n---\n{}", autoremove_text, clean_text);
        emit_progress(&app, "clean_system", "done", &text, 100);
        return Ok(text);
    }

    #[allow(unreachable_code)]
    Err("Plateforme non supportée".into())
}

// ── Helpers ───────────────────────────────────────────────────────────
fn emit_progress(app: &tauri::AppHandle, task: &str, status: &str, output: &str, percent: u8) {
    let _ = app.emit(
        "maintenance-progress",
        MaintenanceProgress {
            task: task.to_string(),
            status: status.to_string(),
            output: output.to_string(),
            percent,
        },
    );
}

fn combine_output(out: &std::process::Output) -> String {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let combined = if stderr.trim().is_empty() {
        stdout.trim().to_string()
    } else if stdout.trim().is_empty() {
        stderr.trim().to_string()
    } else {
        format!("{}\n{}", stdout.trim(), stderr.trim())
    };
    if combined.is_empty() {
        if out.status.success() {
            "Opération terminée avec succès.".to_string()
        } else {
            format!("Erreur (code {})", out.status.code().unwrap_or(-1))
        }
    } else {
        combined
    }
}
