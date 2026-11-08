package core

import (
	"context"
	"encoding/base64"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"
)

// FilesDroppedEvent is emitted when files are dropped on any drop zone
type FilesDroppedEvent struct {
	Files    []string `json:"files"`
	DropZone string   `json:"dropZone"`
}

// StartUploadEvent is received from frontend to start upload
type StartUploadEvent struct {
	Files []string `json:"files"`
}

type UploadManager struct {
	mu       sync.Mutex
	wg       sync.WaitGroup
	cancel   chan struct{}
	canceled bool
	running  bool
	reporter UploadReporter
	logger   *slog.Logger
	// apiOptions is captured from the options of the run in progress.
	apiOptions ApiOptions
}

// NewUploadManager creates a manager that reports progress to reporter. A nil
// reporter or logger discards output.
func NewUploadManager(reporter UploadReporter, logger *slog.Logger) *UploadManager {
	if reporter == nil {
		reporter = NopReporter{}
	}
	if logger == nil {
		logger = slog.New(slog.DiscardHandler)
	}
	return &UploadManager{
		reporter: reporter,
		logger:   logger,
	}
}

func (m *UploadManager) IsRunning() bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.running
}

func (m *UploadManager) Cancel() {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.cancel != nil && !m.canceled {
		close(m.cancel)
		m.canceled = true
		// Don't set to nil - readers still need to detect closure via select
	}
}

// isCancelled checks if cancellation has been requested
func (m *UploadManager) isCancelled() bool {
	m.mu.Lock()
	cancel := m.cancel
	m.mu.Unlock()
	if cancel == nil {
		return false
	}
	select {
	case <-cancel:
		return true
	default:
		return false
	}
}

// getCancelChan returns the cancel channel safely
func (m *UploadManager) getCancelChan() <-chan struct{} {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.cancel
}

type UploadBatchStart struct {
	Total      int   `json:"Total"`
	TotalBytes int64 `json:"TotalBytes"`
}

type FileUploadResult struct {
	MediaKey     string   `json:"MediaKey"`
	IsError      bool     `json:"IsError"`
	IsLivePhoto  bool     `json:"IsLivePhoto"`
	Skipped      bool     `json:"Skipped"`
	SkipCode     string   `json:"SkipCode"`
	SkipReason   string   `json:"SkipReason"`
	Error        error    `json:"-"`
	ErrorMessage string   `json:"ErrorMessage"`
	Path         string   `json:"Path"`
	Paths        []string `json:"Paths"`
	DurationMs   int64    `json:"DurationMs,omitempty"`
	SizeBytes    int64    `json:"SizeBytes,omitempty"`
}

type ThreadStatus struct {
	WorkerID      int    `json:"WorkerID"`
	Status        string `json:"Status"` // "idle", "hashing", "checking", "uploading", "finalizing", "completed", "skipped", "error"
	FilePath      string `json:"FilePath"`
	FileName      string `json:"FileName"`
	Message       string `json:"Message"`
	BytesUploaded int64  `json:"BytesUploaded"`
	BytesTotal    int64  `json:"BytesTotal"`
	Attempt       int    `json:"Attempt"` // Current attempt number (1-based), 0 if not applicable
}

// Upload starts an upload run in the background. Progress is delivered through
// Upload starts an upload run in the background. Progress is delivered through
// the manager's UploadReporter; UploadStop marks the end of the run.
func (m *UploadManager) Upload(paths []string, opts UploadOptions) {
	opts = opts.normalized()
	m.mu.Lock()
	if m.running {
		m.mu.Unlock()
		return
	}
	m.running = true
	m.cancel = make(chan struct{})
	m.canceled = false
	m.apiOptions = opts.Api
	m.mu.Unlock()

	runID := NextRunID()
	startTime := time.Now()

	m.logEntry(LogEntry{
		Level:       LogLevelInfo,
		Category:    LogCategoryUpload,
		RunID:       runID,
		UserMessage: fmt.Sprintf("Backup started (%d paths, %d threads)", len(paths), opts.Threads),
	})

	// Make preflight visible immediately so a long directory or metadata scan can
	// be cancelled from the UI.
	m.reporter.UploadStart(UploadBatchStart{})

	firstDir := ""
	if len(paths) > 0 {
		firstDir = paths[0]
	}
	m.reporter.ScanProgress(ScanProgressEvent{
		FilesFound:       0,
		CurrentDirectory: firstDir,
		IsScanning:       true,
	})

	scanCtx, scanCancel := context.WithCancel(context.Background())
	SetActiveScanCancel(scanCancel)
	defer func() {
		SetActiveScanCancel(nil)
		scanCancel()
	}()

	go func() {
		select {
		case <-m.cancel:
			scanCancel()
		case <-scanCtx.Done():
		}
	}()

	targetPaths, err := ScanDirectoriesIncremental(scanCtx, paths, ScanOptions{
		Recursive:              opts.Recursive,
		ExcludePattern:         opts.ExcludePattern,
		MaxDepth:               DefaultMaxScanDepth,
		FilterUnsupportedFiles: !opts.DisableUnsupportedFilesFilter,
		ProgressCallback: func(event ScanProgressEvent) {
			m.reporter.ScanProgress(event)
		},
	})
	if err != nil {
		if errors.Is(err, context.Canceled) {
			m.reporter.ScanCancelled()
			m.logEntry(LogEntry{
				Level:       LogLevelWarn,
				Category:    LogCategoryScanner,
				RunID:       runID,
				UserMessage: "Directory scanning was cancelled",
			})
			m.finish()
			return
		}
		m.logEntry(LogEntry{
			Level:           LogLevelError,
			Category:        LogCategoryScanner,
			RunID:           runID,
			ReasonCode:      ReasonFileUnreadable,
			UserMessage:     "Error scanning target folders",
			SuggestedAction: "Check folder permissions and file paths.",
			RawError:        err.Error(),
		})
		m.reporter.FileResult(FileUploadResult{
			IsError:      true,
			Error:        err,
			ErrorMessage: err.Error(),
		})
		m.finish()
		return
	}

	workItems, preflightWarnings := ClassifyUploadWork(targetPaths, LivePhotoClassificationOptions{
		Enabled:             opts.PairLivePhotos,
		SkipIncomplete:      opts.SkipIncompleteLivePhotos,
		IgnoreAppleMetadata: opts.IgnoreAppleMetadata,
		Cancelled:           m.isCancelled,
	}, nil)
	if m.isCancelled() {
		m.finish()
		return
	}
	m.reportPreflight(runID, len(workItems), preflightWarnings)

	if len(workItems) == 0 {
		m.logEntry(LogEntry{
			Level:       LogLevelInfo,
			Category:    LogCategoryUpload,
			RunID:       runID,
			UserMessage: "No supported media files found to upload.",
		})
		m.finish()
		return
	}

	if _, err := NewApi(opts.Api); err != nil {
		for _, item := range workItems {
			path := uploadWorkPrimaryPath(item)
			res := FileUploadResult{
				IsError:      true,
				Error:        err,
				ErrorMessage: err.Error(),
				Path:         path,
			}
			m.reporter.FileResult(res)
			m.recordFileResult(runID, res)
		}
		m.finish()
		return
	}

	// Calculate total bytes asynchronously and emit update when complete
	go func() {
		var totalBytes int64
		for _, item := range workItems {
			for _, path := range uploadWorkPaths(item) {
				if info, err := os.Stat(path); err == nil {
					totalBytes += info.Size()
				}
			}
		}
		m.reporter.TotalBytes(totalBytes)
	}()

	// Don't start more threads than files to process
	numWorkers := min(opts.Threads, len(workItems))

	// Shared TokenManager for this upload run so all concurrent workers share one refreshed token
	tokenManager := NewTokenManager(opts.AccountEmail, nil)

	// CircuitBreaker state tracker for account blocks and rate limits
	circuitBreaker := NewCircuitBreaker(m.Cancel, func(event AccountBlockedEvent) {
		m.logger.Warn("Circuit breaker tripped", "reason", event.Reason, "email", event.Email, "message", event.Message)
		m.reporter.AccountBlocked(event)
	})

	// Create a worker pool for concurrent uploads
	workChan := make(chan UploadWorkItem, len(workItems))
	results := make(chan FileUploadResult, len(workItems))

	// Start workers
	for i := range numWorkers {
		m.wg.Add(1)
		go m.runWorker(i, workChan, results, opts, tokenManager, circuitBreaker)
	}

	// Send work to workers
	go func() {
	LOOP:
		for _, item := range workItems {
			select {
			case <-m.cancel:
				break LOOP
			case workChan <- item:
			}
		}
		close(workChan)
	}()

	// Handle results, wait for completion, and create album if configured
	go func() {
		// Collect successful uploads with path -> mediaKey mapping for AUTO mode
		successfulUploads := make(map[string]string) // path -> mediaKey
		var countUploaded, countSkipped, countFailed int
		var totalUploadedBytes int64

		// Wait for all workers to finish in a separate goroutine, then close results
		go func() {
			m.wg.Wait()
			close(results)
		}()

		// Process all results (this blocks until results channel is closed)
		for result := range results {
			m.reporter.FileResult(result)
			m.recordFileResult(runID, result)

			if result.IsError {
				countFailed++
				m.logger.Error("upload error", "path", result.Path, "error", result.Error)
			} else if result.Skipped {
				countSkipped++
			} else {
				countUploaded++
				totalUploadedBytes += result.SizeBytes
				m.logger.Info("upload success", "path", result.Path)
				if result.MediaKey != "" {
					successfulUploads[result.Path] = result.MediaKey
				}
			}
		}

		elapsed := time.Since(startTime)
		var avgSpeedKBps float64
		if elapsed.Seconds() > 0 && totalUploadedBytes > 0 {
			avgSpeedKBps = float64(totalUploadedBytes) / 1024.0 / elapsed.Seconds()
		}
		m.logEntry(LogEntry{
			Level:       LogLevelInfo,
			Category:    LogCategoryUpload,
			RunID:       runID,
			SizeBytes:   totalUploadedBytes,
			DurationMs:  elapsed.Milliseconds(),
			UserMessage: fmt.Sprintf("Backup %s finished: %d uploaded, %d skipped, %d failed (%.1f MB in %s, avg %.1f KB/s)",
				runID, countUploaded, countSkipped, countFailed, float64(totalUploadedBytes)/(1024*1024), elapsed.Round(time.Second), avgSpeedKBps),
		})

		m.logger.Info("upload complete", "successful", len(successfulUploads),
			"albumName", opts.AlbumName, "albumAutoMode", opts.AlbumAutoMode)

		if len(successfulUploads) > 0 {
			m.handleAlbumCreation(successfulUploads, opts.AlbumName, opts.AlbumAutoMode)
		}

		m.finish()
	}()
}

// finish signals the end of a run and releases the manager for the next one.
func (m *UploadManager) finish() {
	m.reporter.UploadStop()
	m.mu.Lock()
	m.running = false
	m.mu.Unlock()
}

func (m *UploadManager) reportPreflight(runID string, uploadItemCount int, warnings []PreflightWarning) {
	skippedCount := 0
	for _, warning := range warnings {
		if isSkippedPreflightWarning(warning.Code) {
			skippedCount++
		}
	}

	// Start before emitting warnings/results so the frontend resets its previous
	// batch while retaining every event from this preflight.
	m.reporter.UploadStart(UploadBatchStart{
		Total:      uploadItemCount + skippedCount,
		TotalBytes: 0,
	})
	for _, warning := range warnings {
		m.reporter.Warning(warning)
		if !isSkippedPreflightWarning(warning.Code) {
			continue
		}
		primaryPath := ""
		if len(warning.Paths) > 0 {
			primaryPath = warning.Paths[0]
		}
		res := FileUploadResult{
			IsLivePhoto: true,
			Skipped:     true,
			SkipCode:    warning.Code,
			SkipReason:  warning.Message,
			Path:        primaryPath,
			Paths:       warning.Paths,
		}
		m.reporter.FileResult(res)
		m.recordFileResult(runID, res)
	}
}

func (m *UploadManager) recordFileResult(runID string, result FileUploadResult) {
	entry := LogEntry{
		RunID:      runID,
		FileName:   filepath.Base(result.Path),
		FilePath:   SanitizeUserPath(result.Path),
		SizeBytes:  result.SizeBytes,
		DurationMs: result.DurationMs,
		Category:   LogCategoryUpload,
	}

	if result.IsError {
		entry.Level = LogLevelError
		entry.Outcome = OutcomeFailed
		entry.RawError = result.ErrorMessage
		prob := MapErrorToProblem(result.Error)
		if prob.ReasonCode == "" && result.ErrorMessage != "" {
			prob = MapErrorToProblem(errors.New(result.ErrorMessage))
		}
		entry.ReasonCode = prob.ReasonCode
		entry.UserMessage = prob.UserMessage
		entry.SuggestedAction = prob.SuggestedAction
	} else if result.Skipped {
		entry.Level = LogLevelInfo
		entry.Outcome = OutcomeSkippedRemoteDedup
		if result.SkipCode == "hash-cache" {
			entry.Outcome = OutcomeSkippedCache
		}
		entry.ReasonCode = result.SkipCode
		entry.UserMessage = result.SkipReason
		if entry.UserMessage == "" {
			entry.UserMessage = "File skipped (already in library)"
		}
	} else {
		entry.Level = LogLevelInfo
		entry.Outcome = OutcomeUploaded
		entry.UserMessage = "Uploaded successfully"
	}

	m.logEntry(entry)
}

func (m *UploadManager) logEntry(entry LogEntry) {
	GetGlobalLogger().AddEntry(entry)
	m.reporter.LogEntry(entry)
}

func isSkippedPreflightWarning(code string) bool {
	return code == "incomplete-live-photo-skipped" || code == "ambiguous-filename-stem"
}

// handleAlbumCreation handles album creation based on config (manual name/key or AUTO mode)
func (m *UploadManager) handleAlbumCreation(uploads map[string]string, albumName string, albumAutoMode bool) {
	// Check if cancelled before starting album creation
	if m.isCancelled() {
		m.logger.Info("Upload cancelled, skipping album creation")
		return
	}

	m.logger.Info(fmt.Sprintf("handleAlbumCreation called with %d uploads", len(uploads)))

	// Create API once for all album operations
	api, err := NewApi(m.apiOptions)
	if err != nil {
		m.logger.Error(fmt.Sprintf("failed to create API for album creation: %v", err))
		m.reporter.AlbumError(AlbumError{
			AlbumName: albumName,
			Error:     fmt.Sprintf("failed to initialize API: %v", err),
		})
		return
	}

	albumManager := NewAlbumManager(api, m.reporter, m.logger, m.getCancelChan())

	// Check if AUTO mode is enabled
	if albumAutoMode {
		m.logger.Info("AUTO mode enabled, creating albums from directories")
		m.createAlbumsFromDirectories(albumManager, uploads)
		return
	}

	// Manual mode: use AlbumName if set
	if albumName == "" {
		m.logger.Info("No album name set and AUTO mode disabled, skipping album creation")
		return
	}

	m.logger.Info(fmt.Sprintf("Creating album with name/key: '%s'", albumName))

	mediaKeys := make([]string, 0, len(uploads))
	for _, mediaKey := range uploads {
		mediaKeys = append(mediaKeys, mediaKey)
	}

	m.logger.Info(fmt.Sprintf("Adding %d media keys to album '%s'", len(mediaKeys), albumName))

	albumKeys, err := albumManager.AddToAlbum(mediaKeys, albumName)
	if err != nil {
		m.logger.Error(fmt.Sprintf("failed to create album '%s': %v", albumName, err))
		m.reporter.AlbumError(AlbumError{
			AlbumName: albumName,
			Error:     err.Error(),
		})
		return
	}
	m.logger.Info(fmt.Sprintf("created album '%s' with %d items, album keys: %v", albumName, len(mediaKeys), albumKeys))
}

// createAlbumsFromDirectories creates albums based on parent directory names (AUTO mode)
func (m *UploadManager) createAlbumsFromDirectories(albumManager *AlbumManager, uploads map[string]string) {
	// Group media keys by parent directory
	mediaKeysByDir := make(map[string][]string)

	for filePath, mediaKey := range uploads {
		parentDir := filepath.Dir(filePath)
		mediaKeysByDir[parentDir] = append(mediaKeysByDir[parentDir], mediaKey)
	}

	// Create an album for each directory
	for dirPath, mediaKeys := range mediaKeysByDir {
		albumName := filepath.Base(dirPath)
		if albumName == "" || albumName == "." {
			albumName = "Uploads"
		}

		albumKeys, err := albumManager.AddToAlbum(mediaKeys, albumName)
		if err != nil {
			m.logger.Error(fmt.Sprintf("failed to create album '%s': %v", albumName, err))
			m.reporter.AlbumError(AlbumError{
				AlbumName: albumName,
				Error:     err.Error(),
			})
			continue
		}
		m.logger.Info(fmt.Sprintf("created album '%s' with %d items, album keys: %v", albumName, len(mediaKeys), albumKeys))
	}
}

// supportedFormats is a map of file extensions supported by Google Photos (O(1) lookup)
var supportedFormats = map[string]bool{
	// Photo formats
	"avif": true, "bmp": true, "gif": true, "heic": true, "heif": true, "ico": true,
	"jpg": true, "jpeg": true, "png": true, "tif": true, "tiff": true, "webp": true,
	"cr2": true, "cr3": true, "nef": true, "arw": true, "orf": true,
	"raf": true, "rw2": true, "pef": true, "sr2": true, "dng": true,
	// Video formats
	"3gp": true, "3g2": true, "asf": true, "avi": true, "divx": true,
	"m2t": true, "m2ts": true, "m4v": true, "mkv": true, "mmv": true,
	"mod": true, "mov": true, "mp4": true, "mpg": true, "mpeg": true,
	"mts": true, "tod": true, "wmv": true, "ts": true, "webm": true,
}

// isSupportedByGooglePhotos checks if a file extension is supported by Google Photos
func isSupportedByGooglePhotos(filename string) bool {
	ext := strings.ToLower(filepath.Ext(filename))
	if ext == "" {
		return false
	}
	// Remove the dot and check map
	return supportedFormats[ext[1:]]
}

func scanDirectoryForFiles(path string, recursive bool, excludePattern string, cancelled func() bool) ([]string, error) {
	var files []string

	if cancelled != nil && cancelled() {
		return nil, context.Canceled
	}
	entries, err := os.ReadDir(path)
	if err != nil {
		return nil, err
	}

	for _, entry := range entries {
		if cancelled != nil && cancelled() {
			return nil, context.Canceled
		}
		fullPath := filepath.Join(path, entry.Name())
		if entry.IsDir() {
			if excludePattern != "" && entry.Name() == excludePattern {
				continue
			}
			if recursive {
				subFiles, err := scanDirectoryForFiles(fullPath, recursive, excludePattern, cancelled)
				if err != nil {
					if errors.Is(err, context.Canceled) {
						return nil, err
					}
					continue
				}
				files = append(files, subFiles...)
			}
		} else {
			files = append(files, fullPath)
		}
	}

	return files, nil
}

// FilterGooglePhotosFiles expands paths into the files that would be uploaded
// under opts: directories are scanned, unsupported files dropped, duplicates removed.
func FilterGooglePhotosFiles(paths []string, opts UploadOptions) ([]string, error) {
	return filterGooglePhotosFilesWithCancel(paths, opts, nil, nil)
}

func filterGooglePhotosFilesWithCancel(paths []string, opts UploadOptions, cancelled func() bool, progress func(ScanProgressEvent)) ([]string, error) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	if cancelled != nil {
		go func() {
			for {
				if cancelled() {
					cancel()
					return
				}
				if ctx.Err() != nil {
					return
				}
				time.Sleep(30 * time.Millisecond)
			}
		}()
	}

	return ScanDirectoriesIncremental(ctx, paths, ScanOptions{
		Recursive:              opts.Recursive,
		ExcludePattern:         opts.ExcludePattern,
		MaxDepth:               DefaultMaxScanDepth,
		FilterUnsupportedFiles: !opts.DisableUnsupportedFilesFilter,
		ProgressCallback:       progress,
	})
}

func canonicalUploadPath(path string) string {
	canonicalPath, err := filepath.Abs(path)
	if err != nil {
		canonicalPath = filepath.Clean(path)
	}
	if resolved, err := filepath.EvalSymlinks(canonicalPath); err == nil {
		canonicalPath = resolved
	}
	return filepath.Clean(canonicalPath)
}

// UploadFile uploads a single file and returns its media key.
func UploadFile(ctx context.Context, api *Api, filePath string, opts UploadOptions, workerID int, reporter UploadReporter) (string, error) {
	if reporter == nil {
		reporter = NopReporter{}
	}
	key, _, err := uploadSingleFile(ctx, api, filePath, opts, workerID, reporter)
	return key, err
}

func uploadSingleFile(ctx context.Context, api *Api, filePath string, opts UploadOptions, workerID int, reporter UploadReporter) (string, bool, error) {
	fileName := filepath.Base(filePath)
	mediakey := ""

	fileInfo, err := os.Stat(filePath)
	if err != nil {
		return "", false, fmt.Errorf("error getting file info: %w", err)
	}

	// Determine the timestamp to use for the upload.
	// Default to file mtime, but allow filename-based timestamp to take precedence if enabled.
	uploadTimestamp := fileInfo.ModTime().Unix()
	if opts.SetDateFromFilename {
		if t, ok := parseTimestampFromFilename(filePath); ok {
			uploadTimestamp = t.Unix()
		}
	}

	// Stage 1: Hashing
	reporter.ThreadStatus(ThreadStatus{
		WorkerID: workerID,
		Status:   "hashing",
		FilePath: filePath,
		FileName: fileName,
		Message:  "Hashing...",
	})

	sha1_hash_bytes, err := CalculateSHA1(ctx, filePath)
	if err != nil {
		return "", false, fmt.Errorf("error calculating hash file: %w", err)
	}

	sha1_hash_b64 := base64.StdEncoding.EncodeToString([]byte(sha1_hash_bytes))

	// Stage 1.5: Checking local hash cache
	if !opts.ForceUpload && opts.UseLocalHashCache && opts.AccountEmail != "" {
		if cache := GetGlobalHashCache(); cache != nil {
			if cachedKey, ok, _ := cache.Get(opts.AccountEmail, sha1_hash_b64, fileInfo.Size()); ok && cachedKey != "" {
				reporter.TotalBytesDelta(-fileInfo.Size())
				reporter.ThreadStatus(ThreadStatus{
					WorkerID: workerID,
					Status:   "skipped",
					FilePath: filePath,
					FileName: fileName,
					Message:  "Already in Google Photos",
				})
				if opts.DeleteFromHost {
					if err := os.Remove(filePath); err != nil {
						return cachedKey, true, fmt.Errorf("file exists in library but failed to delete local copy: %w", err)
					}
				}
				return cachedKey, true, nil
			}
		}
	}

	// Stage 2: Checking if exists in library
	if !opts.ForceUpload {
		reporter.ThreadStatus(ThreadStatus{
			WorkerID: workerID,
			Status:   "checking",
			FilePath: filePath,
			FileName: fileName,
			Message:  "Checking if file exists in library...",
		})

		mediakey, err = api.FindRemoteMediaByHash(sha1_hash_bytes)
		if err != nil {
			// Non-fatal: log via callback and continue with upload
			reporter.ThreadStatus(ThreadStatus{
				WorkerID: workerID,
				Status:   "checking",
				FilePath: filePath,
				FileName: fileName,
				Message:  fmt.Sprintf("Hash check warning: %v, proceeding with upload", err),
			})
		}
		if len(mediakey) > 0 {
			if opts.UseLocalHashCache && opts.AccountEmail != "" {
				if cache := GetGlobalHashCache(); cache != nil {
					_ = cache.Put(opts.AccountEmail, sha1_hash_b64, mediakey, fileInfo.Size(), fileInfo.ModTime().Unix())
				}
			}
			reporter.TotalBytesDelta(-fileInfo.Size())
			reporter.ThreadStatus(ThreadStatus{
				WorkerID: workerID,
				Status:   "skipped",
				FilePath: filePath,
				FileName: fileName,
				Message:  "Already in Google Photos",
			})
			if opts.DeleteFromHost {
				if err := os.Remove(filePath); err != nil {
					return mediakey, true, fmt.Errorf("file exists in library but failed to delete local copy: %w", err)
				}
			}
			return mediakey, true, nil
		}
	}

	// Stage 3: Uploading
	fileSize := fileInfo.Size()
	reporter.ThreadStatus(ThreadStatus{
		WorkerID:      workerID,
		Status:        "uploading",
		FilePath:      filePath,
		FileName:      fileName,
		Message:       "Uploading...",
		BytesUploaded: 0,
		BytesTotal:    fileSize,
	})

	token, err := api.GetUploadToken(sha1_hash_b64, fileSize)
	if err != nil {
		return "", false, fmt.Errorf("error uploading file: %w", err)
	}

	// Create progress callback for upload
	progressCallback := func(bytesUploaded, bytesTotal int64, attempt int) {
		message := "Uploading..."
		if attempt > 1 {
			message = fmt.Sprintf("Retrying... (attempt %d)", attempt)
		}
		reporter.ThreadStatus(ThreadStatus{
			WorkerID:      workerID,
			Status:        "uploading",
			FilePath:      filePath,
			FileName:      fileName,
			Message:       message,
			BytesUploaded: bytesUploaded,
			BytesTotal:    bytesTotal,
			Attempt:       attempt,
		})
	}

	finalizeToken, err := api.UploadFileWithProgress(ctx, filePath, token, progressCallback)
	if err != nil {
		return "", false, fmt.Errorf("error uploading file: %w", err)
	}
	commitToken, err := finalizeToken.legacyCommitToken()
	if err != nil {
		return "", false, fmt.Errorf("error decoding upload finalize token: %w", err)
	}

	// Stage 4: Finalizing
	reporter.ThreadStatus(ThreadStatus{
		WorkerID: workerID,
		Status:   "finalizing",
		FilePath: filePath,
		FileName: fileName,
		Message:  "Committing upload...",
	})

	mediaKey, err := api.CommitUpload(commitToken, fileInfo.Name(), sha1_hash_bytes, uploadTimestamp)
	if err != nil {
		return "", false, fmt.Errorf("error committing file: %w", err)
	}

	if len(mediaKey) == 0 {
		return "", false, fmt.Errorf("media key not received")
	}

	if opts.UseLocalHashCache && opts.AccountEmail != "" {
		if cache := GetGlobalHashCache(); cache != nil {
			_ = cache.Put(opts.AccountEmail, sha1_hash_b64, mediaKey, fileInfo.Size(), fileInfo.ModTime().Unix())
		}
	}

	if opts.DeleteFromHost {
		if err := os.Remove(filePath); err != nil {
			return mediaKey, false, fmt.Errorf("uploaded successfully but failed to delete file: %w", err)
		}
	}

	return mediaKey, false, nil
}

func (m *UploadManager) runWorker(
	workerID int,
	workChan <-chan UploadWorkItem,
	results chan<- FileUploadResult,
	opts UploadOptions,
	tokenManager *TokenManager,
	cb *CircuitBreaker,
) {
	defer m.wg.Done()
	cancel := m.getCancelChan()

	// Emit idle status initially
	m.reporter.ThreadStatus(ThreadStatus{
		WorkerID: workerID,
		Status:   "idle",
		Message:  "Waiting for files...",
	})

	// Create API client once per worker for connection reuse
	api, err := NewApi(opts.Api)
	if err != nil {
		m.reporter.ThreadStatus(ThreadStatus{
			WorkerID: workerID,
			Status:   "error",
			Message:  fmt.Sprintf("Failed to initialize API: %v", err),
		})
		return
	}
	if tokenManager != nil {
		api.SetTokenManager(tokenManager)
	}

	for item := range workChan {
		if cb != nil && cb.IsBlocked() {
			m.reporter.ThreadStatus(ThreadStatus{
				WorkerID: workerID,
				Status:   "idle",
				Message:  "Stopped: Account blocked",
			})
			return
		}

		select {
		case <-cancel:
			m.reporter.ThreadStatus(ThreadStatus{
				WorkerID: workerID,
				Status:   "idle",
				Message:  "Cancelled",
			})
			return // Stop if cancellation is requested
		default:
		}

		path := uploadWorkPrimaryPath(item)
		paths := uploadWorkPaths(item)
		isLivePhoto := item.Kind == UploadWorkLivePhoto
		var itemSize int64
		for _, p := range paths {
			if fi, err := os.Stat(p); err == nil {
				itemSize += fi.Size()
			}
		}

		var (
			mediaKey   string
			skipped    bool
			err        error
			durationMs int64
		)

		for {
			if cb != nil {
				waitCtx, waitCancel := context.WithCancel(context.Background())
				go func() {
					select {
					case <-cancel:
						waitCancel()
					case <-waitCtx.Done():
					}
				}()
				ok := cb.WaitWhilePaused(waitCtx)
				waitCancel()
				if !ok {
					if cb.IsBlocked() {
						m.reporter.ThreadStatus(ThreadStatus{
							WorkerID: workerID,
							Status:   "idle",
							FilePath: path,
							FileName: filepath.Base(path),
							Message:  "Paused: Account issue detected",
						})
						return
					}
					m.reporter.ThreadStatus(ThreadStatus{
						WorkerID: workerID,
						Status:   "idle",
						Message:  "Cancelled",
					})
					return
				}
			}

			if m.isCancelled() {
				m.reporter.ThreadStatus(ThreadStatus{
					WorkerID: workerID,
					Status:   "idle",
					Message:  "Cancelled",
				})
				return
			}

			ctx, cancelUpload := context.WithCancel(context.Background())
			go func() {
				select {
				case <-cancel:
					cancelUpload()
				case <-ctx.Done():
					// Upload completed or context cancelled
				}
			}()

			startItem := time.Now()
			mediaKey, skipped, err = uploadWorkItem(ctx, api, item, opts, workerID, m.reporter)
			durationMs = time.Since(startItem).Milliseconds()
			cancelUpload()

			// Check if circuit breaker tripped during this item or while in flight
			if cb != nil && cb.IsBlocked() {
				m.reporter.ThreadStatus(ThreadStatus{
					WorkerID: workerID,
					Status:   "idle",
					FilePath: path,
					FileName: filepath.Base(path),
					Message:  "Paused: Account issue detected",
				})
				return
			}

			if err != nil {
				kind := ClassifyError(err)
				if kind == ErrorKindSignInRequired || kind == ErrorKindAccountRestricted {
					if cb != nil {
						cb.Trip(kind, opts.AccountEmail, err.Error())
					}
					m.reporter.ThreadStatus(ThreadStatus{
						WorkerID: workerID,
						Status:   "idle",
						FilePath: path,
						FileName: filepath.Base(path),
						Message:  "Upload stopped: " + string(kind),
					})
					// Do NOT send to results channel as failed! File is preserved.
					return
				}
				if kind == ErrorKindRateLimited {
					cooldown := 15 * time.Minute
					var rateErr *RateLimitError
					if errors.As(err, &rateErr) && rateErr.RetryAfter > 0 {
						cooldown = rateErr.RetryAfter
					}
					if cb != nil {
						pauseCtx, pauseCancel := context.WithCancel(context.Background())
						go func() {
							select {
							case <-cancel:
								pauseCancel()
							case <-pauseCtx.Done():
							}
						}()
						cb.Pause(pauseCtx, cooldown, opts.AccountEmail, err.Error())
						pauseCancel()
					}
					if cb != nil && cb.IsBlocked() {
						return
					}
					if m.isCancelled() {
						return
					}
					// Retry upload of this item after cooldown
					continue
				}
			}
			break
		}

			if err != nil && mediaKey != "" {
				results <- FileUploadResult{
					IsLivePhoto: isLivePhoto,
					Path:        path,
					Paths:       paths,
					MediaKey:    mediaKey,
					DurationMs:  durationMs,
					SizeBytes:   itemSize,
				}
				m.reporter.Warning(PreflightWarning{
					Paths:   paths,
					Code:    "local-cleanup-failed",
					Message: err.Error(),
				})
				m.reporter.ThreadStatus(ThreadStatus{
					WorkerID: workerID,
					Status:   "completed",
					FilePath: path,
					FileName: filepath.Base(path),
					Message:  fmt.Sprintf("Uploaded, but local cleanup failed: %v", err),
				})
			} else if err != nil {
				if m.isCancelled() || (cb != nil && cb.IsBlocked()) {
					// Interrupted during upload; do NOT mark failed
					return
				}
				results <- FileUploadResult{
					IsError:      true,
					IsLivePhoto:  isLivePhoto,
					Error:        err,
					ErrorMessage: err.Error(),
					Path:         path,
					Paths:        paths,
					DurationMs:   durationMs,
					SizeBytes:    itemSize,
				}
				m.reporter.ThreadStatus(ThreadStatus{
					WorkerID: workerID,
					Status:   "error",
					FilePath: path,
					FileName: filepath.Base(path),
					Message:  fmt.Sprintf("Error: %v", err),
				})
			} else if skipped {
				skipCode := "remote-duplicate"
				skipReason := "Skipped because this file already exists remotely"
				if isLivePhoto {
					skipCode = "remote-live-photo-component-exists"
					skipReason = "Skipped because a Live Photo component already exists remotely"
				}
				results <- FileUploadResult{
					IsLivePhoto: isLivePhoto,
					Skipped:     true,
					SkipCode:    skipCode,
					SkipReason:  skipReason,
					Path:        path,
					Paths:       paths,
					MediaKey:    mediaKey,
					DurationMs:  durationMs,
					SizeBytes:   itemSize,
				}
				m.reporter.ThreadStatus(ThreadStatus{
					WorkerID: workerID,
					Status:   "skipped",
					FilePath: path,
					FileName: filepath.Base(path),
					Message:  "Already in Google Photos",
				})
			} else {
				results <- FileUploadResult{
					IsLivePhoto: isLivePhoto,
					Path:        path,
					Paths:       paths,
					MediaKey:    mediaKey,
					DurationMs:  durationMs,
					SizeBytes:   itemSize,
				}
				m.reporter.ThreadStatus(ThreadStatus{
					WorkerID: workerID,
					Status:   "completed",
					FilePath: path,
					FileName: filepath.Base(path),
					Message:  "Completed",
				})
			}

			// Mark as idle after completing file
			m.reporter.ThreadStatus(ThreadStatus{
				WorkerID: workerID,
				Status:   "idle",
				Message:  "Waiting for next file...",
			})
		}

	// Final idle status when no more work
	m.reporter.ThreadStatus(ThreadStatus{
		WorkerID: workerID,
		Status:   "idle",
		Message:  "Finished",
	})
}

func uploadWorkItem(ctx context.Context, api *Api, item UploadWorkItem, opts UploadOptions, workerID int, reporter UploadReporter) (string, bool, error) {
	switch item.Kind {
	case UploadWorkSingle:
		if item.Single == nil || item.LivePhoto != nil {
			return "", false, fmt.Errorf("invalid single-media work item")
		}
		return uploadSingleFile(ctx, api, item.Single.Path, opts, workerID, reporter)
	case UploadWorkLivePhoto:
		if item.LivePhoto == nil || item.Single != nil {
			return "", false, fmt.Errorf("invalid Live Photo work item")
		}
		return uploadLivePhotoWithCallback(ctx, api, *item.LivePhoto, LivePhotoUploadOptions{
			Policy:                     buildLivePhotoCommitPolicy(api),
			DeleteFromHost:             opts.DeleteFromHost,
			SetDateFromFilename:        opts.SetDateFromFilename,
			UpdateExistingPhotosToLive: opts.UpdateExistingPhotosToLive,
			UseLocalHashCache:          opts.UseLocalHashCache,
			AccountEmail:               opts.AccountEmail,
			ForceUpload:                opts.ForceUpload,
		}, workerID, reporter)
	default:
		return "", false, fmt.Errorf("unsupported upload work kind %q", item.Kind)
	}
}

func uploadWorkPaths(item UploadWorkItem) []string {
	if item.Kind == UploadWorkLivePhoto && item.LivePhoto != nil {
		return []string{item.LivePhoto.PhotoPath, item.LivePhoto.VideoPath}
	}
	if item.Single != nil {
		return []string{item.Single.Path}
	}
	return nil
}

func uploadWorkPrimaryPath(item UploadWorkItem) string {
	paths := uploadWorkPaths(item)
	if len(paths) == 0 {
		return ""
	}
	return paths[0]
}
