use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const DEFAULT_MAX_SCAN_DEPTH: usize = 10;

pub fn is_supported_by_google_photos(path: &Path) -> bool {
    let ext = match path.extension().and_then(|s| s.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };

    matches!(
        ext.as_str(),
        // Photos
        "avif" | "bmp" | "gif" | "heic" | "heif" | "ico" | "jpg" | "jpeg" | "png" | "tif"
            | "tiff" | "webp" | "cr2" | "cr3" | "nef" | "arw" | "orf" | "raf" | "rw2"
            | "pef" | "sr2" | "dng" |
        // Videos
        "3gp" | "3g2" | "asf" | "avi" | "divx" | "m2t" | "m2ts" | "m4v" | "mkv" | "mmv"
            | "mod" | "mov" | "mp4" | "mpg" | "mpeg" | "mts" | "tod" | "wmv" | "ts" | "webm"
    )
}

pub fn is_trash_or_hidden_directory(dir_name: &str) -> bool {
    let trimmed = dir_name.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return false;
    }
    if trimmed.starts_with('.') {
        return true;
    }
    let upper = trimmed.to_uppercase();
    upper == "$RECYCLE.BIN"
        || upper == "SYSTEM VOLUME INFORMATION"
        || upper == ".TRASH"
        || upper.starts_with(".TRASH-")
        || upper == "THUMBS.DB"
}

pub fn directory_has_nomedia(dir: &Path) -> bool {
    dir.join(".nomedia").exists()
}

pub fn is_ignored_temp_file(path: &Path) -> bool {
    let file_name = match path.file_name().and_then(|s| s.to_str()) {
        Some(n) => n,
        None => return false,
    };
    if file_name.starts_with('.') || file_name.starts_with("~$") {
        return true;
    }
    let ext = match path.extension().and_then(|s| s.to_str()) {
        Some(e) => e.to_lowercase(),
        None => return false,
    };
    matches!(
        ext.as_str(),
        "tmp" | "part" | "crdownload" | "partial" | "download" | "temp"
    )
}

pub fn is_path_excluded(path: &Path, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return false;
    }
    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let full_path = path.to_string_lossy();
    if let Ok(re) = regex::Regex::new(pattern) {
        if re.is_match(&file_name) || re.is_match(&full_path) {
            return true;
        }
    }
    file_name == pattern || full_path.contains(pattern)
}

#[derive(Debug, Clone)]
pub struct ScanProgressEvent {
    pub files_found: usize,
    pub current_directory: String,
    pub is_scanning: bool,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub recursive: bool,
    pub exclude_pattern: String,
    pub max_depth: usize,
    pub filter_unsupported_files: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            recursive: true,
            exclude_pattern: String::new(),
            max_depth: DEFAULT_MAX_SCAN_DEPTH,
            filter_unsupported_files: true,
        }
    }
}

pub fn scan_directories_incremental<F>(
    paths: &[PathBuf],
    opts: &ScanOptions,
    cancelled: Arc<AtomicBool>,
    mut progress: F,
) -> Result<Vec<PathBuf>, String>
where
    F: FnMut(ScanProgressEvent),
{
    let max_depth = if opts.max_depth == 0 {
        DEFAULT_MAX_SCAN_DEPTH
    } else {
        opts.max_depth
    };

    let mut supported_files = Vec::new();
    let mut seen_files = HashSet::new();
    let mut visited_dirs = HashSet::new();
    let mut last_emit = Instant::now();

    for root_path in paths {
        if cancelled.load(Ordering::Relaxed) {
            return Err("scan cancelled".to_string());
        }

        if !root_path.exists() {
            continue;
        }

        if !root_path.is_dir() {
            if is_path_excluded(root_path, &opts.exclude_pattern) {
                continue;
            }
            if !opts.filter_unsupported_files || is_supported_by_google_photos(root_path) {
                if let Ok(canon) = root_path.canonicalize() {
                    if seen_files.insert(canon) {
                        supported_files.push(root_path.clone());
                    }
                }
            }
            continue;
        }

        let mut stack = VecDeque::new();
        stack.push_back((root_path.clone(), 0usize));

        while let Some((dir, depth)) = stack.pop_back() {
            if cancelled.load(Ordering::Relaxed) {
                return Err("scan cancelled".to_string());
            }

            if depth > max_depth {
                continue;
            }

            let canon_dir = dir.canonicalize().unwrap_or_else(|_| dir.clone());
            if !visited_dirs.insert(canon_dir) {
                continue;
            }

            if directory_has_nomedia(&dir) {
                continue;
            }

            if last_emit.elapsed() >= std::time::Duration::from_millis(150) {
                last_emit = Instant::now();
                progress(ScanProgressEvent {
                    files_found: supported_files.len(),
                    current_directory: dir.to_string_lossy().to_string(),
                    is_scanning: true,
                });
            }

            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                if cancelled.load(Ordering::Relaxed) {
                    return Err("scan cancelled".to_string());
                }

                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();

                if path.is_dir() {
                    if !opts.recursive {
                        continue;
                    }
                    if is_path_excluded(&path, &opts.exclude_pattern) {
                        continue;
                    }
                    if is_trash_or_hidden_directory(&file_name) {
                        continue;
                    }
                    stack.push_back((path, depth + 1));
                } else {
                    if is_ignored_temp_file(&path) {
                        continue;
                    }
                    if is_path_excluded(&path, &opts.exclude_pattern) {
                        continue;
                    }
                    if opts.filter_unsupported_files && !is_supported_by_google_photos(&path) {
                        continue;
                    }
                    if let Ok(canon) = path.canonicalize() {
                        if seen_files.insert(canon) {
                            supported_files.push(path);
                        }
                    }
                }
            }
        }
    }

    progress(ScanProgressEvent {
        files_found: supported_files.len(),
        current_directory: String::new(),
        is_scanning: false,
    });

    Ok(supported_files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_is_supported_by_google_photos() {
        assert!(is_supported_by_google_photos(Path::new("image.jpg")));
        assert!(is_supported_by_google_photos(Path::new("image.HEIC")));
        assert!(is_supported_by_google_photos(Path::new("video.mov")));
        assert!(is_supported_by_google_photos(Path::new("video.mp4")));
        assert!(!is_supported_by_google_photos(Path::new("document.pdf")));
        assert!(!is_supported_by_google_photos(Path::new("archive.zip")));
    }

    #[test]
    fn test_scan_directories() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();

        fs::write(dir.path().join("photo1.jpg"), b"data").unwrap();
        fs::write(sub.join("photo2.png"), b"data").unwrap();
        fs::write(dir.path().join("ignored.txt"), b"data").unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let files = scan_directories_incremental(
            &[dir.path().to_path_buf()],
            &ScanOptions::default(),
            cancelled,
            |_| {},
        ).unwrap();

        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_exclude_pattern_regex_and_filename() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();

        fs::write(dir.path().join("photo1.jpg"), b"data").unwrap();
        fs::write(dir.path().join("private_secret.jpg"), b"data").unwrap();
        fs::write(sub.join("photo2.png"), b"data").unwrap();

        let cancelled = Arc::new(AtomicBool::new(false));
        let opts = ScanOptions {
            exclude_pattern: r".*secret.*".to_string(),
            ..Default::default()
        };
        let files = scan_directories_incremental(
            &[dir.path().to_path_buf()],
            &opts,
            cancelled,
            |_| {},
        ).unwrap();

        assert_eq!(files.len(), 2);
        assert!(!files.iter().any(|p| p.to_string_lossy().contains("secret")));
    }
}
