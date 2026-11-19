//! Built-in Tauri WebView sign-in for Google EmbeddedSetup.
//!
//! - On Windows (WebView2) and macOS (WKWebView): embeds an isolated incognito
//!   child `Webview` directly inside Lumen's main window via `Window::add_child`,
//!   positioned to match the M3E sign-in dialog body and live-resized as the window moves.
//! - On Linux (WebKitGTK): opens a centered isolated incognito `WebviewWindow`
//!   (since `tauri-runtime-wry` packs GTK child webviews into `GtkBox` rather than
//!   overlaying arbitrary coordinates).
//!
//! In both modes, Lumen polls the native WebView cookie store off-thread and inspects
//! HTTPS `google.com` / `*.google.com` navigations for the `Secure` `oauth_token`.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Deserialize;
use tauri::webview::Cookie;
use tauri::{
    AppHandle, LogicalPosition, LogicalSize, Manager, Url, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use tokio::time::sleep;

use crate::cdp::is_google_auth_cookie_domain;

const EMBEDDED_SETUP_URL: &str = "https://accounts.google.com/embedded/setup/v2/android";
const SIGNIN_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

#[cfg(any(target_os = "macos", target_os = "linux"))]
const WEBKIT_SAFARI_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Safari/605.1.15";

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
static SIGNIN_CANCELLED: AtomicBool = AtomicBool::new(false);
static ACTIVE_SIGNIN_LABEL: Mutex<Option<String>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct SignInBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl SignInBounds {
    fn clamped(self) -> (f64, f64, f64, f64) {
        let x = if self.x.is_finite() { self.x.max(0.0) } else { 0.0 };
        let y = if self.y.is_finite() { self.y.max(0.0) } else { 0.0 };
        let w = if self.width.is_finite() {
            self.width.max(300.0)
        } else {
            460.0
        };
        let h = if self.height.is_finite() {
            self.height.max(300.0)
        } else {
            540.0
        };
        (x, y, w, h)
    }
}

/// Whether the current OS supports inline child-WebView embedding inside the main M3E dialog.
pub fn is_inline_embedded_supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos"))
}

fn close_signin_target(app: &AppHandle, label: &str) {
    if let Some(wv) = app.get_webview(label) {
        let _ = wv.close();
    }
    if let Some(win) = app.get_webview_window(label) {
        if win.close().is_err() {
            let _ = win.destroy();
        }
    }
}

pub fn cancel_active_sign_in(app: &AppHandle) {
    SIGNIN_CANCELLED.store(true, Ordering::SeqCst);
    let label = ACTIVE_SIGNIN_LABEL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(label) = label {
        close_signin_target(app, &label);
    }
}

/// Resize/reposition the active inline embedded sign-in child WebView to stay aligned
/// with the M3E dialog container when the main window resizes.
pub fn update_embedded_bounds(app: &AppHandle, bounds: SignInBounds) {
    let label = ACTIVE_SIGNIN_LABEL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let Some(label) = label else {
        return;
    };
    if let Some(wv) = app.get_webview(&label) {
        let (x, y, w, h) = bounds.clamped();
        let rect = tauri::Rect {
            position: LogicalPosition::new(x, y).into(),
            size: LogicalSize::new(w, h).into(),
        };
        let _ = wv.set_bounds(rect);
    }
}

/// Extract a valid `oauth_token` from WebView cookies, requiring:
/// - `secure == true`
/// - normalized domain is `google.com` or `*.google.com`
/// - cookie name is `oauth_token` (case-insensitive)
/// - non-empty value
pub(crate) fn extract_oauth_token_from_cookies(cookies: &[Cookie<'_>]) -> Option<String> {
    for cookie in cookies {
        let is_secure = cookie.secure().unwrap_or(false);
        let domain = cookie.domain().unwrap_or("");
        if is_secure
            && is_google_auth_cookie_domain(domain)
            && cookie.name().eq_ignore_ascii_case("oauth_token")
        {
            let value = cookie.value().trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Extract an `oauth_token` parameter from an HTTPS `google.com` / `*.google.com`
/// redirect URL's query string or fragment, if present.
pub(crate) fn extract_oauth_token_from_url(url: &Url) -> Option<String> {
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?;
    if !is_google_auth_cookie_domain(host) {
        return None;
    }

    for (k, v) in url.query_pairs() {
        if k.eq_ignore_ascii_case("oauth_token") {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    if let Some(fragment) = url.fragment() {
        for (k, v) in url::form_urlencoded::parse(fragment.as_bytes()) {
            if k.eq_ignore_ascii_case("oauth_token") {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
    }

    None
}

fn parse_supported_webview_proxy(proxy: Option<&str>) -> Option<Url> {
    let raw = proxy?.trim();
    if raw.is_empty() {
        return None;
    }
    let parsed = Url::parse(raw).ok()?;
    match parsed.scheme() {
        "http" | "socks5" => Some(parsed),
        _ => None,
    }
}

pub async fn run_embedded_webview_sign_in(
    app: &AppHandle,
    bounds: Option<SignInBounds>,
    proxy: Option<&str>,
) -> Result<String> {
    if is_inline_embedded_supported() {
        if let Some(b) = bounds {
            return run_inline_child_sign_in(app, b, proxy).await;
        }
    }
    run_popup_webview_sign_in(app, proxy).await
}

async fn run_inline_child_sign_in(
    app: &AppHandle,
    bounds: SignInBounds,
    proxy: Option<&str>,
) -> Result<String> {
    if let Some(prev_label) = ACTIVE_SIGNIN_LABEL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        close_signin_target(app, &prev_label);
    }

    SIGNIN_CANCELLED.store(false, Ordering::SeqCst);

    let host_window = app
        .get_window("main")
        .context("Main application window not found")?;

    let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    let label = format!("google-signin-{}", session_id);
    let start_url = Url::parse(EMBEDDED_SETUP_URL).context("Invalid Google sign-in URL")?;

    let url_token: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let url_token_nav = Arc::clone(&url_token);

    let mut builder = tauri::webview::WebviewBuilder::new(&label, WebviewUrl::External(start_url))
        .incognito(true)
        .focused(true)
        .on_navigation(move |nav_url| {
            if let Some(tok) = extract_oauth_token_from_url(nav_url) {
                let mut slot = url_token_nav.lock().unwrap_or_else(|e| e.into_inner());
                if slot.is_none() {
                    *slot = Some(tok);
                }
            }
            true
        });

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        builder = builder.user_agent(WEBKIT_SAFARI_USER_AGENT);
    }

    if let Some(proxy_url) = parse_supported_webview_proxy(proxy) {
        builder = builder.proxy_url(proxy_url);
    }

    let (x, y, w, h) = bounds.clamped();
    let webview = host_window
        .add_child(builder, LogicalPosition::new(x, y), LogicalSize::new(w, h))
        .context("Failed to attach embedded Google sign-in view")?;

    *ACTIVE_SIGNIN_LABEL
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(label.clone());

    let _ = webview.set_focus();

    let deadline = Instant::now() + SIGNIN_TIMEOUT;
    let result: Result<String> = loop {
        if SIGNIN_CANCELLED.load(Ordering::SeqCst) {
            break Err(anyhow::anyhow!("Sign-in cancelled by user."));
        }

        if let Some(tok) = url_token
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            break Ok(tok);
        }

        if app.get_webview(&label).is_none() {
            break Err(anyhow::anyhow!(
                "Sign-in cancelled: sign-in view was closed before completing login."
            ));
        }

        if Instant::now() >= deadline {
            break Err(anyhow::anyhow!(
                "Timed out waiting for Google sign-in to complete (5 minutes)."
            ));
        }

        let wv_poll = webview.clone();
        match tokio::task::spawn_blocking(move || wv_poll.cookies()).await {
            Ok(Ok(cookies)) => {
                if let Some(tok) = extract_oauth_token_from_cookies(&cookies) {
                    break Ok(tok);
                }
            }
            Ok(Err(err)) => {
                tracing::debug!("Embedded WebView cookie poll error (will retry): {}", err);
            }
            Err(join_err) => {
                tracing::debug!("Embedded WebView cookie poll join error (will retry): {}", join_err);
            }
        }

        sleep(POLL_INTERVAL).await;
    };

    {
        let mut active = ACTIVE_SIGNIN_LABEL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if active.as_deref() == Some(label.as_str()) {
            *active = None;
        }
    }
    close_signin_target(app, &label);

    result
}

async fn run_popup_webview_sign_in(app: &AppHandle, proxy: Option<&str>) -> Result<String> {
    if let Some(prev_label) = ACTIVE_SIGNIN_LABEL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        close_signin_target(app, &prev_label);
    }

    SIGNIN_CANCELLED.store(false, Ordering::SeqCst);

    let session_id = NEXT_SESSION_ID.fetch_add(1, Ordering::SeqCst);
    let label = format!("google-signin-{}", session_id);
    let start_url = Url::parse(EMBEDDED_SETUP_URL).context("Invalid Google sign-in URL")?;

    let url_token: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let url_token_nav = Arc::clone(&url_token);

    let mut builder = WebviewWindowBuilder::new(app, &label, WebviewUrl::External(start_url))
        .title("Sign in with Google — Lumen")
        .inner_size(500.0, 700.0)
        .min_inner_size(420.0, 560.0)
        .center()
        .resizable(true)
        .incognito(true)
        .focused(true)
        .on_navigation(move |nav_url| {
            if let Some(tok) = extract_oauth_token_from_url(nav_url) {
                let mut slot = url_token_nav.lock().unwrap_or_else(|e| e.into_inner());
                if slot.is_none() {
                    *slot = Some(tok);
                }
            }
            true
        });

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        builder = builder.user_agent(WEBKIT_SAFARI_USER_AGENT);
    }

    if let Some(proxy_url) = parse_supported_webview_proxy(proxy) {
        builder = builder.proxy_url(proxy_url);
    }

    let window = builder
        .build()
        .context("Failed to open in-app Google sign-in window")?;

    *ACTIVE_SIGNIN_LABEL
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(label.clone());

    let window_closed = Arc::new(AtomicBool::new(false));
    let window_closed_cb = Arc::clone(&window_closed);
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed) {
            window_closed_cb.store(true, Ordering::SeqCst);
        }
    });

    let _ = window.set_focus();

    let deadline = Instant::now() + SIGNIN_TIMEOUT;
    let result: Result<String> = loop {
        if SIGNIN_CANCELLED.load(Ordering::SeqCst) {
            break Err(anyhow::anyhow!("Sign-in cancelled by user."));
        }

        if let Some(tok) = url_token
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            break Ok(tok);
        }

        if window_closed.load(Ordering::SeqCst) {
            break Err(anyhow::anyhow!(
                "Sign-in cancelled: sign-in window was closed before completing login."
            ));
        }

        if Instant::now() >= deadline {
            break Err(anyhow::anyhow!(
                "Timed out waiting for Google sign-in to complete (5 minutes)."
            ));
        }

        let w_poll = window.clone();
        match tokio::task::spawn_blocking(move || w_poll.cookies()).await {
            Ok(Ok(cookies)) => {
                if let Some(tok) = extract_oauth_token_from_cookies(&cookies) {
                    break Ok(tok);
                }
            }
            Ok(Err(err)) => {
                tracing::debug!("WebView cookie poll error (will retry): {}", err);
            }
            Err(join_err) => {
                tracing::debug!("WebView cookie poll join error (will retry): {}", join_err);
            }
        }

        sleep(POLL_INTERVAL).await;
    };

    {
        let mut active = ACTIVE_SIGNIN_LABEL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if active.as_deref() == Some(label.as_str()) {
            *active = None;
        }
    }
    close_signin_target(app, &label);

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cookie(name: &str, value: &str, domain: &str, secure: bool) -> Cookie<'static> {
        let mut c = Cookie::new(name.to_string(), value.to_string());
        c.set_domain(domain.to_string());
        c.set_secure(secure);
        c
    }

    #[test]
    fn extracts_secure_google_oauth_token_cookie() {
        let cookies = vec![
            make_cookie("SID", "abc", ".google.com", true),
            make_cookie("oauth_token", "oauth2_4/test-token", ".accounts.google.com", true),
        ];
        assert_eq!(
            extract_oauth_token_from_cookies(&cookies).as_deref(),
            Some("oauth2_4/test-token")
        );
    }

    #[test]
    fn rejects_non_google_or_insecure_cookies() {
        let cookies = vec![
            make_cookie("oauth_token", "evil1", "evilgoogle.com", true),
            make_cookie("oauth_token", "evil2", "accounts.google.com.attacker.test", true),
            make_cookie("oauth_token", "insecure", "accounts.google.com", false),
            make_cookie("oauth_token", "   ", "accounts.google.com", true),
        ];
        assert_eq!(extract_oauth_token_from_cookies(&cookies), None);
    }

    #[test]
    fn extracts_oauth_token_from_https_google_redirect_url() {
        let url_q = Url::parse("https://accounts.google.com/EmbeddedSetup?oauth_token=tok_query").unwrap();
        assert_eq!(extract_oauth_token_from_url(&url_q).as_deref(), Some("tok_query"));

        let url_f = Url::parse("https://accounts.google.com/done#foo=1&oauth_token=tok_frag").unwrap();
        assert_eq!(extract_oauth_token_from_url(&url_f).as_deref(), Some("tok_frag"));

        let url_http = Url::parse("http://accounts.google.com/EmbeddedSetup?oauth_token=bad").unwrap();
        assert_eq!(extract_oauth_token_from_url(&url_http), None);

        let url_evil = Url::parse("https://evilgoogle.com/?oauth_token=bad").unwrap();
        assert_eq!(extract_oauth_token_from_url(&url_evil), None);
    }

    #[test]
    fn parses_only_http_and_socks5_proxies() {
        assert!(parse_supported_webview_proxy(Some("http://127.0.0.1:7890")).is_some());
        assert!(parse_supported_webview_proxy(Some("socks5://127.0.0.1:1080")).is_some());
        assert!(parse_supported_webview_proxy(Some("https://127.0.0.1:7890")).is_none());
        assert!(parse_supported_webview_proxy(Some("")).is_none());
        assert!(parse_supported_webview_proxy(None).is_none());
    }

    #[test]
    fn clamps_invalid_bounds_safely() {
        let b = SignInBounds {
            x: -10.0,
            y: f64::NAN,
            width: 50.0,
            height: f64::INFINITY,
        };
        assert_eq!(b.clamped(), (0.0, 0.0, 300.0, 540.0));
    }
}
