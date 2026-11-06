use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const EMBEDDED_SETUP_AUTH_ENDPOINT: &str = "https://android.clients.google.com/auth";
pub const GOOGLE_AUTH_EMAIL_HINT: &str = "oauth-token@example.com";
pub const GOOGLE_PLAY_SERVICES_SIG: &str = "38918a453d07199354f8b19af05ec6562ced5788";
pub const GOOGLE_PHOTOS_PACKAGE: &str = "com.google.android.apps.photos";
pub const GOOGLE_PHOTOS_SIG: &str = "24bb24c05e47e0aefa68a58a766179d9b613a600";
pub const GOOGLE_PHOTOS_SERVICE: &str = "oauth2:openid https://www.googleapis.com/auth/mobileapps.native https://www.googleapis.com/auth/photos.native";

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    #[error("Token expired or rejected by Google (HTTP 401/403). Please obtain a fresh token.")]
    Unauthorized,
    #[error("Google requires browser verification for this account (NeedsBrowser).")]
    NeedsBrowser,
    #[error("This Google account has been disabled or restricted by Google.")]
    AccountDisabled,
    #[error("Invalid credential format: {0}")]
    InvalidFormat(String),
    #[error("Network error during authentication: {0}")]
    Network(String),
    #[error("Authentication failed: {0}")]
    Other(String),
}

pub fn looks_like_auth_string(val: &str) -> bool {
    let val = val.trim();
    if !val.contains('=') {
        return false;
    }
    let params: HashMap<String, String> = url::form_urlencoded::parse(val.as_bytes())
        .into_owned()
        .collect();
    params.contains_key("Email") && params.contains_key("Token")
}

pub fn normalize_oauth_token(mut val: &str) -> Result<String, AuthError> {
    val = val.trim();
    if let Some(stripped) = val.strip_prefix("oauth_token=") {
        val = stripped.trim();
    }
    let val = val.trim_matches(|c| c == ';' || c == '"' || c == '\'');
    if val.len() < 16 || val.len() > 8192 || val.contains('\r') || val.contains('\n') {
        return Err(AuthError::InvalidFormat(
            "Enter the oauth_token cookie value from Google Embedded Setup".to_string(),
        ));
    }
    Ok(val.to_string())
}

pub fn generate_android_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{:016x}", (now ^ 0x5a5a5a5a5a5a5a5a) & 0x7fffffffffffffff)
}

pub fn build_google_photos_credential(email: &str, master_token: &str, android_id: &str) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(&[
        ("androidId", android_id),
        ("app", GOOGLE_PHOTOS_PACKAGE),
        ("callerPkg", GOOGLE_PHOTOS_PACKAGE),
        ("callerSig", GOOGLE_PHOTOS_SIG),
        ("client_sig", GOOGLE_PHOTOS_SIG),
        ("device_country", "us"),
        ("Email", email),
        ("google_play_services_version", "240913000"),
        ("lang", "en_US"),
        ("oauth2_foreground", "1"),
        ("operatorCountry", "us"),
        ("sdk_version", "33"),
        ("service", GOOGLE_PHOTOS_SERVICE),
        ("source", "android"),
        ("Token", master_token),
    ]);
    serializer.finish()
}

pub fn parse_key_value_response(body: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            result.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    result
}

pub fn classify_auth_error_message(msg: &str) -> AuthError {
    let lower = msg.to_lowercase();
    if lower.contains("badauthentication") || lower.contains("401") || lower.contains("unauthorized") {
        AuthError::Unauthorized
    } else if lower.contains("needsbrowser") {
        AuthError::NeedsBrowser
    } else if lower.contains("accountdisabled") || lower.contains("accountdeleted") || lower.contains("restricted") {
        AuthError::AccountDisabled
    } else if lower.contains("timeout") || lower.contains("connection refused") || lower.contains("no such host") {
        AuthError::Network(msg.to_string())
    } else {
        AuthError::Other(msg.to_string())
    }
}

pub struct GoogleAuthExchange {
    pub email: String,
    pub master_token: String,
}

pub async fn exchange_embedded_setup_token(
    client: &reqwest::Client,
    endpoint: &str,
    oauth_token: &str,
    android_id: &str,
) -> Result<GoogleAuthExchange, AuthError> {
    let clean_token = normalize_oauth_token(oauth_token)?;

    let params = [
        ("accountType", "HOSTED_OR_GOOGLE"),
        ("Email", GOOGLE_AUTH_EMAIL_HINT),
        ("has_permission", "1"),
        ("add_account", "1"),
        ("ACCESS_TOKEN", "1"),
        ("Token", clean_token.as_str()),
        ("service", "ac2dm"),
        ("source", "android"),
        ("androidId", android_id),
        ("device_country", "us"),
        ("operatorCountry", "us"),
        ("lang", "en"),
        ("sdk_version", "17"),
        ("google_play_services_version", "240913000"),
        ("client_sig", GOOGLE_PLAY_SERVICES_SIG),
        ("callerSig", GOOGLE_PLAY_SERVICES_SIG),
        ("droidguard_results", "dummy123"),
    ];

    let resp = client
        .post(endpoint)
        .header("Accept-Encoding", "identity")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("User-Agent", "GoogleAuth/1.4")
        .form(&params)
        .send()
        .await
        .map_err(|e| AuthError::Network(e.to_string()))?;

    let status = resp.status();
    let body = resp.text().await.map_err(|e| AuthError::Network(e.to_string()))?;

    if !status.is_success() {
        return Err(classify_auth_error_message(&format!("HTTP {}: {}", status.as_u16(), body)));
    }

    let parsed = parse_key_value_response(&body);
    if let Some(err_code) = parsed.get("Error") {
        return Err(classify_auth_error_message(err_code));
    }

    let master_token = parsed
        .get("Token")
        .ok_or_else(|| AuthError::Other("response did not contain Token".to_string()))?
        .clone();
    let email = parsed
        .get("Email")
        .ok_or_else(|| AuthError::Other("response did not contain Email".to_string()))?
        .clone();

    Ok(GoogleAuthExchange {
        email,
        master_token,
    })
}

#[derive(Clone)]
pub struct TokenManager {
    account_email: String,
    inner: Arc<RwLock<TokenState>>,
}

struct TokenState {
    access_token: String,
    expires_at: SystemTime,
}

impl TokenManager {
    pub fn new(account_email: impl Into<String>) -> Self {
        Self {
            account_email: account_email.into(),
            inner: Arc::new(RwLock::new(TokenState {
                access_token: String::new(),
                expires_at: UNIX_EPOCH,
            })),
        }
    }

    pub fn account_email(&self) -> &str {
        &self.account_email
    }

    pub fn get_cached_token(&self) -> Option<String> {
        let state = self.inner.read().ok()?;
        if state.access_token.is_empty() {
            return None;
        }
        // Valid only if there are at least 5 minutes remaining before expiry
        let safety_margin = Duration::from_secs(300);
        if SystemTime::now() + safety_margin < state.expires_at {
            Some(state.access_token.clone())
        } else {
            None
        }
    }

    pub fn set_token(&self, token: String, expires_in: Duration) {
        if let Ok(mut state) = self.inner.write() {
            state.access_token = token;
            state.expires_at = SystemTime::now() + expires_in;
        }
    }

    pub fn invalidate(&self) {
        if let Ok(mut state) = self.inner.write() {
            state.access_token.clear();
            state.expires_at = UNIX_EPOCH;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_auth_string() {
        assert!(looks_like_auth_string("Email=user@gmail.com&Token=secret_token&androidId=123"));
        assert!(!looks_like_auth_string("oauth_token=ya29.sample"));
        assert!(!looks_like_auth_string("user@gmail.com"));
    }

    #[test]
    fn test_normalize_oauth_token() {
        assert_eq!(
            normalize_oauth_token("oauth_token=1234567890abcdef;").unwrap(),
            "1234567890abcdef"
        );
        assert!(normalize_oauth_token("too_short").is_err());
    }

    #[test]
    fn test_token_manager_expiry_and_invalidation() {
        let tm = TokenManager::new("test@example.com");
        assert!(tm.get_cached_token().is_none());

        // Set token with 10 minutes expiry (> 5 min margin)
        tm.set_token("token_123".to_string(), Duration::from_secs(600));
        assert_eq!(tm.get_cached_token(), Some("token_123".to_string()));

        // Invalidate
        tm.invalidate();
        assert!(tm.get_cached_token().is_none());

        // Token with 2 minutes expiry (< 5 min margin) should be treated as expired
        tm.set_token("token_short".to_string(), Duration::from_secs(120));
        assert!(tm.get_cached_token().is_none());
    }
}
