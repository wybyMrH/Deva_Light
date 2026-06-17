use crate::config::{apply_configured_proxy_to_env, load_app_config, refresh_config_cache};
use crate::logging::{log_info, log_warn};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub current_version: String,
    pub notes: String,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub phase: String,
}

fn github_updater_token() -> Option<String> {
    if let Ok(token) = std::env::var("DEVA_LIGHT_UPDATER_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(token);
        }
    }

    option_env!("DEVA_LIGHT_UPDATER_TOKEN")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}

fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    let mut builder = app.updater_builder();

    if let Some(token) = github_updater_token() {
        builder = builder
            .header("Authorization", format!("Bearer {token}"))
            .map_err(|error| error.to_string())?;
        builder = builder
            .header("X-GitHub-Api-Version", "2022-11-28")
            .map_err(|error| error.to_string())?;
    }

    builder.build().map_err(|error| error.to_string())
}

fn map_update_error(error: &str) -> String {
    if error.contains("Could not fetch a valid release JSON") {
        if github_updater_token().is_none() {
            return "无法检查更新：无法从 GitHub 获取 latest.json，请确认网络正常且 Release 已发布。".to_string();
        }

        return format!("无法检查更新：无法从 GitHub 获取 latest.json（{error}）");
    }

    error.to_string()
}

pub async fn check_for_update(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let updater = build_updater(app)?;
    let current_version = app.package_info().version.to_string();

    match updater.check().await {
        Ok(Some(update)) => Ok(Some(UpdateInfo {
            version: update.version,
            current_version,
            notes: update.body.unwrap_or_default(),
            date: update.date.map(|value| value.to_string()),
        })),
        Ok(None) => Ok(None),
        Err(error) => Err(map_update_error(&error.to_string())),
    }
}

pub async fn download_and_install(app: &AppHandle) -> Result<(), String> {
    log_info("updater", "starting download and install");
    let updater = build_updater(app)?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|error| map_update_error(&error.to_string()))?
    else {
        return Err("当前已是最新版本".to_string());
    };

    install_update(app, update).await
}

async fn install_update(
    app: &AppHandle,
    update: tauri_plugin_updater::Update,
) -> Result<(), String> {
    log_info(
        "updater",
        format!(
            "downloading update {} (current {})",
            update.version,
            app.package_info().version
        ),
    );

    let app_handle = app.clone();
    let downloaded = Arc::new(AtomicU64::new(0));
    let downloaded_finish = Arc::clone(&downloaded);
    let app_finish = app.clone();

    update
        .download_and_install(
            move |chunk_length, content_length| {
                let total = downloaded.fetch_add(chunk_length as u64, Ordering::Relaxed)
                    + chunk_length as u64;
                let _ = app_handle.emit(
                    "update-download-progress",
                    UpdateProgress {
                        downloaded: total,
                        total: content_length,
                        phase: "downloading".to_string(),
                    },
                );
            },
            move || {
                let _ = app_finish.emit(
                    "update-download-progress",
                    UpdateProgress {
                        downloaded: downloaded_finish.load(Ordering::Relaxed),
                        total: None,
                        phase: "installing".to_string(),
                    },
                );
            },
        )
        .await
        .map_err(|error| {
            let message = map_update_error(&error.to_string());
            log_warn("updater", format!("download/install failed: {message}"));
            message
        })?;

    log_info("updater", "download/install finished, restarting app");
    app.restart();
}

async fn try_silent_update(app: &AppHandle) -> Result<bool, String> {
    if !load_app_config().auto_update_enabled {
        return Ok(false);
    }

    let updater = build_updater(app)?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|error| map_update_error(&error.to_string()))?
    else {
        return Ok(false);
    };

    log_info(
        "updater",
        format!(
            "silent update installing {} (current {})",
            update.version,
            app.package_info().version
        ),
    );

    let _ = app
        .notification()
        .builder()
        .title("Deva Light")
        .body(format!("正在更新到 v{}…", update.version))
        .show();

    update
        .download_and_install(|_chunk_length, _content_length| {}, || {})
        .await
        .map_err(|error| map_update_error(&error.to_string()))?;

    log_info("updater", "silent update finished, restarting app");
    app.restart();
    #[allow(unreachable_code)]
    Ok(true)
}

pub fn spawn_auto_update_service(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(6)).await;

        loop {
            refresh_config_cache();
            apply_configured_proxy_to_env();

            let auto_update_enabled = load_app_config().auto_update_enabled;

            if auto_update_enabled {
                match try_silent_update(&handle).await {
                    Ok(true) => return,
                    Ok(false) => {}
                    Err(error) => {
                        log_warn("updater", format!("silent update failed: {error}"));
                    }
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(30 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_release_json_fetch_error() {
        let message = map_update_error("Could not fetch a valid release JSON from the remote");
        assert!(message.starts_with("无法检查更新："));
        assert!(
            message.contains("latest.json"),
            "unexpected message: {message}"
        );
    }
}
