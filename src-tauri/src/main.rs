#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use deva_light::aggregator::StateAggregator;
use deva_light::app_lock::AppLock;
use deva_light::config::load_app_config;
use deva_light::http_server::{existing_instance_is_healthy, start_http_server};
use deva_light::logging::{log_error, log_info, log_warn};
use deva_light::types::Status;
use std::sync::Arc;
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
    let server_aggregator = Arc::clone(&aggregator);

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(Arc::clone(&aggregator))
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
            ipc::open_settings,
            ipc::resize_main_window,
            ipc::check_hooks,
            ipc::install_hooks_command,
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

            // Apply always_on_top setting
            let _ = window.set_always_on_top(app_config.always_on_top);
            log_info(
                "app",
                format!("always on top set to {}", app_config.always_on_top),
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
            let config_for_notify = app_config.clone();

            aggregator.set_on_change(move || {
                let lights = emit_aggregator.get_lights();
                let _ = emit_window.emit("state-changed", &lights);

                // Send notifications for status changes
                if config_for_notify.notifications_enabled {
                    for light in &lights {
                        let should_notify = match light.status {
                            Status::Waiting => config_for_notify.notify_on_waiting,
                            Status::Done => config_for_notify.notify_on_done,
                            _ => false,
                        };

                        if should_notify {
                            let title = format!("Deva Light - {}", light.project_label);
                            let body = match light.status {
                                Status::Waiting => "AI 需要您的关注".to_string(),
                                Status::Done => "任务已完成".to_string(),
                                _ => String::new(),
                            };
                            let _ = app_handle.emit("notify-status", (title, body));
                        }
                    }
                }
            });

            start_http_server(Arc::clone(&server_aggregator), &app_config)
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            deva_light::codex_watcher::start_codex_watcher(Arc::clone(&aggregator))?;
            deva_light::claude_watcher::start_claude_watcher(Arc::clone(&aggregator));
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
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn setup_tray(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &settings, &quit])?;

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
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .build(&app_handle)?;

    Ok(())
}
