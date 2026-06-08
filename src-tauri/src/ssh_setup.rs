use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshHostHint {
    pub host_alias: String,
    pub target: String,
    pub identity_file: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyCandidate {
    pub id: String,
    pub source: String,
    pub source_label: String,
    pub identity_path: String,
    pub display_path: String,
    pub public_key_path: Option<String>,
    pub hosts: Vec<SshHostHint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshSetupGuide {
    pub generate_key_command: String,
    pub copy_key_command_template: String,
    pub test_command_template: String,
    pub windows_agent_commands: Vec<String>,
    pub wsl_agent_commands: Vec<String>,
}

pub fn build_ssh_setup_guide() -> SshSetupGuide {
    let home = home_dir_display();
    SshSetupGuide {
        generate_key_command: "ssh-keygen -t ed25519 -C \"deva-light\" -f ~/.ssh/id_ed25519".to_string(),
        copy_key_command_template:
            "ssh-copy-id -i ~/.ssh/id_ed25519.pub user@192.168.1.10".to_string(),
        test_command_template: "ssh -i ~/.ssh/id_ed25519 user@192.168.1.10 echo ok".to_string(),
        windows_agent_commands: vec![
            "Get-Service ssh-agent | Set-Service -StartupType Manual".to_string(),
            "Start-Service ssh-agent".to_string(),
            format!("ssh-add {home}\\.ssh\\id_ed25519"),
        ],
        wsl_agent_commands: vec![
            "eval \"$(ssh-agent -s)\"".to_string(),
            "ssh-add ~/.ssh/id_ed25519".to_string(),
        ],
    }
}

pub fn read_ssh_public_key(identity_path: &str) -> Result<String, String> {
    let trimmed = identity_path.trim();
    if trimmed.is_empty() {
        return Err("私钥路径为空".to_string());
    }

    let private_key = PathBuf::from(trimmed);
    let public_key = if trimmed.ends_with(".pub") {
        private_key
    } else {
        PathBuf::from(format!("{trimmed}.pub"))
    };

    if !public_key.is_file() {
        return Err(format!(
            "未找到公钥文件：{}",
            public_key.to_string_lossy()
        ));
    }

    fs::read_to_string(&public_key).map_err(|error| error.to_string())
}

pub fn discover_ssh_key_candidates() -> Vec<SshKeyCandidate> {
    let mut candidates = Vec::new();
    let mut seen = HashMap::new();

    if let Some(windows_ssh) = windows_ssh_dir() {
        push_ssh_dir_candidates(&mut candidates, &mut seen, &windows_ssh, "windows", "Windows 本机");
    }

    #[cfg(target_os = "windows")]
    {
        for (distro, ssh_dir) in discover_wsl_ssh_dirs() {
            let source = format!("wsl:{distro}");
            let label = format!("WSL · {distro}");
            push_ssh_dir_candidates(&mut candidates, &mut seen, &ssh_dir, &source, &label);
        }
    }

    candidates
}

fn push_ssh_dir_candidates(
    candidates: &mut Vec<SshKeyCandidate>,
    seen: &mut HashMap<String, bool>,
    ssh_dir: &Path,
    source: &str,
    source_label: &str,
) {
    let config_hosts = parse_ssh_config_hosts(&ssh_dir.join("config"));
    let private_keys = list_private_keys(ssh_dir);

    for key_path in private_keys {
        let identity_path = key_path.to_string_lossy().to_string();
        if seen.contains_key(&identity_path) {
            continue;
        }
        seen.insert(identity_path.clone(), true);

        let file_name = key_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("id_ed25519");
        let public_key = ssh_dir.join(format!("{file_name}.pub"));
        let public_key_path = public_key.exists().then(|| {
            public_key.to_string_lossy().to_string()
        });

        let display_path = display_identity_path(&identity_path, source);
        let id = format!("{source}:{display_path}");
        let hosts = config_hosts
            .iter()
            .filter(|host| host_matches_identity(host, &identity_path, ssh_dir))
            .cloned()
            .collect();

        candidates.push(SshKeyCandidate {
            id,
            source: source.to_string(),
            source_label: source_label.to_string(),
            identity_path,
            display_path,
            public_key_path,
            hosts,
        });
    }
}

fn list_private_keys(ssh_dir: &Path) -> Vec<PathBuf> {
    let mut keys = Vec::new();
    let Ok(entries) = fs::read_dir(ssh_dir) else {
        return keys;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        if name.ends_with(".pub")
            || name == "config"
            || name == "known_hosts"
            || name == "authorized_keys"
            || name.ends_with(".bak")
        {
            continue;
        }

        if name.starts_with("id_") || name == "identity" || name.ends_with(".pem") {
            keys.push(path);
        }
    }

    keys.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    keys
}

#[derive(Debug, Clone)]
struct ParsedSshHost {
    host_alias: String,
    hostname: Option<String>,
    user: Option<String>,
    identity_file: Option<String>,
    port: Option<String>,
}

fn parse_ssh_config_hosts(config_path: &Path) -> Vec<SshHostHint> {
    let Ok(content) = fs::read_to_string(config_path) else {
        return Vec::new();
    };

    let mut hosts = Vec::new();
    let mut current: Option<ParsedSshHost> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let Some((keyword, value)) = trimmed.split_once(char::is_whitespace) else {
            continue;
        };
        let keyword = keyword.to_ascii_lowercase();
        let value = value.trim().trim_matches('"');

        if keyword == "host" {
            if let Some(host) = current.take() {
                if let Some(hint) = host.into_hint() {
                    hosts.push(hint);
                }
            }
            if value == "*" {
                current = None;
                continue;
            }
            current = Some(ParsedSshHost {
                host_alias: value.to_string(),
                hostname: None,
                user: None,
                identity_file: None,
                port: None,
            });
            continue;
        }

        let Some(host) = current.as_mut() else {
            continue;
        };

        match keyword.as_str() {
            "hostname" => host.hostname = Some(value.to_string()),
            "user" => host.user = Some(value.to_string()),
            "identityfile" => host.identity_file = Some(value.to_string()),
            "port" => host.port = Some(value.to_string()),
            _ => {}
        }
    }

    if let Some(host) = current {
        if let Some(hint) = host.into_hint() {
            hosts.push(hint);
        }
    }

    hosts
}

impl ParsedSshHost {
    fn into_hint(self) -> Option<SshHostHint> {
        if self.host_alias == "*" {
            return None;
        }

        let hostname = self.hostname.unwrap_or_else(|| self.host_alias.clone());
        let user = self.user.unwrap_or_else(|| "root".to_string());
        let port = self.port.map(|value| format!(":{value}")).unwrap_or_default();
        let target = format!("{user}@{hostname}{port}");

        Some(SshHostHint {
            host_alias: self.host_alias,
            target,
            identity_file: self.identity_file,
        })
    }
}

fn host_matches_identity(host: &SshHostHint, identity_path: &str, ssh_dir: &Path) -> bool {
    let Some(configured) = host.identity_file.as_deref() else {
        return default_identity_names()
            .iter()
            .any(|name| identity_path.ends_with(name));
    };

    let expanded = expand_identity_reference(configured, ssh_dir);
    paths_equivalent(&expanded, identity_path)
}

fn expand_identity_reference(value: &str, ssh_dir: &Path) -> String {
    let trimmed = value.trim();
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return ssh_dir.join(rest).to_string_lossy().to_string();
    }
    if trimmed == "~" {
        return ssh_dir.to_string_lossy().to_string();
    }
    trimmed.to_string()
}

fn default_identity_names() -> [&'static str; 2] {
    ["id_ed25519", "id_rsa"]
}

fn paths_equivalent(left: &str, right: &str) -> bool {
    left.replace('\\', "/").eq_ignore_ascii_case(&right.replace('\\', "/"))
}

fn display_identity_path(path: &str, source: &str) -> String {
    if source == "windows" {
        if let Some(home) = home_dir() {
            let home_str = home.to_string_lossy().replace('\\', "/");
            let normalized = path.replace('\\', "/");
            if let Some(rest) = normalized.strip_prefix(&home_str) {
                return format!("~{rest}");
            }
        }
    }

    if source.starts_with("wsl:") {
        if let Some(idx) = path.find("/home/") {
            let tail = &path[idx..];
            return format!("~{}", tail.strip_prefix("/home").unwrap_or(tail));
        }
    }

    path.to_string()
}

fn windows_ssh_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".ssh"))
}

#[cfg(target_os = "windows")]
fn discover_wsl_ssh_dirs() -> Vec<(String, PathBuf)> {
    use crate::codex_paths::{parse_wsl_distro_list, run_wsl_command};

    let mut dirs = Vec::new();
    for distro in parse_wsl_distro_list(&run_wsl_command(&["--list", "--quiet"])) {
        if let Some(path) = wsl_ssh_dir_for_distro(&distro) {
            dirs.push((distro, path));
        }
    }
    dirs
}

#[cfg(not(target_os = "windows"))]
fn discover_wsl_ssh_dirs() -> Vec<(String, PathBuf)> {
    Vec::new()
}

#[cfg(target_os = "windows")]
fn wsl_ssh_dir_for_distro(distro: &str) -> Option<PathBuf> {
    const COMMAND: &str = r#"wslpath -w "$HOME/.ssh""#;
    let output = crate::codex_paths::run_wsl_command(&["-d", distro, "-e", "sh", "-lc", COMMAND]);
    crate::codex_paths::path_from_console_output(&output).filter(|path| path.exists())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn home_dir_display() -> String {
    home_dir()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|| "%USERPROFILE%".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ssh_config_hosts() {
        let dir = std::env::temp_dir().join(format!(
            "ai-light-ssh-setup-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("config"),
            r#"
Host lab
    HostName 192.168.1.10
    User admin
    IdentityFile ~/.ssh/id_ed25519
"#,
        )
        .unwrap();

        let hosts = parse_ssh_config_hosts(&dir.join("config"));
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].target, "admin@192.168.1.10");
        let _ = fs::remove_dir_all(dir);
    }
}
