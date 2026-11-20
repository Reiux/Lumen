#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use flate2::read::GzDecoder;
use serde::Serialize;
use tar::Archive;
use tauri::{AppHandle, Window};
use tauri_plugin_dialog::DialogExt;

const EMBEDDED_PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.tar.gz"));
#[cfg(windows)]
const WINDOWS_NOTIFICATION_ICON_PNG: &[u8] = include_bytes!("../../lumen-tauri/icons/128x128.png");

const IS_DEV_CHANNEL: bool = cfg!(debug_assertions);
const APP_DISPLAY_NAME: &str = if IS_DEV_CHANNEL { "Lumen Dev" } else { "Lumen" };
#[cfg(windows)]
const APP_AUMID: &str = if IS_DEV_CHANNEL { "com.reiux.lumen.dev" } else { "com.reiux.lumen" };
#[cfg(windows)]
const UNINSTALL_REG_KEY_NAME: &str = if IS_DEV_CHANNEL { "LumenDev" } else { "Lumen" };
#[cfg(windows)]
const SHORTCUT_FILENAME: &str = if IS_DEV_CHANNEL { "Lumen Dev.lnk" } else { "Lumen.lnk" };
#[cfg(windows)]
const STARTUP_REG_VALUE_NAME: &str = if IS_DEV_CHANNEL { "Lumen Dev" } else { "Lumen" };
#[cfg(windows)]
const INSTALL_DIR_NAME_WIN: &str = if IS_DEV_CHANNEL { "Lumen Dev" } else { "Lumen" };
#[cfg(unix)]
const INSTALL_DIR_NAME_UNIX: &str = if IS_DEV_CHANNEL { "Lumen-dev" } else { "Lumen" };
const CONFIG_DIR_NAME: &str = if IS_DEV_CHANNEL { "lumen-dev" } else { "lumen" };
#[cfg(windows)]
const WINDOWS_MAIN_EXE_NAME: &str = if IS_DEV_CHANNEL { "Lumen-dev.exe" } else { "Lumen.exe" };

fn channel_install_dir_name() -> &'static str {
    #[cfg(windows)]
    {
        INSTALL_DIR_NAME_WIN
    }
    #[cfg(not(windows))]
    {
        INSTALL_DIR_NAME_UNIX
    }
}

fn is_dedicated_lumen_folder_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(channel_install_dir_name())
        || name.eq_ignore_ascii_case("lumen")
        || name.eq_ignore_ascii_case("lumen dev")
        || name.eq_ignore_ascii_case("lumen-dev")
        || name.eq_ignore_ascii_case("gotohp")
}

fn ensure_dedicated_install_subdir(selected: &Path) -> PathBuf {
    let raw = selected.to_string_lossy();
    let trimmed = raw.trim().trim_end_matches(['/', '\\']);
    let last_component = trimmed.rsplit(['/', '\\']).next().unwrap_or("");
    if !last_component.is_empty() && is_dedicated_lumen_folder_name(last_component) {
        selected.to_path_buf()
    } else {
        selected.join(channel_install_dir_name())
    }
}

#[derive(Serialize)]
pub struct SetupInfo {
    pub is_uninstall: bool,
    pub is_installed: bool,
    pub is_update: bool,
    pub is_dev: bool,
    pub app_name: String,
    pub installed_version: String,
    pub current_version: String,
    pub default_path: String,
    pub config_path: String,
}

#[derive(Debug, Clone, Default)]
struct InstalledAppInfo {
    dir: PathBuf,
    version: String,
    #[allow(dead_code)]
    is_nsis: bool,
}

fn app_binary_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        if IS_DEV_CHANNEL {
            &["Lumen-dev.exe", "lumen-dev.exe"]
        } else {
            &["Lumen.exe", "lumen.exe"]
        }
    }
    #[cfg(not(windows))]
    {
        if IS_DEV_CHANNEL {
            &["Lumen-dev", "lumen-dev", "Lumen", "lumen"]
        } else {
            &["Lumen", "lumen"]
        }
    }
}

fn find_app_binary(dir: &Path) -> Option<PathBuf> {
    app_binary_names()
        .iter()
        .map(|name| dir.join(name))
        .find(|p| p.is_file())
}

fn uninstaller_filename() -> &'static str {
    #[cfg(windows)]
    {
        "uninstall.exe"
    }
    #[cfg(not(windows))]
    {
        "uninstall"
    }
}

fn parse_semver(s: &str) -> (Vec<u32>, Option<String>) {
    let trimmed = s.trim().trim_start_matches(['v', 'V']);
    let without_build = trimmed.split('+').next().unwrap_or("");
    let (core_str, pre) = match without_build.split_once('-') {
        Some((c, p)) => (c, (!p.trim().is_empty()).then(|| p.trim().to_string())),
        None => (without_build, None),
    };
    let mut core: Vec<u32> = core_str
        .split('.')
        .map(|part| part.parse::<u32>().unwrap_or(0))
        .collect();
    while core.len() < 3 {
        core.push(0);
    }
    (core, pre)
}

fn is_older_version(installed: &str, current: &str) -> bool {
    let (inst_core, inst_pre) = parse_semver(installed);
    let (cur_core, cur_pre) = parse_semver(current);
    if inst_core != cur_core {
        return inst_core < cur_core;
    }
    match (inst_pre, cur_pre) {
        (Some(_), None) => true,
        (None, Some(_)) => false,
        (Some(ip), Some(cp)) => ip < cp,
        (None, None) => false,
    }
}

#[cfg(any(windows, test))]
fn extract_exe_path_from_uninstall_string(uninst_str: &str) -> PathBuf {
    let clean = uninst_str.trim().trim_matches('"');
    let exe_part = if let Some(idx) = clean.to_ascii_lowercase().find(".exe") {
        &clean[..idx + 4]
    } else {
        clean
    };
    PathBuf::from(exe_part.trim().trim_matches('"'))
}

#[cfg(windows)]
fn extract_dir_from_reg_key(key: &winreg::RegKey) -> Option<PathBuf> {
    if let Ok(loc) = key.get_value::<String, _>("InstallLocation") {
        let clean = loc.trim().trim_matches('"');
        if !clean.is_empty() {
            let p = PathBuf::from(clean);
            if p.exists() {
                return Some(p);
            }
        }
    }
    if let Ok(uninst_str) = key.get_value::<String, _>("UninstallString") {
        let p = extract_exe_path_from_uninstall_string(&uninst_str);
        if let Some(parent) = p.parent() {
            if parent.exists() {
                return Some(parent.to_path_buf());
            }
        }
    }
    if let Ok(icon) = key.get_value::<String, _>("DisplayIcon") {
        let clean = icon.trim().trim_matches('"');
        let icon_part = clean.split(',').next().unwrap_or(clean).trim().trim_matches('"');
        let p = PathBuf::from(icon_part);
        if let Some(parent) = p.parent() {
            if parent.exists() {
                return Some(parent.to_path_buf());
            }
        }
    }
    None
}

#[cfg(windows)]
fn get_hkcu_modern_install() -> Option<(PathBuf, String)> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let uninstall_root = hkcu
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall")
        .ok()?;
    let app_key = uninstall_root.open_subkey(UNINSTALL_REG_KEY_NAME).ok()?;
    let version = app_key
        .get_value::<String, _>("DisplayVersion")
        .unwrap_or_default();
    let dir = extract_dir_from_reg_key(&app_key)?;
    Some((dir, version))
}

#[cfg(windows)]
fn is_modern_v2_dir(candidate: &Path, modern: Option<&(PathBuf, String)>) -> bool {
    if candidate.join("wails.json").is_file() {
        return false;
    }
    if let Some((modern_dir, ver)) = modern {
        if !ver.is_empty() && !is_older_version(ver, "2.0.0") {
            let same_dir = candidate
                .canonicalize()
                .ok()
                .zip(modern_dir.canonicalize().ok())
                .map(|(a, b)| a == b)
                .unwrap_or_else(|| candidate == modern_dir.as_path());
            if same_dir {
                return true;
            }
        }
    }
    false
}

fn get_legacy_nsis_info() -> Option<InstalledAppInfo> {
    // Dev channel is strictly isolated from release installs and must never
    // detect, update, or remove a legacy v1.x NSIS release installation.
    if IS_DEV_CHANNEL {
        return None;
    }

    let current_exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .and_then(|d| d.canonicalize().ok());

    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu_modern = get_hkcu_modern_install();

        // 1. Direct scan of standard Program Files folders FIRST
        // This is 100% reliable even if registry keys were wiped or corrupted
        for pf_var in &["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
            if let Ok(pf) = env::var(pf_var) {
                let candidates = [
                    PathBuf::from(&pf).join("Reiux").join("Lumen"),
                    PathBuf::from(&pf).join("Lumen"),
                    PathBuf::from(&pf).join("gotohp"),
                ];
                for candidate in candidates {
                    if is_modern_v2_dir(&candidate, hkcu_modern.as_ref()) {
                        continue;
                    }
                    if candidate.is_dir() && has_legacy_nsis_marker(&candidate) {
                        if let Some(ref cur) = current_exe_dir {
                            if let Ok(canon) = candidate.canonicalize() {
                                if canon == *cur {
                                    continue;
                                }
                            }
                        }
                        return Some(InstalledAppInfo {
                            dir: candidate,
                            version: "1.0.1".to_string(),
                            is_nsis: true,
                        });
                    }
                }
            }
        }

        // 2. Scan HKLM, WOW6432Node, and HKCU Uninstall registry keys for legacy Go NSIS
        let hives = [
            (HKEY_LOCAL_MACHINE, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
            (HKEY_LOCAL_MACHINE, "Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
            (HKEY_CURRENT_USER, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        ];

        for (hkey, subpath) in hives {
            let root = RegKey::predef(hkey);
            if let Ok(uninstall_root) = root.open_subkey(subpath) {
                for key_name in uninstall_root.enum_keys().filter_map(|r| r.ok()) {
                    let lower_key = key_name.to_lowercase();
                    let is_legacy_key = lower_key == "reiuxlumen"
                        || lower_key == "reiux lumen"
                        || lower_key == "gotohp"
                        || (lower_key == "lumen" && hkey == HKEY_LOCAL_MACHINE);

                    if is_legacy_key {
                        if let Ok(app_key) = uninstall_root.open_subkey(&key_name) {
                            let version: String = app_key.get_value("DisplayVersion").unwrap_or_default();
                            let found_ver = if version.is_empty() { "1.0.1".to_string() } else { version };
                            if let Some(p) = extract_dir_from_reg_key(&app_key) {
                                if p.is_dir() {
                                    if let Some(ref cur) = current_exe_dir {
                                        if let Ok(canon) = p.canonicalize() {
                                            if canon == *cur {
                                                continue;
                                            }
                                        }
                                    }
                                    return Some(InstalledAppInfo {
                                        dir: p,
                                        version: found_ver,
                                        is_nsis: true,
                                    });
                                }
                            }
                            if let Ok(pf) = env::var("ProgramFiles") {
                                let def_p = PathBuf::from(&pf).join("Reiux").join("Lumen");
                                if def_p.is_dir() {
                                    return Some(InstalledAppInfo {
                                        dir: def_p,
                                        version: found_ver,
                                        is_nsis: true,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // 3. Fallback: check All-Users Start Menu and Public Desktop shortcuts
        if let Ok(pd) = env::var("ProgramData") {
            for lnk_name in &["Lumen.lnk", "gotohp.lnk"] {
                let start_lnk = PathBuf::from(&pd)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs")
                    .join(lnk_name);
                if start_lnk.is_file() {
                    let mut found_dir = PathBuf::from("C:\\Program Files\\Reiux\\Lumen");
                    for pf_var in &["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
                        if let Ok(pf) = env::var(pf_var) {
                            for sub in &["Reiux\\Lumen", "Lumen", "gotohp"] {
                                let c = PathBuf::from(&pf).join(sub);
                                if c.is_dir() {
                                    found_dir = c;
                                    break;
                                }
                            }
                        }
                    }
                    return Some(InstalledAppInfo {
                        dir: found_dir,
                        version: "1.0.1".to_string(),
                        is_nsis: true,
                    });
                }
            }
        }
        if let Ok(public_dir) = env::var("PUBLIC") {
            for lnk_name in &["Lumen.lnk", "gotohp.lnk"] {
                let desktop_lnk = PathBuf::from(&public_dir).join("Desktop").join(lnk_name);
                if desktop_lnk.is_file() {
                    let mut found_dir = PathBuf::from("C:\\Program Files\\Reiux\\Lumen");
                    for pf_var in &["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
                        if let Ok(pf) = env::var(pf_var) {
                            for sub in &["Reiux\\Lumen", "Lumen", "gotohp"] {
                                let c = PathBuf::from(&pf).join(sub);
                                if c.is_dir() {
                                    found_dir = c;
                                    break;
                                }
                            }
                        }
                    }
                    return Some(InstalledAppInfo {
                        dir: found_dir,
                        version: "1.0.1".to_string(),
                        is_nsis: true,
                    });
                }
            }
        }
    }
    let _ = current_exe_dir;
    None
}

fn get_modern_installed_info() -> Option<InstalledAppInfo> {
    get_modern_installed_info_with_opts(is_uninstall_invocation())
}

fn has_any_modern_install_marker(dir: &Path) -> bool {
    if !is_safe_dedicated_install_dir(dir) {
        return false;
    }
    #[cfg(windows)]
    {
        find_app_binary(dir).is_some()
            || dir.join("lumen-cli.exe").is_file()
            || dir.join("uninstall.exe").is_file()
    }
    #[cfg(not(windows))]
    {
        find_app_binary(dir).is_some()
            || dir.join("lumen-cli").is_file()
            || dir.join("uninstall").is_file()
    }
}

fn get_modern_installed_info_with_opts(include_current_dir: bool) -> Option<InstalledAppInfo> {
    let current_exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .and_then(|d| d.canonicalize().ok());

    #[cfg(windows)]
    {
        if let Some((p, version)) = get_hkcu_modern_install() {
            let has_bin = find_app_binary(&p).is_some();
            let has_partial = include_current_dir && has_any_modern_install_marker(&p);
            if has_bin || has_partial {
                if !include_current_dir {
                    if let Some(ref cur) = current_exe_dir {
                        if let Ok(canon) = p.canonicalize() {
                            if canon == *cur {
                                return None;
                            }
                        }
                    }
                }
                return Some(InstalledAppInfo {
                    dir: p,
                    version,
                    is_nsis: false,
                });
            }
        }
    }

    let default_dir = get_default_install_dir();
    if find_app_binary(&default_dir).is_some() || (include_current_dir && has_any_modern_install_marker(&default_dir)) {
        let mut matches_current = false;
        if let Some(ref cur) = current_exe_dir {
            if let Ok(canon_default) = default_dir.canonicalize() {
                if canon_default == *cur {
                    matches_current = true;
                }
            }
        }
        if include_current_dir || !matches_current {
            return Some(InstalledAppInfo {
                dir: default_dir,
                version: String::new(),
                is_nsis: false,
            });
        }
        return None;
    }

    if include_current_dir {
        if let Some(ref cur) = current_exe_dir {
            if has_any_modern_install_marker(cur) {
                return Some(InstalledAppInfo {
                    dir: cur.clone(),
                    version: String::new(),
                    is_nsis: false,
                });
            }
        }
    }
    None
}

fn get_installed_info() -> Option<InstalledAppInfo> {
    get_legacy_nsis_info().or_else(get_modern_installed_info)
}

fn get_installed_dir() -> Option<PathBuf> {
    get_installed_info().map(|info| info.dir)
}

fn check_if_installed(install_dir: &Path) -> bool {
    let current_exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .and_then(|d| d.canonicalize().ok());

    if install_dir.is_absolute() && find_app_binary(install_dir).is_some() {
        if let Ok(canon_target) = install_dir.canonicalize() {
            if let Some(ref cur) = current_exe_dir {
                if canon_target != *cur {
                    return true;
                }
            } else {
                return true;
            }
        }
    }
    if let Some(installed) = get_installed_dir() {
        if find_app_binary(&installed).is_some() {
            return true;
        }
    }
    false
}

fn get_config_display_path() -> String {
    #[cfg(windows)]
    {
        if let Ok(appdata) = env::var("APPDATA") {
            return format!("{}\\{}", appdata, CONFIG_DIR_NAME);
        }
        return format!("%APPDATA%\\{}", CONFIG_DIR_NAME);
    }
    #[cfg(target_os = "macos")]
    {
        format!("~/Library/Application Support/{}", CONFIG_DIR_NAME)
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        format!("~/.config/{}", CONFIG_DIR_NAME)
    }
}

fn is_uninstall_invocation() -> bool {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--uninstall" || a == "-u") {
        return true;
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(stem) = exe.file_stem() {
            let stem_str = stem.to_string_lossy().to_lowercase();
            if stem_str.contains("uninstall") {
                return true;
            }
        }
    }
    false
}

fn get_default_install_dir() -> PathBuf {
    #[cfg(windows)]
    {
        if let Ok(local_appdata) = env::var("LOCALAPPDATA") {
            return PathBuf::from(local_appdata)
                .join("Programs")
                .join(INSTALL_DIR_NAME_WIN);
        }
        PathBuf::from("C:\\Program Files").join(INSTALL_DIR_NAME_WIN)
    }

    #[cfg(unix)]
    {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(INSTALL_DIR_NAME_UNIX);
        }
        env::temp_dir().join(INSTALL_DIR_NAME_UNIX)
    }
}

fn installer_app_version() -> String {
    if cfg!(debug_assertions) {
        format!("{}-dev", env!("CARGO_PKG_VERSION"))
    } else {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

#[tauri::command]
fn get_setup_info() -> SetupInfo {
    let is_uninstall = is_uninstall_invocation();
    let current_version = installer_app_version();
    let default_dir = get_default_install_dir();
    let legacy_nsis = get_legacy_nsis_info();
    let modern_installed = get_modern_installed_info();
    let default_installed = check_if_installed(&default_dir);

    let is_installed = legacy_nsis.is_some() || modern_installed.is_some() || default_installed;
    let mut is_update = false;
    let mut installed_version = String::new();
    let mut default_path = default_dir.to_string_lossy().to_string();

    if let Some(ref legacy) = legacy_nsis {
        // Legacy NSIS install is always upgraded and migrated to modern per-user Rust install
        is_update = true;
        installed_version = if legacy.version.is_empty() {
            "1.0.1".to_string()
        } else {
            legacy.version.clone()
        };
        default_path = default_dir.to_string_lossy().to_string();
    } else if let Some(ref modern) = modern_installed {
        installed_version = modern.version.clone();
        if modern.version.is_empty() || is_older_version(&modern.version, &current_version) {
            is_update = true;
        }
        default_path = modern.dir.to_string_lossy().to_string();
    }

    let config_path = get_config_display_path();

    SetupInfo {
        is_uninstall,
        is_installed,
        is_update,
        is_dev: IS_DEV_CHANNEL,
        app_name: APP_DISPLAY_NAME.to_string(),
        installed_version,
        current_version,
        default_path,
        config_path,
    }
}

#[tauri::command]
async fn pick_directory(app: AppHandle) -> Result<Option<String>, String> {
    use tokio::sync::oneshot;
    let (tx, rx) = oneshot::channel();
    app.dialog()
        .file()
        .pick_folder(move |path| {
            let res = path.map(|p| {
                let pb = PathBuf::from(p.to_string());
                ensure_dedicated_install_subdir(&pb)
                    .to_string_lossy()
                    .to_string()
            });
            let _ = tx.send(res);
        });
    Ok(rx.await.unwrap_or(None))
}

fn is_explicit_absolute_path_str(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    raw.starts_with('/')
        || raw.starts_with("\\\\")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'/' || bytes[2] == b'\\'))
}

fn strip_windows_verbatim_prefix(raw: &str) -> String {
    if let Some(unc) = raw.strip_prefix(r"\\?\UNC\").or_else(|| raw.strip_prefix("//?/UNC/")) {
        return format!(r"\\{}", unc);
    }
    if let Some(rest) = raw.strip_prefix(r"\\?\").or_else(|| raw.strip_prefix("//?/")) {
        return rest.to_string();
    }
    raw.to_string()
}

#[cfg(any(windows, test))]
fn normalize_trusted_win_root(root: &str) -> String {
    let cleaned = strip_windows_verbatim_prefix(root).replace('/', "\\").to_lowercase();
    let trimmed = cleaned.trim_end_matches('\\');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{}\\", trimmed)
    }
}

#[cfg(any(windows, test))]
fn is_verified_legacy_dir_candidate(
    canon_str_raw: &str,
    trusted_roots: &[String],
    has_legacy_marker: bool,
) -> bool {
    if !has_legacy_marker {
        return false;
    }
    let cleaned = strip_windows_verbatim_prefix(canon_str_raw);
    let cleaned_path = PathBuf::from(&cleaned);
    if !is_safe_dedicated_install_dir(&cleaned_path) {
        return false;
    }
    let normalized = cleaned.replace('/', "\\").to_lowercase();
    trusted_roots.iter().any(|root| {
        let norm_root = normalize_trusted_win_root(root);
        !norm_root.is_empty() && normalized.starts_with(&norm_root)
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    let raw = strip_windows_verbatim_prefix(&path.to_string_lossy());
    let s = raw.replace('\\', "/");
    let bytes = s.as_bytes();
    let has_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    let has_unc = s.starts_with("//");
    let has_root = !has_drive && s.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    let mut drive_prefix: Option<&str> = None;

    for (i, part) in s.split('/').enumerate() {
        if part.is_empty() || part == "." {
            continue;
        }
        if i == 0 && has_drive {
            drive_prefix = Some(part);
            continue;
        }
        if part == ".." {
            parts.pop();
        } else {
            parts.push(part);
        }
    }

    let res = if let Some(drv) = drive_prefix {
        if parts.is_empty() {
            format!("{}/", drv)
        } else {
            format!("{}/{}", drv, parts.join("/"))
        }
    } else if has_unc {
        format!("//{}", parts.join("/"))
    } else if has_root {
        format!("/{}", parts.join("/"))
    } else {
        parts.join("/")
    };
    PathBuf::from(res)
}

fn is_safe_dedicated_install_dir(dir: &Path) -> bool {
    let orig = dir.to_string_lossy();
    let orig_slashes_upper = orig.replace('\\', "/").to_ascii_uppercase();
    if orig_slashes_upper.starts_with("//./") || orig_slashes_upper.starts_with("//?/GLOBALROOT") {
        return false;
    }

    let raw = strip_windows_verbatim_prefix(&orig);
    if !is_explicit_absolute_path_str(&raw) {
        return false;
    }

    let clean = normalize_path(Path::new(&raw));
    let path_str = clean.to_string_lossy();
    let normalized = path_str.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');

    let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        // Must have at least a parent/drive and a folder name (cannot be /, C:, or /Lumen)
        return false;
    }

    let folder_name = parts[parts.len() - 1];
    if !is_dedicated_lumen_folder_name(folder_name) {
        return false;
    }

    #[cfg(windows)]
    {
        for var in &["USERPROFILE", "APPDATA", "LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)", "SystemRoot"] {
            if let Ok(val) = env::var(var) {
                let norm_val = val.replace('\\', "/");
                if trimmed.eq_ignore_ascii_case(norm_val.trim_end_matches('/')) {
                    return false;
                }
            }
        }
        let mut sysroots = vec![get_os_windows_dir().to_string_lossy().to_string()];
        if let Ok(sysroot) = env::var("SystemRoot") {
            sysroots.push(sysroot);
        }
        let lower_trimmed = trimmed.to_lowercase();
        for sysroot in sysroots {
            let norm_sr = strip_windows_verbatim_prefix(&sysroot).replace('\\', "/").to_lowercase();
            let sr_trimmed = norm_sr.trim_end_matches('/');
            if !sr_trimmed.is_empty()
                && (lower_trimmed == sr_trimmed || lower_trimmed.starts_with(&format!("{}/", sr_trimmed)))
            {
                return false;
            }
        }
        if let Ok(userprofile) = env::var("USERPROFILE") {
            let norm_up = userprofile.replace('\\', "/");
            let up_trimmed = norm_up.trim_end_matches('/');
            for sub in &["Desktop", "Downloads", "Documents", "Pictures", "Videos", "Music"] {
                let sensitive = format!("{}/{}", up_trimmed, sub);
                if trimmed.eq_ignore_ascii_case(&sensitive) {
                    return false;
                }
            }
        }
    }

    #[cfg(unix)]
    {
        for sys_dir in &[
            "/etc", "/usr", "/bin", "/sbin", "/lib", "/lib64", "/var", "/boot", "/sys", "/proc", "/dev", "/root",
        ] {
            if trimmed == *sys_dir || trimmed.starts_with(&format!("{}/", sys_dir)) {
                return false;
            }
        }
        if let Ok(home) = env::var("HOME") {
            let h = home.trim_end_matches('/');
            if trimmed == h {
                return false;
            }
            for sub in &["Desktop", "Downloads", "Documents", "Pictures", "Videos", "Music"] {
                let sensitive = format!("{}/{}", h, sub);
                if trimmed.eq_ignore_ascii_case(&sensitive) {
                    return false;
                }
            }
        }
    }

    true
}

fn channel_install_dirs(extra_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(d) = extra_dir {
        candidates.push(d.to_path_buf());
    }
    if let Some(m) = get_modern_installed_info_with_opts(true) {
        candidates.push(m.dir);
    }
    candidates.push(get_default_install_dir());
    let mut dirs = Vec::new();
    for d in candidates {
        let resolved = d.canonicalize().unwrap_or(d);
        let clean = PathBuf::from(strip_windows_verbatim_prefix(&resolved.to_string_lossy()));
        if !dirs.contains(&clean) {
            dirs.push(clean);
        }
    }
    dirs
}

#[cfg(not(windows))]
fn unix_channel_process_names() -> &'static [&'static str] {
    if IS_DEV_CHANNEL {
        &["Lumen-dev", "lumen-dev"]
    } else {
        &["Lumen", "lumen"]
    }
}

fn is_app_running_in_dir(extra_dir: Option<&Path>) -> bool {
    let current_pid = std::process::id();
    let current_exe_str = env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let cur_clean = current_exe_str.replace('\\', "/").to_lowercase();
    let cur_canon = cur_clean
        .trim_start_matches("//?/")
        .trim_start_matches("\\\\?\\")
        .to_string();

    let install_dirs = channel_install_dirs(extra_dir);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let channel_exes = if IS_DEV_CHANNEL {
            "Lumen-dev.exe|lumen-dev.exe"
        } else {
            "Lumen.exe|lumen.exe"
        };
        let install_dirs_env = install_dirs
            .iter()
            .map(|d| d.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("|");

        const SCRIPT: &str = "\
            $curPid = [int]$env:LUMEN_CUR_PID; \
            $curExe = $env:LUMEN_CUR_EXE; \
            $chanExes = @($env:LUMEN_CHANNEL_EXES -split '\\|' | Where-Object { $_ }); \
            $instDirs = @($env:LUMEN_INSTALL_DIRS -split '\\|' | Where-Object { $_ } | ForEach-Object { $_.TrimEnd('\\') + '\\' }); \
            $procs = Get-CimInstance Win32_Process -Filter \"Name = 'Lumen.exe' or Name = 'lumen.exe' or Name = 'Lumen-dev.exe' or Name = 'lumen-dev.exe' or Name = 'lumen-cli.exe'\" -ErrorAction SilentlyContinue; \
            foreach ($p in $procs) { \
                if ($p.ProcessId -eq $curPid -or -not $p.ExecutablePath) { continue } \
                if ($curExe -and $p.ExecutablePath.Equals($curExe, [StringComparison]::OrdinalIgnoreCase)) { continue } \
                $isMain = $false; \
                foreach ($n in $chanExes) { \
                    if ($p.Name.Equals($n, [StringComparison]::OrdinalIgnoreCase)) { $isMain = $true; break } \
                } \
                $isChannelCli = $false; \
                if (-not $isMain -and $p.Name.Equals('lumen-cli.exe', [StringComparison]::OrdinalIgnoreCase)) { \
                    foreach ($dir in $instDirs) { \
                        if ($p.ExecutablePath.StartsWith($dir, [StringComparison]::OrdinalIgnoreCase)) { $isChannelCli = $true; break } \
                    } \
                } \
                if ($isMain -or $isChannelCli) { \
                    Write-Output \"$($p.ProcessId)|$($p.ExecutablePath)\" \
                } \
            }";

        let output = if let Some(ps_path) = get_system_powershell() {
            Command::new(ps_path)
                .env("LUMEN_CUR_PID", current_pid.to_string())
                .env("LUMEN_CUR_EXE", &current_exe_str)
                .env("LUMEN_CHANNEL_EXES", channel_exes)
                .env("LUMEN_INSTALL_DIRS", &install_dirs_env)
                .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", SCRIPT])
                .creation_flags(CREATE_NO_WINDOW)
                .output()
        } else {
            return false;
        };

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let parts: Vec<&str> = trimmed.split('|').collect();
                if let Ok(pid) = parts[0].parse::<u32>() {
                    if pid == current_pid {
                        continue;
                    }
                    if parts.len() > 1 && !parts[1].trim().is_empty() {
                        let proc_clean = parts[1].trim().replace('\\', "/").to_lowercase();
                        let proc_canon = proc_clean
                            .trim_start_matches("//?/")
                            .trim_start_matches("\\\\?\\")
                            .to_string();
                        if !cur_canon.is_empty() && proc_canon == cur_canon {
                            continue;
                        }
                        return true;
                    }
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        use std::process::Command;
        for name in unix_channel_process_names() {
            if let Ok(out) = Command::new("pgrep").arg("-x").arg(name).output() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    if let Ok(pid) = line.trim().parse::<u32>() {
                        if pid != current_pid {
                            if let Ok(proc_exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
                                let proc_clean = proc_exe.to_string_lossy().replace('\\', "/").to_lowercase();
                                if !cur_canon.is_empty() && proc_clean == cur_canon {
                                    continue;
                                }
                            }
                            return true;
                        }
                    }
                }
            }
        }
        if let Ok(out) = Command::new("pgrep").arg("-x").arg("lumen-cli").output() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if pid != current_pid {
                        if let Ok(proc_exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
                            if install_dirs.iter().any(|dir| proc_exe.starts_with(dir)) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

#[tauri::command]
fn is_app_running() -> bool {
    is_app_running_in_dir(None)
}

fn kill_running_app_in_dir(extra_dir: Option<&Path>) -> Result<(), String> {
    let current_pid = std::process::id();
    let install_dirs = channel_install_dirs(extra_dir);

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let current_exe_str = env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let channel_exes = if IS_DEV_CHANNEL {
            "Lumen-dev.exe|lumen-dev.exe"
        } else {
            "Lumen.exe|lumen.exe"
        };
        let install_dirs_env = install_dirs
            .iter()
            .map(|d| d.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("|");

        const SCRIPT: &str = "\
            $curPid = [int]$env:LUMEN_CUR_PID; \
            $curExe = $env:LUMEN_CUR_EXE; \
            $chanExes = @($env:LUMEN_CHANNEL_EXES -split '\\|' | Where-Object { $_ }); \
            $instDirs = @($env:LUMEN_INSTALL_DIRS -split '\\|' | Where-Object { $_ } | ForEach-Object { $_.TrimEnd('\\') + '\\' }); \
            $procs = Get-CimInstance Win32_Process -Filter \"Name = 'Lumen.exe' or Name = 'lumen.exe' or Name = 'Lumen-dev.exe' or Name = 'lumen-dev.exe' or Name = 'lumen-cli.exe'\" -ErrorAction SilentlyContinue; \
            foreach ($p in $procs) { \
                if ($p.ProcessId -eq $curPid -or -not $p.ExecutablePath) { continue } \
                if ($curExe -and $p.ExecutablePath.Equals($curExe, [StringComparison]::OrdinalIgnoreCase)) { continue } \
                $isMain = $false; \
                foreach ($n in $chanExes) { \
                    if ($p.Name.Equals($n, [StringComparison]::OrdinalIgnoreCase)) { $isMain = $true; break } \
                } \
                $isChannelCli = $false; \
                if (-not $isMain -and $p.Name.Equals('lumen-cli.exe', [StringComparison]::OrdinalIgnoreCase)) { \
                    foreach ($dir in $instDirs) { \
                        if ($p.ExecutablePath.StartsWith($dir, [StringComparison]::OrdinalIgnoreCase)) { $isChannelCli = $true; break } \
                    } \
                } \
                if ($isMain -or $isChannelCli) { \
                    try { Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue } catch {} \
                } \
            }";

        if let Some(ps_path) = get_system_powershell() {
            let _ = Command::new(ps_path)
                .env("LUMEN_CUR_PID", current_pid.to_string())
                .env("LUMEN_CUR_EXE", &current_exe_str)
                .env("LUMEN_CHANNEL_EXES", channel_exes)
                .env("LUMEN_INSTALL_DIRS", &install_dirs_env)
                .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", SCRIPT])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }

        for _ in 0..15 {
            if !is_app_running_in_dir(extra_dir) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    #[cfg(not(windows))]
    {
        use std::process::Command;
        let current_exe_clean = env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/").to_lowercase())
            .unwrap_or_default();
        let mut target_pids = Vec::new();
        for name in unix_channel_process_names() {
            if let Ok(out) = Command::new("pgrep").arg("-x").arg(name).output() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    if let Ok(pid) = line.trim().parse::<u32>() {
                        if pid != current_pid {
                            if let Ok(proc_exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
                                let proc_clean = proc_exe.to_string_lossy().replace('\\', "/").to_lowercase();
                                if !current_exe_clean.is_empty() && proc_clean == current_exe_clean {
                                    continue;
                                }
                            }
                            if !target_pids.contains(&pid) {
                                target_pids.push(pid);
                            }
                        }
                    }
                }
            }
        }
        if let Ok(out) = Command::new("pgrep").arg("-x").arg("lumen-cli").output() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            for line in stdout.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    if pid != current_pid {
                        if let Ok(proc_exe) = std::fs::read_link(format!("/proc/{}/exe", pid)) {
                            if install_dirs.iter().any(|dir| proc_exe.starts_with(dir))
                                && !target_pids.contains(&pid)
                            {
                                target_pids.push(pid);
                            }
                        }
                    }
                }
            }
        }
        if !target_pids.is_empty() {
            for &pid in &target_pids {
                let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).output();
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
            for &pid in &target_pids {
                let _ = Command::new("kill").args(["-KILL", &pid.to_string()]).output();
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn kill_running_app() -> Result<(), String> {
    kill_running_app_in_dir(None)
}

fn check_not_symlink_or_reparse(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(format!(
                    "Target install directory '{}' must not be a symlink.",
                    path.display()
                ));
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
                if (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0 {
                    return Err(format!(
                        "Target install directory '{}' must not be a junction or reparse point.",
                        path.display()
                    ));
                }
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "Failed to inspect target install directory '{}': {}",
            path.display(),
            e
        )),
    }
}

#[tauri::command]
async fn start_install(
    target_dir: String,
    create_desktop_shortcut: bool,
    launch_on_startup: bool,
    launch_after: bool,
) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = create_desktop_shortcut;
        let _ = launch_on_startup;
    }
    let raw_dest_path = if target_dir.trim().is_empty() {
        get_default_install_dir()
    } else {
        PathBuf::from(&target_dir)
    };
    if !is_safe_dedicated_install_dir(&raw_dest_path) {
        return Err(format!(
            "Target install directory '{}' is not a dedicated, safe Lumen directory.",
            raw_dest_path.display()
        ));
    }
    check_not_symlink_or_reparse(&raw_dest_path)?;
    fs::create_dir_all(&raw_dest_path)
        .map_err(|e| format!("Failed to create install dir: {}", e))?;
    check_not_symlink_or_reparse(&raw_dest_path)?;

    let canonical_dest = raw_dest_path.canonicalize().map_err(|e| {
        format!(
            "Failed to resolve target install directory '{}': {}",
            raw_dest_path.display(),
            e
        )
    })?;
    if !is_safe_dedicated_install_dir(&canonical_dest) {
        return Err(format!(
            "Resolved target install directory '{}' is not a dedicated, safe Lumen directory.",
            canonical_dest.display()
        ));
    }
    let dest_path = PathBuf::from(strip_windows_verbatim_prefix(
        &canonical_dest.to_string_lossy(),
    ));

    // Terminate any running Lumen instances in this channel before unpacking
    let _ = kill_running_app_in_dir(Some(&dest_path));

    #[cfg(windows)]
    {
        if !IS_DEV_CHANNEL {
            remove_legacy_nsis_installation(Some(&dest_path))?;
        }
    }

    // 1. Extract embedded payload securely (preventing Tar Slip and rejecting non-file/non-dir entries)
    let decoder = GzDecoder::new(Cursor::new(EMBEDDED_PAYLOAD));
    let mut archive = Archive::new(decoder);
    for entry in archive.entries().map_err(|e| format!("Failed to read payload: {}", e))? {
        let mut entry = entry.map_err(|e| format!("Corrupt payload entry: {}", e))?;
        let entry_path = entry.path().map_err(|e| format!("Invalid payload path: {}", e))?.to_path_buf();

        for comp in entry_path.components() {
            match comp {
                std::path::Component::ParentDir | std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                    return Err(format!("Unsafe path component in payload entry: {}", entry_path.display()));
                }
                _ => {}
            }
        }

        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(format!(
                "Unsupported entry type in payload entry: {}",
                entry_path.display()
            ));
        }

        entry.unpack_in(&dest_path).map_err(|e| format!("Failed to unpack {}: {}", entry_path.display(), e))?;
    }

    // Standardize executable name on Windows per channel (Lumen.exe vs Lumen-dev.exe)
    #[cfg(windows)]
    let main_exe = {
        let target = dest_path.join(WINDOWS_MAIN_EXE_NAME);
        if !WINDOWS_MAIN_EXE_NAME.eq_ignore_ascii_case("lumen.exe") {
            let unpacked = if dest_path.join("lumen.exe").is_file() {
                Some(dest_path.join("lumen.exe"))
            } else if dest_path.join("Lumen.exe").is_file() {
                Some(dest_path.join("Lumen.exe"))
            } else {
                None
            };
            if let Some(src) = unpacked {
                if target.exists() {
                    fs::remove_file(&target).map_err(|e| {
                        format!(
                            "Failed to replace existing {}: {}",
                            target.display(),
                            e
                        )
                    })?;
                }
                fs::rename(&src, &target).map_err(|e| {
                    format!(
                        "Failed to rename {} to {}: {}",
                        src.display(),
                        target.display(),
                        e
                    )
                })?;
            }
            target
        } else if target.is_file() {
            target
        } else if dest_path.join("lumen.exe").is_file() {
            let src = dest_path.join("lumen.exe");
            fs::rename(&src, &target).map_err(|e| {
                format!(
                    "Failed to rename {} to {}: {}",
                    src.display(),
                    target.display(),
                    e
                )
            })?;
            target
        } else {
            target
        }
    };
    #[cfg(not(windows))]
    let main_exe = find_app_binary(&dest_path).unwrap_or_else(|| dest_path.join("lumen"));

    // 2. Copy current installer as uninstaller into install dir
    if let Ok(current_exe) = env::current_exe() {
        let uninstaller_dest = dest_path.join(uninstaller_filename());
        let same_file = current_exe.canonicalize().ok() == uninstaller_dest.canonicalize().ok()
            && uninstaller_dest.exists();
        if !same_file {
            fs::copy(&current_exe, &uninstaller_dest).map_err(|e| {
                format!(
                    "Failed to copy uninstaller to {}: {}",
                    uninstaller_dest.display(),
                    e
                )
            })?;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if main_exe.is_file() {
                let _ = fs::set_permissions(&main_exe, fs::Permissions::from_mode(0o755));
            }
            if uninstaller_dest.is_file() {
                let _ = fs::set_permissions(&uninstaller_dest, fs::Permissions::from_mode(0o755));
            }
        }
    }

    // 3. Windows-specific registration: Shortcuts and Registry
    #[cfg(windows)]
    {
        let uninstaller_exe = dest_path.join("uninstall.exe");

        // Start Menu Shortcut
        if let Ok(appdata) = env::var("APPDATA") {
            let start_menu = PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs");
            let _ = fs::create_dir_all(&start_menu);
            let lnk_path = start_menu.join(SHORTCUT_FILENAME);
            let _ = create_windows_shortcut(&main_exe, &lnk_path);
        }

        // Desktop Shortcut
        if create_desktop_shortcut {
            if let Ok(userprofile) = env::var("USERPROFILE") {
                let desktop_dir = PathBuf::from(userprofile).join("Desktop");
                let _ = fs::create_dir_all(&desktop_dir);
                let desktop_lnk = desktop_dir.join(SHORTCUT_FILENAME);
                let _ = create_windows_shortcut(&main_exe, &desktop_lnk);
            }
        }

        // Launch on Windows Startup
        if launch_on_startup {
            use winreg::enums::*;
            use winreg::RegKey;
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            if let Ok((run_key, _)) = hkcu.create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Run") {
                let exe_str = format!("\"{}\"", main_exe.to_string_lossy());
                let _ = run_key.set_value(STARTUP_REG_VALUE_NAME, &exe_str);
            }
        }

        // Register in Windows Settings / Add Remove Programs
        let _ = register_windows_uninstall(&dest_path, &main_exe, &uninstaller_exe);
    }

    // 4. Launch app if requested
    if launch_after && main_exe.exists() && is_safe_dedicated_install_dir(&dest_path) {
        let _ = std::process::Command::new(&main_exe).spawn();
    }

    Ok(())
}

fn run_uninstall_sync(purge_config: bool) -> Result<(), String> {
    // 1. Terminate running app instances first so files are not locked
    let _ = kill_running_app();

    let modern_info = get_modern_installed_info_with_opts(true);

    #[cfg(windows)]
    {
        if !IS_DEV_CHANNEL {
            remove_legacy_nsis_installation(modern_info.as_ref().map(|m| m.dir.as_path()))?;
        }
    }

    let install_dir = modern_info
        .map(|m| m.dir)
        .or_else(get_installed_dir)
        .unwrap_or_else(get_default_install_dir);

    #[cfg(windows)]
    {
        // 2. Remove Start Menu shortcut
        if let Ok(appdata) = env::var("APPDATA") {
            let start_menu_lnk = PathBuf::from(&appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join(SHORTCUT_FILENAME);
            let _ = fs::remove_file(start_menu_lnk);
        }

        // 3. Remove Desktop shortcut
        if let Ok(userprofile) = env::var("USERPROFILE") {
            let desktop_lnk = PathBuf::from(userprofile).join("Desktop").join(SHORTCUT_FILENAME);
            let _ = fs::remove_file(desktop_lnk);
        }

        // 4. Remove Run on Startup registry entry
        {
            use winreg::enums::*;
            use winreg::RegKey;
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            if let Ok(run_key) = hkcu.open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", KEY_SET_VALUE) {
                let _ = run_key.delete_value(STARTUP_REG_VALUE_NAME);
            }
        }

        // 5. Remove Windows Registry Uninstall key
        let _ = unregister_windows_uninstall();

        // 6. Optional: Purge configuration & local cache (strictly scoped to current channel)
        if purge_config {
            if let Ok(appdata) = env::var("APPDATA") {
                let channel_config_dir = PathBuf::from(&appdata).join(CONFIG_DIR_NAME);
                let _ = fs::remove_dir_all(channel_config_dir);
                if !IS_DEV_CHANNEL {
                    let legacy_cap_dir = PathBuf::from(&appdata).join("Lumen");
                    if legacy_cap_dir.exists() {
                        let _ = fs::remove_dir_all(legacy_cap_dir);
                    }
                }
            }
            if let Ok(userprofile) = env::var("USERPROFILE") {
                let lumen_config_dir = PathBuf::from(userprofile).join(".config").join(CONFIG_DIR_NAME);
                let _ = fs::remove_dir_all(lumen_config_dir);
            }
        }

        // 7. Remove installed files
        let is_running_from_install_dir = if let Ok(current_exe) = env::current_exe() {
            if let Ok(current_dir) = current_exe.parent().unwrap_or(Path::new(".")).canonicalize() {
                if let Ok(target_dir) = install_dir.canonicalize() {
                    current_dir == target_dir
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        let is_dedicated = is_safe_dedicated_install_dir(&install_dir);

        if !is_running_from_install_dir {
            // Running as setup installer outside the install dir: delete everything directly!
            if install_dir.exists() {
                let _ = fs::remove_file(install_dir.join(WINDOWS_MAIN_EXE_NAME));
                let _ = fs::remove_file(install_dir.join("lumen.exe"));
                let _ = fs::remove_file(install_dir.join("lumen-cli.exe"));
                let _ = fs::remove_file(install_dir.join("app-icon.png"));
                let _ = fs::remove_file(install_dir.join("uninstall.exe"));
                if is_dedicated {
                    let _ = fs::remove_dir_all(&install_dir);
                    if install_dir.exists() {
                        schedule_self_deletion(&install_dir, true);
                    }
                }
            }
        } else {
            // Running as uninstall.exe inside the install dir:
            // Delete main binaries immediately, then schedule deletion of uninstall.exe and dir if dedicated
            let _ = fs::remove_file(install_dir.join(WINDOWS_MAIN_EXE_NAME));
            let _ = fs::remove_file(install_dir.join("lumen.exe"));
            let _ = fs::remove_file(install_dir.join("lumen-cli.exe"));
            let _ = fs::remove_file(install_dir.join("app-icon.png"));
            schedule_self_deletion(&install_dir, is_dedicated);
        }
    }

    #[cfg(not(windows))]
    {
        if purge_config {
            if let Ok(home) = env::var("HOME") {
                #[cfg(target_os = "macos")]
                {
                    let mac_config_dir = PathBuf::from(&home)
                        .join("Library")
                        .join("Application Support")
                        .join(CONFIG_DIR_NAME);
                    let _ = fs::remove_dir_all(mac_config_dir);
                }
                let lumen_config_dir = PathBuf::from(home).join(".config").join(CONFIG_DIR_NAME);
                let _ = fs::remove_dir_all(lumen_config_dir);
            }
        }
        if install_dir.exists() {
            let _ = fs::remove_file(install_dir.join("Lumen"));
            let _ = fs::remove_file(install_dir.join("lumen"));
            let _ = fs::remove_file(install_dir.join("Lumen-dev"));
            let _ = fs::remove_file(install_dir.join("lumen-dev"));
            let _ = fs::remove_file(install_dir.join("lumen-cli"));
            let _ = fs::remove_file(install_dir.join("uninstall"));
            let _ = fs::remove_file(install_dir.join("uninstall.exe"));
            if is_safe_dedicated_install_dir(&install_dir) {
                let _ = fs::remove_dir_all(&install_dir);
            }
        }
    }

    Ok(())
}

#[tauri::command]
async fn start_uninstall(purge_config: bool) -> Result<(), String> {
    run_uninstall_sync(purge_config)
}

#[tauri::command]
fn launch_app(target_dir: String) -> Result<(), String> {
    let dest_path = PathBuf::from(&target_dir);
    if !is_safe_dedicated_install_dir(&dest_path) {
        return Err("Untrusted execution directory".to_string());
    }
    if let Some(main_exe) = find_app_binary(&dest_path) {
        std::process::Command::new(main_exe)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn minimize_installer(window: Window) {
    let _ = window.minimize();
}

#[tauri::command]
fn close_installer(window: Window) {
    let _ = window.close();
}

#[tauri::command]
fn start_dragging(window: Window) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
fn show_window(window: Window) {
    #[cfg(windows)]
    {
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                use std::ffi::c_void;
                #[link(name = "dwmapi")]
                extern "system" {
                    fn DwmSetWindowAttribute(
                        hwnd: *mut c_void,
                        dwAttribute: u32,
                        pvAttribute: *const c_void,
                        cbAttribute: u32,
                    ) -> i32;
                }
                // DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_ROUND = 2
                let preference: u32 = 2;
                let _ = DwmSetWindowAttribute(
                    hwnd.0 as *mut c_void,
                    33,
                    &preference as *const u32 as *const c_void,
                    std::mem::size_of::<u32>() as u32,
                );
            }
        }
    }
    let _ = window.show();
    let _ = window.set_focus();
}

#[cfg(windows)]
fn get_os_windows_dir() -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemWindowsDirectoryW(lp_buffer: *mut u16, u_size: u32) -> u32;
    }

    let mut buf = vec![0u16; 260];
    let len = unsafe { GetSystemWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) };
    if len > 0 && (len as usize) < buf.len() {
        let dir = PathBuf::from(OsString::from_wide(&buf[..len as usize]));
        if dir.is_dir() {
            return dir;
        }
    } else if len as usize >= buf.len() {
        let mut big = vec![0u16; len as usize + 1];
        let len2 = unsafe { GetSystemWindowsDirectoryW(big.as_mut_ptr(), big.len() as u32) };
        if len2 > 0 && (len2 as usize) < big.len() {
            let dir = PathBuf::from(OsString::from_wide(&big[..len2 as usize]));
            if dir.is_dir() {
                return dir;
            }
        }
    }

    if let Ok(nt) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
    {
        if let Ok(sr) = nt.get_value::<String, _>("SystemRoot") {
            let p = PathBuf::from(sr);
            if p.is_dir() {
                return p;
            }
        }
    }

    PathBuf::from(r"C:\Windows")
}

#[cfg(windows)]
fn get_system_powershell() -> Option<PathBuf> {
    let sys_ps = get_os_windows_dir().join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    if sys_ps.is_file() {
        return Some(sys_ps);
    }
    let default_ps = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    if default_ps.is_file() {
        return Some(default_ps);
    }
    None
}

#[cfg(windows)]
fn get_trusted_program_files_roots() -> (Vec<PathBuf>, Vec<String>) {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let mut raw_roots: Vec<PathBuf> = Vec::new();
    if let Ok(cv) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion")
    {
        for key in &["ProgramFilesDir", "ProgramFilesDir (x86)", "ProgramW6432Dir"] {
            if let Ok(val) = cv.get_value::<String, _>(key) {
                if !val.trim().is_empty() {
                    raw_roots.push(PathBuf::from(val));
                }
            }
        }
    }

    let win_dir = get_os_windows_dir();
    let sys_drive = win_dir
        .to_string_lossy()
        .chars()
        .take(2)
        .collect::<String>();
    if sys_drive.len() == 2 && sys_drive.ends_with(':') {
        raw_roots.push(PathBuf::from(format!(r"{}\Program Files", sys_drive)));
        raw_roots.push(PathBuf::from(format!(r"{}\Program Files (x86)", sys_drive)));
    }
    raw_roots.push(PathBuf::from(r"C:\Program Files"));
    raw_roots.push(PathBuf::from(r"C:\Program Files (x86)"));

    let mut root_dirs: Vec<PathBuf> = Vec::new();
    let mut normalized_roots: Vec<String> = Vec::new();
    for root in raw_roots {
        if let Ok(canon) = root.canonicalize() {
            let clean = PathBuf::from(strip_windows_verbatim_prefix(&canon.to_string_lossy()));
            let norm = normalize_trusted_win_root(&clean.to_string_lossy());
            if !norm.is_empty() && !normalized_roots.contains(&norm) {
                normalized_roots.push(norm);
                root_dirs.push(clean);
            }
        }
    }
    (root_dirs, normalized_roots)
}

#[cfg(windows)]
fn has_legacy_nsis_marker(dir: &Path) -> bool {
    dir.join("Lumen.exe").is_file()
        || dir.join("lumen.exe").is_file()
        || dir.join("gotohp.exe").is_file()
        || dir.join("uninstall.exe").is_file()
        || dir.join("Uninstall Lumen.exe").is_file()
        || dir.join("wails.json").is_file()
}

#[cfg(any(windows, test))]
fn escape_powershell_single_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\r' | '\n' | '\0' => {}
            '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => {
                out.push(ch);
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(windows)]
const PS_REMOVE_DIR_SAFE_FN: &str = r#"function Remove-DirSafe([string]$Path) {
    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) { return }
    try { $attrs = [IO.File]::GetAttributes($Path) } catch { return }
    $isDir = ($attrs -band [IO.FileAttributes]::Directory) -ne 0
    $isReparse = ($attrs -band [IO.FileAttributes]::ReparsePoint) -ne 0
    if (-not $isDir) {
        try {
            if (($attrs -band [IO.FileAttributes]::ReparsePoint) -eq 0 -and ($attrs -band [IO.FileAttributes]::ReadOnly) -ne 0) {
                [IO.File]::SetAttributes($Path, ($attrs -band (-bnot [IO.FileAttributes]::ReadOnly)))
            }
            [IO.File]::Delete($Path)
        } catch {}
        return
    }
    if ($isReparse) {
        try { [IO.Directory]::Delete($Path, $false) } catch {}
        return
    }
    try {
        foreach ($f in [IO.Directory]::EnumerateFiles($Path)) {
            try {
                $fa = [IO.File]::GetAttributes($f)
                if (($fa -band [IO.FileAttributes]::ReparsePoint) -eq 0 -and ($fa -band [IO.FileAttributes]::ReadOnly) -ne 0) {
                    [IO.File]::SetAttributes($f, ($fa -band (-bnot [IO.FileAttributes]::ReadOnly)))
                }
                [IO.File]::Delete($f)
            } catch {}
        }
    } catch {}
    try {
        foreach ($sub in [IO.Directory]::EnumerateDirectories($Path)) {
            Remove-DirSafe $sub
        }
    } catch {}
    try {
        if (($attrs -band [IO.FileAttributes]::ReadOnly) -ne 0) {
            [IO.File]::SetAttributes($Path, ($attrs -band (-bnot [IO.FileAttributes]::ReadOnly)))
        }
        [IO.Directory]::Delete($Path, $false)
    } catch {}
}
"#;

#[cfg(windows)]
fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(windows)]
fn to_reg_expand_sz(s: &str) -> winreg::RegValue<'static> {
    let mut bytes: Vec<u8> = s
        .encode_utf16()
        .chain(std::iter::once(0u16))
        .flat_map(|u| u.to_le_bytes())
        .collect();
    if bytes.is_empty() {
        bytes.extend_from_slice(&[0, 0]);
    }
    winreg::RegValue {
        bytes: bytes.into(),
        vtype: winreg::enums::REG_EXPAND_SZ,
    }
}

#[cfg(windows)]
fn create_aumid_shortcut_com(target_exe: &Path, shortcut_path: &Path, aumid: &str) -> bool {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct PropertyKey {
        fmtid: Guid,
        pid: u32,
    }

    #[repr(C)]
    struct PropVariant {
        vt: u16,
        w_reserved1: u16,
        w_reserved2: u16,
        w_reserved3: u16,
        data1: *const u16,
        data2: usize,
    }

    #[repr(C)]
    struct IShellLinkWVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_path: *const c_void,
        get_id_list: *const c_void,
        set_id_list: *const c_void,
        get_description: *const c_void,
        set_description: *const c_void,
        get_working_directory: *const c_void,
        set_working_directory: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        get_arguments: *const c_void,
        set_arguments: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
        get_hotkey: *const c_void,
        set_hotkey: *const c_void,
        get_show_cmd: *const c_void,
        set_show_cmd: *const c_void,
        get_icon_location: *const c_void,
        set_icon_location: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32,
        set_relative_path: *const c_void,
        resolve: *const c_void,
        set_path: unsafe extern "system" fn(*mut c_void, *const u16) -> i32,
    }

    #[repr(C)]
    struct IPropertyStoreVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_count: *const c_void,
        get_at: *const c_void,
        get_value: *const c_void,
        set_value:
            unsafe extern "system" fn(*mut c_void, *const PropertyKey, *const PropVariant) -> i32,
        commit: unsafe extern "system" fn(*mut c_void) -> i32,
    }

    #[repr(C)]
    struct IPersistFileVtbl {
        query_interface:
            unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> i32,
        add_ref: unsafe extern "system" fn(*mut c_void) -> u32,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        get_class_id: *const c_void,
        is_dirty: *const c_void,
        load: *const c_void,
        save: unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32,
        save_completed: *const c_void,
        get_cur_file: *const c_void,
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(pv_reserved: *mut c_void, dw_co_init: u32) -> i32;
        fn CoUninitialize();
        fn CoCreateInstance(
            rclsid: *const Guid,
            p_unk_outer: *mut c_void,
            dw_cls_context: u32,
            riid: *const Guid,
            ppv: *mut *mut c_void,
        ) -> i32;
    }

    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const CLSCTX_INPROC_SERVER: u32 = 0x1;
    const VT_LPWSTR: u16 = 31;

    const CLSID_SHELL_LINK: Guid = Guid {
        data1: 0x00021401,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    const IID_ISHELL_LINK_W: Guid = Guid {
        data1: 0x000214F9,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    const IID_IPROPERTY_STORE: Guid = Guid {
        data1: 0x886D8EEB,
        data2: 0x8CF2,
        data3: 0x4446,
        data4: [0x8D, 0x02, 0xCD, 0xBA, 0x1D, 0xBD, 0xCF, 0x99],
    };
    const IID_IPERSIST_FILE: Guid = Guid {
        data1: 0x0000010B,
        data2: 0x0000,
        data3: 0x0000,
        data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
    };
    const PKEY_APP_USER_MODEL_ID: PropertyKey = PropertyKey {
        fmtid: Guid {
            data1: 0x9F4C2855,
            data2: 0x9F79,
            data3: 0x4B39,
            data4: [0xA8, 0xD0, 0xE1, 0xD4, 0x2D, 0xE1, 0xD5, 0xF3],
        },
        pid: 5,
    };

    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0u16)).collect()
    }

    let target_wide = to_wide(&target_exe.to_string_lossy());
    let workdir_wide = target_exe
        .parent()
        .map(|p| to_wide(&p.to_string_lossy()))
        .unwrap_or_else(|| to_wide("."));
    let shortcut_wide = to_wide(&shortcut_path.to_string_lossy());
    let aumid_wide = to_wide(aumid);
    let empty_wide = to_wide("");

    unsafe {
        let hr_init = CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED);
        let should_uninit = hr_init == 0 || hr_init == 1;

        let mut shell_link: *mut c_void = std::ptr::null_mut();
        let hr = CoCreateInstance(
            &CLSID_SHELL_LINK,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ISHELL_LINK_W,
            &mut shell_link,
        );
        if hr < 0 || shell_link.is_null() {
            if should_uninit {
                CoUninitialize();
            }
            return false;
        }

        let sl_vtbl = &**(shell_link as *mut *const IShellLinkWVtbl);
        let mut ok = (sl_vtbl.set_path)(shell_link, target_wide.as_ptr()) >= 0;
        let _ = (sl_vtbl.set_arguments)(shell_link, empty_wide.as_ptr());
        let _ = (sl_vtbl.set_working_directory)(shell_link, workdir_wide.as_ptr());
        let _ = (sl_vtbl.set_icon_location)(shell_link, target_wide.as_ptr(), 0);

        if ok {
            let mut prop_store: *mut c_void = std::ptr::null_mut();
            if (sl_vtbl.query_interface)(shell_link, &IID_IPROPERTY_STORE, &mut prop_store) >= 0
                && !prop_store.is_null()
            {
                let ps_vtbl = &**(prop_store as *mut *const IPropertyStoreVtbl);
                let pv = PropVariant {
                    vt: VT_LPWSTR,
                    w_reserved1: 0,
                    w_reserved2: 0,
                    w_reserved3: 0,
                    data1: aumid_wide.as_ptr(),
                    data2: 0,
                };
                let hr_set = (ps_vtbl.set_value)(prop_store, &PKEY_APP_USER_MODEL_ID, &pv);
                let hr_commit = if hr_set >= 0 {
                    (ps_vtbl.commit)(prop_store)
                } else {
                    hr_set
                };
                ok = hr_set >= 0 && hr_commit >= 0;
                let _ = (ps_vtbl.release)(prop_store);
            } else {
                ok = false;
            }
        }

        let mut saved = false;
        if ok {
            let mut persist_file: *mut c_void = std::ptr::null_mut();
            if (sl_vtbl.query_interface)(shell_link, &IID_IPERSIST_FILE, &mut persist_file) >= 0
                && !persist_file.is_null()
            {
                let pf_vtbl = &**(persist_file as *mut *const IPersistFileVtbl);
                saved = (pf_vtbl.save)(persist_file, shortcut_wide.as_ptr(), 1) >= 0;
                let _ = (pf_vtbl.release)(persist_file);
            }
        }

        let _ = (sl_vtbl.release)(shell_link);
        if should_uninit {
            CoUninitialize();
        }
        saved
    }
}

#[cfg(windows)]
fn create_windows_shortcut(target_exe: &Path, shortcut_path: &Path) -> Result<(), String> {
    if create_aumid_shortcut_com(target_exe, shortcut_path, APP_AUMID) {
        return Ok(());
    }

    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const SCRIPT: &str = "\
        $ws = New-Object -ComObject WScript.Shell; \
        $s = $ws.CreateShortcut($env:LUMEN_SHORTCUT_PATH); \
        $s.TargetPath = $env:LUMEN_TARGET_EXE; \
        $s.IconLocation = \"$($env:LUMEN_TARGET_EXE),0\"; \
        $s.Save()";
    let ps_exe = get_system_powershell()
        .ok_or_else(|| "System PowerShell executable not found".to_string())?;
    let output = Command::new(&ps_exe)
        .creation_flags(CREATE_NO_WINDOW)
        .env("LUMEN_SHORTCUT_PATH", shortcut_path)
        .env("LUMEN_TARGET_EXE", target_exe)
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", SCRIPT])
        .output()
        .map_err(|e| format!("PowerShell shortcut creation failed: {}", e))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    Ok(())
}

#[cfg(windows)]
fn register_windows_uninstall(
    install_dir: &Path,
    main_exe: &Path,
    uninstaller_exe: &Path,
) -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let reg_path = format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}",
        UNINSTALL_REG_KEY_NAME
    );
    let (key, _) = hkcu
        .create_subkey(&reg_path)
        .map_err(|e| e.to_string())?;

    key.set_value("DisplayName", &APP_DISPLAY_NAME).map_err(|e| e.to_string())?;
    key.set_value("DisplayVersion", &installer_app_version()).map_err(|e| e.to_string())?;
    key.set_value("Publisher", &"Reiux").map_err(|e| e.to_string())?;
    key.set_value("InstallLocation", &install_dir.to_string_lossy().to_string())
        .map_err(|e| e.to_string())?;
    key.set_value(
        "DisplayIcon",
        &format!("{},0", main_exe.to_string_lossy()),
    )
    .map_err(|e| e.to_string())?;
    key.set_value("UninstallString", &format!("\"{}\"", uninstaller_exe.to_string_lossy()))
        .map_err(|e| e.to_string())?;
    key.set_value("QuietUninstallString", &format!("\"{}\" --silent", uninstaller_exe.to_string_lossy()))
        .map_err(|e| e.to_string())?;
    key.set_value("URLInfoAbout", &"https://github.com/Reiux/Lumen")
        .map_err(|e| e.to_string())?;

    // Register Windows Toast Notification AUMID display name & icon so
    // Windows Action Center shows "Lumen" / "Lumen Dev" + app icon instead of raw AUMID.
    let icon_path = install_dir.join("app-icon.png");
    let _ = fs::write(&icon_path, WINDOWS_NOTIFICATION_ICON_PNG);
    let icon_path_str = icon_path.to_string_lossy().to_string();

    let aumid_subkey = format!("Software\\Classes\\AppUserModelId\\{}", APP_AUMID);
    let notif_settings_subkey = format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Notifications\\Settings\\{}",
        APP_AUMID
    );
    let existing_display_name = hkcu
        .open_subkey(&aumid_subkey)
        .ok()
        .and_then(|k| k.get_value::<String, _>("DisplayName").ok());
    let existing_icon_uri = hkcu
        .open_subkey(&aumid_subkey)
        .ok()
        .and_then(|k| k.get_value::<String, _>("IconUri").ok());
    let aumid_changed = existing_display_name.as_deref() != Some(APP_DISPLAY_NAME)
        || existing_icon_uri.as_deref() != Some(icon_path_str.as_str());

    if aumid_changed {
        let _ = hkcu.delete_subkey_all(&aumid_subkey);
        let _ = hkcu.delete_subkey_all(&notif_settings_subkey);
    }
    if let Ok((aumid_key, _)) = hkcu.create_subkey(&aumid_subkey) {
        let _ = aumid_key.set_raw_value("DisplayName", &to_reg_expand_sz(APP_DISPLAY_NAME));
        let _ = aumid_key.set_raw_value("IconUri", &to_reg_expand_sz(&icon_path_str));
        let _ = aumid_key.set_value("IconBackgroundColor", &"FF121316");
    }

    Ok(())
}

#[cfg(windows)]
fn remove_legacy_nsis_installation(modern_dir: Option<&Path>) -> Result<(), String> {
    if IS_DEV_CHANNEL {
        return Ok(());
    }
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use winreg::enums::*;
    use winreg::RegKey;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let hkcu_modern = get_hkcu_modern_install();
    let is_protected_modern = |p: &Path| -> bool {
        if is_modern_v2_dir(p, hkcu_modern.as_ref()) {
            return true;
        }
        if let Some(md) = modern_dir {
            let same = p
                .canonicalize()
                .ok()
                .zip(md.canonicalize().ok())
                .map(|(a, b)| a == b)
                .unwrap_or_else(|| p == md);
            if same && !p.join("wails.json").is_file() {
                return true;
            }
        }
        false
    };

    let (trusted_root_dirs, trusted_roots) = get_trusted_program_files_roots();

    let mut legacy_dirs: Vec<PathBuf> = Vec::new();
    let mut push_verified_legacy = |candidate: &Path| {
        if !candidate.is_dir() || is_protected_modern(candidate) {
            return;
        }
        if let Ok(canon) = candidate.canonicalize() {
            let clean = PathBuf::from(strip_windows_verbatim_prefix(&canon.to_string_lossy()));
            if is_protected_modern(&clean) {
                return;
            }
            let has_marker = has_legacy_nsis_marker(&clean);
            if is_verified_legacy_dir_candidate(&clean.to_string_lossy(), &trusted_roots, has_marker) {
                legacy_dirs.push(clean);
            }
        }
    };

    // 1. Scan OS-verified Program Files folders (HKLM/system-root derived, never process env vars)
    for pf_root in &trusted_root_dirs {
        let candidates = [
            pf_root.join("Reiux").join("Lumen"),
            pf_root.join("Lumen"),
            pf_root.join("gotohp"),
        ];
        for p in &candidates {
            push_verified_legacy(p);
        }
    }

    // 2. Scan HKLM, WOW6432Node, and HKCU Uninstall registry keys for legacy Go NSIS paths
    let hives = [
        (HKEY_LOCAL_MACHINE, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKEY_LOCAL_MACHINE, "Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKEY_CURRENT_USER, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    ];

    let mut has_hklm_legacy_registry = false;
    for (hkey, root_path) in hives {
        let root = RegKey::predef(hkey);
        if let Ok(uninstall_root) = root.open_subkey(root_path) {
            for key_name in uninstall_root.enum_keys().filter_map(|r| r.ok()) {
                let lower_key = key_name.to_lowercase();
                if lower_key == "reiuxlumen" || lower_key == "reiux lumen" || lower_key == "gotohp"
                    || (lower_key == "lumen" && hkey == HKEY_LOCAL_MACHINE)
                {
                    if hkey == HKEY_LOCAL_MACHINE {
                        has_hklm_legacy_registry = true;
                    }
                    if let Ok(app_key) = uninstall_root.open_subkey(&key_name) {
                        if let Some(p) = extract_dir_from_reg_key(&app_key) {
                            push_verified_legacy(&p);
                        }
                    }
                }
            }
        }
    }

    let sys_drive = get_os_windows_dir()
        .to_string_lossy()
        .chars()
        .take(2)
        .collect::<String>();
    let common_appdata = PathBuf::from(format!(r"{}\ProgramData", sys_drive));
    let start_menu_programs = common_appdata
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    let mut public_desktop_dirs = vec![PathBuf::from(format!(r"{}\Users\Public\Desktop", sys_drive))];
    if let Ok(public_dir) = env::var("PUBLIC") {
        let pd = PathBuf::from(public_dir).join("Desktop");
        if !public_desktop_dirs.contains(&pd) {
            public_desktop_dirs.push(pd);
        }
    }
    let has_public_desktop_legacy = public_desktop_dirs.iter().any(|desktop| {
        desktop.join("Lumen.lnk").is_file() || desktop.join("gotohp.lnk").is_file()
    });
    let has_public_legacy = start_menu_programs.join("gotohp.lnk").is_file()
        || start_menu_programs.join("Reiux").join("Lumen.lnk").is_file()
        || start_menu_programs.join("Lumen.lnk").is_file()
        || has_public_desktop_legacy;

    if legacy_dirs.is_empty() && !has_hklm_legacy_registry && !has_public_legacy {
        cleanup_user_legacy_shortcuts_and_hkcu();
        return Ok(());
    }

    legacy_dirs.sort();
    legacy_dirs.dedup();

    // Build elevated PowerShell script to cleanly uninstall and purge legacy Program Files & HKLM
    let mut dir_list_ps = String::new();
    for d in &legacy_dirs {
        dir_list_ps.push_str(&format!("'{}', ", escape_powershell_single_quote(&d.to_string_lossy())));
    }
    let dir_list_ps = dir_list_ps.trim_end_matches(", ");

    let mut reiux_roots_ps = String::new();
    for pf_root in &trusted_root_dirs {
        let reiux_dir = pf_root.join("Reiux");
        reiux_roots_ps.push_str(&format!(
            "'{}', ",
            escape_powershell_single_quote(&reiux_dir.to_string_lossy())
        ));
    }
    let reiux_roots_ps = reiux_roots_ps.trim_end_matches(", ");

    let script = format!(
        r#"{remove_dir_safe_fn}
$dirs = @({dirs});
foreach ($d in $dirs) {{
    if (Test-Path -LiteralPath $d) {{
        $dPrefix = $d.TrimEnd('\') + '\';
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {{ $_.ExecutablePath -and $_.ExecutablePath.StartsWith($dPrefix, [System.StringComparison]::OrdinalIgnoreCase) }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }};
        for ($i = 0; $i -lt 5; $i++) {{
            Remove-DirSafe $d;
            if (-not (Test-Path -LiteralPath $d)) {{ break }}
            Start-Sleep -Milliseconds 400;
        }}
    }}
}}

$reiuxRoots = @({reiux_roots});
foreach ($r in $reiuxRoots) {{
    if (Test-Path -LiteralPath $r) {{
        try {{ [IO.Directory]::Delete($r, $false) }} catch {{}}
    }}
}}

$regKeys = @(
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\ReiuxLumen',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Reiux Lumen',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Lumen',
    'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\gotohp',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\ReiuxLumen',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Reiux Lumen',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\Lumen',
    'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\gotohp'
);
foreach ($reg in $regKeys) {{
    Remove-Item -LiteralPath $reg -Recurse -Force -ErrorAction SilentlyContinue;
}}

$pdata = [Environment]::GetFolderPath('CommonApplicationData');
if ($pdata) {{
    $sm = "$pdata\Microsoft\Windows\Start Menu\Programs";
    $knownLnks = @(
        "$sm\Lumen.lnk",
        "$sm\gotohp.lnk",
        "$sm\Reiux\Lumen.lnk",
        "$sm\Reiux\gotohp.lnk",
        "$sm\Reiux\Uninstall Lumen.lnk",
        "$sm\Lumen\Lumen.lnk",
        "$sm\Lumen\Uninstall Lumen.lnk"
    );
    foreach ($lnk in $knownLnks) {{
        Remove-Item -LiteralPath $lnk -Force -ErrorAction SilentlyContinue;
    }}
    foreach ($smDir in @("$sm\Reiux", "$sm\Lumen")) {{
        if (Test-Path -LiteralPath $smDir) {{
            try {{ [IO.Directory]::Delete($smDir, $false) }} catch {{}}
        }}
    }}
}}
$pubDesktop = [Environment]::GetFolderPath('CommonDesktopDirectory');
if ($pubDesktop) {{
    Remove-Item -LiteralPath "$pubDesktop\Lumen.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pubDesktop\gotohp.lnk" -Force -ErrorAction SilentlyContinue;
}}
"#,
        remove_dir_safe_fn = PS_REMOVE_DIR_SAFE_FN,
        dirs = dir_list_ps,
        reiux_roots = reiux_roots_ps
    );

    // Encode script to UTF-16LE Base64 for PowerShell -EncodedCommand to avoid writing a temp script
    let utf16_bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let b64_encoded = encode_base64(&utf16_bytes);

    let ps_path = get_system_powershell()
        .ok_or_else(|| "System PowerShell executable not found for legacy cleanup.".to_string())?;
    const ELEVATE_SCRIPT: &str = "\
        try { \
            $p = Start-Process -FilePath $env:LUMEN_PS_EXE \
                -WindowStyle Hidden \
                -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden', '-EncodedCommand', $env:LUMEN_ENCODED_CMD) \
                -Verb RunAs -Wait -PassThru -ErrorAction Stop; \
            if (-not $p) { exit 1 } \
            exit $p.ExitCode \
        } catch { \
            Write-Error $_.Exception.Message; \
            exit 1 \
        }";
    let output = Command::new(&ps_path)
        .env("LUMEN_PS_EXE", &ps_path)
        .env("LUMEN_ENCODED_CMD", &b64_encoded)
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", ELEVATE_SCRIPT])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Failed to request administrator elevation for legacy cleanup: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let detail = if stderr.is_empty() {
            "Administrator elevation was canceled or failed.".to_string()
        } else {
            stderr
        };
        return Err(format!("Failed to remove legacy Lumen installation: {}", detail));
    }

    cleanup_user_legacy_shortcuts_and_hkcu();
    Ok(())
}

#[cfg(windows)]
fn cleanup_user_legacy_shortcuts_and_hkcu() {
    use winreg::enums::*;
    use winreg::RegKey;

    if let Ok(appdata) = env::var("APPDATA") {
        let user_start = PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs");
        let _ = fs::remove_file(user_start.join("gotohp.lnk"));
        let _ = fs::remove_file(user_start.join("Reiux").join("Lumen.lnk"));
        let _ = fs::remove_file(user_start.join("Reiux").join("gotohp.lnk"));
        let _ = fs::remove_dir(user_start.join("Reiux"));
    }
    if let Ok(userprofile) = env::var("USERPROFILE") {
        let user_desktop = PathBuf::from(userprofile).join("Desktop");
        let _ = fs::remove_file(user_desktop.join("gotohp.lnk"));
    }

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(uninstall_root) = hkcu.open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall", KEY_WRITE) {
        let _ = uninstall_root.delete_subkey_all("ReiuxLumen");
        let _ = uninstall_root.delete_subkey_all("Reiux Lumen");
        let _ = uninstall_root.delete_subkey_all("gotohp");
    }
}

#[cfg(windows)]
fn unregister_windows_uninstall() -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let reg_path = format!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{}",
        UNINSTALL_REG_KEY_NAME
    );
    let _ = hkcu.delete_subkey_all(&reg_path);
    let aumid_subkey = format!("Software\\Classes\\AppUserModelId\\{}", APP_AUMID);
    let _ = hkcu.delete_subkey_all(&aumid_subkey);
    if !IS_DEV_CHANNEL {
        let _ = hkcu.delete_subkey_all("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ReiuxLumen");
        let _ = hkcu.delete_subkey_all("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\gotohp");
    }

    Ok(())
}

#[cfg(windows)]
fn schedule_self_deletion(install_dir: &Path, delete_dir: bool) {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let target_path = if delete_dir {
        install_dir.to_path_buf()
    } else {
        install_dir.join("uninstall.exe")
    };

    let local_installer = env::var("LOCALAPPDATA")
        .map(|l| PathBuf::from(l).join("com.reiux.lumen.installer"))
        .unwrap_or_default();

    let script = format!(
        "{}\
        for ($i = 0; $i -lt 30; $i++) {{ \
            Start-Sleep -Seconds 1; \
            if ($env:LUMEN_DELETE_IS_DIR -eq '1') {{ \
                Remove-DirSafe $env:LUMEN_DELETE_TARGET; \
            }} else {{ \
                Remove-Item -LiteralPath $env:LUMEN_DELETE_TARGET -Force -ErrorAction SilentlyContinue; \
            }} \
            if ($env:LUMEN_LOCAL_INSTALLER_CACHE) {{ \
                Remove-DirSafe $env:LUMEN_LOCAL_INSTALLER_CACHE; \
            }} \
            if (-not (Test-Path -LiteralPath $env:LUMEN_DELETE_TARGET)) {{ break }} \
        }}",
        PS_REMOVE_DIR_SAFE_FN
    );

    if let Some(ps_path) = get_system_powershell() {
        let _ = Command::new(ps_path)
            .env("LUMEN_DELETE_TARGET", &target_path)
            .env("LUMEN_DELETE_IS_DIR", if delete_dir { "1" } else { "0" })
            .env("LUMEN_LOCAL_INSTALLER_CACHE", &local_installer)
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn();
    }
}

fn main() {
    // Clean up any stale legacy installer cache in Roaming AppData
    #[cfg(windows)]
    {
        if let Ok(appdata) = env::var("APPDATA") {
            let legacy_roaming = PathBuf::from(appdata).join("com.reiux.lumen.installer");
            if legacy_roaming.exists() {
                let _ = fs::remove_dir_all(legacy_roaming);
            }
        }
    }

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--silent" || a == "-s") && is_uninstall_invocation() {
        let purge_config = args.iter().any(|a| a == "--purge-config" || a == "--purge");
        let exit_code = match run_uninstall_sync(purge_config) {
            Ok(()) => 0,
            Err(err) => {
                eprintln!("Silent uninstall failed: {}", err);
                1
            }
        };
        std::process::exit(exit_code);
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_setup_info,
            pick_directory,
            is_app_running,
            kill_running_app,
            start_install,
            start_uninstall,
            launch_app,
            minimize_installer,
            close_installer,
            start_dragging,
            show_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lumen installer");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_safe_dedicated_install_dir() {
        assert!(!is_safe_dedicated_install_dir(Path::new("/")));
        assert!(!is_safe_dedicated_install_dir(Path::new("C:\\")));
        assert!(!is_safe_dedicated_install_dir(Path::new("Lumen")));
        assert!(!is_safe_dedicated_install_dir(Path::new("relative/Lumen")));
        assert!(!is_safe_dedicated_install_dir(Path::new("C:\\Lumen\\..")));
        assert!(!is_safe_dedicated_install_dir(Path::new("/home/user/Downloads")));
        assert!(!is_safe_dedicated_install_dir(Path::new("C:\\Users\\User\\Desktop")));
        assert!(!is_safe_dedicated_install_dir(Path::new("C:\\Users\\User\\Downloads\\Lumen\\..")));
        assert!(!is_safe_dedicated_install_dir(Path::new("/home/user/Desktop/Lumen/..")));
        assert!(is_safe_dedicated_install_dir(Path::new("/home/user/.local/share/Lumen")));
        assert!(is_safe_dedicated_install_dir(Path::new("/home/user/.local/share/Lumen-dev")));
        assert!(is_safe_dedicated_install_dir(Path::new("C:\\Users\\User\\AppData\\Local\\Programs\\Lumen")));
        assert!(is_safe_dedicated_install_dir(Path::new("C:\\Users\\User\\AppData\\Local\\Programs\\Lumen Dev")));
        assert!(is_safe_dedicated_install_dir(Path::new("C:\\Program Files\\Reiux\\Lumen")));
        assert!(is_safe_dedicated_install_dir(Path::new("C:\\Program Files\\Lumen")));
    }

    #[test]
    fn test_normalize_path_preserves_windows_drive_and_root() {
        assert_eq!(normalize_path(Path::new("C:\\Lumen\\..")), PathBuf::from("C:/"));
        assert_eq!(
            normalize_path(Path::new("C:\\Users\\User\\Downloads\\Lumen\\..")),
            PathBuf::from("C:/Users/User/Downloads")
        );
        assert_eq!(normalize_path(Path::new("/home/../Lumen")), PathBuf::from("/Lumen"));
    }

    #[test]
    fn test_is_older_version() {
        assert!(is_older_version("1.0.0", "1.1.0"));
        assert!(is_older_version("v1.0.0", "1.1.0"));
        assert!(is_older_version("1.0.9", "1.1.0"));
        assert!(is_older_version("0.9.0", "1.0.0"));
        assert!(is_older_version("2.1.0-beta.1", "2.1.0"));
        assert!(!is_older_version("2.1.0", "2.1.0-beta.1"));
        assert!(!is_older_version("1.1.0", "1.1.0"));
        assert!(!is_older_version("1.2.0", "1.1.0"));
        assert!(!is_older_version("2.0.0", "1.1.0"));
    }

    #[test]
    fn test_find_app_binary_uses_platform_names() {
        let dir = env::temp_dir().join(format!("lumen_inst_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert!(find_app_binary(&dir).is_none());
        let expected = dir.join(app_binary_names()[1]);
        fs::write(&expected, b"bin").unwrap();
        assert_eq!(find_app_binary(&dir).as_deref(), Some(expected.as_path()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_strip_verbatim_and_verified_legacy_dir_candidate() {
        assert_eq!(
            strip_windows_verbatim_prefix(r"\\?\C:\Program Files\Lumen"),
            r"C:\Program Files\Lumen"
        );
        assert_eq!(
            normalize_path(Path::new(r"\\?\C:\Program Files\Lumen")),
            PathBuf::from("C:/Program Files/Lumen")
        );
        assert!(is_safe_dedicated_install_dir(Path::new(r"\\?\C:\Program Files\Lumen")));
        assert!(!is_safe_dedicated_install_dir(Path::new(r"\\?\C:\Windows\System32")));

        let trusted = vec![
            r"c:\program files\".to_string(),
            r"c:\program files (x86)\".to_string(),
        ];
        assert!(is_verified_legacy_dir_candidate(
            r"\\?\C:\Program Files\Reiux\Lumen",
            &trusted,
            true
        ));
        // Reject spoofed user directory outside trusted OS roots
        assert!(!is_verified_legacy_dir_candidate(
            r"C:\Users\StdUser\FakePF\Lumen",
            &trusted,
            true
        ));
        // Reject directories lacking legacy NSIS markers
        assert!(!is_verified_legacy_dir_candidate(
            r"C:\Program Files\Lumen",
            &trusted,
            false
        ));
        // Reject prefix confusion where root lacks trailing boundary
        assert!(!is_verified_legacy_dir_candidate(
            r"C:\Program Files Evil\Lumen",
            &trusted,
            true
        ));
    }

    #[test]
    fn test_escape_powershell_single_quote() {
        assert_eq!(escape_powershell_single_quote("C:\\Apps\\Lumen"), "C:\\Apps\\Lumen");
        assert_eq!(escape_powershell_single_quote("O'Reilly"), "O''Reilly");
        assert_eq!(
            escape_powershell_single_quote("a\u{2018}b\u{2019}c\u{201A}d\u{201B}e"),
            "a\u{2018}\u{2018}b\u{2019}\u{2019}c\u{201A}\u{201A}d\u{201B}\u{201B}e"
        );
        assert_eq!(
            escape_powershell_single_quote("line1\r\nline2\0'end"),
            "line1line2''end"
        );
    }

    #[test]
    fn test_is_safe_dedicated_install_dir_rejects_device_and_globalroot() {
        assert!(!is_safe_dedicated_install_dir(Path::new(r"\\.\C:\Program Files\Lumen")));
        assert!(!is_safe_dedicated_install_dir(Path::new("//./C:/Program Files/Lumen")));
        assert!(!is_safe_dedicated_install_dir(Path::new(
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\Program Files\Lumen"
        )));
        assert!(!is_safe_dedicated_install_dir(Path::new(
            "//?/GLOBALROOT/Device/HarddiskVolume1/Program Files/Lumen"
        )));
        assert!(!is_safe_dedicated_install_dir(Path::new(
            r"\\?\GlobalRoot\Device\HarddiskVolume1\Lumen"
        )));
    }

    #[test]
    fn test_extract_exe_path_from_uninstall_string_turkish_i() {
        // Turkish capital dotted 'İ' (\u{0130}) expands from 2 bytes to 3 bytes under
        // Unicode .to_lowercase(), which previously panicked when slicing clean[..idx + 4].
        let input = "\"C:\\Users\\\u{0130}stanbul\\AppData\\Local\\Programs\\Lumen\\uninstall.exe\" --silent";
        let extracted = extract_exe_path_from_uninstall_string(input);
        assert_eq!(
            extracted,
            PathBuf::from("C:\\Users\\\u{0130}stanbul\\AppData\\Local\\Programs\\Lumen\\uninstall.exe")
        );

        let upper_ext = "C:\\\u{0130}\u{0130}\u{0130}\\Lumen\\UNINSTALL.EXE /S";
        assert_eq!(
            extract_exe_path_from_uninstall_string(upper_ext),
            PathBuf::from("C:\\\u{0130}\u{0130}\u{0130}\\Lumen\\UNINSTALL.EXE")
        );
    }

    #[test]
    fn test_ensure_dedicated_install_subdir() {
        let base = PathBuf::from("/opt/apps");
        assert_eq!(
            ensure_dedicated_install_subdir(&base),
            base.join(channel_install_dir_name())
        );

        let already_lumen = PathBuf::from("/opt/apps/Lumen");
        assert_eq!(ensure_dedicated_install_subdir(&already_lumen), already_lumen);

        let already_dev = PathBuf::from("/opt/apps/Lumen-dev");
        assert_eq!(ensure_dedicated_install_subdir(&already_dev), already_dev);

        let already_win_dev = PathBuf::from("D:\\Apps\\Lumen Dev");
        assert_eq!(ensure_dedicated_install_subdir(&already_win_dev), already_win_dev);
    }
}
