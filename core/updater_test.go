package core

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestCheckForUpdates_RedirectParsing(t *testing.T) {
	// Tests redirect parsing without following redirect
	var requestedPath string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		requestedPath = r.URL.Path
		if r.URL.Path == "/releases/latest" {
			// GitHub returns 302 redirect with Location pointing to tag
			w.Header().Set("Location", "/Reiux/Lumen/releases/tag/v1.2.0")
			w.WriteHeader(http.StatusFound)
			return
		}
		w.WriteHeader(http.StatusNotFound)
	}))
	defer server.Close()

	ctx := context.Background()
	info, err := CheckForUpdatesWithURL(ctx, server.URL+"/releases/latest", "1.1.2", true, true)
	if err != nil {
		t.Fatalf("unexpected error for redirect check: %v", err)
	}
	if info.Error != "" {
		t.Fatalf("expected empty error for redirect check, got: %s", info.Error)
	}
	if !info.HasUpdate {
		t.Errorf("expected HasUpdate=true for v1.2.0 > 1.1.2")
	}
	if info.LatestVersion != "v1.2.0" {
		t.Errorf("expected LatestVersion 'v1.2.0', got '%s'", info.LatestVersion)
	}
	if !strings.Contains(info.ReleaseURL, "/tag/v1.2.0") {
		t.Errorf("expected ReleaseURL containing '/tag/v1.2.0', got '%s'", info.ReleaseURL)
	}
	if requestedPath != "/releases/latest" {
		t.Errorf("expected requestedPath '/releases/latest', got '%s'", requestedPath)
	}
}

func TestFetchReleaseNotes_304WithETag(t *testing.T) {
	ResetReleaseNotesCache()

	var reqCount int32
	var receivedETag string
	testETag := `"release-notes-v1.2.0-etag"`
	testBody := "### Changes in v1.2.0\n- Great performance improvement"

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&reqCount, 1)
		receivedETag = r.Header.Get("If-None-Match")

		if receivedETag == testETag {
			w.WriteHeader(http.StatusNotModified)
			return
		}

		w.Header().Set("ETag", testETag)
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(fmt.Sprintf(`{"tag_name": "v1.2.0", "body": %q}`, testBody)))
	}))
	defer server.Close()

	ctx := context.Background()

	// 1st request: receives 200 OK and caches notes with ETag
	notes1, err := FetchReleaseNotesWithURL(ctx, server.URL+"/tags/", "v1.2.0", "1.1.2")
	if err != nil {
		t.Fatalf("unexpected error on 1st request: %v", err)
	}
	if notes1 != testBody {
		t.Fatalf("expected notes %q, got %q", testBody, notes1)
	}
	if atomic.LoadInt32(&reqCount) != 1 {
		t.Fatalf("expected reqCount=1, got %d", reqCount)
	}

	// 2nd request: sends If-None-Match and server returns 304 Not Modified
	notes2, err := FetchReleaseNotesWithURL(ctx, server.URL+"/tags/", "v1.2.0", "1.1.2")
	if err != nil {
		t.Fatalf("unexpected error on 2nd request (304): %v", err)
	}
	if notes2 != testBody {
		t.Fatalf("expected cached notes %q on 304, got %q", testBody, notes2)
	}
	if atomic.LoadInt32(&reqCount) != 2 {
		t.Fatalf("expected reqCount=2, got %d", reqCount)
	}
	if receivedETag != testETag {
		t.Fatalf("expected server received ETag %q, got %q", testETag, receivedETag)
	}
}

func TestFetchReleaseNotes_403RateLimitWithResetHeader(t *testing.T) {
	ResetReleaseNotesCache()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch {
		case strings.HasPrefix(r.URL.Path, "/reset"):
			// 15 minutes in the future
			resetEpoch := time.Now().Add(15 * time.Minute).Unix()
			w.Header().Set("X-RateLimit-Reset", strconv.FormatInt(resetEpoch, 10))
			w.WriteHeader(http.StatusForbidden)
			_, _ = w.Write([]byte(`{"message": "API rate limit exceeded"}`))
		case strings.HasPrefix(r.URL.Path, "/retry-after"):
			w.Header().Set("Retry-After", "600") // 10 minutes
			w.WriteHeader(http.StatusTooManyRequests)
			_, _ = w.Write([]byte(`{"message": "Too Many Requests"}`))
		default:
			w.WriteHeader(http.StatusInternalServerError)
		}
	}))
	defer server.Close()

	ctx := context.Background()

	// 1. Test X-RateLimit-Reset
	_, err := FetchReleaseNotesWithURL(ctx, server.URL+"/reset", "v1.2.0", "1.1.2")
	if err == nil {
		t.Fatalf("expected rate limit error, got nil")
	}
	expectedMsg := "GitHub is limiting requests right now. Try again in 15 minutes."
	if err.Error() != expectedMsg {
		t.Errorf("expected error %q, got %q", expectedMsg, err.Error())
	}

	// 2. Test Retry-After
	_, err = FetchReleaseNotesWithURL(ctx, server.URL+"/retry-after", "v1.2.0", "1.1.2")
	if err == nil {
		t.Fatalf("expected rate limit error for Retry-After, got nil")
	}
	expectedRetryMsg := "GitHub is limiting requests right now. Try again in 10 minutes."
	if err.Error() != expectedRetryMsg {
		t.Errorf("expected error %q, got %q", expectedRetryMsg, err.Error())
	}
}

func TestCheckForUpdates_Throttle(t *testing.T) {
	restoreConfigGlobals(t)
	dir := t.TempDir()
	configPath := filepath.Join(dir, "gotohp.config")
	if err := LoadConfig(configPath); err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}

	var requestCount int32

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		atomic.AddInt32(&requestCount, 1)
		w.Header().Set("Location", "/Reiux/Lumen/releases/tag/v1.2.0")
		w.WriteHeader(http.StatusFound)
	}))
	defer server.Close()

	cm := &ConfigManager{}
	_ = cm.SetAutoCheckUpdates(true)
	// Clear any previous last check time
	cm.SetLastUpdateCheck(0)

	// Call 1: automatic background check (manual = false)
	// Must execute network request and record timestamp
	info1, err := cm.CheckForUpdatesWithRedirectURL(server.URL+"/releases/latest", false)
	if err != nil {
		t.Fatalf("unexpected error on call 1: %v", err)
	}
	if !info1.HasUpdate {
		t.Errorf("expected HasUpdate=true on call 1")
	}
	if atomic.LoadInt32(&requestCount) != 1 {
		t.Fatalf("expected 1 HTTP request, got %d", requestCount)
	}

	// Call 2: second automatic check within 12 hours (manual = false)
	// Must be throttled: make NO request
	info2, err := cm.CheckForUpdatesWithRedirectURL(server.URL+"/releases/latest", false)
	if err != nil {
		t.Fatalf("unexpected error on call 2: %v", err)
	}
	if atomic.LoadInt32(&requestCount) != 1 {
		t.Fatalf("expected request count to remain 1 (throttled), got %d", requestCount)
	}
	if info2.HasUpdate {
		t.Errorf("expected throttled check to report HasUpdate=false")
	}

	// Call 3: manual check (manual = true)
	// Manual checks must bypass the 12-hour throttle
	info3, err := cm.CheckForUpdatesWithRedirectURL(server.URL+"/releases/latest", true)
	if err != nil {
		t.Fatalf("unexpected error on manual check: %v", err)
	}
	if !info3.HasUpdate {
		t.Errorf("expected manual check to find update")
	}
	if atomic.LoadInt32(&requestCount) != 2 {
		t.Fatalf("expected request count to increase to 2 for manual check, got %d", requestCount)
	}
}

func TestCheckForUpdates_SilentOnFailure(t *testing.T) {
	restoreConfigGlobals(t)
	dir := t.TempDir()
	configPath := filepath.Join(dir, "gotohp.config")
	if err := LoadConfig(configPath); err != nil {
		t.Fatalf("LoadConfig: %v", err)
	}

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer server.Close()

	cm := &ConfigManager{}
	_ = cm.SetAutoCheckUpdates(true)
	cm.SetLastUpdateCheck(0)

	// Automatic check on failure must be silent (no error returned, no crash)
	info, err := cm.CheckForUpdatesWithRedirectURL(server.URL+"/fail", false)
	if err != nil {
		t.Fatalf("expected nil error on automatic check failure, got: %v", err)
	}
	if info.HasUpdate {
		t.Errorf("expected HasUpdate=false on failure")
	}
}
