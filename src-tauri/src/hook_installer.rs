use crate::logging::{log_info, log_warn};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const HOOK_EVENTS: [(&str, &str); 8] = [
    ("SessionStart", "session-start"),
    ("UserPromptSubmit", "prompt-submit"),
    ("PreToolUse", "pre-tool-use"),
    ("PermissionRequest", "permission-request"),
    ("PostToolUse", "post-tool-use"),
    ("Notification", "notification"),
    ("Stop", "stop"),
    ("SessionEnd", "session-end"),
];

pub fn get_claude_settings_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("settings.json")
}

pub fn get_hook_binary_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".deva_light")
        .join("bin")
        .join(hook_binary_name())
}

pub fn install_hook_binary_from_resource(resource_dir: &Path) -> Result<bool, std::io::Error> {
    let Some(source) = bundled_hook_candidates(resource_dir)
        .into_iter()
        .find(|path| path.exists())
    else {
        log_warn(
            "hook_installer",
            "bundled hook helper not found in resources",
        );
        return Ok(false);
    };

    let destination = get_hook_binary_path();
    if hook_binary_is_current(&source, &destination)? {
        return Ok(false);
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::copy(source, destination)?;
    log_info(
        "hook_installer",
        "copied bundled hook helper into config directory",
    );
    Ok(true)
}

pub fn merge_hooks(mut existing: Value, hook_path: &Path) -> Result<Value, String> {
    if !existing.is_object() {
        return Err("settings root must be a JSON object".to_string());
    }

    if existing.get("hooks").is_none() {
        existing["hooks"] = json!({});
    }

    let hooks = existing
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "settings hooks field must be a JSON object".to_string())?;

    let command_path = hook_path.to_string_lossy().to_string();

    for (claude_event, hook_event) in HOOK_EVENTS {
        let event_hooks = hooks
            .entry(claude_event.to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| format!("settings hooks.{claude_event} field must be an array"))?;

        remove_existing_ai_light_hooks(event_hooks);
        event_hooks.push(json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": command_path.clone(),
                "args": [hook_event]
            }]
        }));
    }

    Ok(existing)
}

pub fn install_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let settings_path = get_claude_settings_path();
    let hook_path = get_hook_binary_path();

    if !hook_path.exists() {
        return Err(format!("hook binary not found: {}", hook_path.display()).into());
    }

    let existing = if settings_path.exists() {
        read_settings_json(&settings_path)?
    } else {
        json!({})
    };

    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if settings_path.exists() {
        fs::copy(&settings_path, settings_path.with_extension("json.bak"))?;
    }

    let merged = merge_hooks(existing, &hook_path)?;
    fs::write(settings_path, serde_json::to_string_pretty(&merged)?)?;
    log_info("hook_installer", "installed Claude hooks");

    Ok(())
}

pub fn remove_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let settings_path = get_claude_settings_path();
    if settings_path.exists() {
        let existing = read_settings_json(&settings_path)?;
        let cleaned = remove_ai_light_hooks(existing)?;

        fs::copy(
            &settings_path,
            settings_path.with_extension("json.ai-light-remove.bak"),
        )?;
        fs::write(&settings_path, serde_json::to_string_pretty(&cleaned)?)?;
    }

    let hook_path = get_hook_binary_path();
    if hook_path.exists() {
        fs::remove_file(hook_path)?;
    }

    log_info("hook_installer", "removed Claude hooks and helper binary");

    Ok(())
}

pub fn preview_hook_config() -> Result<String, String> {
    let existing = if get_claude_settings_path().exists() {
        read_settings_json(&get_claude_settings_path()).map_err(|e| e.to_string())?
    } else {
        json!({})
    };

    let merged = merge_hooks(existing, &get_hook_binary_path())?;
    serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())
}

pub fn check_hooks_installed() -> bool {
    let Ok(content) = fs::read_to_string(get_claude_settings_path()) else {
        return false;
    };

    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let Ok(settings) = serde_json::from_str::<Value>(content) else {
        return false;
    };

    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return false;
    };

    HOOK_EVENTS.iter().all(|(claude_event, hook_event)| {
        hooks
            .get(*claude_event)
            .and_then(Value::as_array)
            .is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| contains_ai_light_hook_for_event(entry, hook_event))
            })
    })
}

pub fn remove_ai_light_hooks(mut existing: Value) -> Result<Value, String> {
    if !existing.is_object() {
        return Err("settings root must be a JSON object".to_string());
    }

    let Some(hooks) = existing.get_mut("hooks") else {
        return Ok(existing);
    };

    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| "settings hooks field must be a JSON object".to_string())?;
    let event_names: Vec<_> = hooks.keys().cloned().collect();

    for event_name in event_names {
        let Some(event_hooks) = hooks.get_mut(&event_name).and_then(Value::as_array_mut) else {
            continue;
        };

        remove_existing_ai_light_hooks(event_hooks);

        if event_hooks.is_empty() {
            hooks.remove(&event_name);
        }
    }

    Ok(existing)
}

fn hook_binary_name() -> &'static str {
    if cfg!(windows) {
        "deva-light-hook.exe"
    } else {
        "deva-light-hook"
    }
}

fn bundled_hook_candidates(resource_dir: &Path) -> Vec<PathBuf> {
    vec![
        resource_dir.join(hook_binary_name()),
        resource_dir.join("deva-light-hook.exe"),
        resource_dir.join("deva-light-hook"),
    ]
}

fn remove_existing_ai_light_hooks(event_hooks: &mut Vec<Value>) {
    event_hooks.retain(|entry| !entry_contains_ai_light_hook(entry));
}

fn entry_contains_ai_light_hook(entry: &Value) -> bool {
    let Some(commands) = entry.get("hooks").and_then(Value::as_array) else {
        return false;
    };

    commands.iter().any(|command| {
        command
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains("deva-light-hook"))
    })
}

fn contains_ai_light_hook_for_event(entry: &Value, hook_event: &str) -> bool {
    let Some(commands) = entry.get("hooks").and_then(Value::as_array) else {
        return false;
    };

    commands.iter().any(|command| {
        let command_matches = command
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains(hook_binary_name()));

        if !command_matches {
            return false;
        }

        command
            .get("args")
            .and_then(Value::as_array)
            .is_some_and(|args| args.iter().any(|arg| arg.as_str() == Some(hook_event)))
            || command
                .get("command")
                .and_then(Value::as_str)
                .is_some_and(|command| command.contains(hook_event))
    })
}

pub fn hook_binary_is_current(source: &Path, destination: &Path) -> Result<bool, std::io::Error> {
    if !destination.exists() {
        return Ok(false);
    }

    Ok(fs::read(source)? == fs::read(destination)?)
}

fn read_settings_json(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    Ok(serde_json::from_str(content)?)
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}
