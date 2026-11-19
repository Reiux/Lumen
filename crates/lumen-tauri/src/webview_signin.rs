//! Built-in Tauri WebView sign-in window for Google EmbeddedSetup.
//!
//! Opens a centered, isolated incognito Tauri `WebviewWindow` (excluded from
//! `capabilities/default.json` so remote pages have zero Tauri IPC access)
//! pointed at Google's EmbeddedSetup endpoint, and polls its native cookie
//! store (`WKHTTPCookieStore` on macOS, `ICoreWebView2CookieManager` on
//! Windows, `WebKitCookieManager` on Linux) off-thread for the `HttpOnly` +
//! `Secure` `oauth_token` cookie.

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

#[cfg(any(target_os = "macos", target_os = "linux"))]
const WEBKIT_SAFARI_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Safari/605.1.15";

struct ActiveWebviewSession {
    label: String,
    cancel_token: Arc<AtomicBool>,
}

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);
static SIGNIN_RUN_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
static ACTIVE_SIGNIN_SESSION: Mutex<Option<ActiveWebviewSession>> = Mutex::new(None);

fn close_signin_window(app: &AppHandle, label: &str) {
    if let Some(win) = app.get_webview_window(label) {
        if win.close().is_err() {
            let _ = win.destroy();
        }
    }
}

pub fn cancel_active_sign_in(app: &AppHandle) {
    let session = ACTIVE_SIGNIN_SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(session) = session {
        session.cancel_token.store(true, Ordering::SeqCst);
        close_signin_window(app, &session.label);
    }
}

/// Navigation allowlist for the sign-in WebView:
/// - Allow `about:blank`
/// - Allow `https://` URLs with a valid host
/// - Explicitly reject `tauri.localhost`, `ipc.localhost` (and their subdomains),
///   and any non-`https` scheme (`http`, `file`, `javascript`, `data`, `tauri`, `ipc`, etc.).
pub(crate) fn is_allowed_signin_navigation_url(url: &Url) -> bool {
    if url.as_str().eq_ignore_ascii_case("about:blank") {
        return true;
    }
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    if lower.is_empty()
        || lower == "tauri.localhost"
        || lower.ends_with(".tauri.localhost")
        || lower == "ipc.localhost"
        || lower.ends_with(".ipc.localhost")
    {
        return false;
    }
    true
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

/// Extract a valid `oauth_token` parameter from an HTTPS `accounts.google.com`
/// redirect URL's query string or fragment, if present.
pub(crate) fn extract_oauth_token_from_url(url: &Url) -> Option<String> {
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?;
    if !host.eq_ignore_ascii_case("accounts.google.com") {
        return None;
    }

    for (k, v) in url.query_pairs() {
        if k.eq_ignore_ascii_case("oauth_token") {
            if let Ok(clean) = lumen_core::auth::normalize_oauth_token(&v) {
                return Some(clean);
            }
        }
    }

    if let Some(fragment) = url.fragment() {
        for (k, v) in url::form_urlencoded::parse(fragment.as_bytes()) {
            if k.eq_ignore_ascii_case("oauth_token") {
                if let Ok(clean) = lumen_core::auth::normalize_oauth_token(&v) {
                    return Some(clean);
                }
            }
        }
    }

    None
}

fn parse_supported_webview_proxy(proxy: Option<&str>) -> Result<Option<Url>> {
    let Some(raw) = proxy.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let parsed = Url::parse(raw).map_err(|e| anyhow::anyhow!("Invalid proxy URL: {}", e))?;
    match parsed.scheme() {
        "http" | "socks5" => {}
        scheme => {
            anyhow::bail!(
                "Embedded WebView sign-in does not support '{}://' proxies (only http:// and socks5:// are supported). Use an http:// proxy or external browser sign-in.",
                scheme
            );
        }
    }
    if parsed.host_str().filter(|h| !h.is_empty()).is_none() {
        anyhow::bail!("Proxy URL must specify a valid host.");
    }
    if parsed.port().is_none() {
        anyhow::bail!("Proxy URL must specify an explicit port for embedded WebView sign-in.");
    }
    Ok(Some(parsed))
}

pub async fn run_webview_sign_in(app: &AppHandle, proxy: Option<&str>) -> Result<String> {
    let proxy_url = parse_supported_webview_proxy(proxy)?;

    // Cancel any prior active sign-in session so the serialized lock is released promptly.
    cancel_active_sign_in(app);
    let _run_guard = SIGNIN_RUN_LOCK.lock().await;

    let cancel_token = Arc::new(AtomicBool::new(false));
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
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .on_navigation(move |nav_url| {
            if !is_allowed_signin_navigation_url(nav_url) {
                tracing::warn!(
                    "Blocked disallowed navigation in sign-in WebView: scheme={}",
                    nav_url.scheme()
                );
                return false;
            }
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

    if let Some(proxy_url) = proxy_url {
        builder = builder.proxy_url(proxy_url);
    }

    let window = builder
        .build()
        .context("Failed to open Google sign-in window")?;

    *ACTIVE_SIGNIN_SESSION
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(ActiveWebviewSession {
        label: label.clone(),
        cancel_token: Arc::clone(&cancel_token),
    });

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
        if cancel_token.load(Ordering::SeqCst) {
            break Err(anyhow::anyhow!("Sign-in cancelled by user."));
        }

        if let Some(tok) = url_token
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            break Ok(tok);
        }

        if window_closed.load(Ordering::SeqCst) || app.get_webview_window(&label).is_none() {
            break Err(anyhow::anyhow!(
                "Sign-in cancelled: sign-in window was closed before completing login."
            ));
        }

        if Instant::now() >= deadline {
            break Err(anyhow::anyhow!(
                "Timed out waiting for Google sign-in to complete (5 minutes)."
            ));
        }

        // Poll cookies off the Tokio async worker thread (required for Windows WebView2)
        // with a bounded timeout so teardown never hangs indefinitely.
        let w_poll = window.clone();
        match tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || w_poll.cookies()),
        )
        .await
        {
            Ok(Ok(Ok(cookies))) => {
                if let Some(tok) = extract_oauth_token_from_cookies(&cookies) {
                    break Ok(tok);
                }
            }
            Ok(Ok(Err(err))) => {
                tracing::debug!("WebView cookie poll error (will retry): {}", err);
            }
            Ok(Err(join_err)) => {
                tracing::debug!("WebView cookie poll join error (will retry): {}", join_err);
            }
            Err(_) => {
                tracing::warn!("WebView cookie poll timed out after 5s (will retry)");
            }
        }

        sleep(POLL_INTERVAL).await;
    };

    {
        let mut active = ACTIVE_SIGNIN_SESSION
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if active.as_ref().map(|s| s.label.as_str()) == Some(label.as_str()) {
            *active = None;
        }
    }
    close_signin_window(app, &label);

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
    fn extracts_oauth_token_only_from_https_accounts_google_com() {
        let url_q = Url::parse(
            "https://accounts.google.com/EmbeddedSetup?oauth_token=oauth2_4/test_query_token_1234",
        )
        .unwrap();
        assert_eq!(
            extract_oauth_token_from_url(&url_q).as_deref(),
            Some("oauth2_4/test_query_token_1234")
        );

        let url_f = Url::parse(
            "https://accounts.google.com/done#foo=1&oauth_token=oauth2_4/test_frag_token_5678",
        )
        .unwrap();
        assert_eq!(
            extract_oauth_token_from_url(&url_f).as_deref(),
            Some("oauth2_4/test_frag_token_5678")
        );

        // Short / invalid token rejected by normalize_oauth_token
        let url_short =
            Url::parse("https://accounts.google.com/EmbeddedSetup?oauth_token=short").unwrap();
        assert_eq!(extract_oauth_token_from_url(&url_short), None);

        // Non-accounts Google subdomain must be rejected
        let url_sites = Url::parse(
            "https://sites.google.com/view/attacker?oauth_token=oauth2_4/test_query_token_1234",
        )
        .unwrap();
        assert_eq!(extract_oauth_token_from_url(&url_sites), None);

        let url_http = Url::parse(
            "http://accounts.google.com/EmbeddedSetup?oauth_token=oauth2_4/test_query_token_1234",
        )
        .unwrap();
        assert_eq!(extract_oauth_token_from_url(&url_http), None);

        let url_evil = Url::parse(
            "https://evilgoogle.com/?oauth_token=oauth2_4/test_query_token_1234",
        )
        .unwrap();
        assert_eq!(extract_oauth_token_from_url(&url_evil), None);
    }

    #[test]
    fn enforces_signin_navigation_allowlist() {
        assert!(is_allowed_signin_navigation_url(
            &Url::parse("https://accounts.google.com/embedded/setup/v2/android").unwrap()
        ));
        assert!(is_allowed_signin_navigation_url(&Url::parse("about:blank").unwrap()));

        assert!(!is_allowed_signin_navigation_url(
            &Url::parse("http://accounts.google.com/").unwrap()
        ));
        assert!(!is_allowed_signin_navigation_url(
            &Url::parse("https://tauri.localhost/index.html").unwrap()
        ));
        assert!(!is_allowed_signin_navigation_url(
            &Url::parse("https://ipc.localhost/invoke").unwrap()
        ));
        assert!(!is_allowed_signin_navigation_url(
            &Url::parse("file:///etc/passwd").unwrap()
        ));
        assert!(!is_allowed_signin_navigation_url(
            &Url::parse("javascript:alert(1)").unwrap()
        ));
    }

    #[test]
    fn validates_webview_proxy_scheme_host_and_port() {
        assert!(parse_supported_webview_proxy(Some("http://127.0.0.1:7890"))
            .unwrap()
            .is_some());
        assert!(parse_supported_webview_proxy(Some("socks5://127.0.0.1:1080"))
            .unwrap()
            .is_some());
        assert!(parse_supported_webview_proxy(Some("")).unwrap().is_none());
        assert!(parse_supported_webview_proxy(None).unwrap().is_none());

        // Unsupported scheme must return explicit Err instead of silently bypassing
        assert!(parse_supported_webview_proxy(Some("https://127.0.0.1:7890")).is_err());

        // Missing port or missing host must return Err
        assert!(parse_supported_webview_proxy(Some("http://127.0.0.1")).is_err());
        assert!(parse_supported_webview_proxy(Some("socks5://127.0.0.1")).is_err());
        assert!(parse_supported_webview_proxy(Some("http://:7890")).is_err());
    }
}
