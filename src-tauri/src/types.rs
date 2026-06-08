use crate::agent_event::PendingActionSummary;
use crate::monitor_origin::MonitorOrigin;
use serde::{Deserialize, Serialize};
use std::time::Instant;

/// Traffic light status for AI coding assistant.
///
/// Semantic mapping (unified with claude-code-traffic-light convention):
/// - Working: 🟢 Green - AI is actively processing (prompt-submit, pre-tool-use, post-tool-use)
/// - Waiting: 🟡 Yellow - Waiting for user action (permission-request, notification)
/// - Error: 🔴 Flashing red - Session hit an error, failed retry, auth/network problem, etc.
/// - Done: 🔴 Red - Session ended or task complete (stop, session-end)
/// - Idle: 🔴 Red - Session started but not yet active
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Status {
    Idle = 0,    // Red - session started, waiting for first prompt
    Done = 1,    // Red - session ended / task complete
    Working = 2, // Green - AI is actively working
    Waiting = 3, // Yellow - needs user attention (permission, notification)
    Error = 4,   // Flashing red - error / failed retry / auth or network problem
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tool {
    ClaudeCode,
    Codex,
    Cursor,
}

/// Source of the session (for retention policy)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionSource {
    Cli,
    VsCodePlugin,
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionRef {
    pub session_id: String,
    pub tool: Tool,
    pub status: Status,
    #[serde(skip)]
    pub started_at: Instant,
    /// Human-readable task name (from Codex prompt or Claude context)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_name: Option<String>,
    /// Human-readable error summary shown while the session is in Error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Lightweight waiting/action summary safe to send to the frontend.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_action: Option<PendingActionSummary>,
    /// Monitor environment (local / WSL / SSH / remote LAN)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitor_origin: Option<MonitorOrigin>,
    /// Process ID for alive detection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_id: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LightState {
    pub project_id: String,
    pub project_label: String,
    pub logical_project_id: String,
    pub monitor_origin: MonitorOrigin,
    pub origin_key: String,
    pub origin_detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_display: Option<String>,
    pub status: Status,
    pub sessions: Vec<SessionRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_path: Option<String>,
    #[serde(skip)]
    pub last_event_at: Instant,
    pub last_tool_call: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

impl LightState {
    pub fn new(
        project_id: String,
        logical_project_id: String,
        project_label: String,
        monitor_origin: MonitorOrigin,
        origin_key: String,
        origin_detail: String,
    ) -> Self {
        Self {
            project_id,
            project_label,
            logical_project_id,
            monitor_origin,
            origin_key,
            origin_detail,
            origin_display: None,
            status: Status::Idle,
            sessions: Vec::new(),
            workspace_path: None,
            last_event_at: Instant::now(),
            last_tool_call: None,
            last_error: None,
        }
    }

    /// Aggregate status from all sessions (max by severity)
    pub fn aggregate_status(&mut self) {
        self.status = self
            .sessions
            .iter()
            .map(|s| s.status)
            .max()
            .unwrap_or(Status::Idle);
    }

    /// A lamp is shown while it needs attention, is working, or is in its
    /// post-completion red-light retention window.
    pub fn is_active(&self) -> bool {
        self.sessions.iter().any(|session| {
            matches!(
                session.status,
                Status::Working | Status::Waiting | Status::Done | Status::Error
            )
        })
    }
}
