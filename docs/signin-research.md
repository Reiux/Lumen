# Automated Google Sign-in Research Spike (Desktop)

This document details the feasibility, platform capabilities, and architecture for automating Google sign-in (`EmbeddedSetup`) on desktop within Lumen (Go + Wails v3 + Vue 3), following the reference implementation in Lumen Android (`GoogleSignInWebViewDialog.kt`).

---

## 1. Android Reference Analysis (`GoogleSignInWebViewDialog.kt`)

Source: `Lumen-Kotlin/app/src/main/java/com/reiux/lumen/ui/components/GoogleSignInWebViewDialog.kt`

- **Target URL**:
  `https://accounts.google.com/EmbeddedSetup`
- **Custom User Agent**:
  `Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.6613.127 Mobile Safari/537.36`
  Google's `EmbeddedSetup` endpoint expects an Android-like client environment to serve the tokenized consent flow.
- **Completion Signal**:
  The Android app checks two independent signals:
  1. **URL Parameter**: Upon completing the flow, Google may redirect to a URL containing query/fragment parameters `oauth_token=` or `Token=` (detected via regex `[?&#](?:oauth_token|Token)=([^&#]+)` in `shouldOverrideUrlLoading` and `onPageFinished`).
  2. **Cookie Jar**: The app queries `android.webkit.CookieManager.getInstance().getCookie("https://accounts.google.com")` for a cookie matching `oauth_token=<TOKEN>`.
- **HttpOnly Cookie Status**:
  **Yes, HttpOnly.** Google sets the `oauth_token` cookie on `accounts.google.com` with the `HttpOnly` attribute (along with `Secure`).
  - On Android, `android.webkit.CookieManager` operates at the native Chromium network level, which grants full read access to HttpOnly cookies.
  - In a standard browser or DOM context, `document.cookie` cannot inspect HttpOnly cookies due to browser security restrictions.

---

## 2. Wails v3 Platform Capability Matrix (v3.0.0-beta.33)

Analyzed from the installed Wails v3 source tree (`github.com/wailsapp/wails/v3@v3.0.0-beta.33/pkg/application`):

| Platform | (a) Second Window with Custom User Agent | (b) Navigation / URL-Change Callbacks | (c) Reading Cookie Jar (HttpOnly) |
| :--- | :--- | :--- | :--- |
| **Windows**<br>*(WebView2)* | **Not available**<br>Second window is supported via `app.Window.NewWithOptions` (`window_manager.go:54`). However, custom User-Agent is not exposed in `WebviewWindowOptions` or `WindowsWindow` (`webview_window_options.go:356-437`). Furthermore, Wails v3 hardcodes appending `"wails.io"` to the default Edge User-Agent on every request (`webview_window_windows.go:2388-2395`). | **Partially available (Event only, no URL)**<br>Emits `events.Windows.WebViewNavigationCompleted` (`pkg/events/events.go:420`, `webview_window_windows.go:2811`). `navigationStarting` (`webview_window_windows.go:2778`) emits no event. The event context does not expose the navigated URL (`context_window_event.go:10-74`). | **Not available**<br>Wails v3 does not bind or expose the native WebView2 `ICoreWebView2CookieManager` interface. Zero cookie methods exist in `pkg/application/`. |
| **macOS**<br>*(WKWebView)* | **Not available**<br>Second window is supported (`window_manager.go:54`). Only `ApplicationNameForUserAgent` is exposed (`webview_window_options.go:792`, `webview_window_darwin.go:176-179`), which only sets the app name suffix appended to Safari's default UA. Full custom user agent (`customUserAgent`) is not exposed. | **Partially available (Event only, no URL)**<br>Emits `events.Mac.WebViewDidCommitNavigation` (1114), `WebViewDidFinishNavigation` (1115), `WebViewDidStartProvisionalNavigation` (1117), `WebViewDidReceiveServerRedirectForProvisionalNavigation` (1116) via `WKNavigationDelegate` (`pkg/events/events.go:170-173`, `webview_window_darwin.m:169-175`). However, `WindowEventContext` does not pass the navigated URL. | **Not available**<br>Wails v3 does not wrap or expose `WKHTTPCookieStore` (`getAllCookies`) in its Go API. Zero cookie methods exist in `pkg/application/`. |
| **Linux**<br>*(WebKitGTK)* | **Not available**<br>Second window is supported (`window_manager.go:54`). User-Agent is hardcoded to `"wails.io"` via `webkit_settings_set_user_agent_with_application_details` (`linux_cgo_gtk3.go:1570`). No custom UA setting exists in `LinuxWindow` (`webview_window_options.go:912-940`). | **Partially available (Event only, no URL)**<br>Emits `events.Linux.WindowLoadStarted`, `WindowLoadRedirected`, `WindowLoadCommitted`, `WindowLoadFinished` (`pkg/events/events.go:214-217`, `linux_cgo.go:1666-1681`) on WebKitGTK `load-changed`. However, `WindowEventContext` does not pass the navigated URL. | **Not available**<br>Wails v3 does not wrap or expose `WebKitCookieManager` in its Go API. Zero cookie methods exist in `pkg/application/`. |

### Summary of Native Wails v3 Limitations
1. Wails v3 does not expose an API to set a full arbitrary User-Agent string across desktop platforms.
2. Navigation event callbacks exist on all three desktop OSes, but they notify only of state transitions without delivering the target or current URL string in the event payload.
3. Wails v3 provides zero native cookie jar APIs on any desktop platform. Because `oauth_token` is an HttpOnly cookie, injected JavaScript (`document.cookie`) cannot read it.

---

## 3. Chrome DevTools Protocol Fallback Architecture (`chromedp`)

Because native desktop webview wrappers in Wails v3 do not expose HttpOnly cookie extraction or arbitrary user agent overriding on Windows/Linux, an automated fallback via the Chrome DevTools Protocol (CDP) provides a clean, robust alternative.

### Architecture Overview
1. **Isolated Ephemeral Profile**:
   Lumen launches an installed Chromium-based browser with `--user-data-dir=<temp-dir>` and a dedicated `--user-agent` set to the Android Pixel 8 user agent. The user's normal browser sessions, extensions, and profile data remain completely untouched.
2. **Direct Network Cookie Inspection**:
   Using `chromedp` (or lightweight WebSocket CDP client), Lumen calls the CDP method `Network.getCookies` with `urls: ["https://accounts.google.com"]`. Unlike DOM `document.cookie`, CDP communicates directly with the browser's network process and returns all cookies including `HttpOnly` and `Secure` attributes.
3. **Automated Completion & Teardown**:
   Lumen listens to `Page.frameNavigated` and periodic cookie polls. As soon as `oauth_token` is detected (in either the cookie store or URL redirection), Lumen retrieves whether it exists and its length, closes the temporary browser process, wipes the temporary profile directory, and delivers the token to the auth exchange.

### Locating Installed Chromium Binaries by Platform

If launching an external automated browser, Lumen locates an available binary using standard platform paths:

- **Windows**:
  - Microsoft Edge:
    - `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`
    - `C:\Program Files\Microsoft\Edge\Application\msedge.exe`
  - Google Chrome:
    - `C:\Program Files\Google\Chrome\Application\chrome.exe`
    - `C:\Program Files (x86)\Google\Chrome\Application\chrome.exe`
    - `%LOCALAPPDATA%\Google\Chrome\Application\chrome.exe`
  - Brave:
    - `%LOCALAPPDATA%\BraveSoftware\Brave-Browser\Application\brave.exe`
  - Registry fallback: `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\msedge.exe` or `chrome.exe`

- **macOS**:
  - Google Chrome: `/Applications/Google Chrome.app/Contents/MacOS/Google Chrome`
  - Microsoft Edge: `/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge`
  - Brave: `/Applications/Brave Browser.app/Contents/MacOS/Brave Browser`
  - Chromium: `/Applications/Chromium.app/Contents/MacOS/Chromium`

- **Linux**:
  - Search `$PATH` in order:
    1. `google-chrome`
    2. `google-chrome-stable`
    3. `chromium`
    4. `chromium-browser`
    5. `microsoft-edge-stable`
    6. `microsoft-edge`
    7. `brave-browser`

### Graceful Fallback Strategy
If no supported Chromium-based binary is found on the system (e.g., pure Firefox or headless environment):
- Lumen gracefully falls back to the existing manual sign-in flow (Option A in `GoogleAuthSetup.vue`), opening the system default browser via `Browser.OpenURL("https://accounts.google.com/EmbeddedSetup")` and offering the clipboard auto-detection button.
