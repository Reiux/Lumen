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

fn is_older_version(installed: &str, current: &str) -> bool {
    let parse_v = |s: &str| -> Vec<u32> {
        let clean = s.trim_start_matches('v').trim();
        clean
            .split('.')
            .map(|part| {
                part.chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or(0)
            })
            .collect()
    };
    let inst_parts = parse_v(installed);
    let cur_parts = parse_v(current);
    for i in 0..cur_parts.len().max(inst_parts.len()) {
        let inst_p = inst_parts.get(i).copied().unwrap_or(0);
        let cur_p = cur_parts.get(i).copied().unwrap_or(0);
        if inst_p < cur_p {
            return true;
        } else if inst_p > cur_p {
            return false;
        }
    }
    false
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

fn get_legacy_nsis_info() -> Option<InstalledAppInfo> {
    let current_exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .and_then(|d| d.canonicalize().ok());

    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

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
    let current_exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .and_then(|d| d.canonicalize().ok());

    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;

        // Check HKCU uninstall key specifically
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(uninstall_root) = hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall") {
            if let Ok(app_key) = uninstall_root.open_subkey("Lumen") {
                let version = app_key.get_value::<String, _>("DisplayVersion").unwrap_or_default();
                if let Some(p) = extract_dir_from_reg_key(&app_key) {
                    let norm_p = p.to_string_lossy().to_lowercase();
                    if !norm_p.contains("program files") && (p.join("Lumen.exe").is_file() || p.join("lumen.exe").is_file()) {
                        if let Some(ref cur) = current_exe_dir {
                            if let Ok(canon) = p.canonicalize() {
                                if canon == *cur {
                                    return None;
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
        }
    }

    let default_dir = get_default_install_dir();
    if default_dir.join("Lumen.exe").is_file() || default_dir.join("lumen.exe").is_file() {
        if let Some(ref cur) = current_exe_dir {
            if let Ok(canon_default) = default_dir.canonicalize() {
                if canon_default == *cur {
                    return None;
                }
            }
        }
        return Some(InstalledAppInfo {
            dir: default_dir,
            version: String::new(),
            is_nsis: false,
        });
    }
    let _ = current_exe_dir;
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

    if install_dir.is_absolute()
        && (install_dir.join("Lumen.exe").is_file() || install_dir.join("lumen.exe").is_file())
    {
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
        if installed.join("Lumen.exe").is_file() || installed.join("lumen.exe").is_file() {
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
    #[cfg(unix)]
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
    }

    #[cfg(unix)]
    {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(".local").join("share").join("Lumen");
        }
    }

    PathBuf::from("C:\\Program Files\\Lumen")
}

#[tauri::command]
fn get_setup_info() -> SetupInfo {
    let is_uninstall = is_uninstall_invocation();
    let current_version = env!("CARGO_PKG_VERSION").to_string();
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

fn normalize_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy().replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    let has_root = s.starts_with('/');
    for part in s.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            parts.pop();
        } else {
            parts.push(part);
        }
    }
    let res = if has_root {
        format!("/{}", parts.join("/"))
    } else {
        parts.join("/")
    };
    PathBuf::from(res)
}

fn is_safe_dedicated_install_dir(dir: &Path) -> bool {
    let clean = normalize_path(dir);
    let path_str = clean.to_string_lossy();
    let normalized = path_str.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');

    let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        // Must have at least a parent and a folder name (cannot be root like / or C:)
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
        if let Ok(sysroot) = env::var("SystemRoot") {
            let norm_sr = sysroot.replace('\\', "/").to_lowercase();
            let sr_trimmed = norm_sr.trim_end_matches('/');
            let lower_trimmed = trimmed.to_lowercase();
            if lower_trimmed == sr_trimmed || lower_trimmed.starts_with(&format!("{}/", sr_trimmed)) {
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

        let output = Command::new(get_system_powershell())
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

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
             $procs = Get-CimInstance Win32_Process -Filter \"Name = 'Lumen.exe' or Name = 'lumen.exe'\" -ErrorAction SilentlyContinue | Where-Object {{ $_.ProcessId -ne $curPid }}; \
             foreach ($p in $procs) {{ \
                 if ($p.ExecutablePath -and $p.ExecutablePath -ne $curExe) {{ \
                     try {{ Stop-Process -Id $p.ProcessId -Force -ErrorAction SilentlyContinue }} catch {{}} \
                 }} \
             }}",
            current_pid, cur_exe_escaped
        );

        let _ = Command::new(get_system_powershell())
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(CREATE_NO_WINDOW)
            .output();

        std::thread::sleep(std::time::Duration::from_millis(400));
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
                        let _ = Command::new("kill").arg("-9").arg(pid.to_string()).output();
                    }
                }
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
        remove_legacy_nsis_installation();
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

    // Standardize executable name as Lumen.exe
    let main_exe = if dest_path.join("Lumen.exe").is_file() {
        dest_path.join("Lumen.exe")
    } else if dest_path.join("lumen.exe").is_file() {
        let target = dest_path.join("Lumen.exe");
        let _ = fs::rename(dest_path.join("lumen.exe"), &target);
        target
    } else {
        dest_path.join("Lumen.exe")
    };

    // 2. Copy current installer as uninstaller into install dir
    if let Ok(current_exe) = env::current_exe() {
        let uninstaller_dest = dest_path.join("uninstall.exe");
        let _ = fs::copy(&current_exe, &uninstaller_dest);
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

    #[cfg(windows)]
    {
        remove_legacy_nsis_installation();
    }

    let install_dir = if let Some(dir) = get_installed_dir() {
        dir
    } else {
        get_default_install_dir()
    };

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
                let lumen_config_dir = PathBuf::from(home).join(".config").join("lumen");
                let _ = fs::remove_dir_all(lumen_config_dir);
            }
        }
        if install_dir.exists() {
            let _ = fs::remove_file(install_dir.join("lumen"));
            let _ = fs::remove_file(install_dir.join("lumen-cli"));
            let _ = fs::remove_file(install_dir.join("uninstall"));
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
    #[cfg(windows)]
    let main_exe = if dest_path.join("Lumen.exe").is_file() {
        dest_path.join("Lumen.exe")
    } else {
        dest_path.join("lumen.exe")
    };
    #[cfg(not(windows))]
    let main_exe = if dest_path.join("Lumen").is_file() {
        dest_path.join("Lumen")
    } else {
        dest_path.join("lumen")
    };
    if main_exe.is_file() {
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
fn get_system_powershell() -> PathBuf {
    if let Ok(sysroot) = env::var("SystemRoot") {
        let ps = PathBuf::from(sysroot).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
        if ps.is_file() {
            return ps;
        }
    }
    PathBuf::from("powershell.exe")
}

#[cfg(windows)]
fn escape_powershell_single_quote(s: &str) -> String {
    s.replace('\'', "''").replace(['\r', '\n', '\0', ';', '`', '$'], "")
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
    let output = Command::new(get_system_powershell())
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
    key.set_value("DisplayVersion", &env!("CARGO_PKG_VERSION")).map_err(|e| e.to_string())?;
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
fn remove_legacy_nsis_installation() {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    use winreg::enums::*;
    use winreg::RegKey;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let mut legacy_dirs: Vec<PathBuf> = Vec::new();

    // 1. Scan standard Program Files folders
    for pf_var in &["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Ok(pf) = env::var(pf_var) {
            let candidates = [
                PathBuf::from(&pf).join("Reiux").join("Lumen"),
                PathBuf::from(&pf).join("Lumen"),
                PathBuf::from(&pf).join("gotohp"),
            ];
            for p in candidates {
                if p.is_dir() {
                    legacy_dirs.push(p);
                }
            }
        }
    }

    // 2. Scan HKLM, WOW6432Node, and HKCU Uninstall registry keys for legacy Go NSIS paths
    let hives = [
        (HKEY_LOCAL_MACHINE, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKEY_LOCAL_MACHINE, "Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
        (HKEY_CURRENT_USER, "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall"),
    ];

    for (hkey, root_path) in hives {
        let root = RegKey::predef(hkey);
        if let Ok(uninstall_root) = root.open_subkey(root_path) {
            for key_name in uninstall_root.enum_keys().filter_map(|r| r.ok()) {
                let lower_key = key_name.to_lowercase();
                if lower_key == "reiuxlumen" || lower_key == "reiux lumen" || lower_key == "gotohp"
                    || (lower_key == "lumen" && hkey == HKEY_LOCAL_MACHINE)
                {
                    if let Ok(app_key) = uninstall_root.open_subkey(&key_name) {
                        if let Some(p) = extract_dir_from_reg_key(&app_key) {
                            if p.is_dir() {
                                legacy_dirs.push(p);
                            }
                        }
                    }
                }
            }
        }
    }

    // Always include standard default Program Files paths for safety
    if let Ok(pf) = env::var("ProgramFiles") {
        let def_p = PathBuf::from(&pf).join("Reiux").join("Lumen");
        if def_p.is_dir() {
            legacy_dirs.push(def_p);
        }
    }
    if let Ok(pf86) = env::var("ProgramFiles(x86)") {
        let def_p86 = PathBuf::from(&pf86).join("Reiux").join("Lumen");
        if def_p86.is_dir() {
            legacy_dirs.push(def_p86);
        }
    }

    legacy_dirs.sort();
    legacy_dirs.dedup();

    // Build elevated PowerShell script to cleanly uninstall and purge legacy Program Files & HKLM
    let mut dir_list_ps = String::new();
    for d in &legacy_dirs {
        dir_list_ps.push_str(&format!("'{}', ", escape_powershell_single_quote(&d.to_string_lossy())));
    }
    let dir_list_ps = dir_list_ps.trim_end_matches(", ");

    let script = format!(
        r#"$dirs = @({dirs});
foreach ($d in $dirs) {{
    if (Test-Path -LiteralPath $d) {{
        $uninst = Join-Path $d 'uninstall.exe';
        if (Test-Path -LiteralPath $uninst) {{
            $sysCmd = Join-Path $env:SystemRoot 'System32\cmd.exe';
            Start-Process -FilePath $sysCmd -ArgumentList ('/c `"' + $uninst + '`" /S _?=' + $d) -Wait -WindowStyle Hidden -ErrorAction SilentlyContinue;
            Start-Sleep -Milliseconds 800;
        }}
        Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {{ $_.ExecutablePath -and $_.ExecutablePath.StartsWith($d, [System.StringComparison]::OrdinalIgnoreCase) }} | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }};
        for ($i = 0; $i -lt 5; $i++) {{
            Remove-Item -LiteralPath $d -Recurse -Force -ErrorAction SilentlyContinue;
            if (-not (Test-Path -LiteralPath $d)) {{ break }}
            Start-Sleep -Milliseconds 400;
        }}
    }}
}}

$reiuxRoots = @('C:\Program Files\Reiux', 'C:\Program Files (x86)\Reiux');
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

$pdata = $env:ProgramData;
if ($pdata) {{
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\Lumen.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\gotohp.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\Reiux\Lumen.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\Reiux" -Recurse -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pdata\Microsoft\Windows\Start Menu\Programs\Lumen" -Recurse -Force -ErrorAction SilentlyContinue;
}}
$pub = $env:PUBLIC;
if ($pub) {{
    Remove-Item -LiteralPath "$pub\Desktop\Lumen.lnk" -Force -ErrorAction SilentlyContinue;
    Remove-Item -LiteralPath "$pub\Desktop\gotohp.lnk" -Force -ErrorAction SilentlyContinue;
}}
"#,
        dirs = dir_list_ps
    );

    let random_id: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let temp_ps1 = env::temp_dir().join(format!("lumen_legacy_cleanup_{:016x}.ps1", random_id));
    if fs::write(&temp_ps1, script).is_ok() {
        let ps_bin = escape_powershell_single_quote(&get_system_powershell().to_string_lossy());
        let ps_arg = format!(
            "Start-Process '{}' -WindowStyle Hidden -ArgumentList '-NoProfile -NonInteractive -ExecutionPolicy Bypass -WindowStyle Hidden -File \"{}\"' -Verb RunAs -Wait",
            ps_bin,
            escape_powershell_single_quote(&temp_ps1.to_string_lossy())
        );
        let _ = Command::new(get_system_powershell())
            .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &ps_arg])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = fs::remove_file(&temp_ps1);
    }

    // Clean up user-scoped shortcuts and HKCU keys
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

    let _ = Command::new(get_system_powershell())
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
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
    fn test_is_older_version() {
        assert!(is_older_version("1.0.0", "1.1.0"));
        assert!(is_older_version("v1.0.0", "1.1.0"));
        assert!(is_older_version("1.0.9", "1.1.0"));
        assert!(is_older_version("0.9.0", "1.0.0"));
        assert!(!is_older_version("1.1.0", "1.1.0"));
        assert!(!is_older_version("1.2.0", "1.1.0"));
        assert!(!is_older_version("2.0.0", "1.1.0"));
    }
}
