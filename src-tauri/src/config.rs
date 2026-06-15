use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Global config cache to avoid repeated disk reads.
/// This is especially important because get_lights() is called every second
/// and previously read config.json from disk each time.
static CONFIG_CACHE: once_cell::sync::Lazy<Arc<RwLock<AppConfig>>> =
    once_cell::sync::Lazy::new(|| Arc::new(RwLock::new(load_app_config_from_disk())));

/// Load config from disk (internal function, use get_cached_config() instead)
fn load_app_config_from_disk() -> AppConfig {
    let path = get_config_path();
    let Ok(content) = fs::read_to_string(path) else {
        return AppConfig::default();
    };

    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let value: serde_json::Value =
        serde_json::from_str(content).unwrap_or(serde_json::Value::Object(Default::default()));
    let mut config: AppConfig = serde_json::from_value(value.clone()).unwrap_or_default();
    migrate_legacy_ssh_target(&mut config, &value);
    config
}

/// Get cached config (fast, no disk I/O)
pub fn get_cached_config() -> AppConfig {
    CONFIG_CACHE
        .read()
        .expect("config cache lock poisoned")
        .clone()
}

/// Refresh cache from disk (call after saving config)
pub fn refresh_config_cache() {
    let new_config = load_app_config_from_disk();
    *CONFIG_CACHE.write().expect("config cache lock poisoned") = new_config;
}

/// Update cache with new config directly (faster than refresh from disk)
pub fn update_config_cache(config: &AppConfig) {
    *CONFIG_CACHE.write().expect("config cache lock poisoned") = config.clone();
}

/// Legacy function - now uses cache. Kept for backwards compatibility.
pub fn load_app_config() -> AppConfig {
    get_cached_config()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    #[default]
    Parallel,
    Compact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct SshRemoteTarget {
    pub target: String,
    #[serde(default)]
    pub identity_file: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub passphrase: Option<String>,
}

impl SshRemoteTarget {
    pub fn normalized(&self) -> Option<Self> {
        let target = self.target.trim().to_string();
        if target.is_empty() {
            return None;
        }

        let identity_file = self
            .identity_file
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let label = self
            .label
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        let passphrase = self
            .passphrase
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);

        Some(Self {
            target,
            identity_file,
            label,
            passphrase,
        })
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
    pub done_light_auto_dismiss: bool,
    pub codex_session_paths: Vec<String>,
    pub display_mode: DisplayMode,
    pub remote_ssh_targets: Vec<SshRemoteTarget>,
    pub remote_codex_via_ssh: bool,
    pub origin_aliases: HashMap<String, String>,
    #[serde(default)]
    pub ssh_discovery_dismissed: Vec<String>,
    #[serde(default = "default_auto_update_enabled")]
    pub auto_update_enabled: bool,
    #[serde(default)]
    pub news_base_url: Option<String>,
    #[serde(default)]
    pub proxy_url: Option<String>,
}

fn default_auto_update_enabled() -> bool {
    true
}

impl AppConfig {
    pub fn normalized_ssh_targets(&self) -> Vec<SshRemoteTarget> {
        let mut targets = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for entry in &self.remote_ssh_targets {
            let Some(normalized) = entry.normalized() else {
                continue;
            };
            if seen.insert(normalized.target.clone()) {
                targets.push(normalized);
            }
        }

        targets
    }
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
            notifications_enabled: false,
            notify_on_waiting: true,
            notify_on_done: false,
            done_light_auto_dismiss: false,
            codex_session_paths: Vec::new(),
            http_token: None,
            display_mode: DisplayMode::Parallel,
            remote_ssh_targets: Vec::new(),
            remote_codex_via_ssh: true,
            origin_aliases: HashMap::new(),
            ssh_discovery_dismissed: Vec::new(),
            auto_update_enabled: true,
            news_base_url: None,
            proxy_url: None,
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

fn migrate_legacy_ssh_target(config: &mut AppConfig, value: &serde_json::Value) {
    if !config.remote_ssh_targets.is_empty() {
        return;
    }

    let Some(target) = value
        .get("remote_ssh_target")
        .and_then(|entry| entry.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    else {
        return;
    };

    let identity_file = value
        .get("ssh_identity_file")
        .and_then(|entry| entry.as_str())
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string);

    config.remote_ssh_targets.push(SshRemoteTarget {
        target: target.to_string(),
        identity_file,
        label: None,
        passphrase: None,
    });
}

pub fn save_app_config(config: &AppConfig) -> io::Result<()> {
    fs::create_dir_all(get_config_dir())?;
    let content = serde_json::to_string_pretty(config).map_err(io::Error::other)?;
    fs::write(get_config_path(), content)?;
    // Update cache after saving so subsequent reads are consistent
    update_config_cache(config);
    Ok(())
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
