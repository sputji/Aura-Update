use serde::{Deserialize, Serialize};
use tokio::process::Command;
use std::time::Duration;

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

// ── Main command ─────────────────────────────────────────────────────
#[tauri::command]
pub async fn check_updates() -> Result<Vec<UpdatePackage>, String> {
    let mut all = Vec::new();

    #[cfg(windows)]
    {
        // Timeout both checks — WU COM and winget can hang indefinitely
        let winget_fut = tokio::time::timeout(Duration::from_secs(30), check_winget());
        let wupd_fut = tokio::time::timeout(Duration::from_secs(45), check_windows_update());
        let (winget_res, wupd_res) = tokio::join!(winget_fut, wupd_fut);

        // winget: use result or empty on timeout
        let winget = winget_res.unwrap_or_default();
        all.extend(winget);

        // Windows Update: use result or empty on timeout
        let wupd = wupd_res.unwrap_or_default();

        // If a reboot is pending, skip re-listing WU items and add a single notification
        if is_reboot_pending() {
            if !wupd.is_empty() {
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
        } else {
            all.extend(wupd);
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

    Ok(all)
}

#[tauri::command]
pub async fn install_update(
    app: tauri::AppHandle,
    pkg: UpdatePackage,
) -> Result<bool, String> {
    use tauri::Emitter;
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
    result.map(|_| true)
}

// ── Install dispatcher ───────────────────────────────────────────────
async fn install_impl(pkg: &UpdatePackage) -> Result<(), String> {
    let output = match pkg.manager.as_str() {
        "winget" => {
            let mut c = Command::new("cmd");
            c.args(["/c", &format!("chcp 65001 > nul && winget upgrade --id {} --silent --accept-package-agreements --accept-source-agreements", pkg.id.strip_prefix("winget-").unwrap_or(&pkg.id))]);
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
    // Brief pause so the service can initialize
    tokio::time::sleep(Duration::from_millis(500)).await;

    let ps = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
# Run the search in a background job with a 40s timeout
$job = Start-Job -ScriptBlock {
    $s = New-Object -ComObject Microsoft.Update.Session
    $q = $s.CreateUpdateSearcher()
    try { $r = $q.Search("IsInstalled=0") } catch { return }
    $r.Updates | ForEach-Object {
        $crit = if ($_.MsrcSeverity -eq 'Critical' -or $_.IsMandatory) { 'critical' } else { 'system' }
        $ver = if ($_.Identity.UpdateID) { $_.Identity.UpdateID.Substring(0,8) } else { 'latest' }
        "$($_.Title)|$ver|$crit"
    }
}
$done = $job | Wait-Job -Timeout 40
if ($done) { Receive-Job $job } else { Stop-Job $job }
Remove-Job $job -Force -ErrorAction SilentlyContinue
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
