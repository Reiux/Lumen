package core

import (
	"log/slog"
	"net/url"
	"strings"
	"testing"
)

func TestKeyringStorage_StoreRetrieveDelete(t *testing.T) {
	mockProvider := NewMockKeyringProvider()
	storage := NewKeyringStorage(mockProvider, slog.New(slog.DiscardHandler))

	email := "testuser@gmail.com"
	rawCred := "androidId=1234567890abcdef&Email=testuser@gmail.com&Token=oauth2rt_1/secret_master_token"

	// 1. Store
	if err := storage.StoreCredential(email, rawCred); err != nil {
		t.Fatalf("StoreCredential failed: %v", err)
	}

	// 2. Retrieve
	retrieved, err := storage.GetCredential(email)
	if err != nil {
		t.Fatalf("GetCredential failed: %v", err)
	}
	if retrieved != rawCred {
		t.Errorf("expected %q, got %q", rawCred, retrieved)
	}

	// 3. Case insensitivity check
	retrievedUpper, err := storage.GetCredential("TESTUSER@GMAIL.COM")
	if err != nil {
		t.Fatalf("GetCredential case-insensitive failed: %v", err)
	}
	if retrievedUpper != rawCred {
		t.Errorf("expected %q, got %q", rawCred, retrievedUpper)
	}

	// 4. Delete
	if err := storage.DeleteCredential(email); err != nil {
		t.Fatalf("DeleteCredential failed: %v", err)
	}

	// 5. Retrieve after delete
	_, err = storage.GetCredential(email)
	if err == nil {
		t.Fatalf("expected error after delete, got nil")
	}
}

func TestKeyringStorage_MigrationFromLegacyConfig(t *testing.T) {
	mockProvider := NewMockKeyringProvider()
	storage := NewKeyringStorage(mockProvider, slog.New(slog.DiscardHandler))

	prevKeyring := GetGlobalKeyring()
	defer SetGlobalKeyring(prevKeyring)
	SetGlobalKeyring(storage)

	rawCred1 := "androidId=abc1&Email=alice@example.com&Token=secret_token_1"
	rawCred2 := "androidId=abc2&Email=bob@example.com&Token=secret_token_2"

	cfg := &Config{
		Account: AccountConfig{
			Credentials: []string{rawCred1, rawCred2},
			Selected:    "alice@example.com",
		},
	}

	migrated := MigrateCredentialsToKeyring(cfg)
	if !migrated {
		t.Fatalf("expected MigrateCredentialsToKeyring to return true")
	}

	// Check that tokens were removed from the config struct
	for _, credStr := range cfg.Account.Credentials {
		params, err := url.ParseQuery(credStr)
		if err != nil {
			t.Fatalf("parse query failed: %v", err)
		}
		if params.Get("Token") != "" {
			t.Errorf("expected Token to be stripped, got %q", params.Get("Token"))
		}
	}

	// Check that tokens were preserved in the mock keyring
	aliceCred, err := storage.GetCredential("alice@example.com")
	if err != nil {
		t.Fatalf("alice not found in keyring: %v", err)
	}
	if !strings.Contains(aliceCred, "Token=secret_token_1") {
		t.Errorf("keyring missing original token: %s", aliceCred)
	}

	bobCred, err := storage.GetCredential("bob@example.com")
	if err != nil {
		t.Fatalf("bob not found in keyring: %v", err)
	}
	if !strings.Contains(bobCred, "Token=secret_token_2") {
		t.Errorf("keyring missing original token: %s", bobCred)
	}
}

func TestKeyringStorage_FallbackWhenUnavailable(t *testing.T) {
	// Provider that always fails (simulating headless without DBus)
	failingProvider := &failingKeyringProvider{}
	storage := NewKeyringStorage(failingProvider, slog.New(slog.DiscardHandler))

	prevKeyring := GetGlobalKeyring()
	defer SetGlobalKeyring(prevKeyring)
	SetGlobalKeyring(storage)

	rawCred := "androidId=abc1&Email=fallback@example.com&Token=secret_fallback"
	cfg := &Config{
		Account: AccountConfig{
			Credentials: []string{rawCred},
			Selected:    "fallback@example.com",
		},
	}

	// Migration should fail gracefully without stripping the config
	migrated := MigrateCredentialsToKeyring(cfg)
	if migrated {
		t.Errorf("expected migrated to be false when keyring fails")
	}

	// Token must still be present in config as fallback
	params, _ := url.ParseQuery(cfg.Account.Credentials[0])
	if params.Get("Token") != "secret_fallback" {
		t.Errorf("token should remain in fallback config")
	}
}

type failingKeyringProvider struct{}

func (failingKeyringProvider) Set(service, key, value string) error {
	return ErrKeyringUnavailable
}

func (failingKeyringProvider) Get(service, key string) (string, error) {
	return "", ErrKeyringUnavailable
}

func (failingKeyringProvider) Delete(service, key string) error {
	return ErrKeyringUnavailable
}

var ErrKeyringUnavailable = &testKeyringError{msg: "secret-service not available"}

type testKeyringError struct {
	msg string
}

func (e *testKeyringError) Error() string { return e.msg }
