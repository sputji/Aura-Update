use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tauri::menu::{Menu, MenuItem};
use tauri_plugin_updater::UpdaterExt;
use tokio::process::Command;
#[cfg(windows)]
use std::time::Duration;
#[cfg(windows)]
use std::{collections::HashMap, path::{Path, PathBuf}};

use super::config::AppState;
use super::logging;

// Module Updates: réactivé (mode standard avec vérification automatique).
// Le mode confidentialité strict reste actif sur les modules IA / Remote / Telemetry.
const STRICT_PRIVACY_MODE: bool = false;

#[cfg(windows)]
const UPDATER_ENDPOINT: &str = "https://github.com/sputji/Aura-Update/releases/latest/download/updater.json";

#[cfg(windows)]
#[derive(Debug, Deserialize)]
struct UpdaterManifest {
    platforms: HashMap<String, UpdaterPlatform>,
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
struct UpdaterPlatform {
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdatePackage {
    pub id: String,
    pub name: String,
    pub current_version: String,
    pub new_version: String,
    pub manager: String,
    #[serde(rename = "type")]
    pub pkg_type: String,
    pub needs_admin: bool,
    #[serde(default)]
    pub pending_reboot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub release_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateProgress {
    pub phase: String,
    pub downloaded: u64,
    pub total: u64,
    pub percent: u64,
}

fn update_menu_label(version: Option<&str>) -> String {
    match version {
        Some(v) if !v.is_empty() => format!("🆕 Nouvelle mise à jour Aura ({v})"),
        _ => "🔄 Vérifier les mises à jour d'Aura".to_string(),
    }
}

pub fn refresh_tray_menu(app: &tauri::AppHandle, version: Option<&str>) -> Result<(), String> {
    let menu_show = MenuItem::with_id(app, "tray_show", "Ouvrir Aura Update", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu_autopilot = MenuItem::with_id(app, "tray_autopilot", "🚀 Auto-Pilote", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu_check_update = MenuItem::with_id(app, "tray_check_update", update_menu_label(version), true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu_settings = MenuItem::with_id(app, "tray_settings", "⚙️ Paramètres", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu_website = MenuItem::with_id(app, "tray_website", "🌐 Site Web", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu_quit = MenuItem::with_id(app, "tray_quit", "Quitter", true, None::<&str>)
        .map_err(|e| e.to_string())?;

    let menu = Menu::with_items(
        app,
        &[
            &menu_show,
            &menu_autopilot,
            &menu_check_update,
            &menu_settings,
            &menu_website,
            &menu_quit,
        ],
    )
    .map_err(|e| e.to_string())?;

    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_menu(Some(menu)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_tray_update_available(
    app: tauri::AppHandle,
    available: bool,
    version: Option<String>,
) -> Result<bool, String> {
    if available {
        refresh_tray_menu(&app, version.as_deref())?;
    } else {
        refresh_tray_menu(&app, None)?;
    }
    Ok(true)
}

#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle) -> Result<AppUpdateInfo, String> {
    logging::log_action_event(
        "updater-check",
        "updates",
        "check_app_update",
        "start",
        Some("check"),
        None,
        None,
        None,
        None,
        false,
        "Checking updater endpoint",
    );
    if STRICT_PRIVACY_MODE {
        return Ok(AppUpdateInfo {
            available: false,
            current_version: app.package_info().version.to_string(),
            version: None,
            release_notes: Some("Mode confidentialité stricte: vérification réseau désactivée".to_string()),
        });
    }

    let current_version = app.package_info().version.to_string();
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;

    if let Some(update) = update {
        let info = AppUpdateInfo {
            available: true,
            current_version,
            version: Some(update.version.to_string()),
            release_notes: update.body.clone(),
        };
        let _ = refresh_tray_menu(&app, info.version.as_deref());
        let _ = app.emit("app-update-available", &info);
        logging::log_action_event(
            "updater-check",
            "updates",
            "check_app_update",
            "done",
            Some("available"),
            None,
            None,
            None,
            None,
            false,
            &format!("Update available: {}", info.version.clone().unwrap_or_default()),
        );
        return Ok(info);
    }

    let _ = refresh_tray_menu(&app, None);
    logging::log_action_event(
        "updater-check",
        "updates",
        "check_app_update",
        "done",
        Some("none"),
        None,
        None,
        None,
        None,
        false,
        "No update available",
    );
    Ok(AppUpdateInfo {
        available: false,
        current_version,
        version: None,
        release_notes: None,
    })
}

#[tauri::command]
pub async fn install_app_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    logging::log_action_event(
        "updater-install",
        "updates",
        "install_app_update",
        "start",
        Some("install"),
        None,
        None,
        None,
        None,
        false,
        "Starting updater install",
    );
    if STRICT_PRIVACY_MODE {
        return Err("Mode confidentialité stricte: installation réseau désactivée".into());
    }

    {
        let mut guard = state.app_update_in_progress.lock().unwrap();
        if *guard {
            return Err("Une mise à jour est déjà en cours".into());
        }
        *guard = true;
    }

    let result = async {
        let update = app
            .updater()
            .map_err(|e| e.to_string())?
            .check()
            .await
            .map_err(|e| e.to_string())?;

        #[cfg(windows)]
        {
            if update.is_none() {
                let _ = app.emit("app-update-progress", AppUpdateProgress {
                    phase: "none".to_string(),
                    downloaded: 0,
                    total: 0,
                    percent: 0,
                });
                return Err("Aucune nouvelle version disponible".into());
            }

            let _ = app.emit("app-update-progress", AppUpdateProgress {
                phase: "starting".to_string(),
                downloaded: 0,
                total: 0,
                percent: 0,
            });

            install_windows_update_in_place(&app, &state).await?;

            let _ = app.emit("app-update-progress", AppUpdateProgress {
                phase: "done".to_string(),
                downloaded: 0,
                total: 0,
                percent: 100,
            });

            let _ = refresh_tray_menu(&app, None);
            return Ok::<bool, String>(true);
        }

        #[cfg(not(windows))]
        let Some(update) = update else {
            let _ = app.emit("app-update-progress", AppUpdateProgress {
                phase: "none".to_string(),
                downloaded: 0,
                total: 0,
                percent: 0,
            });
            return Err("Aucune nouvelle version disponible".into());
        };

        #[cfg(not(windows))]
        let _ = app.emit("app-update-progress", AppUpdateProgress {
            phase: "starting".to_string(),
            downloaded: 0,
            total: 0,
            percent: 0,
        });

        #[cfg(not(windows))]
        update
            .download_and_install(
                |chunk_length, content_length| {
                    let downloaded = chunk_length as u64;
                    let total = content_length.unwrap_or(0);
                    let percent = if total > 0 {
                        downloaded.saturating_mul(100) / total
                    } else {
                        0
                    };
                    let _ = app.emit("app-update-progress", AppUpdateProgress {
                        phase: "downloading".to_string(),
                        downloaded,
                        total,
                        percent,
                    });
                },
                || {
                    let _ = app.emit("app-update-progress", AppUpdateProgress {
                        phase: "installing".to_string(),
                        downloaded: 0,
                        total: 0,
                        percent: 100,
                    });
                },
            )
            .await
            .map_err(|e| e.to_string())?;

        #[cfg(not(windows))]
        {
            let _ = app.emit("app-update-progress", AppUpdateProgress {
                phase: "done".to_string(),
                downloaded: 0,
                total: 0,
                percent: 100,
            });

            let _ = refresh_tray_menu(&app, None);
            return Ok::<bool, String>(true);
        }

        #[cfg(windows)]
        unreachable!("Windows flow returns earlier");
    }
    .await;

    *state.app_update_in_progress.lock().unwrap() = false;

    if result.is_ok() {
        logging::log_action_event(
            "updater-install",
            "updates",
            "install_app_update",
            "done",
            Some("restart"),
            None,
            None,
            None,
            None,
            false,
            "Updater install finished; app restart requested",
        );
        app.restart();
    } else if let Err(e) = &result {
        logging::log_action_event(
            "updater-install",
            "updates",
            "install_app_update",
            "error",
            Some("failed"),
            None,
            None,
            None,
            None,
            false,
            e,
        );
    }

    result
}

#[cfg(windows)]
async fn install_windows_update_in_place(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, AppState>,
) -> Result<(), String> {
    let install_dir = preferred_install_dir(state)?;
    let manifest = fetch_updater_manifest().await?;

    let asset_url = manifest
        .platforms
        .get("windows-x86_64")
        .or_else(|| manifest.platforms.get("windows-x86_64-msi"))
        .or_else(|| manifest.platforms.get("windows-x86_64-nsis"))
        .map(|p| p.url.clone())
        .ok_or_else(|| "Aucun payload Windows trouvé dans updater.json".to_string())?;

    logging::log_action_event(
        "updater-install",
        "updates",
        "install_app_update",
        "progress",
        Some("download-installer"),
        None,
        None,
        None,
        None,
        false,
        &format!("Installer URL: {asset_url}"),
    );

    let payload = reqwest::get(&asset_url)
        .await
        .map_err(|e| format!("Téléchargement impossible: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("Lecture payload impossible: {e}"))?;

    let installer = extract_windows_installer_from_payload(&asset_url, &payload)?;

    let install_dir_str = install_dir.to_string_lossy().to_string();
    let mut cmd = if installer
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("msi"))
    {
        let mut c = Command::new("msiexec");
        c.args([
            "/i",
            installer
                .to_str()
                .ok_or_else(|| "Chemin MSI invalide".to_string())?,
            "/qn",
            "/norestart",
            &format!("TARGETDIR={install_dir_str}"),
        ]);
        c
    } else {
        let mut c = Command::new(
            installer
                .to_str()
                .ok_or_else(|| "Chemin EXE invalide".to_string())?,
        );
        c.args(["/S", &format!("/D={install_dir_str}")]);
        c
    };

    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);

    let output = cmd
        .output()
        .await
        .map_err(|e| format!("Installation impossible: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("Installateur terminé avec code {:?}", output.status.code())
        } else {
            stderr
        });
    }

    let _ = app.emit("app-update-progress", AppUpdateProgress {
        phase: "installing".to_string(),
        downloaded: 0,
        total: 0,
        percent: 100,
    });

    Ok(())
}

#[cfg(windows)]
fn preferred_install_dir(state: &tauri::State<'_, AppState>) -> Result<PathBuf, String> {
    let configured = {
        let cfg = state.config.lock().unwrap();
        cfg.update_install_dir.trim().to_string()
    };

    if !configured.is_empty() {
        let p = PathBuf::from(configured);
        if std::fs::create_dir_all(&p).is_ok() {
            return Ok(p);
        }
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Impossible de déterminer le dossier d'installation courant".to_string())
}

#[cfg(windows)]
async fn fetch_updater_manifest() -> Result<UpdaterManifest, String> {
    reqwest::get(UPDATER_ENDPOINT)
        .await
        .map_err(|e| format!("Impossible de lire updater.json: {e}"))?
        .json::<UpdaterManifest>()
        .await
        .map_err(|e| format!("updater.json invalide: {e}"))
}

#[cfg(windows)]
fn extract_windows_installer_from_payload(url: &str, payload: &[u8]) -> Result<PathBuf, String> {
    let tmp = std::env::temp_dir().join("aura-update-updater");
    let _ = std::fs::create_dir_all(&tmp);

    if url.ends_with(".zip") {
        let zip_path = tmp.join("aura-update-installer.zip");
        std::fs::write(&zip_path, payload).map_err(|e| e.to_string())?;

        let extract_dir = tmp.join("extract");
        let _ = std::fs::remove_dir_all(&extract_dir);
        let _ = std::fs::create_dir_all(&extract_dir);

        let ps = format!(
            "$zip='{}';$dst='{}';Expand-Archive -Path $zip -DestinationPath $dst -Force;Get-ChildItem -Path $dst -Recurse -File | Where-Object {{ $_.Extension -in '.msi','.exe' }} | Select-Object -First 1 -ExpandProperty FullName",
            zip_path.display(),
            extract_dir.display()
        );

        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .output()
            .map_err(|e| format!("Extraction ZIP impossible: {e}"))?;

        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
        }

        let installer_path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if installer_path.is_empty() {
            return Err("Archive updater.zip sans .msi/.exe".to_string());
        }
        return Ok(PathBuf::from(installer_path));
    }

    let lower = url.to_lowercase();
    let ext = if lower.ends_with(".msi") { "msi" } else { "exe" };
    let out = tmp.join(format!("aura-update-installer.{ext}"));
    std::fs::write(&out, payload).map_err(|e| e.to_string())?;
    Ok(out)
}

// ── Main command ─────────────────────────────────────────────────────
#[tauri::command]
pub async fn check_updates() -> Result<Vec<UpdatePackage>, String> {
    logging::log_action_event(
        "updates-scan",
        "updates",
        "check_updates",
        "start",
        Some("scan"),
        None,
        None,
        None,
        None,
        false,
        "Scanning updates across package managers",
    );
    if STRICT_PRIVACY_MODE {
        return Ok(Vec::new());
    }

    let mut all = Vec::new();

    #[cfg(windows)]
    {
        // Timeout both checks — WU COM can take 60-90s on real scans
        let winget_fut = tokio::time::timeout(Duration::from_secs(30), check_winget());
        let wupd_fut = tokio::time::timeout(Duration::from_secs(120), check_windows_update());
        let (winget_res, wupd_res) = tokio::join!(winget_fut, wupd_fut);

        // winget: use result or empty on timeout
        let winget = winget_res.unwrap_or_default();
        all.extend(winget);

        // Windows Update: use result or empty on timeout
        let wupd = wupd_res.unwrap_or_default();

        // Always include found updates
        all.extend(wupd);

        // If a reboot is also pending, add a notification
        if is_reboot_pending() {
            all.push(UpdatePackage {
                id: "wu-reboot".into(),
                name: "Redémarrage requis pour finaliser les mises à jour".into(),
                current_version: String::new(),
                new_version: String::new(),
                manager: "windows-update".into(),
                pkg_type: "reboot".into(),
                needs_admin: false,
                pending_reboot: true,
            });
        }
    }

    #[cfg(target_os = "linux")]
    {
        let (apt, dnf, pac, snap, flat) = tokio::join!(
            check_apt(),
            check_dnf(),
            check_pacman(),
            check_snap(),
            check_flatpak()
        );
        all.extend(apt);
        all.extend(dnf);
        all.extend(pac);
        all.extend(snap);
        all.extend(flat);
    }

    #[cfg(target_os = "macos")]
    {
        let (brew, su, mas) = tokio::join!(check_brew(), check_softwareupdate(), check_mas());
        all.extend(brew);
        all.extend(su);
        all.extend(mas);
    }

    logging::log_action_event(
        "updates-scan",
        "updates",
        "check_updates",
        "done",
        Some("scan"),
        None,
        None,
        None,
        None,
        false,
        &format!("Found {} update(s)", all.len()),
    );
    Ok(all)
}

#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    pkg: UpdatePackage,
) -> Result<bool, String> {
    logging::log_action_event(
        &format!("pkg-{}", pkg.id),
        "updates",
        "install_update",
        "start",
        Some("package"),
        None,
        None,
        None,
        None,
        false,
        &format!("Installing {} via {}", pkg.name, pkg.manager),
    );
    if STRICT_PRIVACY_MODE {
        return Err("Mode confidentialité stricte: installation réseau désactivée".into());
    }

    let id = pkg.id.clone();

    app.emit("update-progress", serde_json::json!({
        "id": &id, "status": "running", "message": format!("Installing {}…", &pkg.name)
    })).ok();

    let result = install_impl(&pkg).await;

    match &result {
        Ok(_) => {
            app.emit("update-progress", serde_json::json!({
                "id": &id, "status": "finished", "message": format!("{} installed", &pkg.name)
            })).ok();
        }
        Err(e) => {
            app.emit("update-progress", serde_json::json!({
                "id": &id, "status": "error", "message": e.clone()
            })).ok();
        }
    }
    if result.is_ok() {
        logging::log_action_event(
            &format!("pkg-{}", pkg.id),
            "updates",
            "install_update",
            "done",
            Some("package"),
            None,
            None,
            None,
            None,
            false,
            &format!("Installed {}", pkg.name),
        );
    } else if let Err(e) = &result {
        logging::log_action_event(
            &format!("pkg-{}", pkg.id),
            "updates",
            "install_update",
            "error",
            Some("package"),
            None,
            None,
            None,
            None,
            false,
            e,
        );
    }
    result.map(|_| true)
}

// ── Install dispatcher ───────────────────────────────────────────────
async fn install_impl(pkg: &UpdatePackage) -> Result<(), String> {
    let output = match pkg.manager.as_str() {
        "winget" => {
            let mut c = Command::new("cmd");
            c.args(["/c", &format!("chcp 65001 > nul && winget upgrade --id {} --silent --force --accept-package-agreements --accept-source-agreements", pkg.id.strip_prefix("winget-").unwrap_or(&pkg.id))]);
            #[cfg(windows)]
            c.creation_flags(0x0800_0000);
            c.output().await
        }
        "windows-update" => {
            let ps = format!(
                "$s=New-Object -ComObject Microsoft.Update.Session;\
                 $u=$s.CreateUpdateSearcher();\
                 $r=$u.Search(\"IsInstalled=0\");\
                 $dl=New-Object -ComObject Microsoft.Update.UpdateColl;\
                 foreach($upd in $r.Updates){{if($upd.Title -like '*{}*'){{$dl.Add($upd)|Out-Null}}}}\
                 $d=$s.CreateUpdateDownloader();$d.Updates=$dl;$d.Download();\
                 $i=$s.CreateUpdateInstaller();$i.Updates=$dl;$i.Install()",
                pkg.name.replace('\'', "''")
            );
            {
                let mut c = Command::new("powershell");
                c.args(["-NoProfile", "-Command", &ps]);
                #[cfg(windows)]
                c.creation_flags(0x0800_0000);
                c.output().await
            }
        }
        "apt" => {
            Command::new("apt-get")
                .args(["install", "-y", &pkg.name])
                .output().await
        }
        "dnf" => {
            Command::new("dnf")
                .args(["upgrade", "-y", &pkg.name])
                .output().await
        }
        "pacman" => {
            Command::new("pacman")
                .args(["-Syu", "--noconfirm", &pkg.name])
                .output().await
        }
        "snap" => {
            Command::new("snap")
                .args(["refresh", &pkg.name])
                .output().await
        }
        "flatpak" => {
            Command::new("flatpak")
                .args(["update", "-y", &pkg.id.strip_prefix("flatpak-").unwrap_or(&pkg.id)])
                .output().await
        }
        "brew" => {
            Command::new("brew")
                .args(["upgrade", &pkg.name])
                .output().await
        }
        "softwareupdate" => {
            Command::new("softwareupdate")
                .args(["--install", &pkg.name])
                .output().await
        }
        "mas" => {
            Command::new("mas")
                .args(["upgrade", &pkg.id.strip_prefix("mas-").unwrap_or(&pkg.id)])
                .output().await
        }
        _ => return Err(format!("Unknown manager: {}", pkg.manager)),
    };

    match output {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

// ══════════════════════════════════════════════════════════════════════
// WINDOWS
// ══════════════════════════════════════════════════════════════════════
#[cfg(windows)]
async fn check_winget() -> Vec<UpdatePackage> {
    let out = Command::new("cmd")
        .args(["/c", "chcp 65001 > nul && winget upgrade --include-unknown --accept-source-agreements --accept-package-agreements"])
        .creation_flags(0x0800_0000)
        .kill_on_drop(true)
        .output()
        .await;

    match out {
        Ok(o) if o.status.success() => parse_winget(&String::from_utf8_lossy(&o.stdout)),
        _ => Vec::new(),
    }
}

#[cfg(windows)]
fn parse_winget(text: &str) -> Vec<UpdatePackage> {
    let mut pkgs = Vec::new();
    let lines: Vec<&str> = text.lines().collect();

    let sep = lines.iter().position(|l| l.starts_with("---"));
    let Some(sep_idx) = sep else { return pkgs };
    let header = lines.get(sep_idx.wrapping_sub(1)).copied().unwrap_or("");

    let id_col = header.find("Id").unwrap_or(40);
    let ver_col = header.find("Version").unwrap_or(70);
    let avail_col = header
        .find("Available")
        .or_else(|| header.find("Disponible"))
        .unwrap_or(90);
    let source_col = header.find("Source").unwrap_or(usize::MAX);

    for line in &lines[sep_idx + 1..] {
        if line.trim().is_empty() || line.contains("upgrade(s)") || line.contains("mise(s)") {
            continue;
        }
        let name = line.get(..id_col).unwrap_or("").trim();
        let id = line.get(id_col..ver_col).unwrap_or("").trim();
        let cur = line.get(ver_col..avail_col).unwrap_or("").trim();
        let new = line
            .get(avail_col..source_col.min(line.len()))
            .unwrap_or("")
            .trim();

        if !id.is_empty() && !name.is_empty() && !new.is_empty() {
            pkgs.push(UpdatePackage {
                id: format!("winget-{id}"),
                name: name.to_string(),
                current_version: cur.to_string(),
                new_version: new.to_string(),
                manager: "winget".into(),
                pkg_type: "app".into(),
                needs_admin: false,
                pending_reboot: false,
            });
        }
    }
    pkgs
}

#[cfg(windows)]
async fn check_windows_update() -> Vec<UpdatePackage> {
    // Ensure wuauserv + BITS are running (may have been stopped by Turbo Mode)
    let _ = Command::new("sc")
        .args(["start", "wuauserv"])
        .creation_flags(0x0800_0000)
        .output()
        .await;
    let _ = Command::new("sc")
        .args(["start", "BITS"])
        .creation_flags(0x0800_0000)
        .output()
        .await;
    // Give wuauserv enough time to fully initialize before COM search
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Run COM search directly — kill_on_drop + Rust timeout (120 s) are the safety net
    let ps = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
try {
    $s = New-Object -ComObject Microsoft.Update.Session
    $q = $s.CreateUpdateSearcher()
    $r = $q.Search("IsInstalled=0")
    foreach ($u in $r.Updates) {
        $crit = if ($u.MsrcSeverity -eq 'Critical' -or $u.IsMandatory) { 'critical' } else { 'system' }
        $ver  = if ($u.Identity.UpdateID) { $u.Identity.UpdateID.Substring(0,8) } else { 'latest' }
        "$($u.Title)|$ver|$crit"
    }
} catch {
    # Silently fail — Rust kill_on_drop is the safety net
}
"#;
    let out = Command::new("powershell")
        .args(["-NoProfile", "-Command", ps])
        .creation_flags(0x0800_0000)
        .kill_on_drop(true)
        .output()
        .await;

    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .filter(|l| !l.trim().is_empty() && l.contains('|'))
                .enumerate()
                .map(|(i, l)| {
                    let parts: Vec<&str> = l.splitn(3, '|').collect();
                    let title = parts.first().unwrap_or(&"").trim();
                    let ver = parts.get(1).unwrap_or(&"latest").trim();
                    let kind = parts.get(2).unwrap_or(&"system").trim();
                    UpdatePackage {
                        id: format!("wu-{i}"),
                        name: title.to_string(),
                        current_version: "installed".into(),
                        new_version: ver.to_string(),
                        manager: "windows-update".into(),
                        pkg_type: kind.to_string(),
                        needs_admin: true,
                        pending_reboot: false,
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

// ══════════════════════════════════════════════════════════════════════
// LINUX
// ══════════════════════════════════════════════════════════════════════
#[cfg(target_os = "linux")]
async fn has_cmd(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
async fn check_apt() -> Vec<UpdatePackage> {
    if !has_cmd("apt").await { return Vec::new(); }
    let _ = Command::new("apt-get").args(["update", "-qq"]).output().await;
    let out = Command::new("apt")
        .args(["list", "--upgradable"])
        .output()
        .await;
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .filter(|l| l.contains("[upgradable"))
                .filter_map(|l| {
                    let name = l.split('/').next()?.trim();
                    let rest = l.splitn(2, ' ').nth(1)?;
                    let ver = rest.split_whitespace().next().unwrap_or("?");
                    let cur = l.split('[').nth(1)
                        .and_then(|s| s.strip_suffix(']'))
                        .unwrap_or("?");
                    Some(UpdatePackage {
                        id: format!("apt-{name}"),
                        name: name.to_string(),
                        current_version: cur.to_string(),
                        new_version: ver.to_string(),
                        manager: "apt".into(),
                        pkg_type: "app".into(),
                        needs_admin: true,
                        pending_reboot: false,
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "linux")]
async fn check_dnf() -> Vec<UpdatePackage> {
    if !has_cmd("dnf").await { return Vec::new(); }
    let out = Command::new("dnf")
        .args(["check-update", "-q"])
        .output()
        .await;
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .filter(|l| !l.trim().is_empty() && !l.starts_with("Last metadata"))
                .filter_map(|l| {
                    let mut parts = l.split_whitespace();
                    let name_arch = parts.next()?;
                    let name = name_arch.split('.').next().unwrap_or(name_arch);
                    let ver = parts.next()?;
                    Some(UpdatePackage {
                        id: format!("dnf-{name}"),
                        name: name.to_string(),
                        current_version: "installed".into(),
                        new_version: ver.to_string(),
                        manager: "dnf".into(),
                        pkg_type: "app".into(),
                        needs_admin: true,
                        pending_reboot: false,
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "linux")]
async fn check_pacman() -> Vec<UpdatePackage> {
    if !has_cmd("pacman").await { return Vec::new(); }
    let out = Command::new("pacman").args(["-Qu"]).output().await;
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .filter_map(|l| {
                    let mut p = l.split_whitespace();
                    let name = p.next()?;
                    let cur = p.next()?;
                    let _ = p.next(); // ->
                    let new = p.next()?;
                    Some(UpdatePackage {
                        id: format!("pac-{name}"),
                        name: name.to_string(),
                        current_version: cur.to_string(),
                        new_version: new.to_string(),
                        manager: "pacman".into(),
                        pkg_type: "app".into(),
                        needs_admin: true,
                        pending_reboot: false,
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "linux")]
async fn check_snap() -> Vec<UpdatePackage> {
    if !has_cmd("snap").await { return Vec::new(); }
    let out = Command::new("snap").args(["refresh", "--list"]).output().await;
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .skip(1)
                .filter_map(|l| {
                    let mut p = l.split_whitespace();
                    let name = p.next()?;
                    let ver = p.next()?;
                    Some(UpdatePackage {
                        id: format!("snap-{name}"),
                        name: name.to_string(),
                        current_version: "installed".into(),
                        new_version: ver.to_string(),
                        manager: "snap".into(),
                        pkg_type: "app".into(),
                        needs_admin: false,
                        pending_reboot: false,
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "linux")]
async fn check_flatpak() -> Vec<UpdatePackage> {
    if !has_cmd("flatpak").await { return Vec::new(); }
    let out = Command::new("flatpak")
        .args(["remote-ls", "--updates", "--columns=application,version"])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .filter(|l| !l.trim().is_empty())
                .filter_map(|l| {
                    let mut p = l.split('\t');
                    let id = p.next()?.trim();
                    let ver = p.next().unwrap_or("latest").trim();
                    let name = id.rsplit('.').next().unwrap_or(id);
                    Some(UpdatePackage {
                        id: format!("flatpak-{id}"),
                        name: name.to_string(),
                        current_version: "installed".into(),
                        new_version: ver.to_string(),
                        manager: "flatpak".into(),
                        pkg_type: "app".into(),
                        needs_admin: false,
                        pending_reboot: false,
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

// ══════════════════════════════════════════════════════════════════════
// macOS
// ══════════════════════════════════════════════════════════════════════
#[cfg(target_os = "macos")]
async fn has_cmd_mac(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
async fn check_brew() -> Vec<UpdatePackage> {
    if !has_cmd_mac("brew").await { return Vec::new(); }
    let _ = Command::new("brew").arg("update").output().await;
    let out = Command::new("brew")
        .args(["outdated", "--json=v2"])
        .output()
        .await;
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                let mut pkgs = Vec::new();
                if let Some(formulae) = json.get("formulae").and_then(|f| f.as_array()) {
                    for f in formulae {
                        let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let cur = f
                            .get("installed_versions")
                            .and_then(|v| v.as_array())
                            .and_then(|a| a.first())
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        let new = f
                            .get("current_version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("latest");
                        pkgs.push(UpdatePackage {
                            id: format!("brew-{name}"),
                            name: name.to_string(),
                            current_version: cur.to_string(),
                            new_version: new.to_string(),
                            manager: "brew".into(),
                            pkg_type: "app".into(),
                            needs_admin: false,
                            pending_reboot: false,
                        });
                    }
                }
                if let Some(casks) = json.get("casks").and_then(|c| c.as_array()) {
                    for c in casks {
                        let name = c.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let cur = c
                            .get("installed_versions")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?");
                        let new = c
                            .get("current_version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("latest");
                        pkgs.push(UpdatePackage {
                            id: format!("brew-{name}"),
                            name: name.to_string(),
                            current_version: cur.to_string(),
                            new_version: new.to_string(),
                            manager: "brew".into(),
                            pkg_type: "app".into(),
                            needs_admin: false,
                            pending_reboot: false,
                        });
                    }
                }
                pkgs
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "macos")]
async fn check_softwareupdate() -> Vec<UpdatePackage> {
    let out = Command::new("softwareupdate")
        .args(["--list"])
        .output()
        .await;
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stderr).to_string()
                + &String::from_utf8_lossy(&o.stdout);
            let mut pkgs = Vec::new();
            let mut current_label = String::new();
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("* Label:") {
                    current_label = trimmed.strip_prefix("* Label:").unwrap_or("").trim().to_string();
                } else if trimmed.starts_with("Title:") || trimmed.contains("Version:") {
                    if !current_label.is_empty() {
                        let is_critical = text.contains("Recommended: YES");
                        pkgs.push(UpdatePackage {
                            id: format!("su-{}", pkgs.len()),
                            name: current_label.clone(),
                            current_version: "N/A".into(),
                            new_version: "latest".into(),
                            manager: "softwareupdate".into(),
                            pkg_type: if is_critical { "critical" } else { "system" }.into(),
                            needs_admin: true,
                            pending_reboot: false,
                        });
                        current_label.clear();
                    }
                }
            }
            if !current_label.is_empty() {
                pkgs.push(UpdatePackage {
                    id: format!("su-{}", pkgs.len()),
                    name: current_label,
                    current_version: "N/A".into(),
                    new_version: "latest".into(),
                    manager: "softwareupdate".into(),
                    pkg_type: "system".into(),
                    needs_admin: true,
                    pending_reboot: false,
                });
            }
            pkgs
        }
        _ => Vec::new(),
    }
}

#[cfg(target_os = "macos")]
async fn check_mas() -> Vec<UpdatePackage> {
    if !has_cmd_mac("mas").await { return Vec::new(); }
    let out = Command::new("mas").arg("outdated").output().await;
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.lines()
                .filter_map(|l| {
                    let mut parts = l.splitn(2, ' ');
                    let id = parts.next()?.trim();
                    let rest = parts.next()?.trim();
                    let name = rest.split('(').next()?.trim();
                    Some(UpdatePackage {
                        id: format!("mas-{id}"),
                        name: name.to_string(),
                        current_version: "installed".into(),
                        new_version: "latest".into(),
                        manager: "mas".into(),
                        pkg_type: "app".into(),
                        needs_admin: false,
                        pending_reboot: false,
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

// ══════════════════════════════════════════════════════════════════════
// Pending reboot detection (Windows)
// ══════════════════════════════════════════════════════════════════════
#[cfg(windows)]
fn is_reboot_pending() -> bool {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};
    use winreg::RegKey;

    // Check Windows Update reboot flag
    let wu_reboot = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired",
            KEY_READ,
        )
        .is_ok();

    // Check Component-Based Servicing reboot flag
    let cbs_reboot = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending",
            KEY_READ,
        )
        .is_ok();

    wu_reboot || cbs_reboot
}
