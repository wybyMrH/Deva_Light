#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use deva_light::aggregator::StateAggregator;
use deva_light::app_lock::AppLock;
use deva_light::config::load_app_config;
use deva_light::http_server::{existing_instance_is_healthy, HttpServerController};
use deva_light::logging::{log_error, log_info, log_warn};
use deva_light::types::Status;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};

mod ipc;

fn main() {
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
            ipc::get_remote_setup_info,
            ipc::get_app_version,
            ipc::check_for_update,
            ipc::download_and_install_update,
            ipc::test_ssh_connection,
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

            let _ = window.set_position(tauri::Position::Physical(
                tauri::PhysicalPosition::new(app_config.window_x, app_config.window_y),
            ));

            // Pin only while active tasks exist; preference is applied in on_change.
            let _ = window.set_always_on_top(false);
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

            let emit_aggregator = Arc::clone(&aggregator);
            let emit_window = window.clone();
            let app_handle = app.handle().clone();
            let last_light_statuses = Mutex::new(HashMap::<String, Status>::new());

            aggregator.set_on_change(move || {
                let lights = emit_aggregator.get_lights();
                let _ = emit_window.emit("state-changed", &lights);

                let config = load_app_config();
                let pin_window = config.always_on_top && emit_aggregator.has_active_lights();
                let _ = emit_window.set_always_on_top(pin_window);

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
                        Status::Done => config.notify_on_done,
                        _ => false,
                    };

                    if !should_notify {
                        continue;
                    }

                    let title = format!("Deva Light - {}", light.project_label);
                    let body = match light.status {
                        Status::Waiting => "AI 需要您的关注".to_string(),
                        Status::Done => "任务已完成".to_string(),
                        _ => String::new(),
                    };
                    let _ = app_handle.emit("notify-status", (title, body));
                }

                last_statuses.retain(|project_id, _| {
                    lights.iter().any(|light| &light.project_id == project_id)
                });
            });

            http_server
                .start(Arc::clone(&aggregator), &app_config)
                .map_err(|error| std::io::Error::other(error))?;
            deva_light::codex_watcher::start_codex_watcher(Arc::clone(&aggregator))?;
            deva_light::claude_watcher::start_claude_watcher(Arc::clone(&aggregator));
            deva_light::cursor_watcher::start_cursor_watcher(Arc::clone(&aggregator));
            log_info("app", "watchers started");

            window.emit("state-changed", aggregator.get_lights())?;
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
            } else {
                log_warn(
                    "app",
                    "resource directory unavailable; skipped hook helper install",
                );
            }

            log_info("app", "startup complete");
            deva_light::updater::spawn_startup_update_check(app.handle());
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
