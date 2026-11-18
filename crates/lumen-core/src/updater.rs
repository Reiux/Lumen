use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

pub const DEFAULT_RELEASES_URL: &str = "https://github.com/Reiux/Lumen/releases/latest";
pub const UPDATE_THROTTLE_DURATION: Duration = Duration::from_secs(12 * 3600); // 12 hours

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct UpdateInfo {
    #[serde(rename = "currentVersion")]
    pub current_version: String,
    #[serde(rename = "latestVersion")]
    pub latest_version: String,
    #[serde(rename = "hasUpdate")]
    pub has_update: bool,
    #[serde(rename = "releaseUrl")]
    pub release_url: String,
    #[serde(rename = "releaseNotes")]
    pub release_notes: String,
    #[serde(rename = "checkedAt")]
    pub checked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone)]
struct CacheEntry {
    etag: String,
    info: UpdateInfo,
}

#[derive(Clone, Default)]
pub struct UpdateChecker {
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    proxy_override: Option<String>,
}

impl UpdateChecker {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            client,
            cache: Arc::new(RwLock::new(HashMap::new())),
            proxy_override: None,
        }
    }

    pub fn with_proxy(proxy: impl Into<String>) -> Self {
        let mut checker = Self::new();
        checker.proxy_override = Some(proxy.into());
        checker
    }

    fn effective_proxy(&self) -> String {
        if let Some(ref p) = self.proxy_override {
            p.clone()
        } else {
            crate::config::ConfigManager::global().get_preferences().proxy
        }
    }

    pub async fn check_for_updates(
        &self,
        redirect_url: &str,
        current_version: &str,
        manual: bool,
        last_check: i64,
    ) -> UpdateInfo {
        self.check_for_updates_inner(redirect_url, current_version, manual, last_check, true)
            .await
    }

    async fn check_for_updates_inner(
        &self,
        redirect_url: &str,
        current_version: &str,
        manual: bool,
        last_check: i64,
        enforce_trusted_origin: bool,
    ) -> UpdateInfo {
        let now_sec = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        if enforce_trusted_origin {
            if let Ok(parsed) = url::Url::parse(redirect_url) {
                let is_https = parsed.scheme() == "https";
                let is_trusted_host = parsed.host_str() == Some("github.com");
                let is_trusted_path = parsed.path().to_lowercase().starts_with("/reiux/");
                if !is_https || !is_trusted_host || !is_trusted_path {
                    return UpdateInfo {
                        current_version: current_version.to_string(),
                        has_update: false,
                        error: Some("Untrusted update redirect URL origin".to_string()),
                        ..Default::default()
                    };
                }
            } else {
                return UpdateInfo {
                    current_version: current_version.to_string(),
                    has_update: false,
                    error: Some("Invalid update URL format".to_string()),
                    ..Default::default()
                };
            }
        }

        if !manual && last_check > 0 && (now_sec - last_check) < (12 * 3600) {
            return UpdateInfo {
                current_version: current_version.to_string(),
                has_update: false,
                checked_at: now_sec.to_string(),
                ..Default::default()
            };
        }

        let cached_entry = self
            .cache
            .read()
            .ok()
            .and_then(|c| c.get(redirect_url).cloned());

        let configured_proxy = self.effective_proxy();
        let active_client = if !configured_proxy.trim().is_empty() {
            match crate::net::create_http_client(Some(&configured_proxy)) {
                Ok(c) => c,
                Err(e) => {
                    return UpdateInfo {
                        current_version: current_version.to_string(),
                        error: Some(format!("Invalid proxy configuration: {}", e)),
                        checked_at: now_sec.to_string(),
                        ..Default::default()
                    };
                }
            }
        } else {
            self.client.clone()
        };

        let mut req = active_client
            .get(redirect_url)
            .header("User-Agent", format!("Lumen-Desktop/{}", current_version));

        if let Some(entry) = &cached_entry {
            req = req.header("If-None-Match", &entry.etag);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(_e) => {
                return UpdateInfo {
                    current_version: current_version.to_string(),
                    error: Some("Unable to reach GitHub. Please check your internet connection.".to_string()),
                    checked_at: now_sec.to_string(),
                    ..Default::default()
                };
            }
        };

        let status = resp.status();

        if status == reqwest::StatusCode::NOT_MODIFIED {
            if let Some(entry) = cached_entry {
                let mut info = entry.info;
                info.current_version = current_version.to_string();
                info.has_update = !info.latest_version.is_empty()
                    && is_version_newer(&info.latest_version, current_version);
                info.error = None;
                info.checked_at = now_sec.to_string();
                return info;
            }
            return UpdateInfo {
                current_version: current_version.to_string(),
                has_update: false,
                checked_at: now_sec.to_string(),
                ..Default::default()
            };
        }

        let response_etag = resp
            .headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let msg = parse_rate_limit_message(&resp);
            return UpdateInfo {
                current_version: current_version.to_string(),
                error: Some(msg),
                checked_at: now_sec.to_string(),
                ..Default::default()
            };
        }

        if status.is_redirection() {
            if let Some(loc_header) = resp.headers().get("location") {
                if let Ok(loc) = loc_header.to_str() {
                    let full_url = if loc.starts_with("http") {
                        loc.to_string()
                    } else {
                        format!("https://github.com{}", loc)
                    };

                    let is_safe_redirect = if !enforce_trusted_origin {
                        true
                    } else if let Ok(u) = url::Url::parse(&full_url) {
                        u.scheme() == "https"
                            && (u.host_str() == Some("github.com") || u.host_str() == Some("api.github.com"))
                            && u.path().to_lowercase().starts_with("/reiux/")
                    } else {
                        false
                    };

                    if !is_safe_redirect {
                        return UpdateInfo {
                            current_version: current_version.to_string(),
                            error: Some("Untrusted redirect location origin in update response".to_string()),
                            checked_at: now_sec.to_string(),
                            ..Default::default()
                        };
                    }

                    if let Some(tag) = extract_tag_from_location(loc) {
                        let has_update = is_version_newer(&tag, current_version);
                        let info = UpdateInfo {
                            current_version: current_version.to_string(),
                            latest_version: tag,
                            has_update,
                            release_url: full_url,
                            checked_at: now_sec.to_string(),
                            ..Default::default()
                        };
                        self.store_cache_entry(redirect_url, response_etag.as_deref(), &info);
                        return info;
                    }
                }
            }
        }

        // Direct response fallback (e.g. for mock server tests)
        if status.is_success() {
            #[derive(Deserialize)]
            struct DirectRelease {
                tag_name: String,
                html_url: Option<String>,
            }
            if let Ok(body_bytes) = crate::net::read_response_bytes_bounded(resp, 512 * 1024).await {
                if let Ok(rel) = serde_json::from_slice::<DirectRelease>(&body_bytes) {
                    let has_update = is_version_newer(&rel.tag_name, current_version);
                    let safe_release_url = rel
                        .html_url
                        .filter(|url_str| {
                            if !enforce_trusted_origin {
                                return true;
                            }
                            if let Ok(u) = url::Url::parse(url_str) {
                                u.scheme() == "https"
                                    && u.host_str() == Some("github.com")
                                    && u.path().to_lowercase().starts_with("/reiux/")
                            } else {
                                false
                            }
                        })
                        .unwrap_or_else(|| redirect_url.to_string());
                    let info = UpdateInfo {
                        current_version: current_version.to_string(),
                        latest_version: rel.tag_name,
                        has_update,
                        release_url: safe_release_url,
                        checked_at: now_sec.to_string(),
                        ..Default::default()
                    };
                    self.store_cache_entry(redirect_url, response_etag.as_deref(), &info);
                    return info;
                }
            }
        }

        UpdateInfo {
            current_version: current_version.to_string(),
            error: Some("Could not parse release from GitHub redirect.".to_string()),
            checked_at: now_sec.to_string(),
            ..Default::default()
        }
    }

    fn store_cache_entry(&self, redirect_url: &str, etag: Option<&str>, info: &UpdateInfo) {
        if let Some(etag_val) = etag {
            if let Ok(mut c) = self.cache.write() {
                c.insert(
                    redirect_url.to_string(),
                    CacheEntry {
                        etag: etag_val.to_string(),
                        info: info.clone(),
                    },
                );
            }
        }
    }

    pub async fn fetch_release_notes(&self, tag: &str) -> Result<String, String> {
        let trimmed = tag.trim().trim_start_matches('v');
        if trimmed.is_empty() {
            return Err("Empty release tag".to_string());
        }
        let clean_tag = if tag.trim().starts_with('v') {
            tag.trim().to_string()
        } else {
            format!("v{}", trimmed)
        };
        if !clean_tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
        {
            return Err("Invalid release tag characters".to_string());
        }

        let configured_proxy = self.effective_proxy();
        let client = if !configured_proxy.trim().is_empty() {
            crate::net::create_http_client(Some(&configured_proxy))
                .map_err(|e| format!("Invalid proxy configuration: {}", e))?
        } else {
            self.client.clone()
        };

        let api_url = format!(
            "https://api.github.com/repos/Reiux/Lumen/releases/tags/{}",
            clean_tag
        );
        let resp = client
            .get(&api_url)
            .header("User-Agent", "Lumen-Desktop")
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if !resp.status().is_success() {
            return Err(format!("GitHub API returned HTTP {}", resp.status()));
        }

        #[derive(Deserialize)]
        struct GitHubRelease {
            body: Option<String>,
        }
        let body_bytes = crate::net::read_response_bytes_bounded(resp, 512 * 1024)
            .await
            .map_err(|e| e.to_string())?;
        let parsed: GitHubRelease =
            serde_json::from_slice(&body_bytes).map_err(|e| e.to_string())?;
        Ok(parsed.body.unwrap_or_default().trim().to_string())
    }
}

pub fn extract_tag_from_location(loc: &str) -> Option<String> {
    let idx = loc.find("/tag/")?;
    let mut tag = &loc[idx + "/tag/".len()..];
    if let Some(end) = tag.find(['?', '#']) {
        tag = &tag[..end];
    }
    let trimmed = tag.trim_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_semver_parts(raw: &str) -> (Vec<u64>, Option<String>) {
    let s = raw.trim().trim_start_matches(['v', 'V']);
    let without_build = s.split('+').next().unwrap_or("");
    let (core_str, pre_str) = match without_build.split_once('-') {
        Some((core, pre)) => (core, (!pre.trim().is_empty()).then(|| pre.trim().to_string())),
        None => (without_build, None),
    };
    let mut core: Vec<u64> = core_str
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect();
    while core.len() < 3 {
        core.push(0);
    }
    (core, pre_str)
}

pub fn is_version_newer(latest: &str, current: &str) -> bool {
    let (latest_core, latest_pre) = parse_semver_parts(latest);
    let (current_core, current_pre) = parse_semver_parts(current);
    if latest_core != current_core {
        return latest_core > current_core;
    }
    match (latest_pre, current_pre) {
        (None, Some(_)) => true,
        (Some(_), None) => false,
        (Some(lp), Some(cp)) => lp > cp,
        (None, None) => false,
    }
}

fn parse_rate_limit_message(resp: &reqwest::Response) -> String {
    if let Some(reset) = resp.headers().get("X-RateLimit-Reset").and_then(|v| v.to_str().ok()) {
        if let Ok(epoch) = reset.parse::<i64>() {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;
            let diff = epoch.saturating_sub(now);
            let mins = std::cmp::max(1, diff.saturating_add(59) / 60);
            return format!("GitHub is limiting requests right now. Try again in {} minutes.", mins);
        }
    }
    if let Some(retry) = resp.headers().get("Retry-After").and_then(|v| v.to_str().ok()) {
        if let Ok(secs) = retry.parse::<i64>() {
            let mins = std::cmp::max(1, secs.saturating_add(59) / 60);
            return format!("GitHub is limiting requests right now. Try again in {} minutes.", mins);
        }
    }
    "GitHub is limiting requests right now. Try again in 60 minutes.".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tag_from_location() {
        assert_eq!(
            extract_tag_from_location("https://github.com/Reiux/Lumen/releases/tag/v1.2.0"),
            Some("v1.2.0".to_string())
        );
        assert_eq!(
            extract_tag_from_location("/Reiux/Lumen/releases/tag/v0.11.0?query=1"),
            Some("v0.11.0".to_string())
        );
    }

    #[test]
    fn test_is_version_newer() {
        assert!(is_version_newer("v1.1.0", "v1.0.0"));
        assert!(is_version_newer("v2.0.0", "v1.9.9"));
        assert!(!is_version_newer("v1.0.0", "v1.0.0"));
        assert!(!is_version_newer("v0.9.0", "v1.0.0"));
        assert!(!is_version_newer("v2.1.0-beta.1", "2.1.0"));
        assert!(is_version_newer("v2.1.0", "2.1.0-beta.1"));
        assert!(is_version_newer("v2.1.1-rc.1", "2.1.0"));
    }

    #[tokio::test]
    async fn test_etag_304_preserves_cached_update_info() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/Reiux/Lumen/releases/latest"))
            .and(header("If-None-Match", "\"v2-etag\""))
            .respond_with(ResponseTemplate::new(304))
            .expect(2)
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path("/Reiux/Lumen/releases/latest"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("ETag", "\"v2-etag\"")
                    .insert_header(
                        "Location",
                        "https://github.com/Reiux/Lumen/releases/tag/v2.5.0",
                    ),
            )
            .up_to_n_times(1)
            .mount(&server)
            .await;

        let checker = UpdateChecker::new();
        let url = format!("{}/Reiux/Lumen/releases/latest", server.uri());

        let first = checker
            .check_for_updates_inner(&url, "2.0.0", true, 0, false)
            .await;
        assert!(first.has_update);
        assert_eq!(first.latest_version, "v2.5.0");

        let second = checker
            .check_for_updates_inner(&url, "2.0.0", true, 0, false)
            .await;
        assert!(second.has_update);
        assert_eq!(second.latest_version, "v2.5.0");
        assert_eq!(
            second.release_url,
            "https://github.com/Reiux/Lumen/releases/tag/v2.5.0"
        );

        let third = checker
            .check_for_updates_inner(&url, "2.5.0", true, 0, false)
            .await;
        assert!(!third.has_update);
    }
}
