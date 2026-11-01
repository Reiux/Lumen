package core

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// AlbumRegistryEntry holds the album media key and creation time.
type AlbumRegistryEntry struct {
	AlbumKey  string    `json:"album_key"`
	CreatedAt time.Time `json:"created_at"`
}

// AlbumRegistry manages a per-account album registry mapping normalized album names
// to album keys and creation timestamps. Persisted to disk in the config directory.
type AlbumRegistry struct {
	mu       sync.RWMutex
	filePath string
	// entries: map[account]map[normalized_name]AlbumRegistryEntry
	entries  map[string]map[string]AlbumRegistryEntry
}

var (
	globalAlbumRegistryMu sync.Mutex
	globalAlbumRegistry   *AlbumRegistry
)

// NormalizeAlbumAccount normalizes an account email for registry indexing.
func NormalizeAlbumAccount(account string) string {
	account = strings.ToLower(strings.TrimSpace(account))
	if account == "" {
		return "default"
	}
	return account
}

// NormalizeAlbumName normalizes an album name (trim + lowercase) for case-insensitive lookup.
func NormalizeAlbumName(name string) string {
	return strings.ToLower(strings.TrimSpace(name))
}

// NewAlbumRegistry initializes an album registry from the specified JSON file.
func NewAlbumRegistry(filePath string) (*AlbumRegistry, error) {
	reg := &AlbumRegistry{
		filePath: filePath,
		entries:  make(map[string]map[string]AlbumRegistryEntry),
	}
	if err := reg.load(); err != nil {
		return nil, err
	}
	return reg, nil
}

// DefaultAlbumRegistryPath returns the default location of album_registry.json.
func DefaultAlbumRegistryPath() string {
	if ConfigPath != "" {
		dir := filepath.Dir(ConfigPath)
		if dir != "" && dir != "." {
			return filepath.Join(dir, "album_registry.json")
		}
	}
	dir, err := os.UserConfigDir()
	if err == nil {
		return filepath.Join(dir, "Lumen", "album_registry.json")
	}
	return "album_registry.json"
}

// GetGlobalAlbumRegistry returns the singleton album registry.
func GetGlobalAlbumRegistry() *AlbumRegistry {
	globalAlbumRegistryMu.Lock()
	defer globalAlbumRegistryMu.Unlock()
	if globalAlbumRegistry == nil {
		path := DefaultAlbumRegistryPath()
		reg, err := NewAlbumRegistry(path)
		if err != nil {
			reg = &AlbumRegistry{
				filePath: path,
				entries:  make(map[string]map[string]AlbumRegistryEntry),
			}
		}
		globalAlbumRegistry = reg
	}
	return globalAlbumRegistry
}

// SetGlobalAlbumRegistry overrides the global registry (used in tests).
func SetGlobalAlbumRegistry(reg *AlbumRegistry) {
	globalAlbumRegistryMu.Lock()
	defer globalAlbumRegistryMu.Unlock()
	globalAlbumRegistry = reg
}

// Get retrieves an album entry for the given account and album name.
func (r *AlbumRegistry) Get(account, name string) (AlbumRegistryEntry, bool) {
	if r == nil {
		return AlbumRegistryEntry{}, false
	}
	r.mu.RLock()
	defer r.mu.RUnlock()

	acctKey := NormalizeAlbumAccount(account)
	nameKey := NormalizeAlbumName(name)
	acctMap, ok := r.entries[acctKey]
	if !ok {
		return AlbumRegistryEntry{}, false
	}
	entry, found := acctMap[nameKey]
	return entry, found
}

// FindNameByKey returns the album name or nickname associated with an album key.
func (r *AlbumRegistry) FindNameByKey(account, key string) (string, bool) {
	if r == nil || key == "" {
		return "", false
	}
	r.mu.RLock()
	defer r.mu.RUnlock()

	acctKey := NormalizeAlbumAccount(account)
	acctMap, ok := r.entries[acctKey]
	if !ok {
		return "", false
	}
	for name, entry := range acctMap {
		if entry.AlbumKey == key {
			return name, true
		}
	}
	return "", false
}

// GetAllNicknames returns a map of albumKey -> nickname for the account.
func (r *AlbumRegistry) GetAllNicknames(account string) map[string]string {
	if r == nil {
		return nil
	}
	r.mu.RLock()
	defer r.mu.RUnlock()

	acctKey := NormalizeAlbumAccount(account)
	result := make(map[string]string)
	if acctMap, ok := r.entries[acctKey]; ok {
		for name, entry := range acctMap {
			if entry.AlbumKey != "" {
				result[entry.AlbumKey] = name
			}
		}
	}
	return result
}

// Set stores an album entry and persists the registry to disk.
func (r *AlbumRegistry) Set(account, name, albumKey string) error {
	if r == nil {
		return nil
	}
	r.mu.Lock()
	defer r.mu.Unlock()

	acctKey := NormalizeAlbumAccount(account)
	nameKey := NormalizeAlbumName(name)
	if _, ok := r.entries[acctKey]; !ok {
		r.entries[acctKey] = make(map[string]AlbumRegistryEntry)
	}
	r.entries[acctKey][nameKey] = AlbumRegistryEntry{
		AlbumKey:  albumKey,
		CreatedAt: time.Now().UTC(),
	}
	return r.saveLocked()
}

// Delete removes an album entry and persists the change to disk.
func (r *AlbumRegistry) Delete(account, name string) error {
	if r == nil {
		return nil
	}
	r.mu.Lock()
	defer r.mu.Unlock()

	acctKey := NormalizeAlbumAccount(account)
	nameKey := NormalizeAlbumName(name)
	if acctMap, ok := r.entries[acctKey]; ok {
		delete(acctMap, nameKey)
		return r.saveLocked()
	}
	return nil
}

func (r *AlbumRegistry) saveLocked() error {
	if r.filePath == "" {
		return nil
	}
	if err := os.MkdirAll(filepath.Dir(r.filePath), 0o700); err != nil {
		return err
	}
	data, err := json.MarshalIndent(r.entries, "", "  ")
	if err != nil {
		return err
	}
	tmpFile := fmt.Sprintf("%s.%d.tmp", r.filePath, time.Now().UnixNano())
	if err := os.WriteFile(tmpFile, data, 0o600); err != nil {
		return err
	}
	return os.Rename(tmpFile, r.filePath)
}

func (r *AlbumRegistry) load() error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if r.filePath == "" {
		return nil
	}
	data, err := os.ReadFile(r.filePath)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return err
	}
	var loaded map[string]map[string]AlbumRegistryEntry
	if err := json.Unmarshal(data, &loaded); err != nil {
		return err
	}
	if loaded != nil {
		r.entries = loaded
	}
	return nil
}

type keyedAlbumLock struct {
	mu    sync.Mutex
	locks map[string]*sync.Mutex
}

var globalKeyedAlbumLock = &keyedAlbumLock{
	locks: make(map[string]*sync.Mutex),
}

func (kl *keyedAlbumLock) get(account, name string) *sync.Mutex {
	key := NormalizeAlbumAccount(account) + "::" + NormalizeAlbumName(name)
	kl.mu.Lock()
	defer kl.mu.Unlock()
	m, ok := kl.locks[key]
	if !ok {
		m = &sync.Mutex{}
		kl.locks[key] = m
	}
	return m
}
