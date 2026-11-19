use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::warn;

use crate::album::{AlbumManager, AlbumRegistry};
use crate::api::{Api, ApiError, ApiOptions, ScottyFinalizeToken};
use crate::auth::TokenManager;
use crate::config::ConfigManager;
use crate::dedup::{calculate_file_sha1, HashCache};
use crate::filename_parser::parse_timestamp_from_filename;
use crate::livephoto::{LivePhotoPair, UploadWorkItem};
use crate::safety::{classify_error, is_retryable, CircuitBreaker, ErrorKind};

const MAX_TRANSIENT_RETRIES: usize = 3;

fn resolve_upload_timestamp(path: &Path, metadata: &fs::Metadata, set_date_from_filename: bool) -> i64 {
    if set_date_from_filename {
        if let Some(ts) = parse_timestamp_from_filename(path) {
            return ts;
        }
    }
    metadata
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

async fn calculate_file_sha1_async(path: &Path) -> std::io::Result<(Vec<u8>, String)> {
    let owned = path.to_path_buf();
    tokio::task::spawn_blocking(move || calculate_file_sha1(&owned))
        .await
        .map_err(|e| std::io::Error::other(format!("hash task join failed: {}", e)))?
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadOptions {
    pub threads: usize,
    pub saver: bool,
    pub use_quota: bool,
    pub recursive: bool,
    pub force_upload: bool,
    pub delete_from_host: bool,
    pub pair_live_photos: bool,
    pub skip_incomplete_live_photos: bool,
    pub update_existing_photos_to_live: bool,
    pub ignore_apple_metadata: bool,
    pub set_date_from_filename: bool,
    pub disable_unsupported_files_filter: bool,
    pub exclude_pattern: String,
    pub use_local_hash_cache: bool,
    pub upload_speed_limit_kbps: u32,
    pub album_name: String,
    pub album_auto_mode: bool,
    pub custom_endpoint_base: Option<String>,
    pub dry_run: bool,
    pub schedule_enabled: bool,
    pub schedule_start_hour: u8,
    pub schedule_end_hour: u8,
    pub pause_on_battery: bool,
}

impl Default for UploadOptions {
    fn default() -> Self {
        Self {
            threads: 3,
            saver: false,
            use_quota: false,
            recursive: false,
            force_upload: false,
            delete_from_host: false,
            pair_live_photos: false,
            skip_incomplete_live_photos: true,
            update_existing_photos_to_live: false,
            ignore_apple_metadata: false,
            set_date_from_filename: false,
            disable_unsupported_files_filter: false,
            exclude_pattern: String::new(),
            use_local_hash_cache: true,
            upload_speed_limit_kbps: 0,
            album_name: String::new(),
            album_auto_mode: false,
            custom_endpoint_base: None,
            dry_run: false,
            schedule_enabled: false,
            schedule_start_hour: 1,
            schedule_end_hour: 7,
            pause_on_battery: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileUploadResult {
    #[serde(rename = "mediaKey")]
    pub media_key: String,
    #[serde(rename = "isError")]
    pub is_error: bool,
    #[serde(rename = "isLivePhoto")]
    pub is_live_photo: bool,
    pub skipped: bool,
    #[serde(rename = "skipCode")]
    pub skip_code: String,
    #[serde(rename = "skipReason")]
    pub skip_reason: String,
    #[serde(rename = "errorMessage")]
    pub error_message: String,
    pub path: PathBuf,
    pub paths: Vec<PathBuf>,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: i64,
    #[serde(rename = "durationMs")]
    pub duration_ms: i64,
    #[serde(rename = "accountEmail")]
    pub account_email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadStatus {
    #[serde(rename = "workerId")]
    pub worker_id: usize,
    pub status: String, // "idle", "hashing", "checking", "uploading", "finalizing", "completed", "skipped", "error"
    #[serde(rename = "filePath")]
    pub file_path: String,
    #[serde(rename = "fileName")]
    pub file_name: String,
    pub message: String,
    #[serde(rename = "bytesUploaded")]
    pub bytes_uploaded: i64,
    #[serde(rename = "bytesTotal")]
    pub bytes_total: i64,
    pub attempt: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UploadEvent {
    BatchStart { total_items: usize, total_bytes: i64 },
    ThreadProgress(ThreadStatus),
    FileCompleted(FileUploadResult),
    AccountBlocked {
        email: String,
        reason: String,
        message: String,
        resume_at: Option<i64>,
    },
    AlbumProgress {
        album_name: String,
        items_added: usize,
        total_items: usize,
        album_keys: Vec<String>,
        is_complete: bool,
    },
    AlbumError {
        album_name: String,
        error: String,
    },
    BatchComplete { uploaded: usize, skipped: usize, failed: usize },
}

pub struct UploadEngine {
    config_manager: Arc<ConfigManager>,
    hash_cache: Option<HashCache>,
    album_registry: AlbumRegistry,
    cancelled: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
}

impl UploadEngine {
    pub fn new(
        config_manager: Arc<ConfigManager>,
        hash_cache: Option<HashCache>,
        album_registry: AlbumRegistry,
    ) -> Self {
        Self {
            config_manager,
            hash_cache,
            album_registry,
            cancelled: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_cancelled(mut self, cancelled: Arc<AtomicBool>) -> Self {
        self.cancelled = cancelled;
        self
    }

    pub fn with_paused(mut self, paused: Arc<AtomicBool>) -> Self {
        self.paused = paused;
        self
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub async fn run_upload(
        &self,
        work_items: Vec<UploadWorkItem>,
        opts: UploadOptions,
        event_tx: mpsc::Sender<UploadEvent>,
    ) -> Result<Vec<FileUploadResult>, String> {
        let total_items = work_items.len();
        let mut total_bytes: i64 = 0;
        for item in &work_items {
            for p in item.all_paths() {
                if let Ok(m) = fs::symlink_metadata(p) {
                    if m.is_file() && !m.file_type().is_symlink() {
                        total_bytes += m.len() as i64;
                    }
                }
            }
        }

        let _ = event_tx.send(UploadEvent::BatchStart {
            total_items,
            total_bytes,
        }).await;

        if work_items.is_empty() {
            let _ = event_tx.send(UploadEvent::BatchComplete {
                uploaded: 0,
                skipped: 0,
                failed: 0,
            }).await;
            return Ok(Vec::new());
        }

        const MAX_UPLOAD_THREADS: usize = 32;
        let num_threads = opts
            .threads
            .clamp(1, MAX_UPLOAD_THREADS)
            .min(work_items.len());
        let shared_rate_limiter = crate::net::GlobalRateLimiter::new((opts.upload_speed_limit_kbps as u64) * 1024);

        // Circuit breakers per account
        let breakers: Arc<tokio::sync::Mutex<HashMap<String, Arc<CircuitBreaker>>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        // Token managers per account
        let token_managers: Arc<tokio::sync::Mutex<HashMap<String, TokenManager>>> =
            Arc::new(tokio::sync::Mutex::new(HashMap::new()));

        let queue = Arc::new(tokio::sync::Mutex::new(VecDeque::from(work_items)));
        let results = Arc::new(tokio::sync::Mutex::new(Vec::<FileUploadResult>::new()));

        let mut tasks = Vec::new();

        for worker_id in 0..num_threads {
            let queue = queue.clone();
            let results = results.clone();
            let event_tx = event_tx.clone();
            let config_mgr = self.config_manager.clone();
            let hash_cache = self.hash_cache.clone();
            let opts = opts.clone();
            let cancelled = self.cancelled.clone();
            let paused = self.paused.clone();
            let breakers = breakers.clone();
            let token_managers = token_managers.clone();
            let worker_limiter = shared_rate_limiter.clone();

            tasks.push(tokio::spawn(async move {
                let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                    worker_id,
                    status: "idle".to_string(),
                    file_path: String::new(),
                    file_name: String::new(),
                    message: "Waiting for files...".to_string(),
                    bytes_uploaded: 0,
                    bytes_total: 0,
                    attempt: 0,
                })).await;

                'worker: loop {
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }

                    // Check user pause
                    while paused.load(Ordering::Relaxed) {
                        if cancelled.load(Ordering::Relaxed) {
                            break 'worker;
                        }
                        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                            worker_id,
                            status: "paused".to_string(),
                            file_path: String::new(),
                            file_name: String::new(),
                            message: "Backup paused".to_string(),
                            bytes_uploaded: 0,
                            bytes_total: 0,
                            attempt: 0,
                        })).await;
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }

                    // Check quiet hours schedule
                    while opts.schedule_enabled && !crate::safety::is_within_schedule(opts.schedule_start_hour, opts.schedule_end_hour) {
                        if cancelled.load(Ordering::Relaxed) {
                            break 'worker;
                        }
                        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                            worker_id,
                            status: "paused".to_string(),
                            file_path: String::new(),
                            file_name: String::new(),
                            message: "Backup paused (quiet hours schedule)".to_string(),
                            bytes_uploaded: 0,
                            bytes_total: 0,
                            attempt: 0,
                        })).await;
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }

                    // Check battery power
                    while opts.pause_on_battery && crate::safety::is_running_on_battery() {
                        if cancelled.load(Ordering::Relaxed) {
                            break 'worker;
                        }
                        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                            worker_id,
                            status: "paused".to_string(),
                            file_path: String::new(),
                            file_name: String::new(),
                            message: "Backup paused on battery power".to_string(),
                            bytes_uploaded: 0,
                            bytes_total: 0,
                            attempt: 0,
                        })).await;
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                    }

                    if cancelled.load(Ordering::Relaxed) {
                        break 'worker;
                    }

                    let item = {
                        let mut q = queue.lock().await;
                        q.pop_front()
                    };

                    let item = match item {
                        Some(it) => it,
                        None => break,
                    };

                    let account_email = item.account_email().to_string();

                    // Check circuit breaker for this specific account (Requirement 7 & 10)
                    let breaker = {
                        let mut b_map = breakers.lock().await;
                        b_map.entry(account_email.clone()).or_insert_with(|| {
                            let tx = event_tx.clone();
                            Arc::new(CircuitBreaker::new(move |event| {
                                let _ = tx.try_send(UploadEvent::AccountBlocked {
                                    email: event.email,
                                    reason: event.reason,
                                    message: event.message,
                                    resume_at: event.resume_at,
                                });
                            }))
                        }).clone()
                    };

                    if breaker.is_blocked() {
                        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                            worker_id,
                            status: "error".to_string(),
                            file_path: item.primary_path().to_string_lossy().to_string(),
                            file_name: item.primary_path().file_name().unwrap_or_default().to_string_lossy().to_string(),
                            message: "Stopped: Account blocked".to_string(),
                            bytes_uploaded: 0,
                            bytes_total: 0,
                            attempt: 0,
                        })).await;

                        let res = FileUploadResult {
                            media_key: String::new(),
                            is_error: true,
                            is_live_photo: matches!(item, UploadWorkItem::LivePhoto { .. }),
                            skipped: false,
                            skip_code: "account-blocked".to_string(),
                            skip_reason: String::new(),
                            error_message: "Account blocked by circuit breaker".to_string(),
                            path: item.primary_path().to_path_buf(),
                            paths: item.all_paths().into_iter().map(Path::to_path_buf).collect(),
                            size_bytes: 0,
                            duration_ms: 0,
                            account_email,
                        };
                        results.lock().await.push(res.clone());
                        let _ = event_tx.send(UploadEvent::FileCompleted(res)).await;
                        continue;
                    }

                    // If another worker paused this account on 429 rate-limiting, wait before pulling traffic
                    while let Some(rem) = breaker.pause_remaining() {
                        if cancelled.load(Ordering::Relaxed) || breaker.is_blocked() {
                            break;
                        }
                        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                            worker_id,
                            status: "paused".to_string(),
                            file_path: String::new(),
                            file_name: String::new(),
                            message: "Rate limited, waiting to resume...".to_string(),
                            bytes_uploaded: 0,
                            bytes_total: 0,
                            attempt: 0,
                        })).await;
                        tokio::time::sleep(rem.min(std::time::Duration::from_millis(500))).await;
                    }

                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }

                    // Retrieve credential for this item's account
                    let cred = match config_mgr.get_credential_for_account(Some(&account_email)) {
                        Ok(c) => c,
                        Err(e) => {
                            let res = FileUploadResult {
                                media_key: String::new(),
                                is_error: true,
                                is_live_photo: matches!(item, UploadWorkItem::LivePhoto { .. }),
                                skipped: false,
                                skip_code: "auth-missing".to_string(),
                                skip_reason: String::new(),
                                error_message: e,
                                path: item.primary_path().to_path_buf(),
                                paths: item.all_paths().into_iter().map(Path::to_path_buf).collect(),
                                size_bytes: 0,
                                duration_ms: 0,
                                account_email,
                            };
                            results.lock().await.push(res.clone());
                            let _ = event_tx.send(UploadEvent::FileCompleted(res)).await;
                            continue;
                        }
                    };

                    let proxy = config_mgr.get_preferences().proxy;
                    let api_opts = ApiOptions {
                        account: account_email.clone(),
                        proxy,
                        saver: opts.saver,
                        use_quota: opts.use_quota,
                        speed_limit_kbps: opts.upload_speed_limit_kbps,
                        custom_endpoint_base: opts.custom_endpoint_base.clone(),
                    };

                    let mut api = match Api::new(api_opts, &cred) {
                        Ok(a) => a,
                        Err(e) => {
                            let res = FileUploadResult {
                                media_key: String::new(),
                                is_error: true,
                                is_live_photo: matches!(item, UploadWorkItem::LivePhoto { .. }),
                                skipped: false,
                                skip_code: "api-init-failed".to_string(),
                                skip_reason: String::new(),
                                error_message: e.to_string(),
                                path: item.primary_path().to_path_buf(),
                                paths: item.all_paths().into_iter().map(Path::to_path_buf).collect(),
                                size_bytes: 0,
                                duration_ms: 0,
                                account_email,
                            };
                            results.lock().await.push(res.clone());
                            let _ = event_tx.send(UploadEvent::FileCompleted(res)).await;
                            continue;
                        }
                    };

                    let tm = {
                        let mut tm_map = token_managers.lock().await;
                        tm_map.entry(account_email.clone()).or_insert_with(|| TokenManager::new(&account_email)).clone()
                    };
                    api.set_token_manager(tm);
                    api.set_rate_limiter(worker_limiter.clone());

                    let start_t = Instant::now();
                    let res = Self::process_work_item(
                        worker_id,
                        &item,
                        &api,
                        &opts,
                        hash_cache.as_ref(),
                        &event_tx,
                        &breaker,
                        cancelled.clone(),
                        paused.clone(),
                    ).await;

                    let mut res = res;
                    res.duration_ms = start_t.elapsed().as_millis() as i64;
                    res.account_email = account_email;

                    results.lock().await.push(res.clone());
                    let _ = event_tx.send(UploadEvent::FileCompleted(res)).await;
                }

                let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                    worker_id,
                    status: "idle".to_string(),
                    file_path: String::new(),
                    file_name: String::new(),
                    message: "Completed".to_string(),
                    bytes_uploaded: 0,
                    bytes_total: 0,
                    attempt: 0,
                })).await;
            }));
        }

        for task in tasks {
            let _ = task.await;
        }

        let mut finished_results = results.lock().await.clone();

        // Count outcomes
        let mut count_uploaded: usize = 0;
        let mut count_skipped: usize = 0;
        let mut count_failed: usize = 0;

        // Group successful uploads by (account, album)
        // map: account -> map: album_name -> Vec<(result_idx, media_key)>
        let mut album_batches: HashMap<String, HashMap<String, Vec<(usize, String)>>> = HashMap::new();

        for (idx, res) in finished_results.iter().enumerate() {
            if res.is_error {
                count_failed += 1;
            } else if res.skipped {
                count_skipped += 1;
            } else {
                count_uploaded += 1;
                if !opts.dry_run && !res.media_key.is_empty() && res.skip_code != "dry-run" {
                    let target_album = if opts.album_auto_mode {
                        res.path.parent()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string()
                    } else if !opts.album_name.is_empty() {
                        opts.album_name.clone()
                    } else {
                        String::new()
                    };

                    if !target_album.is_empty() {
                        album_batches
                            .entry(res.account_email.clone())
                            .or_default()
                            .entry(target_album)
                            .or_default()
                            .push((idx, res.media_key.clone()));
                    }
                }
            }
        }

        // Attach to albums if requested
        let configured_proxy = self.config_manager.get_preferences().proxy;
        for (account, albums) in album_batches {
            let cred = self.config_manager.get_credential_for_account(Some(&account)).unwrap_or_default();
            let api_opts = ApiOptions {
                account: account.clone(),
                proxy: configured_proxy.clone(),
                saver: opts.saver,
                use_quota: opts.use_quota,
                speed_limit_kbps: opts.upload_speed_limit_kbps,
                custom_endpoint_base: opts.custom_endpoint_base.clone(),
            };
            match Api::new(api_opts, &cred) {
                Ok(api) => {
                    let mgr = AlbumManager::new(Arc::new(api), &account, self.album_registry.clone());
                    for (name, items) in albums {
                        let keys: Vec<String> = items.iter().map(|(_, k)| k.clone()).collect();
                        let total_items = keys.len();
                        let _ = event_tx.send(UploadEvent::AlbumProgress {
                            album_name: name.clone(),
                            items_added: 0,
                            total_items,
                            album_keys: Vec::new(),
                            is_complete: false,
                        }).await;

                        match mgr.add_to_album(&keys, &name).await {
                            Ok(created_keys) => {
                                tracing::info!("Successfully added {} items to album '{}' (keys: {:?})", keys.len(), name, created_keys);
                                let _ = event_tx.send(UploadEvent::AlbumProgress {
                                    album_name: name.clone(),
                                    items_added: total_items,
                                    total_items,
                                    album_keys: created_keys,
                                    is_complete: true,
                                }).await;
                            }
                            Err(e) => {
                                let err_msg = e.to_string();
                                tracing::error!("Failed to add items to album '{}': {}", name, err_msg);
                                let _ = event_tx.send(UploadEvent::AlbumError {
                                    album_name: name.clone(),
                                    error: err_msg.clone(),
                                }).await;
                                for (idx, _) in items {
                                    if let Some(res) = finished_results.get_mut(idx) {
                                        if !res.is_error {
                                            res.is_error = true;
                                            res.error_message = format!("Album error for '{}': {}", name, err_msg);
                                            count_failed += 1;
                                            count_uploaded = count_uploaded.saturating_sub(1);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let err_msg = format!("Failed to initialize API client for album attachment: {}", e);
                    tracing::error!("{}", err_msg);
                    for (name, items) in albums {
                        let _ = event_tx.send(UploadEvent::AlbumError {
                            album_name: name.clone(),
                            error: err_msg.clone(),
                        }).await;
                        for (idx, _) in items {
                            if let Some(res) = finished_results.get_mut(idx) {
                                if !res.is_error {
                                    res.is_error = true;
                                    res.error_message = format!("Album error for '{}': {}", name, err_msg);
                                    count_failed += 1;
                                    count_uploaded = count_uploaded.saturating_sub(1);
                                }
                            }
                        }
                    }
                }
            }
        }

        let _ = event_tx.send(UploadEvent::BatchComplete {
            uploaded: count_uploaded,
            skipped: count_skipped,
            failed: count_failed,
        }).await;

        if opts.delete_from_host && !opts.dry_run {
            for res in &finished_results {
                if !res.is_error
                    && res.skip_code != "dry-run"
                    && res.skip_code != "live-photo-partial-remote"
                {
                    for p in &res.paths {
                        remove_regular_file_if_not_symlink(p);
                    }
                }
            }
        }

        Ok(finished_results)
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_work_item(
        worker_id: usize,
        item: &UploadWorkItem,
        api: &Api,
        opts: &UploadOptions,
        hash_cache: Option<&HashCache>,
        event_tx: &mpsc::Sender<UploadEvent>,
        breaker: &CircuitBreaker,
        cancelled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> FileUploadResult {
        match item {
            UploadWorkItem::Single { path, account_email } => {
                Self::process_single_file(worker_id, path, account_email, api, opts, hash_cache, event_tx, breaker, cancelled, paused).await
            }
            UploadWorkItem::LivePhoto { pair, account_email } => {
                Self::process_live_photo(worker_id, pair, account_email, api, opts, hash_cache, event_tx, breaker, cancelled, paused).await
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_single_file(
        worker_id: usize,
        path: &Path,
        account_email: &str,
        api: &Api,
        opts: &UploadOptions,
        hash_cache: Option<&HashCache>,
        event_tx: &mpsc::Sender<UploadEvent>,
        breaker: &CircuitBreaker,
        cancelled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> FileUploadResult {
        let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let metadata = match fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) => {
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: false,
                    skipped: false,
                    skip_code: "stat-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: e.to_string(),
                    path: path.to_path_buf(),
                    paths: vec![path.to_path_buf()],
                    size_bytes: 0,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        let file_size = metadata.len() as i64;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return FileUploadResult {
                media_key: String::new(),
                is_error: true,
                is_live_photo: false,
                skipped: false,
                skip_code: "not-a-regular-file".to_string(),
                skip_reason: String::new(),
                error_message: "Path is not a regular file".to_string(),
                path: path.to_path_buf(),
                paths: vec![path.to_path_buf()],
                size_bytes: 0,
                duration_ms: 0,
                account_email: account_email.to_string(),
            };
        }

        // Stage 1: Hashing
        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
            worker_id,
            status: "hashing".to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_name: file_name.clone(),
            message: "Hashing...".to_string(),
            bytes_uploaded: 0,
            bytes_total: file_size,
            attempt: 1,
        })).await;

        let (sha1_bytes, sha1_b64) = match calculate_file_sha1_async(path).await {
            Ok(h) => h,
            Err(e) => {
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: false,
                    skipped: false,
                    skip_code: "hash-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: e.to_string(),
                    path: path.to_path_buf(),
                    paths: vec![path.to_path_buf()],
                    size_bytes: file_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        // Stage 1.5: Local Hash Cache Check
        if !opts.force_upload && opts.use_local_hash_cache {
            if let Some(cache) = hash_cache {
                if let Ok(Some(cached_key)) = cache.get_async(account_email, &sha1_b64, file_size).await {
                    let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                        worker_id,
                        status: "skipped".to_string(),
                        file_path: path.to_string_lossy().to_string(),
                        file_name: file_name.clone(),
                        message: "Already in Google Photos (local cache)".to_string(),
                        bytes_uploaded: file_size,
                        bytes_total: file_size,
                        attempt: 1,
                    })).await;

                    return FileUploadResult {
                        media_key: cached_key,
                        is_error: false,
                        is_live_photo: false,
                        skipped: true,
                        skip_code: "hash-cache".to_string(),
                        skip_reason: "Already in Google Photos (local cache)".to_string(),
                        error_message: String::new(),
                        path: path.to_path_buf(),
                        paths: vec![path.to_path_buf()],
                        size_bytes: file_size,
                        duration_ms: 0,
                        account_email: account_email.to_string(),
                    };
                }
            }
        }

        // Stage 2: Remote Library Hash Check
        if !opts.force_upload {
            let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                worker_id,
                status: "checking".to_string(),
                file_path: path.to_string_lossy().to_string(),
                file_name: file_name.clone(),
                message: "Checking library...".to_string(),
                bytes_uploaded: 0,
                bytes_total: file_size,
                attempt: 1,
            })).await;

            match api.find_remote_media_by_hash(&sha1_bytes).await {
                Ok(Some(remote_key)) => {
                    if opts.use_local_hash_cache {
                        if let Some(cache) = hash_cache {
                            let _ = cache.put_async(account_email, &sha1_b64, &remote_key, file_size, 0).await;
                        }
                    }

                    let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                        worker_id,
                        status: "skipped".to_string(),
                        file_path: path.to_string_lossy().to_string(),
                        file_name: file_name.clone(),
                        message: "Already in Google Photos".to_string(),
                        bytes_uploaded: file_size,
                        bytes_total: file_size,
                        attempt: 1,
                    })).await;

                    return FileUploadResult {
                        media_key: remote_key,
                        is_error: false,
                        is_live_photo: false,
                        skipped: true,
                        skip_code: "remote-dedup".to_string(),
                        skip_reason: "Already in Google Photos (remote dedup)".to_string(),
                        error_message: String::new(),
                        path: path.to_path_buf(),
                        paths: vec![path.to_path_buf()],
                        size_bytes: file_size,
                        duration_ms: 0,
                        account_email: account_email.to_string(),
                    };
                }
                Err(e) => {
                    let err_str = e.to_string();
                    let kind = classify_error(&err_str);
                    if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                        breaker.trip(kind, account_email, &err_str);
                    } else if matches!(e, ApiError::Auth(_)) {
                        breaker.trip(ErrorKind::SignInRequired, account_email, &err_str);
                    }
                    if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) || matches!(e, ApiError::Auth(_)) {
                        return FileUploadResult {
                            media_key: String::new(),
                            is_error: true,
                            is_live_photo: false,
                            skipped: false,
                            skip_code: "auth-failure".to_string(),
                            skip_reason: String::new(),
                            error_message: err_str,
                            path: path.to_path_buf(),
                            paths: vec![path.to_path_buf()],
                            size_bytes: file_size,
                            duration_ms: 0,
                            account_email: account_email.to_string(),
                        };
                    }
                    warn!("Remote hash check warning: {}, proceeding with upload", e);
                }
                _ => {}
            }
        }

        if opts.dry_run {
            let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                worker_id,
                status: "dry-run".to_string(),
                file_path: path.to_string_lossy().to_string(),
                file_name: file_name.clone(),
                message: "Dry run: would back up".to_string(),
                bytes_uploaded: file_size,
                bytes_total: file_size,
                attempt: 1,
            })).await;

            return FileUploadResult {
                media_key: "dry_run_key".to_string(),
                is_error: false,
                is_live_photo: false,
                skipped: false,
                skip_code: "dry-run".to_string(),
                skip_reason: "Dry run: file not in Google Photos and would be backed up".to_string(),
                error_message: String::new(),
                path: path.to_path_buf(),
                paths: vec![path.to_path_buf()],
                size_bytes: file_size,
                duration_ms: 0,
                account_email: account_email.to_string(),
            };
        }

        let timestamp = resolve_upload_timestamp(path, &metadata, opts.set_date_from_filename);
        let mut final_media_key: Option<String> = None;
        let mut cached_finalize_token: Option<ScottyFinalizeToken> = None;

        for attempt in 1..=MAX_TRANSIENT_RETRIES {
            let mut stage_err: Option<(&'static str, String)> = None;

            if cached_finalize_token.is_none() {
                let tx = event_tx.clone();
                let p_str = path.to_string_lossy().to_string();
                let fn_str = file_name.clone();
                match Self::obtain_and_upload_resumable(
                    api,
                    hash_cache,
                    account_email,
                    path,
                    &sha1_b64,
                    file_size,
                    opts.force_upload && attempt == 1,
                    cancelled.clone(),
                    paused.clone(),
                    move |bytes_up, total, att| {
                        let effective_attempt = attempt.max(att);
                        let _ = tx.try_send(UploadEvent::ThreadProgress(ThreadStatus {
                            worker_id,
                            status: "uploading".to_string(),
                            file_path: p_str.clone(),
                            file_name: fn_str.clone(),
                            message: if effective_attempt > 1 {
                                format!("Retrying... (attempt {})", effective_attempt)
                            } else {
                                "Uploading...".to_string()
                            },
                            bytes_uploaded: bytes_up,
                            bytes_total: total,
                            attempt: effective_attempt,
                        }));
                    },
                )
                .await
                {
                    Ok(tok) => cached_finalize_token = Some(tok),
                    Err((code, msg)) => {
                        stage_err = Some((code, msg));
                    }
                }
            }

            if let Some(ref finalize_token) = cached_finalize_token {
                // Stage 5: Committing
                let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                    worker_id,
                    status: "finalizing".to_string(),
                    file_path: path.to_string_lossy().to_string(),
                    file_name: file_name.clone(),
                    message: "Committing backup...".to_string(),
                    bytes_uploaded: file_size,
                    bytes_total: file_size,
                    attempt,
                })).await;

                match api.commit_upload(finalize_token, &file_name, &sha1_bytes, timestamp).await {
                    Ok(k) => {
                        final_media_key = Some(k);
                        break;
                    }
                    Err(e) => {
                        stage_err = Some(("commit-failed", e.to_string()));
                    }
                }
            }

            if let Some((skip_code, err_str)) = stage_err {
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted)
                    || err_str.starts_with("Authentication error:")
                {
                    let trip_kind = if kind == ErrorKind::AccountRestricted {
                        ErrorKind::AccountRestricted
                    } else {
                        ErrorKind::SignInRequired
                    };
                    breaker.trip(trip_kind, account_email, &err_str);
                }

                let is_rate_limited = kind == ErrorKind::RateLimited;
                if (is_retryable(&err_str) || is_rate_limited)
                    && attempt < MAX_TRANSIENT_RETRIES
                    && !cancelled.load(Ordering::Relaxed)
                    && !breaker.is_blocked()
                {
                    let backoff = if is_rate_limited {
                        let d = std::time::Duration::from_secs(30);
                        breaker.pause(d, account_email, &err_str);
                        d
                    } else {
                        std::time::Duration::from_secs(attempt as u64)
                    };
                    let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                        worker_id,
                        status: "retrying".to_string(),
                        file_path: path.to_string_lossy().to_string(),
                        file_name: file_name.clone(),
                        message: if is_rate_limited {
                            format!("Rate limited, waiting to retry ({}/{})...", attempt + 1, MAX_TRANSIENT_RETRIES)
                        } else {
                            format!("Transient error, retrying ({}/{})...", attempt + 1, MAX_TRANSIENT_RETRIES)
                        },
                        bytes_uploaded: 0,
                        bytes_total: file_size,
                        attempt: attempt + 1,
                    })).await;
                    Self::sleep_cancellable(backoff, &cancelled).await;
                    continue;
                }

                if !cancelled.load(Ordering::Relaxed) && !is_retryable(&err_str) && !is_rate_limited {
                    if let Some(cache) = hash_cache {
                        let _ = cache.remove_upload_session_async(account_email, &sha1_b64, file_size).await;
                    }
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: false,
                    skipped: false,
                    skip_code: skip_code.to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: path.to_path_buf(),
                    paths: vec![path.to_path_buf()],
                    size_bytes: file_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        }

        let media_key = final_media_key.unwrap_or_default();

        if let Some(cache) = hash_cache {
            let _ = cache.remove_upload_session_async(account_email, &sha1_b64, file_size).await;
        }

        if opts.use_local_hash_cache {
            if let Some(cache) = hash_cache {
                let _ = cache.put_async(account_email, &sha1_b64, &media_key, file_size, timestamp).await;
            }
        }

        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
            worker_id,
            status: "completed".to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_name: file_name.clone(),
            message: "Backup completed".to_string(),
            bytes_uploaded: file_size,
            bytes_total: file_size,
            attempt: 1,
        })).await;

        FileUploadResult {
            media_key,
            is_error: false,
            is_live_photo: false,
            skipped: false,
            skip_code: String::new(),
            skip_reason: String::new(),
            error_message: String::new(),
            path: path.to_path_buf(),
            paths: vec![path.to_path_buf()],
            size_bytes: file_size,
            duration_ms: 0,
            account_email: account_email.to_string(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn process_live_photo(
        worker_id: usize,
        pair: &LivePhotoPair,
        account_email: &str,
        api: &Api,
        opts: &UploadOptions,
        hash_cache: Option<&HashCache>,
        event_tx: &mpsc::Sender<UploadEvent>,
        breaker: &CircuitBreaker,
        cancelled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> FileUploadResult {
        let p_name = pair.photo_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let v_name = pair.video_path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let display_name = format!("{} + {}", p_name, v_name);

        let p_meta = match fs::symlink_metadata(&pair.photo_path) {
            Ok(m) => m,
            Err(e) => {
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "stat-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: e.to_string(),
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: 0,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        let v_meta = match fs::symlink_metadata(&pair.video_path) {
            Ok(m) => m,
            Err(e) => {
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "stat-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: e.to_string(),
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: 0,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        let total_size = (p_meta.len() + v_meta.len()) as i64;
        if !p_meta.is_file() || p_meta.file_type().is_symlink() || !v_meta.is_file() || v_meta.file_type().is_symlink() {
            return FileUploadResult {
                media_key: String::new(),
                is_error: true,
                is_live_photo: true,
                skipped: false,
                skip_code: "not-a-regular-file".to_string(),
                skip_reason: String::new(),
                error_message: "One or more components of the Live Photo pair is not a regular file".to_string(),
                path: pair.photo_path.clone(),
                paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                size_bytes: 0,
                duration_ms: 0,
                account_email: account_email.to_string(),
            };
        }

        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
            worker_id,
            status: "hashing".to_string(),
            file_path: pair.photo_path.to_string_lossy().to_string(),
            file_name: display_name.clone(),
            message: "Hashing Live Photo pair...".to_string(),
            bytes_uploaded: 0,
            bytes_total: total_size,
            attempt: 1,
        })).await;

        let (photo_sha1, photo_b64) = match calculate_file_sha1_async(&pair.photo_path).await {
            Ok(h) => h,
            Err(e) => {
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "hash-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: e.to_string(),
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        let (video_sha1, video_b64) = match calculate_file_sha1_async(&pair.video_path).await {
            Ok(h) => h,
            Err(e) => {
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "hash-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: e.to_string(),
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        // Check deduplication
        let mut reconcile_existing_photo = false;
        if !opts.force_upload {
            let mut photo_remote = None;
            let mut video_remote = None;

            if opts.use_local_hash_cache {
                if let Some(cache) = hash_cache {
                    photo_remote = cache.get_async(account_email, &photo_b64, p_meta.len() as i64).await.ok().flatten();
                    video_remote = cache.get_async(account_email, &video_b64, v_meta.len() as i64).await.ok().flatten();
                }
            }

            if photo_remote.is_none() {
                match api.find_remote_media_by_hash(&photo_sha1).await {
                    Ok(res) => photo_remote = res,
                    Err(e) => {
                        let err_str = e.to_string();
                        let kind = classify_error(&err_str);
                        if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                            breaker.trip(kind, account_email, &err_str);
                        } else if matches!(e, ApiError::Auth(_)) {
                            breaker.trip(ErrorKind::SignInRequired, account_email, &err_str);
                        }
                        if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) || matches!(e, ApiError::Auth(_)) {
                            return FileUploadResult {
                                media_key: String::new(),
                                is_error: true,
                                is_live_photo: true,
                                skipped: false,
                                skip_code: "auth-failure".to_string(),
                                skip_reason: String::new(),
                                error_message: err_str,
                                path: pair.photo_path.clone(),
                                paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                                size_bytes: total_size,
                                duration_ms: 0,
                                account_email: account_email.to_string(),
                            };
                        }
                        warn!("Remote photo hash check warning: {}, proceeding with upload", e);
                    }
                }
            }
            if video_remote.is_none() {
                match api.find_remote_media_by_hash(&video_sha1).await {
                    Ok(res) => video_remote = res,
                    Err(e) => {
                        let err_str = e.to_string();
                        let kind = classify_error(&err_str);
                        if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                            breaker.trip(kind, account_email, &err_str);
                        } else if matches!(e, ApiError::Auth(_)) {
                            breaker.trip(ErrorKind::SignInRequired, account_email, &err_str);
                        }
                        if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) || matches!(e, ApiError::Auth(_)) {
                            return FileUploadResult {
                                media_key: String::new(),
                                is_error: true,
                                is_live_photo: true,
                                skipped: false,
                                skip_code: "auth-failure".to_string(),
                                skip_reason: String::new(),
                                error_message: err_str,
                                path: pair.photo_path.clone(),
                                paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                                size_bytes: total_size,
                                duration_ms: 0,
                                account_email: account_email.to_string(),
                            };
                        }
                        warn!("Remote video hash check warning: {}, proceeding with upload", e);
                    }
                }
            }

            let should_skip_live = video_remote.is_some()
                || (photo_remote.is_some() && !opts.update_existing_photos_to_live);
            if should_skip_live {
                let both_remote = photo_remote.is_some() && video_remote.is_some();
                let skip_code = if both_remote {
                    "remote-dedup"
                } else {
                    "live-photo-partial-remote"
                };
                let skip_reason = if both_remote {
                    "Skipped: Live Photo already exists remotely"
                } else {
                    "Skipped: a Live Photo component already exists remotely"
                };
                let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                    worker_id,
                    status: "skipped".to_string(),
                    file_path: pair.photo_path.to_string_lossy().to_string(),
                    file_name: display_name.clone(),
                    message: skip_reason.to_string(),
                    bytes_uploaded: total_size,
                    bytes_total: total_size,
                    attempt: 1,
                })).await;

                return FileUploadResult {
                    media_key: photo_remote.or(video_remote).unwrap_or_default(),
                    is_error: false,
                    is_live_photo: true,
                    skipped: true,
                    skip_code: skip_code.to_string(),
                    skip_reason: skip_reason.to_string(),
                    error_message: String::new(),
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
            reconcile_existing_photo = photo_remote.is_some() && opts.update_existing_photos_to_live;
        }

        if opts.dry_run {
            let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                worker_id,
                status: "dry-run".to_string(),
                file_path: pair.photo_path.to_string_lossy().to_string(),
                file_name: display_name.clone(),
                message: "Dry run: would back up Live Photo".to_string(),
                bytes_uploaded: total_size,
                bytes_total: total_size,
                attempt: 1,
            })).await;

            return FileUploadResult {
                media_key: "dry_run_key".to_string(),
                is_error: false,
                is_live_photo: true,
                skipped: false,
                skip_code: "dry-run".to_string(),
                skip_reason: "Dry run: Live Photo not in Google Photos and would be backed up".to_string(),
                error_message: String::new(),
                path: pair.photo_path.clone(),
                paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                size_bytes: total_size,
                duration_ms: 0,
                account_email: account_email.to_string(),
            };
        }

        let timestamp = resolve_upload_timestamp(&pair.photo_path, &p_meta, opts.set_date_from_filename);
        let p_size = p_meta.len() as i64;
        let v_size = v_meta.len() as i64;
        let mut final_media_key: Option<String> = None;
        let mut cached_photo_finalize: Option<ScottyFinalizeToken> = None;
        let mut cached_video_finalize: Option<ScottyFinalizeToken> = None;

        for attempt in 1..=MAX_TRANSIENT_RETRIES {
            let mut stage_err: Option<(&'static str, String)> = None;

            if cached_photo_finalize.is_none() {
                let tx = event_tx.clone();
                let p_str = pair.photo_path.to_string_lossy().to_string();
                let disp_name = display_name.clone();
                match Self::obtain_and_upload_resumable(
                    api,
                    hash_cache,
                    account_email,
                    &pair.photo_path,
                    &photo_b64,
                    p_size,
                    opts.force_upload && attempt == 1,
                    cancelled.clone(),
                    paused.clone(),
                    move |bytes_up, _, att| {
                        let effective_attempt = attempt.max(att);
                        let _ = tx.try_send(UploadEvent::ThreadProgress(ThreadStatus {
                            worker_id,
                            status: "uploading".to_string(),
                            file_path: p_str.clone(),
                            file_name: disp_name.clone(),
                            message: if effective_attempt > 1 {
                                format!("Uploading photo (attempt {})...", effective_attempt)
                            } else {
                                "Uploading photo...".to_string()
                            },
                            bytes_uploaded: bytes_up,
                            bytes_total: total_size,
                            attempt: effective_attempt,
                        }));
                    },
                )
                .await
                {
                    Ok(tok) => cached_photo_finalize = Some(tok),
                    Err((code, msg)) => stage_err = Some((code, msg)),
                }
            }

            if cached_photo_finalize.is_some() && cached_video_finalize.is_none() {
                let tx = event_tx.clone();
                let v_str = pair.video_path.to_string_lossy().to_string();
                let disp_name = display_name.clone();
                match Self::obtain_and_upload_resumable(
                    api,
                    hash_cache,
                    account_email,
                    &pair.video_path,
                    &video_b64,
                    v_size,
                    opts.force_upload && attempt == 1,
                    cancelled.clone(),
                    paused.clone(),
                    move |bytes_up, _, att| {
                        let effective_attempt = attempt.max(att);
                        let _ = tx.try_send(UploadEvent::ThreadProgress(ThreadStatus {
                            worker_id,
                            status: "uploading".to_string(),
                            file_path: v_str.clone(),
                            file_name: disp_name.clone(),
                            message: if effective_attempt > 1 {
                                format!("Uploading video (attempt {})...", effective_attempt)
                            } else {
                                "Uploading video...".to_string()
                            },
                            bytes_uploaded: p_size + bytes_up,
                            bytes_total: total_size,
                            attempt: effective_attempt,
                        }));
                    },
                )
                .await
                {
                    Ok(tok) => cached_video_finalize = Some(tok),
                    Err((code, msg)) => stage_err = Some((code, msg)),
                }
            }

            if let (Some(ref p_tok), Some(ref v_tok)) = (&cached_photo_finalize, &cached_video_finalize) {
                let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                    worker_id,
                    status: "finalizing".to_string(),
                    file_path: pair.photo_path.to_string_lossy().to_string(),
                    file_name: display_name.clone(),
                    message: "Committing Live Photo...".to_string(),
                    bytes_uploaded: total_size,
                    bytes_total: total_size,
                    attempt,
                })).await;

                let reconcile_sha1 = if reconcile_existing_photo {
                    Some(&photo_sha1[..])
                } else {
                    None
                };

                match api.commit_live_photo_with_reconcile(
                    p_tok,
                    v_tok,
                    &p_name,
                    &photo_sha1,
                    &video_sha1,
                    timestamp,
                    reconcile_sha1,
                ).await {
                    Ok(k) => {
                        final_media_key = Some(k);
                        break;
                    }
                    Err(e) => {
                        stage_err = Some(("commit-failed", e.to_string()));
                    }
                }
            }

            if let Some((skip_code, err_str)) = stage_err {
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted)
                    || err_str.starts_with("Authentication error:")
                {
                    let trip_kind = if kind == ErrorKind::AccountRestricted {
                        ErrorKind::AccountRestricted
                    } else {
                        ErrorKind::SignInRequired
                    };
                    breaker.trip(trip_kind, account_email, &err_str);
                }

                let is_rate_limited = kind == ErrorKind::RateLimited;
                if (is_retryable(&err_str) || is_rate_limited)
                    && attempt < MAX_TRANSIENT_RETRIES
                    && !cancelled.load(Ordering::Relaxed)
                    && !breaker.is_blocked()
                {
                    let backoff = if is_rate_limited {
                        let d = std::time::Duration::from_secs(30);
                        breaker.pause(d, account_email, &err_str);
                        d
                    } else {
                        std::time::Duration::from_secs(attempt as u64)
                    };
                    let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                        worker_id,
                        status: "retrying".to_string(),
                        file_path: pair.photo_path.to_string_lossy().to_string(),
                        file_name: display_name.clone(),
                        message: if is_rate_limited {
                            format!("Rate limited, waiting to retry ({}/{})...", attempt + 1, MAX_TRANSIENT_RETRIES)
                        } else {
                            format!("Transient error, retrying ({}/{})...", attempt + 1, MAX_TRANSIENT_RETRIES)
                        },
                        bytes_uploaded: 0,
                        bytes_total: total_size,
                        attempt: attempt + 1,
                    })).await;
                    Self::sleep_cancellable(backoff, &cancelled).await;
                    continue;
                }

                if !cancelled.load(Ordering::Relaxed) && !is_retryable(&err_str) && !is_rate_limited {
                    if let Some(cache) = hash_cache {
                        let _ = cache.remove_upload_session(account_email, &photo_b64, p_size);
                        let _ = cache.remove_upload_session(account_email, &video_b64, v_size);
                    }
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: skip_code.to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        }

        if let Some(cache) = hash_cache {
            let _ = cache.remove_upload_session_async(account_email, &photo_b64, p_size).await;
            let _ = cache.remove_upload_session_async(account_email, &video_b64, v_size).await;
        }

        let media_key = final_media_key.unwrap_or_default();

        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
            worker_id,
            status: "completed".to_string(),
            file_path: pair.photo_path.to_string_lossy().to_string(),
            file_name: display_name.clone(),
            message: "Backup completed".to_string(),
            bytes_uploaded: total_size,
            bytes_total: total_size,
            attempt: 1,
        })).await;

        if opts.use_local_hash_cache {
            if let Some(cache) = hash_cache {
                let _ = cache.put_async(account_email, &photo_b64, &media_key, p_size, timestamp).await;
                let _ = cache.put_async(account_email, &video_b64, &media_key, v_size, timestamp).await;
            }
        }

        FileUploadResult {
            media_key,
            is_error: false,
            is_live_photo: true,
            skipped: false,
            skip_code: String::new(),
            skip_reason: String::new(),
            error_message: String::new(),
            path: pair.photo_path.clone(),
            paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
            size_bytes: total_size,
            duration_ms: 0,
            account_email: account_email.to_string(),
        }
    }
}

fn remove_regular_file_if_not_symlink(path: &Path) {
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.is_file() && !meta.file_type().is_symlink() {
            let _ = fs::remove_file(path);
        }
    }
}

impl UploadEngine {
    async fn sleep_cancellable(duration: std::time::Duration, cancelled: &AtomicBool) {
        let deadline = std::time::Instant::now() + duration;
        while std::time::Instant::now() < deadline {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            let rem = deadline.saturating_duration_since(std::time::Instant::now());
            tokio::time::sleep(rem.min(std::time::Duration::from_millis(250))).await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn obtain_and_upload_resumable<F>(
        api: &Api,
        hash_cache: Option<&HashCache>,
        account_email: &str,
        file_path: &Path,
        sha1_b64: &str,
        file_size: i64,
        reset_cached_session: bool,
        cancelled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
        on_progress: F,
    ) -> Result<ScottyFinalizeToken, (&'static str, String)>
    where
        F: FnMut(i64, i64, usize) + Send + 'static,
    {
        let mut upload_token: Option<String> = None;
        let mut known_offset: Option<i64> = None;
        if let Some(cache) = hash_cache {
            if reset_cached_session {
                let _ = cache.remove_upload_session_async(account_email, sha1_b64, file_size).await;
            } else if let Ok(Some(cached_tok)) = cache.get_upload_session_async(account_email, sha1_b64, file_size).await {
                match api.query_upload_status(&cached_tok).await {
                    Ok(Some(recv)) => {
                        upload_token = Some(cached_tok);
                        known_offset = Some(recv);
                    }
                    Ok(None) => {
                        let _ = cache.remove_upload_session_async(account_email, sha1_b64, file_size).await;
                    }
                    Err(ApiError::HttpStatus {
                        status: 400 | 404 | 410,
                        ..
                    }) => {
                        let _ = cache.remove_upload_session_async(account_email, sha1_b64, file_size).await;
                    }
                    Err(e) => {
                        return Err(("upload-token-failed", e.to_string()));
                    }
                }
            }
        }

        let upload_token = match upload_token {
            Some(tok) => tok,
            None => match api.get_upload_token(sha1_b64, file_size).await {
                Ok(tok) => {
                    if let Some(cache) = hash_cache {
                        let _ = cache.save_upload_session_async(account_email, sha1_b64, file_size, &tok).await;
                    }
                    known_offset = Some(0);
                    tok
                }
                Err(e) => return Err(("upload-token-failed", e.to_string())),
            },
        };

        api.upload_file_resumable_from_offset(
            &upload_token,
            file_path,
            known_offset,
            Some(cancelled),
            Some(paused),
            on_progress,
        )
        .await
        .map_err(|e| ("upload-stream-failed", e.to_string()))
    }
}
