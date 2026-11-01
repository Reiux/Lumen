package core

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
)

// CircuitState represents the state of the circuit breaker.
type CircuitState string

const (
	CircuitActive  CircuitState = "ACTIVE"
	CircuitPaused  CircuitState = "PAUSED"
	CircuitBlocked CircuitState = "BLOCKED"
)

// ErrorKind classifies account security and network failures.
type ErrorKind string

const (
	ErrorKindNone              ErrorKind = "NONE"
	ErrorKindSignInRequired    ErrorKind = "SIGN_IN_REQUIRED"
	ErrorKindAccountRestricted ErrorKind = "ACCOUNT_RESTRICTED"
	ErrorKindRateLimited       ErrorKind = "RATE_LIMITED"
	ErrorKindTransient         ErrorKind = "TRANSIENT"
	ErrorKindFatal             ErrorKind = "FATAL"
)

// AccountBlockedEvent is emitted to the UI and reporter when an account issue trips the breaker.
type AccountBlockedEvent struct {
	Reason   string `json:"reason"` // "SIGN_IN_REQUIRED", "ACCOUNT_RESTRICTED", "RATE_LIMITED"
	Email    string `json:"email"`
	Message  string `json:"message"`
	ResumeAt int64  `json:"resumeAt,omitempty"` // Unix ms timestamp when rate limit expires
}

// HTTPStatusError represents an explicit HTTP error with status code and body.
type HTTPStatusError struct {
	StatusCode int
	StatusText string
	Body       string
}

func (e *HTTPStatusError) Error() string {
	if e.Body != "" {
		return fmt.Sprintf("request failed with status %d: %s", e.StatusCode, e.Body)
	}
	return fmt.Sprintf("request failed with status %d: %s", e.StatusCode, e.StatusText)
}

// RateLimitError represents an HTTP 429 Too Many Requests error.
type RateLimitError struct {
	RetryAfter time.Duration
	Body       string
}

func (e *RateLimitError) Error() string {
	if e.Body != "" {
		return fmt.Sprintf("request failed with status 429: %s", e.Body)
	}
	return "request failed with status 429: Too Many Requests"
}

var (
	// Matches HTTP status codes only when preceded by status or HTTP, e.g. "status 401", "HTTP 403"
	// Prevents false positives from numbers in filenames (IMG_4012.jpg) or sizes.
	statusRegex = regexp.MustCompile(`(?i)(?:status(?:\s+code)?|HTTP)\s*[:=]?\s*([1-5]\d{2})\b`)

	// Keywords indicating account restriction
	accountRestrictedRegex = regexp.MustCompile(`(?i)\b(AccountDisabled|AccountDeleted|ServiceDisabled|DeviceManagementRequired)\b`)

	// Keywords indicating sign-in / re-auth required
	signInRequiredRegex = regexp.MustCompile(`(?i)\b(NeedsBrowser|BadAuthentication|WebLoginRequired|web\s+verification\s+challenge)\b`)

	// Keywords indicating rate limit
	rateLimitRegex = regexp.MustCompile(`(?i)\b(RateLimitExceeded|rate_limit_exceeded|TooManyRequests)\b`)
)

// ClassifyError inspects an error and returns its typed ErrorKind.
// It strictly parses HTTP status codes and known Google error codes,
// ensuring filenames like "IMG_4012.jpg" or file sizes never cause false positives.
func ClassifyError(err error) ErrorKind {
	if err == nil {
		return ErrorKindNone
	}

	// 1. Check for typed HTTPStatusError
	var httpErr *HTTPStatusError
	if errors.As(err, &httpErr) {
		return classifyStatusCodeAndBody(httpErr.StatusCode, httpErr.Body)
	}

	errMsg := err.Error()

	// 2. Check for Account Restricted keywords first (highest severity)
	if accountRestrictedRegex.MatchString(errMsg) {
		return ErrorKindAccountRestricted
	}

	// 3. Check for Sign-In Required keywords
	if signInRequiredRegex.MatchString(errMsg) {
		return ErrorKindSignInRequired
	}

	// 4. Check for Rate Limited keywords
	if rateLimitRegex.MatchString(errMsg) {
		return ErrorKindRateLimited
	}

	// 5. Check for HTTP status code using bounded regex
	matches := statusRegex.FindStringSubmatch(errMsg)
	if len(matches) == 2 {
		if code, parseErr := strconv.Atoi(matches[1]); parseErr == nil {
			return classifyStatusCodeAndBody(code, errMsg)
		}
	}

	// 6. Check for Transient network errors
	if isTransientNetworkError(err, errMsg) {
		return ErrorKindTransient
	}

	return ErrorKindFatal
}

func classifyStatusCodeAndBody(code int, body string) ErrorKind {
	switch {
	case code == 401 || code == 403:
		if accountRestrictedRegex.MatchString(body) {
			return ErrorKindAccountRestricted
		}
		return ErrorKindSignInRequired
	case code == 429:
		return ErrorKindRateLimited
	case code >= 500 && code < 600:
		return ErrorKindTransient
	default:
		return ErrorKindFatal
	}
}

func isTransientNetworkError(err error, errMsg string) bool {
	var netErr net.Error
	if errors.As(err, &netErr) {
		return true
	}
	transientPhrases := []string{
		"connection reset by peer",
		"connection refused",
		"i/o timeout",
		"unexpected EOF",
		"EOF",
		"broken pipe",
		"tls: handshake failure",
		"no such host",
		"temporary failure in name resolution",
		"network is unreachable",
	}
	for _, phrase := range transientPhrases {
		if strings.Contains(strings.ToLower(errMsg), strings.ToLower(phrase)) {
			return true
		}
	}
	return false
}

// IsRetryable returns whether an error should be retried in worker retry loops.
// Only TRANSIENT errors may be retried. SIGN_IN_REQUIRED and ACCOUNT_RESTRICTED must NEVER be retried.
func IsRetryable(err error) bool {
	return ClassifyError(err) == ErrorKindTransient
}

// ParseRetryAfter parses the Retry-After HTTP header value.
// It accepts integer seconds or an HTTP-date. Defaults to 15 minutes if missing or unparseable.
func ParseRetryAfter(header string) time.Duration {
	header = strings.TrimSpace(header)
	if header == "" {
		return 15 * time.Minute
	}
	if seconds, err := strconv.Atoi(header); err == nil && seconds > 0 {
		return time.Duration(seconds) * time.Second
	}
	if t, err := http.ParseTime(header); err == nil {
		diff := time.Until(t)
		if diff > 0 {
			return diff
		}
	}
	return 15 * time.Minute
}

// CircuitBreaker manages upload halting and cooldown when Google signals account issues.
type CircuitBreaker struct {
	mu           sync.RWMutex
	state        CircuitState
	blockedEvent *AccountBlockedEvent
	resumeAt     time.Time
	cancelFunc   context.CancelFunc
	onBlocked    func(event AccountBlockedEvent)
}

// NewCircuitBreaker creates a CircuitBreaker bound to an upload run context cancel func and notification callback.
func NewCircuitBreaker(cancelFunc context.CancelFunc, onBlocked func(event AccountBlockedEvent)) *CircuitBreaker {
	return &CircuitBreaker{
		state:      CircuitActive,
		cancelFunc: cancelFunc,
		onBlocked:  onBlocked,
	}
}

// State returns the current circuit state.
func (cb *CircuitBreaker) State() CircuitState {
	cb.mu.RLock()
	defer cb.mu.RUnlock()
	return cb.state
}

// IsBlocked reports whether the circuit breaker has tripped to BLOCKED.
func (cb *CircuitBreaker) IsBlocked() bool {
	cb.mu.RLock()
	defer cb.mu.RUnlock()
	return cb.state == CircuitBlocked
}

// Trip immediately transitions to BLOCKED, cancels worker contexts, and fires the alert callback.
func (cb *CircuitBreaker) Trip(kind ErrorKind, email, message string) {
	cb.mu.Lock()
	if cb.state == CircuitBlocked {
		cb.mu.Unlock()
		return
	}
	cb.state = CircuitBlocked
	event := AccountBlockedEvent{
		Reason:  string(kind),
		Email:   email,
		Message: message,
	}
	cb.blockedEvent = &event
	cancel := cb.cancelFunc
	callback := cb.onBlocked
	cb.mu.Unlock()

	if cancel != nil {
		cancel()
	}
	if callback != nil {
		callback(event)
	}
}

// Pause pauses the workers for the given cooldown duration (HTTP 429), emitting a rate limit event.
func (cb *CircuitBreaker) Pause(ctx context.Context, duration time.Duration, email, message string) {
	cb.mu.Lock()
	if cb.state == CircuitBlocked {
		cb.mu.Unlock()
		return
	}
	cb.state = CircuitPaused
	resumeAt := time.Now().Add(duration)
	cb.resumeAt = resumeAt
	event := AccountBlockedEvent{
		Reason:   string(ErrorKindRateLimited),
		Email:    email,
		Message:  message,
		ResumeAt: resumeAt.UnixMilli(),
	}
	callback := cb.onBlocked
	cb.mu.Unlock()

	if callback != nil {
		callback(event)
	}

	// Sleep until cooldown expires or context is cancelled
	select {
	case <-ctx.Done():
		return
	case <-time.After(duration):
	}

	cb.mu.Lock()
	if cb.state == CircuitPaused {
		cb.state = CircuitActive
	}
	cb.mu.Unlock()
}
