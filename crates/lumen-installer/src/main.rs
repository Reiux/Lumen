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

#[derive(Serialize)]
pub struct SetupInfo {
    pub is_uninstall: bool,
    pub is_installed: bool,
    pub is_update: bool,
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
        &["Lumen.exe", "lumen.exe"]
    }
    #[cfg(not(windows))]
    {
        &["Lumen", "lumen"]
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
        let clean = uninst_str.trim().trim_matches('"');
        let exe_part = if let Some(idx) = clean.to_lowercase().find(".exe") {
            &clean[..idx + 4]
        } else {
            clean
        };
        let p = PathBuf::from(exe_part.trim().trim_matches('"'));
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
    let app_key = uninstall_root.open_subkey("Lumen").ok()?;
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
                    if candidate.is_dir() {
                        let has_app = candidate.join("Lumen.exe").is_file()
                            || candidate.join("lumen.exe").is_file()
                            || candidate.join("uninstall.exe").is_file()
                            || candidate.join("wails.json").is_file()
                            || candidate.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false);
                        if has_app {
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
        dir.join("Lumen.exe").is_file()
            || dir.join("lumen.exe").is_file()
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
            let has_bin = p.join("Lumen.exe").is_file() || p.join("lumen.exe").is_file();
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
            return format!("{}\\Lumen", appdata);
        }
        return "%APPDATA%\\Lumen".to_string();
    }
    #[cfg(target_os = "macos")]
    {
        "~/Library/Application Support/lumen".to_string()
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "~/.config/lumen".to_string()
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
            return PathBuf::from(local_appdata).join("Programs").join("Lumen");
        }
        PathBuf::from("C:\\Program Files\\Lumen")
    }

    #[cfg(unix)]
    {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(".local").join("share").join("Lumen");
        }
        env::temp_dir().join("Lumen")
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
            let res = path.map(|p| p.to_string());
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
    let raw = strip_windows_verbatim_prefix(&dir.to_string_lossy());
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
    if !folder_name.eq_ignore_ascii_case("lumen") && !folder_name.eq_ignore_ascii_case("gotohp") {
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

#[tauri::command]
fn is_app_running() -> bool {
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

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let cur_exe_escaped = escape_powershell_single_quote(&current_exe_str);
        let script = format!(
            "$curPid = {}; \
             $curExe = '{}'; \
             $procs = Get-CimInstance Win32_Process -Filter \"Name = 'Lumen.exe' or Name = 'lumen.exe'\" -ErrorAction SilentlyContinue | Where-Object {{ $_.ProcessId -ne $curPid -and $_.ExecutablePath -and $_.ExecutablePath.ToLower() -ne $curExe.ToLower() }}; \
             foreach ($p in $procs) {{ \
                 Write-Output \"$($p.ProcessId)|$($p.ExecutablePath)\" \
             }}",
            current_pid, cur_exe_escaped
        );

        let output = if let Some(ps_path) = get_system_powershell() {
            Command::new(ps_path)
                .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
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
        let output = Command::new("pgrep").arg("-x").arg("lumen").output();
        if let Ok(out) = output {
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
    false
}

#[tauri::command]
fn kill_running_app() -> Result<(), String> {
    let current_pid = std::process::id();

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let current_exe_str = env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let cur_exe_escaped = escape_powershell_single_quote(&current_exe_str);
        let script = format!(
            "$curPid = {}; \
             $curExe = '{}'; \
             $procs = Get-CimInstance Win32_Process -Filter \"Name = 'Lumen.exe' or Name = 'lumen.exe' or Name = 'lumen-cli.exe'\" -ErrorAction SilentlyContinue | Where-Object {{ $_.ProcessId -ne $curPid }}; \
             foreach ($p in $procs) {{ \
                 if ($p.ExecutablePath -and $p.ExecutablePath -ne $curExe) {{ \
                     try {{ Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue }} catch {{}} \
                 }} \
             }}",
            current_pid, cur_exe_escaped
        );

        if let Some(ps_path) = get_system_powershell() {
            let _ = Command::new(ps_path)
                .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }

        for _ in 0..15 {
            if !is_app_running() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    #[cfg(not(windows))]
    {
        use std::process::Command;
        let mut target_pids = Vec::new();
        for name in &["lumen", "lumen-cli"] {
            if let Ok(out) = Command::new("pgrep").arg("-x").arg(name).output() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    if let Ok(pid) = line.trim().parse::<u32>() {
                        if pid != current_pid {
                            target_pids.push(pid);
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
    let dest_path = if target_dir.trim().is_empty() {
        get_default_install_dir()
    } else {
        PathBuf::from(&target_dir)
    };
    if !is_safe_dedicated_install_dir(&dest_path) {
        return Err(format!(
            "Target install directory '{}' is not a dedicated, safe Lumen directory.",
            dest_path.display()
        ));
    }
    fs::create_dir_all(&dest_path).map_err(|e| format!("Failed to create install dir: {}", e))?;

    // Terminate any running Lumen instances before unpacking
    let _ = kill_running_app();

    #[cfg(windows)]
    {
        remove_legacy_nsis_installation(Some(&dest_path));
    }

    // 1. Extract embedded payload securely (preventing Tar Slip and symlink escapes)
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
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            if let Ok(Some(link_target)) = entry.link_name() {
                for comp in link_target.components() {
                    match comp {
                        std::path::Component::ParentDir | std::path::Component::Prefix(_) | std::path::Component::RootDir => {
                            return Err(format!("Unsafe link target in payload entry: {}", link_target.display()));
                        }
                        _ => {}
                    }
                }
            }
        }

        entry.unpack_in(&dest_path).map_err(|e| format!("Failed to unpack {}: {}", entry_path.display(), e))?;
    }

    // Standardize executable name on Windows and resolve primary app binary per OS
    #[cfg(windows)]
    let main_exe = if dest_path.join("Lumen.exe").is_file() {
        dest_path.join("Lumen.exe")
    } else if dest_path.join("lumen.exe").is_file() {
        let target = dest_path.join("Lumen.exe");
        let _ = fs::rename(dest_path.join("lumen.exe"), &target);
        target
    } else {
        dest_path.join("Lumen.exe")
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
            let lnk_path = start_menu.join("Lumen.lnk");
            let _ = create_windows_shortcut(&main_exe, &lnk_path);
        }

        // Desktop Shortcut
        if create_desktop_shortcut {
            if let Ok(userprofile) = env::var("USERPROFILE") {
                let desktop_dir = PathBuf::from(userprofile).join("Desktop");
                let _ = fs::create_dir_all(&desktop_dir);
                let desktop_lnk = desktop_dir.join("Lumen.lnk");
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
                let _ = run_key.set_value("Lumen", &exe_str);
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

#[tauri::command]
async fn start_uninstall(purge_config: bool) -> Result<(), String> {
    // 1. Terminate running app instances first so files are not locked
    let _ = kill_running_app();

    let modern_info = get_modern_installed_info_with_opts(true);

    #[cfg(windows)]
    {
        remove_legacy_nsis_installation(modern_info.as_ref().map(|m| m.dir.as_path()));
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
                .join("Lumen.lnk");
            let _ = fs::remove_file(start_menu_lnk);
        }

        // 3. Remove Desktop shortcut
        if let Ok(userprofile) = env::var("USERPROFILE") {
            let desktop_lnk = PathBuf::from(userprofile).join("Desktop").join("Lumen.lnk");
            let _ = fs::remove_file(desktop_lnk);
        }

        // 4. Remove Run on Startup registry entry
        {
            use winreg::enums::*;
            use winreg::RegKey;
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            if let Ok(run_key) = hkcu.open_subkey_with_flags("Software\\Microsoft\\Windows\\CurrentVersion\\Run", KEY_SET_VALUE) {
                let _ = run_key.delete_value("Lumen");
            }
        }

        // 5. Remove Windows Registry Uninstall key
        let _ = unregister_windows_uninstall();

        // 6. Optional: Purge configuration & local cache
        if purge_config {
            if let Ok(appdata) = env::var("APPDATA") {
                let lumen_config_dir = PathBuf::from(&appdata).join("Lumen");
                let _ = fs::remove_dir_all(lumen_config_dir);
                let legacy_dir = PathBuf::from(&appdata).join("lumen");
                if legacy_dir.exists() {
                    let _ = fs::remove_dir_all(legacy_dir);
                }
            }
            if let Ok(userprofile) = env::var("USERPROFILE") {
                let lumen_config_dir = PathBuf::from(userprofile).join(".config").join("lumen");
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
                let _ = fs::remove_file(install_dir.join("Lumen.exe"));
                let _ = fs::remove_file(install_dir.join("lumen.exe"));
                let _ = fs::remove_file(install_dir.join("lumen-cli.exe"));
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
            let _ = fs::remove_file(install_dir.join("Lumen.exe"));
            let _ = fs::remove_file(install_dir.join("lumen.exe"));
            let _ = fs::remove_file(install_dir.join("lumen-cli.exe"));
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
                        .join("lumen");
                    let _ = fs::remove_dir_all(mac_config_dir);
                }
                let lumen_config_dir = PathBuf::from(home).join(".config").join("lumen");
                let _ = fs::remove_dir_all(lumen_config_dir);
            }
        }
        if install_dir.exists() {
            let _ = fs::remove_file(install_dir.join("Lumen"));
            let _ = fs::remove_file(install_dir.join("lumen"));
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

#[cfg(windows)]
fn escape_powershell_single_quote(s: &str) -> String {
    s.replace('\'', "''").replace(['\r', '\n', '\0'], "")
}

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
fn create_windows_shortcut(target_exe: &Path, shortcut_path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    let target = escape_powershell_single_quote(&target_exe.to_string_lossy());
    let shortcut = escape_powershell_single_quote(&shortcut_path.to_string_lossy());
    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut('{}'); $s.TargetPath = '{}'; $s.Save()",
        shortcut, target
    );
    let ps_exe = get_system_powershell()
        .ok_or_else(|| "System PowerShell executable not found".to_string())?;
    let output = Command::new(&ps_exe)
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
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
    let (key, _) = hkcu
        .create_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Lumen")
        .map_err(|e| e.to_string())?;

    key.set_value("DisplayName", &"Lumen").map_err(|e| e.to_string())?;
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

    Ok(())
}

#[cfg(windows)]
fn remove_legacy_nsis_installation(modern_dir: Option<&Path>) {
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

    let common_appdata = PathBuf::from(format!(
        r"{}\ProgramData",
        get_os_windows_dir()
            .to_string_lossy()
            .chars()
            .take(2)
            .collect::<String>()
    ));
    let start_menu_programs = common_appdata
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    let has_public_legacy = start_menu_programs.join("gotohp.lnk").is_file()
        || start_menu_programs.join("Reiux").join("Lumen.lnk").is_file()
        || start_menu_programs.join("Lumen.lnk").is_file();

    if legacy_dirs.is_empty() && !has_hklm_legacy_registry && !has_public_legacy {
        cleanup_user_legacy_shortcuts_and_hkcu();
        return;
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
        r#"$dirs = @({dirs});
foreach ($d in $dirs) {{
    if (Test-Path -LiteralPath $d) {{
        $item = Get-Item -LiteralPath $d -Force -ErrorAction SilentlyContinue;
        if ($item -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {{
            [IO.Directory]::Delete($d) | Out-Null;
            continue;
        }}
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {{ $_.ExecutablePath -and $_.ExecutablePath.StartsWith($d, [System.StringComparison]::OrdinalIgnoreCase) }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }};
        for ($i = 0; $i -lt 5; $i++) {{
            Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue;
            if (-not (Test-Path -LiteralPath $d)) {{ break }}
            Start-Sleep -Milliseconds 400;
        }}
    }}
}}

$reiuxRoots = @({reiux_roots});
foreach ($r in $reiuxRoots) {{
    if (Test-Path -LiteralPath $r) {{
        $items = Get-ChildItem -LiteralPath $r -ErrorAction SilentlyContinue;
        if (-not $items -or $items.Count -eq 0) {{
            Remove-Item -LiteralPath $r -Recurse -Force -ErrorAction SilentlyContinue;
        }}
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
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\Lumen.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\gotohp.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\Reiux\Lumen.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\Reiux" -Recurse -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\Lumen" -Recurse -Force -ErrorAction SilentlyContinue;
}}
$pubDesktop = [Environment]::GetFolderPath('CommonDesktopDirectory');
if ($pubDesktop) {{
    Remove-Item -LiteralPath "$pubDesktop\Lumen.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pubDesktop\gotohp.lnk" -Force -ErrorAction SilentlyContinue;
}}
"#,
        dirs = dir_list_ps,
        reiux_roots = reiux_roots_ps
    );

    // Encode script to UTF-16LE Base64 for PowerShell -EncodedCommand to avoid writing a temp script
    let utf16_bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let b64_encoded = encode_base64(&utf16_bytes);

    if let Some(ps_path) = get_system_powershell() {
        let ps_bin = escape_powershell_single_quote(&ps_path.to_string_lossy());
        let ps_arg = format!(
            "Start-Process '{}' -WindowStyle Hidden -ArgumentList '-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -EncodedCommand {}' -Verb RunAs -Wait",
            ps_bin,
            b64_encoded
        );
        let _ = Command::new(&ps_path)
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &ps_arg])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    cleanup_user_legacy_shortcuts_and_hkcu();
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
        let _ = fs::remove_dir_all(user_start.join("Reiux"));
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
    let _ = hkcu.delete_subkey_all("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Lumen");
    let _ = hkcu.delete_subkey_all("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ReiuxLumen");
    let _ = hkcu.delete_subkey_all("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\gotohp");

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
    let target = escape_powershell_single_quote(&target_path.to_string_lossy());

    let local_installer = env::var("LOCALAPPDATA")
        .map(|l| escape_powershell_single_quote(&PathBuf::from(l).join("com.reiux.lumen.installer").to_string_lossy()))
        .unwrap_or_default();

    let remove_local_cmd = if !local_installer.is_empty() {
        format!("Remove-Item -LiteralPath '{}' -Recurse -Force -ErrorAction SilentlyContinue; ", local_installer)
    } else {
        String::new()
    };

    let script = format!(
        "for ($i = 0; $i -lt 30; $i++) {{ \
            Start-Sleep -Seconds 1; \
            Remove-Item -LiteralPath '{}' -Recurse -Force -ErrorAction SilentlyContinue; \
            {} \
            if (-not (Test-Path -LiteralPath '{}')) {{ break }} \
        }}",
        target, remove_local_cmd, target
    );

    if let Some(ps_path) = get_system_powershell() {
        let _ = Command::new(ps_path)
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
        assert!(is_safe_dedicated_install_dir(Path::new("C:\\Users\\User\\AppData\\Local\\Programs\\Lumen")));
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
}
