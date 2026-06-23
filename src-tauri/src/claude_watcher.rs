use crate::aggregator::StateAggregator;
use crate::logging::log_info;
use crate::monitoring::is_monitoring_paused;
use crate::types::{Status, Tool};
#[cfg(target_os = "windows")]
use crate::wsl_paths::windows_wsl_claude_sessions_dirs;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Debug, Deserialize)]
struct ClaudeSessionFile {
    pid: i32,
    #[serde(rename = "sessionId")]
    session_id: String,
    cwd: String,
}

#[derive(Debug, Clone)]
struct TrackedSession {
    session_id: String,
    pid: i32,
    file_name: String,
}

pub fn start_claude_watcher(aggregator: Arc<StateAggregator>) {
    thread::Builder::new()
        .name("ai-light-claude-watcher".to_string())
        .spawn(move || run_claude_watcher(aggregator))
        .expect("failed to spawn claude watcher thread");
}

fn run_claude_watcher(aggregator: Arc<StateAggregator>) {
    let mut tracked: HashMap<String, TrackedSession> = HashMap::new();
    let mut previous_session_files: HashMap<String, String> = HashMap::new();

    log_info(
        "claude_watcher",
        format!(
            "scanning Claude sessions at {}",
            claude_sessions_dirs()
                .into_iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    );

    loop {
        if is_monitoring_paused() {
            thread::sleep(POLL_INTERVAL);
            continue;
        }

        if let Ok(entries) = scan_session_files(&claude_sessions_dirs()) {
            let mut seen_file_names: HashMap<String, bool> = HashMap::new();
            let mut current_session_files: HashMap<String, String> = HashMap::new();

            for (file_name, session) in &entries {
                seen_file_names.insert(file_name.clone(), true);
                current_session_files.insert(session.session_id.clone(), file_name.clone());

                if let Some(current_status) = aggregator.session_status(&session.session_id) {
                    if current_status == Status::Working && !is_process_alive(session.pid) {
                        log_info(
                            "claude_watcher",
                            format!(
                                "Claude session {} (pid={}) process ended; marking Done",
                                session.session_id, session.pid
                            ),
                        );
                        aggregator.update_session_status(&session.session_id, Status::Done);
                    }

                    if let Some(existing) = tracked.get_mut(&session.session_id) {
                        existing.pid = session.pid;
                        existing.file_name = file_name.clone();
                    }
                    continue;
                }

                if tracked.contains_key(&session.session_id) {
                    if let Some(existing) = tracked.get_mut(&session.session_id) {
                        existing.pid = session.pid;
                        existing.file_name = file_name.clone();
                    }
                    if aggregator.session_status(&session.session_id).is_none()
                        && is_process_alive(session.pid)
                    {
                        log_info(
                            "claude_watcher",
                            format!(
                                "re-restored Claude session {} (pid={}) at {}",
                                session.session_id, session.pid, session.cwd
                            ),
                        );
                        let cwd = PathBuf::from(&session.cwd);
                        aggregator.add_session(
                            session.session_id.clone(),
                            Tool::ClaudeCode,
                            &cwd,
                            Status::Working,
                        );
                    }
                    continue;
                }

                // New session not tracked by hooks or us
                if is_process_alive(session.pid) {
                    log_info(
                        "claude_watcher",
                        format!(
                            "restored Claude session {} (pid={}) at {}",
                            session.session_id, session.pid, session.cwd
                        ),
                    );
                    let cwd = PathBuf::from(&session.cwd);
                    aggregator.add_session(
                        session.session_id.clone(),
                        Tool::ClaudeCode,
                        &cwd,
                        Status::Working,
                    );
                    tracked.insert(
                        session.session_id.clone(),
                        TrackedSession {
                            session_id: session.session_id.clone(),
                            pid: session.pid,
                            file_name: file_name.clone(),
                        },
                    );
                }
            }

            for session_id in previous_session_files.keys() {
                if current_session_files.contains_key(session_id) {
                    continue;
                }

                if aggregator.session_status(session_id) == Some(Status::Working) {
                    log_info(
                        "claude_watcher",
                        format!("Claude session {session_id} file removed; marking Done"),
                    );
                    aggregator.update_session_status(session_id, Status::Done);
                }
            }

            previous_session_files = current_session_files;

            // Handle dead sessions: file removed or process dead
            let dead_ids: Vec<String> = tracked
                .iter()
                .filter(|(_id, session)| {
                    // Session file removed → ended cleanly
                    if !seen_file_names.contains_key(&session.file_name) {
                        return true;
                    }
                    // Process no longer alive
                    !is_process_alive(session.pid)
                })
                .map(|(id, _)| id.clone())
                .collect();

            for session_id in dead_ids {
                if let Some(session) = tracked.remove(&session_id) {
                    log_info(
                        "claude_watcher",
                        format!(
                            "Claude session {} (pid={}) terminated",
                            session.session_id, session.pid
                        ),
                    );
                    // Only mark Done if hooks haven't already handled it
                    if let Some(current) = aggregator.session_status(&session_id) {
                        if current != Status::Done {
                            aggregator.update_session_status(&session_id, Status::Done);
                        }
                    }
                }
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn claude_sessions_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("sessions")
}

fn claude_sessions_dirs() -> Vec<PathBuf> {
    #[cfg(not(target_os = "windows"))]
    let dirs = vec![claude_sessions_dir()];

    #[cfg(target_os = "windows")]
    let mut dirs = vec![claude_sessions_dir()];

    #[cfg(target_os = "windows")]
    for path in windows_wsl_claude_sessions_dirs() {
        if !dirs.iter().any(|existing| existing == &path) {
            dirs.push(path);
        }
    }

    dirs
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

pub(crate) fn live_claude_session_ids() -> HashSet<String> {
    let Ok(entries) = scan_session_files(&claude_sessions_dirs()) else {
        return HashSet::new();
    };

    entries
        .into_iter()
        .map(|(_file_name, session)| session.session_id)
        .collect()
}

/// Claude sessions seen on disk with their working directory, for proactive
/// discovery when the watcher poll or hooks haven't registered a new session.
pub(crate) fn discover_claude_sessions() -> Vec<(String, PathBuf)> {
    let Ok(entries) = scan_session_files(&claude_sessions_dirs()) else {
        return Vec::new();
    };
    entries
        .into_iter()
        .map(|(_file_name, session)| (session.session_id, PathBuf::from(&session.cwd)))
        .collect()
}

pub(crate) fn claude_session_process_alive(session_id: &str) -> Option<bool> {
    let Ok(entries) = scan_session_files(&claude_sessions_dirs()) else {
        return None;
    };

    entries
        .into_iter()
        .find(|(_file_name, session)| session.session_id == session_id)
        .map(|(_file_name, session)| is_process_alive(session.pid))
}

fn scan_session_files(
    dirs: &[PathBuf],
) -> Result<Vec<(String, ClaudeSessionFile)>, std::io::Error> {
    let mut results = Vec::new();

    for dir in dirs {
        if !dir.exists() {
            continue;
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }

            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let session: ClaudeSessionFile = match serde_json::from_str(&content) {
                Ok(s) => s,
                Err(_) => continue,
            };

            results.push((file_name, session));
        }
    }

    Ok(results)
}

fn is_process_alive(pid: i32) -> bool {
    if pid <= 0 {
        return false;
    }

    #[cfg(unix)]
    {
        unsafe { libc::kill(pid, 0) == 0 }
    }

    #[cfg(windows)]
    {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(
                dwDesiredAccess: u32,
                bInheritHandle: i32,
                dwProcessId: u32,
            ) -> *mut std::ffi::c_void;
            fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
        }

        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32) };

        if handle.is_null() {
            return false;
        }

        unsafe { CloseHandle(handle) };
        true
    }
}
