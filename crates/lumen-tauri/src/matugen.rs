use std::collections::HashMap;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatugenStatus {
    pub supported: bool,
    pub available: bool,
    pub active_path: Option<String>,
    pub colors: Option<HashMap<String, String>>,
}

#[cfg(target_os = "linux")]
pub mod linux {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use tauri::{AppHandle, Emitter};

    static WATCHER_RUNNING: AtomicBool = AtomicBool::new(false);

    pub fn get_candidate_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Ok(env_path) = std::env::var("MATUGEN_COLORS") {
            paths.push(PathBuf::from(env_path));
        }
        if let Ok(env_path) = std::env::var("MATUGEN_PALETTE") {
            paths.push(PathBuf::from(env_path));
        }
        if let Ok(home) = std::env::var("HOME") {
            let h = Path::new(&home);
            paths.push(h.join(".local/state/quickshell/user/generated/colors.json"));
            paths.push(h.join(".local/state/quickshell/user/generated/palette.json"));
            paths.push(h.join(".config/matugen/colors.json"));
            paths.push(h.join(".cache/matugen/colors.json"));
            paths.push(h.join(".local/share/matugen/colors.json"));
            paths.push(h.join(".cache/wal/colors.json"));
            paths.push(h.join(".local/state/quickshell/user/generated/color.txt"));
        }
        paths
    }

    pub fn find_active_color_source() -> Option<(PathBuf, HashMap<String, String>)> {
        for p in get_candidate_paths() {
            if p.is_file() {
                if let Some(colors) = parse_color_file(&p) {
                    if !colors.is_empty() {
                        return Some((p, colors));
                    }
                }
            }
        }
        // Fallback: If matugen CLI exists and wallpaper can be found
        if let Some(wallpaper) = find_wallpaper() {
            if let Some(colors) = generate_colors_from_wallpaper(&wallpaper) {
                return Some((wallpaper, colors));
            }
        }
        None
    }

    pub fn parse_color_file(path: &Path) -> Option<HashMap<String, String>> {
        let content = fs::read_to_string(path).ok()?;
        if path.extension().and_then(|s| s.to_str()) == Some("txt") {
            let trimmed = content.trim();
            if trimmed.starts_with('#') && (trimmed.len() == 7 || trimmed.len() == 9) {
                let mut map = HashMap::new();
                map.insert("seed".to_string(), trimmed.to_string());
                map.insert("primary".to_string(), trimmed.to_string());
                return Some(map);
            }
        }

        let val: serde_json::Value = serde_json::from_str(&content).ok()?;
        parse_json_colors(&val)
    }

    pub fn parse_json_colors(val: &serde_json::Value) -> Option<HashMap<String, String>> {
        let mut map = HashMap::new();
        // Case 1: Raw matugen CLI output with { "colors": { "primary": { "default": { "color": "#..." } } } }
        if let Some(colors_obj) = val.get("colors").and_then(|v| v.as_object()) {
            for (key, color_val) in colors_obj {
                if let Some(hex) = extract_hex_from_matugen_node(color_val) {
                    insert_color_aliases(&mut map, key, &hex);
                }
            }
        } else if let Some(obj) = val.as_object() {
            // Case 2: Flat JSON { "primary": "#...", "surface": "#..." }
            for (key, color_val) in obj {
                if let Some(hex) = color_val.as_str() {
                    if hex.starts_with('#') {
                        insert_color_aliases(&mut map, key, hex);
                    }
                }
            }
        }

        if map.is_empty() {
            None
        } else {
            Some(map)
        }
    }

    fn extract_hex_from_matugen_node(val: &serde_json::Value) -> Option<String> {
        if let Some(s) = val.as_str() {
            if s.starts_with('#') {
                return Some(s.to_string());
            }
        }
        // Lumen runs in dark theme by default: check dark first, then default
        if let Some(dark_hex) = val.get("dark").and_then(|v| v.get("color")).and_then(|s| s.as_str()) {
            return Some(dark_hex.to_string());
        }
        if let Some(def_hex) = val.get("default").and_then(|v| v.get("color")).and_then(|s| s.as_str()) {
            return Some(def_hex.to_string());
        }
        None
    }

    fn insert_color_aliases(map: &mut HashMap<String, String>, raw_key: &str, hex: &str) {
        let clean_key = raw_key.trim_start_matches("--md-sys-color-").to_lowercase();
        let underscored = clean_key.replace('-', "_");
        let hyphenated = clean_key.replace('_', "-");
        map.insert(underscored, hex.to_string());
        map.insert(hyphenated, hex.to_string());
    }

    pub fn find_wallpaper() -> Option<PathBuf> {
        if let Ok(home) = std::env::var("HOME") {
            let qs_wp = Path::new(&home).join(".local/state/quickshell/user/generated/wallpaper/path.txt");
            if qs_wp.is_file() {
                if let Ok(txt) = fs::read_to_string(&qs_wp) {
                    let p = PathBuf::from(txt.trim());
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
        None
    }

    pub fn generate_colors_from_wallpaper(wp: &Path) -> Option<HashMap<String, String>> {
        let dummy_cfg = tempfile::NamedTempFile::new().ok()?;
        fs::write(dummy_cfg.path(), "[config]\n\n[templates]\n").ok()?;
        let output = Command::new("matugen")
            .args([
                "-c",
                dummy_cfg.path().to_string_lossy().as_ref(),
                "image",
                wp.to_string_lossy().as_ref(),
                "-j",
                "hex",
                "--dry-run",
            ])
            .output()
            .ok()?;

        if output.status.success() {
            let parsed: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
            return parse_json_colors(&parsed);
        }
        None
    }

    pub fn init_watcher(app_handle: AppHandle) {
        if WATCHER_RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }

        tauri::async_runtime::spawn(async move {
            let active = find_active_color_source();
            let watch_dir = match active {
                Some((ref p, _)) => p.parent().map(|d| d.to_path_buf()),
                None => {
                    if let Ok(home) = std::env::var("HOME") {
                        let qs = Path::new(&home).join(".local/state/quickshell/user/generated");
                        if qs.is_dir() {
                            Some(qs)
                        } else {
                            let matugen_dir = Path::new(&home).join(".config/matugen");
                            if matugen_dir.is_dir() {
                                Some(matugen_dir)
                            } else {
                                None
                            }
                        }
                    } else {
                        None
                    }
                }
            };

            let Some(dir) = watch_dir else {
                tracing::info!("No matugen directory found to watch for dynamic theming");
                return;
            };

            tracing::info!("Starting Matugen dynamic theme watcher for: {:?}", dir);

            let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(10);
            let mut watcher = match RecommendedWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                            for path in event.paths {
                                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                                if name.ends_with(".json") || name.ends_with(".txt") {
                                    let _ = tx.try_send(());
                                    break;
                                }
                            }
                        }
                    }
                },
                notify::Config::default(),
            ) {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!("Failed to create Matugen watcher: {}", e);
                    return;
                }
            };

            if let Err(e) = watcher.watch(&dir, RecursiveMode::NonRecursive) {
                tracing::warn!("Failed to watch Matugen directory {:?}: {}", dir, e);
                return;
            }

            // Debounce loop
            while rx.recv().await.is_some() {
                tokio::time::sleep(Duration::from_millis(300)).await;
                while rx.try_recv().is_ok() {}

                if let Some((path, colors)) = find_active_color_source() {
                    tracing::info!("Matugen dynamic colors updated from {:?}", path);
                    let _ = app_handle.emit("matugen:colors-changed", &colors);
                }
            }
        });
    }
}

#[cfg(not(target_os = "linux"))]
pub mod non_linux {
    use super::*;
    use tauri::AppHandle;

    pub fn get_status() -> MatugenStatus {
        MatugenStatus {
            supported: false,
            available: false,
            active_path: None,
            colors: None,
        }
    }

    pub fn get_colors() -> Option<HashMap<String, String>> {
        None
    }

    pub fn init_watcher(_app: AppHandle) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flat_json_colors() {
        let json_str = r##"{
            "primary": "#CCC1E5",
            "surface": "#141216",
            "primary_container": "#332850"
        }"##;
        let val: serde_json::Value = serde_json::from_str(json_str).unwrap();
        #[cfg(target_os = "linux")]
        {
            let colors = linux::parse_json_colors(&val).expect("should parse colors");
            assert_eq!(colors.get("primary").unwrap(), "#CCC1E5");
            assert_eq!(colors.get("primary_container").unwrap(), "#332850");
            assert_eq!(colors.get("primary-container").unwrap(), "#332850");
            assert_eq!(colors.get("surface").unwrap(), "#141216");
        }
    }

    #[test]
    fn test_parse_nested_matugen_cli_json() {
        let json_str = r##"{
            "colors": {
                "primary": {
                    "default": { "color": "#112233" },
                    "dark": { "color": "#445566" }
                },
                "surface": {
                    "dark": { "color": "#001122" }
                }
            }
        }"##;
        let val: serde_json::Value = serde_json::from_str(json_str).unwrap();
        #[cfg(target_os = "linux")]
        {
            let colors = linux::parse_json_colors(&val).expect("should parse colors");
            assert_eq!(colors.get("primary").unwrap(), "#445566");
            assert_eq!(colors.get("surface").unwrap(), "#001122");
        }
    }
}
