package core

import (
	"context"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"
)

func TestScanner_DepthLimit(t *testing.T) {
	tmpDir := t.TempDir()

	// Create a nested tree of depth 4: tmpDir/l1/l2/l3/l4/deep.jpg
	deepDir := filepath.Join(tmpDir, "l1", "l2", "l3", "l4")
	if err := os.MkdirAll(deepDir, 0o755); err != nil {
		t.Fatal(err)
	}

	shallowFile := filepath.Join(tmpDir, "l1", "shallow.jpg")
	deepFile := filepath.Join(deepDir, "deep.jpg")

	_ = os.WriteFile(shallowFile, []byte("shallow"), 0o644)
	_ = os.WriteFile(deepFile, []byte("deep"), 0o644)

	// Scan with maxDepth = 2
	files, err := ScanDirectoriesIncremental(context.Background(), []string{tmpDir}, ScanOptions{
		Recursive:              true,
		MaxDepth:               2,
		FilterUnsupportedFiles: true,
	})
	if err != nil {
		t.Fatalf("scan failed: %v", err)
	}

	// shallow.jpg is at depth 1 (under root), deep.jpg is at depth 4
	foundShallow := false
	foundDeep := false
	for _, f := range files {
		if filepath.Base(f) == "shallow.jpg" {
			foundShallow = true
		}
		if filepath.Base(f) == "deep.jpg" {
			foundDeep = true
		}
	}

	if !foundShallow {
		t.Errorf("shallow.jpg should be found")
	}
	if foundDeep {
		t.Errorf("deep.jpg should be skipped by maxDepth=2")
	}
}

func TestScanner_NoMediaSkipping(t *testing.T) {
	tmpDir := t.TempDir()

	// Folder 1: regular photos
	validDir := filepath.Join(tmpDir, "camera")
	_ = os.MkdirAll(validDir, 0o755)
	_ = os.WriteFile(filepath.Join(validDir, "photo1.jpg"), []byte("photo"), 0o644)

	// Folder 2: marked with .nomedia
	noMediaDir := filepath.Join(tmpDir, "stickers")
	_ = os.MkdirAll(noMediaDir, 0o755)
	_ = os.WriteFile(filepath.Join(noMediaDir, ".nomedia"), []byte(""), 0o644)
	_ = os.WriteFile(filepath.Join(noMediaDir, "sticker.png"), []byte("sticker"), 0o644)

	files, err := ScanDirectoriesIncremental(context.Background(), []string{tmpDir}, ScanOptions{
		Recursive:              true,
		MaxDepth:               5,
		FilterUnsupportedFiles: true,
	})
	if err != nil {
		t.Fatalf("scan failed: %v", err)
	}

	foundPhoto := false
	foundSticker := false
	for _, f := range files {
		if filepath.Base(f) == "photo1.jpg" {
			foundPhoto = true
		}
		if filepath.Base(f) == "sticker.png" {
			foundSticker = true
		}
	}

	if !foundPhoto {
		t.Errorf("photo1.jpg should be found")
	}
	if foundSticker {
		t.Errorf("sticker.png in folder with .nomedia must be skipped")
	}
}

func TestScanner_HiddenAndTrashDirectorySkipping(t *testing.T) {
	tmpDir := t.TempDir()

	// Hidden directory .git
	gitDir := filepath.Join(tmpDir, ".git")
	_ = os.MkdirAll(gitDir, 0o755)
	_ = os.WriteFile(filepath.Join(gitDir, "avatar.png"), []byte("hidden"), 0o644)

	// Trash directory
	trashDir := filepath.Join(tmpDir, ".Trash-1000")
	_ = os.MkdirAll(trashDir, 0o755)
	_ = os.WriteFile(filepath.Join(trashDir, "deleted.jpg"), []byte("trash"), 0o644)

	// Normal directory
	normalDir := filepath.Join(tmpDir, "vacation")
	_ = os.MkdirAll(normalDir, 0o755)
	_ = os.WriteFile(filepath.Join(normalDir, "beach.jpg"), []byte("beach"), 0o644)

	files, err := ScanDirectoriesIncremental(context.Background(), []string{tmpDir}, ScanOptions{
		Recursive:              true,
		MaxDepth:               5,
		FilterUnsupportedFiles: true,
	})
	if err != nil {
		t.Fatalf("scan failed: %v", err)
	}

	if len(files) != 1 || filepath.Base(files[0]) != "beach.jpg" {
		t.Errorf("expected only beach.jpg, got %v", files)
	}
}

func TestScanner_SymlinkCycleGuard(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Symlinks require administrative privileges on Windows")
	}

	tmpDir := t.TempDir()
	subDir := filepath.Join(tmpDir, "sub")
	_ = os.MkdirAll(subDir, 0o755)
	_ = os.WriteFile(filepath.Join(subDir, "pic.jpg"), []byte("pic"), 0o644)

	// Create circular symlink: tmpDir/sub/loop -> tmpDir
	loopLink := filepath.Join(subDir, "loop")
	_ = os.Symlink(tmpDir, loopLink)

	// Must finish quickly without hanging in infinite loop
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	files, err := ScanDirectoriesIncremental(ctx, []string{tmpDir}, ScanOptions{
		Recursive:              true,
		MaxDepth:               10,
		FilterUnsupportedFiles: true,
	})
	if err != nil {
		t.Fatalf("scan with symlink failed: %v", err)
	}

	foundPic := false
	for _, f := range files {
		if filepath.Base(f) == "pic.jpg" {
			foundPic = true
		}
	}
	if !foundPic {
		t.Errorf("pic.jpg was not found in symlink test")
	}
}

func TestScanner_ContextCancellation(t *testing.T) {
	tmpDir := t.TempDir()

	for i := 0; i < 20; i++ {
		dir := filepath.Join(tmpDir, "folder", "sub", "deep")
		_ = os.MkdirAll(dir, 0o755)
		_ = os.WriteFile(filepath.Join(dir, "image.jpg"), []byte("data"), 0o644)
	}

	ctx, cancel := context.WithCancel(context.Background())
	// Cancel immediately
	cancel()

	files, err := ScanDirectoriesIncremental(ctx, []string{tmpDir}, ScanOptions{
		Recursive:              true,
		MaxDepth:               10,
		FilterUnsupportedFiles: true,
	})

	if err == nil {
		t.Errorf("expected context.Canceled error, got nil (files: %v)", files)
	}
}

func TestScanner_ExcludePattern(t *testing.T) {
	tmpDir := t.TempDir()

	// Create test structure:
	// tmpDir/keep.jpg
	// tmpDir/skip_me.jpg
	// tmpDir/skip_dir/photo.jpg
	// tmpDir/sub/thumb_abc.jpg
	// tmpDir/sub/valid.jpg
	_ = os.WriteFile(filepath.Join(tmpDir, "keep.jpg"), []byte("data"), 0o644)
	_ = os.WriteFile(filepath.Join(tmpDir, "skip_me.jpg"), []byte("data"), 0o644)

	skipDir := filepath.Join(tmpDir, "skip_dir")
	_ = os.MkdirAll(skipDir, 0o755)
	_ = os.WriteFile(filepath.Join(skipDir, "photo.jpg"), []byte("data"), 0o644)

	subDir := filepath.Join(tmpDir, "sub")
	_ = os.MkdirAll(subDir, 0o755)
	_ = os.WriteFile(filepath.Join(subDir, "thumb_abc.jpg"), []byte("data"), 0o644)
	_ = os.WriteFile(filepath.Join(subDir, "valid.jpg"), []byte("data"), 0o644)

	// Test 1: Wildcard file exclusion
	files, err := ScanDirectoriesIncremental(context.Background(), []string{tmpDir}, ScanOptions{
		Recursive:              true,
		MaxDepth:               5,
		FilterUnsupportedFiles: true,
		ExcludePattern:         "*thumb*",
	})
	if err != nil {
		t.Fatalf("scan failed: %v", err)
	}

	for _, f := range files {
		if filepath.Base(f) == "thumb_abc.jpg" {
			t.Errorf("expected thumb_abc.jpg to be excluded by *thumb*")
		}
	}

	// Test 2: Directory exclusion
	files, err = ScanDirectoriesIncremental(context.Background(), []string{tmpDir}, ScanOptions{
		Recursive:              true,
		MaxDepth:               5,
		FilterUnsupportedFiles: true,
		ExcludePattern:         "skip_dir",
	})
	if err != nil {
		t.Fatalf("scan failed: %v", err)
	}

	for _, f := range files {
		if filepath.Base(f) == "photo.jpg" {
			t.Errorf("expected photo.jpg inside skip_dir to be excluded")
		}
	}
}
