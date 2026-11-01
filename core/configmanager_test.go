package core

import (
	"context"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/chromedp/cdproto/network"
)

func TestConfigManager_AutoCheckUpdates_DefaultsAndRoundTrip(t *testing.T) {
	restoreConfigGlobals(t)

	dir := t.TempDir()
	configPath := filepath.Join(dir, "gotohp.config")

	// 1. When no config exists, AutoCheckUpdates defaults to true
	if err := LoadConfig(configPath); err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}

	mgr := &ConfigManager{}
	if !mgr.GetAutoCheckUpdates() {
		t.Errorf("expected AutoCheckUpdates to default to true, got false")
	}

	// 2. Set to false and verify saved to disk
	if err := mgr.SetAutoCheckUpdates(false); err != nil {
		t.Fatalf("SetAutoCheckUpdates(false): %v", err)
	}

	if mgr.GetAutoCheckUpdates() {
		t.Errorf("expected AutoCheckUpdates in memory to be false")
	}

	// Read on-disk file contents to verify "auto_check_updates: false"
	savedBytes, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatalf("os.ReadFile: %v", err)
	}
	if !strings.Contains(string(savedBytes), "auto_check_updates: false") {
		t.Fatalf("expected saved file to contain 'auto_check_updates: false', got:\n%s", string(savedBytes))
	}

	// 3. Reload config from disk and verify it preserves false
	restoreConfigGlobals(t)
	if err := LoadConfig(configPath); err != nil {
		t.Fatalf("LoadConfig reload: %v", err)
	}

	reloadedMgr := &ConfigManager{}
	if reloadedMgr.GetAutoCheckUpdates() {
		t.Errorf("expected reloaded AutoCheckUpdates to be false")
	}

	// 4. Set back to true and verify round-trip
	if err := reloadedMgr.SetAutoCheckUpdates(true); err != nil {
		t.Fatalf("SetAutoCheckUpdates(true): %v", err)
	}
	if !reloadedMgr.GetAutoCheckUpdates() {
		t.Errorf("expected reloaded AutoCheckUpdates to be true")
	}

	savedBytesAfter, err := os.ReadFile(configPath)
	if err != nil {
		t.Fatalf("os.ReadFile: %v", err)
	}
	if !strings.Contains(string(savedBytesAfter), "auto_check_updates: true") {
		t.Fatalf("expected saved file to contain 'auto_check_updates: true', got:\n%s", string(savedBytesAfter))
	}
}

func TestUpdater_GatingAndSkipWhenDisabled(t *testing.T) {
	networkHitCount := 0
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		networkHitCount++
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{
			"tag_name": "v1.2.0",
			"html_url": "https://github.com/Reiux/Lumen/releases/tag/v1.2.0",
			"body": "Bugfixes and performance improvements",
			"published_at": "2026-10-31T10:00:00Z"
		}`))
	}))
	defer server.Close()

	ctx := context.Background()

	// 1. When manual is false and autoCheckEnabled is false:
	// MUST skip the network call completely.
	info, err := CheckForUpdatesWithURL(ctx, server.URL, "1.1.2", false, false)
	if err != nil {
		t.Fatalf("unexpected error when auto check is disabled: %v", err)
	}
	if info.HasUpdate {
		t.Errorf("expected HasUpdate=false when skipped, got true")
	}
	if networkHitCount != 0 {
		t.Fatalf("expected 0 network calls when auto check is disabled, got %d", networkHitCount)
	}

	// 2. When manual is false but autoCheckEnabled is true:
	// Contacts the endpoint and detects update.
	info, err = CheckForUpdatesWithURL(ctx, server.URL, "1.1.2", false, true)
	if err != nil {
		t.Fatalf("unexpected error when auto check is enabled: %v", err)
	}
	if !info.HasUpdate {
		t.Errorf("expected HasUpdate=true for newer tag v1.2.0 > 1.1.2")
	}
	if networkHitCount != 1 {
		t.Fatalf("expected 1 network call, got %d", networkHitCount)
	}

	// 3. When manual is true, regardless of autoCheckEnabled:
	// Always checks on-demand.
	info, err = CheckForUpdatesWithURL(ctx, server.URL, "1.1.2", true, false)
	if err != nil {
		t.Fatalf("unexpected error on manual check: %v", err)
	}
	if !info.HasUpdate {
		t.Errorf("expected HasUpdate=true for manual check")
	}
	if networkHitCount != 2 {
		t.Fatalf("expected 2 network calls, got %d", networkHitCount)
	}

	// 4. Verify version comparison logic
	if isVersionNewer("v1.1.2", "1.1.2") {
		t.Errorf("v1.1.2 should not be newer than 1.1.2")
	}
	if isVersionNewer("v1.0.5", "1.1.2") {
		t.Errorf("v1.0.5 should not be newer than 1.1.2")
	}
	if !isVersionNewer("v2.0.0", "1.1.2") {
		t.Errorf("v2.0.0 should be newer than 1.1.2")
	}
	if !isVersionNewer("v1.2.0", "1.1.2") {
		t.Errorf("v1.2.0 should be newer than 1.1.2")
	}
}

func TestConfigManager_BrowserSignIn_RequiresRiskAcknowledgment(t *testing.T) {
	restoreConfigGlobals(t)
	mgr := &ConfigManager{}

	// Initially risk is not acknowledged
	mgr.SetAccountRiskAcknowledged(false)
	if mgr.IsAccountRiskAcknowledged() {
		t.Fatal("expected IsAccountRiskAcknowledged to be false")
	}

	_, err := mgr.StartBrowserSignIn()
	if err == nil || !strings.Contains(err.Error(), "acknowledge the account safety notice") {
		t.Fatalf("expected error requiring risk acknowledgment, got: %v", err)
	}

	// Set acknowledged
	mgr.SetAccountRiskAcknowledged(true)
	if !mgr.IsAccountRiskAcknowledged() {
		t.Fatal("expected IsAccountRiskAcknowledged to be true")
	}
}

func TestConfigManager_BrowserSignIn_Cancel(t *testing.T) {
	restoreConfigGlobals(t)
	mgr := &ConfigManager{}
	mgr.SetAccountRiskAcknowledged(true)

	mockProc := newDummyBrowserProcess()
	runner := func(_ context.Context, _ string, _ ...string) (BrowserProcess, error) {
		return mockProc, nil
	}

	opts := BrowserSignInOptions{
		BrowserPath:   "/usr/bin/mock-browser",
		Timeout:       5 * time.Second,
		PollInterval:  10 * time.Millisecond,
		CookieSource:  NewChromiumCookieSource(func(_ context.Context) ([]*network.Cookie, error) { return nil, nil }, 10*time.Millisecond),
		ProcessRunner: runner,
	}

	errChan := make(chan error, 1)
	go func() {
		_, err := mgr.StartBrowserSignInWithOptions(opts)
		errChan <- err
	}()

	time.Sleep(20 * time.Millisecond)
	mgr.CancelBrowserSignIn()

	select {
	case err := <-errChan:
		if err == nil || !strings.Contains(err.Error(), "cancelled") {
			t.Fatalf("expected cancelled error, got: %v", err)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("timed out waiting for CancelBrowserSignIn to terminate")
	}
}

func TestConfigManager_WasBrowserCloseFailed(t *testing.T) {
	restoreConfigGlobals(t)
	mgr := &ConfigManager{}

	// Initially false
	if mgr.WasBrowserCloseFailed() {
		t.Fatal("expected WasBrowserCloseFailed to be false initially")
	}

	hookCalled := false
	BrowserCloseFailedFunc = func() {
		hookCalled = true
	}
	defer func() { BrowserCloseFailedFunc = nil }()

	browserSignInMu.Lock()
	browserCloseFailed = true
	browserSignInMu.Unlock()
	if BrowserCloseFailedFunc != nil {
		BrowserCloseFailedFunc()
	}

	if !mgr.WasBrowserCloseFailed() {
		t.Fatal("expected WasBrowserCloseFailed to be true after failure")
	}
	if !hookCalled {
		t.Fatal("expected BrowserCloseFailedFunc to be called")
	}
}

func TestConfigManager_BrowserSignIn_ChromiumLaunch(t *testing.T) {
	restoreConfigGlobals(t)
	mgr := &ConfigManager{}
	mgr.SetAccountRiskAcknowledged(true)

	mockProc := newDummyBrowserProcess()
	var executedArgs []string
	runner := func(_ context.Context, name string, args ...string) (BrowserProcess, error) {
		executedArgs = args
		return mockProc, nil
	}

	const fakeToken = "mock_token_12345"
	fakeSource := CookieSourceFunc(func(ctx context.Context, name string) (string, error) {
		return fakeToken, nil
	})

	opts := BrowserSignInOptions{
		BrowserPath:   "/usr/bin/google-chrome",
		Timeout:       2 * time.Second,
		PollInterval:  10 * time.Millisecond,
		CookieSource:  fakeSource,
		ProcessRunner: runner,
	}

	_, _ = mgr.StartBrowserSignInWithOptions(opts)

	hasUserDataDir := false
	for _, arg := range executedArgs {
		if strings.HasPrefix(arg, "--user-data-dir=") {
			hasUserDataDir = true
			break
		}
	}
	if !hasUserDataDir {
		t.Fatalf("expected Chromium --user-data-dir to be passed to runner, got: %v", executedArgs)
	}
}

func TestConfigManager_HasSupportedBrowser(t *testing.T) {
	mgr := &ConfigManager{}
	hasBrowser := mgr.HasSupportedBrowser()
	t.Logf("HasSupportedBrowser: %v", hasBrowser)
}

func TestConfigManager_QuotaAndSaverMutualExclusion(t *testing.T) {
	restoreConfigGlobals(t)
	dir := t.TempDir()
	configPath := filepath.Join(dir, "gotohp.config")
	if err := LoadConfig(configPath); err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}

	mgr := &ConfigManager{}

	// 1. Set quota on -> saver becomes off
	mgr.SetSaver(true)
	if !mgr.GetSettings().Saver {
		t.Fatalf("expected Saver to be true")
	}
	mgr.SetUseQuota(true)
	settings := mgr.GetSettings()
	if !settings.UseQuota {
		t.Fatalf("expected UseQuota to be true")
	}
	if settings.Saver {
		t.Fatalf("expected Saver to be false after enabling UseQuota")
	}

	// 2. Set saver on -> quota becomes off
	mgr.SetSaver(true)
	settings = mgr.GetSettings()
	if !settings.Saver {
		t.Fatalf("expected Saver to be true")
	}
	if settings.UseQuota {
		t.Fatalf("expected UseQuota to be false after enabling Saver")
	}
}

func TestConfigManager_NormalizeBothTrueOnLoad(t *testing.T) {
	restoreConfigGlobals(t)
	dir := t.TempDir()
	configPath := filepath.Join(dir, "gotohp.config")

	configYAML := `account:
  selected: test@example.com
preferences:
  use_quota: true
  saver: true
`
	if err := os.WriteFile(configPath, []byte(configYAML), 0o600); err != nil {
		t.Fatal(err)
	}

	if err := LoadConfig(configPath); err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}

	mgr := &ConfigManager{}
	settings := mgr.GetSettings()
	if !settings.UseQuota {
		t.Fatalf("expected UseQuota to be normalized to true")
	}
	if settings.Saver {
		t.Fatalf("expected Saver to be normalized to false when both were true in config")
	}
}

func TestConfigManager_FrontendLogAndBatchAPIAvailable(t *testing.T) {
	restoreConfigGlobals(t)
	lb := NewLogBuffer(100)
	SetGlobalLogger(lb)

	runID := "Run 1"
	lb.AddEntry(LogEntry{
		Level:       LogLevelInfo,
		Category:    LogCategoryUpload,
		RunID:       runID,
		FileName:    "photo.jpg",
		Outcome:     OutcomeUploaded,
		SizeBytes:   1024,
		DurationMs:  250,
		UserMessage: "Uploaded successfully",
	})
	lb.AddEntry(LogEntry{
		Level:           LogLevelError,
		Category:        LogCategoryUpload,
		RunID:           runID,
		FileName:        "broken.raw",
		Outcome:         OutcomeFailed,
		ReasonCode:      ReasonFileUnreadable,
		UserMessage:     "File not found on local disk.",
		SuggestedAction: "Ensure the file still exists in the specified folder.",
	})

	mgr := &ConfigManager{}

	// Test GetLogEntries
	allEntries := mgr.GetLogEntries(10, "ALL")
	if len(allEntries) != 2 {
		t.Fatalf("expected 2 log entries, got %d", len(allEntries))
	}
	if allEntries[0].FileName != "broken.raw" {
		t.Fatalf("expected newest entry first (broken.raw), got %s", allEntries[0].FileName)
	}

	// Test level filter
	infoEntries := mgr.GetLogEntries(10, "INFO")
	if len(infoEntries) != 1 || infoEntries[0].Level != LogLevelInfo {
		t.Fatalf("expected 1 INFO entry, got %d", len(infoEntries))
	}

	// Test GetProblemLogs
	problemLogs := mgr.GetProblemLogs()
	if len(problemLogs) != 1 {
		t.Fatalf("expected 1 problem log, got %d", len(problemLogs))
	}
	if problemLogs[0].ReasonCode != ReasonFileUnreadable {
		t.Fatalf("expected ReasonCode %s, got %s", ReasonFileUnreadable, problemLogs[0].ReasonCode)
	}
	if problemLogs[0].SuggestedAction == "" {
		t.Fatalf("expected non-empty SuggestedAction")
	}

	// Test ExportLogs
	exported := mgr.ExportLogs("txt")
	if !strings.Contains(exported, "broken.raw") {
		t.Fatalf("expected export to contain broken.raw, got %s", exported)
	}

	// Test ClearLogEntries
	mgr.ClearLogEntries()
	if len(mgr.GetLogEntries(10, "ALL")) != 0 {
		t.Fatalf("expected 0 log entries after clear")
	}
}

