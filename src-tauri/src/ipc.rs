use deva_light::aggregator::StateAggregator;
use deva_light::codex_paths::{codex_session_root_summary, format_paths};
use deva_light::config::{
    ensure_http_token, get_config_dir, get_config_path, get_lock_path, get_log_path,
    get_runtime_path, load_app_config, load_runtime_config, save_app_config, DisplayMode,
};
use deva_light::monitoring::{is_monitoring_paused, set_monitoring_paused};
use deva_light::remote::{build_remote_setup_info, RemoteSetupInfo};
use deva_light::hook_installer::{
    check_hooks_installed, install_hooks, preview_hook_config, refresh_wsl_hooks, remove_hooks,
};
use deva_light::http_server::HttpServerController;
use deva_light::logging::log_info;
use deva_light::types::LightState;
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Position, Size, State};

#[derive(Debug, Serialize)]
pub struct Diagnostics {
    pub config_dir: String,
    pub runtime_path: String,
    pub lock_path: String,
    pub log_path: String,
    pub claude_settings_path: String,
    pub hook_binary_path: String,
    pub codex_sessions_path: String,
    pub codex_sessions_paths: Vec<String>,
    pub codex_manual_paths: Vec<String>,
    pub codex_missing_paths: Vec<String>,
    pub hooks_installed: bool,
    pub hook_binary_exists: bool,
    pub runtime_exists: bool,
    pub light_count: usize,
    pub recent_log: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigView {
    pub config_path: String,
    pub http_bind: String,
    pub http_port: Option<u16>,
    pub runtime_port: Option<u16>,
    pub always_on_top: bool,
    pub notifications_enabled: bool,
    pub notify_on_waiting: bool,
    pub notify_on_done: bool,
    pub codex_manual_paths: Vec<String>,
    pub display_mode: String,
    pub remote_ssh_target: Option<String>,
    pub remote_codex_via_ssh: bool,
    pub ssh_identity_file: Option<String>,
    pub http_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigResult {
    pub runtime_port: Option<u16>,
    pub http_reloaded: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfigUpdate {
    pub http_bind: String,
    pub http_port: Option<u16>,
    pub always_on_top: Option<bool>,
    pub notifications_enabled: Option<bool>,
    pub notify_on_waiting: Option<bool>,
    pub notify_on_done: Option<bool>,
    pub codex_manual_paths: Option<Vec<String>>,
    pub display_mode: Option<String>,
    pub remote_ssh_target: Option<String>,
    pub remote_codex_via_ssh: Option<bool>,
    pub ssh_identity_file: Option<String>,
    pub regenerate_http_token: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfigView {
    pub display_mode: String,
}

#[tauri::command]
pub fn confirm_light(project_id: String, aggregator: State<Arc<StateAggregator>>) {
    aggregator.confirm_light(&project_id);
}

#[tauri::command]
pub fn confirm_session(session_id: String, aggregator: State<Arc<StateAggregator>>) {
    aggregator.confirm_session(&session_id);
}

#[tauri::command]
pub fn remove_light(project_id: String, aggregator: State<Arc<StateAggregator>>) {
    aggregator.remove_light(&project_id);
}

#[tauri::command]
pub fn get_lights(aggregator: State<Arc<StateAggregator>>) -> Vec<LightState> {
    aggregator.get_lights()
}

#[tauri::command]
pub fn open_project(project_id: String) -> Result<(), String> {
    open_path(&project_id)
}

#[tauri::command]
pub fn open_session_logs(project_id: String) -> Result<(), String> {
    let path = claude_project_log_dir(&project_id)?;
    open_path(&path.to_string_lossy())
}

#[tauri::command]
pub fn open_app_log() -> Result<(), String> {
    let log_path = get_log_path();
    if !log_path.exists() {
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&log_path, "").map_err(|error| error.to_string())?;
    }

    open_path(&log_path.to_string_lossy())
}

#[tauri::command]
pub fn get_app_config() -> AppConfigView {
    let config = load_app_config();
    AppConfigView {
        config_path: get_config_path().to_string_lossy().to_string(),
        http_bind: config.http_bind,
        http_port: config.http_port,
        runtime_port: load_runtime_config().map(|runtime| runtime.http_port),
        always_on_top: config.always_on_top,
        notifications_enabled: config.notifications_enabled,
        notify_on_waiting: config.notify_on_waiting,
        notify_on_done: config.notify_on_done,
        codex_manual_paths: config.codex_session_paths,
        display_mode: display_mode_to_string(&config.display_mode),
        remote_ssh_target: config.remote_ssh_target.clone(),
        remote_codex_via_ssh: config.remote_codex_via_ssh,
        ssh_identity_file: config.ssh_identity_file.clone(),
        http_token: config.http_token.clone(),
    }
}

#[tauri::command]
pub fn get_ui_config() -> UiConfigView {
    let config = load_app_config();
    UiConfigView {
        display_mode: display_mode_to_string(&config.display_mode),
    }
}

#[tauri::command]
pub fn save_app_config_command(
    app: AppHandle,
    http_server: State<Arc<HttpServerController>>,
    aggregator: State<Arc<StateAggregator>>,
    update: AppConfigUpdate,
) -> Result<SaveConfigResult, String> {
    validate_http_bind(&update.http_bind)?;
    validate_http_port(update.http_port)?;

    let previous = load_app_config();
    let mut config = previous.clone();
    config.http_bind = update.http_bind;
    config.http_port = update.http_port;
    if let Some(always_on_top) = update.always_on_top {
        config.always_on_top = always_on_top;
    }
    if let Some(notifications_enabled) = update.notifications_enabled {
        config.notifications_enabled = notifications_enabled;
    }
    if let Some(notify_on_waiting) = update.notify_on_waiting {
        config.notify_on_waiting = notify_on_waiting;
    }
    if let Some(notify_on_done) = update.notify_on_done {
        config.notify_on_done = notify_on_done;
    }
    if let Some(codex_manual_paths) = update.codex_manual_paths {
        config.codex_session_paths = normalize_codex_manual_paths(codex_manual_paths);
    }
    if let Some(display_mode) = update.display_mode.as_deref() {
        config.display_mode = parse_display_mode(display_mode)?;
    }
    if let Some(remote_ssh_target) = update.remote_ssh_target {
        let trimmed = remote_ssh_target.trim().to_string();
        config.remote_ssh_target = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
    }
    if let Some(remote_codex_via_ssh) = update.remote_codex_via_ssh {
        config.remote_codex_via_ssh = remote_codex_via_ssh;
    }
    if let Some(ssh_identity_file) = update.ssh_identity_file {
        let trimmed = ssh_identity_file.trim().to_string();
        config.ssh_identity_file = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        };
    }
    if update.regenerate_http_token == Some(true) {
        config.http_token = Some(deva_light::config::generate_http_token());
    } else if config.http_bind == "0.0.0.0" {
        let _ = ensure_http_token(&mut config);
    }

    save_app_config(&config).map_err(|error| error.to_string())?;

    let network_changed = previous.http_bind != config.http_bind
        || previous.http_port != config.http_port
        || previous.http_token != config.http_token
        || update.regenerate_http_token == Some(true);

    let mut runtime_port = load_runtime_config().map(|runtime| runtime.http_port);
    let mut http_reloaded = false;

    if network_changed {
        runtime_port = Some(
            http_server
                .restart(Arc::clone(&aggregator), &config)
                .map_err(|error| error.to_string())?,
        );
        http_reloaded = true;
        if let Err(error) = refresh_wsl_hooks() {
            log_info(
                "ipc",
                format!("saved config but failed to refresh WSL hooks: {error}"),
            );
        }
    }

    log_info(
        "ipc",
        format!(
            "saved app config http_bind={} http_port={:?} codex_manual_paths={} http_reloaded={http_reloaded}",
            config.http_bind,
            config.http_port,
            config.codex_session_paths.len()
        ),
    );
    let _ = app.emit("config-changed", get_ui_config());
    Ok(SaveConfigResult {
        runtime_port,
        http_reloaded,
    })
}

#[tauri::command]
pub fn set_always_on_top(app: AppHandle, enabled: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not available".to_string())?;

    window
        .set_always_on_top(enabled)
        .map_err(|error| error.to_string())?;

    let mut config = load_app_config();
    config.always_on_top = enabled;
    save_app_config(&config).map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn prepare_uninstall(keep_config: bool) -> Result<(), String> {
    // Remove hooks first
    remove_hooks().map_err(|error| error.to_string())?;

    if !keep_config {
        // Full cleanup - remove everything
        let config_dir = get_config_dir();
        std::fs::remove_dir_all(&config_dir).map_err(|error| error.to_string())?;
    } else {
        // Keep config.json, remove runtime files
        std::fs::remove_file(get_runtime_path()).ok();
        std::fs::remove_file(get_lock_path()).ok();
        std::fs::remove_file(get_log_path()).ok();
    }

    Ok(())
}

#[tauri::command]
pub fn get_diagnostics(aggregator: State<Arc<StateAggregator>>) -> Diagnostics {
    let log_path = get_log_path();
    let hook_binary_path = deva_light::hook_installer::get_hook_binary_path();
    let codex_summary = codex_session_root_summary();
    Diagnostics {
        config_dir: get_config_dir().to_string_lossy().to_string(),
        runtime_path: get_runtime_path().to_string_lossy().to_string(),
        lock_path: get_lock_path().to_string_lossy().to_string(),
        log_path: log_path.to_string_lossy().to_string(),
        claude_settings_path: deva_light::hook_installer::get_claude_settings_path()
            .to_string_lossy()
            .to_string(),
        hook_binary_path: hook_binary_path.to_string_lossy().to_string(),
        codex_sessions_path: format_paths(&codex_summary.active),
        codex_sessions_paths: codex_summary
            .active
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        codex_manual_paths: codex_summary
            .manual
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        codex_missing_paths: codex_summary
            .missing
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        hooks_installed: check_hooks_installed(),
        hook_binary_exists: hook_binary_path.exists(),
        runtime_exists: get_runtime_path().exists(),
        light_count: aggregator.get_lights().len(),
        recent_log: recent_log(&log_path),
    }
}

#[tauri::command]
pub fn copy_path(project_id: String) -> String {
    project_id
}

#[tauri::command]
pub fn pause_monitoring() {
    set_monitoring_paused(true);
}

#[tauri::command]
pub fn resume_monitoring() {
    set_monitoring_paused(false);
}

#[tauri::command]
pub fn get_monitoring_paused() -> bool {
    is_monitoring_paused()
}

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
pub async fn check_for_update(
    app: AppHandle,
) -> Result<Option<deva_light::updater::UpdateInfo>, String> {
    deva_light::updater::check_for_update(&app).await
}

#[tauri::command]
pub async fn download_and_install_update(app: AppHandle) -> Result<(), String> {
    deva_light::updater::download_and_install(&app).await
}

#[tauri::command]
pub fn get_remote_setup_info() -> Result<RemoteSetupInfo, String> {
    build_remote_setup_info()
}

#[tauri::command]
pub fn persist_window_position(x: i32, y: i32) -> Result<(), String> {
    let mut config = load_app_config();
    config.window_x = x;
    config.window_y = y;
    save_app_config(&config).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn test_ssh_connection(
    ssh_target: Option<String>,
    ssh_identity_file: Option<String>,
) -> deva_light::ssh_remote::SshConnectionTest {
    let config = load_app_config();
    let target = ssh_target
        .filter(|value| !value.trim().is_empty())
        .or(config.remote_ssh_target)
        .unwrap_or_default();
    let identity = ssh_identity_file
        .filter(|value| !value.trim().is_empty())
        .or(config.ssh_identity_file);

    deva_light::ssh_remote::test_ssh_connection(&target, identity.as_deref())
}

#[tauri::command]
pub fn open_settings(app: AppHandle, panel: Option<String>) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings window is not available".to_string())?;

    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    if let Some(panel) = panel.filter(|value| !value.trim().is_empty()) {
        let _ = window.emit("open-settings-panel", panel);
    }
    Ok(())
}

#[tauri::command]
pub fn resize_main_window(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window is not available".to_string())?;

    let width = width.clamp(54.0, 1200.0);
    let height = height.clamp(64.0, 900.0);

    window
        .set_size(Size::Logical(LogicalSize::new(width, height)))
        .map_err(|error| error.to_string())?;

    keep_window_on_current_monitor(&window)?;
    Ok(())
}

fn keep_window_on_current_monitor(window: &tauri::WebviewWindow) -> Result<(), String> {
    let Some(monitor) = window
        .current_monitor()
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };

    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.outer_size().map_err(|error| error.to_string())?;
    let monitor_position = monitor.position();
    let monitor_size = monitor.size();

    let monitor_left = monitor_position.x;
    let monitor_top = monitor_position.y;
    let monitor_right = monitor_left + monitor_size.width as i32;
    let monitor_bottom = monitor_top + monitor_size.height as i32;

    let mut next_x = position.x;
    let mut next_y = position.y;

    if next_x + size.width as i32 > monitor_right {
        next_x = monitor_right - size.width as i32;
    }
    if next_y + size.height as i32 > monitor_bottom {
        next_y = monitor_bottom - size.height as i32;
    }

    next_x = next_x.max(monitor_left);
    next_y = next_y.max(monitor_top);

    if next_x != position.x || next_y != position.y {
        window
            .set_position(Position::Physical(PhysicalPosition::new(next_x, next_y)))
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn validate_http_bind(bind: &str) -> Result<(), String> {
    bind.parse::<IpAddr>().map(|_| ()).map_err(|_| {
        "HTTP bind must be an IP address, for example 127.0.0.1 or 0.0.0.0".to_string()
    })
}

fn display_mode_to_string(mode: &DisplayMode) -> String {
    match mode {
        DisplayMode::Parallel => "parallel".to_string(),
        DisplayMode::Compact => "compact".to_string(),
    }
}

fn parse_display_mode(value: &str) -> Result<DisplayMode, String> {
    match value {
        "parallel" => Ok(DisplayMode::Parallel),
        "compact" => Ok(DisplayMode::Compact),
        _ => Err("display mode must be parallel or compact".to_string()),
    }
}

fn validate_http_port(port: Option<u16>) -> Result<(), String> {
    if matches!(port, Some(0)) {
        return Err("HTTP port must be blank or between 1 and 65535".to_string());
    }

    Ok(())
}

fn normalize_codex_manual_paths(paths: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();

    for path in paths {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            continue;
        }

        if !normalized.iter().any(|existing| existing == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }

    normalized
}

#[tauri::command]
pub fn check_hooks() -> bool {
    check_hooks_installed()
}

#[tauri::command]
pub fn install_hooks_command() -> Result<(), String> {
    install_hooks().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remove_hooks_command() -> Result<(), String> {
    remove_hooks().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn preview_hook_config_command() -> Result<String, String> {
    preview_hook_config()
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

fn open_path(path: &str) -> Result<(), String> {
    let mut command = platform_open_command(path)?;

    command.spawn().map_err(|error| error.to_string())?;
    Ok(())
}

fn claude_project_log_dir(project_id: &str) -> Result<PathBuf, String> {
    let home = home_dir().ok_or_else(|| "failed to resolve home directory".to_string())?;
    Ok(home
        .join(".claude")
        .join("projects")
        .join(encode_claude_project_dir(project_id)))
}

fn encode_claude_project_dir(project_id: &str) -> String {
    project_id
        .replace("\\\\?\\", "")
        .replace(':', "")
        .replace(['\\', '/'], "-")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn recent_log(log_path: &PathBuf) -> String {
    let Ok(content) = fs::read_to_string(log_path) else {
        return String::new();
    };

    let lines: Vec<_> = content.lines().rev().take(20).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

fn platform_open_command(path: &str) -> Result<std::process::Command, String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer");
        command.arg(path);
        return Ok(command);
    }

    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        command.arg(path);
        return Ok(command);
    }

    #[cfg(target_os = "linux")]
    {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path);
        return Ok(command);
    }

    #[allow(unreachable_code)]
    Err("opening paths is not supported on this platform".to_string())
}
