use crate::aggregator::StateAggregator;
use crate::logging::log_info;
use crate::monitoring::is_monitoring_paused;
use crate::types::{Status, Tool};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime};

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const STALE_WORKING_AFTER: Duration = Duration::from_secs(10 * 60);
const REMOVE_INACTIVE_AFTER: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone)]
struct TrackedCursorSession {
    session_id: String,
    cwd: PathBuf,
    last_activity_at: SystemTime,
}

pub fn start_cursor_watcher(aggregator: Arc<StateAggregator>) {
    thread::Builder::new()
        .name("ai-light-cursor-watcher".to_string())
        .spawn(move || run_cursor_watcher(aggregator))
        .expect("failed to spawn cursor watcher thread");
}

fn run_cursor_watcher(aggregator: Arc<StateAggregator>) {
    let projects_dir = cursor_projects_dir();
    log_info(
        "cursor_watcher",
        format!("scanning Cursor projects at {}", projects_dir.display()),
    );

    let mut tracked: HashMap<String, TrackedCursorSession> = HashMap::new();

    loop {
        if is_monitoring_paused() {
            thread::sleep(POLL_INTERVAL);
            continue;
        }

        if let Ok(entries) = scan_cursor_sessions(&projects_dir) {
            let now = SystemTime::now();
            let mut seen = HashMap::new();

            for entry in entries {
                seen.insert(entry.session_id.clone(), true);

                if aggregator.session_status(&entry.session_id).is_some() {
                    if let Some(existing) = tracked.get_mut(&entry.session_id) {
                        existing.last_activity_at = entry.last_activity_at;
                    }
                    continue;
                }

                if tracked.contains_key(&entry.session_id) {
                    if let Some(existing) = tracked.get_mut(&entry.session_id) {
                        existing.last_activity_at = entry.last_activity_at;
                    }
                    continue;
                }

                if is_recently_active(entry.last_activity_at, now) {
                    log_info(
                        "cursor_watcher",
                        format!(
                            "restored Cursor session {} at {}",
                            entry.session_id,
                            entry.cwd.display()
                        ),
                    );
                    aggregator.add_session(
                        entry.session_id.clone(),
                        Tool::Cursor,
                        &entry.cwd,
                        Status::Working,
                    );
                    tracked.insert(
                        entry.session_id.clone(),
                        TrackedCursorSession {
                            session_id: entry.session_id.clone(),
                            cwd: entry.cwd.clone(),
                            last_activity_at: entry.last_activity_at,
                        },
                    );
                }
            }

            let stale_ids: Vec<String> = tracked
                .iter()
                .filter_map(|(session_id, session)| {
                    if seen.contains_key(session_id) {
                        return None;
                    }

                    let inactive_for = now
                        .duration_since(session.last_activity_at)
                        .unwrap_or(REMOVE_INACTIVE_AFTER);

                    if inactive_for >= REMOVE_INACTIVE_AFTER {
                        Some(session_id.clone())
                    } else if inactive_for >= STALE_WORKING_AFTER
                        && aggregator.session_status(session_id) == Some(Status::Working)
                    {
                        aggregator.update_session_status(session_id, Status::Waiting);
                        None
                    } else {
                        None
                    }
                })
                .collect();

            for session_id in stale_ids {
                if let Some(session) = tracked.remove(&session_id) {
                    log_info(
                        "cursor_watcher",
                        format!(
                            "Cursor session {} inactive; marking Done",
                            session.session_id
                        ),
                    );
                    if aggregator.session_status(&session_id) != Some(Status::Done) {
                        aggregator.update_session_status(&session_id, Status::Done);
                    }
                }
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}

#[derive(Debug, Clone)]
struct CursorSessionEntry {
    session_id: String,
    cwd: PathBuf,
    last_activity_at: SystemTime,
}

fn scan_cursor_sessions(projects_dir: &Path) -> Result<Vec<CursorSessionEntry>, std::io::Error> {
    if !projects_dir.exists() {
        return Ok(Vec::new());
    }

    let mut results = Vec::new();

    for project_entry in fs::read_dir(projects_dir)? {
        let project_entry = project_entry?;
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        let Some(cwd) = project_path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(decode_cursor_project_slug)
        else {
            continue;
        };

        let transcripts_dir = project_path.join("agent-transcripts");
        if !transcripts_dir.exists() {
            continue;
        }

        for session_entry in fs::read_dir(&transcripts_dir)? {
            let session_entry = session_entry?;
            let session_path = session_entry.path();
            if !session_path.is_dir() {
                continue;
            }

            let Some(session_id) = session_path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            let Some(last_activity_at) = latest_mtime_in_dir(&session_path) else {
                continue;
            };

            results.push(CursorSessionEntry {
                session_id: session_id.to_string(),
                cwd: cwd.clone(),
                last_activity_at,
            });
        }
    }

    Ok(results)
}

fn latest_mtime_in_dir(dir: &Path) -> Option<SystemTime> {
    let mut latest: Option<SystemTime> = None;

    let mut update = |path: &Path| {
        if let Ok(metadata) = fs::metadata(path) {
            if let Ok(modified) = metadata.modified() {
                latest = Some(match latest {
                    Some(current) => current.max(modified),
                    None => modified,
                });
            }
        }
    };

    update(dir);

    let transcript_file = dir.join(format!(
        "{}.jsonl",
        dir.file_name().and_then(|name| name.to_str()).unwrap_or("")
    ));
    update(&transcript_file);

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                update(&path);
            } else if path.is_dir() {
                if let Ok(sub_entries) = fs::read_dir(&path) {
                    for sub_entry in sub_entries.flatten() {
                        update(&sub_entry.path());
                    }
                }
            }
        }
    }

    latest
}

fn is_recently_active(last_activity_at: SystemTime, now: SystemTime) -> bool {
    now.duration_since(last_activity_at)
        .map(|elapsed| elapsed < REMOVE_INACTIVE_AFTER)
        .unwrap_or(false)
}

fn cursor_projects_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cursor")
        .join("projects")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Decode Cursor project folder slug to filesystem path.
/// Examples: `mnt-e-code-Python-AI-Light` → `/mnt/e/code/Python/AI-Light`
pub fn decode_cursor_project_slug(slug: &str) -> Option<PathBuf> {
    if slug.starts_with("tmp-") || slug.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    if let Some(rest) = slug.strip_prefix("mnt-") {
        let segments: Vec<&str> = rest.split('-').collect();
        if segments.len() < 2 {
            return None;
        }
        let base = format!("/mnt/{}", segments[0]);
        if let Some(path) = resolve_path_from_segments(&segments[1..], &base) {
            return Some(path);
        }
        return Some(PathBuf::from(format!("{}/{}", base, segments[1..].join("/"))));
    }

    if slug.len() >= 3 {
        let drive = slug.chars().next()?;
        if drive.is_ascii_alphabetic() && slug.as_bytes().get(1) == Some(&b'-') {
            let segments: Vec<&str> = slug[2..].split('-').collect();
            let base = format!("{}:", drive);
            if let Some(path) = resolve_path_from_segments(&segments, &base) {
                return Some(path);
            }
            return Some(PathBuf::from(format!("{}/{}", base, segments.join("/"))));
        }
    }

    None
}

fn resolve_path_from_segments(segments: &[&str], base: &str) -> Option<PathBuf> {
    if segments.is_empty() {
        let path = PathBuf::from(base);
        if path.exists() {
            return Some(path);
        }
        return underscore_variant(&path).filter(|candidate| candidate.exists());
    }

    for end in 1..=segments.len() {
        let component = segments[..end].join("-");
        let Some(candidate) = join_project_segment(base, &component) else {
            continue;
        };

        if let Some(path) = resolve_path_from_segments(&segments[end..], &candidate) {
            return Some(path);
        }
    }

    None
}

fn join_project_segment(base: &str, component: &str) -> Option<String> {
    let primary = if base.ends_with(':') {
        format!("{}/{}", base, component)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), component)
    };

    if Path::new(&primary).exists() {
        return Some(primary);
    }

    underscore_variant(&PathBuf::from(&primary))
        .filter(|candidate| candidate.exists())
        .map(|path| path.to_string_lossy().to_string())
}

fn underscore_variant(path: &Path) -> Option<PathBuf> {
    let file_name = path.file_name()?.to_string_lossy();
    if !file_name.contains('-') {
        return None;
    }

    let mut variant = path.to_path_buf();
    variant.set_file_name(file_name.replace('-', "_"));
    Some(variant)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_wsl_style_cursor_project_slug() {
        assert_eq!(
            decode_cursor_project_slug("mnt-e-code-Python-AI-Light"),
            Some(PathBuf::from("/mnt/e/code/Python/AI_Light"))
        );
        assert_eq!(
            decode_cursor_project_slug("mnt-d-code-Python-searxng-master"),
            Some(PathBuf::from("/mnt/d/code/Python/searxng-master"))
        );
    }

    #[test]
    fn ignores_temp_and_numeric_cursor_project_slugs() {
        assert_eq!(decode_cursor_project_slug("tmp-66a3e247"), None);
        assert_eq!(decode_cursor_project_slug("1780589580426"), None);
    }
}
