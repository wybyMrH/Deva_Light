pub fn configure_main_window_workspace(window: &tauri::WebviewWindow) {
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.set_skip_taskbar(true);
    configure_macos_all_spaces(window, false);
}

pub fn apply_main_window_pin(window: &tauri::WebviewWindow, pin_window: bool) -> tauri::Result<()> {
    window.set_always_on_top(pin_window)?;
    let _ = window.set_visible_on_all_workspaces(true);
    let _ = window.set_skip_taskbar(true);
    configure_macos_all_spaces(window, pin_window);
    Ok(())
}

#[cfg(target_os = "macos")]
fn configure_macos_all_spaces(window: &tauri::WebviewWindow, pin_window: bool) {
    use objc2_app_kit::{NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior};

    let Ok(ns_window) = window.ns_window() else {
        return;
    };

    unsafe {
        let ns_window = &*(ns_window.cast::<NSWindow>());
        let mut behavior = ns_window.collectionBehavior();
        behavior.remove(
            NSWindowCollectionBehavior::MoveToActiveSpace
                | NSWindowCollectionBehavior::Managed
                | NSWindowCollectionBehavior::Transient
                | NSWindowCollectionBehavior::ParticipatesInCycle
                | NSWindowCollectionBehavior::FullScreenPrimary
                | NSWindowCollectionBehavior::FullScreenNone,
        );
        behavior.insert(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        ns_window.setCollectionBehavior(behavior);
        if pin_window {
            ns_window.setLevel(NSStatusWindowLevel);
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn configure_macos_all_spaces(_window: &tauri::WebviewWindow, _pin_window: bool) {}
