use serde_json::Value as JsonValue;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml::Value as TomlValue;

/// Identify the project represented by a working directory.
///
/// Returns `(project_id, project_label)`. `project_id` prefers a stable git
/// remote identity, otherwise a normalized path key that merges WSL/Windows
/// views of the same directory.
pub fn identify_project(cwd: &Path) -> (String, String) {
    let project_path = find_git_root(cwd).unwrap_or_else(|| normalize_path(cwd));
    let project_label = project_label(&project_path);
    let project_id = git_remote_identity(&project_path)
        .unwrap_or_else(|| normalize_path_key(&display_path(&project_path)));

    (project_id, project_label)
}

pub fn normalize_path_key(path: &str) -> String {
    let mut normalized = strip_windows_verbatim_prefix(path).replace('\\', "/");

    if let Some(rest) = normalized.strip_prefix("/mnt/") {
        if let Some((drive, tail)) = rest.split_once('/') {
            if drive.len() == 1 && drive.chars().all(|ch| ch.is_ascii_alphabetic()) {
                normalized = format!("{drive}:/{tail}");
            }
        }
    }

    let lower = normalized.to_lowercase();
    if lower.starts_with("//wsl.localhost/") || lower.starts_with("//wsl$/") {
        let segments: Vec<&str> = normalized
            .split('/')
            .filter(|part| !part.is_empty())
            .collect();
        if segments.len() >= 3 {
            let distro = segments[2].to_lowercase();
            let tail = segments[3..].join("/").to_lowercase();
            normalized = format!("wsl://{distro}/{tail}");
        }
    } else if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
        normalized = normalized.to_lowercase();
    }

    normalized
}

fn git_remote_identity(project_path: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["config", "--get", "remote.origin.url"])
        .current_dir(project_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let remote = String::from_utf8(output.stdout).ok()?;
    let remote = remote.trim();
    if remote.is_empty() {
        None
    } else {
        Some(format!("git:{remote}"))
    }
}

fn find_git_root(cwd: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    let root = stdout.trim();

    if root.is_empty() {
        None
    } else {
        Some(normalize_path(Path::new(root)))
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn project_label(project_path: &Path) -> String {
    declared_project_name(project_path).unwrap_or_else(|| fallback_project_label(project_path))
}

fn declared_project_name(project_path: &Path) -> Option<String> {
    tauri_product_name(project_path)
        .or_else(|| package_json_name(project_path))
        .or_else(|| cargo_package_name(project_path))
        .or_else(|| pyproject_name(project_path))
        .or_else(|| go_module_name(project_path))
}

fn tauri_product_name(project_path: &Path) -> Option<String> {
    let config_path = project_path.join("src-tauri").join("tauri.conf.json");
    let value: JsonValue = serde_json::from_str(&fs::read_to_string(config_path).ok()?).ok()?;
    non_empty_string(value.get("productName")?.as_str())
}

fn package_json_name(project_path: &Path) -> Option<String> {
    let value: JsonValue =
        serde_json::from_str(&fs::read_to_string(project_path.join("package.json")).ok()?).ok()?;
    non_empty_string(value.get("name")?.as_str())
}

fn cargo_package_name(project_path: &Path) -> Option<String> {
    let value: TomlValue =
        toml::from_str(&fs::read_to_string(project_path.join("Cargo.toml")).ok()?).ok()?;
    non_empty_string(value.get("package")?.get("name")?.as_str())
}

fn pyproject_name(project_path: &Path) -> Option<String> {
    let value: TomlValue =
        toml::from_str(&fs::read_to_string(project_path.join("pyproject.toml")).ok()?).ok()?;
    non_empty_string(value.get("project")?.get("name")?.as_str())
}

fn go_module_name(project_path: &Path) -> Option<String> {
    let content = fs::read_to_string(project_path.join("go.mod")).ok()?;
    let module = content
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("module "))?
        .trim();
    non_empty_string(module.rsplit('/').next())
}

fn non_empty_string(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn fallback_project_label(project_path: &Path) -> String {
    project_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .to_string()
}

fn display_path(path: &Path) -> String {
    strip_windows_verbatim_prefix(&path.to_string_lossy())
}

fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_uses_cwd_when_git_root_is_unavailable() {
        let cwd = std::env::temp_dir().join(unique_name("ai-light-no-git"));
        std::fs::create_dir_all(&cwd).unwrap();

        let (project_id, project_label) = identify_project(&cwd);

        assert_eq!(project_id, display_path(&normalize_path(&cwd)));
        assert_eq!(project_label, cwd.file_name().unwrap().to_string_lossy());

        std::fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn uses_declared_package_name_before_directory_name() {
        let cwd = std::env::temp_dir().join(unique_name("ai-light-package-dir"));
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::write(
            cwd.join("package.json"),
            r#"{"name":"declared-package-name"}"#,
        )
        .unwrap();

        let (_project_id, project_label) = identify_project(&cwd);

        assert_eq!(project_label, "declared-package-name");

        std::fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn uses_tauri_product_name_before_workspace_fallback() {
        let cwd = std::env::temp_dir().join(unique_name("ai-light-tauri-dir"));
        std::fs::create_dir_all(cwd.join("src-tauri")).unwrap();
        std::fs::write(cwd.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
        std::fs::write(
            cwd.join("src-tauri").join("tauri.conf.json"),
            r#"{"productName":"AI Light"}"#,
        )
        .unwrap();

        let (_project_id, project_label) = identify_project(&cwd);

        assert_eq!(project_label, "AI Light");

        std::fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn normalizes_wsl_and_windows_paths_to_same_key() {
        assert_eq!(
            normalize_path_key("/mnt/c/Users/alice/projects/demo"),
            normalize_path_key(r"C:\Users\alice\projects\demo")
        );
    }

    #[test]
    fn strips_windows_verbatim_prefix_for_display() {
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\N:\AI\ai_light"),
            r"N:\AI\ai_light"
        );
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\UNC\server\share"),
            r"\\server\share"
        );
    }

    fn unique_name(prefix: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        format!("{prefix}-{nanos}")
    }
}
