use crate::codex_paths::is_wsl_unc_path;
use crate::ssh_remote::is_ssh_virtual_path;
use serde::{Deserialize, Serialize};
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

pub fn compose_light_id(logical_project_id: &str, origin: MonitorOrigin) -> String {
    format!("{logical_project_id}@@{}", origin.as_key())
}

pub fn format_light_label(origin: MonitorOrigin, project_label: &str) -> String {
    format!("{} · {project_label}", origin.label_prefix())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
        let id = compose_light_id("git:https://github.com/foo/bar", MonitorOrigin::Wsl);
        assert_eq!(id, "git:https://github.com/foo/bar@@wsl");
    }
}
