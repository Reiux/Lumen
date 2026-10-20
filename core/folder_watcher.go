package core

import (
	"log"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/fsnotify/fsnotify"
)

// FolderWatcher monitors configured directories for newly created or modified
// media files and notifies a callback after a debounce period.
type FolderWatcher struct {
	watcher    *fsnotify.Watcher
	mu         sync.Mutex
	folders    map[string]bool
	pending    map[string]time.Time
	timer      *time.Timer
	debounce   time.Duration
	stopCh     chan struct{}
	running    bool
	onNewFiles func([]string)
}

var (
	globalFolderWatcher   *FolderWatcher
	globalFolderWatcherMu sync.Mutex
)

// InitFolderWatcher initializes the singleton folder watcher with the given callback.
func InitFolderWatcher(onNewFiles func([]string)) error {
	globalFolderWatcherMu.Lock()
	defer globalFolderWatcherMu.Unlock()

	if globalFolderWatcher != nil {
		_ = globalFolderWatcher.Close()
	}

	fw, err := NewFolderWatcher(2*time.Second, onNewFiles)
	if err != nil {
		return err
	}
	globalFolderWatcher = fw
	syncFolderWatcherLocked()
	return nil
}

// SyncFolderWatcher synchronizes the folder watcher with the current preferences.
func SyncFolderWatcher() {
	globalFolderWatcherMu.Lock()
	defer globalFolderWatcherMu.Unlock()
	syncFolderWatcherLocked()
}

func syncFolderWatcherLocked() {
	if globalFolderWatcher == nil {
		return
	}
	cfg := currentConfig().Preferences
	if !cfg.WatchFoldersEnabled || len(cfg.WatchedFolders) == 0 {
		globalFolderWatcher.Stop()
		return
	}

	_ = globalFolderWatcher.SetFolders(cfg.WatchedFolders)
	globalFolderWatcher.Start()
}

// NewFolderWatcher creates a new folder watcher instance.
func NewFolderWatcher(debounce time.Duration, onNewFiles func([]string)) (*FolderWatcher, error) {
	if debounce <= 0 {
		debounce = 2 * time.Second
	}
	fsWatcher, err := fsnotify.NewWatcher()
	if err != nil {
		return nil, err
	}

	fw := &FolderWatcher{
		watcher:    fsWatcher,
		folders:    make(map[string]bool),
		pending:    make(map[string]time.Time),
		debounce:   debounce,
		stopCh:     make(chan struct{}),
		onNewFiles: onNewFiles,
	}

	return fw, nil
}

// Start begins monitoring events in a background goroutine.
func (fw *FolderWatcher) Start() {
	fw.mu.Lock()
	if fw.running {
		fw.mu.Unlock()
		return
	}
	fw.running = true
	fw.stopCh = make(chan struct{})
	fw.mu.Unlock()

	go fw.loop()
}

// Stop terminates the watcher background loop.
func (fw *FolderWatcher) Stop() {
	fw.mu.Lock()
	if !fw.running {
		fw.mu.Unlock()
		return
	}
	fw.running = false
	close(fw.stopCh)
	if fw.timer != nil {
		fw.timer.Stop()
	}
	fw.mu.Unlock()
}

// Close closes the underlying fsnotify watcher.
func (fw *FolderWatcher) Close() error {
	fw.Stop()
	return fw.watcher.Close()
}

// AddFolder adds a directory to be monitored, including existing subdirectories.
func (fw *FolderWatcher) AddFolder(dirPath string) error {
	fw.mu.Lock()
	defer fw.mu.Unlock()

	dirPath = filepath.Clean(dirPath)
	if fw.folders[dirPath] {
		return nil
	}

	err := filepath.Walk(dirPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}
		if info.IsDir() {
			base := filepath.Base(path)
			if strings.HasPrefix(base, ".") && path != dirPath {
				return filepath.SkipDir
			}
			return fw.watcher.Add(path)
		}
		return nil
	})
	if err != nil {
		return err
	}

	fw.folders[dirPath] = true
	return nil
}

// RemoveFolder stops monitoring a directory and its subdirectories.
func (fw *FolderWatcher) RemoveFolder(dirPath string) error {
	fw.mu.Lock()
	defer fw.mu.Unlock()

	dirPath = filepath.Clean(dirPath)
	if !fw.folders[dirPath] {
		return nil
	}

	_ = filepath.Walk(dirPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}
		if info.IsDir() {
			_ = fw.watcher.Remove(path)
		}
		return nil
	})

	delete(fw.folders, dirPath)
	return nil
}

// SetFolders replaces the set of watched folders.
func (fw *FolderWatcher) SetFolders(folders []string) error {
	fw.mu.Lock()
	current := make([]string, 0, len(fw.folders))
	for f := range fw.folders {
		current = append(current, f)
	}
	fw.mu.Unlock()

	for _, c := range current {
		_ = fw.RemoveFolder(c)
	}
	for _, f := range folders {
		_ = fw.AddFolder(f)
	}
	return nil
}

func (fw *FolderWatcher) loop() {
	for {
		select {
		case <-fw.stopCh:
			return
		case err, ok := <-fw.watcher.Errors:
			if !ok {
				return
			}
			log.Printf("folder watcher error: %v", err)
		case event, ok := <-fw.watcher.Events:
			if !ok {
				return
			}
			fw.handleEvent(event)
		}
	}
}

func (fw *FolderWatcher) handleEvent(event fsnotify.Event) {
	if event.Op&(fsnotify.Create|fsnotify.Write) == 0 {
		return
	}

	path := event.Name
	base := filepath.Base(path)
	if strings.HasPrefix(base, ".") {
		return
	}

	info, err := os.Stat(path)
	if err != nil {
		return
	}

	if info.IsDir() {
		// New subdirectory created inside watched folder; add it to watcher
		_ = fw.watcher.Add(path)
		return
	}

	if !isSupportedByGooglePhotos(path) {
		return
	}

	fw.mu.Lock()
	defer fw.mu.Unlock()

	fw.pending[path] = time.Now()

	if fw.timer != nil {
		fw.timer.Stop()
	}
	fw.timer = time.AfterFunc(fw.debounce, fw.flushPending)
}

func (fw *FolderWatcher) flushPending() {
	fw.mu.Lock()
	if len(fw.pending) == 0 {
		fw.mu.Unlock()
		return
	}

	candidates := make([]string, 0, len(fw.pending))
	for p := range fw.pending {
		candidates = append(candidates, p)
	}
	fw.pending = make(map[string]time.Time)
	callback := fw.onNewFiles
	fw.mu.Unlock()

	var readyFiles []string
	for _, p := range candidates {
		info, err := os.Stat(p)
		if err != nil || info.IsDir() || info.Size() == 0 {
			continue
		}
		// Verify file can be read and is not still being exclusively locked/copied
		f, err := os.Open(p)
		if err != nil {
			// If locked, re-queue for next flush
			fw.mu.Lock()
			fw.pending[p] = time.Now()
			if fw.timer != nil {
				fw.timer.Stop()
			}
			fw.timer = time.AfterFunc(fw.debounce, fw.flushPending)
			fw.mu.Unlock()
			continue
		}
		_ = f.Close()
		readyFiles = append(readyFiles, p)
	}

	if len(readyFiles) > 0 && callback != nil {
		callback(readyFiles)
	}
}
