use crate::aggregator::StateAggregator;
use crate::claude_watcher::{claude_session_process_alive, live_claude_session_ids};
use crate::codex_watcher::live_codex_session_ids;
use crate::cursor_watcher::recent_cursor_session_ids;
use crate::logging::log_info;
use crate::types::{Status, Tool};
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshLightsResult {
    pub removed_sessions: usize,
    pub updated_sessions: usize,
}

pub fn refresh_tracked_sessions(aggregator: &StateAggregator) -> RefreshLightsResult {
    let live_claude = live_claude_session_ids();
    let live_codex = live_codex_session_ids();
    let live_cursor = recent_cursor_session_ids();

    let tracked_sessions = aggregator.tracked_sessions();
    let mut to_remove = Vec::new();
    let mut to_mark_done = Vec::new();

    for session in tracked_sessions {
        if session.status == Status::Error {
            continue;
        }

        let is_live = match session.tool {
            Tool::ClaudeCode => live_claude.contains(&session.session_id),
            Tool::Codex => live_codex.contains(&session.session_id),
            Tool::Cursor => live_cursor.contains(&session.session_id),
        };

        if !is_live {
            to_remove.push(session.session_id);
            continue;
        }

        if session.tool == Tool::ClaudeCode
            && session.status == Status::Working
            && claude_session_process_alive(&session.session_id) == Some(false)
        {
            to_mark_done.push(session.session_id);
        }
    }

    let mut updated_sessions = 0usize;
    for session_id in to_mark_done {
        aggregator.update_session_status(&session_id, Status::Done);
        updated_sessions += 1;
    }

    let mut removed_sessions = 0usize;
    for session_id in to_remove {
        aggregator.remove_session(&session_id);
        removed_sessions += 1;
    }

    if removed_sessions > 0 || updated_sessions > 0 {
        log_info(
            "session_refresh",
            format!(
                "removed {removed_sessions} stale session(s), updated {updated_sessions} session(s) to Done"
            ),
        );
    }

    RefreshLightsResult {
        removed_sessions,
        updated_sessions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn refresh_removes_untracked_working_session() {
        let aggregator = StateAggregator::new();
        aggregator.add_session(
            "phantom-claude".to_string(),
            Tool::ClaudeCode,
            Path::new("/tmp/demo"),
            Status::Working,
        );

        let result = refresh_tracked_sessions(&aggregator);

        assert_eq!(result.removed_sessions, 1);
        assert_eq!(result.updated_sessions, 0);
        assert!(aggregator.get_lights().is_empty());
    }

    #[test]
    fn refresh_keeps_error_sessions_without_live_source() {
        let aggregator = StateAggregator::new();
        aggregator.add_session(
            "broken".to_string(),
            Tool::Codex,
            Path::new("/tmp/demo"),
            Status::Error,
        );

        let result = refresh_tracked_sessions(&aggregator);

        assert_eq!(result.removed_sessions, 0);
        assert_eq!(aggregator.get_lights().len(), 1);
        assert_eq!(aggregator.get_lights()[0].status, Status::Error);
    }
}
