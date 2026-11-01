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
	watcher       *fsnotify.Watcher
	mu            sync.Mutex
	folderConfigs map[string]bool // dirPath -> includeSubfolders
	pending       map[string]*fileStatCandidate
	timer         *time.Timer
	debounce      time.Duration
	stopCh        chan struct{}
	running       bool
	onNewFiles    func([]string)
}

type fileStatCandidate struct {
	lastSeen time.Time
	size     int64
	mtime    int64
	checks   int
}

var (
	globalFolderWatcher   *FolderWatcher
	globalFolderWatcherMu sync.Mutex

	lastInotifyError   string
	lastInotifyErrorMu sync.RWMutex
)

// SetLastInotifyError records the most recent inotify limit error.
func SetLastInotifyError(err error) {
	lastInotifyErrorMu.Lock()
	defer lastInotifyErrorMu.Unlock()
	if err != nil {
		lastInotifyError = err.Error()
	} else {
		lastInotifyError = ""
	}
}

// GetLastInotifyError returns any recorded inotify limit error message.
func GetLastInotifyError() string {
	lastInotifyErrorMu.RLock()
	defer lastInotifyErrorMu.RUnlock()
	return lastInotifyError
}

func isInotifyLimit(err error) bool {
	if err == nil {
		return false
	}
	s := strings.ToLower(err.Error())
	return strings.Contains(s, "no space left on device") ||
		strings.Contains(s, "too many open files") ||
		strings.Contains(s, "inotify") ||
		strings.Contains(s, "limit")
}

func isIgnoredTempFile(path string) bool {
	base := filepath.Base(path)
	if strings.HasPrefix(base, ".") || strings.HasPrefix(base, "~$") {
		return true
	}
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".tmp", ".part", ".crdownload", ".partial", ".download", ".temp":
		return true
	}
	return false
}

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
	if !cfg.WatchFoldersEnabled {
		globalFolderWatcher.Stop()
		return
	}

	activeFolders := make(map[string]bool)
	if len(cfg.WatchedFolderConfigs) > 0 {
		for _, fc := range cfg.WatchedFolderConfigs {
			if fc.Enabled {
				activeFolders[filepath.Clean(fc.Path)] = fc.IncludeSubfolders
			}
		}
	} else {
		for _, f := range cfg.WatchedFolders {
			activeFolders[filepath.Clean(f)] = false
		}
	}

	if len(activeFolders) == 0 {
		globalFolderWatcher.Stop()
		return
	}

	_ = globalFolderWatcher.SetFolderConfigs(activeFolders)
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
		watcher:       fsWatcher,
		folderConfigs: make(map[string]bool),
		pending:       make(map[string]*fileStatCandidate),
		debounce:      debounce,
		stopCh:        make(chan struct{}),
		onNewFiles:    onNewFiles,
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

// AddFolderWithSubfolders adds a directory to be monitored with optional recursive subfolders.
func (fw *FolderWatcher) AddFolderWithSubfolders(dirPath string, includeSubfolders bool) error {
	fw.mu.Lock()
	defer fw.mu.Unlock()

	dirPath = filepath.Clean(dirPath)
	if existingSub, exists := fw.folderConfigs[dirPath]; exists && existingSub == includeSubfolders {
		return nil
	}
	fw.folderConfigs[dirPath] = includeSubfolders

	if !includeSubfolders {
		return fw.watcher.Add(dirPath)
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
			if err := fw.watcher.Add(path); err != nil {
				if isInotifyLimit(err) {
					SetLastInotifyError(err)
				}
				return err
			}
			return nil
		}
		return nil
	})
	return err
}

// AddFolder adds a directory to be monitored (backward compatible, no subfolders).
func (fw *FolderWatcher) AddFolder(dirPath string) error {
	return fw.AddFolderWithSubfolders(dirPath, false)
}

// RemoveFolder stops monitoring a directory and its subdirectories.
func (fw *FolderWatcher) RemoveFolder(dirPath string) error {
	fw.mu.Lock()
	defer fw.mu.Unlock()

	dirPath = filepath.Clean(dirPath)
	if _, exists := fw.folderConfigs[dirPath]; !exists {
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

	delete(fw.folderConfigs, dirPath)
	return nil
}

// SetFolderConfigs replaces the set of watched folders with their respective subfolder flags.
func (fw *FolderWatcher) SetFolderConfigs(folders map[string]bool) error {
	fw.mu.Lock()
	current := make([]string, 0, len(fw.folderConfigs))
	for f := range fw.folderConfigs {
		current = append(current, f)
	}
	fw.mu.Unlock()

	for _, c := range current {
		_ = fw.RemoveFolder(c)
	}
	for f, sub := range folders {
		_ = fw.AddFolderWithSubfolders(f, sub)
	}
	return nil
}

// SetFolders replaces the set of watched folders.
func (fw *FolderWatcher) SetFolders(folders []string) error {
	m := make(map[string]bool)
	for _, f := range folders {
		m[f] = false
	}
	return fw.SetFolderConfigs(m)
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
			if isInotifyLimit(err) {
				SetLastInotifyError(err)
			}
		case event, ok := <-fw.watcher.Events:
			if !ok {
				return
			}
			fw.handleEvent(event)
		}
	}
}

func (fw *FolderWatcher) isSubfolderAllowedLocked(path string) bool {
	clean := filepath.Clean(path)
	for root, subfolders := range fw.folderConfigs {
		if clean == root {
			return true
		}
		if filepath.Dir(clean) == root {
			return true
		}
		if subfolders && strings.HasPrefix(clean, root+string(filepath.Separator)) {
			return true
		}
	}
	return false
}

func (fw *FolderWatcher) handleEvent(event fsnotify.Event) {
	if event.Op&(fsnotify.Create|fsnotify.Write) == 0 {
		return
	}

	path := event.Name
	if isIgnoredTempFile(path) {
		return
	}

	info, err := os.Stat(path)
	if err != nil {
		return
	}

	if info.IsDir() {
		// New subdirectory created inside watched folder; add it only if parent permits subfolders
		fw.mu.Lock()
		allowed := fw.isSubfolderAllowedLocked(path)
		fw.mu.Unlock()
		if allowed {
			if err := fw.watcher.Add(path); err != nil && isInotifyLimit(err) {
				SetLastInotifyError(err)
			}
		}
		return
	}

	// Verify file is within a watched folder configuration
	fw.mu.Lock()
	allowed := fw.isSubfolderAllowedLocked(path)
	fw.mu.Unlock()
	if !allowed {
		return
	}

	if !isSupportedByGooglePhotos(path) {
		return
	}

	fw.mu.Lock()
	defer fw.mu.Unlock()

	if candidate, exists := fw.pending[path]; exists {
		candidate.lastSeen = time.Now()
		// If modified again, reset stability check count
		candidate.checks = 0
	} else {
		fw.pending[path] = &fileStatCandidate{
			lastSeen: time.Now(),
			size:     -1,
			mtime:    0,
			checks:   0,
		}
	}

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

	type checkItem struct {
		path      string
		candidate *fileStatCandidate
	}
	var items []checkItem
	for p, c := range fw.pending {
		items = append(items, checkItem{path: p, candidate: c})
	}
	fw.mu.Unlock()

	var readyFiles []string

	for _, item := range items {
		p := item.path
		c := item.candidate

		if isIgnoredTempFile(p) {
			fw.mu.Lock()
			delete(fw.pending, p)
			fw.mu.Unlock()
			continue
		}

		info, err := os.Stat(p)
		if err != nil || info.IsDir() || info.Size() == 0 {
			continue
		}

		// Verify file can be read and is not still being exclusively locked/copied
		f, err := os.Open(p)
		if err != nil {
			c.checks = 0
			continue
		}
		_ = f.Close()

		currSize := info.Size()
		currMtime := info.ModTime().UnixNano()

		// If size and mtime are identical to the previous check, increment stability check count
		if c.size == currSize && c.mtime == currMtime {
			c.checks++
		} else {
			c.size = currSize
			c.mtime = currMtime
			c.checks = 1
		}

		// Require at least 2 consecutive stable checks separated by debounce interval
		if c.checks >= 2 {
			readyFiles = append(readyFiles, p)
		}
	}

	fw.mu.Lock()
	// Remove ready files from pending
	for _, rf := range readyFiles {
		delete(fw.pending, rf)
	}
	// Re-arm timer if there are still pending files stabilizing
	if len(fw.pending) > 0 {
		if fw.timer != nil {
			fw.timer.Stop()
		}
		fw.timer = time.AfterFunc(fw.debounce, fw.flushPending)
	}
	callback := fw.onNewFiles
	fw.mu.Unlock()

	if len(readyFiles) > 0 && callback != nil {
		callback(readyFiles)
	}
}
