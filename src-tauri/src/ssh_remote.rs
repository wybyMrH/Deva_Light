use crate::config::{get_config_dir, load_app_config};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn expand_tilde_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed == "~" {
        return home_dir()
            .map(|home| home.to_string_lossy().to_string())
            .unwrap_or_else(|| trimmed.to_string());
    }

    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest).to_string_lossy().to_string();
        }
    }

    trimmed.to_string()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

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
    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        files.push(build_ssh_virtual_path(
            &target,
            line.trim_start_matches('/'),
        ));
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
    let output = run_ssh(target, remote_command, SshAuthOverride::default())?;
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

pub fn pick_ssh_private_key() -> Result<Option<String>, String> {
    let picked = rfd::FileDialog::new()
        .set_title("选择 SSH 私钥")
        .pick_file();

    Ok(picked.map(|path| path.to_string_lossy().to_string()))
}

pub fn test_ssh_connection(
    target: &str,
    identity_file: Option<&str>,
    passphrase: Option<&str>,
) -> SshConnectionTest {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return SshConnectionTest {
            ok: false,
            message: "请先填写 SSH 目标，例如 user@192.168.1.10".to_string(),
            codex_path: None,
        };
    }

    match run_ssh(
        trimmed,
        "echo deva-light-ok",
        SshAuthOverride {
            identity_file: identity_file.map(str::to_string),
            passphrase: passphrase.map(str::to_string),
        },
    ) {
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
        "公钥认证失败：后台监控无法输入密码，请用 ssh-copy-id 配置免密，或检查私钥路径是否正确（支持 ~/.ssh/...）"
            .to_string()
    } else if lower.contains("incorrect passphrase") || lower.contains("bad passphrase") {
        "私钥口令错误：请检查「私钥口令」字段，或改用 ssh-agent 预先加载密钥。".to_string()
    } else if lower.contains("permission denied") {
        "认证失败：请确认公钥已写入远程 authorized_keys。若服务器要求「密钥+登录密码」双重验证，请为监控账号单独开启仅公钥登录。"
            .to_string()
    } else if lower.contains("identity file") && lower.contains("not accessible") {
        format!("私钥文件不可用：{stderr}")
    } else if lower.contains("host key verification failed") {
        "主机密钥未信任：请先在终端手动 ssh 连接一次".to_string()
    } else if lower.contains("connection refused") {
        "连接被拒绝：请确认 SSH 服务已启动且端口正确".to_string()
    } else if lower.contains("could not resolve hostname")
        || lower.contains("name or service not known")
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

#[derive(Debug, Clone, Default)]
struct SshAuthOverride {
    identity_file: Option<String>,
    passphrase: Option<String>,
}

fn run_ssh(
    target: &str,
    remote_command: &str,
    auth_override: SshAuthOverride,
) -> Result<Output, String> {
    let config_entry = load_app_config()
        .normalized_ssh_targets()
        .into_iter()
        .find(|entry| entry.target == target);

    let identity_file = auth_override
        .identity_file
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            config_entry
                .as_ref()
                .and_then(|entry| entry.identity_file.clone())
        });

    let passphrase = auth_override
        .passphrase
        .filter(|value| !value.trim().is_empty())
        .or_else(|| config_entry.and_then(|entry| entry.passphrase));

    let mut command = Command::new("ssh");
    command.args([
        "-o",
        "ConnectTimeout=8",
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "PreferredAuthentications=publickey",
    ]);

    if let Some(passphrase) = passphrase.as_deref() {
        let askpass = write_askpass_helper(passphrase)?;
        command.env("SSH_ASKPASS", &askpass);
        command.env("SSH_ASKPASS_REQUIRE", "force");
        command.arg("-o").arg("BatchMode=no");
    } else {
        command.arg("-o").arg("BatchMode=yes");
    }

    if let Some(path) = identity_file {
        let expanded = expand_identity_path(&path);
        if !PathBuf::from(&expanded).exists() {
            return Err(format!("私钥文件不存在：{expanded}"));
        }
        command.arg("-i").arg(expanded);
    }

    command
        .arg(target)
        .arg(remote_command)
        .output()
        .map_err(|error| format!("failed to run ssh for {target}: {error}"))
}

pub fn expand_identity_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.starts_with("\\\\") || trimmed.starts_with("//") {
        return trimmed.to_string();
    }
    expand_tilde_path(trimmed)
}

fn write_askpass_helper(passphrase: &str) -> Result<PathBuf, String> {
    let askpass_path = get_config_dir().join("ssh-askpass-helper");

    #[cfg(windows)]
    {
        let escaped = passphrase.replace('%', "%%").replace('"', "\"\"");
        fs::write(&askpass_path, format!("@echo off\r\necho {escaped}\r\n"))
            .map_err(|error| format!("failed to write ssh askpass helper: {error}"))?;
    }

    #[cfg(not(windows))]
    {
        let escaped = passphrase.replace('\'', "'\\''");
        fs::write(&askpass_path, format!("#!/bin/sh\necho '{escaped}'\n"))
            .map_err(|error| format!("failed to write ssh askpass helper: {error}"))?;
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&askpass_path)
            .map_err(|error| error.to_string())?
            .permissions();
        perms.set_mode(0o700);
        fs::set_permissions(&askpass_path, perms).map_err(|error| error.to_string())?;
    }

    Ok(askpass_path)
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
        let result = test_ssh_connection("", None, None);
        assert!(!result.ok);
    }

    #[test]
    fn expands_tilde_in_identity_path() {
        if let Some(home) = home_dir() {
            let expanded = expand_tilde_path("~/.ssh/id_ed25519");
            assert_eq!(
                expanded,
                home.join(".ssh/id_ed25519").to_string_lossy().to_string()
            );
        }
    }
}
