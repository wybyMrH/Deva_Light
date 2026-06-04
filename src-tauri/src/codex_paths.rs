use crate::config::{load_app_config, AppConfig};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexSessionRootSummary {
    pub auto: Vec<PathBuf>,
    pub manual: Vec<PathBuf>,
    pub active: Vec<PathBuf>,
    pub missing: Vec<PathBuf>,
}

pub fn codex_session_root_summary() -> CodexSessionRootSummary {
    codex_session_root_summary_for_auto(&auto_codex_sessions_dirs(), &load_app_config())
}

pub fn codex_session_root_summary_for_config(config: &AppConfig) -> CodexSessionRootSummary {
    codex_session_root_summary_for_auto(&auto_codex_sessions_dirs(), config)
}

pub fn codex_session_root_summary_for_auto(
    auto_roots: &[PathBuf],
    config: &AppConfig,
) -> CodexSessionRootSummary {
    let auto = auto_roots.to_vec();
    let manual = parse_manual_codex_sessions_dirs(config);

    let mut candidates = auto.clone();
    for path in &manual {
        push_unique(&mut candidates, path.clone());
    }

    let mut active = Vec::new();
    let mut missing = Vec::new();

    for path in candidates {
        if path.exists() {
            push_unique(&mut active, path);
        } else {
            push_unique(&mut missing, path);
        }
    }

    CodexSessionRootSummary {
        auto,
        manual,
        active,
        missing,
    }
}

pub fn codex_sessions_dirs() -> Vec<PathBuf> {
    codex_session_root_summary().active
}

pub fn format_paths(paths: &[PathBuf]) -> String {
    if paths.is_empty() {
        return "(none)".to_string();
    }

    paths
        .iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn push_unique(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|path| path == &candidate) {
        paths.push(candidate);
    }
}

fn parse_manual_codex_sessions_dirs(config: &AppConfig) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for value in &config.codex_session_paths {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }

        push_unique(&mut paths, PathBuf::from(trimmed));
    }

    paths
}

pub fn auto_codex_sessions_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    push_unique(&mut dirs, default_codex_sessions_dir());

    #[cfg(target_os = "windows")]
    {
        for path in windows_wsl_codex_sessions_dirs() {
            push_unique(&mut dirs, path);
        }
    }

    dirs
}

fn default_codex_sessions_dir() -> PathBuf {
    if let Some(codex_home) = std::env::var_os("CODEX_HOME") {
        return PathBuf::from(codex_home).join("sessions");
    }

    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
        .join("sessions")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(target_os = "windows")]
fn windows_wsl_codex_sessions_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Some(path) = windows_wsl_codex_sessions_dir(None) {
        push_unique(&mut dirs, path);
    }

    for distro in parse_wsl_distro_list(&run_wsl_command(&["--list", "--quiet"])) {
        if let Some(path) = windows_wsl_codex_sessions_dir(Some(&distro)) {
            push_unique(&mut dirs, path);
        }
    }

    dirs
}

#[cfg(target_os = "windows")]
fn windows_wsl_codex_sessions_dir(distro: Option<&str>) -> Option<PathBuf> {
    const WSL_CODEX_SESSIONS_COMMAND: &str =
        r#"codex_home="${CODEX_HOME:-$HOME/.codex}"; wslpath -w "$codex_home/sessions""#;

    let mut args = Vec::new();
    if let Some(distro) = distro {
        args.push("-d");
        args.push(distro);
    }
    args.extend(["-e", "sh", "-lc", WSL_CODEX_SESSIONS_COMMAND]);

    path_from_console_output(&run_wsl_command(&args))
}

#[cfg(target_os = "windows")]
fn run_wsl_command(args: &[&str]) -> Vec<u8> {
    let Ok(output) = std::process::Command::new("wsl.exe").args(args).output() else {
        return Vec::new();
    };

    if output.status.success() {
        output.stdout
    } else {
        Vec::new()
    }
}

#[cfg(any(test, target_os = "windows"))]
fn path_from_console_output(bytes: &[u8]) -> Option<PathBuf> {
    decode_console_text(bytes).lines().find_map(|line| {
        let trimmed = line.trim();
        (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
    })
}

#[cfg(any(test, target_os = "windows"))]
fn parse_wsl_distro_list(bytes: &[u8]) -> Vec<String> {
    decode_console_text(bytes)
        .lines()
        .filter_map(|line| {
            let distro = line.trim();
            (!distro.is_empty()).then(|| distro.to_string())
        })
        .collect()
}

#[cfg(any(test, target_os = "windows"))]
fn decode_console_text(bytes: &[u8]) -> String {
    if looks_like_utf16le(bytes) {
        let utf16 = bytes
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        String::from_utf16_lossy(&utf16)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(any(test, target_os = "windows"))]
fn looks_like_utf16le(bytes: &[u8]) -> bool {
    bytes.len() >= 2
        && bytes.len() % 2 == 0
        && bytes.iter().skip(1).step_by(2).any(|byte| *byte == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn decodes_utf16le_console_output() {
        let bytes = b"U\0b\0u\0n\0t\0u\0\r\0\n\0";
        assert_eq!(decode_console_text(bytes), "Ubuntu\r\n");
    }

    #[test]
    fn parses_wsl_distro_list_from_utf16le_output() {
        let bytes = b"U\0b\0u\0n\0t\0u\0\r\0\n\0 \0\r\0\n\0D\0e\0b\0i\0a\0n\0\r\0\n\0";
        assert_eq!(parse_wsl_distro_list(bytes), vec!["Ubuntu", "Debian"]);
    }

    #[test]
    fn path_from_console_output_uses_first_non_empty_line() {
        let path = path_from_console_output(
            b"\r\n\\\\wsl.localhost\\Ubuntu\\home\\hzj\\.codex\\sessions\r\nsecond line\r\n",
        );
        assert_eq!(
            path,
            Some(PathBuf::from(
                r"\\wsl.localhost\Ubuntu\home\alice\.codex\sessions"
            ))
        );
    }

    #[test]
    fn manual_paths_are_trimmed_deduped_and_classified() {
        let existing = std::env::temp_dir().join(unique_name("ai-light-codex-manual-existing"));
        let missing = std::env::temp_dir().join(unique_name("ai-light-codex-manual-missing"));
        fs::create_dir_all(&existing).unwrap();

        let config = AppConfig {
            codex_session_paths: vec![
                format!("  {}  ", existing.display()),
                existing.display().to_string(),
                missing.display().to_string(),
                String::new(),
            ],
            ..AppConfig::default()
        };

        let summary = codex_session_root_summary_for_config(&config);
        assert!(summary.manual.contains(&existing));
        assert!(summary.manual.contains(&missing));
        assert_eq!(summary.manual.len(), 2);
        assert!(summary.active.contains(&existing));
        assert!(summary.missing.contains(&missing));

        let _ = fs::remove_dir_all(existing);
    }

    #[test]
    fn cached_auto_paths_are_classified_without_redetecting() {
        let auto = std::env::temp_dir().join(unique_name("ai-light-codex-auto-existing"));
        let missing = std::env::temp_dir().join(unique_name("ai-light-codex-auto-missing"));
        fs::create_dir_all(&auto).unwrap();

        let summary = codex_session_root_summary_for_auto(
            &[auto.clone(), missing.clone()],
            &AppConfig::default(),
        );
        assert_eq!(summary.auto, vec![auto.clone(), missing.clone()]);
        assert!(summary.active.contains(&auto));
        assert!(summary.missing.contains(&missing));

        let _ = fs::remove_dir_all(auto);
    }

    fn unique_name(prefix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{prefix}-{nanos}")
    }
}
