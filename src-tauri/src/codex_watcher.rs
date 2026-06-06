use crate::aggregator::StateAggregator;
use crate::codex_paths::{
    auto_codex_sessions_dirs, codex_session_root_summary_for_auto, is_wsl_unc_path,
    CodexSessionRootSummary,
};
use crate::config::load_app_config;
use crate::monitoring::is_monitoring_paused;
use crate::ssh_remote::{is_ssh_virtual_path, read_rollout_from_offset, rollout_modified};
use crate::logging::{log_error, log_info, log_warn};
use crate::types::{Status, Tool};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

const POLL_INTERVAL: Duration = Duration::from_secs(1);
const WSL_SCAN_INTERVAL: Duration = Duration::from_secs(10);
const STALE_WORKING_AFTER: Duration = Duration::from_secs(10 * 60);
const REMOVE_INACTIVE_AFTER: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexSessionMeta {
    pub session_id: String,
    pub cwd: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexLineEvent {
    SessionMeta(CodexSessionMeta),
    Status(Status),
    ToolCall(String),
    Ignore,
}

#[derive(Debug, Clone)]
struct WatchedRollout {
    offset: u64,
    meta: Option<CodexSessionMeta>,
    added_to_aggregator: bool,
    last_status: Option<Status>,
    last_activity_at: SystemTime,
}

pub fn start_codex_watcher(aggregator: Arc<StateAggregator>) -> io::Result<()> {
    thread::Builder::new()
        .name("ai-light-codex-watcher".to_string())
        .spawn(move || run_codex_watcher(aggregator))?;

    Ok(())
}

fn run_codex_watcher(aggregator: Arc<StateAggregator>) {
    let mut files = HashMap::new();
    let mut baseline = true;
    let auto_roots = auto_codex_sessions_dirs();
    let mut roots = codex_session_root_summary_for_auto(&auto_roots, &load_app_config());
    let mut last_wsl_scan = SystemTime::now()
        .checked_sub(WSL_SCAN_INTERVAL)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    log_root_summary("watching Codex session roots", &roots);

    loop {
        if is_monitoring_paused() {
            thread::sleep(POLL_INTERVAL);
            continue;
        }

        let next_roots = codex_session_root_summary_for_auto(&auto_roots, &load_app_config());
        if next_roots != roots {
            log_root_summary("reloaded Codex session roots", &next_roots);
            roots = next_roots;
            baseline = true;
        }

        let now = SystemTime::now();
        let should_scan_wsl = now
            .duration_since(last_wsl_scan)
            .map(|elapsed| elapsed >= WSL_SCAN_INTERVAL)
            .unwrap_or(true);
        let (scan_roots, tracked_only_roots) =
            partition_roots_for_poll(&roots.active, should_scan_wsl);

        if let Err(error) =
            poll_codex_sessions(&aggregator, &mut files, baseline, &scan_roots, &tracked_only_roots)
        {
            log_watcher_error("poll codex sessions", &error);
        }
        if should_scan_wsl {
            last_wsl_scan = now;
        }
        baseline = false;
        thread::sleep(POLL_INTERVAL);
    }
}

fn partition_roots_for_poll(
    roots: &[PathBuf],
    should_scan_wsl: bool,
) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut scan_roots = Vec::new();
    let mut tracked_only_roots = Vec::new();

    for root in roots {
        if is_wsl_unc_path(root) && !should_scan_wsl {
            tracked_only_roots.push(root.clone());
        } else {
            scan_roots.push(root.clone());
        }
    }

    (scan_roots, tracked_only_roots)
}

fn poll_codex_sessions(
    aggregator: &StateAggregator,
    files: &mut HashMap<PathBuf, WatchedRollout>,
    baseline: bool,
    scan_roots: &[PathBuf],
    tracked_only_roots: &[PathBuf],
) -> io::Result<()> {
    let mut rollouts = Vec::new();
    let mut seen = HashSet::new();
    let tracked_paths = files.keys().cloned().collect::<HashSet<_>>();

    for root in scan_roots {
        for path in find_rollout_files(root, &tracked_paths)? {
            if seen.insert(path.clone()) {
                rollouts.push(path);
            }
        }
    }

    for root in tracked_only_roots {
        for path in tracked_paths.iter().filter(|path| path.starts_with(root)) {
            if seen.insert(path.clone()) {
                rollouts.push(path.clone());
            }
        }
    }

    poll_rollout_paths(aggregator, files, baseline, rollouts)
}

#[cfg(test)]
fn poll_rollout_root(
    aggregator: &StateAggregator,
    files: &mut HashMap<PathBuf, WatchedRollout>,
    baseline: bool,
    root: &Path,
) -> io::Result<()> {
    let tracked_paths = files.keys().cloned().collect::<HashSet<_>>();
    let rollouts = find_rollout_files(&root, &tracked_paths)?;
    poll_rollout_paths(aggregator, files, baseline, rollouts)
}

fn poll_rollout_paths(
    aggregator: &StateAggregator,
    files: &mut HashMap<PathBuf, WatchedRollout>,
    baseline: bool,
    mut rollouts: Vec<PathBuf>,
) -> io::Result<()> {
    rollouts.sort();
    let live_paths: HashSet<_> = rollouts.iter().cloned().collect();
    let removed_paths = files
        .keys()
        .filter(|path| !live_paths.contains(*path))
        .cloned()
        .collect::<Vec<_>>();

    for path in removed_paths {
        if let Some(watched) = files.remove(&path) {
            remove_missing_rollout(aggregator, &path, watched);
        }
    }

    for path in rollouts {
        if files.contains_key(&path) {
            process_new_lines(aggregator, files, &path)?;
            if let Some(watched) = files.get_mut(&path) {
                update_inactive_session(aggregator, watched, &path)?;
            }
            continue;
        }

        let mut watched = WatchedRollout {
            offset: 0,
            meta: None,
            added_to_aggregator: false,
            last_status: None,
            last_activity_at: SystemTime::now(),
        };

        if baseline {
            initialize_existing_rollout(aggregator, &path, &mut watched)?;
        } else {
            log_watcher_note(&format!("tracking new rollout {}", path.display()));
        }

        files.insert(path.clone(), watched);

        if !baseline {
            process_new_lines(aggregator, files, &path)?;
        }

        if let Some(watched) = files.get_mut(&path) {
            update_inactive_session(aggregator, watched, &path)?;
        }
    }

    Ok(())
}

fn initialize_existing_rollout(
    aggregator: &StateAggregator,
    path: &Path,
    watched: &mut WatchedRollout,
) -> io::Result<()> {
    if is_ssh_virtual_path(path) {
        return initialize_existing_rollout_ssh(aggregator, path, watched);
    }

    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut last_tool_call = None;

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }

        match parse_codex_line(line.trim_end()) {
            Ok(CodexLineEvent::SessionMeta(meta)) => watched.meta = Some(meta),
            Ok(CodexLineEvent::Status(status)) => watched.last_status = Some(status),
            Ok(CodexLineEvent::ToolCall(tool_call)) => last_tool_call = Some(tool_call),
            Ok(CodexLineEvent::Ignore) => {}
            Err(error) => log_watcher_error(&format!("parse {}", path.display()), &error),
        }
    }

    watched.offset = reader.stream_position()?;
    watched.last_activity_at = rollout_modified_at(path).unwrap_or_else(|_| SystemTime::now());
    restore_existing_rollout(aggregator, path, watched, last_tool_call)?;
    Ok(())
}

fn restore_existing_rollout(
    aggregator: &StateAggregator,
    path: &Path,
    watched: &mut WatchedRollout,
    last_tool_call: Option<String>,
) -> io::Result<()> {
    let Some(meta) = watched.meta.clone() else {
        return Ok(());
    };

    let modified = rollout_modified_at(path)?;
    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return Ok(());
    };

    if age >= REMOVE_INACTIVE_AFTER {
        return Ok(());
    }

    let mut status = watched.last_status.unwrap_or(Status::Idle);
    if status == Status::Working && age >= STALE_WORKING_AFTER {
        status = Status::Waiting;
    }

    aggregator.add_session_with_context(
        meta.session_id.clone(),
        Tool::Codex,
        &meta.cwd,
        status,
        Some(path),
    );
    if let Some(tool_call) = last_tool_call {
        aggregator.set_last_tool_call(&meta.session_id, tool_call);
    }

    watched.added_to_aggregator = true;
    watched.last_status = Some(status);
    log_watcher_note(&format!(
        "restored session {} from {} with status {}",
        meta.session_id,
        path.display(),
        status_name(status)
    ));
    Ok(())
}

fn process_new_lines(
    aggregator: &StateAggregator,
    files: &mut HashMap<PathBuf, WatchedRollout>,
    path: &Path,
) -> io::Result<()> {
    if is_ssh_virtual_path(path) {
        return process_new_lines_ssh(aggregator, files, path);
    }

    let Some(watched) = files.get_mut(path) else {
        return Ok(());
    };

    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(watched.offset))?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        let line_start = reader.stream_position()?;
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }

        if !line.ends_with('\n') {
            reader.seek(SeekFrom::Start(line_start))?;
            break;
        }

        match parse_codex_line(line.trim_end()) {
            Ok(event) => apply_codex_event(aggregator, watched, event, path),
            Err(error) => log_watcher_error(&format!("parse {}", path.display()), &error),
        }
    }

    watched.offset = reader.stream_position()?;
    Ok(())
}

fn apply_codex_event(
    aggregator: &StateAggregator,
    watched: &mut WatchedRollout,
    event: CodexLineEvent,
    rollout_path: &Path,
) {
    match event {
        CodexLineEvent::SessionMeta(meta) => {
            let is_new_meta = watched.meta.as_ref() != Some(&meta);
            if !watched.added_to_aggregator {
                aggregator.add_session_with_context(
                    meta.session_id.clone(),
                    Tool::Codex,
                    &meta.cwd,
                    Status::Idle,
                    Some(rollout_path),
                );
                watched.added_to_aggregator = true;
                watched.last_status = Some(Status::Idle);
            }
            watched.last_activity_at = SystemTime::now();
            if is_new_meta {
                log_watcher_note(&format!(
                    "session {} mapped to {}",
                    meta.session_id,
                    meta.cwd.display()
                ));
            }
            watched.meta = Some(meta);
        }
        CodexLineEvent::Status(status) => {
            let Some(meta) = watched.meta.clone() else {
                return;
            };
            let status_changed = watched.last_status != Some(status);

            if !watched.added_to_aggregator {
                aggregator.add_session_with_context(
                    meta.session_id.clone(),
                    Tool::Codex,
                    &meta.cwd,
                    status,
                    Some(rollout_path),
                );
                watched.added_to_aggregator = true;
            } else {
                aggregator.update_session_status(&meta.session_id, status);
            }
            watched.last_status = Some(status);
            watched.last_activity_at = SystemTime::now();
            if status_changed {
                log_watcher_note(&format!(
                    "session {} -> {}",
                    meta.session_id,
                    status_name(status)
                ));
            }
        }
        CodexLineEvent::ToolCall(tool_call) => {
            if let Some(meta) = &watched.meta {
                aggregator.set_last_tool_call(&meta.session_id, tool_call.clone());
                log_watcher_note(&format!("session {} tool {}", meta.session_id, tool_call));
            }
            watched.last_activity_at = SystemTime::now();
        }
        CodexLineEvent::Ignore => {}
    }
}

fn update_inactive_session(
    aggregator: &StateAggregator,
    watched: &mut WatchedRollout,
    path: &Path,
) -> io::Result<()> {
    let Some(meta) = &watched.meta else {
        return Ok(());
    };

    let modified = rollout_modified_at(path)?;
    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return Ok(());
    };

    if watched.last_status == Some(Status::Working) && age >= STALE_WORKING_AFTER {
        aggregator.update_session_status(&meta.session_id, Status::Waiting);
        watched.last_status = Some(Status::Waiting);
        watched.last_activity_at = SystemTime::now();
        log_watcher_note(&format!(
            "marked stale Codex session {} as waiting after {}s without rollout updates",
            meta.session_id,
            age.as_secs()
        ));
    }

    let Ok(inactive_for) = SystemTime::now().duration_since(watched.last_activity_at) else {
        return Ok(());
    };

    if inactive_for >= REMOVE_INACTIVE_AFTER && watched.last_status != Some(Status::Working) {
        aggregator.remove_session(&meta.session_id);
        watched.added_to_aggregator = false;
        watched.last_status = None;
        watched.last_activity_at = SystemTime::now();
        log_watcher_note(&format!(
            "removed inactive Codex session {} after {}s without rollout events",
            meta.session_id,
            inactive_for.as_secs()
        ));
    }

    Ok(())
}

fn remove_missing_rollout(aggregator: &StateAggregator, path: &Path, watched: WatchedRollout) {
    if let Some(meta) = watched.meta {
        if watched.added_to_aggregator {
            aggregator.remove_session(&meta.session_id);
        }
        log_watcher_note(&format!(
            "stopped tracking missing rollout {} for session {}",
            path.display(),
            meta.session_id
        ));
    } else {
        log_watcher_note(&format!(
            "stopped tracking missing rollout {}",
            path.display()
        ));
    }
}

pub fn parse_codex_line(line: &str) -> serde_json::Result<CodexLineEvent> {
    let line = line.trim_start_matches('\u{feff}');

    if line.trim().is_empty() {
        return Ok(CodexLineEvent::Ignore);
    }

    let value: Value = serde_json::from_str(line)?;
    let line_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let payload = value.get("payload").unwrap_or(&Value::Null);

    match line_type {
        "session_meta" => {
            let session_id = payload
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let cwd = payload
                .get("cwd")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));

            Ok(CodexLineEvent::SessionMeta(CodexSessionMeta {
                session_id,
                cwd,
            }))
        }
        "event_msg" => {
            let event_type = payload
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();

            Ok(match event_type {
                "task_started" | "agent_message" => CodexLineEvent::Status(Status::Working),
                "task_complete" => CodexLineEvent::Status(Status::Done),
                "error" | "stream_error" | "turn_aborted" => {
                    CodexLineEvent::Status(Status::Waiting)
                }
                _ => CodexLineEvent::Ignore,
            })
        }
        "response_item" => {
            let payload_type = payload
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default();

            if payload_type == "function_call" {
                let tool_call = payload
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string();
                Ok(CodexLineEvent::ToolCall(tool_call))
            } else {
                Ok(CodexLineEvent::Ignore)
            }
        }
        _ => Ok(CodexLineEvent::Ignore),
    }
}

fn rollout_modified_at(path: &Path) -> io::Result<SystemTime> {
    if is_ssh_virtual_path(path) {
        rollout_modified(path).map_err(|error| io::Error::new(io::ErrorKind::Other, error))
    } else {
        fs::metadata(path)?.modified()
    }
}

fn initialize_existing_rollout_ssh(
    aggregator: &StateAggregator,
    path: &Path,
    watched: &mut WatchedRollout,
) -> io::Result<()> {
    let (content, new_offset) = read_rollout_from_offset(path, 0)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;
    let mut last_tool_call = None;

    for line in content.lines() {
        match parse_codex_line(line) {
            Ok(CodexLineEvent::SessionMeta(meta)) => watched.meta = Some(meta),
            Ok(CodexLineEvent::Status(status)) => watched.last_status = Some(status),
            Ok(CodexLineEvent::ToolCall(tool_call)) => last_tool_call = Some(tool_call),
            Ok(CodexLineEvent::Ignore) => {}
            Err(error) => log_watcher_error(&format!("parse {}", path.display()), &error),
        }
    }

    watched.offset = new_offset;
    watched.last_activity_at = rollout_modified_at(path).unwrap_or_else(|_| SystemTime::now());
    restore_existing_rollout(aggregator, path, watched, last_tool_call)
}

fn process_new_lines_ssh(
    aggregator: &StateAggregator,
    files: &mut HashMap<PathBuf, WatchedRollout>,
    path: &Path,
) -> io::Result<()> {
    let Some(watched) = files.get_mut(path) else {
        return Ok(());
    };

    let (chunk, new_offset) = read_rollout_from_offset(path, watched.offset)
        .map_err(|error| io::Error::new(io::ErrorKind::Other, error))?;

    if chunk.is_empty() {
        return Ok(());
    }

    let mut processable = chunk.as_str();
    let mut final_offset = new_offset;

    if !chunk.ends_with('\n') {
        let Some(last_newline) = chunk.rfind('\n') else {
            return Ok(());
        };
        processable = &chunk[..=last_newline];
        final_offset = watched.offset + processable.len() as u64;
    }

    for line in processable.lines() {
        match parse_codex_line(line) {
            Ok(event) => apply_codex_event(aggregator, watched, event, path),
            Err(error) => log_watcher_error(&format!("parse {}", path.display()), &error),
        }
    }

    watched.offset = final_offset;
    Ok(())
}

fn find_rollout_files(root: &Path, tracked_paths: &HashSet<PathBuf>) -> io::Result<Vec<PathBuf>> {
    if is_ssh_virtual_path(root) {
        return crate::ssh_remote::list_rollout_files(root)
            .map_err(|error| io::Error::new(io::ErrorKind::Other, error));
    }

    let mut files = Vec::new();

    if !root.exists() {
        return Ok(files);
    }

    collect_rollout_files(root, tracked_paths, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_rollout_files(
    dir: &Path,
    tracked_paths: &HashSet<PathBuf>,
    files: &mut Vec<PathBuf>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            collect_rollout_files(&path, tracked_paths, files)?;
        } else if file_type.is_file()
            && is_rollout_file(&path)
            && should_track_rollout_file(&path, tracked_paths)
        {
            files.push(path);
        }
    }

    Ok(())
}

fn should_track_rollout_file(path: &Path, tracked_paths: &HashSet<PathBuf>) -> bool {
    if tracked_paths.contains(path) {
        return true;
    }

    let Ok(modified) = fs::metadata(path).and_then(|metadata| metadata.modified()) else {
        return true;
    };

    let Ok(age) = SystemTime::now().duration_since(modified) else {
        return true;
    };

    age < REMOVE_INACTIVE_AFTER
}

fn is_rollout_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    file_name.starts_with("rollout-") && file_name.ends_with(".jsonl")
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Idle => "idle",
        Status::Done => "done",
        Status::Working => "working",
        Status::Waiting => "waiting",
    }
}

fn log_root_summary(prefix: &str, roots: &CodexSessionRootSummary) {
    let message = format!(
        "{prefix}: active=[{}] manual=[{}] missing=[{}]",
        crate::codex_paths::format_paths(&roots.active),
        crate::codex_paths::format_paths(&roots.manual),
        crate::codex_paths::format_paths(&roots.missing)
    );

    if roots.active.is_empty() {
        log_warn("codex_watcher", message);
    } else {
        log_info("codex_watcher", message);
    }
}

fn log_watcher_error(context: &str, error: &dyn std::fmt::Display) {
    log_error("codex_watcher", format!("{context}: {error}"));
}

fn log_watcher_note(message: &str) {
    log_info("codex_watcher", message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_codex_status_events() {
        assert_eq!(
            parse_codex_line(r#"{"type":"event_msg","payload":{"type":"task_started"}}"#).unwrap(),
            CodexLineEvent::Status(Status::Working)
        );
        assert_eq!(
            parse_codex_line(r#"{"type":"event_msg","payload":{"type":"task_complete"}}"#).unwrap(),
            CodexLineEvent::Status(Status::Done)
        );
        assert_eq!(
            parse_codex_line(r#"{"type":"event_msg","payload":{"type":"stream_error"}}"#).unwrap(),
            CodexLineEvent::Status(Status::Waiting)
        );
    }

    #[test]
    fn parses_codex_session_meta_and_tool_call() {
        assert_eq!(
            parse_codex_line(
                r#"{"type":"session_meta","payload":{"id":"s1","cwd":"N:\\AI\\ai_light"}}"#,
            )
            .unwrap(),
            CodexLineEvent::SessionMeta(CodexSessionMeta {
                session_id: "s1".to_string(),
                cwd: PathBuf::from(r"N:\AI\ai_light"),
            })
        );

        assert_eq!(
            parse_codex_line(
                r#"{"type":"response_item","payload":{"type":"function_call","name":"shell_command"}}"#,
            )
            .unwrap(),
            CodexLineEvent::ToolCall("shell_command".to_string())
        );
    }

    #[test]
    fn polling_new_rollout_drives_codex_light() {
        let root = std::env::temp_dir().join(unique_name("ai-light-codex-root"));
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        let day = root.join("2026").join("05").join("31");
        fs::create_dir_all(&day).unwrap();
        fs::create_dir_all(&project).unwrap();

        let rollout = day.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        fs::write(
            &rollout,
            format!(
                "{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                json_line("event_msg", r#"{"type":"task_started"}"#)
            ),
        )
        .unwrap();

        let aggregator = StateAggregator::new();
        let mut files = HashMap::new();
        poll_rollout_root(&aggregator, &mut files, false, &root).unwrap();

        let lights = aggregator.get_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].status, Status::Working);
        assert_eq!(lights[0].sessions[0].tool, Tool::Codex);

        fs::write(
            &rollout,
            format!(
                "{}\n{}\n{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                json_line("event_msg", r#"{"type":"task_started"}"#),
                json_line(
                    "response_item",
                    r#"{"type":"function_call","name":"apply_patch"}"#
                ),
                json_line("event_msg", r#"{"type":"task_complete"}"#)
            ),
        )
        .unwrap();

        poll_rollout_root(&aggregator, &mut files, false, &root).unwrap();
        let lights = aggregator.get_lights();
        assert_eq!(lights[0].status, Status::Done);
        assert_eq!(lights[0].last_tool_call.as_deref(), Some("apply_patch"));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn baseline_existing_rollout_restores_recent_state() {
        let root = std::env::temp_dir().join(unique_name("ai-light-codex-root"));
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&project).unwrap();

        let rollout = root.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        fs::write(
            &rollout,
            format!(
                "{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                json_line("event_msg", r#"{"type":"task_started"}"#)
            ),
        )
        .unwrap();

        let aggregator = StateAggregator::new();
        let mut files = HashMap::new();
        poll_rollout_root(&aggregator, &mut files, true, &root).unwrap();

        let lights = aggregator.get_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].status, Status::Working);

        fs::write(
            &rollout,
            format!(
                "{}\n{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                json_line("event_msg", r#"{"type":"task_started"}"#),
                json_line("event_msg", r#"{"type":"task_complete"}"#)
            ),
        )
        .unwrap();

        poll_rollout_root(&aggregator, &mut files, false, &root).unwrap();

        let lights = aggregator.get_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].status, Status::Done);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn baseline_existing_rollouts_group_multiple_sessions_by_project() {
        let root = std::env::temp_dir().join(unique_name("ai-light-codex-root"));
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&project).unwrap();

        let first_rollout = root.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        let second_rollout = root.join("rollout-2026-05-31T00-05-00-s2.jsonl");

        for (path, session_id, event_type) in [
            (&first_rollout, "s1", "task_started"),
            (&second_rollout, "s2", "task_complete"),
        ] {
            fs::write(
                path,
                format!(
                    "{}\n{}\n",
                    json_line(
                        "session_meta",
                        &format!(r#"{{"id":"{session_id}","cwd":"{}"}}"#, json_path(&project))
                    ),
                    json_line("event_msg", &format!(r#"{{"type":"{event_type}"}}"#))
                ),
            )
            .unwrap();
        }

        let aggregator = StateAggregator::new();
        let mut files = HashMap::new();
        poll_rollout_root(&aggregator, &mut files, true, &root).unwrap();

        let lights = aggregator.get_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].sessions.len(), 2);
        assert_eq!(lights[0].status, Status::Working);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn baseline_existing_stale_completed_rollout_is_not_restored() {
        let root = std::env::temp_dir().join(unique_name("ai-light-codex-root"));
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&project).unwrap();

        let rollout = root.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        fs::write(
            &rollout,
            format!(
                "{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                json_line("event_msg", r#"{"type":"task_complete"}"#)
            ),
        )
        .unwrap();

        let old_time = filetime::FileTime::from_system_time(
            SystemTime::now() - REMOVE_INACTIVE_AFTER - Duration::from_secs(1),
        );
        filetime::set_file_mtime(&rollout, old_time).unwrap();

        let aggregator = StateAggregator::new();
        let mut files = HashMap::new();
        poll_rollout_root(&aggregator, &mut files, true, &root).unwrap();

        assert!(aggregator.get_lights().is_empty());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn baseline_existing_stale_working_rollout_is_not_restored() {
        let root = std::env::temp_dir().join(unique_name("ai-light-codex-root"));
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&project).unwrap();

        let rollout = root.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        fs::write(
            &rollout,
            format!(
                "{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                json_line("event_msg", r#"{"type":"task_started"}"#)
            ),
        )
        .unwrap();

        let old_time = filetime::FileTime::from_system_time(
            SystemTime::now() - REMOVE_INACTIVE_AFTER - Duration::from_secs(1),
        );
        filetime::set_file_mtime(&rollout, old_time).unwrap();

        let aggregator = StateAggregator::new();
        let mut files = HashMap::new();
        poll_rollout_root(&aggregator, &mut files, true, &root).unwrap();

        assert!(aggregator.get_lights().is_empty());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn polling_multiple_roots_restores_sessions_from_each_root() {
        let first_root = std::env::temp_dir().join(unique_name("ai-light-codex-root-a"));
        let second_root = std::env::temp_dir().join(unique_name("ai-light-codex-root-b"));
        let first_project = std::env::temp_dir().join(unique_name("ai-light-codex-project-a"));
        let second_project = std::env::temp_dir().join(unique_name("ai-light-codex-project-b"));
        fs::create_dir_all(&first_root).unwrap();
        fs::create_dir_all(&second_root).unwrap();
        fs::create_dir_all(&first_project).unwrap();
        fs::create_dir_all(&second_project).unwrap();

        fs::write(
            first_root.join("rollout-2026-05-31T00-00-00-s1.jsonl"),
            format!(
                "{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&first_project))
                ),
                json_line("event_msg", r#"{"type":"task_started"}"#)
            ),
        )
        .unwrap();

        fs::write(
            second_root.join("rollout-2026-05-31T00-01-00-s2.jsonl"),
            format!(
                "{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s2","cwd":"{}"}}"#, json_path(&second_project))
                ),
                json_line("event_msg", r#"{"type":"task_complete"}"#)
            ),
        )
        .unwrap();

        let aggregator = StateAggregator::new();
        let mut files = HashMap::new();
        let roots = vec![first_root.clone(), second_root.clone()];
        poll_codex_sessions(&aggregator, &mut files, true, &roots, &[]).unwrap();

        let lights = aggregator.get_lights();
        assert_eq!(lights.len(), 2);
        assert!(lights
            .iter()
            .any(|light| light.logical_project_id == first_project));
        assert!(lights
            .iter()
            .any(|light| light.logical_project_id == second_project));

        let _ = fs::remove_dir_all(first_root);
        let _ = fs::remove_dir_all(second_root);
        let _ = fs::remove_dir_all(first_project);
        let _ = fs::remove_dir_all(second_project);
    }

    #[test]
    fn removing_rollout_file_removes_session_from_aggregator() {
        let root = std::env::temp_dir().join(unique_name("ai-light-codex-root"));
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&project).unwrap();

        let rollout = root.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        fs::write(
            &rollout,
            format!(
                "{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                json_line("event_msg", r#"{"type":"task_started"}"#)
            ),
        )
        .unwrap();

        let aggregator = StateAggregator::new();
        let mut files = HashMap::new();
        poll_rollout_root(&aggregator, &mut files, false, &root).unwrap();
        assert_eq!(aggregator.get_lights().len(), 1);

        fs::remove_file(&rollout).unwrap();
        poll_rollout_root(&aggregator, &mut files, false, &root).unwrap();
        assert!(aggregator.get_lights().is_empty());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn initial_scan_skips_old_untracked_rollouts_but_keeps_tracked_ones() {
        let root = std::env::temp_dir().join(unique_name("ai-light-codex-root"));
        fs::create_dir_all(&root).unwrap();

        let stale = root.join("rollout-2026-05-31T00-00-00-stale.jsonl");
        let tracked = root.join("rollout-2026-05-31T00-00-00-tracked.jsonl");
        let fresh = root.join("rollout-2026-05-31T00-00-00-fresh.jsonl");

        for path in [&stale, &tracked, &fresh] {
            fs::write(path, "").unwrap();
        }

        let old_time = filetime::FileTime::from_system_time(
            SystemTime::now() - REMOVE_INACTIVE_AFTER - Duration::from_secs(1),
        );
        filetime::set_file_mtime(&stale, old_time).unwrap();
        filetime::set_file_mtime(&tracked, old_time).unwrap();

        let tracked_paths = HashSet::from([tracked.clone()]);
        let rollouts = find_rollout_files(&root, &tracked_paths).unwrap();

        assert!(rollouts.contains(&fresh));
        assert!(rollouts.contains(&tracked));
        assert!(!rollouts.contains(&stale));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_json_line_is_retried_on_next_poll() {
        let root = std::env::temp_dir().join(unique_name("ai-light-codex-root"));
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&project).unwrap();

        let rollout = root.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        fs::write(
            &rollout,
            format!(
                "{}\n{}",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                r#"{"type":"event_msg","payload":{"type":"task_started"}}"#
            ),
        )
        .unwrap();

        let aggregator = StateAggregator::new();
        let mut files = HashMap::new();
        poll_rollout_root(&aggregator, &mut files, false, &root).unwrap();

        let lights = aggregator.get_lights();
        assert_eq!(lights.len(), 1);
        assert_eq!(lights[0].status, Status::Idle);

        fs::write(
            &rollout,
            format!(
                "{}\n{}\n",
                json_line(
                    "session_meta",
                    &format!(r#"{{"id":"s1","cwd":"{}"}}"#, json_path(&project))
                ),
                r#"{"type":"event_msg","payload":{"type":"task_started"}}"#
            ),
        )
        .unwrap();

        poll_rollout_root(&aggregator, &mut files, false, &root).unwrap();
        assert_eq!(aggregator.get_lights()[0].status, Status::Working);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn stale_working_session_is_marked_error() {
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        fs::create_dir_all(&project).unwrap();
        let rollout = project.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        fs::write(&rollout, "").unwrap();

        let aggregator = StateAggregator::new();
        let mut watched = WatchedRollout {
            offset: 0,
            meta: Some(CodexSessionMeta {
                session_id: "s1".to_string(),
                cwd: project.clone(),
            }),
            added_to_aggregator: true,
            last_status: Some(Status::Working),
            last_activity_at: SystemTime::now(),
        };
        aggregator.add_session("s1".to_string(), Tool::Codex, &project, Status::Working);

        update_inactive_session(&aggregator, &mut watched, &rollout).unwrap();
        assert_eq!(aggregator.get_lights()[0].status, Status::Working);

        let old_time = filetime::FileTime::from_system_time(
            SystemTime::now() - STALE_WORKING_AFTER - Duration::from_secs(1),
        );
        filetime::set_file_mtime(&rollout, old_time).unwrap();

        update_inactive_session(&aggregator, &mut watched, &rollout).unwrap();
        assert_eq!(aggregator.get_lights()[0].status, Status::Waiting);
        assert_eq!(watched.last_status, Some(Status::Waiting));

        let _ = fs::remove_dir_all(project);
    }

    #[test]
    fn inactive_done_session_is_removed() {
        let project = std::env::temp_dir().join(unique_name("ai-light-codex-project"));
        fs::create_dir_all(&project).unwrap();
        let rollout = project.join("rollout-2026-05-31T00-00-00-s1.jsonl");
        fs::write(&rollout, "").unwrap();

        let aggregator = StateAggregator::new();
        let mut watched = WatchedRollout {
            offset: 0,
            meta: Some(CodexSessionMeta {
                session_id: "s1".to_string(),
                cwd: project.clone(),
            }),
            added_to_aggregator: true,
            last_status: Some(Status::Done),
            last_activity_at: SystemTime::now() - REMOVE_INACTIVE_AFTER - Duration::from_secs(1),
        };
        aggregator.add_session("s1".to_string(), Tool::Codex, &project, Status::Done);

        update_inactive_session(&aggregator, &mut watched, &rollout).unwrap();

        assert!(aggregator.get_lights().is_empty());
        assert!(!watched.added_to_aggregator);
        assert_eq!(watched.last_status, None);

        let _ = fs::remove_dir_all(project);
    }

    fn json_line(line_type: &str, payload: &str) -> String {
        format!(r#"{{"type":"{line_type}","payload":{payload}}}"#)
    }

    fn json_path(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "\\\\")
    }

    fn unique_name(prefix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        format!("{prefix}-{nanos}")
    }
}
