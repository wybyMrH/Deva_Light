use crate::aggregator::StateAggregator;
use crate::logging::{log_info, log_warn};
use crate::types::{Status, Tool};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const STALE_WORKING_AFTER: Duration = Duration::from_secs(5 * 60);

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
    cwd: PathBuf,
    pid: i32,
    file_name: String,
    last_seen_alive: std::time::Instant,
    status: Status,
}

pub fn start_claude_watcher(aggregator: Arc<StateAggregator>) {
    thread::Builder::new()
        .name("ai-light-claude-watcher".to_string())
        .spawn(move || run_claude_watcher(aggregator))
        .expect("failed to spawn claude watcher thread");
}

fn run_claude_watcher(aggregator: Arc<StateAggregator>) {
    let mut tracked: HashMap<String, TrackedSession> = HashMap::new();

    // Initial scan for existing sessions
    let sessions_dir = claude_sessions_dir();
    log_info(
        "claude_watcher",
        format!("scanning Claude sessions at {}", sessions_dir.display()),
    );

    if let Ok(entries) = scan_session_files(&sessions_dir) {
        for (file_name, session) in entries {
            if is_process_alive(session.pid) {
                let tracked_session = TrackedSession {
                    session_id: session.session_id.clone(),
                    cwd: PathBuf::from(&session.cwd),
                    pid: session.pid,
                    file_name,
                    last_seen_alive: std::time::Instant::now(),
                    status: Status::Working,
                };
                log_info(
                    "claude_watcher",
                    format!(
                        "restored existing Claude session {} (pid={}) at {}",
                        session.session_id, session.pid, session.cwd
                    ),
                );
                aggregator.add_session(
                    session.session_id.clone(),
                    Tool::ClaudeCode,
                    &tracked_session.cwd,
                    Status::Working,
                );
                tracked.insert(session.session_id, tracked_session);
            }
        }
    }

    loop {
        // Poll for new/changed session files
        if let Ok(entries) = scan_session_files(&sessions_dir) {
            let mut seen_ids = std::collections::HashSet::new();

            for (_file_name, session) in entries {
                seen_ids.insert(session.session_id.clone());

                if let Some(existing) = tracked.get_mut(&session.session_id) {
                    // Update pid if it changed (session reused same ID)
                    if existing.pid != session.pid {
                        existing.pid = session.pid;
                        existing.last_seen_alive = std::time::Instant::now();
                    }
                } else if is_process_alive(session.pid) {
                    // New session discovered
                    let tracked_session = TrackedSession {
                        session_id: session.session_id.clone(),
                        cwd: PathBuf::from(&session.cwd),
                        pid: session.pid,
                        file_name: String::new(),
                        last_seen_alive: std::time::Instant::now(),
                        status: Status::Working,
                    };
                    log_info(
                        "claude_watcher",
                        format!(
                            "discovered new Claude session {} (pid={}) at {}",
                            session.session_id, session.pid, session.cwd
                        ),
                    );
                    aggregator.add_session(
                        session.session_id.clone(),
                        Tool::ClaudeCode,
                        &tracked_session.cwd,
                        Status::Working,
                    );
                    tracked.insert(session.session_id, tracked_session);
                }
            }

            // Check alive status and handle dead sessions
            let dead_sessions: Vec<String> = tracked
                .iter()
                .filter(|(id, session)| {
                    // If the session file is gone, the session ended cleanly
                    let file_exists = sessions_dir.join(&session.file_name).exists();
                    if !file_exists {
                        return true;
                    }
                    !seen_ids.contains(*id) || !is_process_alive(session.pid)
                })
                .map(|(id, _)| id.clone())
                .collect();

            for session_id in dead_sessions {
                if let Some(session) = tracked.remove(&session_id) {
                    log_info(
                        "claude_watcher",
                        format!(
                            "Claude session {} (pid={}) terminated, marking as done",
                            session.session_id, session.pid
                        ),
                    );
                    // The hook may have already handled stop/end events,
                    // so only update if it's still in Working/Waiting status
                    if let Some(current) = aggregator.session_status(&session_id) {
                        if current != Status::Done {
                            aggregator.update_session_status(&session_id, Status::Done);
                        }
                    }
                }
            }
        }

        // Check for stale working sessions
        let now = std::time::Instant::now();
        for session in tracked.values_mut() {
            if is_process_alive(session.pid) {
                session.last_seen_alive = now;
            }

            let elapsed = now.duration_since(session.last_seen_alive);
            if session.status == Status::Working && elapsed >= STALE_WORKING_AFTER {
                log_warn(
                    "claude_watcher",
                    format!(
                        "Claude session {} (pid={}) stale for {}s, marking as waiting",
                        session.session_id,
                        session.pid,
                        elapsed.as_secs()
                    ),
                );
                session.status = Status::Waiting;
                aggregator.update_session_status(&session.session_id, Status::Waiting);
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

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn scan_session_files(
    dir: &Path,
) -> Result<Vec<(String, ClaudeSessionFile)>, std::io::Error> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.extension().is_some_and(|ext| ext == "json") {
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
        // Use raw kernel32 FFI to check if a process is still running
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = (-1isize) as *mut std::ffi::c_void;

        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(
                dwDesiredAccess: u32,
                bInheritHandle: i32,
                dwProcessId: u32,
            ) -> *mut std::ffi::c_void;
            fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
        }

        let handle = unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32)
        };

        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            return false;
        }

        unsafe { CloseHandle(handle) };
        true
    }
}
