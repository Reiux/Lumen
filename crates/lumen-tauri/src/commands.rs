//! Tauri commands implementing all Bridge functions

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Window};
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
use lumen_core::net::{create_http_client, validate_proxy_url};
use lumen_core::scanner::{scan_directories_incremental, validate_exclude_pattern, ScanOptions};
use lumen_core::updater::{UpdateChecker, DEFAULT_RELEASES_URL};
use lumen_core::upload::{UploadEngine, UploadEvent, UploadOptions};

use crate::cdp;
use crate::models::*;

#[derive(Clone, Debug)]
pub struct PendingUploadBatch {
    pub files: Vec<String>,
    pub is_watched: bool,
    pub dry_run_override: Option<bool>,
    pub recursive_override: Option<bool>,
}

#[derive(Clone)]
pub struct AppState {
    pub config_manager: Arc<ConfigManager>,
    pub hash_cache: Option<HashCache>,
    pub album_registry: AlbumRegistry,
    pub logs: Arc<std::sync::RwLock<Vec<LogEntry>>>,
    pub upload_cancel: Arc<AtomicBool>,
    pub upload_pause: Arc<AtomicBool>,
    pub scan_cancel: Arc<AtomicBool>,
    pub is_uploading: Arc<AtomicBool>,
    pub account_blocked: Arc<std::sync::RwLock<Option<AccountBlockedEvent>>>,
    pub pending_watch_files: Arc<std::sync::Mutex<Vec<PendingUploadBatch>>>,
    pub watcher: Arc<tokio::sync::Mutex<Option<lumen_core::watcher::FolderWatcher>>>,
    pub tray_available: Arc<AtomicBool>,
    pub update_checker: UpdateChecker,
    pub run_seq: Arc<std::sync::atomic::AtomicU64>,
    pub log_seq: Arc<std::sync::atomic::AtomicU64>,
}

pub fn sync_folder_watcher(state: &AppState) {
    let config_manager = state.config_manager.clone();
    let watcher_arc = state.watcher.clone();
    tauri::async_runtime::spawn(async move {
        let mut lock = watcher_arc.lock().await;
        if let Some(ref mut w) = *lock {
            let cfg = config_manager.get_config();
            let mut folders = std::collections::HashMap::new();
            if cfg.preferences.watch_folders_enabled {
                for c in &cfg.preferences.watched_folder_configs {
                    if c.enabled {
                        folders.insert(std::path::PathBuf::from(&c.path), c.include_subfolders);
                    }
                }
            }
            let filter_unsupported = !cfg.preferences.disable_unsupported_files_filter;
            let exclude_pattern = cfg.preferences.exclude_pattern.clone();
            w.set_filter_options(filter_unsupported, exclude_pattern);
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
        launch_on_startup: is_launch_on_startup_active(),
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
        schedule_enabled: cfg.preferences.schedule_enabled,
        schedule_start_hour: cfg.preferences.schedule_start_hour,
        schedule_end_hour: cfg.preferences.schedule_end_hour,
        pause_on_battery: cfg.preferences.pause_on_battery,
        dry_run: cfg.preferences.dry_run,
        locale: cfg.preferences.locale,
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

    // Safety net: If selected account is configured and not yet in accounts list, include it
    if !cfg.account.selected.trim().is_empty()
        && !accounts.iter().any(|a| a.email.eq_ignore_ascii_case(&cfg.account.selected))
    {
        accounts.push(AccountSummary {
            email: cfg.account.selected.clone(),
            needs_token_binding: false,
        });
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
    })?;
    sync_folder_watcher(&state);
    Ok(())
}

#[tauri::command]
pub fn set_set_date_from_filename(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.set_date_from_filename = v;
    })
}

#[tauri::command]
pub fn set_exclude_pattern(pattern: String, state: tauri::State<AppState>) -> Result<(), String> {
    let trimmed = pattern.trim();
    validate_exclude_pattern(trimmed)?;
    state.config_manager.update(|cfg| {
        cfg.preferences.exclude_pattern = trimmed.to_string();
    })?;
    sync_folder_watcher(&state);
    Ok(())
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

#[cfg(target_os = "macos")]
fn escape_xml_entity(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

const STARTUP_APP_NAME: &str = if cfg!(debug_assertions) {
    "Lumen Dev"
} else {
    "Lumen"
};
#[cfg(target_os = "macos")]
const LAUNCH_AGENT_LABEL: &str = if cfg!(debug_assertions) {
    "com.reiux.lumen.dev"
} else {
    "com.reiux.lumen"
};
#[cfg(target_os = "linux")]
const AUTOSTART_DESKTOP_FILENAME: &str = if cfg!(debug_assertions) {
    "lumen-dev.desktop"
} else {
    "lumen.desktop"
};

fn is_launch_on_startup_active() -> bool {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
            if key.get_value::<String, _>(STARTUP_APP_NAME).is_ok() {
                return true;
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let plist = std::path::PathBuf::from(home)
                .join("Library")
                .join("LaunchAgents")
                .join(format!("{}.plist", LAUNCH_AGENT_LABEL));
            if plist.exists() {
                return true;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let autostart_dir = if let Some(xdg) = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            std::path::PathBuf::from(xdg).join("autostart")
        } else if let Some(home) = std::env::var("HOME")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            std::path::PathBuf::from(home).join(".config").join("autostart")
        } else {
            std::path::PathBuf::from(".config").join("autostart")
        };
        let desktop_file = autostart_dir.join(AUTOSTART_DESKTOP_FILENAME);
        if desktop_file.exists() {
            return true;
        }
    }
    false
}

#[tauri::command]
pub fn get_launch_on_startup(_state: tauri::State<AppState>) -> bool {
    is_launch_on_startup_active()
}

#[tauri::command]
pub fn set_launch_on_startup(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
            .map_err(|e| format!("Failed to open Run registry key: {}", e))?;
        if v {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let exe_str = format!("\"{}\"", exe.to_string_lossy());
            key.set_value(STARTUP_APP_NAME, &exe_str)
                .map_err(|e| format!("Failed to set startup registry entry: {}", e))?;
        } else {
            let _ = key.delete_value(STARTUP_APP_NAME);
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let launch_agents = std::path::PathBuf::from(&home).join("Library").join("LaunchAgents");
            let plist_path = launch_agents.join(format!("{}.plist", LAUNCH_AGENT_LABEL));
            if v {
                std::fs::create_dir_all(&launch_agents).map_err(|e| e.to_string())?;
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let escaped_exe = escape_xml_entity(&exe.to_string_lossy());
                let plist_content = format!(
                    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n    <key>Label</key>\n    <string>{}</string>\n    <key>ProgramArguments</key>\n    <array>\n        <string>{}</string>\n    </array>\n    <key>RunAtLoad</key>\n    <true/>\n    <key>ProcessType</key>\n    <string>Interactive</string>\n</dict>\n</plist>",
                    LAUNCH_AGENT_LABEL, escaped_exe
                );
                std::fs::write(&plist_path, plist_content).map_err(|e| e.to_string())?;
            } else if plist_path.exists() {
                std::fs::remove_file(&plist_path).map_err(|e| e.to_string())?;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let autostart_dir = if let Some(xdg) = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            std::path::PathBuf::from(xdg).join("autostart")
        } else if let Some(home) = std::env::var("HOME")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            std::path::PathBuf::from(home).join(".config").join("autostart")
        } else {
            std::path::PathBuf::from(".config").join("autostart")
        };
        let desktop_file = autostart_dir.join(AUTOSTART_DESKTOP_FILENAME);
        if v {
            std::fs::create_dir_all(&autostart_dir).map_err(|e| e.to_string())?;
            let launch_path = std::env::var("APPIMAGE")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .map(std::path::PathBuf::from)
                .unwrap_or(std::env::current_exe().map_err(|e| e.to_string())?);
            let escaped_exe = launch_path
                .to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .fold(String::new(), |mut acc, c| {
                    match c {
                        '\\' => acc.push_str("\\\\"),
                        '"' => acc.push_str("\\\""),
                        '$' => acc.push_str("\\$"),
                        '`' => acc.push_str("\\`"),
                        '%' => acc.push_str("%%"),
                        other => acc.push(other),
                    }
                    acc
                });
            let desktop_entry = format!(
                "[Desktop Entry]\nType=Application\nName={}\nComment={} Google Photos Backup\nExec=\"{}\"\nTerminal=false\nCategories=Utility;\n",
                STARTUP_APP_NAME, STARTUP_APP_NAME, escaped_exe
            );
            std::fs::write(&desktop_file, desktop_entry).map_err(|e| e.to_string())?;
        } else if desktop_file.exists() {
            std::fs::remove_file(&desktop_file).map_err(|e| e.to_string())?;
        }
    }
    state.config_manager.update(|cfg| {
        cfg.preferences.launch_on_startup = v;
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
    let trimmed = proxy.trim();
    validate_proxy_url(trimmed)?;
    state.config_manager.update(|cfg| {
        cfg.preferences.proxy = trimmed.to_string();
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

#[tauri::command]
pub fn set_schedule_enabled(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.schedule_enabled = v;
    })
}

#[tauri::command]
pub fn set_schedule_hours(start: u8, end: u8, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.schedule_start_hour = start;
        cfg.preferences.schedule_end_hour = end;
    })
}

#[tauri::command]
pub fn has_battery() -> bool {
    lumen_core::safety::has_battery()
}

#[tauri::command]
pub fn set_pause_on_battery(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.pause_on_battery = v;
    })
}

#[tauri::command]
pub fn set_locale(locale: String, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.locale = locale;
    })
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
    state
        .album_registry
        .set_nickname(&account, &key, &nickname)
        .map_err(|e| e.to_string())
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
    let p = std::path::Path::new(&folder);
    if !p.is_dir() {
        return Err(format!("'{}' is not an accessible directory", folder));
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
    let p = std::path::Path::new(&folder);
    if !p.is_dir() {
        return Err(format!("'{}' is not an accessible directory", folder));
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

pub fn path_starts_with_case_insensitive(path: &Path, prefix: &Path) -> bool {
    #[cfg(windows)]
    {
        let p_str = path.to_string_lossy().to_lowercase().replace('/', "\\");
        let pref_str = prefix.to_string_lossy().to_lowercase().replace('/', "\\");
        let p_trimmed = p_str.trim_end_matches('\\');
        let pref_trimmed = pref_str.trim_end_matches('\\');
        if p_trimmed == pref_trimmed {
            return true;
        }
        if p_str.starts_with(pref_trimmed) {
            let rem = &p_str[pref_trimmed.len()..];
            return rem.starts_with('\\');
        }
        false
    }
    #[cfg(not(windows))]
    {
        path.starts_with(prefix)
    }
}

pub fn find_watched_folder_config_for_path(
    file_path: &str,
    state: &AppState,
) -> Option<WatchedFolderConfig> {
    let p = Path::new(file_path);
    let cfg = state.config_manager.get_config();
    let mut configs = cfg.preferences.watched_folder_configs.clone();
    // Sort descending by path length to ensure the most specific folder config matches first
    configs.sort_by_key(|a| std::cmp::Reverse(a.path.len()));

    for folder_cfg in &configs {
        if !folder_cfg.enabled {
            continue;
        }
        let folder_path = Path::new(&folder_cfg.path);
        if path_starts_with_case_insensitive(p, folder_path) {
            if !folder_cfg.include_subfolders {
                if let Some(parent) = p.parent() {
                    #[cfg(windows)]
                    {
                        let parent_s = parent.to_string_lossy().to_lowercase().replace('/', "\\");
                        let folder_s = folder_path.to_string_lossy().to_lowercase().replace('/', "\\");
                        if parent_s.trim_end_matches('\\') != folder_s.trim_end_matches('\\') {
                            continue;
                        }
                    }
                    #[cfg(not(windows))]
                    {
                        if parent != folder_path {
                            continue;
                        }
                    }
                }
            }
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
pub async fn count_folder_backlog(folder: String) -> usize {
    tokio::task::spawn_blocking(move || {
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
    })
    .await
    .unwrap_or(0)
}

#[tauri::command]
pub fn get_inotify_error() -> String {
    String::new()
}

// -------------------------------------------------------------
// Authentication & Credentials
// -------------------------------------------------------------

fn clear_account_blocked_for_email(state: &AppState, email: &str) {
    let mut guard = state.account_blocked.write().unwrap();
    if let Some(ref ev) = *guard {
        if ev.email.is_empty() || ev.email.eq_ignore_ascii_case(email.trim()) {
            *guard = None;
        }
    }
}

fn check_account_blocked(state: &AppState, account: &str) -> Result<(), String> {
    let mut guard = state.account_blocked.write().unwrap();
    let Some(ev) = guard.clone() else {
        return Ok(());
    };
    if !ev.email.is_empty() && !ev.email.eq_ignore_ascii_case(account.trim()) {
        return Ok(());
    }
    if ev.reason == "RATE_LIMITED" {
        if let Some(resume_at) = ev.resume_at {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            if now_ms >= resume_at {
                *guard = None;
                return Ok(());
            }
            let secs = ((resume_at - now_ms + 999) / 1000).max(1);
            return Err(format!(
                "Uploads are temporarily paused due to Google rate limiting. Try again in {}s.",
                secs
            ));
        }
        return Err("Uploads are temporarily paused due to Google rate limiting.".to_string());
    }
    Err(if ev.reason == "ACCOUNT_RESTRICTED" {
        "Uploads are blocked because this Google account is restricted. Resolve the issue or remove the account to continue.".to_string()
    } else {
        "Uploads are blocked because Google requires you to sign in again.".to_string()
    })
}

#[tauri::command]
pub fn remove_credentials(email: String, state: tauri::State<AppState>) -> Result<(), String> {
    let res = state.config_manager.remove_credential(&email);
    if res.is_ok() {
        clear_account_blocked_for_email(&state, &email);
    }
    res
}

async fn exchange_oauth_token_for_credential(
    raw_token: &str,
    proxy: Option<&str>,
) -> Result<(String, String), String> {
    let clean = normalize_oauth_token(raw_token).map_err(|e| e.to_string())?;
    let client = create_http_client(proxy)?;
    let android_id = generate_android_id();
    let exchange = exchange_embedded_setup_token(&client, EMBEDDED_SETUP_AUTH_ENDPOINT, &clean, &android_id)
        .await
        .map_err(|e| e.to_string())?;

    if exchange.email.is_empty() || exchange.email.eq_ignore_ascii_case("unknown@example.com") {
        return Err("Google token exchange returned invalid or missing account email.".to_string());
    }

    let cred = build_google_photos_credential(&exchange.email, &exchange.master_token, &android_id);
    Ok((exchange.email, cred))
}

async fn persist_validated_credential(
    email: String,
    full_credential: String,
    proxy: Option<&str>,
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
) -> Result<String, String> {
    let custom_endpoint = std::env::var("LUMEN_API_ENDPOINT")
        .ok()
        .filter(|s| !s.trim().is_empty());
    lumen_core::api::validate_google_photos_credential(
        &full_credential,
        proxy,
        custom_endpoint.as_deref(),
    )
    .await
    .map_err(|e| format!("Google Photos authorization (Stage 2) failed: {}", e))?;

    let save_status = state
        .config_manager
        .save_credential(&email, &full_credential)
        .map_err(|e| e.to_string())?;
    clear_account_blocked_for_email(state, &email);

    if !save_status.keyring_saved {
        let _ = app.emit(
            "log:entry",
            LogEntry {
                id: format!("keyring-warn-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()),
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

async fn validate_and_add_oauth_token(
    raw_oauth_token: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let raw = raw_oauth_token.trim();
    if raw.is_empty() {
        return Err("Credential input cannot be empty".to_string());
    }
    if looks_like_auth_string(raw) {
        return Err("Browser sign-in returned an unexpected credential string instead of an OAuth token.".to_string());
    }

    let prefs = state.config_manager.get_preferences();
    let proxy = (!prefs.proxy.trim().is_empty()).then_some(prefs.proxy.clone());
    let (email, full_credential) = exchange_oauth_token_for_credential(raw, proxy.as_deref()).await?;
    persist_validated_credential(email, full_credential, proxy.as_deref(), &app, &state).await
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

    let prefs = state.config_manager.get_preferences();
    let proxy = (!prefs.proxy.trim().is_empty()).then_some(prefs.proxy.clone());

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
        exchange_oauth_token_for_credential(raw, proxy.as_deref()).await?
    };

    persist_validated_credential(email, full_credential, proxy.as_deref(), &app, &state).await
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
// Automated In-App WebView Sign-In & Chromium CDP Fallback
// -------------------------------------------------------------

#[tauri::command]
pub async fn start_embedded_sign_in(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let prefs = state.config_manager.get_preferences();
    let proxy = (!prefs.proxy.trim().is_empty()).then_some(prefs.proxy.clone());

    let token = crate::webview_signin::run_webview_sign_in(&app, proxy.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    validate_and_add_oauth_token(token, app, state).await
}

#[tauri::command]
pub async fn start_external_browser_sign_in(
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

    validate_and_add_oauth_token(token, app, state).await
}

#[tauri::command]
pub async fn start_browser_sign_in(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    start_embedded_sign_in(app, state).await
}

#[tauri::command]
pub async fn start_browser_sign_in_with_options(
    _opts: serde_json::Value,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    start_embedded_sign_in(app, state).await
}

#[tauri::command]
pub fn cancel_browser_sign_in(app: AppHandle) -> Result<(), String> {
    crate::webview_signin::cancel_active_sign_in(&app);
    cdp::cancel_active_browser();
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
pub fn clear_problem_logs(state: tauri::State<AppState>) {
    state.logs.write().unwrap().retain(|e| e.level != "WARN" && e.level != "ERROR");
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

pub async fn run_update_check(
    config_manager: &ConfigManager,
    checker: &UpdateChecker,
    redirect_url: &str,
    manual: bool,
) -> UpdateInfo {
    let last_check = config_manager.get_config().preferences.last_update_check;
    let now_sec = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let should_persist_check = manual || last_check == 0 || (now_sec - last_check) >= (12 * 3600);

    let current_ver = get_app_version();
    let info = checker
        .check_for_updates(redirect_url, &current_ver, manual, last_check)
        .await;

    if should_persist_check && info.error.is_none() {
        let _ = config_manager.update(|cfg| {
            cfg.preferences.last_update_check = now_sec;
        });
    }

    UpdateInfo {
        current_version: info.current_version,
        latest_version: info.latest_version,
        has_update: info.has_update,
        release_url: info.release_url,
        release_notes: info.release_notes,
        published_at: String::new(),
        checked_at: chrono_iso(),
        error: info.error,
    }
}

#[tauri::command]
pub async fn check_for_updates(manual: bool, state: tauri::State<'_, AppState>) -> Result<UpdateInfo, String> {
    Ok(run_update_check(
        &state.config_manager,
        &state.update_checker,
        DEFAULT_RELEASES_URL,
        manual,
    )
    .await)
}

#[tauri::command]
pub async fn check_for_updates_with_redirect_url(
    redirect_url: String,
    manual: bool,
    state: tauri::State<'_, AppState>,
) -> Result<UpdateInfo, String> {
    Ok(run_update_check(
        &state.config_manager,
        &state.update_checker,
        &redirect_url,
        manual,
    )
    .await)
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
pub async fn get_release_notes(tag: String) -> String {
    UpdateChecker::new()
        .fetch_release_notes(&tag)
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_app_version() -> String {
    // Dev builds (cargo/tauri debug profile — local machines and CI dev
    // artifacts alike) always report a "-dev" suffix so they're never
    // mistaken for a stable release. Release builds report the plain version.
    if cfg!(debug_assertions) {
        format!("{}-dev", env!("CARGO_PKG_VERSION"))
    } else {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[tauri::command]
pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}

#[tauri::command]
pub fn is_tray_available(state: tauri::State<AppState>) -> bool {
    state.tray_available.load(Ordering::Relaxed)
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
        .set_title("Select Photos & Videos to Back Up")
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
        .set_title("Select Folder to Back Up")
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

pub fn drain_ready_pending_batches(app: &AppHandle, state: &AppState) {
    loop {
        if state.is_uploading.load(Ordering::SeqCst) {
            return;
        }

        let cfg = state.config_manager.get_config();
        let watch_allowed = cfg.preferences.watch_folders_enabled
            && (!cfg.preferences.schedule_enabled
                || lumen_core::safety::is_within_schedule(
                    cfg.preferences.schedule_start_hour,
                    cfg.preferences.schedule_end_hour,
                ))
            && (!cfg.preferences.pause_on_battery || !lumen_core::safety::is_running_on_battery());

        let next_batch = {
            let mut pending = state.pending_watch_files.lock().unwrap();
            if !cfg.preferences.watch_folders_enabled {
                pending.retain(|b| !b.is_watched);
            }
            pending
                .iter()
                .position(|b| !b.is_watched || watch_allowed)
                .map(|idx| pending.remove(idx))
        };

        let Some(batch) = next_batch else {
            return;
        };

        if batch.is_watched {
            let _ = app.emit(
                "folder-watch:detected",
                serde_json::json!({
                    "files": &batch.files,
                    "count": batch.files.len(),
                }),
            );
        }

        match execute_upload(
            batch.files,
            app.clone(),
            state.clone(),
            batch.is_watched,
            batch.dry_run_override,
            batch.recursive_override,
        ) {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!("Pending upload batch failed to start: {}", e);
                let _ = app.emit("upload:error", e);
            }
        }
    }
}

fn spawn_drain_pending_watch_files(app: AppHandle, state: AppState) {
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        drain_ready_pending_batches(&app, &state);
    });
}

pub fn execute_upload(
    files: Vec<String>,
    app: AppHandle,
    state: AppState,
    is_watched: bool,
    dry_run_override: Option<bool>,
    recursive_override: Option<bool>,
) -> Result<(), String> {
    if files.is_empty() {
        return Ok(());
    }

    if state
        .is_uploading
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        let mut pending = state.pending_watch_files.lock().unwrap();
        pending.push(PendingUploadBatch {
            files,
            is_watched,
            dry_run_override,
            recursive_override,
        });
        return Ok(());
    }

    let mut files = files;
    if is_watched && files.len() > 1 {
        let mut group_order: Vec<(String, String, String, bool)> = Vec::new();
        let mut groups: std::collections::HashMap<(String, String, String, bool), Vec<String>> =
            std::collections::HashMap::new();
        for f in files {
            let key = match find_watched_folder_config_for_path(&f, &state) {
                Some(fc) => (
                    fc.account_email,
                    fc.album_mode,
                    fc.album_name_or_key,
                    fc.include_subfolders,
                ),
                None => (String::new(), String::new(), String::new(), false),
            };
            if !groups.contains_key(&key) {
                group_order.push(key.clone());
            }
            groups.entry(key).or_default().push(f);
        }
        let mut iter = group_order.into_iter();
        if let Some(first_key) = iter.next() {
            files = groups.remove(&first_key).unwrap_or_default();
            let remaining: Vec<PendingUploadBatch> = iter
                .filter_map(|k| groups.remove(&k))
                .filter(|g| !g.is_empty())
                .map(|g| PendingUploadBatch {
                    files: g,
                    is_watched: true,
                    dry_run_override,
                    recursive_override,
                })
                .collect();
            if !remaining.is_empty() {
                let mut pending = state.pending_watch_files.lock().unwrap();
                for (offset, b) in remaining.into_iter().enumerate() {
                    let insert_at = offset.min(pending.len());
                    pending.insert(insert_at, b);
                }
            }
        } else {
            files = Vec::new();
        }
    }

    state.upload_cancel.store(false, Ordering::SeqCst);
    state.upload_pause.store(false, Ordering::SeqCst);
    state.scan_cancel.store(false, Ordering::SeqCst);

    let cfg = state.config_manager.get_config();
    let mut account = cfg.account.selected.clone();

    let mut album_name = cfg.preferences.album_name.clone();
    let mut album_auto_mode = cfg.preferences.album_auto_mode;
    let mut recursive = recursive_override.unwrap_or(cfg.preferences.recursive);
    let mut delete_from_host = cfg.preferences.delete_from_host;

    if is_watched {
        delete_from_host = false; // Watcher NEVER deletes local files
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
            }
        }
    } else if recursive_override.is_none() {
        // Auto-enable recursion when a folder is selected — but only when
        // the caller did not pass an explicit override (review dialog
        // toggle wins over this heuristic).
        for path in &files {
            if let Ok(m) = std::fs::metadata(path) {
                if m.is_dir() {
                    recursive = true;
                    break;
                }
            }
        }
    }

    if account.is_empty() {
        state.is_uploading.store(false, Ordering::SeqCst);
        return Err("No Google account selected for upload".to_string());
    }

    if let Err(msg) = check_account_blocked(&state, &account) {
        state.is_uploading.store(false, Ordering::SeqCst);
        return Err(msg);
    }

    state
        .config_manager
        .get_credential_for_account(Some(&account))
        .map_err(|e| {
            state.is_uploading.store(false, Ordering::SeqCst);
            format!("Could not retrieve credentials for {}: {}", account, e)
        })?;

    let upload_cancel_flag = state.upload_cancel.clone();
    let upload_pause_flag = state.upload_pause.clone();
    let scan_cancel_flag = state.scan_cancel.clone();
    let is_uploading_flag = state.is_uploading.clone();
    let notify_on_complete = cfg.preferences.notify_on_complete;
    let hash_cache = state.hash_cache.clone();
    let album_registry = state.album_registry.clone();
    let config_manager = state.config_manager.clone();
    let logs = state.logs.clone();
    let log_seq = state.log_seq.clone();
    let run_id = format!("run-{}", state.run_seq.fetch_add(1, Ordering::Relaxed));
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

        // Combine upload_cancel and scan_cancel for scanning loop
        let scan_cancelled = Arc::new(AtomicBool::new(false));
        let scan_cancelled_thread = scan_cancelled.clone();
        let uc = upload_cancel_flag.clone();
        let sc = scan_cancel_flag.clone();
        let cancel_poll = tokio::spawn(async move {
            while !uc.load(Ordering::Relaxed) && !sc.load(Ordering::Relaxed) && !scan_cancelled_thread.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            scan_cancelled_thread.store(true, Ordering::SeqCst);
        });

        let app_for_scan = app_clone.clone();
        let scan_cancelled_blocking = scan_cancelled.clone();
        let live_photo_opts = LivePhotoClassificationOptions {
            enabled: cfg.preferences.pair_live_photos,
            skip_incomplete: cfg.preferences.skip_incomplete_live_photos,
            ignore_apple_metadata: false,
            default_account: account.clone(),
        };

        let scan_classify_result = tokio::task::spawn_blocking(move || {
            let scanned_files = scan_directories_incremental(
                &path_bufs,
                &scan_opts,
                scan_cancelled_blocking,
                move |p| {
                    let _ = app_for_scan.emit("scan:progress", serde_json::json!({
                        "filesFound": p.files_found,
                        "currentDirectory": p.current_directory,
                        "isScanning": p.is_scanning,
                    }));
                },
            )?;
            Ok::<_, String>(classify_upload_work(&scanned_files, &live_photo_opts))
        })
        .await
        .map_err(|e| format!("Scan task failed: {}", e))
        .and_then(|res| res);

        let (mut items, preflight_warnings) = match scan_classify_result {
            Ok(res) => {
                cancel_poll.abort();
                res
            }
            Err(e) => {
                cancel_poll.abort();
                tracing::error!("Scan error: {}", e);
                if e.contains("cancelled") {
                    let _ = app_clone.emit("scan:cancelled", ());
                }
                let _ = app_clone.emit("uploadStop", ());
                is_uploading_flag.store(false, Ordering::SeqCst);
                spawn_drain_pending_watch_files(app_clone.clone(), state_for_drain.clone());
                return;
            }
        };

        // For watched folder triggers, ensure each work item maps to its watched folder's account
        if is_watched {
            for item in &mut items {
                let primary = item.primary_path().to_string_lossy();
                if let Some(folder_cfg) = find_watched_folder_config_for_path(&primary, &state) {
                    if !folder_cfg.account_email.is_empty() {
                        match item {
                            lumen_core::livephoto::UploadWorkItem::Single { account_email, .. } => {
                                *account_email = folder_cfg.account_email;
                            }
                            lumen_core::livephoto::UploadWorkItem::LivePhoto { account_email, .. } => {
                                *account_email = folder_cfg.account_email;
                            }
                        }
                    }
                }
            }
        }

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
            dry_run: dry_run_override.unwrap_or(if is_watched { false } else { cfg.preferences.dry_run }),
            schedule_enabled: cfg.preferences.schedule_enabled,
            schedule_start_hour: cfg.preferences.schedule_start_hour,
            schedule_end_hour: cfg.preferences.schedule_end_hour,
            pause_on_battery: cfg.preferences.pause_on_battery,
        };

        let is_dry_run_batch = upload_opts.dry_run;

        let config_mgr_for_events = config_manager.clone();

        let engine = UploadEngine::new(
            config_manager,
            if cfg.preferences.use_local_hash_cache { hash_cache } else { None },
            album_registry,
        )
        .with_cancelled(upload_cancel_flag)
        .with_paused(upload_pause_flag);

        let event_handle = tokio::spawn({
            let app_clone = app_clone.clone();
            let logs = logs.clone();
            let log_seq = log_seq.clone();
            let run_id = run_id.clone();
            let config_mgr_for_events = config_mgr_for_events.clone();
            let account_blocked_slot = state_for_drain.account_blocked.clone();
            let mut pending_warnings = Some(preflight_warnings);
            async move {
                while let Some(ev) = event_rx.recv().await {
                    match ev {
                        UploadEvent::BatchStart { total_items, total_bytes } => {
                            let _ = app_clone.emit(
                                "uploadStart",
                                UploadBatchStart {
                                    total: total_items,
                                    total_bytes: total_bytes as u64,
                                    is_dry_run: is_dry_run_batch,
                                },
                            );
                            let _ = app_clone.emit("uploadTotalBytes", total_bytes);
                            if let Some(warnings) = pending_warnings.take() {
                                for w in warnings {
                                    let _ = app_clone.emit(
                                        "uploadWarning",
                                        crate::models::PreflightWarning {
                                            paths: w.paths.into_iter().map(|p| p.to_string_lossy().to_string()).collect(),
                                            code: w.code,
                                            message: w.message,
                                        },
                                    );
                                }
                            }
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

                            let res_model = crate::models::FileUploadResult {
                                media_key: res.media_key.clone(),
                                is_error: res.is_error,
                                is_live_photo: res.is_live_photo,
                                skipped: res.skipped,
                                skip_code: res.skip_code.clone(),
                                skip_reason: res.skip_reason.clone(),
                                error_message: res.error_message.clone(),
                                path: path_str.clone(),
                                paths: res.paths.iter().map(|p| p.to_string_lossy().to_string()).collect(),
                                duration_ms: Some(res.duration_ms as u64),
                                size_bytes: Some(res.size_bytes as u64),
                            };
                            let _ = app_clone.emit("FileStatus", &res_model);

                            let log_id = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis();
                            let seq = log_seq.fetch_add(1, Ordering::Relaxed);
                            let log_entry = LogEntry {
                                id: format!("log-{}-{}", log_id, seq),
                                timestamp: chrono_iso(),
                                level: if is_error { "ERROR".to_string() } else { "INFO".to_string() },
                                category: "Upload".to_string(),
                                run_id: Some(run_id.clone()),
                                file_name: Some(file_name),
                                file_path: Some(path_str),
                                size_bytes: Some(res.size_bytes as u64),
                                duration_ms: Some(res.duration_ms as u64),
                                outcome: if is_error { Some("FAILED".to_string()) } else if skipped { Some("SKIPPED".to_string()) } else { Some("UPLOADED".to_string()) },
                                user_message: error_message.clone().or_else(|| skip_reason.clone()),
                                ..Default::default()
                            };
                            {
                                let mut log_guard = logs.write().unwrap();
                                log_guard.push(log_entry.clone());
                                const MAX_LOGS: usize = 5000;
                                if log_guard.len() > MAX_LOGS {
                                    let excess = log_guard.len() - MAX_LOGS;
                                    log_guard.drain(0..excess);
                                }
                            }
                            let _ = app_clone.emit("log:entry", log_entry);
                        }
                        UploadEvent::ThreadProgress(status) => {
                            let status_model = crate::models::ThreadStatus {
                                worker_id: status.worker_id,
                                status: status.status,
                                file_path: status.file_path,
                                file_name: status.file_name,
                                message: status.message,
                                bytes_uploaded: status.bytes_uploaded.max(0) as u64,
                                bytes_total: status.bytes_total.max(0) as u64,
                                attempt: status.attempt,
                            };
                            let _ = app_clone.emit("ThreadStatus", status_model);
                        }
                        UploadEvent::AlbumProgress {
                            album_name,
                            items_added,
                            total_items,
                            album_keys,
                            is_complete,
                        } => {
                            let status_model = crate::models::AlbumStatus {
                                album_name: album_name.clone(),
                                items_added,
                                total_items,
                                album_keys,
                                is_complete,
                            };
                            let _ = app_clone.emit("albumProgress", &status_model);
                            if is_complete {
                                let _ = app_clone.emit("albumComplete", &status_model);
                            }
                        }
                        UploadEvent::AlbumError { album_name, error } => {
                            let error_model = crate::models::AlbumError {
                                album_name,
                                error,
                            };
                            let _ = app_clone.emit("albumError", error_model);
                        }
                        UploadEvent::AccountBlocked {
                            reason,
                            email,
                            message,
                            resume_at,
                        } => {
                            let blocked_ev = AccountBlockedEvent {
                                reason,
                                email,
                                message,
                                resume_at,
                            };
                            *account_blocked_slot.write().unwrap() = Some(blocked_ev.clone());
                            let _ = app_clone.emit("account:blocked", blocked_ev);
                        }
                        UploadEvent::BatchComplete { uploaded, skipped, failed } => {
                            if notify_on_complete {
                                let locale = config_mgr_for_events.get_config().preferences.locale;
                                let (title, body) = format_batch_complete_notification(&locale, uploaded, skipped, failed);
                                let _ = app_clone.notification()
                                    .builder()
                                    .title(title)
                                    .body(body)
                                    .show();
                            }
                        }
                    }
                }
                if let Some(warnings) = pending_warnings.take() {
                    for w in warnings {
                        let _ = app_clone.emit(
                            "uploadWarning",
                            crate::models::PreflightWarning {
                                paths: w.paths.into_iter().map(|p| p.to_string_lossy().to_string()).collect(),
                                code: w.code,
                                message: w.message,
                            },
                        );
                    }
                }
            }
        });

        let _ = engine.run_upload(items, upload_opts, event_tx).await;
        let _ = event_handle.await;

        let _ = app_clone.emit("uploadStop", ());
        is_uploading_flag.store(false, Ordering::SeqCst);

        spawn_drain_pending_watch_files(app_clone.clone(), state_for_drain.clone());
    });

    Ok(())
}

#[tauri::command]
pub async fn start_upload(
    files: Vec<String>,
    dry_run: Option<bool>,
    recursive: Option<bool>,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    execute_upload(files, app, (*state).clone(), false, dry_run, recursive)
}

#[tauri::command]
pub fn set_dry_run(v: bool, state: tauri::State<AppState>) -> Result<(), String> {
    state.config_manager.update(|cfg| {
        cfg.preferences.dry_run = v;
    })
}

#[tauri::command]
pub fn cancel_upload(state: tauri::State<AppState>) {
    state.upload_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn pause_upload(state: tauri::State<AppState>) {
    state.upload_pause.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn resume_upload(state: tauri::State<AppState>) {
    state.upload_pause.store(false, Ordering::SeqCst);
}

#[tauri::command]
pub fn is_upload_paused(state: tauri::State<AppState>) -> bool {
    state.upload_pause.load(Ordering::Relaxed)
}

#[tauri::command]
pub fn cancel_scan(state: tauri::State<AppState>) {
    state.scan_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub async fn scan_for_review(
    files: Vec<String>,
    recursive: Option<bool>,
    state: tauri::State<'_, AppState>,
) -> Result<ReviewSummary, String> {
    let cfg = state.config_manager.get_config();
    state.scan_cancel.store(false, Ordering::SeqCst);
    let scan_cancel = state.scan_cancel.clone();

    tokio::task::spawn_blocking(move || {
        let has_folder = files.iter().any(|f| {
            std::fs::metadata(f)
                .map(|m| m.is_dir())
                .unwrap_or(false)
        });
        let recursive = recursive.unwrap_or(cfg.preferences.recursive || has_folder);
        let path_bufs: Vec<PathBuf> = files.iter().map(PathBuf::from).collect();
        let scan_opts = ScanOptions {
            recursive,
            filter_unsupported_files: !cfg.preferences.disable_unsupported_files_filter,
            exclude_pattern: cfg.preferences.exclude_pattern.clone(),
            ..Default::default()
        };

        let scanned_files = scan_directories_incremental(
            &path_bufs,
            &scan_opts,
            scan_cancel,
            |_| {},
        )?;

        let live_photo_opts = LivePhotoClassificationOptions {
            enabled: cfg.preferences.pair_live_photos,
            skip_incomplete: cfg.preferences.skip_incomplete_live_photos,
            ignore_apple_metadata: false,
            default_account: cfg.account.selected.clone(),
        };
        let (items, _) = classify_upload_work(&scanned_files, &live_photo_opts);

        let mut review_items = Vec::new();
        let mut total_bytes = 0u64;

        for item in items {
            let p = item.primary_path();
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let path = p.to_string_lossy().to_string();
            let size_bytes: u64 = item
                .all_paths()
                .iter()
                .filter_map(|ap| std::fs::metadata(ap).ok())
                .map(|m| m.len())
                .sum();
            total_bytes += size_bytes;
            review_items.push(ReviewItem {
                name,
                path,
                size_bytes,
            });
        }

        let total_count = review_items.len();
        Ok(ReviewSummary {
            items: review_items,
            total_count,
            total_bytes,
            has_folder,
        })
    })
    .await
    .map_err(|e| format!("Review scan failed: {}", e))?
}

#[tauri::command]
pub fn app_ready(window: Window) {
    let _ = window.show();
    let _ = window.set_focus();
}

fn format_batch_complete_notification(
    locale: &str,
    uploaded: usize,
    skipped: usize,
    failed: usize,
) -> (&'static str, String) {
    match locale {
        "id" => (
            "Pencadangan Selesai",
            format!(
                "Selesai: {} dicadangkan, {} dilewati, {} gagal.",
                uploaded, skipped, failed
            ),
        ),
        "ja" => (
            "バックアップ完了",
            format!(
                "完了: {}件成功、{}件スキップ、{}件失敗",
                uploaded, skipped, failed
            ),
        ),
        "es" => (
            "Copia de seguridad completada",
            format!(
                "Terminado: {} completados, {} omitidos, {} fallidos.",
                uploaded, skipped, failed
            ),
        ),
        _ => (
            "Backup Complete",
            format!(
                "Finished: {} completed, {} skipped, {} failed.",
                uploaded, skipped, failed
            ),
        ),
    }
}

fn format_test_notification(locale: &str) -> (&'static str, &'static str) {
    match locale {
        "id" => ("Lumen", "Pencadangan batch selesai (notifikasi uji coba)"),
        "ja" => ("Lumen", "バックアップバッチが完了しました（テスト通知）"),
        "es" => ("Lumen", "Lote de copia completado (notificación de prueba)"),
        _ => ("Lumen", "Backup batch complete (test notification)"),
    }
}

#[tauri::command]
pub fn test_notification(
    title: Option<String>,
    body: Option<String>,
    app: AppHandle,
    state: tauri::State<AppState>,
) {
    let cfg = state.config_manager.get_config();
    let (default_title, default_body) = format_test_notification(&cfg.preferences.locale);
    let final_title = title
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| default_title.to_string());
    let final_body = body
        .filter(|b| !b.trim().is_empty())
        .unwrap_or_else(|| default_body.to_string());

    let _ = app
        .notification()
        .builder()
        .title(final_title)
        .body(final_body)
        .show();
}

// -------------------------------------------------------------
// Window & System Runtime Helpers
// -------------------------------------------------------------

#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    let trimmed = url.trim();
    let parsed = url::Url::parse(trimmed).map_err(|e| format!("Invalid URL: {}", e))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err("Only http/https URLs are permitted to be opened".to_string());
    }
    if parsed.host_str().is_none() {
        return Err("URL must specify a valid host".to_string());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URLs containing embedded credentials are not permitted".to_string());
    }
    app.opener().open_url(parsed.as_str(), None::<&str>).map_err(|e| e.to_string())
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
pub fn clipboard_set_text(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clipboard_get_text() -> String {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => clipboard.get_text().unwrap_or_default(),
        Err(e) => {
            tracing::warn!("Failed to access clipboard: {}", e);
            String::new()
        }
    }
}

#[tauri::command]
pub fn open_devtools(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        w.open_devtools();
    }
}

#[tauri::command]
pub fn toggle_devtools(app: AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_devtools_open() {
            w.close_devtools();
        } else {
            w.open_devtools();
        }
    }
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
    Err("ADB token binding extraction is not supported in this version. Please sign in with Embedded Setup instead.".to_string())
}

#[tauri::command]
pub async fn get_matugen_status() -> crate::matugen::MatugenStatus {
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(|| {
            if let Some((path, colors)) = crate::matugen::linux::find_active_color_source() {
                crate::matugen::MatugenStatus {
                    supported: true,
                    available: true,
                    active_path: Some(path.to_string_lossy().to_string()),
                    colors: Some(colors),
                }
            } else {
                let has_cli = std::path::Path::new("/usr/bin/matugen").exists()
                    || std::process::Command::new("which")
                        .arg("matugen")
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false);
                crate::matugen::MatugenStatus {
                    supported: true,
                    available: has_cli,
                    active_path: None,
                    colors: None,
                }
            }
        })
        .await
        .unwrap_or(crate::matugen::MatugenStatus {
            supported: true,
            available: false,
            active_path: None,
            colors: None,
        })
    }
    #[cfg(not(target_os = "linux"))]
    {
        crate::matugen::non_linux::get_status()
    }
}

#[tauri::command]
pub async fn get_matugen_colors() -> Option<std::collections::HashMap<String, String>> {
    #[cfg(target_os = "linux")]
    {
        tokio::task::spawn_blocking(|| {
            crate::matugen::linux::find_active_color_source().map(|(_, colors)| colors)
        })
        .await
        .ok()
        .flatten()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[tauri::command]
pub async fn reload_matugen_colors() -> Option<std::collections::HashMap<String, String>> {
    get_matugen_colors().await
}

fn chrono_iso() -> String {
    let now = std::time::SystemTime::now();
    let dt: chrono::DateTime<chrono::Utc> = now.into();
    dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
