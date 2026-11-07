use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::scanner::{is_ignored_temp_file, is_supported_by_google_photos};

#[derive(Debug, Clone)]
struct FileCandidate {
    size: i64,
    mtime: u64,
    checks: usize,
}

pub struct FolderWatcher {
    watcher: Option<RecommendedWatcher>,
    folder_configs: Arc<Mutex<HashMap<PathBuf, bool>>>, // dir -> include_subfolders
    pending: Arc<Mutex<HashMap<PathBuf, FileCandidate>>>,
    debounce: Duration,
    stop_tx: Option<tokio::sync::broadcast::Sender<()>>,
}

impl FolderWatcher {
    pub fn new(debounce: Duration) -> Self {
        Self {
            watcher: None,
            folder_configs: Arc::new(Mutex::new(HashMap::new())),
            pending: Arc::new(Mutex::new(HashMap::new())),
            debounce,
            stop_tx: None,
        }
    }

    pub fn start<F>(&mut self, on_new_files: F) -> Result<(), String>
    where
        F: Fn(Vec<PathBuf>) + Send + Sync + 'static,
    {
        let (tx, mut rx) = mpsc::channel::<PathBuf>(1000);
        let folder_configs = self.folder_configs.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    for path in event.paths {
                        if !is_ignored_temp_file(&path) && is_supported_by_google_photos(&path) {
                            let _ = tx.blocking_send(path);
                        }
                    }
                }
            }
        }).map_err(|e| e.to_string())?;

        // Add monitored folders
        {
            let configs = folder_configs.lock().unwrap();
            for (path, recursive) in configs.iter() {
                if path.exists() {
                    let mode = if *recursive {
                        RecursiveMode::Recursive
                    } else {
                        RecursiveMode::NonRecursive
                    };
                    let _ = watcher.watch(path, mode);
                }
            }
        }

        self.watcher = Some(watcher);
        let (stop_tx, mut stop_rx) = tokio::sync::broadcast::channel::<()>(1);
        self.stop_tx = Some(stop_tx);

        let pending = self.pending.clone();
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
                        });
                    }
                    _ = ticker.tick() => {
                        let candidates: Vec<PathBuf> = {
                            let p = pending.lock().unwrap();
                            p.keys().cloned().collect()
                        };

                        if candidates.is_empty() {
                            continue;
                        }

                        let mut ready = Vec::new();
                        let mut to_remove = Vec::new();

                        for path in candidates {
                            let metadata = match fs::metadata(&path) {
                                Ok(m) => m,
                                Err(_) => {
                                    to_remove.push(path);
                                    continue;
                                }
                            };

                            if metadata.is_dir() || metadata.len() == 0 {
                                continue;
                            }

                            // Check readability
                            if fs::File::open(&path).is_err() {
                                continue;
                            }

                            let curr_size = metadata.len() as i64;
                            let curr_mtime = metadata
                                .modified()
                                .unwrap_or(SystemTime::UNIX_EPOCH)
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos() as u64;

                            let mut p = pending.lock().unwrap();
                            if let Some(cand) = p.get_mut(&path) {
                                if cand.size == curr_size && cand.mtime == curr_mtime {
                                    cand.checks += 1;
                                } else {
                                    cand.size = curr_size;
                                    cand.mtime = curr_mtime;
                                    cand.checks = 1;
                                }

                                if cand.checks >= 2 {
                                    ready.push(path.clone());
                                    to_remove.push(path);
                                }
                            }
                        }

                        let mut p = pending.lock().unwrap();
                        for r in to_remove {
                            p.remove(&r);
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

        Ok(())
    }

    pub fn set_folders(&mut self, folders: HashMap<PathBuf, bool>) -> Result<(), String> {
        let mut configs = self.folder_configs.lock().unwrap();
        if let Some(ref mut watcher) = self.watcher {
            for p in configs.keys() {
                let _ = watcher.unwatch(p);
            }
            for (p, recursive) in &folders {
                if p.exists() {
                    let mode = if *recursive {
                        RecursiveMode::Recursive
                    } else {
                        RecursiveMode::NonRecursive
                    };
                    let _ = watcher.watch(p, mode);
                }
            }
        }
        *configs = folders;
        Ok(())
    }

    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        self.watcher = None;
    }
}
