use deva_light::aggregator::StateAggregator;
use deva_light::codex_paths::{codex_session_root_summary, format_paths};
use deva_light::config::{
    ensure_http_token, get_config_dir, get_config_path, get_lock_path, get_log_path,
    get_runtime_path, load_app_config, load_runtime_config, save_app_config, DisplayMode,
};
use deva_light::hook_installer::{
    check_hooks_installed, install_hooks, preview_hook_config, refresh_wsl_hooks, remove_hooks,
};
use deva_light::http_server::HttpServerController;
use deva_light::logging::log_info;
use deva_light::monitoring::{is_monitoring_paused, set_monitoring_paused};
use deva_light::providers::ProviderCapabilityView;
use deva_light::remote::{build_remote_setup_info, RemoteSetupInfo};
use deva_light::types::LightState;
use deva_light::window_behavior::{apply_main_window_pin, configure_main_window_workspace};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Position, Size, State};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
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
    pub provider_capabilities: Vec<ProviderCapabilityView>,
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
    pub done_light_auto_dismiss: bool,
    pub codex_manual_paths: Vec<String>,
    pub display_mode: String,
    pub remote_ssh_targets: Vec<SshRemoteTargetView>,
    pub remote_codex_via_ssh: bool,
    pub origin_aliases: Vec<OriginAliasView>,
    pub http_token: Option<String>,
    pub auto_update_enabled: bool,
    pub news_base_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OriginAliasView {
    pub key: String,
    pub alias: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SshRemoteTargetView {
    pub target: String,
    pub identity_file: Option<String>,
    pub label: Option<String>,
    pub passphrase: Option<String>,
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
    pub done_light_auto_dismiss: Option<bool>,
    pub codex_manual_paths: Option<Vec<String>>,
    pub display_mode: Option<String>,
    pub remote_ssh_targets: Option<Vec<SshRemoteTargetView>>,
    pub remote_codex_via_ssh: Option<bool>,
    pub origin_aliases: Option<Vec<OriginAliasView>>,
    pub regenerate_http_token: Option<bool>,
    pub auto_update_enabled: Option<bool>,
    pub news_base_url: Option<String>,
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
    aggregator.prune_expired_done_lights(done_light_retention());
    aggregator.get_lights()
}

#[tauri::command]
pub fn refresh_lights(aggregator: State<Arc<StateAggregator>>) -> deva_light::session_refresh::RefreshLightsResult {
    deva_light::session_refresh::refresh_tracked_sessions(&aggregator)
}

#[tauri::command]
pub fn open_project(
    project_id: String,
    aggregator: State<Arc<StateAggregator>>,
) -> Result<(), String> {
    if let Some(path) = aggregator.workspace_path(&project_id) {
        return open_path(&path);
    }

    if let Some(logical) = project_id.split("@@").next() {
        if logical.starts_with("git:") {
            return Err("Git 项目请在对应环境（本地/WSL/远程）中打开工作区".to_string());
        }
        return open_path(logical);
    }

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
    let remote_ssh_targets = config
        .normalized_ssh_targets()
        .into_iter()
        .map(|entry| SshRemoteTargetView {
            target: entry.target.clone(),
            identity_file: entry.identity_file.clone(),
            label: entry.label.clone().or_else(|| {
                config
                    .origin_aliases
                    .get(&format!("ssh:{}", entry.target))
                    .cloned()
            }),
            passphrase: entry.passphrase.clone(),
        })
        .collect();
    let origin_aliases = config
        .origin_aliases
        .iter()
        .map(|(key, alias)| OriginAliasView {
            key: key.clone(),
            alias: alias.clone(),
        })
        .collect();

    AppConfigView {
        config_path: get_config_path().to_string_lossy().to_string(),
        http_bind: config.http_bind,
        http_port: config.http_port,
        runtime_port: load_runtime_config().map(|runtime| runtime.http_port),
        always_on_top: config.always_on_top,
        notifications_enabled: config.notifications_enabled,
        notify_on_waiting: config.notify_on_waiting,
        notify_on_done: config.notify_on_done,
        done_light_auto_dismiss: config.done_light_auto_dismiss,
        codex_manual_paths: config.codex_session_paths,
        display_mode: display_mode_to_string(&config.display_mode),
        remote_ssh_targets,
        remote_codex_via_ssh: config.remote_codex_via_ssh,
        origin_aliases,
        http_token: config.http_token,
        auto_update_enabled: config.auto_update_enabled,
        news_base_url: config.news_base_url,
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
pub fn set_display_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let mut config = load_app_config();
    config.display_mode = parse_display_mode(&mode)?;
    save_app_config(&config).map_err(|error| error.to_string())?;
    let _ = app.emit("config-changed", get_ui_config());
    Ok(())
}

#[tauri::command]
pub fn set_done_light_auto_dismiss(enabled: bool) -> Result<(), String> {
    let mut config = load_app_config();
    config.done_light_auto_dismiss = enabled;
    save_app_config(&config).map_err(|error| error.to_string())?;
    log_info(
        "ipc",
        format!("done_light_auto_dismiss set to {enabled}"),
    );
    Ok(())
}

#[tauri::command]
pub fn set_auto_update_enabled(enabled: bool) -> Result<(), String> {
    let mut config = load_app_config();
    config.auto_update_enabled = enabled;
    save_app_config(&config).map_err(|error| error.to_string())?;
    log_info("ipc", format!("auto_update_enabled set to {enabled}"));
    Ok(())
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
    if let Some(done_light_auto_dismiss) = update.done_light_auto_dismiss {
        config.done_light_auto_dismiss = done_light_auto_dismiss;
    }
    if let Some(codex_manual_paths) = update.codex_manual_paths {
        config.codex_session_paths = normalize_codex_manual_paths(codex_manual_paths);
    }
    if let Some(display_mode) = update.display_mode.as_deref() {
        config.display_mode = parse_display_mode(display_mode)?;
    }
    if let Some(remote_ssh_targets) = update.remote_ssh_targets {
        config.remote_ssh_targets = remote_ssh_targets
            .into_iter()
            .filter_map(|entry| {
                deva_light::config::SshRemoteTarget {
                    target: entry.target,
                    identity_file: entry.identity_file,
                    label: entry.label,
                    passphrase: entry.passphrase,
                }
                .normalized()
            })
            .collect();

        for entry in &config.remote_ssh_targets {
            if let Some(label) = entry.label.as_deref().filter(|value| !value.is_empty()) {
                config
                    .origin_aliases
                    .insert(format!("ssh:{}", entry.target), label.to_string());
            }
        }
    }
    if let Some(origin_aliases) = update.origin_aliases {
        config.origin_aliases = origin_aliases
            .into_iter()
            .filter_map(|entry| {
                let key = entry.key.trim().to_string();
                let alias = entry.alias.trim().to_string();
                if key.is_empty() || alias.is_empty() {
                    None
                } else {
                    Some((key, alias))
                }
            })
            .collect();
    }
    if let Some(remote_codex_via_ssh) = update.remote_codex_via_ssh {
        config.remote_codex_via_ssh = remote_codex_via_ssh;
    }
    if let Some(auto_update_enabled) = update.auto_update_enabled {
        config.auto_update_enabled = auto_update_enabled;
    }
    if let Some(news_base_url) = update.news_base_url {
        let trimmed = news_base_url.trim();
        config.news_base_url = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
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
pub fn set_always_on_top(
    app: AppHandle,
    aggregator: State<Arc<StateAggregator>>,
    enabled: bool,
) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not available".to_string())?;

    let pin_window = enabled && aggregator.has_active_lights();
    apply_main_window_pin(&window, pin_window).map_err(|error| error.to_string())?;

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
        provider_capabilities: deva_light::providers::all_provider_capabilities(),
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
pub fn get_remote_setup_info(probe_ssh: Option<bool>) -> Result<RemoteSetupInfo, String> {
    build_remote_setup_info(probe_ssh.unwrap_or(false))
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
    ssh_passphrase: Option<String>,
) -> deva_light::ssh_remote::SshConnectionTest {
    let config = load_app_config();
    let target = ssh_target
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            config
                .normalized_ssh_targets()
                .first()
                .map(|entry| entry.target.clone())
        })
        .unwrap_or_default();
    let identity = ssh_identity_file
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            config
                .normalized_ssh_targets()
                .first()
                .and_then(|entry| entry.identity_file.clone())
        });
    let passphrase = ssh_passphrase
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            config
                .normalized_ssh_targets()
                .first()
                .and_then(|entry| entry.passphrase.clone())
        });

    deva_light::ssh_remote::test_ssh_connection(&target, identity.as_deref(), passphrase.as_deref())
}

#[tauri::command]
pub fn pick_ssh_private_key() -> Result<Option<String>, String> {
    deva_light::ssh_remote::pick_ssh_private_key()
}

#[tauri::command]
pub fn get_ssh_setup_guide() -> deva_light::ssh_setup::SshSetupGuide {
    deva_light::ssh_setup::build_ssh_setup_guide()
}

#[tauri::command]
pub fn discover_ssh_key_candidates() -> Vec<deva_light::ssh_setup::SshKeyCandidate> {
    let config = load_app_config();
    deva_light::ssh_setup::discover_ssh_key_candidates()
        .into_iter()
        .filter(|candidate| !config.ssh_discovery_dismissed.contains(&candidate.id))
        .collect()
}

#[tauri::command]
pub fn dismiss_ssh_discovery(candidate_id: String) -> Result<(), String> {
    let trimmed = candidate_id.trim();
    if trimmed.is_empty() {
        return Err("candidate id is empty".to_string());
    }

    let mut config = load_app_config();
    if !config
        .ssh_discovery_dismissed
        .iter()
        .any(|id| id == trimmed)
    {
        config.ssh_discovery_dismissed.push(trimmed.to_string());
        save_app_config(&config).map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub fn read_ssh_public_key(identity_path: String) -> Result<String, String> {
    deva_light::ssh_setup::read_ssh_public_key(&identity_path)
}

#[tauri::command]
pub fn hide_settings(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings window is not available".to_string())?;
    window.hide().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_config_dir() -> Result<(), String> {
    open_path(&get_config_dir().to_string_lossy())
}

#[tauri::command]
pub fn open_path_in_explorer(path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("路径为空".to_string());
    }

    let target = PathBuf::from(trimmed);
    if target.is_file() {
        return open_path(&target.to_string_lossy());
    }

    open_path(&target.to_string_lossy())
}

#[tauri::command]
pub fn open_settings(app: AppHandle, panel: Option<String>) -> Result<(), String> {
    let window = app
        .get_webview_window("settings")
        .ok_or_else(|| "settings window is not available".to_string())?;

    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    let _ = window.emit("settings-reload", ());
    if let Some(panel) = panel.filter(|value| !value.trim().is_empty()) {
        let _ = window.emit("open-settings-panel", panel);
    }
    Ok(())
}

#[tauri::command]
pub fn get_news_sources() -> Vec<deva_light::news::NewsSourceView> {
    deva_light::news::sources().to_vec()
}

#[tauri::command]
pub async fn fetch_news(
    source: String,
    force: Option<bool>,
) -> Result<deva_light::news::NewsResult, String> {
    deva_light::news::fetch_source(&source, force.unwrap_or(false)).await
}

#[tauri::command]
pub fn open_in_browser(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http") {
        return Err("无效的链接".to_string());
    }
    open_url(trimmed)
}

#[tauri::command]
pub fn open_news(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("news")
        .ok_or_else(|| "资讯窗口不可用".to_string())?;
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    let _ = window.emit("news-reload", ());
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

    configure_main_window_workspace(&window);

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

pub fn done_light_retention() -> std::time::Duration {
    if load_app_config().done_light_auto_dismiss {
        std::time::Duration::from_secs(15)
    } else {
        std::time::Duration::from_secs(120)
    }
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
pub fn check_cursor_hooks() -> bool {
    deva_light::hook_installer::check_cursor_hooks_installed()
}

#[tauri::command]
pub fn install_hooks_command() -> Result<(), String> {
    install_hooks().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn install_cursor_hooks_command() -> Result<(), String> {
    deva_light::hook_installer::install_cursor_hooks().map_err(|error| error.to_string())
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

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Ok(())
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
