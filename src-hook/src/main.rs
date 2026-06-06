use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct RuntimeConfig {
    http_port: u16,
}

#[derive(Debug, Serialize)]
struct HookEvent {
    event_type: String,
    session_id: String,
    cwd: Option<String>,
    tool_call: Option<String>,
    task_hint: Option<String>,
    source: Option<String>,
}

fn main() {
    let payload = match read_stdin_payload() {
        Ok(payload) => payload,
        Err(error) => {
            append_log(format!("ignored: invalid stdin payload: {error}"));
            return;
        }
    };

    let raw_event_type = env::args().nth(1);
    let event_type = resolve_event_type(raw_event_type.as_deref(), &payload);
    if event_type == "unknown" {
        append_log("ignored: could not resolve event type");
        return;
    }

    let Some((target_url, target_source)) = resolve_event_url() else {
        append_log(format!(
            "ignored: no target url for event={event_type}; runtime_path={}",
            runtime_config_path().display()
        ));
        return;
    };

    let source = resolve_source(&payload);
    let session_id = resolve_session_id(&payload, &event_type);
    let event = HookEvent {
        event_type,
        session_id,
        cwd: resolve_cwd(&payload),
        tool_call: resolve_tool_call(&payload),
        task_hint: resolve_task_hint(&payload),
        source: Some(source.to_string()),
    };

    match post_event(&target_url, &event) {
        Ok(status) => append_log(format!(
            "sent: event={} session={} source={} target={} via={} status={}",
            event.event_type, event.session_id, source, target_url, target_source, status
        )),
        Err(error) => append_log(format!(
            "failed: event={} session={} source={} target={} via={} error={}",
            event.event_type, event.session_id, source, target_url, target_source, error
        )),
    }
}

fn read_stdin_payload() -> Result<serde_json::Value, String> {
    let mut stdin_content = String::new();
    io::stdin()
        .read_to_string(&mut stdin_content)
        .map_err(|error| error.to_string())?;

    if stdin_content.trim().is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }

    serde_json::from_str(&stdin_content).map_err(|error| error.to_string())
}

fn resolve_event_url() -> Option<(String, &'static str)> {
    if let Some(url) = env::var_os("AI_LIGHT_URL").and_then(|value| {
        let value = value.to_string_lossy().trim().to_string();
        (!value.is_empty()).then_some(value)
    }) {
        return Some((normalize_event_url(&url), "AI_LIGHT_URL"));
    }

    let config = load_runtime_config()?;
    Some((
        format!("http://127.0.0.1:{}/events", config.http_port),
        "runtime.json",
    ))
}

fn load_runtime_config() -> Option<RuntimeConfig> {
    let content = fs::read_to_string(runtime_config_path()).ok()?;
    serde_json::from_str(&content).ok()
}

fn runtime_config_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".deva_light")
        .join("runtime.json")
}

fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

fn extract_string(payload: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        payload
            .get(key)
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
    })
}

fn resolve_event_type(argv_event: Option<&str>, payload: &serde_json::Value) -> String {
    if let Some(arg) = argv_event.filter(|value| !value.is_empty()) {
        return normalize_event_type(arg);
    }

    if let Some(name) = extract_string(payload, &["hook_event_name", "event_type", "eventType"]) {
        return normalize_event_type(&name);
    }

    "unknown".to_string()
}

fn resolve_source(payload: &serde_json::Value) -> &'static str {
    if payload.get("hook_event_name").is_some()
        || payload.get("cursor_version").is_some()
        || payload.get("conversation_id").is_some()
        || payload.get("conversationId").is_some()
    {
        "cursor"
    } else {
        "claude"
    }
}

fn resolve_session_id(payload: &serde_json::Value, event_type: &str) -> String {
    if matches!(event_type, "subagent-start" | "subagent-stop") {
        if let Some(id) = extract_string(
            payload,
            &[
                "parent_conversation_id",
                "parentConversationId",
                "conversation_id",
                "conversationId",
                "session_id",
                "sessionId",
            ],
        ) {
            return id;
        }
    }

    extract_string(
        payload,
        &[
            "conversation_id",
            "conversationId",
            "session_id",
            "sessionId",
        ],
    )
    .unwrap_or_else(|| "unknown".to_string())
}

fn resolve_cwd(payload: &serde_json::Value) -> Option<String> {
    extract_string(payload, &["cwd"]).or_else(|| {
        payload
            .get("workspace_roots")
            .and_then(|value| value.as_array())
            .and_then(|roots| roots.first())
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
    })
}

fn resolve_tool_call(payload: &serde_json::Value) -> Option<String> {
    extract_string(
        payload,
        &["tool_name", "tool", "toolName", "command", "subagent_type"],
    )
}

fn resolve_task_hint(payload: &serde_json::Value) -> Option<String> {
    extract_string(payload, &["prompt", "task", "user_prompt", "message", "description"])
}

fn normalize_event_type(event_type: &str) -> String {
    match event_type {
        // Claude Code
        "SessionStart" | "session_start" | "sessionstart" | "sessionStart" => "session-start",
        "UserPromptSubmit" | "prompt_submit" | "user-prompt-submit" | "userpromptsubmit" => {
            "prompt-submit"
        }
        "PreToolUse" | "pre_tool_use" | "pre-tool-use" | "pretooluse" | "preToolUse" => {
            "pre-tool-use"
        }
        "PermissionRequest" | "permission_request" | "permission-request" | "permissionrequest" => {
            "permission-request"
        }
        "PostToolUse" | "post_tool_use" | "post-tool-use" | "posttooluse" | "postToolUse" => {
            "post-tool-use"
        }
        "Notification" | "notification" => "notification",
        "Stop" | "stop" => "stop",
        "SessionEnd" | "session_end" | "sessionend" | "sessionEnd" => "session-end",
        // Cursor-specific
        "beforeSubmitPrompt" | "before-submit-prompt" => "before-submit-prompt",
        "postToolUseFailure" | "post-tool-use-failure" => "post-tool-use-failure",
        "beforeShellExecution" | "before-shell-execution" => "before-shell-execution",
        "afterShellExecution" | "after-shell-execution" => "after-shell-execution",
        "beforeMCPExecution" | "before-mcp-execution" => "before-mcp-execution",
        "afterMCPExecution" | "after-mcp-execution" => "after-mcp-execution",
        "beforeReadFile" | "before-read-file" => "before-read-file",
        "afterFileEdit" | "after-file-edit" => "after-file-edit",
        "afterAgentResponse" | "after-agent-response" => "after-agent-response",
        "afterAgentThought" | "after-agent-thought" => "after-agent-thought",
        "subagentStart" | "subagent-start" => "subagent-start",
        "subagentStop" | "subagent-stop" => "subagent-stop",
        "preCompact" | "pre-compact" => "pre-compact",
        other => other,
    }
    .to_string()
}

fn post_event(url: &str, event: &HookEvent) -> Result<u16, String> {
    let client = reqwest::blocking::Client::new();
    let mut request = client.post(url).json(event);

    if let Ok(token) = env::var("AI_LIGHT_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            request = request.header("X-Deva-Light-Token", token);
        }
    }

    let response = request.send().map_err(|error| error.to_string())?;

    Ok(response.status().as_u16())
}

fn normalize_event_url(url: &str) -> String {
    if url.ends_with("/events") {
        url.to_string()
    } else {
        format!("{}/events", url.trim_end_matches('/'))
    }
}

fn append_log(message: impl AsRef<str>) {
    let Some(home) = home_dir() else {
        return;
    };

    let log_dir = home.join(".deva_light");
    if fs::create_dir_all(&log_dir).is_err() {
        return;
    }

    let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("hook.log"))
    else {
        return;
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let _ = writeln!(file, "[{timestamp}] {}", message.as_ref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_claude_and_cursor_hook_names() {
        assert_eq!(normalize_event_type("SessionStart"), "session-start");
        assert_eq!(normalize_event_type("sessionStart"), "session-start");
        assert_eq!(normalize_event_type("preToolUse"), "pre-tool-use");
        assert_eq!(
            normalize_event_type("beforeShellExecution"),
            "before-shell-execution"
        );
        assert_eq!(normalize_event_type("subagentStart"), "subagent-start");
    }

    #[test]
    fn resolves_cursor_conversation_id() {
        let payload = serde_json::json!({
            "conversation_id": "conv-123",
            "hook_event_name": "beforeShellExecution",
            "command": "npm test"
        });

        assert_eq!(resolve_source(&payload), "cursor");
        assert_eq!(
            resolve_session_id(&payload, "before-shell-execution"),
            "conv-123"
        );
        assert_eq!(
            resolve_event_type(None, &payload),
            "before-shell-execution"
        );
    }

    #[test]
    fn subagent_events_use_parent_conversation_id() {
        let payload = serde_json::json!({
            "subagent_id": "sub-1",
            "parent_conversation_id": "conv-parent",
            "hook_event_name": "subagentStart"
        });

        assert_eq!(
            resolve_session_id(&payload, "subagent-start"),
            "conv-parent"
        );
    }

    #[test]
    fn extracts_workspace_root_as_cwd() {
        let payload = serde_json::json!({
            "workspace_roots": ["/home/user/project"]
        });

        assert_eq!(
            resolve_cwd(&payload).as_deref(),
            Some("/home/user/project")
        );
    }
}
