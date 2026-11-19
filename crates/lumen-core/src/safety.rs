use std::sync::{Arc, LazyLock, RwLock};
use std::time::{Duration, Instant};
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
    pause_until: Arc<RwLock<Option<Instant>>>,
    blocked_event: Arc<RwLock<Option<AccountBlockedEvent>>>,
    on_blocked: Arc<dyn Fn(AccountBlockedEvent) + Send + Sync>,
}

fn error_kind_reason(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::None => "NONE",
        ErrorKind::SignInRequired => "SIGN_IN_REQUIRED",
        ErrorKind::AccountRestricted => "ACCOUNT_RESTRICTED",
        ErrorKind::RateLimited => "RATE_LIMITED",
        ErrorKind::Transient => "TRANSIENT",
        ErrorKind::Fatal => "FATAL",
    }
}

impl CircuitBreaker {
    pub fn new<F>(on_blocked: F) -> Self
    where
        F: Fn(AccountBlockedEvent) + Send + Sync + 'static,
    {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Active)),
            pause_until: Arc::new(RwLock::new(None)),
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

    pub fn pause_remaining(&self) -> Option<Duration> {
        let now = Instant::now();
        {
            let state = self.state.read().unwrap();
            if *state != CircuitState::Paused {
                return None;
            }
            let until = (*self.pause_until.read().unwrap())?;
            if until > now {
                return Some(until.duration_since(now));
            }
        }

        // Re-check under write locks before resetting so a concurrent pause() isn't clobbered
        let mut state = self.state.write().unwrap();
        if *state != CircuitState::Paused {
            return None;
        }
        let mut pause_until = self.pause_until.write().unwrap();
        let now = Instant::now();
        match *pause_until {
            Some(until) if until > now => Some(until.duration_since(now)),
            _ => {
                *state = CircuitState::Active;
                *pause_until = None;
                None
            }
        }
    }

    pub fn trip(&self, kind: ErrorKind, email: &str, message: &str) {
        let event = {
            let mut state = self.state.write().unwrap();
            if *state == CircuitState::Blocked {
                return;
            }
            *state = CircuitState::Blocked;
            *self.pause_until.write().unwrap() = None;

            let event = AccountBlockedEvent {
                reason: error_kind_reason(kind).to_string(),
                email: email.to_string(),
                message: message.to_string(),
                resume_at: None,
            };
            *self.blocked_event.write().unwrap() = Some(event.clone());
            event
        };
        (self.on_blocked)(event);
    }

    pub fn pause(&self, duration: Duration, email: &str, message: &str) {
        let event = {
            let mut state = self.state.write().unwrap();
            if *state == CircuitState::Blocked {
                return;
            }
            let now = Instant::now();
            let candidate_until = now + duration;
            let mut pause_until = self.pause_until.write().unwrap();
            let already_actively_paused = *state == CircuitState::Paused
                && pause_until.is_some_and(|existing| existing > now);
            *state = CircuitState::Paused;
            let effective_until = match *pause_until {
                Some(existing) if existing > candidate_until => existing,
                _ => candidate_until,
            };
            *pause_until = Some(effective_until);

            let effective_duration = effective_until.saturating_duration_since(now);
            let resume_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64
                + effective_duration.as_millis() as i64;

            let event = AccountBlockedEvent {
                reason: "RATE_LIMITED".to_string(),
                email: email.to_string(),
                message: message.to_string(),
                resume_at: Some(resume_at),
            };
            *self.blocked_event.write().unwrap() = Some(event.clone());
            if already_actively_paused {
                None
            } else {
                Some(event)
            }
        };
        if let Some(event) = event {
            (self.on_blocked)(event);
        }
    }

    pub fn reset_if_paused(&self) {
        let mut state = self.state.write().unwrap();
        if *state == CircuitState::Paused {
            *state = CircuitState::Active;
            *self.pause_until.write().unwrap() = None;
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
        "network error:",
        "error sending request",
        "error decoding response body",
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

pub fn is_within_schedule_at_hour(current: u8, start_hour: u8, end_hour: u8) -> bool {
    if start_hour == end_hour {
        return true;
    }
    if start_hour < end_hour {
        current >= start_hour && current < end_hour
    } else {
        // Wraps over midnight (e.g. 23:00 to 07:00)
        current >= start_hour || current < end_hour
    }
}

pub fn is_within_schedule(start_hour: u8, end_hour: u8) -> bool {
    use chrono::Timelike;
    let now = chrono::Local::now();
    is_within_schedule_at_hour(now.hour() as u8, start_hour, end_hour)
}

#[cfg(windows)]
#[repr(C)]
struct SystemPowerStatus {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
    battery_full_life_time: u32,
}

#[cfg(windows)]
unsafe extern "system" {
    fn GetSystemPowerStatus(lp_system_power_status: *mut SystemPowerStatus) -> i32;
}

#[cfg(windows)]
fn get_windows_power_status() -> Option<SystemPowerStatus> {
    let mut status = SystemPowerStatus {
        ac_line_status: 255,
        battery_flag: 255,
        battery_life_percent: 255,
        system_status_flag: 0,
        battery_life_time: u32::MAX,
        battery_full_life_time: u32::MAX,
    };
    let ok = unsafe { GetSystemPowerStatus(&mut status) };
    if ok != 0 {
        Some(status)
    } else {
        None
    }
}

#[cfg(target_os = "linux")]
fn is_system_battery_supply(path: &std::path::Path) -> bool {
    let Ok(t) = std::fs::read_to_string(path.join("type")) else {
        return false;
    };
    if !t.trim().eq_ignore_ascii_case("Battery") {
        return false;
    }
    // Skip peripheral batteries (wireless mice, keyboards, headsets report scope = "Device")
    if let Ok(scope) = std::fs::read_to_string(path.join("scope")) {
        if scope.trim().eq_ignore_ascii_case("Device") {
            return false;
        }
    }
    if let Ok(present) = std::fs::read_to_string(path.join("present")) {
        if present.trim() == "0" {
            return false;
        }
    }
    true
}

/// Returns true if the machine has a system battery (e.g., laptop/tablet),
/// false on desktop PCs without a system battery.
pub fn has_battery() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                if is_system_battery_supply(&entry.path()) {
                    return true;
                }
            }
        }
        false
    }
    #[cfg(windows)]
    {
        if let Some(status) = get_windows_power_status() {
            // 128 = no system battery, 255 = unknown status
            return (status.battery_flag & 128) == 0 && status.battery_flag != 255;
        }
        false
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(out) = std::process::Command::new("/usr/bin/pmset")
            .args(["-g", "batt"])
            .output()
        {
            let stdout = String::from_utf8_lossy(&out.stdout);
            return stdout.contains("InternalBattery");
        }
        false
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        false
    }
}

#[cfg(any(target_os = "macos", test))]
fn parse_macos_pmset_on_battery(stdout: &str) -> bool {
    stdout.contains("'Battery Power'")
}

#[cfg(target_os = "macos")]
static MACOS_BATTERY_CACHE: LazyLock<std::sync::Mutex<Option<(Instant, bool)>>> =
    LazyLock::new(|| std::sync::Mutex::new(None));

pub fn is_running_on_battery() -> bool {
    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
            for entry in entries.flatten() {
                let path = entry.path();
                if is_system_battery_supply(&path) {
                    if let Ok(s) = std::fs::read_to_string(path.join("status")) {
                        if s.trim().eq_ignore_ascii_case("Discharging") {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
    #[cfg(windows)]
    {
        if let Some(status) = get_windows_power_status() {
            let has_batt = (status.battery_flag & 128) == 0 && status.battery_flag != 255;
            // ac_line_status == 0 means offline (running on battery)
            return has_batt && status.ac_line_status == 0;
        }
        false
    }
    #[cfg(target_os = "macos")]
    {
        const CACHE_TTL: Duration = Duration::from_secs(5);
        if let Ok(guard) = MACOS_BATTERY_CACHE.lock() {
            if let Some((checked_at, cached)) = *guard {
                if checked_at.elapsed() < CACHE_TTL {
                    return cached;
                }
            }
        }
        let on_battery = std::process::Command::new("/usr/bin/pmset")
            .args(["-g", "batt"])
            .output()
            .ok()
            .map(|out| parse_macos_pmset_on_battery(&String::from_utf8_lossy(&out.stdout)))
            .unwrap_or(false);
        if let Ok(mut guard) = MACOS_BATTERY_CACHE.lock() {
            *guard = Some((Instant::now(), on_battery));
        }
        on_battery
    }
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    {
        false
    }
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

    #[test]
    fn test_schedule_hours_matching_and_wrapping() {
        // Normal window (1am to 7am)
        assert!(!is_within_schedule_at_hour(0, 1, 7));
        assert!(is_within_schedule_at_hour(1, 1, 7));
        assert!(is_within_schedule_at_hour(6, 1, 7));
        assert!(!is_within_schedule_at_hour(7, 1, 7));
        assert!(!is_within_schedule_at_hour(12, 1, 7));

        // Midnight wrapping window (22:00 to 06:00)
        assert!(!is_within_schedule_at_hour(21, 22, 6));
        assert!(is_within_schedule_at_hour(22, 22, 6));
        assert!(is_within_schedule_at_hour(23, 22, 6));
        assert!(is_within_schedule_at_hour(0, 22, 6));
        assert!(is_within_schedule_at_hour(5, 22, 6));
        assert!(!is_within_schedule_at_hour(6, 22, 6));

        // Equal start and end means anytime
        assert!(is_within_schedule_at_hour(0, 5, 5));
        assert!(is_within_schedule_at_hour(12, 5, 5));
    }

    #[test]
    fn test_circuit_breaker_emits_expected_reason_codes() {
        let seen = Arc::new(RwLock::new(Vec::<String>::new()));
        let seen_cb = seen.clone();
        let cb = CircuitBreaker::new(move |ev| {
            seen_cb.write().unwrap().push(ev.reason);
        });
        cb.trip(ErrorKind::SignInRequired, "a@example.com", "401");
        assert_eq!(seen.read().unwrap().as_slice(), &["SIGN_IN_REQUIRED"]);

        let seen2 = Arc::new(RwLock::new(Vec::<String>::new()));
        let seen2_cb = seen2.clone();
        let cb2 = CircuitBreaker::new(move |ev| {
            seen2_cb.write().unwrap().push(ev.reason);
        });
        cb2.trip(ErrorKind::AccountRestricted, "a@example.com", "AccountDisabled");
        assert_eq!(seen2.read().unwrap().as_slice(), &["ACCOUNT_RESTRICTED"]);

        assert!(parse_macos_pmset_on_battery("Now drawing from 'Battery Power'\n -InternalBattery-0\t95%; discharging"));
        assert!(!parse_macos_pmset_on_battery("Now drawing from 'AC Power'\n -InternalBattery-0\t100%; charged"));
    }
}
