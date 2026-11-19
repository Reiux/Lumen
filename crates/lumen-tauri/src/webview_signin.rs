//! Built-in Tauri WebView window sign-in for Google EmbeddedSetup.
//!
//! Opens an isolated incognito Tauri WebviewWindow pointed at Google's
//! EmbeddedSetup endpoint and polls its native WebView cookie store
//! (`WKHTTPCookieStore` on macOS, `ICoreWebView2CookieManager` on Windows,
//! `WebKitCookieManager` on Linux) for the `HttpOnly` + `Secure`
//! `oauth_token` cookie.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tauri::webview::Cookie;
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tokio::time::sleep;

use crate::cdp::is_google_auth_cookie_domain;

const EMBEDDED_SETUP_URL: &str = "https://accounts.google.com/embedded/setup/v2/android";
const SIGNIN_TIMEOUT: Duration = Duration::from_secs(300);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// On WebKit-based desktop WebViews (macOS WKWebView and Linux WebKitGTK),
/// use a standard Safari macOS User-Agent matching the WebKit engine so Google
/// does not flag the embedded view for lacking `Version/... Safari/...`.
/// On Windows, WebView2 is Chromium-based and uses its genuine Edge/Chromium UA.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const WEBKIT_SAFARI_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Safari/605.1.15";

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
static SIGNIN_CANCELLED: AtomicBool = AtomicBool::new(false);
static ACTIVE_SIGNIN_LABEL: Mutex<Option<String>> = Mutex::new(None);

fn close_window_by_label(app: &AppHandle, label: &str) {
    if let Some(window) = app.get_webview_window(label) {
        if window.close().is_err() {
            let _ = window.destroy();
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
        close_window_by_label(app, &label);
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

pub async fn run_webview_sign_in(app: &AppHandle, proxy: Option<&str>) -> Result<String> {
    // Close any prior in-app sign-in window cleanly before starting a new session
    if let Some(prev_label) = ACTIVE_SIGNIN_LABEL
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
    {
        close_window_by_label(app, &prev_label);
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

        // Poll cookies off the Tokio async worker thread (important for Windows WebView2)
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

    // Always close the ephemeral sign-in window and clear active session tracking
    {
        let mut active = ACTIVE_SIGNIN_LABEL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if active.as_deref() == Some(label.as_str()) {
            *active = None;
        }
    }
    close_window_by_label(app, &label);

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
}
