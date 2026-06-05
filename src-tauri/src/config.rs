use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    Parallel,
    Compact,
}

impl Default for DisplayMode {
    fn default() -> Self {
        Self::Parallel
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct AppConfig {
    pub window_x: i32,
    pub window_y: i32,
    pub monitoring_paused: bool,
    pub hooks_installed: bool,
    pub http_bind: String,
    pub http_port: Option<u16>,
    pub http_token: Option<String>,
    pub always_on_top: bool,
    pub notifications_enabled: bool,
    pub notify_on_waiting: bool,
    pub notify_on_done: bool,
    pub codex_session_paths: Vec<String>,
    pub display_mode: DisplayMode,
    pub remote_ssh_target: Option<String>,
    pub remote_codex_via_ssh: bool,
    pub ssh_identity_file: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            window_x: 100,
            window_y: 100,
            monitoring_paused: false,
            hooks_installed: false,
            http_bind: "127.0.0.1".to_string(),
            http_port: None,
            always_on_top: true,
            notifications_enabled: true,
            notify_on_waiting: true,
            notify_on_done: false,
            codex_session_paths: Vec::new(),
            http_token: None,
            display_mode: DisplayMode::Parallel,
            remote_ssh_target: None,
            remote_codex_via_ssh: true,
            ssh_identity_file: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub http_port: u16,
    #[serde(default)]
    pub http_token: Option<String>,
}

pub fn ensure_http_token(config: &mut AppConfig) -> Option<String> {
    if config.http_bind != "0.0.0.0" {
        return config.http_token.clone();
    }

    if config.http_token.is_none() {
        config.http_token = Some(generate_http_token());
    }

    config.http_token.clone()
}

pub fn generate_http_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);

    format!(
        "{:016x}{:016x}",
        nanos ^ (std::process::id() as u128),
        counter ^ (std::process::id() as u64)
    )
}

pub fn get_config_dir() -> PathBuf {
    if let Some(config_dir) = std::env::var_os("DEVA_LIGHT_CONFIG_DIR") {
        return PathBuf::from(config_dir);
    }

    home_dir()
        .expect("failed to resolve home directory")
        .join(".deva_light")
}

pub fn get_config_path() -> PathBuf {
    get_config_dir().join("config.json")
}

pub fn get_runtime_path() -> PathBuf {
    get_config_dir().join("runtime.json")
}

pub fn get_lock_path() -> PathBuf {
    get_config_dir().join("deva-light.lock")
}

pub fn get_log_path() -> PathBuf {
    get_config_dir().join("deva-light.log")
}

pub fn load_app_config() -> AppConfig {
    let path = get_config_path();
    let Ok(content) = fs::read_to_string(path) else {
        return AppConfig::default();
    };

    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    serde_json::from_str(content).unwrap_or_default()
}

pub fn save_app_config(config: &AppConfig) -> io::Result<()> {
    fs::create_dir_all(get_config_dir())?;
    let content = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
    fs::write(get_config_path(), content)
}

pub fn load_runtime_config() -> Option<RuntimeConfig> {
    let content = fs::read_to_string(get_runtime_path()).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_runtime_config(config: &RuntimeConfig) -> io::Result<()> {
    fs::create_dir_all(get_config_dir())?;
    let content = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
    fs::write(get_runtime_path(), content)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}
