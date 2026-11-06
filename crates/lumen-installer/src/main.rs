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
    pub default_path: String,
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
    let default_path = if is_uninstall {
        if let Ok(exe) = env::current_exe() {
            exe.parent().unwrap_or_else(|| Path::new(".")).to_string_lossy().to_string()
        } else {
            get_default_install_dir().to_string_lossy().to_string()
        }
    } else {
        get_default_install_dir().to_string_lossy().to_string()
    };

    SetupInfo {
        is_uninstall,
        default_path,
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

#[tauri::command]
async fn start_install(
    target_dir: String,
    create_desktop_shortcut: bool,
    launch_after: bool,
) -> Result<(), String> {
    #[cfg(not(windows))]
    let _ = create_desktop_shortcut;
    let dest_path = PathBuf::from(&target_dir);
    fs::create_dir_all(&dest_path).map_err(|e| format!("Failed to create install dir: {}", e))?;

    // 1. Extract embedded payload
    let decoder = GzDecoder::new(Cursor::new(EMBEDDED_PAYLOAD));
    let mut archive = Archive::new(decoder);
    archive
        .unpack(&dest_path)
        .map_err(|e| format!("Failed to unpack payload: {}", e))?;

    let main_exe = dest_path.join("lumen.exe");

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
            let lnk_path = start_menu.join("Lumen.lnk");
            let _ = create_windows_shortcut(&main_exe, &lnk_path);
        }

        // Desktop Shortcut
        if create_desktop_shortcut {
            if let Ok(userprofile) = env::var("USERPROFILE") {
                let desktop_lnk = PathBuf::from(userprofile).join("Desktop").join("Lumen.lnk");
                let _ = create_windows_shortcut(&main_exe, &desktop_lnk);
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
    let install_dir = if let Ok(exe) = env::current_exe() {
        exe.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
    } else {
        get_default_install_dir()
    };

    #[cfg(windows)]
    {
        // 1. Remove Start Menu shortcut
        if let Ok(appdata) = env::var("APPDATA") {
            let start_menu_lnk = PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Lumen.lnk");
            let _ = fs::remove_file(start_menu_lnk);
        }

        // 2. Remove Desktop shortcut
        if let Ok(userprofile) = env::var("USERPROFILE") {
            let desktop_lnk = PathBuf::from(userprofile).join("Desktop").join("Lumen.lnk");
            let _ = fs::remove_file(desktop_lnk);
        }

        // 3. Remove Windows Registry Uninstall key
        let _ = unregister_windows_uninstall();

        // 4. Optional: Purge configuration & local cache
        if purge_config {
            if let Ok(userprofile) = env::var("USERPROFILE") {
                let lumen_config_dir = PathBuf::from(userprofile).join(".config").join("lumen");
                let _ = fs::remove_dir_all(lumen_config_dir);
            }
        }

        // 5. Schedule self-deletion of install directory
        schedule_self_deletion(&install_dir);
    }

    #[cfg(not(windows))]
    {
        let _ = purge_config;
        let _ = install_dir;
    }

    Ok(())
}

#[tauri::command]
fn launch_app(target_dir: String) -> Result<(), String> {
    let dest_path = PathBuf::from(&target_dir);
    let main_exe = dest_path.join("lumen.exe");
    if main_exe.exists() {
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

#[cfg(windows)]
fn create_windows_shortcut(target_exe: &Path, shortcut_path: &Path) -> Result<(), String> {
    use std::process::Command;
    let target = target_exe.to_string_lossy();
    let shortcut = shortcut_path.to_string_lossy();
    let script = format!(
        "$ws = New-Object -ComObject WScript.Shell; $s = $ws.CreateShortcut('{}'); $s.TargetPath = '{}'; $s.Save()",
        shortcut, target
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
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
    key.set_value("DisplayVersion", &"1.1.0").map_err(|e| e.to_string())?;
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
fn schedule_self_deletion(install_dir: &Path) {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;
    const DETACHED_PROCESS: u32 = 0x00000008;

    let dir = install_dir.to_string_lossy();
    let cmd = format!("timeout /t 2 /nobreak > NUL & rmdir /s /q \"{}\"", dir);
    let _ = Command::new("cmd.exe")
        .args(["/C", &cmd])
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn();
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            get_setup_info,
            pick_directory,
            start_install,
            start_uninstall,
            launch_app,
            minimize_installer,
            close_installer,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lumen installer");
}
