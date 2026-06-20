use crate::config::{get_config_dir, load_app_config, AppConfig};
use crate::logging::{log_info, log_warn};
use serde::Serialize;
use std::fs;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const DEFAULT_CACHE_RETENTION_DAYS: u16 = 30;
pub const CLEANUP_INTERVAL: Duration = Duration::from_secs(12 * 60 * 60);

const LOG_FILE_NAMES: [&str; 3] = ["deva-light.log", "ai-light.log", "hook.log"];

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CacheCleanupReport {
    pub retention_days: u16,
    pub log_files_trimmed: usize,
    pub log_lines_removed: usize,
    pub files_removed: usize,
    pub directories_removed: usize,
}

impl CacheCleanupReport {
    pub fn changed(&self) -> bool {
        self.log_files_trimmed > 0
            || self.log_lines_removed > 0
            || self.files_removed > 0
            || self.directories_removed > 0
    }
}

pub fn cache_retention(config: &AppConfig) -> Option<Duration> {
    config
        .auto_cleanup_stale_cache
        .then_some(Duration::from_secs(
            DEFAULT_CACHE_RETENTION_DAYS as u64 * 24 * 60 * 60,
        ))
}

pub fn cleanup_if_enabled() -> io::Result<Option<CacheCleanupReport>> {
    let config = load_app_config();
    let Some(retention) = cache_retention(&config) else {
        return Ok(None);
    };

    let report = cleanup_config_dir(&get_config_dir(), retention, SystemTime::now())?;
    if report.changed() {
        log_info(
            "cache_cleanup",
            format!(
                "trimmed {} log files, removed {} old log lines, deleted {} stale files and {} directories",
                report.log_files_trimmed,
                report.log_lines_removed,
                report.files_removed,
                report.directories_removed
            ),
        );
    }
    Ok(Some(report))
}

pub fn cleanup_now() -> io::Result<CacheCleanupReport> {
    let report = cleanup_config_dir(
        &get_config_dir(),
        Duration::from_secs(DEFAULT_CACHE_RETENTION_DAYS as u64 * 24 * 60 * 60),
        SystemTime::now(),
    )?;

    if report.changed() {
        log_info(
            "cache_cleanup",
            format!(
                "manual cleanup trimmed {} log files, removed {} old log lines, deleted {} stale files and {} directories",
                report.log_files_trimmed,
                report.log_lines_removed,
                report.files_removed,
                report.directories_removed
            ),
        );
    }

    Ok(report)
}

fn cleanup_config_dir(
    config_dir: &Path,
    retention: Duration,
    now: SystemTime,
) -> io::Result<CacheCleanupReport> {
    let mut report = CacheCleanupReport {
        retention_days: DEFAULT_CACHE_RETENTION_DAYS,
        ..CacheCleanupReport::default()
    };

    let cutoff = now.checked_sub(retention).unwrap_or(UNIX_EPOCH);
    let cutoff_secs = cutoff
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    for file_name in LOG_FILE_NAMES {
        let removed = trim_log_file(&config_dir.join(file_name), cutoff_secs)?;
        if removed > 0 {
            report.log_files_trimmed += 1;
            report.log_lines_removed += removed;
        }
    }

    if !config_dir.exists() {
        return Ok(report);
    }

    cleanup_directory_entries(config_dir, cutoff, &mut report)?;
    Ok(report)
}

fn cleanup_directory_entries(
    dir: &Path,
    cutoff: SystemTime,
    report: &mut CacheCleanupReport,
) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            if should_skip_directory(&path) {
                continue;
            }

            cleanup_directory_entries(&path, cutoff, report)?;

            if directory_is_empty(&path)?
                && stale_by_modified_time(&metadata, cutoff)
                && is_stale_cache_directory(&path)
            {
                fs::remove_dir(&path)?;
                report.directories_removed += 1;
            }
            continue;
        }

        if should_delete_stale_file(&path) && stale_by_modified_time(&metadata, cutoff) {
            if let Err(error) = fs::remove_file(&path) {
                log_warn(
                    "cache_cleanup",
                    format!("failed to remove stale file {}: {error}", path.display()),
                );
            } else {
                report.files_removed += 1;
            }
        }
    }

    Ok(())
}

fn directory_is_empty(path: &Path) -> io::Result<bool> {
    Ok(fs::read_dir(path)?.next().is_none())
}

fn stale_by_modified_time(metadata: &fs::Metadata, cutoff: SystemTime) -> bool {
    metadata
        .modified()
        .map(|modified| modified < cutoff)
        .unwrap_or(false)
}

fn should_skip_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "bin"))
}

fn is_stale_cache_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "cache" | "tmp" | "temp" | "updates"))
}

fn should_delete_stale_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };

    if matches!(name, "config.json") {
        return false;
    }

    if LOG_FILE_NAMES.contains(&name) {
        return false;
    }

    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|ext| matches!(ext, "bak" | "tmp" | "old" | "part"))
}

fn trim_log_file(path: &Path, cutoff_secs: u64) -> io::Result<usize> {
    if !path.exists() {
        return Ok(0);
    }

    let source = fs::File::open(path)?;
    let reader = BufReader::new(source);
    let temp_path = log_temp_path(path);
    let mut writer = BufWriter::new(fs::File::create(&temp_path)?);

    let mut removed = 0usize;

    for line in reader.lines() {
        let line = line?;
        if log_line_is_stale(&line, cutoff_secs) {
            removed += 1;
            continue;
        }

        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
    }

    writer.flush()?;
    drop(writer);

    if removed == 0 {
        let _ = fs::remove_file(temp_path);
        return Ok(0);
    }

    if let Err(error) = replace_file(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return Err(error);
    }

    Ok(removed)
}

fn replace_file(temp_path: &Path, destination: &Path) -> io::Result<()> {
    match fs::rename(temp_path, destination) {
        Ok(()) => Ok(()),
        Err(_) => {
            let _ = fs::remove_file(destination);
            fs::rename(temp_path, destination)
        }
    }
}

fn log_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("log");
    path.with_file_name(format!("{file_name}.cleanup.tmp"))
}

fn log_line_is_stale(line: &str, cutoff_secs: u64) -> bool {
    extract_log_timestamp_secs(line).is_some_and(|secs| secs < cutoff_secs)
}

fn extract_log_timestamp_secs(line: &str) -> Option<u64> {
    let trimmed = line.trim_start();

    if let Some(bracketed) = trimmed.strip_prefix('[') {
        let end = bracketed.find(']')?;
        return bracketed[..end].trim().parse::<u64>().ok();
    }

    let token = trimmed.split_whitespace().next()?;
    let token = token.split('.').next().unwrap_or(token);
    token.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use filetime::{set_file_mtime, FileTime};

    #[test]
    fn trims_old_lines_from_deva_and_hook_logs() {
        let dir = std::env::temp_dir().join(unique_name("deva-light-cleanup-logs"));
        fs::create_dir_all(&dir).unwrap();

        let now = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        let retention = Duration::from_secs(DEFAULT_CACHE_RETENTION_DAYS as u64 * 24 * 60 * 60);
        let cutoff_secs = now
            .checked_sub(retention)
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        fs::write(
            dir.join("deva-light.log"),
            format!(
                "{} [INFO] old line\n{} [INFO] recent line\n",
                cutoff_secs - 1,
                cutoff_secs + 1
            ),
        )
        .unwrap();
        fs::write(
            dir.join("hook.log"),
            format!(
                "[{}] sent: old\n[{}] sent: recent\n",
                cutoff_secs - 10,
                cutoff_secs + 10
            ),
        )
        .unwrap();

        let report = cleanup_config_dir(&dir, retention, now).unwrap();
        assert_eq!(report.log_files_trimmed, 2);
        assert_eq!(report.log_lines_removed, 2);
        assert_eq!(
            fs::read_to_string(dir.join("deva-light.log")).unwrap(),
            format!("{} [INFO] recent line\n", cutoff_secs + 1)
        );
        assert_eq!(
            fs::read_to_string(dir.join("hook.log")).unwrap(),
            format!("[{}] sent: recent\n", cutoff_secs + 10)
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_runtime_and_lock_files_but_removes_stale_backups() {
        let dir = std::env::temp_dir().join(unique_name("deva-light-cleanup-files"));
        let bin_dir = dir.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();

        let now = SystemTime::now();
        let retention = Duration::from_secs(DEFAULT_CACHE_RETENTION_DAYS as u64 * 24 * 60 * 60);
        let old_time = FileTime::from_system_time(now - retention - Duration::from_secs(60));

        let runtime = dir.join("runtime.json");
        let lock = dir.join("deva-light.lock");
        let backup = dir.join("old.json.bak");
        let config = dir.join("config.json");
        let hook_binary = bin_dir.join("deva-light-hook.exe");

        fs::write(&runtime, "{}").unwrap();
        fs::write(&lock, "").unwrap();
        fs::write(&backup, "backup").unwrap();
        fs::write(&config, "{\"http_bind\":\"127.0.0.1\"}").unwrap();
        fs::write(&hook_binary, "binary").unwrap();

        set_file_mtime(&runtime, old_time).unwrap();
        set_file_mtime(&lock, old_time).unwrap();
        set_file_mtime(&backup, old_time).unwrap();
        set_file_mtime(&config, old_time).unwrap();
        set_file_mtime(&hook_binary, old_time).unwrap();

        let report = cleanup_config_dir(&dir, retention, now).unwrap();
        assert_eq!(report.files_removed, 1);
        assert!(runtime.exists());
        assert!(lock.exists());
        assert!(!backup.exists());
        assert!(config.exists());
        assert!(hook_binary.exists());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ignores_unparseable_log_lines() {
        let dir = std::env::temp_dir().join(unique_name("deva-light-cleanup-unparseable"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("deva-light.log"), "no timestamp here\n").unwrap();

        let report = cleanup_config_dir(
            &dir,
            Duration::from_secs(DEFAULT_CACHE_RETENTION_DAYS as u64 * 24 * 60 * 60),
            UNIX_EPOCH + Duration::from_secs(2_000_000_000),
        )
        .unwrap();

        assert_eq!(report.log_files_trimmed, 0);
        assert_eq!(
            fs::read_to_string(dir.join("deva-light.log")).unwrap(),
            "no timestamp here\n"
        );

        let _ = fs::remove_dir_all(dir);
    }

    fn unique_name(prefix: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("{prefix}-{nanos}")
    }
}
