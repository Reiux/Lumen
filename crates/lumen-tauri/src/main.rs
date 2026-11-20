#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};
use tracing_subscriber::EnvFilter;

use lumen_core::album::AlbumRegistry;
use lumen_core::config::{
    determine_album_registry_path, determine_config_path, determine_hash_cache_path, ConfigManager,
};
use lumen_core::dedup::HashCache;
use lumen_core::keyring::KeyringStorage;

mod cdp;
mod commands;
mod matugen;
mod models;
mod tray;
mod webview_signin;

use commands::AppState;

#[cfg(windows)]
const WINDOWS_NOTIFICATION_ICON_PNG: &[u8] = include_bytes!("../icons/128x128.png");

#[cfg(windows)]
fn to_reg_expand_sz(s: &str) -> winreg::RegValue<'static> {
    let mut bytes: Vec<u8> = s
        .encode_utf16()
        .chain(std::iter::once(0u16))
        .flat_map(|u| u.to_le_bytes())
        .collect();
    if bytes.is_empty() {
        bytes.extend_from_slice(&[0, 0]);
    }
    winreg::RegValue {
        bytes: bytes.into(),
        vtype: winreg::enums::REG_EXPAND_SZ,
    }
}

#[cfg(windows)]
fn set_process_aumid(aumid: &str) {
    #[link(name = "shell32")]
    extern "system" {
        fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
    }
    let wide: Vec<u16> = aumid.encode_utf16().chain(std::iter::once(0u16)).collect();
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(wide.as_ptr());
    }
}

#[cfg(windows)]
fn create_aumid_shortcut_com(target_exe: &Path, shortcut_path: &Path, aumid: &str) -> bool {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct PropertyKey {
        fmtid: Guid,
        pid: u32,
    }

    #[repr(C)]
    struct PropVariant {
        vt: u16,
        w_reserved1: u16,
        w_reserved2: u16,
        w_reserved3: u16,
        data1: *const u16,
        data2: usize,
    }

    #[repr(C)]
    struct IShellLinkWVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_path: *const c_void,
        get_id_list: *const c_void,
        set_id_list: *const c_void,
        get_description: *const c_void,
        set_description: *const c_void,
        get_working_directory: *const c_void,
        set_working_directory: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        get_arguments: *const c_void,
        set_arguments: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        get_hotkey: *const c_void,
        set_hotkey: *const c_void,
        get_show_cmd: *const c_void,
        set_show_cmd: *const c_void,
        get_icon_location: *const c_void,
        set_icon_location: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32,
        set_relative_path: *const c_void,
        resolve: *const c_void,
        set_path: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
    }

    #[repr(C)]
    struct IPropertyStoreVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_count: *const c_void,
        get_at: *const c_void,
        get_value: *const c_void,
        set_value:
            unsafe extern "system" fn(*mut c_void, *const PropertyKey, *const PropVariant) -> i32,
        commit: unsafe extern "system" fn(*mut c_void) -> i32,
    }

    #[repr(C)]
    struct IPersistFileVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_class_id: *const c_void,
        is_dirty: *const c_void,
        load: *const c_void,
        save: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32,
        save_completed: *const c_void,
        get_cur_file: *const c_void,
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pv_reserved: *mut c_void, dw_co_init: u32) -> i32;
        fn CoUninitialize();
        fn CoCreateInstance(
            rclsid: *const Guid,
            p_unk_outer: *mut c_void,
            dw_cls_context: u32,
            riid: *const Guid,
            ppv: *mut *mut c_void,
        ) -> i32;
    }

    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const CLSCTX_INPROC_SERVER: u32 = 0x1;
    const VT_LPWSTR: u16 = 31;

    const CLSID_SHELL_LINK: Guid = Guid {
        data1: 0x00021401,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    const IID_ISHELL_LINK_W: Guid = Guid {
        data1: 0x000214F9,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    const IID_IPROPERTY_STORE: Guid = Guid {
        data1: 0x886D8EEB,
        data2: 0x8CF2,
        data3: 0x4446,
        data4: [0x8D, 0x02, 0xCD, 0xBA, 0x1D, 0xBD, 0xCF, 0x99],
    };
    const IID_IPERSIST_FILE: Guid = Guid {
        data1: 0x0000010B,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    const PKEY_APP_USER_MODEL_ID: PropertyKey = PropertyKey {
        fmtid: Guid {
            data1: 0x9F4C2855,
            data2: 0x9F79,
            data3: 0x4B39,
            data4: [0xA8, 0xD0, 0xE1, 0xD4, 0x2D, 0xE1, 0xD5, 0xF3],
        },
        pid: 5,
    };

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0u16)).collect()
    }

    let target_wide = to_wide(&target_exe.to_string_lossy());
    let workdir_wide = target_exe
        .parent()
        .map(|p| to_wide(&p.to_string_lossy()))
        .unwrap_or_else(|| to_wide("."));
    let shortcut_wide = to_wide(&shortcut_path.to_string_lossy());
    let aumid_wide = to_wide(aumid);
    let empty_wide = to_wide("");

    unsafe {
        let hr_init = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        let should_uninit = hr_init == 0 || hr_init == 1; // S_OK or S_FALSE

        let mut shell_link: *mut c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_SHELL_LINK,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ISHELL_LINK_W,
            &mut shell_link,
        );
        if hr < 0 || shell_link.is_null() {
            if should_uninit {
                CoUninitialize();
            }
            return false;
        }

        let sl_vtbl = &**(shell_link as *mut *const IShellLinkWVtbl);
        let mut ok = (sl_vtbl.set_path)(shell_link, target_wide.as_ptr()) >= 0;
        let _ = (sl_vtbl.set_arguments)(shell_link, empty_wide.as_ptr());
        let _ = (sl_vtbl.set_working_directory)(shell_link, workdir_wide.as_ptr());
        let _ = (sl_vtbl.set_icon_location)(shell_link, target_wide.as_ptr(), 0);

        if ok {
            let mut prop_store: *mut c_void = std::ptr::null_mut();
            if (sl_vtbl.query_interface)(shell_link, &IID_IPROPERTY_STORE, &mut prop_store) >= 0
                && !prop_store.is_null()
            {
                let ps_vtbl = &**(prop_store as *mut *const IPropertyStoreVtbl);
                let pv = PropVariant {
                    vt: VT_LPWSTR,
                    w_reserved1: 0,
                    w_reserved2: 0,
                    w_reserved3: 0,
                    data1: aumid_wide.as_ptr(),
                    data2: 0,
                };
                let hr_set = (ps_vtbl.set_value)(prop_store, &PKEY_APP_USER_MODEL_ID, &pv);
                let hr_commit = if hr_set >= 0 {
                    (ps_vtbl.commit)(prop_store)
                } else {
                    hr_set
                };
                ok = hr_set >= 0 && hr_commit >= 0;
                let _ = (ps_vtbl.release)(prop_store);
            } else {
                ok = false;
            }
        }

        let mut saved = false;
        if ok {
            let mut persist_file: *mut c_void = std::ptr::null_mut();
            if (sl_vtbl.query_interface)(shell_link, &IID_IPERSIST_FILE, &mut persist_file) >= 0
                && !persist_file.is_null()
            {
                let pf_vtbl = &**(persist_file as *mut *const IPersistFileVtbl);
                saved = (pf_vtbl.save)(persist_file, shortcut_wide.as_ptr(), 1) >= 0;
                let _ = (pf_vtbl.release)(persist_file);
            }
        }

        let _ = (sl_vtbl.release)(shell_link);
        if should_uninit {
            CoUninitialize();
        }
        saved
    }
}

#[cfg(windows)]
fn register_windows_notification_aumid(data_dir: &Path) {
    use winreg::enums::*;
    use winreg::RegKey;

    const AUMID: &str = if cfg!(debug_assertions) {
        "com.reiux.lumen.dev"
    } else {
        "com.reiux.lumen"
    };
    set_process_aumid(AUMID);

    let _ = std::fs::create_dir_all(data_dir);
    let icon_path = data_dir.join("app-icon.png");
    let _ = std::fs::write(&icon_path, WINDOWS_NOTIFICATION_ICON_PNG);
    let icon_path_str = icon_path.to_string_lossy().to_string();

    let display_name = if cfg!(debug_assertions) {
        "Lumen Dev"
    } else {
        "Lumen"
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    for target_aumid in [AUMID] {
        let aumid_subkey = format!("Software\\Classes\\AppUserModelId\\{}", target_aumid);
        let notif_subkey = format!(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Notifications\\Settings\\{}",
            target_aumid
        );

        let needs_cache_reset = match hkcu.open_subkey(&aumid_subkey) {
            Ok(existing) => {
                let vtype_mismatch = existing
                    .get_raw_value("DisplayName")
                    .map(|v| v.vtype != REG_EXPAND_SZ)
                    .unwrap_or(true)
                    || existing
                        .get_raw_value("IconUri")
                        .map(|v| v.vtype != REG_EXPAND_SZ)
                        .unwrap_or(true);
                let name_mismatch = existing
                    .get_value::<String, _>("DisplayName")
                    .map(|v| v != display_name)
                    .unwrap_or(true);
                let icon_mismatch = existing
                    .get_value::<String, _>("IconUri")
                    .map(|v| v != icon_path_str)
                    .unwrap_or(true);
                vtype_mismatch || name_mismatch || icon_mismatch
            }
            Err(_) => true,
        };

        if needs_cache_reset {
            let _ = hkcu.delete_subkey_all(&aumid_subkey);
            let _ = hkcu.delete_subkey_all(&notif_subkey);
        }

        if let Ok((key, _)) = hkcu.create_subkey(&aumid_subkey) {
            let _ = key.set_raw_value("DisplayName", &to_reg_expand_sz(display_name));
            let _ = key.set_raw_value("IconUri", &to_reg_expand_sz(&icon_path_str));
            let _ = key.set_value("IconBackgroundColor", &"FF121316");
        }
    }

    if let (Ok(current_exe), Ok(appdata)) = (std::env::current_exe(), std::env::var("APPDATA")) {
        let exe_str = current_exe.to_string_lossy();
        if !exe_str.contains("\\target\\debug\\") && !exe_str.contains("\\target\\release\\") {
            let start_menu = Path::new(&appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            let _ = std::fs::create_dir_all(&start_menu);
            let shortcut_name = if cfg!(debug_assertions) {
                "Lumen Dev.lnk"
            } else {
                "Lumen.lnk"
            };
            let lnk_path = start_menu.join(shortcut_name);
            let _ = create_aumid_shortcut_com(&current_exe, &lnk_path, AUMID);
        }
    }
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::capture();
        let msg = format!("Lumen Crash Report:\nInfo: {}\nBacktrace:\n{:?}\n", info, backtrace);
        eprintln!("{}", msg);
        let config_path = determine_config_path(None);
        if let Some(dir) = config_path.parent() {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join("crash.log"), msg);
        }
    }));

    // Linux WebKit flicker workaround matching Go app
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .try_init();

    let config_path = determine_config_path(None);
    let keyring = Arc::new(KeyringStorage::new_os());
    let config_manager = Arc::new(ConfigManager::new(config_path.clone(), keyring));

    let cache_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    #[cfg(windows)]
    register_windows_notification_aumid(cache_dir);

    let db_path = determine_hash_cache_path(cache_dir);
    let hash_cache = HashCache::open(&db_path).ok();

    let album_reg_path = determine_album_registry_path(cache_dir);
    let album_registry = AlbumRegistry::new(album_reg_path)
        .unwrap_or_else(|_| AlbumRegistry::in_memory());

    let app_state = AppState {
        config_manager: config_manager.clone(),
        hash_cache,
        album_registry,
        logs: Arc::new(std::sync::RwLock::new(Vec::new())),
        upload_cancel: Arc::new(AtomicBool::new(false)),
        upload_pause: Arc::new(AtomicBool::new(false)),
        scan_cancel: Arc::new(AtomicBool::new(false)),
        review_scan_cancel: Arc::new(AtomicBool::new(false)),
        is_uploading: Arc::new(AtomicBool::new(false)),
        account_blocked: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        pending_watch_files: Arc::new(std::sync::Mutex::new(Vec::new())),
        watcher: Arc::new(tokio::sync::Mutex::new(None)),
        inotify_error: Arc::new(std::sync::RwLock::new(String::new())),
        tray_available: Arc::new(AtomicBool::new(false)),
        initial_show_done: Arc::new(AtomicBool::new(false)),
        update_checker: Arc::new(std::sync::Mutex::new((
            String::new(),
            lumen_core::updater::UpdateChecker::new(),
        ))),
        run_seq: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        log_seq: Arc::new(std::sync::atomic::AtomicU64::new(1)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::get_accounts,
            commands::get_account_blocked_state,
            commands::set_selected,
            commands::add_credentials,
            commands::validate_and_add_credentials,
            commands::add_google_account,
            commands::remove_credentials,
            commands::get_album_name,
            commands::set_album_name,
            commands::get_album_auto_mode,
            commands::set_album_auto_mode,
            commands::get_recent_albums,
            commands::add_recent_album,
            commands::remove_recent_album,
            commands::get_album_nicknames,
            commands::get_album_nickname,
            commands::set_album_nickname,
            commands::get_watched_folders,
            commands::get_watched_folder_configs,
            commands::add_watched_folder,
            commands::add_watched_folder_with_account,
            commands::remove_watched_folder,
            commands::update_watched_folder_config,
            commands::get_watched_folder_config_for_path,
            commands::count_folder_backlog,
            commands::get_inotify_error,
            commands::set_upload_threads,
            commands::set_saver,
            commands::set_use_quota,
            commands::set_pair_live_photos,
            commands::set_skip_incomplete_live_photos,
            commands::set_update_existing_photos_to_live,
            commands::set_delete_from_host,
            commands::set_disable_unsupported_files_filter,
            commands::set_set_date_from_filename,
            commands::set_exclude_pattern,
            commands::get_exclude_pattern,
            commands::set_minimize_to_tray,
            commands::get_launch_on_startup,
            commands::set_launch_on_startup,
            commands::set_notify_on_complete,
            commands::set_use_local_hash_cache,
            commands::set_upload_speed_limit_kbps,
            commands::set_watch_folders_enabled,
            commands::set_proxy,
            commands::set_recursive,
            commands::set_force_upload,
            commands::is_account_risk_acknowledged,
            commands::set_account_risk_acknowledged,
            commands::get_hash_cache_count,
            commands::clear_hash_cache,
            commands::get_log_entries,
            commands::get_problem_logs,
            commands::clear_problem_logs,
            commands::clear_log_entries,
            commands::export_logs,
            commands::check_for_updates,
            commands::check_for_updates_with_redirect_url,
            commands::get_auto_check_updates,
            commands::set_auto_check_updates,
            commands::get_auto_update_interval_hours,
            commands::set_auto_update_interval_hours,
            commands::get_last_update_check,
            commands::set_last_update_check,
            commands::get_release_notes,
            commands::get_app_version,
            commands::is_debug_build,
            commands::is_tray_available,
            commands::start_embedded_sign_in,
            commands::start_external_browser_sign_in,
            commands::start_browser_sign_in,
            commands::start_browser_sign_in_with_options,
            commands::cancel_browser_sign_in,
            commands::was_browser_close_failed,
            commands::has_supported_browser,
            commands::get_supported_browser_name,
            commands::pick_upload_files,
            commands::pick_upload_folder,
            commands::pick_watched_folder,
            commands::start_upload,
            commands::start_watched_folder_upload,
            commands::cancel_upload,
            commands::pause_upload,
            commands::resume_upload,
            commands::is_upload_paused,
            commands::set_schedule_enabled,
            commands::set_schedule_hours,
            commands::has_battery,
            commands::set_pause_on_battery,
            commands::set_locale,
            commands::set_dry_run,
            commands::scan_for_review,
            commands::cancel_scan,
            commands::app_ready,
            commands::test_notification,
            commands::open_url,
            commands::set_resizable,
            commands::set_min_size,
            commands::center_window,
            commands::set_title,
            commands::clipboard_set_text,
            commands::clipboard_get_text,
            commands::get_account_for_path,
            commands::credential_needs_token_binding,
            commands::add_token_binding_alias_from_adb,
            commands::log_frontend,
            commands::get_matugen_status,
            commands::get_matugen_colors,
            commands::reload_matugen_colors,
            commands::open_devtools,
            commands::toggle_devtools,
        ])
        .setup(|app| {
            let tray_ok = if let Ok(tray) = tray::setup_tray(app.handle()) {
                app.manage(tray);
                true
            } else {
                false
            };
            app.state::<AppState>()
                .tray_available
                .store(tray_ok, std::sync::atomic::Ordering::SeqCst);

            let app_handle = app.handle().clone();
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(&format!("Lumen v{}", commands::get_app_version()));
                let w = window.clone();
                let ah = app_handle.clone();
                window.on_window_event(move |event| match event {
                    tauri::WindowEvent::CloseRequested { api, .. } => {
                        let state = ah.state::<AppState>();
                        if state.tray_available.load(std::sync::atomic::Ordering::Relaxed)
                            && state.config_manager.get_config().preferences.minimize_to_tray
                        {
                            api.prevent_close();
                            let _ = w.hide();
                        }
                    }
                    tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Enter { .. }) => {
                        let _ = ah.emit("file-drag-enter", ());
                    }
                    tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                        let file_paths: Vec<String> =
                            paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
                        let _ = ah.emit(
                            "files-dropped",
                            crate::models::FilesDroppedEvent {
                                files: file_paths,
                                drop_zone: String::new(),
                            },
                        );
                    }
                    tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Leave) => {
                        let _ = ah.emit("file-drag-cancelled", ());
                    }
                    _ => {}
                });

                // Fallback show after 1500ms only if app_ready hasn't run yet
                let w_show = window.clone();
                let initial_show_done = app
                    .state::<AppState>()
                    .initial_show_done
                    .clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    if !initial_show_done.swap(true, std::sync::atomic::Ordering::SeqCst) {
                        let _ = w_show.show();
                    }
                });
            }

            // Startup & periodic background update checker:
            // 1. Always checks once shortly after launch when auto_check_updates is enabled.
            // 2. While running, re-checks every `auto_update_interval_hours` (default 6h).
            // 3. Emits `app:update-available` to the UI and sends an OS desktop notification.
            let ah_update = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let mut last_notified_version = String::new();

                // Launch check (always runs on startup if auto_check_updates is on)
                {
                    let state = ah_update.state::<AppState>();
                    let prefs = state.config_manager.get_config().preferences;
                    if prefs.auto_check_updates {
                        let info = commands::run_update_check(
                            &state.config_manager,
                            &state.update_checker,
                            lumen_core::updater::DEFAULT_RELEASES_URL,
                            true,
                        )
                        .await;
                        if info.has_update {
                            let latest = info.latest_version.clone();
                            let _ = ah_update.emit("app:update-available", info);
                            commands::notify_update_available(&ah_update, &prefs.locale, &latest);
                            last_notified_version = latest;
                        }
                    }
                }

                // Periodic check loop while app remains open / in tray
                let mut ticker = tokio::time::interval(Duration::from_secs(60));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                // Consume immediate first tick
                ticker.tick().await;

                loop {
                    ticker.tick().await;
                    let state = ah_update.state::<AppState>();
                    let prefs = state.config_manager.get_config().preferences;
                    if !prefs.auto_check_updates {
                        continue;
                    }

                    let interval_secs = (prefs.auto_update_interval_hours.clamp(1, 168) as i64) * 3600;
                    let now_sec = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    if prefs.last_update_check > 0 && (now_sec - prefs.last_update_check) < interval_secs {
                        continue;
                    }

                    let info = commands::run_update_check(
                        &state.config_manager,
                        &state.update_checker,
                        lumen_core::updater::DEFAULT_RELEASES_URL,
                        true,
                    )
                    .await;

                    if info.has_update && info.latest_version != last_notified_version {
                        let latest = info.latest_version.clone();
                        let _ = ah_update.emit("app:update-available", info);
                        commands::notify_update_available(&ah_update, &prefs.locale, &latest);
                        last_notified_version = latest;
                    }
                }
            });

            // Background Folder Watcher initialization matching Go main.go
            let ah_watcher = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                let state = ah_watcher.state::<AppState>();
                let cfg = state.config_manager.get_config();

                let mut fw = lumen_core::watcher::FolderWatcher::new(Duration::from_millis(1500));
                fw.set_filter_options(
                    !cfg.preferences.disable_unsupported_files_filter,
                    cfg.preferences.exclude_pattern.clone(),
                );
                let mut initial_folders = std::collections::HashMap::new();
                if cfg.preferences.watch_folders_enabled {
                    for c in &cfg.preferences.watched_folder_configs {
                        if c.enabled {
                            initial_folders.insert(std::path::PathBuf::from(&c.path), c.include_subfolders);
                        }
                    }
                }
                match fw.set_folders(initial_folders) {
                    Ok(()) => {
                        *state.inotify_error.write().unwrap_or_else(|e| e.into_inner()) =
                            String::new();
                    }
                    Err(e) => {
                        tracing::warn!("Initial folder watcher error: {}", e);
                        *state.inotify_error.write().unwrap_or_else(|e| e.into_inner()) = e;
                    }
                }

                let ah_event = ah_watcher.clone();
                let _ = fw.start(move |ready_files| {
                    let state = ah_event.state::<AppState>();
                    let cfg = state.config_manager.get_config();
                    if !cfg.preferences.watch_folders_enabled {
                        return;
                    }
                    let string_paths: Vec<String> = ready_files
                        .iter()
                        .map(|p| p.to_string_lossy().to_string())
                        .collect();
                    if string_paths.is_empty() {
                        return;
                    }

                    if cfg.preferences.schedule_enabled
                        && !lumen_core::safety::is_within_schedule(
                            cfg.preferences.schedule_start_hour,
                            cfg.preferences.schedule_end_hour,
                        )
                    {
                        tracing::info!("Folder watcher: holding {} files outside scheduled quiet hours", string_paths.len());
                        let mut pending = state.pending_watch_files.lock().unwrap();
                        pending.push(commands::PendingUploadBatch {
                            files: string_paths,
                            is_watched: true,
                            notify_watch_detected: true,
                            dry_run_override: None,
                            recursive_override: None,
                        });
                        return;
                    }

                    if cfg.preferences.pause_on_battery && lumen_core::safety::is_running_on_battery() {
                        tracing::info!("Folder watcher: holding {} files while on battery", string_paths.len());
                        let mut pending = state.pending_watch_files.lock().unwrap();
                        pending.push(commands::PendingUploadBatch {
                            files: string_paths,
                            is_watched: true,
                            notify_watch_detected: true,
                            dry_run_override: None,
                            recursive_override: None,
                        });
                        return;
                    }

                    if let Err(e) = commands::execute_upload(
                        string_paths,
                        ah_event.clone(),
                        (*state).clone(),
                        true,
                        None,
                        None,
                    ) {
                        tracing::warn!("Folder watcher upload failed to start: {}", e);
                    }
                });

                let watcher_arc = state.watcher.clone();
                let mut lock = watcher_arc.lock().await;
                *lock = Some(fw);
                drop(lock);

                let ah_drain = ah_watcher.clone();
                tauri::async_runtime::spawn(async move {
                    let mut ticker = tokio::time::interval(Duration::from_secs(30));
                    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        ticker.tick().await;
                        let st = ah_drain.state::<AppState>();
                        let has_pending = !st.pending_watch_files.lock().unwrap().is_empty();
                        if has_pending {
                            commands::drain_ready_pending_batches(&ah_drain, &st);
                        }
                    }
                });
            });

            #[cfg(target_os = "linux")]
            {
                crate::matugen::linux::init_watcher(app_handle.clone());
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Lumen application");
}
