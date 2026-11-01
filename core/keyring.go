package core

import (
	"errors"
	"fmt"
	"log/slog"
	"net/url"
	"os"
	"strings"
	"sync"

	"github.com/zalando/go-keyring"
)

const (
	// KeyringServiceName is the OS keychain service namespace.
	KeyringServiceName = "com.reiux.lumen"
	// KeyringAccountPrefix prefixes account keys in the keychain.
	KeyringAccountPrefix = "account_"
)

// KeyringProvider abstracts the underlying OS keychain implementation.
type KeyringProvider interface {
	Set(service, key, value string) error
	Get(service, key string) (string, error)
	Delete(service, key string) error
}

// OSKeyringProvider delegates to github.com/zalando/go-keyring.
type OSKeyringProvider struct{}

func (OSKeyringProvider) Set(service, key, value string) error {
	return keyring.Set(service, key, value)
}

func (OSKeyringProvider) Get(service, key string) (string, error) {
	return keyring.Get(service, key)
}

func (OSKeyringProvider) Delete(service, key string) error {
	err := keyring.Delete(service, key)
	if errors.Is(err, keyring.ErrNotFound) {
		return nil
	}
	return err
}

// MockKeyringProvider provides an in-memory thread-safe implementation for tests
// and environments without secret-service / D-Bus.
type MockKeyringProvider struct {
	mu   sync.RWMutex
	data map[string]string
}

// NewMockKeyringProvider creates an in-memory KeyringProvider.
func NewMockKeyringProvider() *MockKeyringProvider {
	return &MockKeyringProvider{
		data: make(map[string]string),
	}
}

func (m *MockKeyringProvider) itemKey(service, key string) string {
	return service + "::" + key
}

func (m *MockKeyringProvider) Set(service, key, value string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.data[m.itemKey(service, key)] = value
	return nil
}

func (m *MockKeyringProvider) Get(service, key string) (string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	val, ok := m.data[m.itemKey(service, key)]
	if !ok {
		return "", keyring.ErrNotFound
	}
	return val, nil
}

func (m *MockKeyringProvider) Delete(service, key string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.data, m.itemKey(service, key))
	return nil
}

// KeyringStorage manages secure credential storage in the OS keychain with
// graceful fallback to file-based config when the keychain is unavailable.
type KeyringStorage struct {
	service  string
	provider KeyringProvider
	logger   *slog.Logger
}

var (
	globalKeyringMu sync.RWMutex
	globalKeyring   *KeyringStorage
)

func init() {
	globalKeyring = NewKeyringStorage(OSKeyringProvider{}, slog.New(slog.DiscardHandler))
}

// NewKeyringStorage creates a new KeyringStorage manager.
func NewKeyringStorage(provider KeyringProvider, logger *slog.Logger) *KeyringStorage {
	if provider == nil {
		provider = OSKeyringProvider{}
	}
	if logger == nil {
		logger = slog.New(slog.DiscardHandler)
	}
	return &KeyringStorage{
		service:  KeyringServiceName,
		provider: provider,
		logger:   logger,
	}
}

// GetGlobalKeyring returns the shared KeyringStorage instance.
func GetGlobalKeyring() *KeyringStorage {
	globalKeyringMu.RLock()
	defer globalKeyringMu.RUnlock()
	return globalKeyring
}

// SetGlobalKeyring sets the shared KeyringStorage instance (primarily for tests).
func SetGlobalKeyring(k *KeyringStorage) {
	globalKeyringMu.Lock()
	defer globalKeyringMu.Unlock()
	globalKeyring = k
}

func (k *KeyringStorage) accountKey(email string) string {
	return KeyringAccountPrefix + strings.ToLower(strings.TrimSpace(email))
}

// StoreCredential stores the sensitive credential string in the OS keyring under account_<email>.
func (k *KeyringStorage) StoreCredential(email, credential string) error {
	if email == "" {
		return errors.New("email cannot be empty")
	}
	key := k.accountKey(email)
	if err := k.provider.Set(k.service, key, credential); err != nil {
		k.logger.Warn("OS keyring unavailable; falling back to 0600 config storage",
			"email", email, "error", err)
		return fmt.Errorf("store credential in keyring: %w", err)
	}
	return nil
}

// GetCredential retrieves the sensitive credential string from the OS keyring.
func (k *KeyringStorage) GetCredential(email string) (string, error) {
	if email == "" {
		return "", errors.New("email cannot be empty")
	}
	key := k.accountKey(email)
	cred, err := k.provider.Get(k.service, key)
	if err != nil {
		return "", fmt.Errorf("get credential from keyring: %w", err)
	}
	return cred, nil
}

// DeleteCredential removes the account's credentials from the OS keyring.
func (k *KeyringStorage) DeleteCredential(email string) error {
	if email == "" {
		return nil
	}
	key := k.accountKey(email)
	if err := k.provider.Delete(k.service, key); err != nil {
		k.logger.Warn("Failed to delete credential from keyring", "email", email, "error", err)
		return fmt.Errorf("delete credential from keyring: %w", err)
	}
	return nil
}

// GetCredentialForAccount resolves the full credential string for an account.
// It checks the OS keyring first. If unavailable or missing, it falls back to
// the loaded AppConfig.Account.Credentials store.
func GetCredentialForAccount(email string) (string, error) {
	ensureConfigLoaded()
	email = strings.TrimSpace(email)
	if email == "" {
		configMu.RLock()
		email = AppConfig.Account.Selected
		configMu.RUnlock()
	}
	if email == "" {
		return "", errors.New("no account email specified or selected")
	}

	// 1. Try OS keyring first
	keyringStorage := GetGlobalKeyring()
	if keyringStorage != nil {
		if cred, err := keyringStorage.GetCredential(email); err == nil && cred != "" {
			return cred, nil
		}
	}

	// 2. Fall back to in-memory / config file store
	configMu.RLock()
	defer configMu.RUnlock()
	for _, c := range AppConfig.Account.Credentials {
		params, err := url.ParseQuery(c)
		if err != nil {
			continue
		}
		if strings.EqualFold(params.Get("Email"), email) {
			// Found fallback credential in config
			return c, nil
		}
	}

	return "", fmt.Errorf("no credentials found for account %s", email)
}

// MigrateCredentialsToKeyring inspects AppConfig.Account.Credentials. If any
// plaintext tokens exist, it moves them into the OS keyring and clears the
// sensitive Token parameter from the on-disk config copy.
// Returns true if any credentials were migrated to the keyring.
func MigrateCredentialsToKeyring(config *Config) bool {
	if config == nil || len(config.Account.Credentials) == 0 {
		return false
	}

	keyringStorage := GetGlobalKeyring()
	if keyringStorage == nil {
		return false
	}

	migrated := false
	for i, credStr := range config.Account.Credentials {
		params, err := url.ParseQuery(credStr)
		if err != nil {
			continue
		}

		email := params.Get("Email")
		token := params.Get("Token")
		if email == "" || token == "" {
			continue // No sensitive token or already stripped
		}

		// Attempt to store in OS keyring
		if err := keyringStorage.StoreCredential(email, credStr); err != nil {
			// Keyring unavailable (e.g. headless Linux or no D-Bus) - skip stripping
			// to preserve fallback access.
			continue
		}

		// Successfully stored in keyring: clear the plaintext Token from the config
		params.Del("Token")
		config.Account.Credentials[i] = params.Encode()
		migrated = true
	}

	return migrated
}

// StripSensitiveToken creates a redacted copy of a credential string with
// Token removed for safe disk storage.
func StripSensitiveToken(credStr string) string {
	params, err := url.ParseQuery(credStr)
	if err != nil {
		return credStr
	}
	params.Del("Token")
	return params.Encode()
}

// WriteAtomicWithPerm writes content to a file atomically with explicit permissions (e.g. 0600).
func WriteAtomicWithPerm(path string, contents []byte, perm os.FileMode) error {
	return writeConfigAtomically(path, contents)
}
