//! Models and JSON shapes matching Go Wails backend exactly

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub proxy: String,
    pub use_quota: bool,
    pub saver: bool,
    pub recursive: bool,
    pub force_upload: bool,
    pub pair_live_photos: bool,
    pub skip_incomplete_live_photos: bool,
    pub update_existing_photos_to_live: bool,
    pub upload_threads: u32,
    pub delete_from_host: bool,
    pub disable_unsupported_files_filter: bool,
    pub set_date_from_filename: bool,
    pub exclude_pattern: String,
    pub minimize_to_tray: bool,
    pub launch_on_startup: bool,
    pub notify_on_complete: bool,
    pub use_local_hash_cache: bool,
    #[serde(rename = "uploadSpeedLimitKBps")]
    pub upload_speed_limit_kbps: u32,
    pub watch_folders_enabled: bool,
    pub watched_folders: Vec<String>,
    pub watched_folder_configs: Vec<WatchedFolderConfig>,
    pub recent_albums: Vec<String>,
    pub auto_check_updates: bool,
    pub last_update_check: i64,
    pub album_name: String,
    pub album_auto_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub email: String,
    pub needs_token_binding: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountsState {
    pub accounts: Vec<AccountSummary>,
    pub selected: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchedFolderConfig {
    pub path: String,
    pub account_email: String,
    pub enabled: bool,
    pub include_subfolders: bool,
    pub album_mode: String,
    pub album_name_or_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: String,
    pub checked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UploadBatchStart {
    pub total: usize,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PreflightWarning {
    pub paths: Vec<String>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ThreadStatus {
    #[serde(rename = "WorkerID")]
    pub worker_id: usize,
    pub status: String,
    pub file_path: String,
    pub file_name: String,
    pub message: String,
    pub bytes_uploaded: u64,
    pub bytes_total: u64,
    pub attempt: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct FileUploadResult {
    pub media_key: String,
    pub is_error: bool,
    pub is_live_photo: bool,
    pub skipped: bool,
    pub skip_code: String,
    pub skip_reason: String,
    pub error_message: String,
    pub path: String,
    pub paths: Vec<String>,
    #[serde(rename = "DurationMs", skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(rename = "SizeBytes", skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AlbumStatus {
    pub album_name: String,
    pub items_added: usize,
    pub total_items: usize,
    pub album_keys: Vec<String>,
    pub is_complete: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AlbumError {
    pub album_name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBlockedEvent {
    pub reason: String,
    pub email: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgressEvent {
    pub files_found: usize,
    pub current_directory: String,
    pub is_scanning: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilesDroppedEvent {
    pub files: Vec<String>,
    pub drop_zone: String,
}
