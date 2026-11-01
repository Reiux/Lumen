package core

import (
	"context"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"
)

// TokenFetchFunc fetches a fresh bearer token and expiry from Google's auth endpoint
// for the given credentials.
type TokenFetchFunc func(ctx context.Context, email string) (token string, expiresAt time.Time, err error)

// TokenManager provides thread-safe access to cached Google Photos OAuth tokens,
// ensuring concurrent workers share a single refreshed token and prevent thundering herds.
type TokenManager struct {
	mu           sync.RWMutex
	accessToken  string
	expiresAt    time.Time
	accountEmail string
	fetcher      TokenFetchFunc
}

// NewTokenManager creates a TokenManager for an account with a custom or default fetcher.
func NewTokenManager(accountEmail string, fetcher TokenFetchFunc) *TokenManager {
	if fetcher == nil {
		fetcher = defaultTokenFetcher
	}
	return &TokenManager{
		accountEmail: strings.TrimSpace(accountEmail),
		fetcher:      fetcher,
	}
}

// GetToken returns a valid bearer token. If the cached token is valid (with a 5-minute
// safety margin), it is returned immediately. Otherwise, it acquires a write lock,
// double-checks token freshness, refreshes the token, updates the cache, and returns.
func (tm *TokenManager) GetToken(ctx context.Context) (string, error) {
	// 1. Fast read-lock path
	tm.mu.RLock()
	if tm.isValidLocked() {
		token := tm.accessToken
		tm.mu.RUnlock()
		return token, nil
	}
	tm.mu.RUnlock()

	// 2. Slow write-lock path with double-checked locking
	tm.mu.Lock()
	defer tm.mu.Unlock()

	// Check context cancellation
	if ctx.Err() != nil {
		return "", ctx.Err()
	}

	// Double-check: another goroutine might have refreshed the token while we waited for Lock
	if tm.isValidLocked() {
		return tm.accessToken, nil
	}

	if tm.fetcher == nil {
		return "", errors.New("no token fetcher configured")
	}

	token, expiresAt, err := tm.fetcher(ctx, tm.accountEmail)
	if err != nil {
		return "", fmt.Errorf("refresh token for %s: %w", tm.accountEmail, err)
	}

	tm.accessToken = token
	tm.expiresAt = expiresAt
	return token, nil
}

// isValidLocked checks token freshness under RLock or Lock with a 5-minute safety margin.
func (tm *TokenManager) isValidLocked() bool {
	if tm.accessToken == "" {
		return false
	}
	// Valid only if there are at least 5 minutes remaining before expiry
	return time.Now().Add(5 * time.Minute).Before(tm.expiresAt)
}

// InvalidateToken forces the cached token to expire so the next request will refresh.
func (tm *TokenManager) InvalidateToken(accountEmail string) {
	tm.mu.Lock()
	defer tm.mu.Unlock()
	if accountEmail == "" || strings.EqualFold(accountEmail, tm.accountEmail) {
		tm.accessToken = ""
		tm.expiresAt = time.Time{}
	}
}

// AccountEmail returns the account email associated with this TokenManager.
func (tm *TokenManager) AccountEmail() string {
	tm.mu.RLock()
	defer tm.mu.RUnlock()
	return tm.accountEmail
}

// defaultTokenFetcher retrieves credentials from KeyringStorage and requests a token from Google.
func defaultTokenFetcher(ctx context.Context, email string) (string, time.Time, error) {
	cred, err := GetCredentialForAccount(email)
	if err != nil {
		return "", time.Time{}, fmt.Errorf("retrieve credentials for %s: %w", email, err)
	}

	api, err := newAPIFromCredential(cred, currentConfig().Preferences.Proxy)
	if err != nil {
		return "", time.Time{}, fmt.Errorf("create auth client: %w", err)
	}

	authResp, err := api.getAuthToken()
	if err != nil {
		return "", time.Time{}, err
	}

	token := authResp["Auth"]
	if token == "" {
		return "", time.Time{}, errors.New("auth response missing Auth token")
	}

	expiryStr := authResp["Expiry"]
	expirySec, err := strconv.ParseInt(expiryStr, 10, 64)
	var expiresAt time.Time
	if err == nil && expirySec > 0 {
		expiresAt = time.Unix(expirySec, 0)
	} else {
		expiresAt = time.Now().Add(1 * time.Hour)
	}

	return token, expiresAt, nil
}
