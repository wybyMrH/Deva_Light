use crate::codex_paths::is_wsl_unc_path;
use crate::ssh_remote::{is_ssh_virtual_path, parse_ssh_virtual_path};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorOrigin {
    Local,
    Wsl,
    Ssh,
    Remote,
}

impl MonitorOrigin {
    pub fn as_key(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Wsl => "wsl",
            Self::Ssh => "ssh",
            Self::Remote => "remote",
        }
    }

    pub fn label_prefix(self) -> &'static str {
        match self {
            Self::Local => "本地",
            Self::Wsl => "WSL",
            Self::Ssh => "SSH",
            Self::Remote => "远程",
        }
    }
}

pub fn detect_monitor_origin(cwd: &Path, context_path: Option<&Path>) -> MonitorOrigin {
    if let Some(context) = context_path {
        if is_ssh_virtual_path(context) {
            return MonitorOrigin::Ssh;
        }
        if is_wsl_unc_path(context) {
            return MonitorOrigin::Wsl;
        }
    }

    detect_monitor_origin_from_cwd(cwd)
}

pub fn detect_monitor_origin_from_cwd(cwd: &Path) -> MonitorOrigin {
    let normalized = cwd.to_string_lossy().replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();

    if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
        return MonitorOrigin::Local;
    }

    if lower.starts_with("//wsl.localhost/")
        || lower.starts_with("//wsl$/")
        || lower.starts_with("wsl://")
        || lower.starts_with("/mnt/")
    {
        return MonitorOrigin::Wsl;
    }

    if lower.starts_with("/users/") || lower.starts_with("/volumes/") {
        return MonitorOrigin::Local;
    }

    #[cfg(target_os = "windows")]
    if lower.starts_with("/home/") || lower.starts_with("/root/") {
        return MonitorOrigin::Remote;
    }

    #[cfg(not(target_os = "windows"))]
    if lower.starts_with("/home/") || lower.starts_with("/root/") {
        return MonitorOrigin::Local;
    }

    MonitorOrigin::Local
}

pub fn compose_light_id(
    logical_project_id: &str,
    origin: MonitorOrigin,
    tool: crate::types::Tool,
) -> String {
    format!(
        "{logical_project_id}@@{}@@{}",
        origin.as_key(),
        tool.as_key()
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginIdentity {
    pub origin: MonitorOrigin,
    pub key: String,
    pub detail: String,
}

pub fn resolve_origin_identity(cwd: &Path, context_path: Option<&Path>) -> OriginIdentity {
    if let Some(context) = context_path {
        if let Some((target, _)) = parse_ssh_virtual_path(context) {
            return OriginIdentity {
                origin: MonitorOrigin::Ssh,
                key: format!("ssh:{target}"),
                detail: target,
            };
        }
        if let Some(distro) = wsl_distro_from_path(context) {
            return OriginIdentity {
                origin: MonitorOrigin::Wsl,
                key: format!("wsl:{distro}"),
                detail: distro,
            };
        }
    }

    if let Some(distro) = wsl_distro_from_path(cwd) {
        return OriginIdentity {
            origin: MonitorOrigin::Wsl,
            key: format!("wsl:{distro}"),
            detail: distro,
        };
    }

    let origin = detect_monitor_origin_from_cwd(cwd);
    OriginIdentity {
        key: origin.as_key().to_string(),
        detail: origin.label_prefix().to_string(),
        origin,
    }
}

pub fn resolve_origin_display(
    identity: &OriginIdentity,
    aliases: &HashMap<String, String>,
) -> String {
    if let Some(alias) = aliases
        .get(&identity.key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return alias.to_string();
    }

    match identity.origin {
        MonitorOrigin::Local => "本地".to_string(),
        MonitorOrigin::Wsl | MonitorOrigin::Ssh => identity.detail.clone(),
        MonitorOrigin::Remote => "远程".to_string(),
    }
}

fn wsl_distro_from_path(path: &Path) -> Option<String> {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();

    if lower.starts_with("//wsl.localhost/") || lower.starts_with("//wsl$/") {
        let segments: Vec<&str> = normalized
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        if segments.len() >= 3 {
            return Some(segments[2].to_string());
        }
    }

    if lower.starts_with("wsl://") {
        let segments: Vec<&str> = normalized
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        if segments.len() >= 2 {
            return Some(segments[1].to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_windows_local_path() {
        assert_eq!(
            detect_monitor_origin_from_cwd(Path::new(r"C:\Users\alice\projects\demo")),
            MonitorOrigin::Local
        );
    }

    #[test]
    fn detects_wsl_mount_path() {
        assert_eq!(
            detect_monitor_origin_from_cwd(Path::new("/mnt/c/Users/alice/projects/demo")),
            MonitorOrigin::Wsl
        );
    }

    #[test]
    fn detects_ssh_rollout_context() {
        assert_eq!(
            detect_monitor_origin(
                Path::new("/home/user/project"),
                Some(Path::new(
                    "ssh://user@host/home/user/.codex/sessions/a.jsonl"
                )),
            ),
            MonitorOrigin::Ssh
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn detects_remote_linux_cwd_on_windows() {
        assert_eq!(
            detect_monitor_origin_from_cwd(Path::new("/home/user/project")),
            MonitorOrigin::Remote
        );
    }

    #[test]
    fn composes_light_id_with_origin_suffix() {
        let id = compose_light_id(
            "git:https://github.com/foo/bar",
            MonitorOrigin::Wsl,
            crate::types::Tool::ClaudeCode,
        );
        assert_eq!(id, "git:https://github.com/foo/bar@@wsl@@claude");
    }
}
