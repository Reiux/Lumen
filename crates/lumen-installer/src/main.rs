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
    pub default_path: String,
    pub config_path: String,
}

fn get_installed_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(key) = hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Lumen") {
            if let Ok(loc) = key.get_value::<String, _>("InstallLocation") {
                let p = PathBuf::from(loc);
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    let default_dir = get_default_install_dir();
    if default_dir.join("Lumen.exe").exists() || default_dir.join("lumen.exe").exists() {
        return Some(default_dir);
    }
    None
}

fn check_if_installed(install_dir: &Path) -> bool {
    if install_dir.join("Lumen.exe").exists() || install_dir.join("lumen.exe").exists() {
        return true;
    }
    #[cfg(windows)]
    {
        use winreg::enums::*;
        use winreg::RegKey;
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if hkcu.open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Lumen").is_ok() {
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
        return "~/.config/lumen".to_string();
    }
}

fn is_uninstall_invocation() -> bool {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--uninstall" || a == "-u") {
        return true;
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(stem) = exe.file_stem() {
            if stem.to_string_lossy().to_lowercase().contains("uninstall") {
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
    let default_dir = get_default_install_dir();
    let installed_dir = get_installed_dir();
    let is_installed = installed_dir.is_some() || check_if_installed(&default_dir);
    let default_path = if is_uninstall || is_installed {
        if let Some(dir) = installed_dir {
            dir.to_string_lossy().to_string()
        } else if let Ok(exe) = env::current_exe() {
            exe.parent().unwrap_or_else(|| Path::new(".")).to_string_lossy().to_string()
        } else {
            default_dir.to_string_lossy().to_string()
        }
    } else {
        default_dir.to_string_lossy().to_string()
    };
    let config_path = get_config_display_path();

    SetupInfo {
        is_uninstall,
        is_installed,
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

fn is_safe_dedicated_install_dir(dir: &Path) -> bool {
    let path_str = dir.to_string_lossy();
    let normalized = path_str.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');

    let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        // Must have at least a parent and a folder name (cannot be root like / or C:)
        return false;
    }

    let folder_name = parts[parts.len() - 1];
    if !folder_name.eq_ignore_ascii_case("lumen") {
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
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        for exe_name in &["Lumen.exe", "lumen.exe"] {
            let output = Command::new("tasklist")
                .args(["/FI", &format!("IMAGENAME eq {}", exe_name), "/FO", "CSV", "/NH"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            if let Ok(out) = output {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        let name = parts[0].trim().trim_matches('"');
                        let pid_str = parts[1].trim().trim_matches('"');
                        if name.eq_ignore_ascii_case(exe_name) {
                            if let Ok(pid) = pid_str.parse::<u32>() {
                                if pid != current_pid {
                                    return true;
                                }
                            }
                        }
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
        let pid_filter = format!("PID ne {}", current_pid);
        for exe_name in &["Lumen.exe", "lumen.exe"] {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", exe_name, "/FI", &pid_filter])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
        }
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
    let dest_path = PathBuf::from(&target_dir);
    fs::create_dir_all(&dest_path).map_err(|e| format!("Failed to create install dir: {}", e))?;

    // Terminate any running Lumen instances before unpacking
    let _ = kill_running_app();

    // 1. Extract embedded payload
    let decoder = GzDecoder::new(Cursor::new(EMBEDDED_PAYLOAD));
    let mut archive = Archive::new(decoder);
    archive
        .unpack(&dest_path)
        .map_err(|e| format!("Failed to unpack payload: {}", e))?;

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
    if launch_after && main_exe.exists() {
        let _ = std::process::Command::new(&main_exe).spawn();
    }

    Ok(())
}

#[tauri::command]
async fn start_uninstall(purge_config: bool) -> Result<(), String> {
    // 1. Terminate running app instances first so files are not locked
    let _ = kill_running_app();

    let install_dir = if let Some(dir) = get_installed_dir() {
        dir
    } else if let Ok(exe) = env::current_exe() {
        let parent = exe.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
        if parent.join("Lumen.exe").exists() || parent.join("lumen.exe").exists() {
            parent
        } else {
            get_default_install_dir()
        }
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
    let main_exe = if dest_path.join("Lumen.exe").is_file() {
        dest_path.join("Lumen.exe")
    } else {
        dest_path.join("lumen.exe")
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
fn escape_powershell_single_quote(s: &str) -> String {
    s.replace('\'', "''").replace(['\r', '\n', '\0'], "")
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
    let output = Command::new("powershell")
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
fn unregister_windows_uninstall() -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Lumen");
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

    let _ = Command::new("powershell")
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
        assert!(is_safe_dedicated_install_dir(Path::new("/home/user/.local/share/Lumen")));
        assert!(is_safe_dedicated_install_dir(Path::new("C:\\Users\\User\\AppData\\Local\\Programs\\Lumen")));
    }
}
