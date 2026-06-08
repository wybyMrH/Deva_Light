use deva_light::aggregator::StateAggregator;
use deva_light::config::AppConfig;
use deva_light::http_server::start_http_server;
use deva_light::types::Status;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn hook_http_server_drives_session_lifecycle() {
    let config_dir = std::env::temp_dir().join(unique_name("deva-light-config"));
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("DEVA_LIGHT_CONFIG_DIR", &config_dir);

    let project_dir = std::env::temp_dir().join(unique_name("deva-light-project"));
    std::fs::create_dir_all(&project_dir).unwrap();

    let aggregator = Arc::new(StateAggregator::new());
    let port = start_http_server(Arc::clone(&aggregator), &AppConfig::default()).unwrap();

    post_event(
        port,
        &format!(
            r#"{{"event_type":"session-start","session_id":"s1","cwd":"{}"}}"#,
            json_path(&project_dir)
        ),
    );

    eventually(|| aggregator.session_status("s1") == Some(Status::Idle));

    post_event(
        port,
        r#"{"event_type":"prompt-submit","session_id":"s1","tool_call":"shell"}"#,
    );

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1
            && lights[0].status == Status::Working
            && lights[0].last_tool_call.as_deref() == Some("shell")
    });

    post_event(port, r#"{"event_type":"stop","session_id":"s1"}"#);

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1 && lights[0].status == Status::Done
    });

    post_event(port, r#"{"event_type":"prompt-submit","session_id":"s1"}"#);

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1 && lights[0].status == Status::Working
    });

    post_event(port, r#"{"event_type":"session-end","session_id":"s1"}"#);

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.is_empty()
    });

    let _ = std::fs::remove_dir_all(project_dir);
    let _ = std::fs::remove_dir_all(config_dir);
    std::env::remove_var("DEVA_LIGHT_CONFIG_DIR");
}

#[test]
fn orphan_hook_event_auto_creates_session() {
    let config_dir = std::env::temp_dir().join(unique_name("deva-light-orphan-config"));
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("DEVA_LIGHT_CONFIG_DIR", &config_dir);

    let project_dir = std::env::temp_dir().join(unique_name("deva-light-orphan-project"));
    std::fs::create_dir_all(&project_dir).unwrap();

    let aggregator = Arc::new(StateAggregator::new());
    let port = start_http_server(Arc::clone(&aggregator), &AppConfig::default()).unwrap();

    post_event(
        port,
        &format!(
            r#"{{"event_type":"prompt-submit","session_id":"orphan-1","cwd":"{}"}}"#,
            json_path(&project_dir)
        ),
    );

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1 && lights[0].status == Status::Working
    });

    let _ = std::fs::remove_dir_all(project_dir);
    let _ = std::fs::remove_dir_all(config_dir);
    std::env::remove_var("DEVA_LIGHT_CONFIG_DIR");
}

#[test]
fn hook_error_notification_stays_error_after_stop() {
    let config_dir = std::env::temp_dir().join(unique_name("deva-light-error-config"));
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("DEVA_LIGHT_CONFIG_DIR", &config_dir);

    let project_dir = std::env::temp_dir().join(unique_name("deva-light-error-project"));
    std::fs::create_dir_all(&project_dir).unwrap();

    let aggregator = Arc::new(StateAggregator::new());
    let port = start_http_server(Arc::clone(&aggregator), &AppConfig::default()).unwrap();

    post_event(
        port,
        &format!(
            r#"{{"event_type":"session-start","session_id":"s-error","cwd":"{}"}}"#,
            json_path(&project_dir)
        ),
    );
    post_event(
        port,
        r#"{"event_type":"notification","session_id":"s-error","message":"unexpected status 502 Bad Gateway: auth_not_found: no auth available"}"#,
    );

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1
            && lights[0].status == Status::Error
            && lights[0].sessions[0]
                .error_message
                .as_deref()
                .is_some_and(|message| message.contains("502 Bad Gateway"))
    });

    post_event(port, r#"{"event_type":"stop","session_id":"s-error"}"#);

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1 && lights[0].status == Status::Error
    });

    post_event(
        port,
        r#"{"event_type":"session-end","session_id":"s-error"}"#,
    );

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1 && lights[0].status == Status::Error
    });

    let _ = std::fs::remove_dir_all(project_dir);
    let _ = std::fs::remove_dir_all(config_dir);
    std::env::remove_var("DEVA_LIGHT_CONFIG_DIR");
}

#[test]
fn hook_permission_request_exposes_pending_action_summary() {
    let config_dir = std::env::temp_dir().join(unique_name("deva-light-pending-config"));
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("DEVA_LIGHT_CONFIG_DIR", &config_dir);

    let project_dir = std::env::temp_dir().join(unique_name("deva-light-pending-project"));
    std::fs::create_dir_all(&project_dir).unwrap();

    let aggregator = Arc::new(StateAggregator::new());
    let port = start_http_server(Arc::clone(&aggregator), &AppConfig::default()).unwrap();

    post_event(
        port,
        &format!(
            r#"{{"event_type":"permission-request","session_id":"s-pending","cwd":"{}","tool_call":"Bash","message":"允许执行 npm test 吗？"}}"#,
            json_path(&project_dir)
        ),
    );

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1
            && lights[0].status == Status::Waiting
            && lights[0].sessions[0]
                .pending_action
                .as_ref()
                .is_some_and(|action| action.title.contains("npm test"))
    });

    post_event(
        port,
        r#"{"event_type":"post-tool-use","session_id":"s-pending","tool_call":"Bash"}"#,
    );

    eventually(|| {
        let lights = aggregator.get_lights();
        lights.len() == 1
            && lights[0].status == Status::Working
            && lights[0].sessions[0].pending_action.is_none()
    });

    let _ = std::fs::remove_dir_all(project_dir);
    let _ = std::fs::remove_dir_all(config_dir);
    std::env::remove_var("DEVA_LIGHT_CONFIG_DIR");
}

#[test]
fn hook_http_server_respects_fixed_port_config() {
    let config_dir = std::env::temp_dir().join(unique_name("deva-light-fixed-port-config"));
    std::fs::create_dir_all(&config_dir).unwrap();
    std::env::set_var("DEVA_LIGHT_CONFIG_DIR", &config_dir);

    let probe = TcpListener::bind("127.0.0.1:0").unwrap();
    let fixed_port = probe.local_addr().unwrap().port();
    drop(probe);

    let aggregator = Arc::new(StateAggregator::new());
    let config = AppConfig {
        http_port: Some(fixed_port),
        ..AppConfig::default()
    };

    let port = start_http_server(aggregator, &config).unwrap();
    assert_eq!(port, fixed_port);

    let _ = std::fs::remove_dir_all(config_dir);
    std::env::remove_var("DEVA_LIGHT_CONFIG_DIR");
}

fn post_event(port: u16, body: &str) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let request = format!(
        "POST /events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    stream.write_all(request.as_bytes()).unwrap();

    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
}

fn eventually(mut predicate: impl FnMut() -> bool) {
    for _ in 0..20 {
        if predicate() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    assert!(predicate());
}

fn unique_name(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    format!("{prefix}-{nanos}")
}

fn json_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}
