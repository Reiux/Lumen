//! Chromium DevTools Protocol (CDP) client for automated Google Sign-In

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

static BROWSER_CLOSE_FAILED: AtomicBool = AtomicBool::new(false);

pub fn set_browser_close_failed(failed: bool) {
    BROWSER_CLOSE_FAILED.store(failed, Ordering::SeqCst);
}

pub fn was_browser_close_failed() -> bool {
    BROWSER_CLOSE_FAILED.load(Ordering::SeqCst)
}

pub fn find_chromium_browser() -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let user_flatpak_brave = format!("{}/.local/share/flatpak/exports/bin/com.brave.Browser", home);
    let user_flatpak_chromium = format!("{}/.local/share/flatpak/exports/bin/org.chromium.Chromium", home);
    let user_flatpak_chrome = format!("{}/.local/share/flatpak/exports/bin/com.google.Chrome", home);
    let user_flatpak_edge = format!("{}/.local/share/flatpak/exports/bin/com.microsoft.Edge", home);

    let candidates = [
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "chromium-browser",
        "brave-browser",
        "microsoft-edge-stable",
        "com.brave.Browser",
        "org.chromium.Chromium",
        "com.google.Chrome",
        "com.microsoft.Edge",
        "/var/lib/flatpak/exports/bin/com.brave.Browser",
        "/var/lib/flatpak/exports/bin/org.chromium.Chromium",
        "/var/lib/flatpak/exports/bin/com.google.Chrome",
        "/var/lib/flatpak/exports/bin/com.microsoft.Edge",
        user_flatpak_brave.as_str(),
        user_flatpak_chromium.as_str(),
        user_flatpak_chrome.as_str(),
        user_flatpak_edge.as_str(),
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/usr/bin/brave-browser",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
        "C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe",
        "C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe",
    ];

    for candidate in &candidates {
        let p = Path::new(candidate);
        if p.is_absolute() && p.exists() {
            return Some(p.to_path_buf());
        }
        if let Ok(which) = which_browser(candidate) {
            return Some(which);
        }
    }
    None
}

pub fn get_supported_browser_name() -> String {
    if let Some(path) = find_chromium_browser() {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
        if name.contains("brave") {
            return "Brave".to_string();
        } else if name.contains("edge") {
            return "Microsoft Edge".to_string();
        } else if name.contains("chromium") {
            return "Chromium".to_string();
        } else {
            return "Google Chrome".to_string();
        }
    }
    "Google Chrome".to_string()
}

fn which_browser(name: &str) -> Result<PathBuf> {
    let output = Command::new("which").arg(name).output()?;
    if output.status.success() {
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path_str.is_empty() {
            return Ok(PathBuf::from(path_str));
        }
    }
    bail!("not found")
}

#[derive(Serialize)]
struct CdpRequest {
    id: u64,
    method: &'static str,
}

#[derive(Deserialize)]
struct CdpCookie {
    name: String,
    value: String,
    domain: String,
}

#[derive(Deserialize)]
struct StorageGetCookiesResult {
    cookies: Vec<CdpCookie>,
}

#[derive(Deserialize)]
struct CdpResponse {
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<StorageGetCookiesResult>,
}

pub struct CdpSession {
    child: Child,
    #[allow(dead_code)]
    user_data_dir: tempfile::TempDir,
    ws_url: String,
}

#[allow(dead_code)]
pub type ActiveSession = Arc<Mutex<Option<Child>>>;

impl CdpSession {
    pub async fn launch() -> Result<Self> {
        let browser_path = find_chromium_browser()
            .context("No supported Chromium or Google Chrome browser found on this system.")?;

        let user_data_dir = tempfile::tempdir().context("Failed to create temporary browser profile")?;

        let child = Command::new(&browser_path)
            .arg(format!("--user-data-dir={}", user_data_dir.path().display()))
            .arg("--remote-debugging-port=0")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("https://accounts.google.com/embedded/setup/v2/android")
            .spawn()
            .context("Failed to spawn browser process")?;

        let port_file = user_data_dir.path().join("DevToolsActivePort");

        let mut port_content = String::new();
        for _ in 0..40 {
            if port_file.exists() {
                if let Ok(c) = fs::read_to_string(&port_file) {
                    if !c.trim().is_empty() {
                        port_content = c;
                        break;
                    }
                }
            }
            sleep(Duration::from_millis(250)).await;
        }

        if port_content.is_empty() {
            bail!("Timed out waiting for Chromium remote debugging port to become active.");
        }

        let lines: Vec<&str> = port_content.lines().collect();
        if lines.len() < 2 {
            bail!("Invalid DevToolsActivePort file format from browser");
        }

        let port = lines[0].trim();
        let ws_path = lines[1].trim();
        let ws_url = format!("ws://127.0.0.1:{}{}", port, ws_path);

        Ok(Self {
            child,
            user_data_dir,
            ws_url,
        })
    }

    pub async fn wait_for_google_auth_cookies(&mut self) -> Result<String> {
        let (mut ws_stream, _) = connect_async(&self.ws_url).await
            .context("Failed to connect to Chromium CDP WebSocket")?;

        for _ in 0..180 {
            // Check if user closed the browser early
            match self.child.try_wait() {
                Ok(Some(status)) => {
                    bail!("Browser was closed early before sign-in completed (exit status: {})", status);
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::debug!("Error polling browser child status: {}", e);
                }
            }

            // Query cookies
            let req = serde_json::to_string(&CdpRequest {
                id: 1,
                method: "Storage.getCookies",
            })?;

            if ws_stream.send(Message::Text(req.into())).await.is_err() {
                bail!("Browser connection was closed before sign-in completed");
            }

            // Read response
            if let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(Duration::from_millis(800), ws_stream.next()).await {
                if let Ok(resp) = serde_json::from_str::<CdpResponse>(&text) {
                    if let Some(res) = resp.result {
                        for c in &res.cookies {
                            if c.domain.contains("google") && c.name == "oauth_token" && !c.value.is_empty() {
                                return Ok(c.value.clone());
                            }
                        }
                    }
                }
            }

            sleep(Duration::from_millis(1000)).await;
        }

        bail!("Browser sign-in timed out. Please try again or paste the oauth_token manually.")
    }

    pub fn close(&mut self) -> bool {
        let mut success = true;
        if let Err(e) = self.child.kill() {
            tracing::warn!("Failed to terminate browser process: {}", e);
            success = false;
        }
        let _ = self.child.wait();
        if !success {
            set_browser_close_failed(true);
        }
        success
    }
}

impl Drop for CdpSession {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_chromium_browser_detection() {
        let browser = find_chromium_browser();
        assert!(browser.is_some(), "Expected a Chromium-compatible browser to be detected");
        let name = get_supported_browser_name();
        assert!(!name.is_empty());
        println!("Detected browser: {:?} ({})", browser, name);
    }
}
