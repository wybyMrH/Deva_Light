use deva_light::monitor_origin::MonitorOrigin;
use deva_light::types::{LightState, SessionRef, Status, Tool};
use std::time::Instant;

#[test]
fn test_status_ordering() {
    assert!(Status::Error > Status::Waiting);
    assert!(Status::Waiting > Status::Working);
    assert!(Status::Working > Status::Done);
    assert!(Status::Done > Status::Idle);
}

#[test]
fn test_status_max() {
    let statuses = [Status::Idle, Status::Working, Status::Done];
    assert_eq!(statuses.iter().max(), Some(&Status::Working));

    let with_error = [Status::Working, Status::Waiting, Status::Error];
    assert_eq!(with_error.iter().max(), Some(&Status::Error));
}

#[test]
fn test_light_state_aggregation() {
    let mut light = LightState::new(
        "local@@local".to_string(),
        "/home/user/project".to_string(),
        "project".to_string(),
        MonitorOrigin::Local,
        "local".to_string(),
        "本地".to_string(),
    );

    // No sessions = Idle
    light.aggregate_status();
    assert_eq!(light.status, Status::Idle);

    // Add working session
    light.sessions.push(SessionRef {
        session_id: "s1".to_string(),
        tool: Tool::ClaudeCode,
        status: Status::Working,
        started_at: Instant::now(),
        task_name: None,
        error_message: None,
        pending_action: None,
        monitor_origin: Some(MonitorOrigin::Local),
        process_id: None,
    });
    light.aggregate_status();
    assert_eq!(light.status, Status::Working);

    // Add waiting session - should override
    light.sessions.push(SessionRef {
        session_id: "s2".to_string(),
        tool: Tool::Codex,
        status: Status::Waiting,
        started_at: Instant::now(),
        task_name: None,
        error_message: None,
        pending_action: None,
        monitor_origin: Some(MonitorOrigin::Local),
        process_id: None,
    });
    light.aggregate_status();
    assert_eq!(light.status, Status::Waiting);

    // Add error session - should override waiting
    light.sessions.push(SessionRef {
        session_id: "s3".to_string(),
        tool: Tool::Codex,
        status: Status::Error,
        started_at: Instant::now(),
        task_name: None,
        error_message: Some("unexpected status 502 Bad Gateway".to_string()),
        pending_action: None,
        monitor_origin: Some(MonitorOrigin::Local),
        process_id: None,
    });
    light.aggregate_status();
    assert_eq!(light.status, Status::Error);
}
