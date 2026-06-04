use crate::config::get_log_path;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

pub fn append_log(message: &str) -> io::Result<()> {
    let log_path = get_log_path();
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let timestamp = unix_timestamp();

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    writeln!(file, "{timestamp} {message}")
}

pub fn append_component_log(
    component: &str,
    level: LogLevel,
    message: impl AsRef<str>,
) -> io::Result<()> {
    append_log(&format!(
        "[{}] {}: {}",
        level_name(level),
        component,
        message.as_ref()
    ))
}

pub fn log_info(component: &str, message: impl AsRef<str>) {
    let _ = append_component_log(component, LogLevel::Info, message);
}

pub fn log_warn(component: &str, message: impl AsRef<str>) {
    let _ = append_component_log(component, LogLevel::Warn, message);
}

pub fn log_error(component: &str, message: impl AsRef<str>) {
    let _ = append_component_log(component, LogLevel::Error, message);
}

fn level_name(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    }
}

fn unix_timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| format!("{}.{:03}", duration.as_secs(), duration.subsec_millis()))
        .unwrap_or_else(|_| "0.000".to_string())
}
