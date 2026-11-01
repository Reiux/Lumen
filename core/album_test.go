package core

import (
	"errors"
	"fmt"
	"io"
	"log/slog"
	"path/filepath"
	"sync"
	"testing"
)

type fakeAlbumAPI struct {
	mu           sync.Mutex
	createCalls  int
	addCalls     int
	createdNames []string
	deletedKeys  map[string]bool
	nextKeyID    int
}

func newFakeAlbumAPI() *fakeAlbumAPI {
	return &fakeAlbumAPI{
		deletedKeys: make(map[string]bool),
	}
}

func (f *fakeAlbumAPI) CreateAlbum(albumName string, mediaKeys []string) (string, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.createCalls++
	f.nextKeyID++
	f.createdNames = append(f.createdNames, albumName)
	return fmt.Sprintf("AF1QipFakeKey_%d_%s", f.nextKeyID, albumName), nil
}

func (f *fakeAlbumAPI) AddMediaToAlbum(albumMediaKey string, mediaKeys []string) error {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.addCalls++
	if f.deletedKeys[albumMediaKey] {
		return errors.New("album not found: 404")
	}
	return nil
}

func (f *fakeAlbumAPI) getCreateCalls() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.createCalls
}

func (f *fakeAlbumAPI) getAddCalls() int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.addCalls
}

func setupTestAlbumManager(t *testing.T, api AlbumAPI) (*AlbumManager, *AlbumRegistry) {
	dir := t.TempDir()
	regPath := filepath.Join(dir, "album_registry.json")
	reg, err := NewAlbumRegistry(regPath)
	if err != nil {
		t.Fatalf("failed to create test album registry: %v", err)
	}

	logger := slog.New(slog.NewTextHandler(io.Discard, nil))
	mgr := NewAlbumManager(api, NopReporter{}, logger, nil)
	mgr.SetAccountEmail("user@example.com")
	mgr.SetRegistry(reg)
	return mgr, reg
}

// 1. Same name twice -> 1 CreateAlbum call
func TestAlbumManager_SameNameTwice_OneCreateCall(t *testing.T) {
	fakeAPI := newFakeAlbumAPI()
	mgr, _ := setupTestAlbumManager(t, fakeAPI)

	// First upload with name "Vacation 2026"
	keys1, err := mgr.AddToAlbum([]string{"media1", "media2"}, "Vacation 2026")
	if err != nil {
		t.Fatalf("unexpected error on first call: %v", err)
	}
	if len(keys1) != 1 {
		t.Fatalf("expected 1 album key, got %v", keys1)
	}
	if fakeAPI.getCreateCalls() != 1 {
		t.Fatalf("expected 1 CreateAlbum call, got %d", fakeAPI.getCreateCalls())
	}

	// Second upload with same name (different case and spacing) -> should reuse existing album
	keys2, err := mgr.AddToAlbum([]string{"media3"}, "  vacation 2026  ")
	if err != nil {
		t.Fatalf("unexpected error on second call: %v", err)
	}
	if len(keys2) != 1 {
		t.Fatalf("expected 1 album key, got %v", keys2)
	}
	if keys2[0] != keys1[0] {
		t.Errorf("expected reused key %s, got %s", keys1[0], keys2[0])
	}
	if fakeAPI.getCreateCalls() != 1 {
		t.Errorf("expected still 1 CreateAlbum call after second upload, got %d", fakeAPI.getCreateCalls())
	}
	if fakeAPI.getAddCalls() != 1 {
		t.Errorf("expected 1 AddMediaToAlbum call, got %d", fakeAPI.getAddCalls())
	}
}

// 2. Concurrent 10 goroutines same name -> 1 CreateAlbum
func TestAlbumManager_Concurrent10Goroutines_OneCreateCall(t *testing.T) {
	fakeAPI := newFakeAlbumAPI()
	mgr, _ := setupTestAlbumManager(t, fakeAPI)

	var wg sync.WaitGroup
	numWorkers := 10
	errs := make(chan error, numWorkers)

	for i := 0; i < numWorkers; i++ {
		wg.Add(1)
		go func(workerID int) {
			defer wg.Done()
			itemKey := fmt.Sprintf("item_%d", workerID)
			_, err := mgr.AddToAlbum([]string{itemKey}, "Parallel Vacation")
			if err != nil {
				errs <- err
			}
		}(i)
	}

	wg.Wait()
	close(errs)

	for err := range errs {
		t.Fatalf("worker failed: %v", err)
	}

	if fakeAPI.getCreateCalls() != 1 {
		t.Fatalf("expected exactly 1 CreateAlbum call for 10 concurrent requests, got %d", fakeAPI.getCreateCalls())
	}
	// 9 of the workers must have added to the existing album
	if fakeAPI.getAddCalls() != 9 {
		t.Fatalf("expected 9 AddMediaToAlbum calls, got %d", fakeAPI.getAddCalls())
	}
}

// 3. Key input -> no create
func TestAlbumManager_KeyInput_NoCreateCall(t *testing.T) {
	fakeAPI := newFakeAlbumAPI()
	mgr, _ := setupTestAlbumManager(t, fakeAPI)

	rawKey := "AF1QipExistingAlbumKey12345"
	keys, err := mgr.AddToAlbum([]string{"media1", "media2"}, rawKey)
	if err != nil {
		t.Fatalf("unexpected error when adding to existing key: %v", err)
	}
	if len(keys) != 1 || keys[0] != rawKey {
		t.Fatalf("expected key %s, got %v", rawKey, keys)
	}
	if fakeAPI.getCreateCalls() != 0 {
		t.Errorf("expected 0 CreateAlbum calls for key input, got %d", fakeAPI.getCreateCalls())
	}
	if fakeAPI.getAddCalls() != 1 {
		t.Errorf("expected 1 AddMediaToAlbum call for key input, got %d", fakeAPI.getAddCalls())
	}
}

// 4. Registry persists across new AlbumManager
func TestAlbumManager_RegistryPersistsAcrossNewAlbumManager(t *testing.T) {
	dir := t.TempDir()
	regPath := filepath.Join(dir, "album_registry.json")

	reg1, err := NewAlbumRegistry(regPath)
	if err != nil {
		t.Fatalf("failed to create reg1: %v", err)
	}

	fakeAPI := newFakeAlbumAPI()
	logger := slog.New(slog.NewTextHandler(io.Discard, nil))

	// First AlbumManager creates album
	mgr1 := NewAlbumManager(fakeAPI, NopReporter{}, logger, nil)
	mgr1.SetAccountEmail("user@example.com")
	mgr1.SetRegistry(reg1)

	keys1, err := mgr1.AddToAlbum([]string{"item1"}, "Persisted Album")
	if err != nil {
		t.Fatalf("mgr1 AddToAlbum failed: %v", err)
	}
	if fakeAPI.getCreateCalls() != 1 {
		t.Fatalf("expected 1 create call, got %d", fakeAPI.getCreateCalls())
	}

	// Create a brand new AlbumManager reading the same on-disk registry file
	reg2, err := NewAlbumRegistry(regPath)
	if err != nil {
		t.Fatalf("failed to load reg2: %v", err)
	}
	mgr2 := NewAlbumManager(fakeAPI, NopReporter{}, logger, nil)
	mgr2.SetAccountEmail("user@example.com")
	mgr2.SetRegistry(reg2)

	// Second AlbumManager uploads to same album name
	keys2, err := mgr2.AddToAlbum([]string{"item2"}, "Persisted Album")
	if err != nil {
		t.Fatalf("mgr2 AddToAlbum failed: %v", err)
	}
	if fakeAPI.getCreateCalls() != 1 {
		t.Errorf("expected create calls to remain 1 after second manager call, got %d", fakeAPI.getCreateCalls())
	}
	if keys2[0] != keys1[0] {
		t.Errorf("expected reused key %s, got %s", keys1[0], keys2[0])
	}
}

// 5. Deleted album -> recreated once
func TestAlbumManager_DeletedAlbum_RecreatedOnce(t *testing.T) {
	fakeAPI := newFakeAlbumAPI()
	mgr, _ := setupTestAlbumManager(t, fakeAPI)

	// 1. Initial creation
	keys1, err := mgr.AddToAlbum([]string{"item1"}, "Temporary Album")
	if err != nil {
		t.Fatalf("initial AddToAlbum failed: %v", err)
	}
	if fakeAPI.getCreateCalls() != 1 {
		t.Fatalf("expected 1 create call, got %d", fakeAPI.getCreateCalls())
	}
	firstKey := keys1[0]

	// 2. Mark initial album as deleted in fake API
	fakeAPI.mu.Lock()
	fakeAPI.deletedKeys[firstKey] = true
	fakeAPI.mu.Unlock()

	// 3. Second call: attempts to add to deleted album, detects 404, deletes registry entry, and recreates once
	keys2, err := mgr.AddToAlbum([]string{"item2"}, "Temporary Album")
	if err != nil {
		t.Fatalf("AddToAlbum after deletion failed: %v", err)
	}
	if fakeAPI.getCreateCalls() != 2 {
		t.Fatalf("expected exactly 2 create calls (recreated once), got %d", fakeAPI.getCreateCalls())
	}
	if keys2[0] == firstKey {
		t.Errorf("expected new album key after recreation, got original %s", keys2[0])
	}
}

// 6. Link parsing unit tests (valid, invalid, trailing params)
func TestParseAlbumLink(t *testing.T) {
	tests := []struct {
		name        string
		input       string
		expectedKey string
		expectErr   bool
	}{
		// Valid links
		{
			name:        "valid google photos album url",
			input:       "https://photos.google.com/album/AF1QipM7f0e_validKey123",
			expectedKey: "AF1QipM7f0e_validKey123",
			expectErr:   false,
		},
		{
			name:        "valid google photos share url",
			input:       "https://photos.google.com/share/AF1QipM7f0e_shareKey456",
			expectedKey: "AF1QipM7f0e_shareKey456",
			expectErr:   false,
		},
		{
			name:        "valid user account album url",
			input:       "https://photos.google.com/u/1/album/AF1QipM7f0e_userKey789",
			expectedKey: "AF1QipM7f0e_userKey789",
			expectErr:   false,
		},
		{
			name:        "valid raw key",
			input:       "AF1QipM7f0e_directKey999",
			expectedKey: "AF1QipM7f0e_directKey999",
			expectErr:   false,
		},
		// Trailing parameters and query fragments
		{
			name:        "valid url with query params",
			input:       "https://photos.google.com/album/AF1QipM7f0e_validKey123?key=secret&source=share",
			expectedKey: "AF1QipM7f0e_validKey123",
			expectErr:   false,
		},
		{
			name:        "valid url with trailing slash and query params",
			input:       "https://photos.google.com/share/AF1QipM7f0e_shareKey456/?hl=en#photos",
			expectedKey: "AF1QipM7f0e_shareKey456",
			expectErr:   false,
		},
		{
			name:        "valid raw key with trailing whitespace",
			input:       "  AF1QipM7f0e_directKey999  \n",
			expectedKey: "AF1QipM7f0e_directKey999",
			expectErr:   false,
		},
		// Invalid links
		{
			name:      "empty input",
			input:     "",
			expectErr: true,
		},
		{
			name:      "google photos url missing album key",
			input:     "https://photos.google.com/albums",
			expectErr: true,
		},
		{
			name:      "generic non-album url",
			input:     "https://google.com/search?q=photos",
			expectErr: true,
		},
		{
			name:      "plain text name",
			input:     "Summer Vacation 2024",
			expectErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			key, err := ParseAlbumLink(tt.input)
			if tt.expectErr {
				if err == nil {
					t.Errorf("expected error for %q, got nil (key: %q)", tt.input, key)
				}
			} else {
				if err != nil {
					t.Errorf("unexpected error for %q: %v", tt.input, err)
				}
				if key != tt.expectedKey {
					t.Errorf("expected key %q, got %q", tt.expectedKey, key)
				}
			}
		})
	}
}
