mod commands;

use commands::config::{get_portable_dir, load_config, AppState};
use std::sync::Mutex;
use tauri::Manager;
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};
use tauri::menu::{Menu, MenuItem};

/// Entry point for Aura Update desktop application (Tauri 2).
pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    let is_admin_relaunch = args.contains(&"--admin-relaunch".to_string());

    if is_admin_relaunch {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--auto-start"])));

    // On n'active l'instance unique QUE si ce n'est pas un redémarrage Admin
    if !is_admin_relaunch {
        builder = builder.plugin(
            tauri_plugin_single_instance::init(|app, _args, _cwd| {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            })
        );
    }

    builder.setup(|app| {
            let data_dir = get_portable_dir();
            let config = load_config(&data_dir);

            // Initialize rotating log system (3 files max)
            commands::logging::init_logging(&data_dir);

            app.manage(AppState {
                data_dir,
                config: Mutex::new(config),
                remote_port: Mutex::new(None),
            });

            // Set window icon from embedded icon.png
            if let Some(window) = app.get_webview_window("main") {
                let png_data = include_bytes!("../../frontend/icons/icon.png");
                if let Ok(img) = image::load_from_memory(png_data) {
                    let rgba = img.to_rgba8();
                    let (w, h) = rgba.dimensions();
                    let icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
                    let _ = window.set_icon(icon);
                }
            }

            // System tray icon — left-click show/focus, right-click context menu
            let png_data = include_bytes!("../../frontend/icons/icon.png");
            if let Ok(img) = image::load_from_memory(png_data) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let tray_icon = tauri::image::Image::new_owned(rgba.into_raw(), w, h);
                let app_handle = app.handle().clone();

                // Build right-click context menu
                let menu_show = MenuItem::with_id(app, "tray_show", "Ouvrir Aura Update", true, None::<&str>)?;
                let menu_autopilot = MenuItem::with_id(app, "tray_autopilot", "🚀 Auto-Pilote", true, None::<&str>)?;
                let menu_settings = MenuItem::with_id(app, "tray_settings", "⚙️ Paramètres", true, None::<&str>)?;
                let menu_website = MenuItem::with_id(app, "tray_website", "🌐 Site Web", true, None::<&str>)?;
                let menu_quit = MenuItem::with_id(app, "tray_quit", "Quitter", true, None::<&str>)?;

                let menu = Menu::with_items(app, &[
                    &menu_show,
                    &menu_autopilot,
                    &menu_settings,
                    &menu_website,
                    &menu_quit,
                ])?;

                let _ = TrayIconBuilder::new()
                    .icon(tray_icon)
                    .tooltip("Aura Update")
                    .menu(&menu)
                    .on_menu_event(move |app, event| {
                        match event.id().as_ref() {
                            "tray_show" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.unminimize();
                                    let _ = w.set_focus();
                                }
                            }
                            "tray_autopilot" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.unminimize();
                                    let _ = w.set_focus();
                                    let _ = w.eval("runAutoPilot()");
                                }
                            }
                            "tray_settings" => {
                                if let Some(w) = app.get_webview_window("main") {
                                    let _ = w.show();
                                    let _ = w.unminimize();
                                    let _ = w.set_focus();
                                    let _ = w.eval("openSettings()");
                                }
                            }
                            "tray_website" => {
                                let _ = open::that("https://www.auraneo.fr");
                            }
                            "tray_quit" => {
                                app.exit(0);
                            }
                            _ => {}
                        }
                    })
                    .on_tray_icon_event(move |_tray, event| {
                        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                            if let Some(w) = app_handle.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                    })
                    .build(app);
            }

            // Apply startup mode
            let startup_mode = {
                let state: tauri::State<'_, AppState> = app.state();
                let cfg = state.config.lock().unwrap();
                cfg.startup_mode.clone()
            };
            if let Some(win) = app.get_webview_window("main") {
                match startup_mode.as_str() {
                    "minimized" => { let _ = win.minimize(); }
                    "tray" => { let _ = win.hide(); }
                    _ => { /* visible — default, do nothing */ }
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state: tauri::State<'_, AppState> = window.app_handle().state();
                let close_to_tray = state.config.lock().unwrap().close_to_tray;
                if close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // ── Config & Platform ────────────────────────────
            commands::config::get_config,
            commands::config::set_config_value,
            commands::config::get_translations,
            commands::config::get_platform,
            commands::config::get_app_version,
            commands::config::get_predicted_cleanup_gain,
            commands::config::open_url,
            commands::config::get_disk_free_space,
            // ── Admin ────────────────────────────────────────
            commands::admin::is_admin,
            commands::admin::elevate,
            // ── Updates ──────────────────────────────────────
            commands::updates::check_updates,
            commands::updates::install_update,
            // ── Cleanup ──────────────────────────────────────
            commands::cleanup::scan_cleanup,
            commands::cleanup::run_cleanup,
            commands::cleanup::scan_os_residues,
            commands::cleanup::clean_os_residues,
            commands::cleanup::scan_browser_caches,
            commands::cleanup::scan_browser_granular,
            commands::cleanup::detect_installed_browsers,
            commands::cleanup::kill_browser_processes,
            commands::cleanup::check_temp_size,
            commands::cleanup::list_bloatwares,
            commands::cleanup::purge_bloatwares,
            commands::cleanup::disable_telemetry,
            commands::cleanup::disable_telemetry_granular,
            // ── Startup ──────────────────────────────────────
            commands::startup::get_startup_items,
            commands::startup::toggle_startup_item,
            // ── Health & Vitals ──────────────────────────────
            commands::health::get_health_score,
            commands::health::run_autopilot,
            commands::health::get_system_vitals,
            commands::health::get_vitals,
            commands::health::get_system_info,
            commands::health::get_system_specs,
            // ── Cooling ──────────────────────────────────────
            commands::cooling::set_fan_boost,
            // ── Snapshot ─────────────────────────────────────
            commands::snapshot::create_snapshot,
            commands::snapshot::has_snapshot_support,
            commands::snapshot::list_snapshots,
            commands::snapshot::get_default_backup_dir,
            commands::snapshot::create_local_backup,
            commands::snapshot::list_local_backups,
            // ── Processes & Turbo ────────────────────────────
            commands::processes::get_heavy_processes,
            commands::processes::kill_process,
            commands::processes::toggle_game_mode,
            // ── AI ───────────────────────────────────────────
            commands::ai::ai_is_available,
            commands::ai::configure_ai,
            commands::ai::ai_analyze,
            // ── Scheduler ────────────────────────────────────
            commands::scheduler::get_schedule,
            commands::scheduler::set_schedule,
            commands::scheduler::set_auto_clean_schedule,
            commands::scheduler::get_auto_clean_schedule,
            // ── Remote Dashboard ─────────────────────────────
            commands::remote::start_remote,
            commands::remote::stop_remote,
            commands::remote::get_remote_status,            // ── Crash Report ─────────────────────────────────────
            commands::logging::check_pending_crash,
            commands::logging::send_crash_report,
            commands::logging::clear_crash_report,        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
