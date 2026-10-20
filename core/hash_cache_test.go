package core

import (
	"path/filepath"
	"testing"
)

func TestHashCache(t *testing.T) {
	tempDir := t.TempDir()
	dbPath := filepath.Join(tempDir, "test_hash_cache.db")

	cache, err := NewHashCache(dbPath)
	if err != nil {
		t.Fatalf("failed to create HashCache: %v", err)
	}
	defer cache.Close()

	acc1 := "user1@gmail.com"
	acc2 := "user2@gmail.com"

	// Initial count should be 0
	count, err := cache.Count(acc1)
	if err != nil {
		t.Fatalf("failed to get count: %v", err)
	}
	if count != 0 {
		t.Errorf("expected count 0, got %d", count)
	}

	// Not found check
	key, found, err := cache.Get(acc1, "nonexistent_hash", 1024)
	if err != nil {
		t.Fatalf("unexpected error getting hash: %v", err)
	}
	if found || key != "" {
		t.Errorf("expected found=false, got found=%v, key=%s", found, key)
	}

	// Put an entry for acc1
	err = cache.Put(acc1, "hash123", "mediaKey456", 1024, 1600000000)
	if err != nil {
		t.Fatalf("failed to put hash: %v", err)
	}

	// Verify get for acc1
	key, found, err = cache.Get(acc1, "hash123", 1024)
	if err != nil {
		t.Fatalf("error getting hash: %v", err)
	}
	if !found || key != "mediaKey456" {
		t.Errorf("expected mediaKey456, got %s (found=%v)", key, found)
	}

	// Verify acc2 cannot see acc1's cached entry
	key2, found2, err := cache.Get(acc2, "hash123", 1024)
	if err != nil {
		t.Fatalf("error getting hash for acc2: %v", err)
	}
	if found2 || key2 != "" {
		t.Errorf("expected acc2 not to find entry, got key=%s", key2)
	}

	// Verify wrong size returns false
	_, foundSize, _ := cache.Get(acc1, "hash123", 2048)
	if foundSize {
		t.Errorf("expected false for mismatched size")
	}

	// Check counts
	count1, _ := cache.Count(acc1)
	if count1 != 1 {
		t.Errorf("expected acc1 count 1, got %d", count1)
	}
	count2, _ := cache.Count(acc2)
	if count2 != 0 {
		t.Errorf("expected acc2 count 0, got %d", count2)
	}

	// Clear acc1
	cleared, err := cache.Clear(acc1)
	if err != nil {
		t.Fatalf("failed to clear cache: %v", err)
	}
	if cleared != 1 {
		t.Errorf("expected 1 cleared row, got %d", cleared)
	}

	countAfter, _ := cache.Count(acc1)
	if countAfter != 0 {
		t.Errorf("expected count 0 after clear, got %d", countAfter)
	}
}
