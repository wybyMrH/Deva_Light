use crate::config::{load_app_config, save_app_config};

pub fn is_monitoring_paused() -> bool {
    load_app_config().monitoring_paused
}

pub fn set_monitoring_paused(paused: bool) {
    let mut config = load_app_config();
    config.monitoring_paused = paused;
    let _ = save_app_config(&config);
}
