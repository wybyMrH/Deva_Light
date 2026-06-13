use deva_light::aggregator::StateAggregator;
use deva_light::types::{Status, Tool};
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn add_session_creates_project_light() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::ClaudeCode, &cwd, Status::Working);

    let lights = agg.get_lights();
    assert_eq!(lights.len(), 1);
    assert_eq!(lights[0].status, Status::Working);
    assert_eq!(lights[0].sessions.len(), 1);
}

#[test]
fn update_session_status_reaggregates_light() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::ClaudeCode, &cwd, Status::Working);
    agg.update_session_status("s1", Status::Done);

    let lights = agg.get_lights();
    assert_eq!(lights.len(), 1);
    assert_eq!(lights[0].status, Status::Done);
    assert_eq!(agg.session_status("s1"), Some(Status::Done));

    assert!(agg.prune_expired_done_lights(Duration::ZERO));
    assert!(agg.get_lights().is_empty());
    assert_eq!(agg.session_status("s1"), None);
}

#[test]
fn remove_working_session_removes_empty_light() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::ClaudeCode, &cwd, Status::Working);
    agg.remove_session("s1");

    assert!(agg.get_lights().is_empty());
}

#[test]
fn set_task_name_updates_session_label() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::ClaudeCode, &cwd, Status::Working);
    agg.set_task_name("s1", "Fix the drawer switching bug".to_string());

    let lights = agg.get_lights();
    assert_eq!(
        lights[0].sessions[0].task_name.as_deref(),
        Some("Fix the drawer switching bug")
    );
}

#[test]
fn confirm_waiting_session_keeps_tracking() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::ClaudeCode, &cwd, Status::Waiting);
    agg.confirm_session("s1");

    assert!(agg.get_lights().is_empty());
    assert_eq!(agg.session_status("s1"), Some(Status::Idle));
}

#[test]
fn confirm_done_session_removes_tracking() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::ClaudeCode, &cwd, Status::Done);
    agg.confirm_session("s1");

    assert!(agg.get_lights().is_empty());
    assert_eq!(agg.session_status("s1"), None);
}

#[test]
fn done_light_is_removed_when_session_ends() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::ClaudeCode, &cwd, Status::Done);
    agg.remove_session("s1");

    assert!(agg.get_lights().is_empty());
}

#[test]
fn separates_lights_by_tool() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::ClaudeCode, &cwd, Status::Working);
    agg.add_session("s2".to_string(), Tool::Codex, &cwd, Status::Waiting);
    agg.add_session("s3".to_string(), Tool::Cursor, &cwd, Status::Error);

    let lights = agg.get_lights();
    // Same project, different tools → one independent lamp per tool.
    assert_eq!(lights.len(), 3);
    assert!(lights.iter().all(|light| light.sessions.len() == 1));

    let mut by_tool: std::collections::HashMap<Tool, Status> =
        std::collections::HashMap::new();
    for light in &lights {
        by_tool.insert(light.sessions[0].tool, light.status);
    }
    assert_eq!(by_tool.get(&Tool::ClaudeCode), Some(&Status::Working));
    assert_eq!(by_tool.get(&Tool::Codex), Some(&Status::Waiting));
    assert_eq!(by_tool.get(&Tool::Cursor), Some(&Status::Error));
}

#[test]
fn error_light_is_not_pruned_as_done() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::Codex, &cwd, Status::Error);
    agg.set_error_message("s1", "unexpected status 502 Bad Gateway".to_string());

    assert!(!agg.prune_expired_done_lights(Duration::ZERO));
    let lights = agg.get_lights();
    assert_eq!(lights.len(), 1);
    assert_eq!(lights[0].status, Status::Error);
    assert_eq!(
        lights[0].sessions[0].error_message.as_deref(),
        Some("unexpected status 502 Bad Gateway")
    );
}

#[test]
fn automatic_status_updates_do_not_overwrite_error() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::Codex, &cwd, Status::Working);
    agg.update_session_status("s1", Status::Error);
    agg.update_session_status("s1", Status::Working);
    agg.update_session_status("s1", Status::Waiting);
    agg.update_session_status("s1", Status::Done);

    assert_eq!(agg.session_status("s1"), Some(Status::Error));
    assert_eq!(agg.get_lights()[0].status, Status::Error);
}

#[test]
fn confirming_error_session_removes_tracking() {
    let agg = StateAggregator::new();
    let cwd = PathBuf::from("/home/user/project");

    agg.add_session("s1".to_string(), Tool::Codex, &cwd, Status::Error);
    agg.confirm_session("s1");

    assert!(agg.get_lights().is_empty());
    assert_eq!(agg.session_status("s1"), None);
}

#[test]
fn preserves_first_seen_project_order() {
    let agg = StateAggregator::new();

    agg.add_session(
        "s1".to_string(),
        Tool::ClaudeCode,
        &PathBuf::from("/home/user/first"),
        Status::Working,
    );
    agg.add_session(
        "s2".to_string(),
        Tool::Codex,
        &PathBuf::from("/home/user/second"),
        Status::Working,
    );
    agg.update_session_status("s1", Status::Done);

    let labels: Vec<_> = agg
        .get_lights()
        .iter()
        .map(|light| light.project_label.clone())
        .collect();

    assert_eq!(labels, vec!["first", "second"]);

    assert!(agg.prune_expired_done_lights(Duration::ZERO));
    let labels: Vec<_> = agg
        .get_lights()
        .iter()
        .map(|light| light.project_label.clone())
        .collect();
    assert_eq!(labels, vec!["second"]);

    agg.add_session(
        "s3".to_string(),
        Tool::ClaudeCode,
        &PathBuf::from("/home/user/third"),
        Status::Working,
    );

    let labels: Vec<_> = agg
        .get_lights()
        .iter()
        .map(|light| light.project_label.clone())
        .collect();

    assert_eq!(labels, vec!["second", "third"]);
}
