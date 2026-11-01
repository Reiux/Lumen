package core

import (
	"errors"
	"fmt"
	"log/slog"
	"strings"
	"time"
)

// AlbumError represents an album creation error
type AlbumError struct {
	AlbumName string `json:"AlbumName"`
	Error     string `json:"Error"`
}

const (
	// AlbumBatchSize is the maximum number of items per API call
	AlbumBatchSize = 500
	// AlbumLimit is the maximum number of items per album
	AlbumLimit = 20000
)

// AlbumStatus represents the status of album creation
type AlbumStatus struct {
	AlbumName  string   `json:"AlbumName"`
	ItemsAdded int      `json:"ItemsAdded"`
	TotalItems int      `json:"TotalItems"`
	AlbumKeys  []string `json:"AlbumKeys"`
	IsComplete bool     `json:"IsComplete"`
}

// AlbumAPI is an interface satisfied by *Api or test mocks for album operations.
type AlbumAPI interface {
	CreateAlbum(albumName string, mediaKeys []string) (string, error)
	AddMediaToAlbum(albumMediaKey string, mediaKeys []string) error
}

// AlbumManager handles album creation, reuse, batching, and serialization.
type AlbumManager struct {
	api          AlbumAPI
	accountEmail string
	registry     *AlbumRegistry
	reporter     UploadReporter
	logger       *slog.Logger
	cancel       <-chan struct{}
}

// NewAlbumManager creates a new AlbumManager.
func NewAlbumManager(api AlbumAPI, reporter UploadReporter, logger *slog.Logger, cancel <-chan struct{}) *AlbumManager {
	account := ""
	if accGetter, ok := api.(interface{ AccountEmail() string }); ok {
		account = accGetter.AccountEmail()
	}
	return &AlbumManager{
		api:          api,
		accountEmail: account,
		registry:     GetGlobalAlbumRegistry(),
		reporter:     reporter,
		logger:       logger,
		cancel:       cancel,
	}
}

// SetAccountEmail overrides the associated account email for album registry indexing.
func (m *AlbumManager) SetAccountEmail(email string) {
	m.accountEmail = email
}

// SetRegistry overrides the AlbumRegistry instance (useful for unit tests).
func (m *AlbumManager) SetRegistry(registry *AlbumRegistry) {
	m.registry = registry
}

func (m *AlbumManager) getRegistry() *AlbumRegistry {
	if m.registry != nil {
		return m.registry
	}
	return GetGlobalAlbumRegistry()
}

func (m *AlbumManager) getAccountEmail() string {
	if m.accountEmail != "" {
		return m.accountEmail
	}
	if accGetter, ok := m.api.(interface{ AccountEmail() string }); ok {
		return accGetter.AccountEmail()
	}
	return ""
}

// isCancelled checks if cancellation has been requested.
func (m *AlbumManager) isCancelled() bool {
	if m.cancel == nil {
		return false
	}
	select {
	case <-m.cancel:
		return true
	default:
		return false
	}
}

// IsAlbumKey checks if the input looks like an album media key (starts with "AF1Qip").
func IsAlbumKey(input string) bool {
	return len(input) > 6 && strings.HasPrefix(input, "AF1Qip")
}

// isAlbumLink checks if the input starts with http:// or https://.
func isAlbumLink(input string) bool {
	return strings.HasPrefix(input, "http://") || strings.HasPrefix(input, "https://")
}

// ParseAlbumLink extracts the album media key (starts with "AF1Qip") from a Google Photos album link or raw key.
// Returns an error if the input is an invalid link, missing the key, or empty.
func ParseAlbumLink(input string) (string, error) {
	trimmed := strings.TrimSpace(input)
	if trimmed == "" {
		return "", errors.New("album link cannot be empty")
	}

	// 1. Raw album key
	if IsAlbumKey(trimmed) {
		key := extractCleanKey(trimmed)
		if IsAlbumKey(key) {
			return key, nil
		}
	}

	// 2. HTTP / HTTPS link
	if isAlbumLink(trimmed) {
		idx := strings.Index(trimmed, "AF1Qip")
		if idx == -1 {
			return "", errors.New("invalid Google Photos album link: no album key found (expected AF1Qip...)")
		}
		key := extractCleanKey(trimmed[idx:])
		if !IsAlbumKey(key) {
			return "", errors.New("invalid Google Photos album link: malformed album key")
		}
		return key, nil
	}

	return "", errors.New("input is not a valid Google Photos album link or key")
}

func extractCleanKey(raw string) string {
	for i, r := range raw {
		if r == '?' || r == '#' || r == '/' || r == '&' || r == ' ' || r == '\t' || r == '\n' || r == '\r' {
			return raw[:i]
		}
	}
	return raw
}

func isAlbumNotFoundError(err error) bool {
	if err == nil {
		return false
	}
	s := strings.ToLower(err.Error())
	return strings.Contains(s, "not found") ||
		strings.Contains(s, "not_found") ||
		strings.Contains(s, "404") ||
		strings.Contains(s, "deleted") ||
		strings.Contains(s, "unavailable") ||
		strings.Contains(s, "does not exist") ||
		strings.Contains(s, "invalid album")
}

// AddToAlbum adds media items to an album with proper batching and persistence:
// - If albumNameOrKey is an album key (starts with AF1Qip) or a Google Photos album link, adds to existing album
// - Otherwise looks up the per-account registry; if found, adds to that album
// - If not found, creates the album once, saves name->key in registry, and adds media
// - Serializes get-or-create per (account, name) with mutex/singleflight
// - If adding fails with album not found/deleted, deletes from registry and recreates once
// - Keeps 20,000-item split behavior and numbered names, registering each numbered album
// Returns a list of album media keys for all created/used albums.
func (m *AlbumManager) AddToAlbum(mediaKeys []string, albumNameOrKey string) ([]string, error) {
	if len(mediaKeys) == 0 {
		return nil, fmt.Errorf("no media keys provided")
	}

	albumNameOrKey = strings.TrimSpace(albumNameOrKey)
	if albumNameOrKey == "" {
		return nil, fmt.Errorf("album name or key cannot be empty")
	}

	// (a) If the name is an album key or Google Photos album link, extract and use key
	if isAlbumLink(albumNameOrKey) {
		key, err := ParseAlbumLink(albumNameOrKey)
		if err != nil {
			return nil, err
		}
		return m.addToExistingAlbum(mediaKeys, key)
	}
	if IsAlbumKey(albumNameOrKey) {
		cleanKey := extractCleanKey(albumNameOrKey)
		return m.addToExistingAlbum(mediaKeys, cleanKey)
	}

	// (f) 20,000-item split behavior and numbered names, registering each numbered album
	totalItems := len(mediaKeys)
	itemsAdded := 0
	var allAlbumKeys []string
	albumCounter := 1

	if len(mediaKeys) > AlbumLimit && m.logger != nil {
		m.logger.Warn(fmt.Sprintf("%d items exceed the album limit of %d. They will be split into multiple albums.", len(mediaKeys), AlbumLimit))
	}

	for i := 0; i < len(mediaKeys); i += AlbumLimit {
		if m.isCancelled() {
			return allAlbumKeys, fmt.Errorf("album creation cancelled (added %d/%d items)", itemsAdded, totalItems)
		}

		end := min(i+AlbumLimit, len(mediaKeys))
		chunk := mediaKeys[i:end]

		currentAlbumName := albumNameOrKey
		if len(mediaKeys) > AlbumLimit {
			currentAlbumName = fmt.Sprintf("%s (%d)", albumNameOrKey, albumCounter)
		}

		key, err := m.getOrCreateAndAddToAlbum(chunk, currentAlbumName)
		if err != nil {
			return allAlbumKeys, fmt.Errorf("failed to process album '%s' (added %d/%d items): %w", currentAlbumName, itemsAdded, totalItems, err)
		}

		allAlbumKeys = append(allAlbumKeys, key)
		itemsAdded += len(chunk)
		albumCounter++
	}

	if m.reporter != nil {
		m.reporter.AlbumComplete(AlbumStatus{
			AlbumName:  albumNameOrKey,
			ItemsAdded: itemsAdded,
			TotalItems: totalItems,
			AlbumKeys:  allAlbumKeys,
			IsComplete: true,
		})
	}

	return allAlbumKeys, nil
}

// getOrCreateAndAddToAlbum gets or creates an album for (account, albumName) under serialized lock.
func (m *AlbumManager) getOrCreateAndAddToAlbum(mediaKeys []string, albumName string) (string, error) {
	account := m.getAccountEmail()
	lock := globalKeyedAlbumLock.get(account, albumName)
	lock.Lock()
	defer lock.Unlock()

	reg := m.getRegistry()

	// (b) Look up the registry; if found, add to that album
	if entry, found := reg.Get(account, albumName); found {
		err := m.addMediaInBatches(entry.AlbumKey, albumName, mediaKeys)
		if err == nil {
			return entry.AlbumKey, nil
		}

		// (e) If add fails with "album not found"/deleted, remove registry entry and recreate once
		if isAlbumNotFoundError(err) {
			if m.logger != nil {
				m.logger.Warn(fmt.Sprintf("Album '%s' (key %s) not found; recreating once.", albumName, entry.AlbumKey))
			}
			_ = reg.Delete(account, albumName)
			return m.createAndAddInBatches(mediaKeys, albumName)
		}

		return entry.AlbumKey, err
	}

	// (c) If not found, create album ONCE, save name->key in registry, add
	return m.createAndAddInBatches(mediaKeys, albumName)
}

// createAndAddInBatches creates a new album with the first batch, registers it, and adds any remainder.
func (m *AlbumManager) createAndAddInBatches(mediaKeys []string, albumName string) (string, error) {
	reg := m.getRegistry()
	account := m.getAccountEmail()
	totalItems := len(mediaKeys)
	itemsAdded := 0

	// First batch creates the album
	firstEnd := min(AlbumBatchSize, len(mediaKeys))
	firstBatch := mediaKeys[:firstEnd]

	albumKey, err := m.createAlbumWithRetry(albumName, firstBatch)
	if err != nil {
		return "", err
	}
	itemsAdded += len(firstBatch)

	// Save name -> key in registry immediately
	_ = reg.Set(account, albumName, albumKey)

	if m.reporter != nil {
		m.reporter.AlbumProgress(AlbumStatus{
			AlbumName:  albumName,
			ItemsAdded: itemsAdded,
			TotalItems: totalItems,
			AlbumKeys:  []string{albumKey},
			IsComplete: itemsAdded == totalItems,
		})
	}

	// Subsequent batches add to the newly created album
	for i := firstEnd; i < len(mediaKeys); i += AlbumBatchSize {
		if m.isCancelled() {
			return albumKey, fmt.Errorf("album creation cancelled (added %d/%d items)", itemsAdded, totalItems)
		}

		end := min(i+AlbumBatchSize, len(mediaKeys))
		batch := mediaKeys[i:end]

		err := m.addMediaWithRetry(albumKey, batch)
		if err != nil {
			return albumKey, fmt.Errorf("failed to add media to album '%s': %w", albumName, err)
		}
		itemsAdded += len(batch)

		if m.reporter != nil {
			m.reporter.AlbumProgress(AlbumStatus{
				AlbumName:  albumName,
				ItemsAdded: itemsAdded,
				TotalItems: totalItems,
				AlbumKeys:  []string{albumKey},
				IsComplete: itemsAdded == totalItems,
			})
		}
	}

	return albumKey, nil
}

// addMediaInBatches adds items to an existing album in chunks of AlbumBatchSize.
func (m *AlbumManager) addMediaInBatches(albumKey, albumName string, mediaKeys []string) error {
	totalItems := len(mediaKeys)
	itemsAdded := 0

	for i := 0; i < len(mediaKeys); i += AlbumBatchSize {
		if m.isCancelled() {
			return fmt.Errorf("album upload cancelled (added %d/%d items)", itemsAdded, totalItems)
		}

		end := min(i+AlbumBatchSize, len(mediaKeys))
		batch := mediaKeys[i:end]

		err := m.addMediaWithRetry(albumKey, batch)
		if err != nil {
			return err
		}
		itemsAdded += len(batch)

		if m.reporter != nil {
			m.reporter.AlbumProgress(AlbumStatus{
				AlbumName:  albumName,
				ItemsAdded: itemsAdded,
				TotalItems: totalItems,
				AlbumKeys:  []string{albumKey},
				IsComplete: itemsAdded == totalItems,
			})
		}
	}

	return nil
}

// addToExistingAlbum adds media to an existing album using its key (bypassing registry creation).
func (m *AlbumManager) addToExistingAlbum(mediaKeys []string, albumKey string) ([]string, error) {
	displayName := fmt.Sprintf("Album (%s...)", albumKey[:min(10, len(albumKey))])
	err := m.addMediaInBatches(albumKey, displayName, mediaKeys)
	if err != nil {
		return []string{albumKey}, err
	}

	if m.reporter != nil {
		m.reporter.AlbumComplete(AlbumStatus{
			AlbumName:  displayName,
			ItemsAdded: len(mediaKeys),
			TotalItems: len(mediaKeys),
			AlbumKeys:  []string{albumKey},
			IsComplete: true,
		})
	}

	return []string{albumKey}, nil
}

// addMediaWithRetry adds media to an album with retry logic.
func (m *AlbumManager) addMediaWithRetry(albumKey string, mediaKeys []string) error {
	retryConfig := DefaultRetryConfig()
	var lastErr error

	for attempt := 0; attempt <= retryConfig.MaxRetries; attempt++ {
		if attempt > 0 {
			if m.isCancelled() {
				return fmt.Errorf("cancelled during retry")
			}
			delay := CalculateBackoff(attempt-1, retryConfig)
			time.Sleep(delay)
		}

		err := m.api.AddMediaToAlbum(albumKey, mediaKeys)
		if err == nil {
			return nil
		}
		lastErr = err

		if !isRetryableAlbumError(err) {
			return err
		}
	}

	return fmt.Errorf("failed after %d attempts: %w", retryConfig.MaxRetries+1, lastErr)
}

// createAlbumWithRetry creates an album with retry logic.
func (m *AlbumManager) createAlbumWithRetry(albumName string, mediaKeys []string) (string, error) {
	retryConfig := DefaultRetryConfig()
	var lastErr error

	for attempt := 0; attempt <= retryConfig.MaxRetries; attempt++ {
		if attempt > 0 {
			if m.isCancelled() {
				return "", fmt.Errorf("cancelled during retry")
			}
			delay := CalculateBackoff(attempt-1, retryConfig)
			time.Sleep(delay)
		}

		albumKey, err := m.api.CreateAlbum(albumName, mediaKeys)
		if err == nil {
			return albumKey, nil
		}
		lastErr = err

		if !isRetryableAlbumError(err) {
			return "", err
		}
	}

	return "", fmt.Errorf("failed after %d attempts: %w", retryConfig.MaxRetries+1, lastErr)
}

// isRetryableAlbumError checks if an error should be retried.
func isRetryableAlbumError(err error) bool {
	if err == nil {
		return false
	}
	errStr := err.Error()
	if strings.Contains(errStr, "status 5") || strings.Contains(errStr, "status 429") {
		return true
	}
	if strings.Contains(errStr, "connection") || strings.Contains(errStr, "timeout") {
		return true
	}
	return false
}
