package core

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestFolderWatcher(t *testing.T) {
	tempDir := t.TempDir()

	received := make(chan []string, 1)
	watcher, err := NewFolderWatcher(100*time.Millisecond, func(files []string) {
		received <- files
	})
	if err != nil {
		t.Fatalf("failed to create FolderWatcher: %v", err)
	}
	defer watcher.Close()

	if err := watcher.AddFolder(tempDir); err != nil {
		t.Fatalf("failed to add folder: %v", err)
	}

	watcher.Start()

	// Create a supported file in the watched directory
	testFile := filepath.Join(tempDir, "photo.jpg")
	if err := os.WriteFile(testFile, []byte("fake image data"), 0o644); err != nil {
		t.Fatalf("failed to write test file: %v", err)
	}

	select {
	case files := <-received:
		if len(files) != 1 || filepath.Clean(files[0]) != filepath.Clean(testFile) {
			t.Errorf("expected [%s], got %v", testFile, files)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for folder watcher event")
	}

	watcher.Stop()
}

func TestFolderWatcherIgnoresTempFiles(t *testing.T) {
	for _, name := range []string{
		"upload.tmp",
		"video.crdownload",
		"photo.part",
		".hidden.jpg",
		"~$document.jpg",
	} {
		if !isIgnoredTempFile(name) {
			t.Errorf("expected %s to be treated as ignored temp file", name)
		}
	}

	for _, name := range []string{
		"photo.jpg",
		"video.mp4",
		"IMG_1234.HEIC",
	} {
		if isIgnoredTempFile(name) {
			t.Errorf("expected %s to NOT be treated as ignored temp file", name)
		}
	}
}

func TestIsInotifyLimit(t *testing.T) {
	if isInotifyLimit(os.ErrNotExist) {
		t.Fatal("expected ErrNotExist to not be inotify limit")
	}
	customErr := &testError{msg: "no space left on device"}
	if !isInotifyLimit(customErr) {
		t.Errorf("expected 'no space left on device' to be detected as inotify limit")
	}
	inotifyLimitErr := &testError{msg: "system limit for number of file watchers reached"}
	if !isInotifyLimit(inotifyLimitErr) {
		t.Errorf("expected limit message to be detected as inotify limit")
	}
}

type testError struct {
	msg string
}

func (e *testError) Error() string {
	return e.msg
}
