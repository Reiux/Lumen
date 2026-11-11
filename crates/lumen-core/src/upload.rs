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
use crate::api::{Api, ApiOptions};
use crate::auth::TokenManager;
use crate::config::ConfigManager;
use crate::dedup::{calculate_file_sha1, HashCache};
use crate::livephoto::{LivePhotoPair, UploadWorkItem};
use crate::safety::{classify_error, CircuitBreaker, ErrorKind};

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
    AccountBlocked { email: String, reason: String, message: String },
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
        }
    }

    pub fn with_cancelled(mut self, cancelled: Arc<AtomicBool>) -> Self {
        self.cancelled = cancelled;
        self
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
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

                loop {
                    if cancelled.load(Ordering::Relaxed) {
                        break;
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
        // map: account -> map: album_name -> Vec<media_key>
        let mut album_batches: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();

        for res in &finished_results {
            if res.is_error {
                count_failed += 1;
            } else if res.skipped {
                count_skipped += 1;
            } else {
                count_uploaded += 1;
                if !res.media_key.is_empty() {
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
                            .push(res.media_key.clone());
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
            if let Ok(api) = Api::new(api_opts, &cred) {
                let mgr = AlbumManager::new(Arc::new(api), &account, self.album_registry.clone());
                for (name, keys) in albums {
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
                            for res in &mut finished_results {
                                if keys.contains(&res.media_key) {
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

        Ok(finished_results)
    }

    async fn process_work_item(
        worker_id: usize,
        item: &UploadWorkItem,
        api: &Api,
        opts: &UploadOptions,
        hash_cache: Option<&HashCache>,
        event_tx: &mpsc::Sender<UploadEvent>,
        breaker: &CircuitBreaker,
    ) -> FileUploadResult {
        match item {
            UploadWorkItem::Single { path, account_email } => {
                Self::process_single_file(worker_id, path, account_email, api, opts, hash_cache, event_tx, breaker).await
            }
            UploadWorkItem::LivePhoto { pair, account_email } => {
                Self::process_live_photo(worker_id, pair, account_email, api, opts, hash_cache, event_tx, breaker).await
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

        let (sha1_bytes, sha1_b64) = match calculate_file_sha1(path) {
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
                if let Ok(Some(cached_key)) = cache.get(account_email, &sha1_b64, file_size) {
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

                    if opts.delete_from_host {
                        remove_regular_file_if_not_symlink(path);
                    }

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
                            let _ = cache.put(account_email, &sha1_b64, &remote_key, file_size, 0);
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

                    if opts.delete_from_host {
                        remove_regular_file_if_not_symlink(path);
                    }

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

        // Stage 3: Request Scotty Upload Token
        let upload_token = match api.get_upload_token(&sha1_b64, file_size).await {
            Ok(tok) => tok,
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                    breaker.trip(kind, account_email, &err_str);
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: false,
                    skipped: false,
                    skip_code: "upload-token-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: path.to_path_buf(),
                    paths: vec![path.to_path_buf()],
                    size_bytes: file_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        // Stage 4: Uploading
        let tx = event_tx.clone();
        let p_str = path.to_string_lossy().to_string();
        let fn_str = file_name.clone();

        let finalize_token = match api.upload_file_stream(&upload_token, path, move |bytes_up, total, att| {
            let _ = tx.try_send(UploadEvent::ThreadProgress(ThreadStatus {
                worker_id,
                status: "uploading".to_string(),
                file_path: p_str.clone(),
                file_name: fn_str.clone(),
                message: if att > 1 { format!("Retrying... (attempt {})", att) } else { "Uploading...".to_string() },
                bytes_uploaded: bytes_up,
                bytes_total: total,
                attempt: att,
            }));
        }).await {
            Ok(tok) => tok,
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                    breaker.trip(kind, account_email, &err_str);
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: false,
                    skipped: false,
                    skip_code: "upload-stream-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: path.to_path_buf(),
                    paths: vec![path.to_path_buf()],
                    size_bytes: file_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        // Stage 5: Committing
        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
            worker_id,
            status: "finalizing".to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_name: file_name.clone(),
            message: "Committing upload...".to_string(),
            bytes_uploaded: file_size,
            bytes_total: file_size,
            attempt: 1,
        })).await;

        let timestamp = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH)
            .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

        let media_key = match api.commit_upload(&finalize_token, &file_name, &sha1_bytes, timestamp).await {
            Ok(k) => k,
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                    breaker.trip(kind, account_email, &err_str);
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: false,
                    skipped: false,
                    skip_code: "commit-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: path.to_path_buf(),
                    paths: vec![path.to_path_buf()],
                    size_bytes: file_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        if opts.use_local_hash_cache {
            if let Some(cache) = hash_cache {
                let _ = cache.put(account_email, &sha1_b64, &media_key, file_size, timestamp);
            }
        }

        if opts.delete_from_host {
            remove_regular_file_if_not_symlink(path);
        }

        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
            worker_id,
            status: "completed".to_string(),
            file_path: path.to_string_lossy().to_string(),
            file_name: file_name.clone(),
            message: "Upload completed".to_string(),
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

        let (photo_sha1, photo_b64) = match calculate_file_sha1(&pair.photo_path) {
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

        let (video_sha1, video_b64) = match calculate_file_sha1(&pair.video_path) {
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
        if !opts.force_upload {
            let mut photo_remote = None;
            let mut video_remote = None;

            if opts.use_local_hash_cache {
                if let Some(cache) = hash_cache {
                    photo_remote = cache.get(account_email, &photo_b64, p_meta.len() as i64).ok().flatten();
                    video_remote = cache.get(account_email, &video_b64, v_meta.len() as i64).ok().flatten();
                }
            }

            if photo_remote.is_none() {
                photo_remote = api.find_remote_media_by_hash(&photo_sha1).await.ok().flatten();
            }
            if video_remote.is_none() {
                video_remote = api.find_remote_media_by_hash(&video_sha1).await.ok().flatten();
            }

            if photo_remote.is_some() || video_remote.is_some() {
                let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
                    worker_id,
                    status: "skipped".to_string(),
                    file_path: pair.photo_path.to_string_lossy().to_string(),
                    file_name: display_name.clone(),
                    message: "Skipped: a Live Photo component already exists remotely".to_string(),
                    bytes_uploaded: total_size,
                    bytes_total: total_size,
                    attempt: 1,
                })).await;

                return FileUploadResult {
                    media_key: photo_remote.or(video_remote).unwrap_or_default(),
                    is_error: false,
                    is_live_photo: true,
                    skipped: true,
                    skip_code: "remote-dedup".to_string(),
                    skip_reason: "Skipped: a Live Photo component already exists remotely".to_string(),
                    error_message: String::new(),
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        }

        // Upload photo component
        let photo_upload_token = match api.get_upload_token(&photo_b64, p_meta.len() as i64).await {
            Ok(tok) => tok,
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                    breaker.trip(kind, account_email, &err_str);
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "upload-token-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        let tx = event_tx.clone();
        let p_str = pair.photo_path.to_string_lossy().to_string();
        let disp_name = display_name.clone();
        let photo_finalize_tok = match api.upload_file_stream(&photo_upload_token, &pair.photo_path, move |bytes_up, _, att| {
            let _ = tx.try_send(UploadEvent::ThreadProgress(ThreadStatus {
                worker_id,
                status: "uploading".to_string(),
                file_path: p_str.clone(),
                file_name: disp_name.clone(),
                message: if att > 1 { format!("Uploading photo (attempt {})...", att) } else { "Uploading photo...".to_string() },
                bytes_uploaded: bytes_up,
                bytes_total: total_size,
                attempt: att,
            }));
        }).await {
            Ok(tok) => tok,
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                    breaker.trip(kind, account_email, &err_str);
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "upload-stream-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        // Upload video component
        let video_upload_token = match api.get_upload_token(&video_b64, v_meta.len() as i64).await {
            Ok(tok) => tok,
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                    breaker.trip(kind, account_email, &err_str);
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "upload-token-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        let tx = event_tx.clone();
        let v_str = pair.video_path.to_string_lossy().to_string();
        let disp_name = display_name.clone();
        let p_size = p_meta.len() as i64;
        let video_finalize_tok = match api.upload_file_stream(&video_upload_token, &pair.video_path, move |bytes_up, _, att| {
            let _ = tx.try_send(UploadEvent::ThreadProgress(ThreadStatus {
                worker_id,
                status: "uploading".to_string(),
                file_path: v_str.clone(),
                file_name: disp_name.clone(),
                message: if att > 1 { format!("Uploading video (attempt {})...", att) } else { "Uploading video...".to_string() },
                bytes_uploaded: p_size + bytes_up,
                bytes_total: total_size,
                attempt: att,
            }));
        }).await {
            Ok(tok) => tok,
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                    breaker.trip(kind, account_email, &err_str);
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "upload-stream-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
            worker_id,
            status: "finalizing".to_string(),
            file_path: pair.photo_path.to_string_lossy().to_string(),
            file_name: display_name.clone(),
            message: "Committing Live Photo...".to_string(),
            bytes_uploaded: total_size,
            bytes_total: total_size,
            attempt: 1,
        })).await;

        let timestamp = p_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
            .duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

        let media_key = match api.commit_live_photo(
            &photo_finalize_tok,
            &video_finalize_tok,
            &p_name,
            &photo_sha1,
            &video_sha1,
            timestamp,
        ).await {
            Ok(k) => k,
            Err(e) => {
                let err_str = e.to_string();
                let kind = classify_error(&err_str);
                if matches!(kind, ErrorKind::SignInRequired | ErrorKind::AccountRestricted) {
                    breaker.trip(kind, account_email, &err_str);
                }
                return FileUploadResult {
                    media_key: String::new(),
                    is_error: true,
                    is_live_photo: true,
                    skipped: false,
                    skip_code: "commit-failed".to_string(),
                    skip_reason: String::new(),
                    error_message: err_str,
                    path: pair.photo_path.clone(),
                    paths: vec![pair.photo_path.clone(), pair.video_path.clone()],
                    size_bytes: total_size,
                    duration_ms: 0,
                    account_email: account_email.to_string(),
                };
            }
        };

        let _ = event_tx.send(UploadEvent::ThreadProgress(ThreadStatus {
            worker_id,
            status: "completed".to_string(),
            file_path: pair.photo_path.to_string_lossy().to_string(),
            file_name: display_name.clone(),
            message: "Upload completed".to_string(),
            bytes_uploaded: total_size,
            bytes_total: total_size,
            attempt: 1,
        })).await;

        if opts.use_local_hash_cache {
            if let Some(cache) = hash_cache {
                let _ = cache.put(account_email, &photo_b64, &media_key, p_meta.len() as i64, timestamp);
                let _ = cache.put(account_email, &video_b64, &media_key, v_meta.len() as i64, timestamp);
            }
        }

        if opts.delete_from_host {
            remove_regular_file_if_not_symlink(&pair.photo_path);
            remove_regular_file_if_not_symlink(&pair.video_path);
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
