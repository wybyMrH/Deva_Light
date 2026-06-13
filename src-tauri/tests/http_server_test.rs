use deva_light::agent_event::{PendingActionKind, ProviderId, UserDecisionKind};
use deva_light::http_server::{parse_hook_event, HookEvent};
use deva_light::types::Status;

#[test]
fn parse_session_start_event() {
    let payload =
        r#"{"event_type":"session-start","session_id":"abc123","cwd":"/home/user/project"}"#;

    let event = parse_hook_event(payload).unwrap();

    assert_eq!(event.event_type, "session-start");
    assert_eq!(event.session_id, "abc123");
    assert_eq!(event.cwd.as_deref(), Some("/home/user/project"));
}

#[test]
fn parse_prompt_submit_event() {
    let payload = r#"{"event_type":"prompt-submit","session_id":"abc123"}"#;

    let event = parse_hook_event(payload).unwrap();

    assert_eq!(event.event_type, "prompt-submit");
    assert_eq!(event.session_id, "abc123");
    assert_eq!(event.cwd, None);
}

#[test]
fn parse_event_accepts_claude_style_aliases_and_missing_session_id() {
    let payload = r#"{"event_type":"prompt-submit","sessionId":"abc123","toolName":"Bash"}"#;

    let event = parse_hook_event(payload).unwrap();

    assert_eq!(event.session_id, "abc123");
    assert_eq!(event.tool_call.as_deref(), Some("Bash"));

    let missing_session = parse_hook_event(r#"{"event_type":"notification"}"#).unwrap();
    assert_eq!(missing_session.session_id, "unknown");
}

#[test]
fn map_hook_event_types_to_statuses() {
    assert_eq!(
        HookEvent::event_type_to_status("session-start"),
        Some(Status::Idle)
    );
    assert_eq!(
        HookEvent::event_type_to_status("prompt-submit"),
        Some(Status::Working)
    );
    assert_eq!(
        HookEvent::event_type_to_status("pre-tool-use"),
        Some(Status::Working)
    );
    assert_eq!(
        HookEvent::event_type_to_status("permission-request"),
        Some(Status::Waiting)
    );
    assert_eq!(
        HookEvent::event_type_to_status("post-tool-use"),
        Some(Status::Working)
    );
    assert_eq!(
        HookEvent::event_type_to_status("post-tool-use-failure"),
        Some(Status::Error)
    );
    assert_eq!(
        HookEvent::event_type_to_status("stream-error"),
        Some(Status::Error)
    );
    assert_eq!(
        HookEvent::event_type_to_status("notification"),
        Some(Status::Waiting)
    );
    assert_eq!(HookEvent::event_type_to_status("stop"), Some(Status::Done));
    assert_eq!(HookEvent::event_type_to_status("session-end"), None);
    assert_eq!(
        HookEvent::event_type_to_status("before-shell-execution"),
        Some(Status::Waiting)
    );
    assert_eq!(
        HookEvent::event_type_to_status("before-mcp-execution"),
        Some(Status::Waiting)
    );
    assert_eq!(
        HookEvent::event_type_to_status("after-shell-execution"),
        Some(Status::Working)
    );
}

#[test]
fn cursor_event_resolves_cursor_tool() {
    let event = parse_hook_event(
        r#"{"event_type":"before-shell-execution","session_id":"conv-1","source":"cursor"}"#,
    )
    .unwrap();

    assert_eq!(event.resolve_tool(), deva_light::types::Tool::Cursor);
    assert!(event.should_track());
}

#[test]
fn cursor_pre_tool_use_resolves_to_working() {
    let event = parse_hook_event(
        r#"{"event_type":"pre-tool-use","session_id":"conv-1","source":"cursor","tool_call":"Shell"}"#,
    )
    .unwrap();

    // Cursor running a tool (e.g. Bash) is actively working, not waiting.
    assert_eq!(event.resolve_status(), Some(Status::Working));
}

#[test]
fn permission_request_builds_pending_action_summary() {
    let event = parse_hook_event(
        r#"{"event_type":"permission-request","session_id":"s1","tool_call":"Bash","message":"允许执行 npm test 吗？"}"#,
    )
    .unwrap();

    let agent_event = event.to_agent_event();
    let pending = agent_event.pending_action.unwrap();

    assert_eq!(agent_event.provider, ProviderId::ClaudeCode);
    assert_eq!(pending.kind, PendingActionKind::PermissionRequest);
    assert!(pending.title.contains("npm test"));
    assert_eq!(
        pending.decisions,
        vec![UserDecisionKind::OpenProvider, UserDecisionKind::Dismiss]
    );
}

#[test]
fn error_notification_resolves_to_error() {
    let event = parse_hook_event(
        r#"{"event_type":"notification","session_id":"s1","message":"unexpected status 502 Bad Gateway: auth_not_found: no auth available"}"#,
    )
    .unwrap();

    assert_eq!(event.resolve_status(), Some(Status::Error));
    assert_eq!(
        event.error_message().as_deref(),
        Some("unexpected status 502 Bad Gateway: auth_not_found: no auth available")
    );
}
