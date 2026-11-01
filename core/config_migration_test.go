package core

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestLoadConfigMigratesLegacyFlatLayout(t *testing.T) {
	restoreConfigGlobals(t)

	path := filepath.Join(t.TempDir(), "gotohp.config")
	legacy := strings.Join([]string{
		"credentials:",
		"  - Email=person%40example.com&Token=secret",
		"selected: person@example.com",
		"proxy: http://proxy.example",
		"upload_threads: 7",
		"recursive: true",
		"skip_incomplete_live_photos: false",
		"",
	}, "\n")
	if err := os.WriteFile(path, []byte(legacy), 0o600); err != nil {
		t.Fatal(err)
	}

	if err := LoadConfig(path); err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}
	got := (&ConfigManager{}).GetConfig()
	if got.Account.Selected != "person@example.com" || len(got.Account.Credentials) != 1 {
		t.Fatalf("account not migrated: %+v", got.Account)
	}
	p := got.Preferences
	if p.Proxy != "http://proxy.example" || p.UploadThreads != 7 || !p.Recursive || p.SkipIncompleteLivePhotos {
		t.Fatalf("preferences not migrated: %+v", p)
	}

	saved, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{"account:", "preferences:", "upload_threads: 7"} {
		if !strings.Contains(string(saved), want) {
			t.Fatalf("migrated file missing %q:\n%s", want, saved)
		}
	}
	if strings.HasPrefix(string(saved), "credentials:") {
		t.Fatalf("file still in legacy layout:\n%s", saved)
	}

	// A second load must read the sectioned layout without re-migrating.
	if err := LoadConfig(path); err != nil {
		t.Fatalf("LoadConfig (second): %v", err)
	}
	if again := (&ConfigManager{}).GetConfig(); !reflect.DeepEqual(again, got) {
		t.Fatalf("second load differs:\n%+v\n%+v", again, got)
	}
}

func TestLoadConfigMissingFileUsesDefaults(t *testing.T) {
	restoreConfigGlobals(t)

	path := filepath.Join(t.TempDir(), "gotohp.config")
	if err := LoadConfig(path); err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}
	if got := (&ConfigManager{}).GetConfig(); !reflect.DeepEqual(got, DefaultConfig) {
		t.Fatalf("config = %+v, want defaults", got)
	}
}

func TestExistingLocalConfig(t *testing.T) {
	dir := t.TempDir()
	if path, ok := existingLocalConfig(dir); ok || path != "" {
		t.Fatalf("found config in empty directory: %q", path)
	}

	// 1. Fallback to legacy gotohp.config
	legacyWant := filepath.Join(dir, "gotohp.config")
	if err := os.WriteFile(legacyWant, []byte{}, 0o600); err != nil {
		t.Fatal(err)
	}
	if path, ok := existingLocalConfig(dir); !ok || path != legacyWant {
		t.Fatalf("existingLocalConfig() = %q, %v; want %q, true", path, ok, legacyWant)
	}

	// 2. lumen.config takes priority over legacy gotohp.config
	lumenWant := filepath.Join(dir, "lumen.config")
	if err := os.WriteFile(lumenWant, []byte{}, 0o600); err != nil {
		t.Fatal(err)
	}
	if path, ok := existingLocalConfig(dir); !ok || path != lumenWant {
		t.Fatalf("existingLocalConfig() = %q, %v; want %q, true", path, ok, lumenWant)
	}
}

func TestDetermineConfigPath_MigratesGotohpToLumen(t *testing.T) {
	restoreConfigGlobals(t)

	configHome := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", configHome)

	// Set up legacy ~/.config/gotohp/gotohp.config and hash_cache.db
	legacyDir := filepath.Join(configHome, "gotohp")
	if err := os.MkdirAll(legacyDir, 0o700); err != nil {
		t.Fatal(err)
	}
	legacyConfig := filepath.Join(legacyDir, "gotohp.config")
	configContent := "preferences:\n  upload_threads: 5\n"
	if err := os.WriteFile(legacyConfig, []byte(configContent), 0o600); err != nil {
		t.Fatal(err)
	}
	legacyDB := filepath.Join(legacyDir, "hash_cache.db")
	if err := os.WriteFile(legacyDB, []byte("mock-db-content"), 0o600); err != nil {
		t.Fatal(err)
	}

	// Call determineConfigPath
	determineConfigPath()

	expectedPath := filepath.Join(configHome, "lumen", "lumen.config")
	if ConfigPath != expectedPath {
		t.Fatalf("ConfigPath = %q, want %q", ConfigPath, expectedPath)
	}

	// Verify migrated files exist in ~/.config/lumen
	migratedConfig, err := os.ReadFile(expectedPath)
	if err != nil {
		t.Fatalf("migrated config not found: %v", err)
	}
	if string(migratedConfig) != configContent {
		t.Fatalf("migrated config content = %q, want %q", string(migratedConfig), configContent)
	}

	migratedDB, err := os.ReadFile(filepath.Join(configHome, "lumen", "hash_cache.db"))
	if err != nil {
		t.Fatalf("migrated hash_cache.db not found: %v", err)
	}
	if string(migratedDB) != "mock-db-content" {
		t.Fatalf("migrated db content = %q, want mock-db-content", string(migratedDB))
	}
}
