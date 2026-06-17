#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use deva_light::aggregator::StateAggregator;
use deva_light::app_lock::AppLock;
use deva_light::config::{apply_configured_proxy_to_env, load_app_config};
use deva_light::http_server::{existing_instance_is_healthy, HttpServerController};
use deva_light::logging::{log_error, log_info, log_warn};
use deva_light::types::Status;
use deva_light::window_behavior::{apply_main_window_pin, configure_main_window_workspace};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;

mod ipc;

fn main() {
    apply_configured_proxy_to_env();
    log_info("app", "starting Deva Light");

    let app_lock = match AppLock::acquire() {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            log_info("app", "another app instance already owns the lock");
            return;
        }
        Err(error) => {
            log_error("app", format!("failed to acquire app lock: {error}"));
            eprintln!("failed to acquire app lock: {error}");
            return;
        }
    };

    let app_config = load_app_config();
    let aggregator = Arc::new(StateAggregator::new());
    let http_server = Arc::new(HttpServerController::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(Arc::clone(&aggregator))
        .manage(Arc::clone(&http_server))
        .manage(app_lock)
        .invoke_handler(tauri::generate_handler![
            ipc::confirm_light,
            ipc::confirm_session,
            ipc::remove_light,
            ipc::get_lights,
            ipc::refresh_lights,
            ipc::get_diagnostics,
            ipc::open_project,
            ipc::open_session_logs,
            ipc::open_app_log,
            ipc::get_app_config,
            ipc::save_app_config_command,
            ipc::set_always_on_top,
            ipc::prepare_uninstall,
            ipc::copy_path,
            ipc::pause_monitoring,
            ipc::resume_monitoring,
            ipc::get_monitoring_paused,
            ipc::get_ui_config,
            ipc::set_display_mode,
            ipc::set_done_light_auto_dismiss,
            ipc::set_auto_update_enabled,
            ipc::get_remote_setup_info,
            ipc::get_app_version,
            ipc::check_for_update,
            ipc::download_and_install_update,
            ipc::test_ssh_connection,
            ipc::pick_ssh_private_key,
            ipc::get_ssh_setup_guide,
            ipc::discover_ssh_key_candidates,
            ipc::dismiss_ssh_discovery,
            ipc::read_ssh_public_key,
            ipc::persist_window_position,
            ipc::open_settings,
            ipc::hide_settings,
            ipc::open_config_dir,
            ipc::open_path_in_explorer,
            ipc::resize_main_window,
            ipc::check_hooks,
            ipc::check_cursor_hooks,
            ipc::install_hooks_command,
            ipc::install_cursor_hooks_command,
            ipc::remove_hooks_command,
            ipc::preview_hook_config_command,
            ipc::get_news_sources,
            ipc::fetch_news,
            ipc::open_in_browser,
            ipc::open_news,
            ipc::quit_app
        ])
        .setup(move |app| {
            if existing_instance_is_healthy() {
                log_info(
                    "app",
                    "existing healthy instance detected; exiting current launch",
                );
                app.handle().exit(0);
                return Ok(());
            }

            // Setup system tray
            setup_tray(app.handle())?;
            log_info("app", "tray initialized");

            let window = app
                .get_webview_window("main")
                .expect("main window should exist");

            #[cfg(target_os = "macos")]
            {
                use tauri::window::Color;
                let _ = window.set_background_color(Some(Color(0, 0, 0, 0)));
            }

            configure_main_window_workspace(&window);

            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
                app_config.window_x,
                app_config.window_y,
            )));

            // Pin only while active tasks exist; preference is applied in on_change.
            let _ = apply_main_window_pin(&window, false);
            log_info(
                "app",
                format!(
                    "always on top preference saved as {}",
                    app_config.always_on_top
                ),
            );

            if let Some(settings_window) = app.get_webview_window("settings") {
                let window_to_hide = settings_window.clone();
                settings_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_to_hide.hide();
                    }
                });
            }

            if let Some(news_window) = app.get_webview_window("news") {
                let window_to_hide = news_window.clone();
                news_window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_to_hide.hide();
                    }
                });
            }

            let emit_aggregator = Arc::clone(&aggregator);
            let emit_window = window.clone();
            #[cfg(target_os = "macos")]
            let app_handle_for_ui = app.handle().clone();
            let app_handle_for_notify = app.handle().clone();
            let last_light_statuses = Mutex::new(HashMap::<String, Status>::new());

            aggregator.set_on_change(move || {
                let lights = emit_aggregator.get_lights();
                let _ = emit_window.emit("state-changed", &lights);

                let config = load_app_config();
                let pin_window = config.always_on_top && emit_aggregator.has_active_lights();

                // macOS requires UI operations (like set_always_on_top) to run on the main thread.
                // Use run_on_main_thread to dispatch the window pin operation safely.
                #[cfg(target_os = "macos")]
                {
                    let window_for_pin = emit_window.clone();
                    let app_for_pin = app_handle_for_ui.clone();
                    let _ = app_for_pin.run_on_main_thread(move || {
                        let _ = apply_main_window_pin(&window_for_pin, pin_window);
                    });
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let _ = apply_main_window_pin(&emit_window, pin_window);
                }

                if !config.notifications_enabled {
                    return;
                }

                let mut last_statuses = last_light_statuses
                    .lock()
                    .expect("notification status lock poisoned");

                for light in &lights {
                    let previous = last_statuses.insert(light.project_id.clone(), light.status);
                    if previous == Some(light.status) {
                        continue;
                    }

                    let should_notify = match light.status {
                        Status::Waiting => config.notify_on_waiting,
                        Status::Error => config.notify_on_waiting,
                        Status::Done => config.notify_on_done,
                        _ => false,
                    };

                    if !should_notify {
                        continue;
                    }

                    let title = format!("Deva Light - {}", light.project_label);
                    let body = match light.status {
                        Status::Waiting => "AI 需要您的关注".to_string(),
                        Status::Error => "AI 任务出现错误".to_string(),
                        Status::Done => "任务已完成".to_string(),
                        _ => String::new(),
                    };

                    // Notifications on macOS also require main thread dispatch.
                    #[cfg(target_os = "macos")]
                    {
                        let app_for_notify = app_handle_for_notify.clone();
                        let title_for_notify = title;
                        let body_for_notify = body;
                        let app_clone = app_for_notify.clone();
                        let _ = app_for_notify.run_on_main_thread(move || {
                            if let Err(error) = app_clone
                                .notification()
                                .builder()
                                .title(title_for_notify)
                                .body(body_for_notify)
                                .show()
                            {
                                log_warn(
                                    "notification",
                                    format!("failed to show notification: {error}"),
                                );
                            }
                        });
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        if let Err(error) = app_handle_for_notify
                            .notification()
                            .builder()
                            .title(title)
                            .body(body)
                            .show()
                        {
                            log_warn(
                                "notification",
                                format!("failed to show notification: {error}"),
                            );
                        }
                    }
                }

                last_statuses.retain(|project_id, _| {
                    lights.iter().any(|light| &light.project_id == project_id)
                });
            });

            http_server
                .start(Arc::clone(&aggregator), &app_config)
                .map_err(std::io::Error::other)?;
            deva_light::codex_watcher::start_codex_watcher(Arc::clone(&aggregator))?;
            deva_light::claude_watcher::start_claude_watcher(Arc::clone(&aggregator));
            deva_light::cursor_watcher::start_cursor_watcher(Arc::clone(&aggregator));
            start_done_light_cleanup(Arc::clone(&aggregator));
            log_info("app", "watchers started");

            window.emit("state-changed", aggregator.get_lights())?;
            let _ = window.emit("config-changed", ipc::get_ui_config());
            log_info("app", "initial state emitted");

            if let Ok(resource_dir) = app.path().resource_dir() {
                match deva_light::hook_installer::install_hook_binary_from_resource(&resource_dir) {
                    Ok(true) => log_info("app", "installed bundled hook helper from resources"),
                    Ok(false) => log_info("app", "bundled hook helper already current"),
                    Err(error) => log_warn(
                        "app",
                        format!("failed to install bundled hook helper: {error}"),
                    ),
                }

                // Ensure Cursor hooks exist (idempotent; no-op if Cursor isn't installed).
                match deva_light::hook_installer::install_cursor_hooks_with_resource_dir(Some(
                    &resource_dir,
                )) {
                    Ok(()) => log_info("app", "Cursor hooks ensured"),
                    Err(error) => log_warn("app", format!("failed to install Cursor hooks: {error}")),
                }
            } else {
                log_warn(
                    "app",
                    "resource directory unavailable; skipped hook helper install",
                );

                match deva_light::hook_installer::install_cursor_hooks() {
                    Ok(()) => log_info("app", "Cursor hooks ensured"),
                    Err(error) => log_warn("app", format!("failed to install Cursor hooks: {error}")),
                }
            }

            // Refresh WSL hooks on startup to ensure the embedded HTTP port matches
            // the current runtime.json (the port may change if http_port is None or
            // the previous port was occupied).
            #[cfg(target_os = "windows")]
            {
                match deva_light::hook_installer::refresh_wsl_hooks() {
                    Ok(()) => log_info("app", "WSL hooks refreshed"),
                    Err(error) => log_warn("app", format!("failed to refresh WSL hooks: {error}")),
                }
            }

            if app_config.http_bind == "0.0.0.0" {
                // Warm the LAN address cache in the background so opening the
                // remote panel later never blocks on a cold PowerShell probe.
                thread::spawn(|| {
                    let _ = deva_light::remote::detect_local_addresses();
                });
            }
            log_info("app", "startup complete");
            deva_light::updater::spawn_auto_update_service(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let update = MenuItem::with_id(app, "update", "检查更新", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &settings, &update, &quit])?;

    let app_handle = app.clone();
    TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip("Deva Light")
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    configure_main_window_workspace(&win);
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "settings" => {
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "update" => {
                let _ = ipc::open_settings(app.clone(), Some("about".to_string()));
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(&app_handle)?;

    Ok(())
}

fn start_done_light_cleanup(aggregator: Arc<StateAggregator>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(1));
        aggregator.prune_expired_done_lights(ipc::done_light_retention());
    });
}
