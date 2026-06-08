#[cfg(target_os = "windows")]
use crate::codex_paths::{parse_wsl_distro_list, path_from_console_output, run_wsl_command};
#[cfg(target_os = "windows")]
use crate::config::{load_app_config, load_runtime_config};
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

const CURSOR_HOOK_EVENTS: [&str; 17] = [
    "sessionStart",
    "sessionEnd",
    "beforeSubmitPrompt",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "beforeShellExecution",
    "afterShellExecution",
    "beforeMCPExecution",
    "afterMCPExecution",
    "beforeReadFile",
    "subagentStart",
    "subagentStop",
    "stop",
    "afterAgentResponse",
    "afterAgentThought",
    "afterFileEdit",
];

pub fn get_claude_settings_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("settings.json")
}

pub fn get_cursor_hooks_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cursor")
        .join("hooks.json")
}

pub fn get_hook_binary_path() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".deva_light")
        .join("bin")
        .join(hook_binary_name())
}

fn set_hook_binary_executable(path: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
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

    fs::copy(source, &destination)?;
    set_hook_binary_executable(&destination)?;
    log_info(
        "hook_installer",
        "copied bundled hook helper into config directory",
    );
    Ok(true)
}

pub fn merge_cursor_hooks(mut existing: Value, hook_path: &Path) -> Result<Value, String> {
    if !existing.is_object() {
        return Err("hooks root must be a JSON object".to_string());
    }

    existing["version"] = json!(1);
    if existing.get("hooks").is_none() {
        existing["hooks"] = json!({});
    }

    let hooks = existing
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "hooks field must be a JSON object".to_string())?;

    let command = quote_command(hook_path);

    for event_name in CURSOR_HOOK_EVENTS {
        let event_hooks = hooks
            .entry(event_name.to_string())
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| format!("hooks.{event_name} field must be an array"))?;

        remove_cursor_ai_light_hook_entries(event_hooks);
        event_hooks.push(json!({ "command": command.clone() }));
    }

    Ok(existing)
}

pub fn remove_cursor_ai_light_hooks(mut existing: Value) -> Result<Value, String> {
    if !existing.is_object() {
        return Err("hooks root must be a JSON object".to_string());
    }

    let Some(hooks) = existing.get_mut("hooks") else {
        return Ok(existing);
    };

    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| "hooks field must be a JSON object".to_string())?;
    let event_names: Vec<_> = hooks.keys().cloned().collect();

    for event_name in event_names {
        let Some(event_hooks) = hooks.get_mut(&event_name).and_then(Value::as_array_mut) else {
            continue;
        };

        remove_cursor_ai_light_hook_entries(event_hooks);

        if event_hooks.is_empty() {
            hooks.remove(&event_name);
        }
    }

    Ok(existing)
}

fn remove_cursor_ai_light_hook_entries(event_hooks: &mut Vec<Value>) {
    event_hooks.retain(|entry| !cursor_entry_contains_ai_light_hook(entry));
}

fn cursor_entry_contains_ai_light_hook(entry: &Value) -> bool {
    entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains("deva-light-hook"))
}

fn quote_command(path: &Path) -> String {
    let command = path.to_string_lossy();
    if command.contains(' ') {
        format!("\"{command}\"")
    } else {
        command.to_string()
    }
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
    install_claude_hooks()?;

    #[cfg(target_os = "windows")]
    install_wsl_hooks()?;

    Ok(())
}

pub fn install_claude_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let settings_path = get_claude_settings_path();
    let hook_path = get_hook_binary_path();

    if !hook_path.exists() {
        return Err(format!("hook binary not found: {}", hook_path.display()).into());
    }

    set_hook_binary_executable(&hook_path)?;

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

pub fn install_cursor_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let hooks_path = get_cursor_hooks_path();
    let hook_path = get_hook_binary_path();

    if !hook_path.exists() {
        return Err(format!("hook binary not found: {}", hook_path.display()).into());
    }

    set_hook_binary_executable(&hook_path)?;

    let existing = if hooks_path.exists() {
        read_settings_json(&hooks_path)?
    } else {
        json!({})
    };

    if let Some(parent) = hooks_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if hooks_path.exists() {
        fs::copy(&hooks_path, hooks_path.with_extension("json.bak"))?;
    }

    let merged = merge_cursor_hooks(existing, &hook_path)?;
    fs::write(hooks_path, serde_json::to_string_pretty(&merged)?)?;
    log_info("hook_installer", "installed Cursor hooks");
    Ok(())
}

pub fn refresh_wsl_hooks() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        return install_wsl_hooks();
    }

    #[cfg(not(target_os = "windows"))]
    Ok(())
}

pub fn remove_hooks() -> Result<(), Box<dyn std::error::Error>> {
    remove_claude_hooks()?;
    remove_cursor_hooks()?;

    let hook_path = get_hook_binary_path();
    if hook_path.exists() {
        fs::remove_file(hook_path)?;
    }

    log_info(
        "hook_installer",
        "removed Claude/Cursor hooks and helper binary",
    );
    Ok(())
}

pub fn remove_claude_hooks() -> Result<(), Box<dyn std::error::Error>> {
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

    #[cfg(target_os = "windows")]
    remove_wsl_hooks()?;

    Ok(())
}

pub fn remove_cursor_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let hooks_path = get_cursor_hooks_path();
    if !hooks_path.exists() {
        return Ok(());
    }

    let existing = read_settings_json(&hooks_path)?;
    let cleaned = remove_cursor_ai_light_hooks(existing)?;
    fs::copy(
        &hooks_path,
        hooks_path.with_extension("json.ai-light-remove.bak"),
    )?;
    fs::write(hooks_path, serde_json::to_string_pretty(&cleaned)?)?;
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

#[cfg(target_os = "windows")]
fn install_wsl_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let Some(runtime) = load_runtime_config() else {
        log_warn(
            "hook_installer",
            "skipped WSL Claude hook install because runtime.json is unavailable",
        );
        return Ok(());
    };
    let app_config = load_app_config();
    let http_token = runtime
        .http_token
        .as_deref()
        .or(app_config.http_token.as_deref());

    let Some(wsl_hook_path) = windows_path_to_wsl_path(&get_hook_binary_path()) else {
        log_warn(
            "hook_installer",
            "skipped WSL Claude hook install because hook path could not be converted to a WSL path",
        );
        return Ok(());
    };

    let mut installed = 0usize;

    for distro in parse_wsl_distro_list(&run_wsl_command(&["--list", "--quiet"])) {
        let Some(settings_path) = wsl_claude_settings_path(&distro) else {
            continue;
        };

        let existing = if settings_path.exists() {
            read_settings_json(&settings_path)?
        } else {
            json!({})
        };

        if let Some(parent) = settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if settings_path.exists() {
            let backup_path = settings_path.with_extension("json.deva-light.bak");
            let _ = fs::copy(&settings_path, backup_path);
        }

        let merged = merge_wsl_hooks(existing, &wsl_hook_path, runtime.http_port, http_token)?;
        fs::write(&settings_path, serde_json::to_string_pretty(&merged)?)?;
        installed += 1;
        log_info(
            "hook_installer",
            format!(
                "installed WSL Claude hooks for distro {} at {}",
                distro,
                settings_path.display()
            ),
        );
    }

    if installed == 0 {
        log_info(
            "hook_installer",
            "no WSL distros detected for Claude hook install",
        );
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn remove_wsl_hooks() -> Result<(), Box<dyn std::error::Error>> {
    let mut removed = 0usize;

    for distro in parse_wsl_distro_list(&run_wsl_command(&["--list", "--quiet"])) {
        let Some(settings_path) = wsl_claude_settings_path(&distro) else {
            continue;
        };
        if !settings_path.exists() {
            continue;
        }

        let existing = read_settings_json(&settings_path)?;
        let cleaned = remove_ai_light_hooks(existing)?;
        let backup_path = settings_path.with_extension("json.deva-light-remove.bak");
        let _ = fs::copy(&settings_path, backup_path);
        fs::write(&settings_path, serde_json::to_string_pretty(&cleaned)?)?;
        removed += 1;
        log_info(
            "hook_installer",
            format!(
                "removed WSL Claude hooks for distro {} at {}",
                distro,
                settings_path.display()
            ),
        );
    }

    if removed == 0 {
        log_info("hook_installer", "no WSL Claude hooks needed removal");
    }

    Ok(())
}

pub fn check_cursor_hooks_installed() -> bool {
    let Ok(content) = fs::read_to_string(get_cursor_hooks_path()) else {
        return false;
    };

    let content = content.strip_prefix('\u{feff}').unwrap_or(&content);
    let Ok(settings) = serde_json::from_str::<Value>(content) else {
        return false;
    };

    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return false;
    };

    CURSOR_HOOK_EVENTS.iter().all(|event_name| {
        hooks
            .get(*event_name)
            .and_then(Value::as_array)
            .is_some_and(|entries| entries.iter().any(cursor_entry_contains_ai_light_hook))
    })
}

pub fn check_hooks_installed() -> bool {
    check_claude_hooks_installed()
}

pub fn check_claude_hooks_installed() -> bool {
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

pub fn wsl_ai_light_url_prefix(port: u16, token: Option<&str>) -> String {
    let mut parts = vec![format!(
        "AI_LIGHT_URL=http://$(grep -m1 '^nameserver ' /etc/resolv.conf 2>/dev/null | awk '{{print $2}}' || echo 127.0.0.1):{port}/events"
    )];

    if let Some(token) = token.filter(|value| !value.is_empty()) {
        parts.push(format!("AI_LIGHT_TOKEN={}", sh_single_quote(token)));
    }

    parts.join(" ")
}

pub fn merge_wsl_hooks(
    existing: Value,
    hook_path: &str,
    http_port: u16,
    http_token: Option<&str>,
) -> Result<Value, String> {
    if !existing.is_object() {
        return Err("settings root must be a JSON object".to_string());
    }

    let mut existing = existing;
    if existing.get("hooks").is_none() {
        existing["hooks"] = json!({});
    }

    let hooks = existing
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "settings hooks field must be a JSON object".to_string())?;

    let command_prefix = format!(
        "{} {}",
        wsl_ai_light_url_prefix(http_port, http_token),
        sh_single_quote(hook_path)
    );

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
                "command": format!("{command_prefix} {hook_event}"),
            }]
        }));
    }

    Ok(existing)
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

pub fn windows_path_to_wsl_path(path: &Path) -> Option<String> {
    let raw = path.to_string_lossy();
    let (drive, rest) = raw.split_once(':')?;
    if drive.len() != 1 {
        return None;
    }

    let drive = drive.chars().next()?.to_ascii_lowercase();
    let rest = rest.trim_start_matches(['\\', '/']);
    let rest = rest.replace('\\', "/");
    Some(format!("/mnt/{drive}/{rest}"))
}

#[cfg(target_os = "windows")]
fn wsl_claude_settings_path(distro: &str) -> Option<PathBuf> {
    path_from_console_output(&run_wsl_command(&[
        "-d",
        distro,
        "-e",
        "sh",
        "-lc",
        r#"wslpath -w "$HOME/.claude/settings.json""#,
    ]))
}

fn sh_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'"'"'"#))
}
