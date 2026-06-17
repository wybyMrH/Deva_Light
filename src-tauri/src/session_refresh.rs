use crate::aggregator::StateAggregator;
use crate::claude_watcher::{
    claude_session_process_alive, discover_claude_sessions, live_claude_session_ids,
};
use crate::codex_watcher::live_codex_session_ids;
use crate::cursor_watcher::{discover_cursor_sessions, recent_cursor_session_ids};
use crate::logging::log_info;
use crate::types::{Status, Tool};
use serde::Serialize;
use std::time::Duration;

const CURSOR_HOOK_ACTIVITY: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshLightsResult {
    pub removed_sessions: usize,
    pub updated_sessions: usize,
    pub added_sessions: usize,
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
            Tool::Cursor => {
                live_cursor.contains(&session.session_id)
                    || aggregator
                        .session_had_recent_hook_activity(&session.session_id, CURSOR_HOOK_ACTIVITY)
            }
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

    // Proactively discover brand-new sessions so the "refresh" action lights up
    // lamps immediately instead of waiting for the next watcher poll.
    let mut added_sessions = 0usize;
    for (session_id, cwd) in discover_claude_sessions() {
        if aggregator.session_status(&session_id).is_some() {
            continue;
        }
        if claude_session_process_alive(&session_id) != Some(true) {
            continue;
        }
        aggregator.add_session(session_id, Tool::ClaudeCode, &cwd, Status::Working);
        added_sessions += 1;
    }
    for (session_id, cwd) in discover_cursor_sessions() {
        if aggregator.session_status(&session_id).is_some() {
            continue;
        }
        aggregator.add_session(session_id, Tool::Cursor, &cwd, Status::Working);
        added_sessions += 1;
    }

    if added_sessions > 0 {
        log_info(
            "session_refresh",
            format!("discovered {added_sessions} new session(s)"),
        );
    }

    RefreshLightsResult {
        removed_sessions,
        updated_sessions,
        added_sessions,
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
        // The phantom must be gone. (Other sessions may be discovered from the
        // real on-disk state on a developer machine, so don't assert emptiness.)
        assert!(aggregator.session_status("phantom-claude").is_none());
    }

    #[test]
    fn refresh_keeps_cursor_session_with_recent_hook_activity() {
        let aggregator = StateAggregator::new();
        aggregator.add_session(
            "cursor-conv-not-on-disk".to_string(),
            Tool::Cursor,
            Path::new("/tmp/demo"),
            Status::Working,
        );
        aggregator.record_hook_activity("cursor-conv-not-on-disk");

        let result = refresh_tracked_sessions(&aggregator);

        assert_eq!(result.removed_sessions, 0);
        assert_eq!(
            aggregator.session_status("cursor-conv-not-on-disk"),
            Some(Status::Working)
        );
    }
}
