//! Tauri commands implementing all Bridge functions

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Window};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

use lumen_core::album::AlbumRegistry;
use lumen_core::auth::{
    build_google_photos_credential, exchange_embedded_setup_token, generate_android_id,
    looks_like_auth_string, normalize_oauth_token, EMBEDDED_SETUP_AUTH_ENDPOINT,
};
use lumen_core::config::ConfigManager;
use lumen_core::dedup::HashCache;
use lumen_core::livephoto::{classify_upload_work, LivePhotoClassificationOptions};
use lumen_core::net::create_http_client;
use lumen_core::scanner::{scan_directories_incremental, ScanOptions};
use lumen_core::updater::{UpdateChecker, DEFAULT_RELEASES_URL};
use lumen_core::upload::{UploadEngine, UploadEvent, UploadOptions};

use crate::cdp;
use crate::models::*;

#[derive(Clone)]
pub struct AppState {
    pub config_manager: Arc<ConfigManager>,
    pub hash_cache: Option<HashCache>,
    pub album_registry: AlbumRegistry,
    pub logs: Arc<std::sync::RwLock<Vec<LogEntry>>>,
    pub upload_cancel: Arc<AtomicBool>,
    pub scan_cancel: Arc<AtomicBool>,
    pub is_uploading: Arc<AtomicBool>,
    pub pending_watch_files: Arc<std::sync::Mutex<Vec<String>>>,
    pub watcher: Arc<tokio::sync::Mutex<Option<lumen_core::watcher::FolderWatcher>>>,
}

pub fn sync_folder_watcher(state: &AppState) {
    let cfg = state.config_manager.get_config();
    let mut folders = std::collections::HashMap::new();
    if cfg.preferences.watch_folders_enabled {
        for c in &cfg.preferences.watched_folder_configs {
            if c.enabled {
                folders.insert(std::path::PathBuf::from(&c.path), c.include_subfolders);
            }
        }
    }
    let watcher_arc = state.watcher.clone();
    tauri::async_runtime::spawn(async move {
        let mut lock = watcher_arc.lock().await;
        if let Some(ref mut w) = *lock {
            let _ = w.set_folders(folders);
        }
    });
}

// -------------------------------------------------------------
// Preferences & Config Commands
// -------------------------------------------------------------

#[tauri::command]
pub fn get_settings(state: tauri::State<AppState>) -> Preferences {
    let cfg = state.config_manager.get_config();
    Preferences {
        proxy: cfg.preferences.proxy,
        use_quota: cfg.preferences.use_quota,
        saver: cfg.preferences.saver,
        recursive: cfg.preferences.recursive,
        force_upload: cfg.preferences.force_upload,
        pair_live_photos: cfg.preferences.pair_live_photos,
        skip_incomplete_live_photos: cfg.preferences.skip_incomplete_live_photos,
        update_existing_photos_to_live: cfg.preferences.update_existing_photos_to_live,
        upload_threads: cfg.preferences.upload_threads as u32,
        delete_from_host: cfg.preferences.delete_from_host,
        disable_unsupported_files_filter: cfg.preferences.disable_unsupported_files_filter,
        set_date_from_filename: cfg.preferences.set_date_from_filename,
        exclude_pattern: cfg.preferences.exclude_pattern,
        minimize_to_tray: cfg.preferences.minimize_to_tray,
        notify_on_complete: cfg.preferences.notify_on_complete,
        use_local_hash_cache: cfg.preferences.use_local_hash_cache,
        upload_speed_limit_kbps: cfg.preferences.upload_speed_limit_kbps,
        watch_folders_enabled: cfg.preferences.watch_folders_enabled,
        watched_folders: cfg.preferences.watched_folders,
        watched_folder_configs: cfg
            .preferences
            .watched_folder_configs
            .into_iter()
            .map(|c| WatchedFolderConfig {
                path: c.path,
                account_email: c.account_email,
                enabled: c.enabled,
                include_subfolders: c.include_subfolders,
                album_mode: c.album_mode,
                album_name_or_key: c.album_name_or_key,
            })
            .collect(),
        recent_albums: cfg.preferences.recent_albums,
        auto_check_updates: cfg.preferences.auto_check_updates,
        last_update_check: cfg.preferences.last_update_check,
        album_name: cfg.preferences.album_name,
        album_auto_mode: cfg.preferences.album_auto_mode,
    }
}

#[tauri::command]
pub fn get_accounts(state: tauri::State<AppState>) -> AccountsState {
    let cfg = state.config_manager.get_config();
    let mut accounts = Vec::new();
    for cred in &cfg.account.credentials {
        if let Some((_, email)) = url::form_urlencoded::parse(cred.as_bytes()).find(|(k, _)| k == "Email") {
            let email_str = email.into_owned();
            if !accounts.iter().any(|a: &AccountSummary| a.email == email_str) {
                accounts.push(AccountSummary {
                    email: email_str,
                    needs_token_binding: false,
                });
            }
        }
    }
    AccountsState {
        accounts,
        selected: cfg.account.selected,
    }
}

#[tauri::command]
pub fn set_selected(email: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.account.selected = email;
    })
}

#[tauri::command]
pub fn set_upload_threads(upload_threads: u32, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.upload_threads = upload_threads as usize;
    })
}

#[tauri::command]
pub fn set_saver(saver: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.saver = saver;
    })
}

#[tauri::command]
pub fn set_use_quota(use_quota: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.use_quota = use_quota;
    })
}

#[tauri::command]
pub fn set_pair_live_photos(pair_live_photos: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.pair_live_photos = pair_live_photos;
    })
}

#[tauri::command]
pub fn set_skip_incomplete_live_photos(
    skip_incomplete_live_photos: bool,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.skip_incomplete_live_photos = skip_incomplete_live_photos;
    })
}

#[tauri::command]
pub fn set_update_existing_photos_to_live(
    update_existing_photos_to_live: bool,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.update_existing_photos_to_live = update_existing_photos_to_live;
    })
}

#[tauri::command]
pub fn set_delete_from_host(delete_from_host: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.delete_from_host = delete_from_host;
    })
}

#[tauri::command]
pub fn set_disable_unsupported_files_filter(
    disable_unsupported_files_filter: bool,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.disable_unsupported_files_filter = disable_unsupported_files_filter;
    })
}

#[tauri::command]
pub fn set_set_date_from_filename(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.set_date_from_filename = v;
    })
}

#[tauri::command]
pub fn set_exclude_pattern(pattern: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.exclude_pattern = pattern;
    })
}

#[tauri::command]
pub fn get_exclude_pattern(state: tauri::State<AppState>) -> String {
    state.config_manager.get_config().preferences.exclude_pattern
}

#[tauri::command]
pub fn set_minimize_to_tray(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.minimize_to_tray = v;
    })
}

#[tauri::command]
pub fn set_notify_on_complete(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.notify_on_complete = v;
    })
}

#[tauri::command]
pub fn set_use_local_hash_cache(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.use_local_hash_cache = v;
    })
}

#[tauri::command]
pub fn set_upload_speed_limit_kbps(v: u32, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.upload_speed_limit_kbps = v;
    })
}

#[tauri::command]
pub fn set_watch_folders_enabled(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    let res = state.config_manager.update(|cfg| {
        cfg.preferences.watch_folders_enabled = v;
    });
    if res.is_ok() {
        sync_folder_watcher(&state);
    }
    res
}

#[tauri::command]
pub fn set_proxy(proxy: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.proxy = proxy;
    })
}

#[tauri::command]
pub fn set_recursive(recursive: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.recursive = recursive;
    })
}

#[tauri::command]
pub fn set_force_upload(force_upload: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.force_upload = force_upload;
    })
}

#[tauri::command]
pub fn is_account_risk_acknowledged(_state: tauri::State<AppState>) -> bool {
    true
}

#[tauri::command]
pub fn set_account_risk_acknowledged(_acknowledged: bool, _state: tauri::State<AppState>) -> Result<(), String> {
    Ok(())
}

// -------------------------------------------------------------
// Album Management
// -------------------------------------------------------------

#[tauri::command]
pub fn get_album_name(state: tauri::State<AppState>) -> String {
    state.config_manager.get_config().preferences.album_name
}

#[tauri::command]
pub fn set_album_name(album_name: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.album_name = album_name;
    })
}

#[tauri::command]
pub fn get_album_auto_mode(state: tauri::State<AppState>) -> bool {
    state.config_manager.get_config().preferences.album_auto_mode
}

#[tauri::command]
pub fn set_album_auto_mode(auto_mode: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.album_auto_mode = auto_mode;
    })
}

#[tauri::command]
pub fn get_recent_albums(state: tauri::State<AppState>) -> Vec<String> {
    state.config_manager.get_config().preferences.recent_albums
}

#[tauri::command]
pub fn add_recent_album(name: String, state: tauri::State<AppState>) -> Result<(), String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Ok(());
    }
    state.config_manager.update(|cfg| {
        cfg.preferences.recent_albums.retain(|a| a != &name);
        cfg.preferences.recent_albums.insert(0, name);
        if cfg.preferences.recent_albums.len() > 10 {
            cfg.preferences.recent_albums.truncate(10);
        }
    })
}

#[tauri::command]
pub fn remove_recent_album(name: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.recent_albums.retain(|a| a != &name);
    })
}

#[tauri::command]
pub fn get_album_nicknames(
    account: String,
    state: tauri::State<'_, AppState>,
) -> std::collections::HashMap<String, String> {
    state.album_registry.get_all_nicknames(&account)
}

#[tauri::command]
pub fn get_album_nickname(
    account: String,
    key: String,
    state: tauri::State<'_, AppState>,
) -> String {
    state.album_registry.find_name_by_key(&account, &key).unwrap_or_default()
}

#[tauri::command]
pub fn set_album_nickname(
    account: String,
    key: String,
    nickname: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.album_registry.set(&account, &nickname, &key).map_err(|e| e.to_string())
}

// -------------------------------------------------------------
// Watched Folders
// -------------------------------------------------------------

#[tauri::command]
pub fn get_watched_folders(state: tauri::State<AppState>) -> Vec<String> {
    state.config_manager.get_config().preferences.watched_folders
}

#[tauri::command]
pub fn get_watched_folder_configs(state: tauri::State<AppState>) -> Vec<WatchedFolderConfig> {
    state
        .config_manager
        .get_config()
        .preferences
        .watched_folder_configs
        .into_iter()
        .map(|c| WatchedFolderConfig {
            path: c.path,
            account_email: c.account_email,
            enabled: c.enabled,
            include_subfolders: c.include_subfolders,
            album_mode: c.album_mode,
            album_name_or_key: c.album_name_or_key,
        })
        .collect()
}

#[tauri::command]
pub fn add_watched_folder(folder: String, state: tauri::State<AppState>) -> Result<(), String> {
    let folder = folder.trim().to_string();
    if folder.is_empty() {
        return Ok(());
    }
    let res = state.config_manager.update(|cfg| {
        if !cfg.preferences.watched_folders.contains(&folder) {
            cfg.preferences.watched_folders.push(folder.clone());
        }
        if !cfg.preferences.watched_folder_configs.iter().any(|c| c.path == folder) {
            cfg.preferences.watched_folder_configs.push(lumen_core::config::WatchedFolderConfig {
                path: folder,
                account_email: cfg.account.selected.clone(),
                enabled: true,
                include_subfolders: false,
                album_mode: "none".to_string(),
                album_name_or_key: String::new(),
            });
        }
    });
    if res.is_ok() {
        sync_folder_watcher(&state);
    }
    res
}

#[tauri::command]
pub fn add_watched_folder_with_account(
    folder: String,
    account_email: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let folder = folder.trim().to_string();
    if folder.is_empty() {
        return Ok(());
    }
    let res = state.config_manager.update(|cfg| {
        if !cfg.preferences.watched_folders.contains(&folder) {
            cfg.preferences.watched_folders.push(folder.clone());
        }
        cfg.preferences.watched_folder_configs.retain(|c| c.path != folder);
        cfg.preferences.watched_folder_configs.push(lumen_core::config::WatchedFolderConfig {
            path: folder,
            account_email,
            enabled: true,
            include_subfolders: false,
            album_mode: "none".to_string(),
            album_name_or_key: String::new(),
        });
    });
    if res.is_ok() {
        sync_folder_watcher(&state);
    }
    res
}

#[tauri::command]
pub fn remove_watched_folder(folder: String, state: tauri::State<AppState>) -> Result<(), String> {
    let folder = folder.trim();
    let res = state.config_manager.update(|cfg| {
        cfg.preferences.watched_folders.retain(|f| f != folder);
        cfg.preferences.watched_folder_configs.retain(|c| c.path != folder);
    });
    if res.is_ok() {
        sync_folder_watcher(&state);
    }
    res
}

#[tauri::command]
pub fn update_watched_folder_config(
    cfg: WatchedFolderConfig,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let res = state.config_manager.update(|c| {
        for item in &mut c.preferences.watched_folder_configs {
            if item.path == cfg.path {
                item.account_email = cfg.account_email.clone();
                item.enabled = cfg.enabled;
                item.include_subfolders = cfg.include_subfolders;
                item.album_mode = cfg.album_mode.clone();
                item.album_name_or_key = cfg.album_name_or_key.clone();
                break;
            }
        }
    });
    if res.is_ok() {
        sync_folder_watcher(&state);
    }
    res
}

pub fn find_watched_folder_config_for_path(
    file_path: &str,
    state: &AppState,
) -> Option<WatchedFolderConfig> {
    let p = Path::new(file_path);
    let cfg = state.config_manager.get_config();
    for folder_cfg in &cfg.preferences.watched_folder_configs {
        let folder_path = Path::new(&folder_cfg.path);
        if p.starts_with(folder_path) {
            return Some(WatchedFolderConfig {
                path: folder_cfg.path.clone(),
                account_email: folder_cfg.account_email.clone(),
                enabled: folder_cfg.enabled,
                include_subfolders: folder_cfg.include_subfolders,
                album_mode: folder_cfg.album_mode.clone(),
                album_name_or_key: folder_cfg.album_name_or_key.clone(),
            });
        }
    }
    None
}

#[tauri::command]
pub fn get_watched_folder_config_for_path(
    file_path: String,
    state: tauri::State<AppState>,
) -> Option<WatchedFolderConfig> {
    find_watched_folder_config_for_path(&file_path, &state)
}

#[tauri::command]
pub fn count_folder_backlog(folder: String) -> usize {
    let p = Path::new(&folder);
    if !p.exists() || !p.is_dir() {
        return 0;
    }
    let opts = ScanOptions {
        recursive: true,
        filter_unsupported_files: true,
        ..Default::default()
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    match scan_directories_incremental(&[p.to_path_buf()], &opts, cancelled, |_| {}) {
        Ok(files) => files.len(),
        Err(_) => 0,
    }
}

#[tauri::command]
pub fn get_inotify_error() -> String {
    String::new()
}

// -------------------------------------------------------------
// Authentication & Credentials
// -------------------------------------------------------------

#[tauri::command]
pub fn remove_credentials(email: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.remove_credential(&email)
}

#[tauri::command]
pub async fn validate_and_add_credentials(
    raw_input: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let raw = raw_input.trim();
    if raw.is_empty() {
        return Err("Credential input cannot be empty".to_string());
    }

    let (email, full_credential) = if looks_like_auth_string(raw) {
        let params: std::collections::HashMap<String, String> =
            url::form_urlencoded::parse(raw.as_bytes()).into_owned().collect();
        let email = params.get("Email").map(|s| s.trim()).unwrap_or("");
        let token = params.get("Token").map(|s| s.trim()).unwrap_or("");
        if email.is_empty() || email.eq_ignore_ascii_case("unknown@example.com") {
            return Err("Invalid credential format. Email and Token are required.".to_string());
        }
        if token.is_empty() {
            return Err("Invalid credential format. Token is required.".to_string());
        }
        (email.to_string(), raw.to_string())
    } else {
        let clean = normalize_oauth_token(raw).map_err(|e| e.to_string())?;
        let client = create_http_client(None)?;
        let android_id = generate_android_id();
        let exchange = exchange_embedded_setup_token(&client, EMBEDDED_SETUP_AUTH_ENDPOINT, &clean, &android_id)
            .await
            .map_err(|e| e.to_string())?;

        if exchange.email.is_empty() || exchange.email.eq_ignore_ascii_case("unknown@example.com") {
            return Err("Google token exchange returned invalid or missing account email.".to_string());
        }

        let cred = build_google_photos_credential(&exchange.email, &exchange.master_token, &android_id);
        (exchange.email, cred)
    };

    let save_status = state
        .config_manager
        .save_credential(&email, &full_credential)
        .map_err(|e| e.to_string())?;

    if !save_status.keyring_saved {
        let _ = app.emit(
            "log:entry",
            LogEntry {
                id: format!("keyring-warn-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()),
                timestamp: chrono_iso(),
                level: "WARN".to_string(),
                category: "Auth".to_string(),
                user_message: Some("OS keyring unavailable; saving credentials directly to configuration file with mode 0600 permissions.".to_string()),
                ..Default::default()
            },
        );
    }

    Ok(email)
}

#[tauri::command]
pub async fn add_credentials(
    new_auth_string: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    validate_and_add_credentials(new_auth_string, app, state).await.map(|_| ())
}

#[tauri::command]
pub async fn add_google_account(
    oauth_token: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    validate_and_add_credentials(oauth_token, app, state).await
}

// -------------------------------------------------------------
// Automated Chromium CDP Sign-In
// -------------------------------------------------------------

#[tauri::command]
pub async fn start_browser_sign_in(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let mut session = cdp::CdpSession::launch().await.map_err(|e| e.to_string())?;

    let token = match session.wait_for_google_auth_cookies().await {
        Ok(t) => t,
        Err(e) => {
            session.close();
            return Err(e.to_string());
        }
    };

    let close_ok = session.close();
    if !close_ok {
        let _ = app.emit("browser-signin:close-failed", ());
    }

    validate_and_add_credentials(token, app, state).await
}

#[tauri::command]
pub async fn start_browser_sign_in_with_options(
    _opts: serde_json::Value,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    start_browser_sign_in(app, state).await
}

#[tauri::command]
pub fn cancel_browser_sign_in() -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn was_browser_close_failed() -> bool {
    cdp::was_browser_close_failed()
}

#[tauri::command]
pub fn has_supported_browser() -> bool {
    cdp::find_chromium_browser().is_some()
}

#[tauri::command]
pub fn get_supported_browser_name() -> String {
    cdp::get_supported_browser_name()
}

// -------------------------------------------------------------
// Hash Cache & Deduplication
// -------------------------------------------------------------

#[tauri::command]
pub fn get_hash_cache_count(state: tauri::State<AppState>) -> i64 {
    if let Some(ref cache) = state.hash_cache {
        cache.count(None).unwrap_or(0)
    } else {
        0
    }
}

#[tauri::command]
pub fn clear_hash_cache(state: tauri::State<AppState>) -> Result<i64, String> {
    if let Some(ref cache) = state.hash_cache {
        let count = cache.count(None).unwrap_or(0);
        cache.clear(None).map_err(|e| e.to_string())?;
        Ok(count)
    } else {
        Ok(0)
    }
}

// -------------------------------------------------------------
// Logging & Updates
// -------------------------------------------------------------

#[tauri::command]
pub fn get_log_entries(
    limit: usize,
    level_filter: String,
    state: tauri::State<AppState>,
) -> Vec<LogEntry> {
    let logs = state.logs.read().unwrap();
    let filter = level_filter.to_uppercase();
    logs.iter()
        .rev()
        .filter(|e| filter.is_empty() || filter == "ALL" || e.level == filter)
        .take(if limit == 0 { usize::MAX } else { limit })
        .cloned()
        .collect()
}

#[tauri::command]
pub fn get_problem_logs(state: tauri::State<AppState>) -> Vec<LogEntry> {
    let logs = state.logs.read().unwrap();
    logs.iter()
        .rev()
        .filter(|e| e.level == "WARN" || e.level == "ERROR")
        .cloned()
        .collect()
}

#[tauri::command]
pub fn clear_log_entries(state: tauri::State<AppState>) {
    state.logs.write().unwrap().clear();
}

#[tauri::command]
pub fn export_logs(format: String, state: tauri::State<AppState>) -> String {
    let logs = state.logs.read().unwrap();
    if format.to_lowercase() == "json" {
        serde_json::to_string_pretty(&*logs).unwrap_or_default()
    } else {
        let mut out = String::new();
        for e in logs.iter() {
            out.push_str(&format!("[{}] [{}] [{}]: {}\n", e.timestamp, e.level, e.category, e.user_message.as_deref().unwrap_or("")));
        }
        out
    }
}

#[tauri::command]
pub async fn check_for_updates(manual: bool) -> UpdateInfo {
    let checker = UpdateChecker::new();
    let info = checker
        .check_for_updates(
            DEFAULT_RELEASES_URL,
            "1.1.0",
            manual,
            0,
        )
        .await;

    UpdateInfo {
        current_version: info.current_version,
        latest_version: info.latest_version,
        has_update: info.has_update,
        release_url: info.release_url,
        release_notes: info.release_notes,
        published_at: chrono_iso(),
        checked_at: chrono_iso(),
        error: info.error,
    }
}

#[tauri::command]
pub async fn check_for_updates_with_redirect_url(redirect_url: String, manual: bool) -> UpdateInfo {
    let checker = UpdateChecker::new();
    let info = checker
        .check_for_updates(&redirect_url, "1.1.0", manual, 0)
        .await;

    UpdateInfo {
        current_version: info.current_version,
        latest_version: info.latest_version,
        has_update: info.has_update,
        release_url: info.release_url,
        release_notes: info.release_notes,
        published_at: chrono_iso(),
        checked_at: chrono_iso(),
        error: info.error,
    }
}

#[tauri::command]
pub fn get_auto_check_updates(state: tauri::State<AppState>) -> bool {
    state.config_manager.get_config().preferences.auto_check_updates
}

#[tauri::command]
pub fn set_auto_check_updates(enabled: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.auto_check_updates = enabled;
    })
}

#[tauri::command]
pub fn get_last_update_check(state: tauri::State<AppState>) -> i64 {
    state.config_manager.get_config().preferences.last_update_check
}

#[tauri::command]
pub fn set_last_update_check(timestamp: i64, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.last_update_check = timestamp;
    })
}

#[tauri::command]
pub fn get_release_notes(_tag: String) -> String {
    String::new()
}

#[tauri::command]
pub fn get_app_version() -> String {
    "1.1.0".to_string()
}

#[tauri::command]
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

#[tauri::command]
pub fn is_tray_available() -> bool {
    true
}

// -------------------------------------------------------------
// System Dialogs (via tauri-plugin-dialog)
// -------------------------------------------------------------

#[tauri::command]
pub async fn pick_upload_files(app: AppHandle) -> Vec<String> {
    use tokio::sync::oneshot;
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .file()
        .set_title("Select Photos & Videos to Upload")
        .pick_files(move |paths| {
            let res = paths
                .map(|list| list.into_iter().map(|p| p.to_string()).collect())
                .unwrap_or_default();
            let _ = tx.send(res);
        });
    rx.await.unwrap_or_default()
}

#[tauri::command]
pub async fn pick_upload_folder(app: AppHandle) -> String {
    use tokio::sync::oneshot;
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .file()
        .set_title("Select Folder to Upload")
        .pick_folder(move |path| {
            let res = path.map(|p| p.to_string()).unwrap_or_default();
            let _ = tx.send(res);
        });
    rx.await.unwrap_or_default()
}

#[tauri::command]
pub async fn pick_watched_folder(app: AppHandle) -> String {
    use tokio::sync::oneshot;
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .file()
        .set_title("Select Folder to Watch")
        .pick_folder(move |path| {
            let res = path.map(|p| p.to_string()).unwrap_or_default();
            let _ = tx.send(res);
        });
    rx.await.unwrap_or_default()
}

// -------------------------------------------------------------
// Upload Orchestration & Events
// -------------------------------------------------------------

pub fn execute_upload(
    files: Vec<String>,
    app: AppHandle,
    state: AppState,
    is_watched: bool,
) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    state.upload_cancel.store(false, Ordering::SeqCst);
    state.is_uploading.store(true, Ordering::SeqCst);

    let cfg = state.config_manager.get_config();
    let mut account = cfg.account.selected.clone();

    let mut album_name = cfg.preferences.album_name.clone();
    let mut album_auto_mode = cfg.preferences.album_auto_mode;
    let mut recursive = cfg.preferences.recursive;
    let mut delete_from_host = cfg.preferences.delete_from_host;

    if is_watched {
        delete_from_host = false; // Watcher NEVER deletes local files
    }

    if let Some(first_path) = files.first() {
        if let Some(folder_cfg) = find_watched_folder_config_for_path(first_path, &state) {
            if !folder_cfg.account_email.is_empty() {
                account = folder_cfg.account_email;
            }
            if folder_cfg.album_mode == "auto" {
                album_auto_mode = true;
                album_name = String::new();
            } else if folder_cfg.album_mode == "custom" && !folder_cfg.album_name_or_key.is_empty() {
                album_auto_mode = false;
                album_name = folder_cfg.album_name_or_key;
            } else if folder_cfg.album_mode == "none" {
                album_auto_mode = false;
                album_name = String::new();
            }
            if folder_cfg.include_subfolders {
                recursive = true;
            }
        } else if !is_watched {
            for path in &files {
                if let Ok(m) = std::fs::metadata(path) {
                    if m.is_dir() {
                        recursive = true;
                        break;
                    }
                }
            }
        }
    }

    if account.is_empty() {
        state.is_uploading.store(false, Ordering::SeqCst);
        return Err("No Google account selected for upload".to_string());
    }

    state
        .config_manager
        .get_credential_for_account(Some(&account))
        .map_err(|e| {
            state.is_uploading.store(false, Ordering::SeqCst);
            format!("Could not retrieve credentials for {}: {}", account, e)
        })?;

    let cancel_flag = state.upload_cancel.clone();
    let is_uploading_flag = state.is_uploading.clone();
    let notify_on_complete = cfg.preferences.notify_on_complete;
    let hash_cache = state.hash_cache.clone();
    let album_registry = state.album_registry.clone();
    let config_manager = state.config_manager.clone();
    let logs = state.logs.clone();
    let app_clone = app.clone();
    let state_for_drain = state.clone();

    tokio::spawn(async move {
        // Classify work
        let path_bufs: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        let scan_opts = ScanOptions {
            recursive,
            filter_unsupported_files: !cfg.preferences.disable_unsupported_files_filter,
            exclude_pattern: cfg.preferences.exclude_pattern.clone(),
            ..Default::default()
        };
        let scanned_files = match scan_directories_incremental(&path_bufs, &scan_opts, cancel_flag.clone(), |_| {}) {
            Ok(f) => f,
            Err(e) => {
                tracing::error!("Scan error: {}", e);
                is_uploading_flag.store(false, Ordering::SeqCst);
                let _ = app_clone.emit("uploadStop", ());
                return;
            }
        };

        let live_photo_opts = LivePhotoClassificationOptions {
            enabled: cfg.preferences.pair_live_photos,
            skip_incomplete: cfg.preferences.skip_incomplete_live_photos,
            ignore_apple_metadata: false,
            default_account: account.clone(),
        };
        let (items, _preflight_warnings) = classify_upload_work(&scanned_files, &live_photo_opts);

        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<UploadEvent>(100);

        let upload_opts = UploadOptions {
            threads: cfg.preferences.upload_threads,
            saver: cfg.preferences.saver,
            use_quota: cfg.preferences.use_quota,
            recursive,
            force_upload: cfg.preferences.force_upload,
            delete_from_host,
            pair_live_photos: cfg.preferences.pair_live_photos,
            skip_incomplete_live_photos: cfg.preferences.skip_incomplete_live_photos,
            update_existing_photos_to_live: cfg.preferences.update_existing_photos_to_live,
            ignore_apple_metadata: false,
            set_date_from_filename: cfg.preferences.set_date_from_filename,
            disable_unsupported_files_filter: cfg.preferences.disable_unsupported_files_filter,
            exclude_pattern: cfg.preferences.exclude_pattern.clone(),
            use_local_hash_cache: cfg.preferences.use_local_hash_cache,
            upload_speed_limit_kbps: cfg.preferences.upload_speed_limit_kbps,
            album_name,
            album_auto_mode,
            custom_endpoint_base: std::env::var("LUMEN_API_ENDPOINT").ok(),
        };

        let engine = UploadEngine::new(
            config_manager,
            if cfg.preferences.use_local_hash_cache { hash_cache } else { None },
            album_registry,
        ).with_cancelled(cancel_flag);

        let event_handle = tokio::spawn({
            let app_clone = app_clone.clone();
            let logs = logs.clone();
            async move {
                while let Some(ev) = event_rx.recv().await {
                    match ev {
                        UploadEvent::BatchStart { total_items, total_bytes } => {
                            let _ = app_clone.emit(
                                "uploadStart",
                                UploadBatchStart {
                                    total: total_items,
                                    total_bytes: total_bytes as u64,
                                },
                            );
                            let _ = app_clone.emit("uploadTotalBytes", total_bytes);
                        }
                        UploadEvent::FileCompleted(res) => {
                            let file_name = res.path.file_name()
                                .map(|f| f.to_string_lossy().to_string())
                                .unwrap_or_default();
                            let path_str = res.path.to_string_lossy().to_string();
                            let is_error = res.is_error;
                            let skipped = res.skipped;
                            let error_message = if res.error_message.is_empty() { None } else { Some(res.error_message.clone()) };
                            let skip_reason = if res.skip_reason.is_empty() { None } else { Some(res.skip_reason.clone()) };

                            let _ = app_clone.emit("FileStatus", &res);

                            let log_entry = LogEntry {
                                id: format!("log-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()),
                                timestamp: chrono_iso(),
                                level: if is_error { "ERROR".to_string() } else { "INFO".to_string() },
                                category: "Upload".to_string(),
                                file_name: Some(file_name),
                                file_path: Some(path_str),
                                size_bytes: None,
                                duration_ms: None,
                                outcome: if is_error { Some("FAILED".to_string()) } else if skipped { Some("SKIPPED".to_string()) } else { Some("UPLOADED".to_string()) },
                                user_message: error_message.clone().or_else(|| skip_reason.clone()),
                                ..Default::default()
                            };
                            logs.write().unwrap().push(log_entry.clone());
                            let _ = app_clone.emit("log:entry", log_entry);
                        }
                        UploadEvent::ThreadProgress(status) => {
                            let _ = app_clone.emit("ThreadStatus", status);
                        }
                        UploadEvent::AccountBlocked { reason, email, message } => {
                            let _ = app_clone.emit(
                                "account:blocked",
                                AccountBlockedEvent {
                                    reason,
                                    email,
                                    message,
                                    resume_at: None,
                                },
                            );
                        }
                        UploadEvent::BatchComplete { uploaded, skipped, failed } => {
                            if notify_on_complete {
                                let _ = app_clone.notification()
                                    .builder()
                                    .title("Upload Complete")
                                    .body(format!("Finished: {} uploaded, {} skipped, {} failed.", uploaded, skipped, failed))
                                    .show();
                            }
                        }
                    }
                }
            }
        });

        let _ = engine.run_upload(items, upload_opts, event_tx).await;
        let _ = event_handle.await;

        is_uploading_flag.store(false, Ordering::SeqCst);
        let _ = app_clone.emit("uploadStop", ());

        // Drain queued watch files matching Go main.go
        let ah_drain = app_clone.clone();
        let state_drain = state_for_drain.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let next_batch: Vec<String> = {
                let mut pending = state_drain.pending_watch_files.lock().unwrap();
                if pending.is_empty() {
                    Vec::new()
                } else {
                    pending.drain(..).collect()
                }
            };
            if !next_batch.is_empty() {
                let _ = ah_drain.emit(
                    "folder-watch:detected",
                    serde_json::json!({
                        "files": &next_batch,
                        "count": next_batch.len(),
                    }),
                );
                let _ = execute_upload(next_batch, ah_drain, state_drain, true);
            }
        });
    });

    Ok(())
}

#[tauri::command]
pub async fn start_upload(
    files: Vec<String>,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    execute_upload(files, app, (*state).clone(), false)
}

#[tauri::command]
pub fn cancel_upload(state: tauri::State<AppState>) {
    state.upload_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn cancel_scan(state: tauri::State<AppState>) {
    state.scan_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn app_ready(window: Window) {
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub fn test_notification(app: AppHandle) {
    let _ = app
        .notification()
        .builder()
        .title("Lumen")
        .body("Upload batch complete (test notification)")
        .show();
}

// -------------------------------------------------------------
// Window & System Runtime Helpers
// -------------------------------------------------------------

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    app.opener().open_url(&url, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_resizable(window: Window, resizable: bool) -> Result<(), String> {
    window.set_resizable(resizable).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_min_size(window: Window, width: f64, height: f64) -> Result<(), String> {
    window
        .set_min_size(Some(tauri::LogicalSize::new(width, height)))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn center_window(window: Window) -> Result<(), String> {
    window.center().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_title(window: Window, title: String) -> Result<(), String> {
    window.set_title(&title).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clipboard_set_text(_text: String) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn clipboard_get_text() -> String {
    String::new()
}

#[tauri::command]
pub fn log_frontend(level: String, message: String) {
    println!("[FRONTEND {}] {}", level, message);
}

#[tauri::command]
pub fn get_account_for_path(file_path: String, state: tauri::State<AppState>) -> String {
    if let Some(cfg) = get_watched_folder_config_for_path(file_path, state.clone()) {
        if !cfg.account_email.is_empty() {
            return cfg.account_email;
        }
    }
    state.config_manager.get_config().account.selected
}

#[tauri::command]
pub fn credential_needs_token_binding(auth_string: String) -> bool {
    let pairs: std::collections::HashMap<String, String> = url::form_urlencoded::parse(auth_string.as_bytes())
        .into_owned()
        .collect();
    if let Some(tb) = pairs.get("token_binding_alias") {
        if !tb.is_empty() {
            return false;
        }
    }
    (pairs.get("assertion_jwt").map(|s| !s.is_empty()).unwrap_or(false))
        || (pairs.get("check_tb_upgrade_eligible").map(|s| !s.is_empty()).unwrap_or(false))
}

#[tauri::command]
pub fn add_token_binding_alias_from_adb(_email: String) -> Result<(), String> {
    Ok(())
}

fn chrono_iso() -> String {
    let now = std::time::SystemTime::now();
    let dt: chrono::DateTime<chrono::Utc> = now.into();
    dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
