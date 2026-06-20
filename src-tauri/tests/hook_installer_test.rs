use deva_light::hook_installer::{
    hook_binary_is_current, merge_hooks, merge_wsl_hooks, remove_ai_light_hooks,
    windows_path_to_wsl_path, wsl_ai_light_url_prefix,
};
use serde_json::json;
use std::path::Path;

#[test]
fn merge_hooks_creates_hooks_object_when_missing() {
    let merged = merge_hooks(json!({}), Path::new("/path/to/deva-light-hook")).unwrap();

    let hooks = merged.get("hooks").unwrap();
    assert!(hooks.get("SessionStart").is_some());
    assert!(hooks.get("UserPromptSubmit").is_some());
    assert!(hooks.get("PreToolUse").is_some());
    assert!(hooks.get("PermissionRequest").is_some());
    assert!(hooks.get("PostToolUse").is_some());
    assert!(hooks.get("Notification").is_some());
    assert!(hooks.get("Stop").is_some());
    assert!(hooks.get("SessionEnd").is_some());
}

#[test]
fn merge_hooks_preserves_existing_settings_and_hooks() {
    let existing = json!({
        "hooks": {
            "PreToolUse": [{
                "matcher": "",
                "hooks": [{"type": "command", "command": "echo test"}]
            }]
        },
        "theme": "dark"
    });

    let merged = merge_hooks(existing, Path::new("/path/to/deva-light-hook")).unwrap();

    assert!(merged["hooks"].get("PreToolUse").is_some());
    assert!(merged["hooks"].get("SessionStart").is_some());
    assert_eq!(merged["theme"], "dark");

    let pre_tool_use = merged["hooks"]["PreToolUse"].as_array().unwrap();
    assert!(pre_tool_use
        .iter()
        .any(|entry| entry["hooks"][0]["command"] == "echo test"));
    assert!(pre_tool_use.iter().any(|entry| entry["hooks"][0]["command"]
        .as_str()
        .unwrap()
        .contains("deva-light-hook")));
}

#[test]
fn merge_hooks_preserves_existing_hooks_for_same_event() {
    let existing = json!({
        "hooks": {
            "SessionStart": [{
                "matcher": "",
                "hooks": [{"type": "command", "command": "echo existing"}]
            }]
        }
    });

    let merged = merge_hooks(existing, Path::new("/path/to/deva-light-hook")).unwrap();
    let session_start = merged["hooks"]["SessionStart"].as_array().unwrap();

    assert_eq!(session_start.len(), 2);
    assert!(session_start
        .iter()
        .any(|entry| entry["hooks"][0]["command"] == "echo existing"));
    assert!(session_start
        .iter()
        .any(|entry| entry["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("deva-light-hook")));
}

#[test]
fn merge_hooks_replaces_existing_ai_light_hooks_for_same_event() {
    let existing = json!({
        "hooks": {
            "SessionStart": [{
                "matcher": "",
                "hooks": [{"type": "command", "command": "/old/deva-light-hook session-start"}]
            }]
        }
    });

    let merged = merge_hooks(existing, Path::new("/new/deva-light-hook")).unwrap();
    let session_start = merged["hooks"]["SessionStart"].as_array().unwrap();

    assert_eq!(session_start.len(), 1);
    assert_eq!(
        session_start[0]["hooks"][0]["command"],
        "/new/deva-light-hook"
    );
    assert_eq!(session_start[0]["hooks"][0]["args"][0], "session-start");
}

#[test]
fn merge_hooks_writes_event_as_args_to_avoid_shell_parsing() {
    let hook_path = Path::new(r"C:\Users\kemp\.ai_light\bin\deva-light-hook.exe");
    let merged = merge_hooks(json!({}), hook_path).unwrap();
    let hook = &merged["hooks"]["Notification"][0]["hooks"][0];

    assert_eq!(
        hook["command"],
        r"C:\Users\kemp\.ai_light\bin\deva-light-hook.exe"
    );
    assert_eq!(hook["args"][0], "notification");
}

#[test]
fn merge_wsl_hooks_embeds_ai_light_url_and_wsl_command() {
    let merged = merge_wsl_hooks(
        json!({}),
        "/mnt/c/Users/Admin/.deva_light/bin/deva-light-hook.exe",
        55_548,
        Some("secret-token"),
    )
    .unwrap();
    let hook = &merged["hooks"]["Notification"][0]["hooks"][0];

    assert_eq!(
        hook["command"],
        format!(
            "{} '/mnt/c/Users/Admin/.deva_light/bin/deva-light-hook.exe' notification",
            wsl_ai_light_url_prefix(55_548, Some("secret-token"))
        )
    );
}

#[test]
fn wsl_ai_light_url_prefix_resolves_windows_host_at_runtime() {
    let prefix = wsl_ai_light_url_prefix(17_321, None);
    assert!(prefix.contains("nameserver"));
    assert!(prefix.contains("17321/events"));
    assert!(prefix.contains("127.0.0.1"));
}

#[test]
fn windows_hook_path_can_be_projected_into_wsl_mounts() {
    let wsl_path = windows_path_to_wsl_path(Path::new(
        r"C:\Users\Admin\.deva_light\bin\deva-light-hook.exe",
    ));

    assert_eq!(
        wsl_path.as_deref(),
        Some("/mnt/c/Users/Admin/.deva_light/bin/deva-light-hook.exe")
    );
}

#[test]
fn remove_ai_light_hooks_preserves_other_hooks_and_settings() {
    let existing = json!({
        "hooks": {
            "SessionStart": [
                {
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "/old/deva-light-hook session-start"}]
                },
                {
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "echo existing"}]
                }
            ],
            "Stop": [{
                "matcher": "",
                "hooks": [{"type": "command", "command": "/old/deva-light-hook stop"}]
            }]
        },
        "theme": "dark"
    });

    let cleaned = remove_ai_light_hooks(existing).unwrap();

    assert_eq!(cleaned["theme"], "dark");
    assert_eq!(
        cleaned["hooks"]["SessionStart"][0]["hooks"][0]["command"],
        "echo existing"
    );
    assert!(cleaned["hooks"].get("Stop").is_none());
}

#[test]
fn merge_hooks_rejects_non_object_hooks_field() {
    let result = merge_hooks(json!({"hooks": []}), Path::new("/path/to/deva-light-hook"));

    assert!(result.is_err());
}

#[test]
fn merge_hooks_rejects_non_array_event_field() {
    let result = merge_hooks(
        json!({"hooks": {"SessionStart": {}}}),
        Path::new("/path/to/deva-light-hook"),
    );

    assert!(result.is_err());
}

#[test]
fn hook_binary_current_compares_file_content() {
    let dir = std::env::temp_dir().join(unique_name("deva-light-hook-current"));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source-hook");
    let destination = dir.join("destination-hook");

    std::fs::write(&source, "same-size-a").unwrap();
    std::fs::write(&destination, "same-size-b").unwrap();
    assert!(!hook_binary_is_current(&source, &destination).unwrap());

    std::fs::write(&destination, "same-size-a").unwrap();
    assert!(hook_binary_is_current(&source, &destination).unwrap());

    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn auto_install_claude_hooks_requires_existing_claude_dir_or_settings() {
    let missing = Path::new("/tmp/deva-light-missing-claude/settings.json");
    assert!(!claude_hooks_can_auto_install_for_test(missing));

    let existing_file = std::env::temp_dir()
        .join(unique_name("deva-light-claude-settings"))
        .join("settings.json");
    std::fs::create_dir_all(existing_file.parent().unwrap()).unwrap();
    std::fs::write(&existing_file, "{}").unwrap();
    assert!(claude_hooks_can_auto_install_for_test(&existing_file));
    std::fs::remove_file(&existing_file).unwrap();
    std::fs::remove_dir_all(existing_file.parent().unwrap()).unwrap();

    let existing_parent = std::env::temp_dir()
        .join(unique_name("deva-light-claude-dir"))
        .join("settings.json");
    std::fs::create_dir_all(existing_parent.parent().unwrap()).unwrap();
    assert!(claude_hooks_can_auto_install_for_test(&existing_parent));
    std::fs::remove_dir_all(existing_parent.parent().unwrap()).unwrap();
}

fn unique_name(prefix: &str) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    format!("{prefix}-{nanos}")
}

fn claude_hooks_can_auto_install_for_test(path: &Path) -> bool {
    let parent_exists = path.parent().is_some_and(|parent| parent.exists());
    path.exists() || parent_exists
}
