//! System tray integration for Lumen desktop

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub fn setup_tray(app: &AppHandle) -> Result<tauri::tray::TrayIcon, Box<dyn std::error::Error>> {
    let open_i = MenuItemBuilder::with_id("open", "Open Lumen").build(app)?;
    let quit_i = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&open_i)
        .separator()
        .item(&quit_i)
        .build()?;

    let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/trayicon.png"))?;

    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(tray_icon)
        .tooltip("Lumen")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                crate::webview_signin::cancel_active_sign_in(app);
                crate::cdp::cancel_active_browser();
                if let Some(state) = app.try_state::<crate::commands::AppState>() {
                    state.upload_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    state.scan_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    state.review_scan_cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                    if let Ok(mut guard) = state.watcher.try_lock() {
                        if let Some(ref mut w) = *guard {
                            w.stop();
                        }
                    }
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if let Ok(visible) = window.is_visible() {
                        let is_minimized = window.is_minimized().unwrap_or(false);
                        if visible && !is_minimized {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                }
            }
        })
        .build(app)?;

    Ok(tray)
}
