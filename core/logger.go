package core

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// Standard log levels
const (
	LogLevelDebug = "DEBUG"
	LogLevelInfo  = "INFO"
	LogLevelWarn  = "WARN"
	LogLevelError = "ERROR"
)

// Standard log categories
const (
	LogCategoryUpload  = "Upload"
	LogCategoryAuth    = "Auth"
	LogCategoryScanner = "Scanner"
	LogCategoryNetwork = "Network"
	LogCategorySystem  = "System"
)

// Standard upload outcomes
const (
	OutcomeUploaded            = "UPLOADED"
	OutcomeSkippedCache        = "SKIPPED_CACHE"
	OutcomeSkippedRemoteDedup  = "SKIPPED_REMOTE_DEDUP"
	OutcomeRecoveredCommit     = "RECOVERED_COMMIT"
	OutcomeFailed              = "FAILED"
)

// Standard reason codes
const (
	ReasonRateLimited    = "RATE_LIMITED"
	ReasonAuthExpired    = "AUTH_EXPIRED"
	ReasonAccountBlocked = "ACCOUNT_RESTRICTED"
	ReasonNetworkTimeout = "NETWORK_TIMEOUT"
	ReasonFileUnreadable = "FILE_UNREADABLE"
	ReasonServerError    = "SERVER_ERROR"
)

// LogEntry is a structured event recording an operation or problem.
type LogEntry struct {
	ID              string    `json:"id"`
	Timestamp       time.Time `json:"timestamp"`
	Level           string    `json:"level"`           // DEBUG, INFO, WARN, ERROR
	Category        string    `json:"category"`        // Upload, Auth, Scanner, Network, System
	RunID           string    `json:"runId,omitempty"` // e.g. "Run 1"
	FileName        string    `json:"fileName,omitempty"`
	FilePath        string    `json:"filePath,omitempty"`
	SizeBytes       int64     `json:"sizeBytes,omitempty"`
	DurationMs      int64     `json:"durationMs,omitempty"`
	Outcome         string    `json:"outcome,omitempty"`         // UPLOADED, SKIPPED_CACHE, SKIPPED_REMOTE_DEDUP, RECOVERED_COMMIT, FAILED
	ReasonCode      string    `json:"reasonCode,omitempty"`      // RATE_LIMITED, AUTH_EXPIRED, NETWORK_TIMEOUT, FILE_UNREADABLE, SERVER_ERROR
	UserMessage     string    `json:"userMessage,omitempty"`     // Plain-language user explanation
	SuggestedAction string    `json:"suggestedAction,omitempty"` // Actionable advice
	RawError        string    `json:"rawError,omitempty"`        // Technical error details
}

var (
	idCounter uint64
	runSeq    uint64
)

// NextRunID returns an incrementing identifier for upload runs (e.g. "Run 1", "Run 2").
func NextRunID() string {
	num := atomic.AddUint64(&runSeq, 1)
	return fmt.Sprintf("Run %d", num)
}

func nextLogID() string {
	val := atomic.AddUint64(&idCounter, 1)
	return fmt.Sprintf("log_%d_%d", time.Now().UnixMilli(), val)
}

// LogListener receives log entries in real-time.
type LogListener func(entry LogEntry)

// LogBuffer is a bounded, thread-safe ring buffer for structured log entries.
type LogBuffer struct {
	mu        sync.RWMutex
	capacity  int
	entries   []LogEntry
	listeners []LogListener
}

const DefaultLogCapacity = 2000

var (
	globalLoggerMu sync.RWMutex
	globalLogger   *LogBuffer
)

func init() {
	globalLogger = NewLogBuffer(DefaultLogCapacity)
}

// NewLogBuffer creates a new LogBuffer with the specified maximum capacity.
func NewLogBuffer(capacity int) *LogBuffer {
	if capacity <= 0 {
		capacity = DefaultLogCapacity
	}
	return &LogBuffer{
		capacity:  capacity,
		entries:   make([]LogEntry, 0, min(capacity, 256)),
		listeners: make([]LogListener, 0, 2),
	}
}

// GetGlobalLogger returns the shared log buffer instance.
func GetGlobalLogger() *LogBuffer {
	globalLoggerMu.RLock()
	defer globalLoggerMu.RUnlock()
	return globalLogger
}

// SetGlobalLogger overrides the shared log buffer instance (primarily for tests).
func SetGlobalLogger(lb *LogBuffer) {
	globalLoggerMu.Lock()
	defer globalLoggerMu.Unlock()
	globalLogger = lb
}

// AddListener registers a real-time event listener.
func (b *LogBuffer) AddListener(listener LogListener) {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.listeners = append(b.listeners, listener)
}

// AddEntry stores an entry in the buffer, dropping the oldest entry if capacity is reached.
// It also sanitizes credentials and sensitive tokens from the entry.
func (b *LogBuffer) AddEntry(entry LogEntry) {
	if entry.ID == "" {
		entry.ID = nextLogID()
	}
	if entry.Timestamp.IsZero() {
		entry.Timestamp = time.Now()
	}

	// Redact sensitive patterns from RawError, UserMessage, and FilePath
	entry.RawError = RedactSecrets(entry.RawError)
	entry.UserMessage = RedactSecrets(entry.UserMessage)
	entry.SuggestedAction = RedactSecrets(entry.SuggestedAction)

	b.mu.Lock()
	if len(b.entries) >= b.capacity {
		// Drop the oldest entry
		b.entries = append(b.entries[1:], entry)
	} else {
		b.entries = append(b.entries, entry)
	}
	listeners := append([]LogListener(nil), b.listeners...)
	b.mu.Unlock()

	for _, l := range listeners {
		l(entry)
	}
}

// GetEntries returns log entries in reverse chronological order (newest first),
// optionally filtered by level and limited by count.
func (b *LogBuffer) GetEntries(limit int, levelFilter string) []LogEntry {
	b.mu.RLock()
	defer b.mu.RUnlock()

	levelFilter = strings.ToUpper(strings.TrimSpace(levelFilter))
	var result []LogEntry

	// Walk backwards for newest first
	for i := len(b.entries) - 1; i >= 0; i-- {
		entry := b.entries[i]
		if levelFilter != "" && levelFilter != "ALL" && entry.Level != levelFilter {
			continue
		}
		result = append(result, entry)
		if limit > 0 && len(result) >= limit {
			break
		}
	}

	return result
}

// GetProblems returns only WARN and ERROR entries (newest first).
func (b *LogBuffer) GetProblems() []LogEntry {
	b.mu.RLock()
	defer b.mu.RUnlock()

	var result []LogEntry
	for i := len(b.entries) - 1; i >= 0; i-- {
		entry := b.entries[i]
		if entry.Level == LogLevelWarn || entry.Level == LogLevelError {
			result = append(result, entry)
		}
	}
	return result
}

// Clear flushes all entries from the ring buffer.
func (b *LogBuffer) Clear() {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.entries = b.entries[:0]
}

// Count returns the number of entries currently stored.
func (b *LogBuffer) Count() int {
	b.mu.RLock()
	defer b.mu.RUnlock()
	return len(b.entries)
}

// Export formats all stored entries into redacted plaintext or JSON for user sharing.
func (b *LogBuffer) Export(format string) string {
	b.mu.RLock()
	entries := append([]LogEntry(nil), b.entries...)
	b.mu.RUnlock()

	if strings.ToLower(strings.TrimSpace(format)) == "json" {
		data, err := json.MarshalIndent(entries, "", "  ")
		if err == nil {
			return string(data)
		}
	}

	var sb strings.Builder
	sb.WriteString(fmt.Sprintf("=== Lumen Activity Log Export (%s) ===\n\n", time.Now().Format(time.RFC3339)))
	for _, e := range entries {
		sb.WriteString(fmt.Sprintf("[%s] [%s] [%s]", e.Timestamp.Format("2006-01-02 15:04:05"), e.Level, e.Category))
		if e.RunID != "" {
			sb.WriteString(fmt.Sprintf(" [%s]", e.RunID))
		}
		if e.FileName != "" {
			sb.WriteString(fmt.Sprintf(" file=%s", e.FileName))
		}
		if e.Outcome != "" {
			sb.WriteString(fmt.Sprintf(" outcome=%s", e.Outcome))
		}
		if e.ReasonCode != "" {
			sb.WriteString(fmt.Sprintf(" reason=%s", e.ReasonCode))
		}
		if e.UserMessage != "" {
			sb.WriteString(fmt.Sprintf(" - %s", e.UserMessage))
		}
		if e.SuggestedAction != "" {
			sb.WriteString(fmt.Sprintf(" (Action: %s)", e.SuggestedAction))
		}
		if e.RawError != "" {
			sb.WriteString(fmt.Sprintf(" details: %s", e.RawError))
		}
		sb.WriteString("\n")
	}
	return sb.String()
}

// ProblemDetails maps an error to plain-language explanation and recommended action.
type ProblemDetails struct {
	ReasonCode      string
	UserMessage     string
	SuggestedAction string
}

// MapErrorToProblem categorizes an error into a user-friendly ProblemDetails struct.
func MapErrorToProblem(err error) ProblemDetails {
	if err == nil {
		return ProblemDetails{}
	}

	kind := ClassifyError(err)
	switch kind {
	case ErrorKindSignInRequired:
		return ProblemDetails{
			ReasonCode:      ReasonAuthExpired,
			UserMessage:     "Google rejected credentials or your sign-in session expired.",
			SuggestedAction: "Open Settings and sign in to reconnect your Google account.",
		}
	case ErrorKindAccountRestricted:
		return ProblemDetails{
			ReasonCode:      ReasonAccountBlocked,
			UserMessage:     "Google has restricted or disabled this account from uploading.",
			SuggestedAction: "Check your account status directly at myaccount.google.com; do not retry immediately.",
		}
	case ErrorKindRateLimited:
		return ProblemDetails{
			ReasonCode:      ReasonRateLimited,
			UserMessage:     "Google upload servers requested a temporary rate-limit cooldown.",
			SuggestedAction: "Uploads will automatically resume when the cooldown timer finishes.",
		}
	case ErrorKindTransient:
		return ProblemDetails{
			ReasonCode:      ReasonNetworkTimeout,
			UserMessage:     "Network connection to Google Photos was interrupted or timed out.",
			SuggestedAction: "Check your internet connection or proxy settings; Lumen will retry automatically.",
		}
	}

	// File access / OS errors
	if errors.Is(err, os.ErrNotExist) {
		return ProblemDetails{
			ReasonCode:      ReasonFileUnreadable,
			UserMessage:     "File not found on local disk.",
			SuggestedAction: "Ensure the file still exists in the specified folder.",
		}
	}
	if errors.Is(err, os.ErrPermission) {
		return ProblemDetails{
			ReasonCode:      ReasonFileUnreadable,
			UserMessage:     "Permission denied reading local file.",
			SuggestedAction: "Check file permissions and ensure another application does not have the file locked.",
		}
	}

	return ProblemDetails{
		ReasonCode:      ReasonServerError,
		UserMessage:     "Encountered an unexpected error communicating with Google Photos.",
		SuggestedAction: "Check your local file and retry, or review the detailed log below.",
	}
}

var (
	// Redact sensitive patterns: OAuth cookies, master tokens, emails, home dirs
	oauthTokenRegex = regexp.MustCompile(`(?i)(oauth_token=)[^&\s]+`)
	masterTokenRegex = regexp.MustCompile(`(?i)(Token=)[^&\s]+`)
	bearerRegex      = regexp.MustCompile(`(?i)(Bearer\s+)[A-Za-z0-9_\-\.]+`)
)

// RedactSecrets removes sensitive credentials, tokens, and authorization headers from text.
func RedactSecrets(text string) string {
	if text == "" {
		return ""
	}
	text = oauthTokenRegex.ReplaceAllString(text, "${1}[REDACTED]")
	text = masterTokenRegex.ReplaceAllString(text, "${1}[REDACTED]")
	text = bearerRegex.ReplaceAllString(text, "${1}[REDACTED]")
	return text
}

// SanitizeUserPath redacts the home directory prefix to preserve local username privacy.
func SanitizeUserPath(path string) string {
	if path == "" {
		return ""
	}
	home, err := os.UserHomeDir()
	if err == nil && home != "" && strings.HasPrefix(path, home) {
		return "~" + strings.TrimPrefix(path, home)
	}
	return filepath.Clean(path)
}
