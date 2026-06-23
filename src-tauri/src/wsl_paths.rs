#[cfg(target_os = "windows")]
use crate::codex_paths::{parse_wsl_distro_list, path_from_console_output, run_wsl_command};
use std::path::PathBuf;

#[cfg(target_os = "windows")]
use std::sync::{LazyLock, Mutex};
#[cfg(target_os = "windows")]
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
const WSL_PATH_CACHE_TTL: Duration = Duration::from_secs(60);

#[cfg(target_os = "windows")]
static CLAUDE_SESSIONS_CACHE: LazyLock<Mutex<Option<(Instant, Vec<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(None));
#[cfg(target_os = "windows")]
static CURSOR_PROJECTS_CACHE: LazyLock<Mutex<Option<(Instant, Vec<PathBuf>)>>> =
    LazyLock::new(|| Mutex::new(None));

#[cfg(target_os = "windows")]
fn wsl_path_for_each_distro(shell_command: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for distro in parse_wsl_distro_list(&run_wsl_command(&["--list", "--quiet"])) {
        let mut args = vec!["-d", distro.as_str(), "-e", "sh", "-lc", shell_command];
        if let Some(path) = path_from_console_output(&run_wsl_command(&args)) {
            push_unique(&mut paths, path);
        }
    }

    paths
}

#[cfg(target_os = "windows")]
fn cached_paths(
    cache: &LazyLock<Mutex<Option<(Instant, Vec<PathBuf>)>>>,
    shell_command: &str,
) -> Vec<PathBuf> {
    let Ok(mut cache) = cache.lock() else {
        return wsl_path_for_each_distro(shell_command);
    };

    let now = Instant::now();
    if let Some((fetched_at, paths)) = cache.as_ref() {
        if now.duration_since(*fetched_at) < WSL_PATH_CACHE_TTL {
            return paths.clone();
        }
    }

    let paths = wsl_path_for_each_distro(shell_command);
    *cache = Some((now, paths.clone()));
    paths
}

#[cfg(target_os = "windows")]
fn push_unique(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|path| path == &candidate) {
        paths.push(candidate);
    }
}

#[cfg(target_os = "windows")]
pub fn windows_wsl_claude_sessions_dirs() -> Vec<PathBuf> {
    cached_paths(
        &CLAUDE_SESSIONS_CACHE,
        r#"wslpath -w "$HOME/.claude/sessions""#,
    )
}

#[cfg(not(target_os = "windows"))]
pub fn windows_wsl_claude_sessions_dirs() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(target_os = "windows")]
pub fn windows_wsl_cursor_projects_dirs() -> Vec<PathBuf> {
    cached_paths(
        &CURSOR_PROJECTS_CACHE,
        r#"wslpath -w "$HOME/.cursor/projects""#,
    )
}

#[cfg(not(target_os = "windows"))]
pub fn windows_wsl_cursor_projects_dirs() -> Vec<PathBuf> {
    Vec::new()
}
