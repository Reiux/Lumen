use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

pub const APPLE_MAKER_NOTE_HEADER: &[u8] = b"Apple iOS\0";
pub const MAX_MAKER_NOTE_SCAN_SIZE: usize = 64 * 1024 * 1024;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum LivePhotoError {
    #[error("Content identifier is missing")]
    MissingContentIdentifier,
    #[error("Metadata parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    Io(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePhotoMetadata {
    pub content_identifier: String,
    pub has_still_image_time: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LivePhotoPair {
    pub content_identifier: String,
    pub photo_path: PathBuf,
    pub video_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadWorkItem {
    Single { path: PathBuf, account_email: String },
    LivePhoto { pair: LivePhotoPair, account_email: String },
}

impl UploadWorkItem {
    pub fn primary_path(&self) -> &Path {
        match self {
            Self::Single { path, .. } => path,
            Self::LivePhoto { pair, .. } => &pair.photo_path,
        }
    }

    pub fn all_paths(&self) -> Vec<&Path> {
        match self {
            Self::Single { path, .. } => vec![path.as_path()],
            Self::LivePhoto { pair, .. } => vec![pair.photo_path.as_path(), pair.video_path.as_path()],
        }
    }

    pub fn account_email(&self) -> &str {
        match self {
            Self::Single { account_email, .. } => account_email,
            Self::LivePhoto { account_email, .. } => account_email,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightWarning {
    pub paths: Vec<PathBuf>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct LivePhotoClassificationOptions {
    pub enabled: bool,
    pub skip_incomplete: bool,
    pub ignore_apple_metadata: bool,
    pub default_account: String,
}

impl Default for LivePhotoClassificationOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            skip_incomplete: true,
            ignore_apple_metadata: false,
            default_account: String::new(),
        }
    }
}

pub fn read_photo_content_identifier(path: &Path) -> Result<String, LivePhotoError> {
    let mut file = File::open(path).map_err(|e| LivePhotoError::Io(e.to_string()))?;
    let mut buf = Vec::new();
    file.by_ref()
        .take(MAX_MAKER_NOTE_SCAN_SIZE as u64)
        .read_to_end(&mut buf)
        .map_err(|e| LivePhotoError::Io(e.to_string()))?;

    // Search for Apple MakerNote marker
    let marker = APPLE_MAKER_NOTE_HEADER;
    let mut search_from = 0;
    while let Some(pos) = buf[search_from..].windows(marker.len()).position(|w| w == marker) {
        let abs_pos = search_from + pos;
        if let Ok(id) = parse_apple_maker_note(&buf[abs_pos..]) {
            return Ok(id);
        }
        search_from = abs_pos + marker.len();
    }

    Err(LivePhotoError::MissingContentIdentifier)
}

fn parse_apple_maker_note(data: &[u8]) -> Result<String, LivePhotoError> {
    if data.len() < 16 || &data[..APPLE_MAKER_NOTE_HEADER.len()] != APPLE_MAKER_NOTE_HEADER {
        return Err(LivePhotoError::ParseError("invalid header".to_string()));
    }

    let is_big_endian = match &data[12..14] {
        b"MM" => true,
        b"II" => false,
        _ => return Err(LivePhotoError::ParseError("invalid byte order".to_string())),
    };

    let read_u16 = |slice: &[u8]| -> u16 {
        if is_big_endian {
            u16::from_be_bytes([slice[0], slice[1]])
        } else {
            u16::from_le_bytes([slice[0], slice[1]])
        }
    };

    let read_u32 = |slice: &[u8]| -> u32 {
        if is_big_endian {
            u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]])
        } else {
            u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]])
        }
    };

    // Offset of first IFD is at 14..18 relative to TIFF header (offset 12)
    if data.len() < 20 {
        return Err(LivePhotoError::ParseError("data too short".to_string()));
    }
    let ifd_offset = read_u32(&data[16..20]) as usize;
    let tiff_base = 12usize;
    let mut curr_offset = match tiff_base.checked_add(ifd_offset) {
        Some(o) => o,
        None => return Err(LivePhotoError::ParseError("ifd offset overflow".to_string())),
    };

    if data.len() < curr_offset.saturating_add(2) {
        return Err(LivePhotoError::ParseError("ifd out of bounds".to_string()));
    }
    let num_entries = read_u16(&data[curr_offset..curr_offset + 2]) as usize;
    curr_offset += 2;

    for _ in 0..num_entries {
        if data.len() < curr_offset.saturating_add(12) {
            break;
        }
        let tag = read_u16(&data[curr_offset..curr_offset + 2]);
        let count = read_u32(&data[curr_offset + 4..curr_offset + 8]) as usize;
        let value_or_offset = &data[curr_offset + 8..curr_offset + 12];

        // Tag 0x0011 (17) is Content Identifier
        if tag == 0x0011 {
            let val_bytes = if count <= 4 {
                &value_or_offset[..count]
            } else {
                let offset = match tiff_base.checked_add(read_u32(value_or_offset) as usize) {
                    Some(o) => o,
                    None => return Err(LivePhotoError::ParseError("offset overflow".to_string())),
                };
                if offset <= data.len() && count <= data.len().saturating_sub(offset) {
                    &data[offset..offset + count]
                } else {
                    return Err(LivePhotoError::ParseError("content identifier out of bounds".to_string()));
                }
            };
            let trimmed = std::str::from_utf8(val_bytes)
                .map_err(|e| LivePhotoError::ParseError(e.to_string()))?
                .trim_matches('\0')
                .trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
        curr_offset += 12;
    }

    Err(LivePhotoError::MissingContentIdentifier)
}

pub fn read_video_live_photo_metadata(path: &Path) -> Result<LivePhotoMetadata, LivePhotoError> {
    let mut file = File::open(path).map_err(|e| LivePhotoError::Io(e.to_string()))?;
    let mut buf = Vec::new();
    // Read up to 2MB to find QuickTime metadata atoms
    file.by_ref()
        .take(2 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| LivePhotoError::Io(e.to_string()))?;

    let mut content_id = String::new();
    let mut has_still = false;

    // Scan for QuickTime string keys: com.apple.quicktime.content.identifier and still-image-time
    let id_key = b"com.apple.quicktime.content.identifier";
    let still_key = b"com.apple.quicktime.still-image-time";

    if buf.windows(still_key.len()).any(|w| w == still_key) {
        has_still = true;
    }

    if let Some(pos) = buf.windows(id_key.len()).position(|w| w == id_key) {
        // Look ahead for the value string (UUID format, approx 36 chars)
        let search_start = pos + id_key.len();
        let search_end = std::cmp::min(search_start + 256, buf.len());
        if search_start < search_end {
            let slice = &buf[search_start..search_end];
            // Look for UUID-like format (8-4-4-4-12 hex) or alphanumeric identifier
            for window in slice.windows(36) {
                if let Ok(s) = std::str::from_utf8(window) {
                    if s.chars().all(|c| c.is_ascii_hexdigit() || c == '-') && s.contains('-') {
                        content_id = s.to_string();
                        break;
                    }
                }
            }
        }
    }

    if content_id.is_empty() && !has_still {
        return Err(LivePhotoError::MissingContentIdentifier);
    }

    Ok(LivePhotoMetadata {
        content_identifier: content_id,
        has_still_image_time: has_still,
    })
}

fn candidate_type(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    match ext.as_str() {
        "heic" | "heif" | "jpg" | "jpeg" => Some("photo"),
        "mov" => Some("video"),
        _ => None,
    }
}

fn filename_match_key(path: &Path) -> String {
    let dir = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    dir.join(stem).to_string_lossy().to_string()
}

pub fn classify_upload_work(
    paths: &[PathBuf],
    options: &LivePhotoClassificationOptions,
) -> (Vec<UploadWorkItem>, Vec<PreflightWarning>) {
    if !options.enabled {
        let items = paths
            .iter()
            .map(|p| UploadWorkItem::Single {
                path: p.clone(),
                account_email: options.default_account.clone(),
            })
            .collect();
        return (items, Vec::new());
    }

    #[derive(Default)]
    struct Candidates {
        photos: Vec<(usize, PathBuf)>,
        videos: Vec<(usize, PathBuf)>,
    }

    let mut candidates_map: HashMap<String, Candidates> = HashMap::new();
    let mut warnings = Vec::new();

    for (index, path) in paths.iter().enumerate() {
        match candidate_type(path) {
            Some("photo") => {
                let id = if options.ignore_apple_metadata {
                    filename_match_key(path)
                } else {
                    match read_photo_content_identifier(path) {
                        Ok(id) => id.to_lowercase(),
                        Err(LivePhotoError::MissingContentIdentifier) => continue,
                        Err(e) => {
                            warnings.push(PreflightWarning {
                                paths: vec![path.clone()],
                                code: "metadata-read-failed".to_string(),
                                message: format!("could not read Live Photo metadata: {}", e),
                            });
                            continue;
                        }
                    }
                };
                candidates_map.entry(id).or_default().photos.push((index, path.clone()));
            }
            Some("video") => {
                if options.ignore_apple_metadata {
                    let id = filename_match_key(path);
                    candidates_map.entry(id).or_default().videos.push((index, path.clone()));
                } else {
                    match read_video_live_photo_metadata(path) {
                        Ok(meta) => {
                            if !meta.has_still_image_time {
                                warnings.push(PreflightWarning {
                                    paths: vec![path.clone()],
                                    code: "missing-still-image-time".to_string(),
                                    message: "video has a Live Photo identifier but no still-image-time marker".to_string(),
                                });
                                continue;
                            }
                            let id = meta.content_identifier.to_lowercase();
                            if !id.is_empty() {
                                candidates_map.entry(id).or_default().videos.push((index, path.clone()));
                            }
                        }
                        Err(LivePhotoError::MissingContentIdentifier) => continue,
                        Err(e) => {
                            warnings.push(PreflightWarning {
                                paths: vec![path.clone()],
                                code: "metadata-read-failed".to_string(),
                                message: format!("could not read video Live Photo metadata: {}", e),
                            });
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let mut pairs_by_index: HashMap<usize, LivePhotoPair> = HashMap::new();
    let mut consumed: HashMap<usize, bool> = HashMap::new();

    for (identifier, candidates) in &candidates_map {
        if candidates.photos.len() == 1 && candidates.videos.len() == 1 {
            let (p_idx, p_path) = &candidates.photos[0];
            let (v_idx, v_path) = &candidates.videos[0];
            let pair_index = std::cmp::min(*p_idx, *v_idx);

            pairs_by_index.insert(
                pair_index,
                LivePhotoPair {
                    content_identifier: if options.ignore_apple_metadata { String::new() } else { identifier.clone() },
                    photo_path: p_path.clone(),
                    video_path: v_path.clone(),
                },
            );
            consumed.insert(*p_idx, true);
            consumed.insert(*v_idx, true);
            continue;
        }

        if candidates.photos.len() > 1 || candidates.videos.len() > 1 {
            let mut paths = Vec::new();
            for (_, p) in &candidates.photos {
                paths.push(p.clone());
            }
            for (_, v) in &candidates.videos {
                paths.push(v.clone());
            }
            warnings.push(PreflightWarning {
                paths,
                code: "ambiguous-identifier".to_string(),
                message: "multiple photo or video candidates share one Live Photo identifier".to_string(),
            });
            continue;
        }

        if options.skip_incomplete {
            let mut orphan_paths = Vec::new();
            for (idx, p) in &candidates.photos {
                consumed.insert(*idx, true);
                orphan_paths.push(p.clone());
            }
            for (idx, v) in &candidates.videos {
                consumed.insert(*idx, true);
                orphan_paths.push(v.clone());
            }
            warnings.push(PreflightWarning {
                paths: orphan_paths,
                code: "incomplete-live-photo-skipped".to_string(),
                message: "Skipped an incomplete Live Photo because its matching file is not in this queue.".to_string(),
            });
        }
    }

    let mut work = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if let Some(pair) = pairs_by_index.remove(&index) {
            work.push(UploadWorkItem::LivePhoto {
                pair,
                account_email: options.default_account.clone(),
            });
            continue;
        }
        if consumed.get(&index).copied().unwrap_or(false) {
            continue;
        }
        work.push(UploadWorkItem::Single {
            path: path.clone(),
            account_email: options.default_account.clone(),
        });
    }

    (work, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_by_filename_stem() {
        let paths = vec![
            PathBuf::from("/photos/IMG_0001.HEIC"),
            PathBuf::from("/photos/IMG_0001.MOV"),
            PathBuf::from("/photos/IMG_0002.JPG"),
        ];
        let options = LivePhotoClassificationOptions {
            enabled: true,
            skip_incomplete: false,
            ignore_apple_metadata: true,
            default_account: "test@example.com".to_string(),
        };

        let (work, _warnings) = classify_upload_work(&paths, &options);
        assert_eq!(work.len(), 2);
        match &work[0] {
            UploadWorkItem::LivePhoto { pair, account_email } => {
                assert_eq!(pair.photo_path, paths[0]);
                assert_eq!(pair.video_path, paths[1]);
                assert_eq!(account_email, "test@example.com");
            }
            _ => panic!("expected LivePhoto pair"),
        }
        match &work[1] {
            UploadWorkItem::Single { path, account_email } => {
                assert_eq!(path, &paths[2]);
                assert_eq!(account_email, "test@example.com");
            }
            _ => panic!("expected Single media"),
        }
    }

    #[test]
    fn test_classify_by_filename_stem_skip_incomplete() {
        let paths = vec![
            PathBuf::from("/photos/IMG_0001.HEIC"),
            PathBuf::from("/photos/IMG_0001.MOV"),
            PathBuf::from("/photos/IMG_0002.JPG"),
        ];
        let options = LivePhotoClassificationOptions {
            enabled: true,
            skip_incomplete: true,
            ignore_apple_metadata: true,
            default_account: "test@example.com".to_string(),
        };

        let (work, warnings) = classify_upload_work(&paths, &options);
        assert_eq!(work.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "incomplete-live-photo-skipped");
    }

    #[test]
    fn test_parse_apple_maker_note_malformed_no_panic() {
        // Buffer shorter than header
        assert!(parse_apple_maker_note(b"Apple iOS").is_err());

        // Header with invalid endianness
        let mut data = vec![0u8; 32];
        data[..10].copy_from_slice(b"Apple iOS\0");
        data[12..14].copy_from_slice(b"XX");
        assert!(parse_apple_maker_note(&data).is_err());

        // Out of bounds IFD offset
        data[12..14].copy_from_slice(b"MM");
        data[16..20].copy_from_slice(&u32::to_be_bytes(0xffff_ff00));
        assert!(parse_apple_maker_note(&data).is_err());
    }
}
