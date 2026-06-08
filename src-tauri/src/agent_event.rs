use crate::types::{Status, Tool};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderId {
    ClaudeCode,
    Cursor,
    Codex,
}

impl ProviderId {
    pub fn tool(self) -> Tool {
        match self {
            Self::ClaudeCode => Tool::ClaudeCode,
            Self::Cursor => Tool::Cursor,
            Self::Codex => Tool::Codex,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentEventType {
    SessionStart,
    SessionEnd,
    PromptSubmit,
    PreToolUse,
    PermissionRequest,
    ToolUpdate,
    Notification,
    Stop,
    Error,
    Other,
}

impl AgentEventType {
    pub fn from_raw(value: &str) -> Self {
        match value {
            "session-start" => Self::SessionStart,
            "session-end" => Self::SessionEnd,
            "prompt-submit" | "before-submit-prompt" => Self::PromptSubmit,
            "pre-tool-use" => Self::PreToolUse,
            "permission-request" => Self::PermissionRequest,
            "post-tool-use"
            | "before-shell-execution"
            | "after-shell-execution"
            | "before-mcp-execution"
            | "after-mcp-execution"
            | "before-read-file"
            | "after-file-edit"
            | "after-agent-response"
            | "after-agent-thought"
            | "subagent-start"
            | "subagent-stop"
            | "pre-compact" => Self::ToolUpdate,
            "notification" => Self::Notification,
            "stop" => Self::Stop,
            "post-tool-use-failure"
            | "error"
            | "stream-error"
            | "connection-error"
            | "retry-error"
            | "turn-aborted" => Self::Error,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrivacyLevel {
    MetadataOnly,
    LocalSensitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendingActionKind {
    ToolApproval,
    PermissionRequest,
    ShellExecution,
    McpExecution,
    FileRead,
    UserQuestion,
    StaleSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UserDecisionKind {
    Approve,
    Deny,
    AskInProvider,
    Defer,
    OpenProvider,
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeoutDecision {
    AskInProvider,
    Defer,
    Deny,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingActionSummary {
    pub action_id: String,
    pub kind: PendingActionKind,
    pub title: String,
    pub decisions: Vec<UserDecisionKind>,
    pub expires_at_ms: i64,
}

impl PendingActionSummary {
    pub fn new(
        provider: ProviderId,
        session_id: &str,
        kind: PendingActionKind,
        title: impl Into<String>,
        decisions: Vec<UserDecisionKind>,
        ttl_ms: i64,
    ) -> Self {
        let now = unix_time_ms();
        Self {
            action_id: action_id(provider, session_id, kind, now),
            kind,
            title: title.into(),
            decisions,
            expires_at_ms: now.saturating_add(ttl_ms),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentEvent {
    pub event_id: String,
    pub provider: ProviderId,
    pub session_id: String,
    pub cwd: Option<PathBuf>,
    pub status: Option<Status>,
    pub event_type: AgentEventType,
    pub raw_event_type: String,
    pub task_hint: Option<String>,
    pub tool_name: Option<String>,
    pub summary: Option<String>,
    pub pending_action: Option<PendingActionSummary>,
    pub privacy: PrivacyLevel,
}

impl AgentEvent {
    pub fn new(
        provider: ProviderId,
        session_id: String,
        raw_event_type: impl Into<String>,
    ) -> Self {
        let raw_event_type = raw_event_type.into();
        let now = unix_time_ms();
        Self {
            event_id: format!(
                "{}:{}:{}",
                provider_slug(provider),
                sanitize_id(&session_id),
                now
            ),
            provider,
            session_id,
            cwd: None,
            status: None,
            event_type: AgentEventType::from_raw(&raw_event_type),
            raw_event_type,
            task_hint: None,
            tool_name: None,
            summary: None,
            pending_action: None,
            privacy: PrivacyLevel::MetadataOnly,
        }
    }
}

pub fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn action_id(
    provider: ProviderId,
    session_id: &str,
    kind: PendingActionKind,
    now_ms: i64,
) -> String {
    format!(
        "{}:{}:{:?}:{}",
        provider_slug(provider),
        sanitize_id(session_id),
        kind,
        now_ms
    )
}

fn provider_slug(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::ClaudeCode => "claude",
        ProviderId::Cursor => "cursor",
        ProviderId::Codex => "codex",
    }
}

fn sanitize_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .take(80)
        .collect()
}
