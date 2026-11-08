package core

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"
)

// DefaultMaxScanDepth limits directory recursion to prevent runaway walks or symlink cycles.
const DefaultMaxScanDepth = 10

// ScanProgressEvent reports live discovery stats to the UI and reporter.
type ScanProgressEvent struct {
	FilesFound       int    `json:"filesFound"`
	CurrentDirectory string `json:"currentDirectory"`
	IsScanning       bool   `json:"isScanning"`
}

// ScanOptions configures directory walking filters.
type ScanOptions struct {
	Recursive              bool
	ExcludePattern         string
	MaxDepth               int
	FilterUnsupportedFiles bool
	ProgressCallback       func(ScanProgressEvent)
}

var (
	scanControlMu sync.Mutex
	activeScanCancel context.CancelFunc
)

// SetActiveScanCancel registers the cancellation function for the current active scan.
func SetActiveScanCancel(cancel context.CancelFunc) {
	scanControlMu.Lock()
	defer scanControlMu.Unlock()
	activeScanCancel = cancel
}

// CancelActiveScan cancels any in-progress folder scanning.
func CancelActiveScan() {
	scanControlMu.Lock()
	cancel := activeScanCancel
	activeScanCancel = nil
	scanControlMu.Unlock()
	if cancel != nil {
		cancel()
	}
}

// IsTrashOrHiddenDirectory checks if a directory name is hidden or a system trash/temp directory.
func IsTrashOrHiddenDirectory(dirName string) bool {
	dirName = strings.TrimSpace(dirName)
	if dirName == "" || dirName == "." || dirName == ".." {
		return false
	}
	// Hidden folder (starts with dot)
	if strings.HasPrefix(dirName, ".") {
		return true
	}
	// Common system / trash / temp directories
	upper := strings.ToUpper(dirName)
	if upper == "$RECYCLE.BIN" || upper == "SYSTEM VOLUME INFORMATION" ||
		upper == ".TRASH" || strings.HasPrefix(upper, ".TRASH-") ||
		upper == "THUMBS.DB" {
		return true
	}
	return false
}

// DirectoryHasNoMedia reports whether a folder contains a .nomedia file.
func DirectoryHasNoMedia(dirPath string) bool {
	_, err := os.Stat(filepath.Join(dirPath, ".nomedia"))
	return err == nil
}

type dirStackItem struct {
	path  string
	depth int
}

func matchesExcludePattern(pattern, name string) bool {
	if pattern == "" {
		return false
	}
	if name == pattern {
		return true
	}
	matched, err := filepath.Match(pattern, name)
	return err == nil && matched
}

// ScanDirectoriesIncremental iteratively walks roots using an explicit stack with depth limits,
// symlink cycle detection, .nomedia skipping, and throttled progress reporting.
func ScanDirectoriesIncremental(ctx context.Context, paths []string, opts ScanOptions) ([]string, error) {
	maxDepth := opts.MaxDepth
	if maxDepth <= 0 {
		maxDepth = DefaultMaxScanDepth
	}

	var supportedFiles []string
	seenFiles := make(map[string]bool)
	visitedDirs := make(map[string]bool)

	lastProgressEmit := time.Time{}
	emitProgress := func(dir string, isScanning bool) {
		if opts.ProgressCallback == nil {
			return
		}
		now := time.Now()
		if !isScanning || lastProgressEmit.IsZero() || now.Sub(lastProgressEmit) >= 150*time.Millisecond {
			lastProgressEmit = now
			opts.ProgressCallback(ScanProgressEvent{
				FilesFound:       len(supportedFiles),
				CurrentDirectory: dir,
				IsScanning:       isScanning,
			})
		}
	}

	for _, rootPath := range paths {
		if ctx.Err() != nil {
			emitProgress("", false)
			return nil, ctx.Err()
		}

		info, err := os.Stat(rootPath)
		if err != nil {
			return nil, fmt.Errorf("error accessing path %s: %w", rootPath, err)
		}

		if !info.IsDir() {
			// Single file
			name := filepath.Base(rootPath)
			if matchesExcludePattern(opts.ExcludePattern, name) {
				continue
			}
			if !opts.FilterUnsupportedFiles || isSupportedByGooglePhotos(rootPath) {
				clean := filepath.Clean(rootPath)
				key := clean
				if runtime.GOOS == "windows" {
					key = strings.ToLower(key)
				}
				if !seenFiles[key] {
					seenFiles[key] = true
					supportedFiles = append(supportedFiles, clean)
				}
			}
			continue
		}

		// Iterative directory walk with explicit stack
		stack := []dirStackItem{{path: filepath.Clean(rootPath), depth: 0}}

		for len(stack) > 0 {
			if ctx.Err() != nil {
				emitProgress("", false)
				return nil, ctx.Err()
			}

			// Pop from stack
			item := stack[len(stack)-1]
			stack = stack[:len(stack)-1]

			if item.depth > maxDepth {
				continue // Enforce maximum recursion depth
			}

			// Symlink cycle check using canonical evaluated path
			evalPath := item.path
			if realPath, err := filepath.EvalSymlinks(item.path); err == nil {
				evalPath = realPath
			}
			if runtime.GOOS == "windows" {
				evalPath = strings.ToLower(evalPath)
			}
			if visitedDirs[evalPath] {
				continue // Guard against symlink recursion loop
			}
			visitedDirs[evalPath] = true

			// Skip directories marked with .nomedia
			if DirectoryHasNoMedia(item.path) {
				continue
			}

			emitProgress(item.path, true)

			entries, err := os.ReadDir(item.path)
			if err != nil {
				continue // Skip unreadable directory contents
			}

			for _, entry := range entries {
				if ctx.Err() != nil {
					emitProgress("", false)
					return nil, ctx.Err()
				}

				name := entry.Name()
				fullPath := filepath.Join(item.path, name)

				if entry.IsDir() {
					if !opts.Recursive {
						continue
					}
					if matchesExcludePattern(opts.ExcludePattern, name) {
						continue
					}
					if IsTrashOrHiddenDirectory(name) {
						continue
					}
					// Push child directory onto stack
					stack = append(stack, dirStackItem{path: fullPath, depth: item.depth + 1})
				} else {
					// File entry
					if matchesExcludePattern(opts.ExcludePattern, name) {
						continue
					}
					if opts.FilterUnsupportedFiles && !isSupportedByGooglePhotos(name) {
						continue
					}
					clean := filepath.Clean(fullPath)
					key := clean
					if runtime.GOOS == "windows" {
						key = strings.ToLower(key)
					}
					if !seenFiles[key] {
						seenFiles[key] = true
						supportedFiles = append(supportedFiles, clean)
						emitProgress(item.path, true)
					}
				}
			}
		}
	}

	// Final completion progress emit
	emitProgress("", false)
	return supportedFiles, nil
}
