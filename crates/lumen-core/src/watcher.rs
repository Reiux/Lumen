use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::scanner::{
    directory_has_nomedia, is_ignored_temp_file, is_path_excluded_with_regex,
    is_supported_by_google_photos, is_trash_or_hidden_directory, MAX_EXCLUDE_REGEX_SIZE_BYTES,
};

const MAX_ZERO_BYTE_CHECKS: usize = 2;
const MAX_UNREADABLE_CHECKS: usize = 5;

#[derive(Debug, Clone)]
struct FileCandidate {
    size: i64,
    mtime: u64,
    checks: usize,
    zero_byte_checks: usize,
    unreadable_checks: usize,
}

#[derive(Debug, Clone)]
struct WatcherFilterOptions {
    filter_unsupported: bool,
    exclude_pattern: String,
    exclude_regex: Option<regex::Regex>,
}

impl Default for WatcherFilterOptions {
    fn default() -> Self {
        Self {
            filter_unsupported: true,
            exclude_pattern: String::new(),
            exclude_regex: None,
        }
    }
}

fn normalize_for_ancestors(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| {
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            if let Ok(canon_parent) = fs::canonicalize(parent) {
                return canon_parent.join(name);
            }
        }
        path.to_path_buf()
    })
}

fn has_excluded_ancestor(path: &Path, watched_roots: &[PathBuf]) -> bool {
    let norm_path = normalize_for_ancestors(path);
    let norm_roots: Vec<PathBuf> = watched_roots
        .iter()
        .map(|root| normalize_for_ancestors(root))
        .collect();
    let matched_root = norm_roots
        .iter()
        .filter(|root| norm_path.starts_with(root))
        .max_by_key(|root| root.components().count());

    let Some(mut dir) = norm_path.parent() else {
        return false;
    };

    loop {
        if directory_has_nomedia(dir) {
            return true;
        }
        if let Some(root) = matched_root {
            if dir == root.as_path() {
                break;
            }
            if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
                if is_trash_or_hidden_directory(name) {
                    return true;
                }
            }
        } else {
            break;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }

    false
}

#[cfg(test)]
fn has_nomedia_in_ancestors(path: &Path, watched_roots: &[PathBuf]) -> bool {
    has_excluded_ancestor(path, watched_roots)
}

fn should_watch_path_with_regex(
    path: &Path,
    watched_roots: &[PathBuf],
    filter_unsupported: bool,
    exclude_pattern: &str,
    exclude_regex: Option<&regex::Regex>,
) -> bool {
    if is_ignored_temp_file(path) {
        return false;
    }
    if filter_unsupported && !is_supported_by_google_photos(path) {
        return false;
    }
    if !exclude_pattern.trim().is_empty()
        && is_path_excluded_with_regex(path, exclude_pattern, exclude_regex)
    {
        return false;
    }
    if has_excluded_ancestor(path, watched_roots) {
        return false;
    }
    true
}

#[cfg(test)]
fn should_watch_path(
    path: &Path,
    watched_roots: &[PathBuf],
    filter_unsupported: bool,
    exclude_pattern: &str,
) -> bool {
    let compiled = crate::scanner::compile_exclude_regex(exclude_pattern);
    should_watch_path_with_regex(
        path,
        watched_roots,
        filter_unsupported,
        exclude_pattern,
        compiled.as_ref(),
    )
}

pub struct FolderWatcher {
    watcher: Option<RecommendedWatcher>,
    folder_configs: Arc<Mutex<HashMap<PathBuf, bool>>>, // dir -> include_subfolders
    pending: Arc<Mutex<HashMap<PathBuf, FileCandidate>>>,
    filter_options: Arc<Mutex<WatcherFilterOptions>>,
    last_watch_errors: Arc<Mutex<Vec<String>>>,
    debounce: Duration,
    stop_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

impl Drop for FolderWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

impl FolderWatcher {
    pub fn new(debounce: Duration) -> Self {
        Self {
            watcher: None,
            folder_configs: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            filter_options: Arc::new(Mutex::new(WatcherFilterOptions::default())),
            last_watch_errors: Arc::new(Mutex::new(Vec::new())),
            debounce,
            stop_tx: None,
        }
    }

    pub fn last_watch_errors(&self) -> Vec<String> {
        self.last_watch_errors.lock().unwrap().clone()
    }

    pub fn set_filter_options(&self, filter_unsupported: bool, exclude_pattern: String) {
        let trimmed = exclude_pattern.trim();
        let exclude_regex = if trimmed.is_empty() || trimmed.len() > 1024 {
            None
        } else {
            regex::RegexBuilder::new(trimmed)
                .size_limit(MAX_EXCLUDE_REGEX_SIZE_BYTES)
                .build()
                .ok()
        };
        let mut opts = self.filter_options.lock().unwrap();
        opts.filter_unsupported = filter_unsupported;
        opts.exclude_pattern = exclude_pattern;
        opts.exclude_regex = exclude_regex;
    }

    pub fn start<F>(&mut self, on_new_files: F) -> Result<(), String>
    where
        F: Fn(Vec<PathBuf>) + Send + Sync + 'static,
    {
        self.stop();

        let (tx, mut rx) = mpsc::channel::<PathBuf>(1000);
        let folder_configs = self.folder_configs.clone();
        let cb_roots = self.folder_configs.clone();
        let cb_filters = self.filter_options.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    let roots: Vec<PathBuf> = cb_roots.lock().unwrap().keys().cloned().collect();
                    let filters = cb_filters.lock().unwrap().clone();
                    for path in event.paths {
                        if should_watch_path_with_regex(
                            &path,
                            &roots,
                            filters.filter_unsupported,
                            &filters.exclude_pattern,
                            filters.exclude_regex.as_ref(),
                        ) {
                            let _ = tx.blocking_send(path);
                        }
                    }
                }
            }
        }).map_err(|e| e.to_string())?;

        // Add monitored folders
        let mut watch_errors = Vec::new();
        {
            let configs = folder_configs.lock().unwrap();
            for (path, recursive) in configs.iter() {
                let mode = if *recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                if let Err(e) = watcher.watch(path, mode) {
                    watch_errors.push(format!("Failed to watch '{}': {}", path.display(), e));
                }
            }
        }
        *self.last_watch_errors.lock().unwrap() = watch_errors.clone();

        self.watcher = Some(watcher);
        let (stop_tx, mut stop_rx) = tokio::sync::broadcast::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        let pending = self.pending.clone();
        let loop_roots = self.folder_configs.clone();
        let loop_filters = self.filter_options.clone();
        let debounce = self.debounce;

        // Background loop for debouncing and stability checks
        let debounce_loop = async move {
            let mut ticker = tokio::time::interval(debounce);
            loop {
                tokio::select! {
                    _ = stop_rx.recv() => break,
                    Some(path) = rx.recv() => {
                        let mut p = pending.lock().unwrap();
                        p.entry(path).or_insert(FileCandidate {
                            size: -1,
                            mtime: 0,
                            checks: 0,
                            zero_byte_checks: 0,
                            unreadable_checks: 0,
                        });
                    }
                    _ = ticker.tick() => {
                        let candidates: Vec<(PathBuf, FileCandidate)> = {
                            let p = pending.lock().unwrap();
                            p.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                        };

                        if candidates.is_empty() {
                            continue;
                        }

                        let roots: Vec<PathBuf> = loop_roots.lock().unwrap().keys().cloned().collect();
                        let filters = loop_filters.lock().unwrap().clone();

                        struct InspectedCandidate {
                            path: PathBuf,
                            size: i64,
                            mtime: u64,
                            readable: bool,
                            remove: bool,
                            zero_byte: bool,
                        }

                        let inspected = tokio::task::spawn_blocking(move || {
                            let mut results = Vec::with_capacity(candidates.len());
                            for (path, cand) in candidates {
                                let metadata = match fs::symlink_metadata(&path) {
                                    Ok(m) => m,
                                    Err(_) => {
                                        results.push(InspectedCandidate {
                                            path,
                                            size: cand.size,
                                            mtime: cand.mtime,
                                            readable: false,
                                            remove: true,
                                            zero_byte: false,
                                        });
                                        continue;
                                    }
                                };

                                if !metadata.is_file() || metadata.file_type().is_symlink() {
                                    results.push(InspectedCandidate {
                                        path,
                                        size: cand.size,
                                        mtime: cand.mtime,
                                        readable: false,
                                        remove: true,
                                        zero_byte: false,
                                    });
                                    continue;
                                }

                                if metadata.len() == 0 {
                                    results.push(InspectedCandidate {
                                        path,
                                        size: cand.size,
                                        mtime: cand.mtime,
                                        readable: false,
                                        remove: false,
                                        zero_byte: true,
                                    });
                                    continue;
                                }

                                if !should_watch_path_with_regex(
                                    &path,
                                    &roots,
                                    filters.filter_unsupported,
                                    &filters.exclude_pattern,
                                    filters.exclude_regex.as_ref(),
                                ) {
                                    results.push(InspectedCandidate {
                                        path,
                                        size: cand.size,
                                        mtime: cand.mtime,
                                        readable: false,
                                        remove: true,
                                        zero_byte: false,
                                    });
                                    continue;
                                }

                                if fs::File::open(&path).is_err() {
                                    results.push(InspectedCandidate {
                                        path,
                                        size: cand.size,
                                        mtime: cand.mtime,
                                        readable: false,
                                        remove: false,
                                        zero_byte: false,
                                    });
                                    continue;
                                }

                                let curr_size = metadata.len() as i64;
                                let curr_mtime = metadata
                                    .modified()
                                    .unwrap_or(SystemTime::UNIX_EPOCH)
                                    .duration_since(SystemTime::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_nanos() as u64;

                                results.push(InspectedCandidate {
                                    path,
                                    size: curr_size,
                                    mtime: curr_mtime,
                                    readable: true,
                                    remove: false,
                                    zero_byte: false,
                                });
                            }
                            results
                        })
                        .await
                        .unwrap_or_default();

                        let mut ready = Vec::new();
                        {
                            let mut p = pending.lock().unwrap();
                            for item in inspected {
                                if item.remove {
                                    p.remove(&item.path);
                                    continue;
                                }
                                if item.zero_byte {
                                    let mut drop_zero = false;
                                    if let Some(cand) = p.get_mut(&item.path) {
                                        cand.zero_byte_checks += 1;
                                        if cand.zero_byte_checks >= MAX_ZERO_BYTE_CHECKS {
                                            drop_zero = true;
                                        }
                                    }
                                    if drop_zero {
                                        p.remove(&item.path);
                                    }
                                    continue;
                                }
                                if !item.readable {
                                    let mut drop_unreadable = false;
                                    if let Some(cand) = p.get_mut(&item.path) {
                                        cand.unreadable_checks += 1;
                                        if cand.unreadable_checks >= MAX_UNREADABLE_CHECKS {
                                            drop_unreadable = true;
                                        }
                                    }
                                    if drop_unreadable {
                                        p.remove(&item.path);
                                    }
                                    continue;
                                }

                                let mut is_ready = false;
                                if let Some(cand) = p.get_mut(&item.path) {
                                    cand.zero_byte_checks = 0;
                                    cand.unreadable_checks = 0;
                                    if cand.size == item.size && cand.mtime == item.mtime {
                                        cand.checks += 1;
                                    } else {
                                        cand.size = item.size;
                                        cand.mtime = item.mtime;
                                        cand.checks = 1;
                                    }
                                    if cand.checks >= 2 {
                                        is_ready = true;
                                    }
                                }
                                if is_ready {
                                    p.remove(&item.path);
                                    ready.push(item.path);
                                }
                            }
                        }

                        if !ready.is_empty() {
                            on_new_files(ready);
                        }
                    }
                }
            }
        };

        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(debounce_loop);
        } else {
            std::thread::Builder::new()
                .name("lumen-watcher".to_string())
                .spawn(move || {
                    if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                    {
                        rt.block_on(debounce_loop);
                    }
                })
                .map_err(|e| format!("Failed to spawn watcher background thread: {}", e))?;
        }

        if !watch_errors.is_empty() {
            return Err(watch_errors.join("; "));
        }

        Ok(())
    }

    pub fn set_folders(&mut self, folders: HashMap<PathBuf, bool>) -> Result<(), String> {
        let mut configs = self.folder_configs.lock().unwrap();
        let mut watch_errors = Vec::new();
        if let Some(ref mut watcher) = self.watcher {
            for p in configs.keys() {
                let _ = watcher.unwatch(p);
            }
            for (p, recursive) in &folders {
                let mode = if *recursive {
                    RecursiveMode::Recursive
                } else {
                    RecursiveMode::NonRecursive
                };
                if let Err(e) = watcher.watch(p, mode) {
                    watch_errors.push(format!("Failed to watch '{}': {}", p.display(), e));
                }
            }
        }
        *configs = folders;
        *self.last_watch_errors.lock().unwrap() = watch_errors.clone();
        if !watch_errors.is_empty() {
            return Err(watch_errors.join("; "));
        }
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        self.watcher = None;
        if let Ok(mut pending) = self.pending.lock() {
            pending.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_should_watch_path_filters_and_ancestor_nomedia() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("Photos");
        let sub = root.join("Private").join("2026");
        fs::create_dir_all(&sub).unwrap();
        fs::write(root.join("Private").join(".nomedia"), b"").unwrap();

        let nested = sub.join("img.jpg");
        assert!(has_nomedia_in_ancestors(&nested, std::slice::from_ref(&root)));
        assert!(!should_watch_path(&nested, std::slice::from_ref(&root), true, ""));

        let regular = root.join("holiday.raw");
        assert!(!should_watch_path(&regular, std::slice::from_ref(&root), true, ""));
        assert!(should_watch_path(&regular, std::slice::from_ref(&root), false, ""));
        assert!(!should_watch_path(&regular, std::slice::from_ref(&root), false, r"\.raw$"));
    }
}
