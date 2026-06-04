use deva_light::config::{get_config_dir, AppConfig, RuntimeConfig};
use std::sync::Mutex;

static CONFIG_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn config_dir_points_to_deva_light_home_directory() {
    let _guard = CONFIG_ENV_LOCK.lock().unwrap();
    std::env::remove_var("DEVA_LIGHT_CONFIG_DIR");

    let dir = get_config_dir();

    assert_eq!(dir.file_name().unwrap().to_string_lossy(), ".deva_light");
    assert!(dir.parent().is_some());
}

#[test]
fn app_config_defaults_match_mvp_startup_state() {
    let config = AppConfig::default();

    assert_eq!(config.window_x, 100);
    assert_eq!(config.window_y, 100);
    assert!(!config.monitoring_paused);
    assert!(!config.hooks_installed);
    assert_eq!(config.http_bind, "127.0.0.1");
    assert_eq!(config.http_port, None);
    assert!(config.codex_session_paths.is_empty());
}

#[test]
fn app_config_deserializes_old_documents_with_defaults() {
    let json = r#"{"window_x":250,"window_y":260}"#;
    let parsed: AppConfig = serde_json::from_str(json).unwrap();

    assert_eq!(parsed.window_x, 250);
    assert_eq!(parsed.window_y, 260);
    assert_eq!(parsed.http_bind, "127.0.0.1");
    assert_eq!(parsed.http_port, None);
    assert!(parsed.codex_session_paths.is_empty());
}

#[test]
fn load_app_config_accepts_utf8_bom() {
    let _guard = CONFIG_ENV_LOCK.lock().unwrap();
    let config_dir = std::env::temp_dir().join(unique_name("deva-light-config-bom"));
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("DEVA_LIGHT_CONFIG_DIR", &config_dir);

    std::fs::write(
        config_dir.join("config.json"),
        "\u{feff}{\"http_bind\":\"0.0.0.0\",\"http_port\":17321}",
    )
    .unwrap();

    let config = deva_light::config::load_app_config();
    assert_eq!(config.http_bind, "0.0.0.0");
    assert_eq!(config.http_port, Some(17321));

    let _ = std::fs::remove_dir_all(config_dir);
    std::env::remove_var("DEVA_LIGHT_CONFIG_DIR");
}

#[test]
fn runtime_config_serializes_http_port() {
    let runtime = RuntimeConfig { http_port: 12345 };

    let json = serde_json::to_string(&runtime).unwrap();
    let parsed: RuntimeConfig = serde_json::from_str(&json).unwrap();

    assert!(json.contains("12345"));
    assert_eq!(parsed, runtime);
}

fn unique_name(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}-{nanos}")
}
