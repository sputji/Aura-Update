use super::config::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::oneshot;
use rand::distributions::Alphanumeric;
use rand::Rng;
use warp::Filter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteInfo {
    pub url: String,
    pub qr_svg: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStatus {
    pub running: bool,
    pub url: String,
    pub port: u16,
}

/// Unified state for remote polling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteState {
    pub health_score: u8,
    pub cpu_temp: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub battery_percent: Option<u8>,
    pub battery_charging: bool,
    pub cool_boost_active: bool,
    pub cool_boost_cooldown: bool,
    pub pending_updates: usize,
    pub lang: String,
}

// Global shutdown sender — stored so stop_remote can signal the server to quit
static SHUTDOWN_TX: std::sync::Mutex<Option<oneshot::Sender<()>>> = std::sync::Mutex::new(None);
// Global security token
static REMOTE_TOKEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
// Global language for remote dashboard
static REMOTE_LANG: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn generate_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// Start the remote web dashboard server.
#[tauri::command]
pub async fn start_remote(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<RemoteInfo, String> {
    // Check if already running — stop previous server first to avoid stale state
    let was_running = {
        let port = state.remote_port.lock().unwrap();
        port.is_some()
    };

    if was_running {
        if let Some(tx) = SHUTDOWN_TX.lock().unwrap().take() {
            let _ = tx.send(());
        }
        *state.remote_port.lock().unwrap() = None;
        *REMOTE_TOKEN.lock().unwrap() = None;
        // Brief delay to let the old server release the port
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    let local_ip = local_ip_address::local_ip().map_err(|e| e.to_string())?;
    let port = find_free_port().await?;

    // Generate security token
    let token = generate_token();
    *REMOTE_TOKEN.lock().unwrap() = Some(token.clone());

    // Store language from config
    {
        let cfg = state.config.lock().unwrap();
        *REMOTE_LANG.lock().unwrap() = Some(cfg.language.clone());
    }

    let url = format!("http://{}:{}/?t={}", local_ip, port, token);

    // Generate QR code as SVG
    let qr = qrcode::QrCode::new(url.as_bytes()).map_err(|e| e.to_string())?;
    let svg = qr
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(200, 200)
        .dark_color(qrcode::render::svg::Color("#58a6ff"))
        .light_color(qrcode::render::svg::Color("#121212"))
        .build();

    // Store port
    *state.remote_port.lock().unwrap() = Some(port);

    // Create shutdown channel
    let (tx, rx) = oneshot::channel::<()>();
    *SHUTDOWN_TX.lock().unwrap() = Some(tx);

    // Spawn server in a DETACHED async task via tauri runtime
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        run_server(port, app_handle, rx).await;
    });

    Ok(RemoteInfo {
        url,
        qr_svg: svg,
        port,
    })
}

/// Stop the remote web dashboard server.
#[tauri::command]
pub async fn stop_remote(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    // Send shutdown signal to warp
    if let Some(tx) = SHUTDOWN_TX.lock().unwrap().take() {
        let _ = tx.send(());
    }
    let mut port = state.remote_port.lock().unwrap();
    *port = None;
    // Clear token
    *REMOTE_TOKEN.lock().unwrap() = None;
    Ok(true)
}

/// Get current remote server status.
#[tauri::command]
pub fn get_remote_status(state: tauri::State<'_, AppState>) -> RemoteStatus {
    let port = state.remote_port.lock().unwrap();
    match *port {
        Some(p) => {
            let ip = local_ip_address::local_ip()
                .map(|ip| format!("http://{}:{}", ip, p))
                .unwrap_or_default();
            RemoteStatus { running: true, url: ip, port: p }
        }
        None => RemoteStatus { running: false, url: String::new(), port: 0 },
    }
}

// ── Token validation warp filter ─────────────────────────────────────

#[derive(Debug)]
struct TokenError;
impl warp::reject::Reject for TokenError {}

/// Warp filter that validates the security token from query `?t=` or `Authorization` header.
/// Rejects with 403 if the token is missing or invalid.
fn with_token() -> impl warp::Filter<Extract = ((),), Error = warp::Rejection> + Clone {
    warp::query::raw()
        .or(warp::any().map(|| String::new()))
        .unify()
        .and(warp::header::optional::<String>("authorization"))
        .and_then(|query: String, auth_header: Option<String>| async move {
            let expected = REMOTE_TOKEN.lock().unwrap().clone();
            if let Some(ref tok) = expected {
                // Check Authorization header
                if let Some(ref hv) = auth_header {
                    let trimmed = hv.trim();
                    if trimmed == tok || trimmed == format!("Bearer {}", tok) {
                        return Ok(());
                    }
                }
                // Check query param ?t=TOKEN
                let found = query.split('&')
                    .find_map(|pair| {
                        let mut parts = pair.splitn(2, '=');
                        if parts.next() == Some("t") { parts.next() } else { None }
                    });
                if found == Some(tok.as_str()) {
                    return Ok(());
                }
            }
            Err(warp::reject::custom(TokenError))
        })
}

async fn handle_rejection(err: warp::Rejection) -> Result<impl warp::Reply, std::convert::Infallible> {
    if err.find::<TokenError>().is_some() {
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "unauthorized"})),
            warp::http::StatusCode::FORBIDDEN,
        ))
    } else {
        Ok(warp::reply::with_status(
            warp::reply::json(&serde_json::json!({"error": "not found"})),
            warp::http::StatusCode::NOT_FOUND,
        ))
    }
}

// ── Unified state gatherer ───────────────────────────────────────────
fn gather_state() -> RemoteState {
    let vitals = super::health::get_system_vitals();
    // Use cached full health score if available, otherwise fall back to sync estimate
    let health_score = super::health::get_cached_health_score()
        .unwrap_or_else(super::health::get_health_score_sync);
    let lang = REMOTE_LANG.lock().unwrap().clone().unwrap_or_else(|| "fr".into());

    RemoteState {
        health_score,
        cpu_temp: vitals.cpu_temp,
        gpu_temp: vitals.gpu_temp,
        battery_percent: vitals.battery_percent,
        battery_charging: vitals.battery_charging,
        cool_boost_active: false,
        cool_boost_cooldown: false,
        pending_updates: 0,
        lang,
    }
}

// ── Screenshot capture ───────────────────────────────────────────────

/// Opens a PNG from `path`, deletes it, resizes to 800×600 max, encodes as JPEG.
fn resize_to_jpeg(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let img = image::open(path).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(path);
    let resized = img.thumbnail(800, 600);
    let mut buf = std::io::Cursor::new(Vec::new());
    resized
        .write_to(&mut buf, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    Ok(buf.into_inner())
}

/// Temporary screenshot path shared by all platforms.
fn screenshot_path() -> std::path::PathBuf {
    std::env::temp_dir().join("aura_screenshot.png")
}

#[cfg(target_os = "windows")]
fn capture_screenshot_jpeg() -> Result<Vec<u8>, String> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let path = screenshot_path();
    let path_str = path.to_string_lossy().to_string();

    let ps_script = format!(
        r#"Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $s = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $b = New-Object System.Drawing.Bitmap($s.Width,$s.Height); $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen($s.Location,$([System.Drawing.Point]::Empty),$s.Size); $b.Save('{}'); $g.Dispose(); $b.Dispose()"#,
        path_str.replace('\'', "''")
    );

    let result = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;

    if !result.status.success() {
        return Err("Screenshot capture failed".into());
    }

    resize_to_jpeg(&path)
}

#[cfg(target_os = "macos")]
fn capture_screenshot_jpeg() -> Result<Vec<u8>, String> {
    use std::process::Command;

    let path = screenshot_path();
    let path_str = path.to_string_lossy().to_string();

    Command::new("screencapture")
        .args(["-x", &path_str])
        .output()
        .map_err(|e| e.to_string())?;

    resize_to_jpeg(&path)
}

#[cfg(target_os = "linux")]
fn capture_screenshot_jpeg() -> Result<Vec<u8>, String> {
    use std::process::Command;

    let path = screenshot_path();
    let path_str = path.to_string_lossy().to_string();

    // Try gnome-screenshot first, then fall back to scrot
    let result = Command::new("gnome-screenshot")
        .args(["-f", &path_str])
        .output();

    if result.is_err() || !result.as_ref().unwrap().status.success() {
        Command::new("scrot")
            .arg(&path_str)
            .output()
            .map_err(|e| e.to_string())?;
    }

    resize_to_jpeg(&path)
}

// ── Power actions ────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
fn power_sleep() -> Result<(), String> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("rundll32.exe")
        .args(["powrprof.dll,SetSuspendState", "0,1,0"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn power_shutdown() -> Result<(), String> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("shutdown")
        .args(["/s", "/t", "60", "/c", "Aura Update — Shutdown in 60s"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn power_sleep() -> Result<(), String> {
    use std::process::Command;
    Command::new("pmset").arg("sleepnow").output().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn power_shutdown() -> Result<(), String> {
    use std::process::Command;
    Command::new("osascript")
        .args(["-e", r#"tell application "System Events" to shut down"#])
        .output()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn power_sleep() -> Result<(), String> {
    use std::process::Command;
    Command::new("systemctl").arg("suspend").output().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn power_shutdown() -> Result<(), String> {
    use std::process::Command;
    Command::new("shutdown").args(["+1"]).output().map_err(|e| e.to_string())?;
    Ok(())
}

// ── Lock ──────────────────────────────────────────────────────────────
#[cfg(target_os = "windows")]
fn power_lock() -> Result<(), String> {
    use std::process::Command;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    Command::new("rundll32.exe")
        .args(["user32.dll,LockWorkStation"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn power_lock() -> Result<(), String> {
    use std::process::Command;
    Command::new("pmset").arg("displaysleepnow").output().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn power_lock() -> Result<(), String> {
    use std::process::Command;
    Command::new("loginctl").arg("lock-session").output().map_err(|e| e.to_string())?;
    Ok(())
}

// ── Internal: HTTP server ────────────────────────────────────────────
async fn run_server(port: u16, app: tauri::AppHandle, shutdown_rx: oneshot::Receiver<()>) {
    use tauri::Emitter;

    let app_pilot = Arc::new(app.clone());
    let app_turbo = Arc::new(app.clone());
    let app_cool = Arc::new(app.clone());
    let app_sleep = Arc::new(app.clone());
    let app_shut = Arc::new(app.clone());
    let app_lock = Arc::new(app.clone());

    // ── Serve embedded HTML (public — token validated by JS on first API call) ──
    let index = warp::path::end()
        .and(warp::get())
        .map(|| warp::reply::html(REMOTE_HTML));

    // ── GET /api/state — Unified telemetry (token required) ──
    let api_state = warp::path!("api" / "state")
        .and(warp::get())
        .and(with_token())
        .map(|_| warp::reply::json(&gather_state()));

    // ── GET /api/screenshot — Capture screen (token required) ──
    let api_screenshot = warp::path!("api" / "screenshot")
        .and(warp::get())
        .and(with_token())
        .map(|_| {
            match capture_screenshot_jpeg() {
                Ok(data) => warp::http::Response::builder()
                    .status(200)
                    .header("Content-Type", "image/jpeg")
                    .header("Cache-Control", "no-store")
                    .body(data)
                    .unwrap(),
                Err(_) => warp::http::Response::builder()
                    .status(500)
                    .body(Vec::from(b"Screenshot failed" as &[u8]))
                    .unwrap(),
            }
        });

    // ── POST /api/action/* — All actions (token required) ──
    let action_autopilot = warp::path!("api" / "action" / "autopilot")
        .and(warp::post())
        .and(with_token())
        .map(move |_| {
            app_pilot.emit("remote-action", "autopilot").ok();
            warp::reply::json(&serde_json::json!({"ok": true, "action": "autopilot"}))
        });

    let action_turbo = warp::path!("api" / "action" / "turbo")
        .and(warp::post())
        .and(with_token())
        .map(move |_| {
            app_turbo.emit("remote-action", "turbo").ok();
            warp::reply::json(&serde_json::json!({"ok": true, "action": "turbo"}))
        });

    let action_cool = warp::path!("api" / "action" / "coolboost")
        .and(warp::post())
        .and(with_token())
        .map(move |_| {
            app_cool.emit("remote-action", "coolboost").ok();
            warp::reply::json(&serde_json::json!({"ok": true, "action": "coolboost"}))
        });

    let action_sleep = warp::path!("api" / "action" / "sleep")
        .and(warp::post())
        .and(with_token())
        .map(move |_| {
            app_sleep.emit("remote-action", "sleep").ok();
            let _ = power_sleep();
            warp::reply::json(&serde_json::json!({"ok": true, "action": "sleep"}))
        });

    let action_shutdown = warp::path!("api" / "action" / "shutdown")
        .and(warp::post())
        .and(with_token())
        .map(move |_| {
            app_shut.emit("remote-action", "shutdown").ok();
            let _ = power_shutdown();
            warp::reply::json(&serde_json::json!({"ok": true, "action": "shutdown"}))
        });

    let action_lock = warp::path!("api" / "action" / "lock")
        .and(warp::post())
        .and(with_token())
        .map(move |_| {
            app_lock.emit("remote-action", "lock").ok();
            let _ = power_lock();
            warp::reply::json(&serde_json::json!({"ok": true, "action": "lock"}))
        });

    let cors = warp::cors()
        .allow_any_origin()
        .allow_methods(vec!["GET", "POST"])
        .allow_headers(vec!["Content-Type", "Authorization"]);

    let routes = index
        .or(api_state)
        .or(api_screenshot)
        .or(action_autopilot)
        .or(action_turbo)
        .or(action_cool)
        .or(action_sleep)
        .or(action_shutdown)
        .or(action_lock)
        .with(cors)
        .recover(handle_rejection);

    // Use graceful shutdown so the server can be stopped cleanly
    let (_, server) = warp::serve(routes).bind_with_graceful_shutdown(
        ([0, 0, 0, 0], port),
        async { shutdown_rx.await.ok(); },
    );
    server.await;
}

async fn find_free_port() -> Result<u16, String> {
    // Try ports in range 3500-3599 to be predictable and avoid conflicts
    for port in 3500..3600 {
        if let Ok(listener) = tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            drop(listener);
            return Ok(port);
        }
    }
    // Fallback: OS random port
    let listener = tokio::net::TcpListener::bind("0.0.0.0:0")
        .await
        .map_err(|e| format!("No free port found: {}", e))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    drop(listener);
    Ok(port)
}

// ── Embedded HTML for remote dashboard ───────────────────────────────
const REMOTE_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1,user-scalable=no">
<title>Aura Update — Remote</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
:root{--bg:#0a0a0f;--card:rgba(20,20,30,.85);--border:rgba(88,166,255,.15);--accent:#58a6ff;--accent2:#bc8cff;--success:#3fb950;--warning:#f59e0b;--danger:#f85149;--text:#e0e6ed;--text2:#8b949e;--radius:16px}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',sans-serif;background:var(--bg);color:var(--text);min-height:100dvh;display:flex;flex-direction:column;padding:16px;overflow-x:hidden;-webkit-user-select:none;user-select:none}

/* ── Loading ── */
#loading{position:fixed;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:16px;background:var(--bg);z-index:100;animation:fade-in .4s ease}
.loader-ring{width:64px;height:64px;position:relative}
.loader-ring .ring{position:absolute;inset:0;border:3px solid transparent;border-top-color:var(--accent);border-radius:50%;animation:spin 1.2s linear infinite}
.loader-ring .ring:nth-child(2){inset:8px;border-top-color:var(--accent2);animation-duration:1.8s;animation-direction:reverse}
.loader-ring .ring:nth-child(3){inset:16px;border-top-color:var(--success);animation-duration:2.4s}
.loader-icon{position:absolute;inset:0;display:flex;align-items:center;justify-content:center;font-size:1.3rem}
#loading h2{font-size:1rem;font-weight:600;background:linear-gradient(135deg,var(--accent),var(--accent2));-webkit-background-clip:text;-webkit-text-fill-color:transparent}
.dot-pulse::after{content:'';animation:dots 1.5s steps(4,end) infinite}

/* ── Auth Error ── */
#authError{display:none;position:fixed;inset:0;flex-direction:column;align-items:center;justify-content:center;gap:12px;background:var(--bg);z-index:101}
#authError .lock-icon{font-size:3rem}
#authError h2{color:var(--danger);font-size:1.1rem}
#authError p{color:var(--text2);font-size:.85rem;text-align:center;max-width:260px}

/* ── Dashboard ── */
#dashboard{display:none;flex-direction:column;gap:14px;width:100%;max-width:400px;margin:0 auto;animation:slide-up .5s ease}
.dash-header{text-align:center;margin-bottom:4px}
.dash-header h1{font-size:1.2rem;background:linear-gradient(135deg,var(--accent),var(--accent2));-webkit-background-clip:text;-webkit-text-fill-color:transparent}
.dash-header p{color:var(--text2);font-size:.75rem;margin-top:2px}

/* ── Zone 1: Monitoring ── */
.zone-monitor{display:flex;align-items:center;justify-content:center;gap:16px;padding:16px;background:var(--card);border:1px solid var(--border);border-radius:var(--radius);backdrop-filter:blur(12px)}
.health-ring{position:relative;width:100px;height:100px;flex-shrink:0}
.health-ring svg{width:100%;height:100%;transform:rotate(-90deg)}
.health-ring circle{fill:none;stroke-width:7;stroke-linecap:round}
.health-ring .bg{stroke:rgba(255,255,255,.06)}
.health-ring .fg{stroke:var(--success);transition:stroke-dashoffset .8s ease,stroke .5s}
.health-ring .score{position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;font-size:1.6rem;font-weight:800;line-height:1}
.health-ring .score small{font-size:.6rem;font-weight:400;color:var(--text2);margin-top:2px}
.temp-gauges{display:flex;flex-direction:column;gap:10px;flex:1}
.temp-gauge{display:flex;align-items:center;gap:8px;font-size:.82rem}
.temp-gauge .icon{font-size:1.1rem}
.temp-gauge .label{color:var(--text2);min-width:32px}
.temp-gauge .value{font-weight:700;font-variant-numeric:tabular-nums;min-width:44px}
.temp-gauge .bar{flex:1;height:6px;background:rgba(255,255,255,.06);border-radius:3px;overflow:hidden}
.temp-gauge .bar-fill{height:100%;border-radius:3px;transition:width .6s ease,background .5s}
.temp-hot{color:var(--danger)}
.temp-warm{color:var(--warning)}
.temp-cool{color:var(--success)}
.bar-hot{background:var(--danger)}
.bar-warm{background:var(--warning)}
.bar-cool{background:var(--success)}

/* ── Zone 2: Screenshot ── */
.zone-visual{background:var(--card);border:1px solid var(--border);border-radius:var(--radius);padding:12px;text-align:center;backdrop-filter:blur(12px)}
.zone-visual h3{font-size:.8rem;color:var(--text2);margin-bottom:8px;font-weight:500}
.shot-frame{position:relative;width:100%;aspect-ratio:4/3;background:#000;border-radius:10px;overflow:hidden;cursor:pointer}
.shot-frame img{width:100%;height:100%;object-fit:contain;display:none}
.shot-placeholder{position:absolute;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:6px;color:var(--text2);font-size:.82rem}
.shot-placeholder .cam{font-size:2rem;opacity:.5}
.shot-spinner{display:none;position:absolute;inset:0;align-items:center;justify-content:center;background:rgba(0,0,0,.6)}
.shot-spinner .ring{width:30px;height:30px;border:3px solid transparent;border-top-color:var(--accent);border-radius:50%;animation:spin .8s linear infinite}

/* ── Zone 3: Actions ── */
.zone-actions{display:flex;flex-direction:column;gap:10px}
.action-row-cool{display:flex;align-items:center;gap:10px;padding:14px;background:var(--card);border:1px solid var(--border);border-radius:var(--radius);backdrop-filter:blur(12px)}
.btn-boost{flex:1;padding:12px;border:none;border-radius:12px;background:linear-gradient(135deg,#00c6ff,#0072ff);color:#fff;font-size:.95rem;font-weight:600;cursor:pointer;transition:transform .15s,box-shadow .2s}
.btn-boost:active{transform:scale(.96)}
.btn-boost:disabled{opacity:.5;cursor:not-allowed}
.btn-boost .icon{margin-right:6px}
.boost-timer{font-size:1.4rem;font-weight:800;font-variant-numeric:tabular-nums;color:var(--accent);min-width:40px;text-align:center}
.action-grid{display:grid;grid-template-columns:1fr 1fr;gap:10px}
.btn-action{padding:16px 10px;border:none;border-radius:14px;font-size:.88rem;font-weight:600;cursor:pointer;color:#fff;position:relative;overflow:hidden;transition:transform .15s,opacity .15s,box-shadow .2s}
.btn-action:active{transform:scale(.96);opacity:.85}
.btn-action::after{content:'';position:absolute;inset:0;background:linear-gradient(135deg,rgba(255,255,255,.08),transparent);pointer-events:none}
.btn-action .emoji{font-size:1.3rem;display:block;margin-bottom:4px}
.btn-pilot{background:linear-gradient(135deg,#f59e0b,#ef4444);box-shadow:0 4px 18px rgba(245,158,11,.2)}
.btn-turbo{background:linear-gradient(135deg,#06b6d4,#8b5cf6);box-shadow:0 4px 18px rgba(139,92,246,.2)}
.btn-sleep{background:linear-gradient(135deg,#667eea,#764ba2);box-shadow:0 4px 18px rgba(118,75,162,.2)}
.btn-shut{background:linear-gradient(135deg,#f85149,#da3633);box-shadow:0 4px 18px rgba(248,81,73,.2)}
.btn-lock{background:linear-gradient(135deg,#3fb950,#1a7f37);box-shadow:0 4px 18px rgba(63,185,80,.2)}

/* ── Status Toast ── */
#toast{position:fixed;bottom:20px;left:50%;transform:translateX(-50%) translateY(80px);padding:10px 20px;border-radius:12px;font-size:.82rem;font-weight:600;background:var(--card);border:1px solid var(--border);backdrop-filter:blur(12px);transition:transform .3s ease,opacity .3s;opacity:0;z-index:50}
#toast.show{transform:translateX(-50%) translateY(0);opacity:1}
#toast.ok{border-color:var(--success);color:var(--success)}
#toast.err{border-color:var(--danger);color:var(--danger)}

/* ── Confirm Modal ── */
#confirmOverlay{display:none;position:fixed;inset:0;background:rgba(0,0,0,.7);z-index:60;align-items:center;justify-content:center}
.confirm-box{background:var(--card);border:1px solid var(--border);border-radius:var(--radius);padding:24px;text-align:center;max-width:300px;backdrop-filter:blur(16px)}
.confirm-box h3{margin-bottom:10px;font-size:1rem}
.confirm-box p{color:var(--text2);font-size:.85rem;margin-bottom:16px}
.confirm-btns{display:flex;gap:10px;justify-content:center}
.confirm-btns button{padding:10px 22px;border:none;border-radius:10px;font-weight:600;cursor:pointer;font-size:.85rem}
.btn-confirm-yes{background:var(--danger);color:#fff}
.btn-confirm-no{background:rgba(255,255,255,.1);color:var(--text)}

.footer{text-align:center;font-size:.65rem;color:#444;margin-top:8px}
.footer .lock{color:var(--success);margin-right:4px}

@keyframes spin{to{transform:rotate(360deg)}}
@keyframes fade-in{from{opacity:0;transform:scale(.95)}to{opacity:1;transform:scale(1)}}
@keyframes slide-up{from{opacity:0;transform:translateY(20px)}to{opacity:1;transform:translateY(0)}}
@keyframes dots{0%{content:''}25%{content:'.'}50%{content:'..'}75%{content:'...'}}
</style>
</head>
<body>

<!-- Loading -->
<div id="loading">
  <div class="loader-ring">
    <div class="ring"></div><div class="ring"></div><div class="ring"></div>
    <div class="loader-icon">🩺</div>
  </div>
  <h2>Aura Update Remote</h2>
  <p class="i18n" data-fr="Connexion" data-en="Connecting"><span class="dot-pulse"></span></p>
</div>

<!-- Auth Error -->
<div id="authError">
  <span class="lock-icon">🔒</span>
  <h2 class="i18n" data-fr="Accès refusé" data-en="Access Denied"></h2>
  <p class="i18n" data-fr="Token de sécurité invalide ou manquant. Scannez le QR code depuis l'application Aura Update." data-en="Invalid or missing security token. Please scan the QR code from the Aura Update app."></p>
</div>

<!-- Dashboard -->
<div id="dashboard">
  <div class="dash-header">
    <h1>🩺 Aura Update</h1>
    <p class="i18n" data-fr="Centre de Santé à Distance" data-en="Remote Health Center"></p>
  </div>

  <!-- Zone 1: Monitoring -->
  <div class="zone-monitor">
    <div class="health-ring">
      <svg viewBox="0 0 100 100">
        <circle class="bg" cx="50" cy="50" r="42"/>
        <circle class="fg" id="ringFg" cx="50" cy="50" r="42" stroke-dasharray="264" stroke-dashoffset="264"/>
      </svg>
      <div class="score" id="healthScore">—<small class="i18n" data-fr="Santé" data-en="Health"></small></div>
    </div>
    <div class="temp-gauges">
      <div class="temp-gauge">
        <span class="icon">🌡️</span>
        <span class="label">CPU</span>
        <span class="value" id="cpuTemp">—</span>
        <div class="bar"><div class="bar-fill" id="cpuBar" style="width:0%"></div></div>
      </div>
      <div class="temp-gauge">
        <span class="icon">🎮</span>
        <span class="label">GPU</span>
        <span class="value" id="gpuTemp">—</span>
        <div class="bar"><div class="bar-fill" id="gpuBar" style="width:0%"></div></div>
      </div>
      <div class="temp-gauge">
        <span class="icon" id="battIcon">🔋</span>
        <span class="label">BAT</span>
        <span class="value" id="battPct">—</span>
        <div class="bar"><div class="bar-fill bar-cool" id="battBar" style="width:0%"></div></div>
      </div>
    </div>
  </div>

  <!-- Zone 2: Screenshot -->
  <div class="zone-visual">
    <h3 class="i18n" data-fr="📸 Capture rapide" data-en="📸 Quick Screenshot"></h3>
    <div class="shot-frame" id="shotFrame">
      <div class="shot-placeholder" id="shotPlaceholder">
        <span class="cam">📷</span>
        <span class="i18n" data-fr="Appuyez pour capturer" data-en="Tap to capture"></span>
      </div>
      <div class="shot-spinner" id="shotSpinner"><div class="ring"></div></div>
      <img id="shotImg" alt="screenshot"/>
    </div>
  </div>

  <!-- Zone 3: Actions -->
  <div class="zone-actions">
    <div class="action-row-cool">
      <button class="btn-boost" id="btnBoost"><span class="icon">❄️</span> Cool Boost</button>
      <span class="boost-timer" id="boostTimer"></span>
    </div>
    <div class="action-grid">
      <button class="btn-action btn-pilot" id="btnPilot"><span class="emoji">🚀</span>Auto-Pilot</button>
      <button class="btn-action btn-turbo" id="btnTurbo"><span class="emoji">⚡</span>Turbo</button>
      <button class="btn-action btn-sleep" id="btnSleep"><span class="emoji">🌙</span><span class="i18n" data-fr="Veille" data-en="Sleep"></span></button>
      <button class="btn-action btn-shut" id="btnShut"><span class="emoji">🔌</span><span class="i18n" data-fr="Éteindre" data-en="Shutdown"></span></button>
      <button class="btn-action btn-lock" id="btnLock"><span class="emoji">🔒</span><span class="i18n" data-fr="Verrouiller" data-en="Lock"></span></button>
    </div>
  </div>

  <div class="footer"><span class="lock">🔒</span><span class="i18n" data-fr="Session sécurisée par token" data-en="Session secured by token"></span></div>
</div>

<!-- Confirm Modal -->
<div id="confirmOverlay">
  <div class="confirm-box">
    <h3 id="confirmTitle"></h3>
    <p id="confirmDesc"></p>
    <div class="confirm-btns">
      <button class="btn-confirm-no" id="btnConfirmNo"><span class="i18n" data-fr="Annuler" data-en="Cancel"></span></button>
      <button class="btn-confirm-yes" id="btnConfirmYes"><span class="i18n" data-fr="Confirmer" data-en="Confirm"></span></button>
    </div>
  </div>
</div>

<!-- Toast -->
<div id="toast"></div>

<script>
const TOKEN = new URLSearchParams(location.search).get('t') || '';
let refreshInterval = null;
let boostInterval = null;
let boostActive = false;
let boostCooldown = false;
let currentLang = 'fr';

// ── i18n ──
function applyLang(lang) {
  currentLang = lang;
  document.querySelectorAll('.i18n').forEach(el => {
    const text = el.getAttribute('data-' + lang) || el.getAttribute('data-en');
    if (text) el.textContent = text;
  });
}

// ── API helpers ──
function apiUrl(path) {
  return path + (path.includes('?') ? '&' : '?') + 't=' + TOKEN;
}

async function apiGet(path) {
  const r = await fetch(apiUrl(path));
  if (r.status === 403) throw new Error('unauthorized');
  return r;
}

async function apiPost(path) {
  const r = await fetch(apiUrl(path), { method: 'POST' });
  if (r.status === 403) throw new Error('unauthorized');
  return r.json();
}

// ── Toast ──
function toast(msg, type) {
  const el = document.getElementById('toast');
  el.textContent = msg;
  el.className = 'show ' + (type || '');
  setTimeout(() => el.className = '', 3000);
}

// ── Init ──
async function init() {
  try {
    const r = await apiGet('/api/state');
    if (!r.ok) throw new Error('auth');
    // Success — show dashboard
    const data = await r.json();
    applyLang(data.lang || 'fr');
    document.getElementById('loading').style.display = 'none';
    document.getElementById('dashboard').style.display = 'flex';
    updateState(data);
    // Start polling every 2s
    refreshInterval = setInterval(pollState, 2000);
  } catch (e) {
    applyLang('fr');
    document.getElementById('loading').style.display = 'none';
    document.getElementById('authError').style.display = 'flex';
  }
}
setTimeout(init, 800);

// ── Polling ──
async function pollState() {
  try {
    const r = await apiGet('/api/state');
    if (r.ok) updateState(await r.json());
  } catch (_) {}
}

function updateState(s) {
  // i18n live
  if (s.lang && s.lang !== currentLang) {
    applyLang(s.lang);
  }

  // Health ring
  const score = s.health_score || 0;
  const healthLabel = currentLang === 'fr' ? 'Santé' : 'Health';
  document.getElementById('healthScore').innerHTML = score + '<small>' + healthLabel + '</small>';
  const circumference = 264;
  const offset = circumference - (circumference * score / 100);
  const fg = document.getElementById('ringFg');
  fg.style.strokeDashoffset = offset;
  fg.style.stroke = score >= 70 ? 'var(--success)' : score >= 40 ? 'var(--warning)' : 'var(--danger)';

  // CPU
  if (s.cpu_temp != null) {
    const ct = Math.round(s.cpu_temp);
    const cls = ct >= 85 ? 'hot' : ct >= 65 ? 'warm' : 'cool';
    document.getElementById('cpuTemp').textContent = ct + '°C';
    document.getElementById('cpuTemp').className = 'value temp-' + cls;
    const pct = Math.min(ct, 100);
    document.getElementById('cpuBar').style.width = pct + '%';
    document.getElementById('cpuBar').className = 'bar-fill bar-' + cls;
  }

  // GPU
  if (s.gpu_temp != null) {
    const gt = Math.round(s.gpu_temp);
    const cls = gt >= 85 ? 'hot' : gt >= 65 ? 'warm' : 'cool';
    document.getElementById('gpuTemp').textContent = gt + '°C';
    document.getElementById('gpuTemp').className = 'value temp-' + cls;
    const pct = Math.min(gt, 100);
    document.getElementById('gpuBar').style.width = pct + '%';
    document.getElementById('gpuBar').className = 'bar-fill bar-' + cls;
  }

  // Battery
  if (s.battery_percent != null) {
    document.getElementById('battPct').textContent = s.battery_percent + '%';
    document.getElementById('battBar').style.width = s.battery_percent + '%';
    document.getElementById('battIcon').textContent = s.battery_charging ? '⚡' : s.battery_percent <= 20 ? '🪫' : '🔋';
  }
}

// ── Screenshot ──
document.getElementById('shotFrame').addEventListener('click', async () => {
  const placeholder = document.getElementById('shotPlaceholder');
  const spinner = document.getElementById('shotSpinner');
  const img = document.getElementById('shotImg');
  placeholder.style.display = 'none';
  spinner.style.display = 'flex';
  img.style.display = 'none';
  try {
    const r = await apiGet('/api/screenshot');
    if (!r.ok) throw new Error('fail');
    const blob = await r.blob();
    img.src = URL.createObjectURL(blob);
    img.style.display = 'block';
  } catch (_) {
    placeholder.style.display = 'flex';
    toast(currentLang === 'fr' ? 'Capture échouée' : 'Screenshot failed', 'err');
  }
  spinner.style.display = 'none';
});

// ── Cool Boost ──
document.getElementById('btnBoost').addEventListener('click', async () => {
  if (boostActive || boostCooldown) return;
  boostActive = true;
  const btn = document.getElementById('btnBoost');
  const timer = document.getElementById('boostTimer');
  btn.disabled = true;
  try {
    await apiPost('/api/action/coolboost');
  } catch (_) { toast(currentLang === 'fr' ? 'Erreur Cool Boost' : 'Cool Boost error', 'err'); boostActive = false; btn.disabled = false; return; }
  toast(currentLang === 'fr' ? '❄️ Cool Boost actif' : '❄️ Cool Boost active', 'ok');
  pollState();
  let remaining = 30;
  timer.textContent = remaining + 's';
  boostInterval = setInterval(() => {
    remaining--;
    timer.textContent = remaining + 's';
    if (remaining <= 0) {
      clearInterval(boostInterval);
      boostActive = false;
      startCooldown();
    }
  }, 1000);
});

function startCooldown() {
  boostCooldown = true;
  const btn = document.getElementById('btnBoost');
  const timer = document.getElementById('boostTimer');
  btn.style.background = 'linear-gradient(135deg,#667eea,#764ba2)';
  btn.style.opacity = '0.6';
  let cd = 60;
  timer.textContent = cd + 's';
  timer.style.color = 'var(--warning)';
  const int = setInterval(() => {
    cd--;
    timer.textContent = cd + 's';
    if (cd <= 0) {
      clearInterval(int);
      boostCooldown = false;
      btn.disabled = false;
      btn.style.background = '';
      btn.style.opacity = '';
      timer.textContent = '';
      timer.style.color = '';
    }
  }, 1000);
}

// ── Actions ──
document.getElementById('btnPilot').addEventListener('click', async () => {
  try { await apiPost('/api/action/autopilot'); toast(currentLang === 'fr' ? '🚀 Auto-Pilot lancé' : '🚀 Auto-Pilot started', 'ok'); pollState(); } catch(_) { toast(currentLang === 'fr' ? 'Erreur' : 'Error', 'err'); }
});
document.getElementById('btnTurbo').addEventListener('click', async () => {
  try { await apiPost('/api/action/turbo'); toast(currentLang === 'fr' ? '⚡ Turbo basculé' : '⚡ Turbo toggled', 'ok'); pollState(); } catch(_) { toast(currentLang === 'fr' ? 'Erreur' : 'Error', 'err'); }
});

// ── Power with confirmation ──
let confirmCallback = null;
function showConfirm(title, desc, cb) {
  document.getElementById('confirmTitle').textContent = title;
  document.getElementById('confirmDesc').textContent = desc;
  document.getElementById('confirmOverlay').style.display = 'flex';
  confirmCallback = cb;
}
document.getElementById('btnConfirmNo').addEventListener('click', () => {
  document.getElementById('confirmOverlay').style.display = 'none';
  confirmCallback = null;
});
document.getElementById('btnConfirmYes').addEventListener('click', async () => {
  document.getElementById('confirmOverlay').style.display = 'none';
  if (confirmCallback) await confirmCallback();
  confirmCallback = null;
});

document.getElementById('btnSleep').addEventListener('click', () => {
  showConfirm(
    '🌙 ' + (currentLang === 'fr' ? 'Veille' : 'Sleep'),
    currentLang === 'fr' ? 'Mettre le PC en veille maintenant ?' : 'Put the PC to sleep now?',
    async () => {
      try { await apiPost('/api/action/sleep'); toast(currentLang === 'fr' ? '🌙 Mise en veille…' : '🌙 PC going to sleep', 'ok'); } catch(_) { toast(currentLang === 'fr' ? 'Erreur' : 'Error', 'err'); }
    }
  );
});
document.getElementById('btnShut').addEventListener('click', () => {
  showConfirm(
    '🔌 ' + (currentLang === 'fr' ? 'Éteindre' : 'Shutdown'),
    currentLang === 'fr' ? 'Éteindre le PC ? (délai 60s)' : 'Shut down the PC? (60s delay)',
    async () => {
      try { await apiPost('/api/action/shutdown'); toast(currentLang === 'fr' ? '🔌 Extinction dans 60s' : '🔌 Shutdown in 60s', 'ok'); } catch(_) { toast(currentLang === 'fr' ? 'Erreur' : 'Error', 'err'); }
    }
  );
});
document.getElementById('btnLock').addEventListener('click', async () => {
  try { await apiPost('/api/action/lock'); toast(currentLang === 'fr' ? '🔒 PC verrouillé' : '🔒 PC locked', 'ok'); } catch(_) { toast(currentLang === 'fr' ? 'Erreur' : 'Error', 'err'); }
});
</script>
</body>
</html>
"##;
