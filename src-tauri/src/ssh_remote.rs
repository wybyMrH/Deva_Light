use crate::logging::{log_error, log_info, log_warn};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SSH_URI_PREFIX: &str = "ssh://";

pub fn is_ssh_virtual_path(path: &Path) -> bool {
    path.to_str()
        .is_some_and(|value| value.starts_with(SSH_URI_PREFIX))
}

pub fn parse_ssh_virtual_path(path: &Path) -> Option<(String, String)> {
    let value = path.to_str()?;
    let rest = value.strip_prefix(SSH_URI_PREFIX)?;
    let (target, remote) = rest.split_once('/')?;
    if target.is_empty() || remote.is_empty() {
        return None;
    }

    Some((target.to_string(), format!("/{remote}")))
}

pub fn build_ssh_virtual_path(target: &str, remote_path: &str) -> PathBuf {
    let remote = remote_path.trim_start_matches('/');
    PathBuf::from(format!("{SSH_URI_PREFIX}{target}/{remote}"))
}

pub fn discover_codex_sessions_dir(target: &str) -> Option<PathBuf> {
    let remote_dir = ssh_command(target, "printf '%s' \"$HOME/.codex/sessions\"").ok()?;
    if remote_dir.is_empty() {
        return None;
    }

    let exists = ssh_command(
        target,
        &format!("test -d {} && echo ok", shell_quote(&remote_dir)),
    )
    .ok();

    if exists.as_deref() != Some("ok") {
        return None;
    }

    Some(build_ssh_virtual_path(target, &remote_dir))
}

pub fn list_rollout_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let (target, remote_root) = parse_ssh_virtual_path(root)
        .ok_or_else(|| format!("invalid ssh path: {}", root.display()))?;

    let output = ssh_command(
        &target,
        &format!(
            "find {} -type f -name 'rollout-*.jsonl' 2>/dev/null",
            shell_quote(&remote_root)
        ),
    )?;

    let mut files = Vec::new();
    for line in output.lines().map(str::trim).filter(|line| !line.is_empty()) {
        files.push(build_ssh_virtual_path(&target, line.trim_start_matches('/')));
    }

    files.sort();
    Ok(files)
}

pub fn rollout_modified(path: &Path) -> Result<SystemTime, String> {
    let (target, remote_path) = parse_ssh_virtual_path(path)
        .ok_or_else(|| format!("invalid ssh path: {}", path.display()))?;

    let seconds = ssh_command(
        &target,
        &format!("stat -c %Y {}", shell_quote(&remote_path)),
    )?
    .trim()
    .parse::<u64>()
    .map_err(|error| format!("invalid remote mtime for {}: {error}", path.display()))?;

    Ok(UNIX_EPOCH + Duration::from_secs(seconds))
}

pub fn read_rollout_from_offset(path: &Path, offset: u64) -> Result<(String, u64), String> {
    let (target, remote_path) = parse_ssh_virtual_path(path)
        .ok_or_else(|| format!("invalid ssh path: {}", path.display()))?;

    let skip = offset.saturating_add(1);
    let output = ssh_command(
        &target,
        &format!(
            "if [ -f {} ]; then tail -c +{skip} {}; fi",
            shell_quote(&remote_path),
            shell_quote(&remote_path)
        ),
    )?;

    let new_offset = offset.saturating_add(output.len() as u64);
    Ok((output, new_offset))
}

pub fn ssh_command(target: &str, remote_command: &str) -> Result<String, String> {
    let output = run_ssh(target, remote_command)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("ssh command failed for {target}")
        } else {
            stderr
        })
    }
}

fn run_ssh(target: &str, remote_command: &str) -> Result<Output, String> {
    Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=8",
            "-o",
            "StrictHostKeyChecking=accept-new",
        ])
        .arg(target)
        .arg(remote_command)
        .output()
        .map_err(|error| format!("failed to run ssh for {target}: {error}"))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_virtual_path() {
        let path = PathBuf::from("ssh://user@host/home/user/.codex/sessions/a.jsonl");
        let (target, remote) = parse_ssh_virtual_path(&path).unwrap();
        assert_eq!(target, "user@host");
        assert_eq!(remote, "/home/user/.codex/sessions/a.jsonl");
    }

    #[test]
    fn builds_ssh_virtual_path() {
        let path = build_ssh_virtual_path("user@host", "/home/user/.codex/sessions");
        assert_eq!(
            path,
            PathBuf::from("ssh://user@host/home/user/.codex/sessions")
        );
    }
}
