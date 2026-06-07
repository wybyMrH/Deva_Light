use crate::config::load_app_config;
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
    let remote_dir = ssh_command(
        target,
        r#"printf '%s' "${CODEX_HOME:-$HOME/.codex}/sessions""#,
    )
    .ok()?;
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
    let output = run_ssh(target, remote_command, None)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("ssh command failed for {target}")
        } else {
            classify_ssh_error(&stderr)
        })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshConnectionTest {
    pub ok: bool,
    pub message: String,
    pub codex_path: Option<String>,
}

pub fn test_ssh_connection(target: &str, identity_file: Option<&str>) -> SshConnectionTest {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return SshConnectionTest {
            ok: false,
            message: "请先填写 SSH 目标，例如 user@192.168.1.10".to_string(),
            codex_path: None,
        };
    }

    match run_ssh(trimmed, "echo deva-light-ok", identity_file) {
        Ok(output) if output.status.success() => {
            let codex_path =
                discover_codex_sessions_dir(trimmed).map(|path| path.to_string_lossy().to_string());
            let message = if codex_path.is_some() {
                "连接成功，已检测到远程 Codex 会话目录".to_string()
            } else {
                "连接成功，但未找到远程 ~/.codex/sessions".to_string()
            };
            SshConnectionTest {
                ok: true,
                message,
                codex_path,
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            SshConnectionTest {
                ok: false,
                message: classify_ssh_error(stderr.trim()),
                codex_path: None,
            }
        }
        Err(message) => SshConnectionTest {
            ok: false,
            message,
            codex_path: None,
        },
    }
}

fn classify_ssh_error(stderr: &str) -> String {
    let lower = stderr.to_ascii_lowercase();

    if lower.contains("permission denied") && lower.contains("publickey") {
        "公钥认证失败：请将本机公钥添加到远程 authorized_keys（可用 ssh-copy-id）".to_string()
    } else if lower.contains("permission denied") {
        "认证失败：后台监控使用非交互 SSH（BatchMode），不支持密码输入。请配置密钥或 ssh-agent。"
            .to_string()
    } else if lower.contains("identity file") && lower.contains("not accessible") {
        format!("私钥文件不可用：{stderr}")
    } else if lower.contains("host key verification failed") {
        "主机密钥未信任：请先在终端手动 ssh 连接一次".to_string()
    } else if lower.contains("connection refused") {
        "连接被拒绝：请确认 SSH 服务已启动且端口正确".to_string()
    } else if lower.contains("could not resolve hostname") || lower.contains("name or service not known")
    {
        "无法解析主机名，请检查 SSH 目标格式".to_string()
    } else if lower.contains("network is unreachable") || lower.contains("no route to host") {
        "网络不可达，请检查 IP 与防火墙".to_string()
    } else if lower.contains("operation timed out") || lower.contains("timed out") {
        "连接超时，请检查网络与 SSH 服务".to_string()
    } else if stderr.is_empty() {
        "SSH 命令失败".to_string()
    } else {
        format!("SSH 失败：{stderr}")
    }
}

fn run_ssh(
    target: &str,
    remote_command: &str,
    identity_override: Option<&str>,
) -> Result<Output, String> {
    let identity_file = identity_override
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .map(str::to_string)
        .or_else(|| {
            load_app_config()
                .normalized_ssh_targets()
                .into_iter()
                .find(|entry| entry.target == target)
                .and_then(|entry| entry.identity_file)
                .filter(|value| !value.trim().is_empty())
        });

    let mut command = Command::new("ssh");
    command.args([
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "StrictHostKeyChecking=accept-new",
    ]);

    if let Some(path) = identity_file {
        command.arg("-i").arg(path);
    }

    command
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

    #[test]
    fn classifies_password_auth_error() {
        let message = classify_ssh_error("Permission denied (publickey,password).");
        assert!(message.contains("公钥"));
    }

    #[test]
    fn rejects_empty_ssh_target() {
        let result = test_ssh_connection("", None);
        assert!(!result.ok);
    }
}
