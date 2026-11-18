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

use commands::AppState;

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
        is_uploading: Arc::new(AtomicBool::new(false)),
        pending_watch_files: Arc::new(std::sync::Mutex::new(Vec::new())),
        watcher: Arc::new(tokio::sync::Mutex::new(None)),
        tray_available: Arc::new(AtomicBool::new(false)),
        update_checker: lumen_core::updater::UpdateChecker::new(),
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
            commands::get_last_update_check,
            commands::set_last_update_check,
            commands::get_release_notes,
            commands::get_app_version,
            commands::is_debug_build,
            commands::is_tray_available,
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
            commands::cancel_upload,
            commands::pause_upload,
            commands::resume_upload,
            commands::is_upload_paused,
            commands::set_schedule_enabled,
            commands::set_schedule_hours,
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

                // Fallback show after 1500ms
                let w_show = window.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    let _ = w_show.show();
                });
            }

            // Background update check after 3 seconds
            let ah_update = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let state = ah_update.state::<AppState>();
                if state.config_manager.get_config().preferences.auto_check_updates {
                    let info = commands::run_update_check(
                        &state.config_manager,
                        &state.update_checker,
                        lumen_core::updater::DEFAULT_RELEASES_URL,
                        false,
                    )
                    .await;
                    if info.has_update {
                        let _ = ah_update.emit("app:update-available", info);
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
                let _ = fw.set_folders(initial_folders);

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
                            dry_run_override: None,
                            recursive_override: None,
                        });
                        return;
                    }

                    let _ = ah_event.emit(
                        "folder-watch:detected",
                        serde_json::json!({
                            "files": &string_paths,
                            "count": string_paths.len(),
                        }),
                    );

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
