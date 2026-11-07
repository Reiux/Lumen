use std::sync::{Arc, LazyLock, RwLock};
use std::time::Duration;
use regex::Regex;
use serde::{Deserialize, Serialize};

static RESTRICTED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(AccountDisabled|AccountDeleted|ServiceDisabled|DeviceManagementRequired)\b").unwrap()
});

static SIGN_IN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(NeedsBrowser|BadAuthentication|WebLoginRequired|web\s+verification\s+challenge)\b").unwrap()
});

static RATE_LIMIT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(RateLimitExceeded|rate_limit_exceeded|TooManyRequests)\b").unwrap()
});

static STATUS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:status(?:\s+code)?|HTTP)\s*[:=]?\s*([1-5]\d{2})\b").unwrap()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CircuitState {
    Active,
    Paused,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    None,
    SignInRequired,
    AccountRestricted,
    RateLimited,
    Transient,
    Fatal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountBlockedEvent {
    pub reason: String, // "SIGN_IN_REQUIRED", "ACCOUNT_RESTRICTED", "RATE_LIMITED"
    pub email: String,
    pub message: String,
    #[serde(rename = "resumeAt", skip_serializing_if = "Option::is_none")]
    pub resume_at: Option<i64>, // Unix ms timestamp
}

pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    blocked_event: Arc<RwLock<Option<AccountBlockedEvent>>>,
    on_blocked: Arc<dyn Fn(AccountBlockedEvent) + Send + Sync>,
}

impl CircuitBreaker {
    pub fn new<F>(on_blocked: F) -> Self
    where
        F: Fn(AccountBlockedEvent) + Send + Sync + 'static,
    {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Active)),
            blocked_event: Arc::new(RwLock::new(None)),
            on_blocked: Arc::new(on_blocked),
        }
    }

    pub fn state(&self) -> CircuitState {
        *self.state.read().unwrap()
    }

    pub fn is_blocked(&self) -> bool {
        *self.state.read().unwrap() == CircuitState::Blocked
    }

    pub fn trip(&self, kind: ErrorKind, email: &str, message: &str) {
        let mut state = self.state.write().unwrap();
        if *state == CircuitState::Blocked {
            return;
        }
        *state = CircuitState::Blocked;

        let event = AccountBlockedEvent {
            reason: format!("{:?}", kind).to_uppercase(),
            email: email.to_string(),
            message: message.to_string(),
            resume_at: None,
        };
        *self.blocked_event.write().unwrap() = Some(event.clone());
        (self.on_blocked)(event);
    }

    pub fn pause(&self, duration: Duration, email: &str, message: &str) {
        let mut state = self.state.write().unwrap();
        if *state == CircuitState::Blocked {
            return;
        }
        *state = CircuitState::Paused;

        let resume_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
            + duration.as_millis() as i64;

        let event = AccountBlockedEvent {
            reason: "RATE_LIMITED".to_string(),
            email: email.to_string(),
            message: message.to_string(),
            resume_at: Some(resume_at),
        };
        (self.on_blocked)(event);
    }

    pub fn reset_if_paused(&self) {
        let mut state = self.state.write().unwrap();
        if *state == CircuitState::Paused {
            *state = CircuitState::Active;
        }
    }
}

pub fn classify_error(err_msg: &str) -> ErrorKind {
    if err_msg.is_empty() {
        return ErrorKind::None;
    }

    // Account Restricted regex
    if RESTRICTED_RE.is_match(err_msg) {
        return ErrorKind::AccountRestricted;
    }

    // Sign In Required regex
    if SIGN_IN_RE.is_match(err_msg) {
        return ErrorKind::SignInRequired;
    }

    // Rate Limited regex
    if RATE_LIMIT_RE.is_match(err_msg) {
        return ErrorKind::RateLimited;
    }

    // Status codes with boundary: prevents false positives from IMG_4012.jpg or 401000 bytes
    if let Some(caps) = STATUS_RE.captures(err_msg) {
        if let Some(m) = caps.get(1) {
            if let Ok(code) = m.as_str().parse::<u16>() {
                return match code {
                    401 | 403 => ErrorKind::SignInRequired,
                    429 => ErrorKind::RateLimited,
                    500..=599 => ErrorKind::Transient,
                    _ => ErrorKind::Fatal,
                };
            }
        }
    }

    // Transient network errors
    let transient_phrases = [
        "connection reset by peer",
        "connection refused",
        "i/o timeout",
        "unexpected eof",
        "broken pipe",
        "handshake failure",
        "no such host",
        "network is unreachable",
        "timed out",
    ];
    let lower = err_msg.to_lowercase();
    for phrase in &transient_phrases {
        if lower.contains(phrase) {
            return ErrorKind::Transient;
        }
    }

    ErrorKind::Fatal
}

pub fn is_retryable(err_msg: &str) -> bool {
    classify_error(err_msg) == ErrorKind::Transient
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_classification_and_false_positive_prevention() {
        assert_eq!(classify_error("request failed with status 401: Unauthorized"), ErrorKind::SignInRequired);
        assert_eq!(classify_error("HTTP 403: Forbidden"), ErrorKind::SignInRequired);
        assert_eq!(classify_error("request failed with status 429: Too Many Requests"), ErrorKind::RateLimited);
        assert_eq!(classify_error("HTTP 503: Service Unavailable"), ErrorKind::Transient);

        // Filename containing 4012 must NOT trip the circuit breaker!
        assert_eq!(classify_error("error reading /path/to/IMG_4012.jpg: permission denied"), ErrorKind::Fatal);
        assert_eq!(classify_error("stat /photos/IMG_401.jpg: file not found"), ErrorKind::Fatal);
        assert_eq!(classify_error("buffer overflow processing 401000 bytes"), ErrorKind::Fatal);

        // Broken pipe should be transient
        assert_eq!(classify_error("upload failed: broken pipe"), ErrorKind::Transient);
        assert!(is_retryable("upload failed: connection reset by peer"));
        assert!(!is_retryable("request failed with status 401: Unauthorized"));
    }
}
