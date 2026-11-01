package core

import (
	"context"
	"errors"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/chromedp/cdproto/network"
	"github.com/chromedp/cdproto/target"
)

type dummyBrowserProcess struct {
	mu     sync.Mutex
	killed bool
	done   chan struct{}
	once   sync.Once
}

func newDummyBrowserProcess() *dummyBrowserProcess {
	return &dummyBrowserProcess{
		done: make(chan struct{}),
	}
}

func (p *dummyBrowserProcess) Kill() error {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.killed = true
	if p.done != nil {
		p.once.Do(func() {
			close(p.done)
		})
	}
	return nil
}

func (p *dummyBrowserProcess) Wait() error {
	if p.done != nil {
		<-p.done
	}
	return nil
}

func (p *dummyBrowserProcess) Pid() int {
	return 12345
}

func TestBrowserSignIn_Success(t *testing.T) {
	var capturedDir string
	var callCount int
	var countMu sync.Mutex

	const fakeTokenLength = 16
	fakeTokenPayload := "0123456789abcdef" // length 16

	mockProc := newDummyBrowserProcess()
	runner := func(_ context.Context, _ string, _ ...string) (BrowserProcess, error) {
		return mockProc, nil
	}

	cookieSource := func(_ context.Context) ([]*network.Cookie, error) {
		countMu.Lock()
		defer countMu.Unlock()
		callCount++
		if callCount < 2 {
			return []*network.Cookie{}, nil
		}
		return []*network.Cookie{
			{
				Name:     "oauth_token",
				Value:    fakeTokenPayload,
				HTTPOnly: true,
			},
		}, nil
	}

	opts := BrowserSignInOptions{
		BrowserPath:   "/usr/bin/mock-browser",
		Timeout:       2 * time.Second,
		PollInterval:  10 * time.Millisecond,
		CookieSource:  NewChromiumCookieSource(cookieSource, 10*time.Millisecond),
		ProcessRunner: runner,
		OnTempDir: func(dir string) {
			capturedDir = dir
			// Verify temp dir was actually created on disk during execution
			if fi, err := os.Stat(dir); err != nil || !fi.IsDir() {
				t.Errorf("expected temp dir to exist during run: %v", err)
			}
		},
	}

	ctx := context.Background()
	token, err := RunBrowserSignInWithOptions(ctx, opts)
	if err != nil {
		t.Fatalf("expected sign-in success, got error: %v", err)
	}

	// Verify token presence and length without printing/logging value
	if len(token) != fakeTokenLength {
		t.Fatalf("expected token length %d, got %d", fakeTokenLength, len(token))
	}

	// Verify that the temporary profile directory was deleted on disk
	if capturedDir == "" {
		t.Fatal("expected temp dir path to be captured")
	}
	if _, statErr := os.Stat(capturedDir); !os.IsNotExist(statErr) {
		t.Fatalf("expected temp dir %s to be deleted after success, stat err: %v", capturedDir, statErr)
	}
}

func TestBrowserSignIn_Timeout(t *testing.T) {
	var capturedDir string
	mockProc := newDummyBrowserProcess()
	runner := func(_ context.Context, _ string, _ ...string) (BrowserProcess, error) {
		return mockProc, nil
	}

	cookieSource := func(_ context.Context) ([]*network.Cookie, error) {
		return []*network.Cookie{}, nil
	}

	opts := BrowserSignInOptions{
		BrowserPath:   "/usr/bin/mock-browser",
		Timeout:       50 * time.Millisecond,
		PollInterval:  10 * time.Millisecond,
		CookieSource:  NewChromiumCookieSource(cookieSource, 10*time.Millisecond),
		ProcessRunner: runner,
		OnTempDir: func(dir string) {
			capturedDir = dir
			if fi, err := os.Stat(dir); err != nil || !fi.IsDir() {
				t.Errorf("expected temp dir to exist during run: %v", err)
			}
		},
	}

	ctx := context.Background()
	_, err := RunBrowserSignInWithOptions(ctx, opts)
	if err == nil {
		t.Fatal("expected timeout error, got nil")
	}

	// Verify that the temporary profile directory was deleted on disk
	if capturedDir == "" {
		t.Fatal("expected temp dir path to be captured")
	}
	if _, statErr := os.Stat(capturedDir); !os.IsNotExist(statErr) {
		t.Fatalf("expected temp dir %s to be deleted after timeout, stat err: %v", capturedDir, statErr)
	}
}

func TestBrowserSignIn_Cancel(t *testing.T) {
	var capturedDir string
	mockProc := newDummyBrowserProcess()
	runner := func(_ context.Context, _ string, _ ...string) (BrowserProcess, error) {
		return mockProc, nil
	}

	cookieSource := func(_ context.Context) ([]*network.Cookie, error) {
		return []*network.Cookie{}, nil
	}

	opts := BrowserSignInOptions{
		BrowserPath:   "/usr/bin/mock-browser",
		Timeout:       5 * time.Second,
		PollInterval:  10 * time.Millisecond,
		CookieSource:  NewChromiumCookieSource(cookieSource, 10*time.Millisecond),
		ProcessRunner: runner,
		OnTempDir: func(dir string) {
			capturedDir = dir
			if fi, err := os.Stat(dir); err != nil || !fi.IsDir() {
				t.Errorf("expected temp dir to exist during run: %v", err)
			}
		},
	}

	ctx, cancel := context.WithCancel(context.Background())
	go func() {
		time.Sleep(20 * time.Millisecond)
		cancel()
	}()

	_, err := RunBrowserSignInWithOptions(ctx, opts)
	if !errors.Is(err, context.Canceled) {
		t.Fatalf("expected context.Canceled error, got: %v", err)
	}

	// Verify that the temporary profile directory was deleted on disk
	if capturedDir == "" {
		t.Fatal("expected temp dir path to be captured")
	}
	if _, statErr := os.Stat(capturedDir); !os.IsNotExist(statErr) {
		t.Fatalf("expected temp dir %s to be deleted after cancel, stat err: %v", capturedDir, statErr)
	}
}

func TestStripQueryAndFragment(t *testing.T) {
	tests := []struct {
		input    string
		expected string
	}{
		{"", ""},
		{"https://accounts.google.com/EmbeddedSetup", "https://accounts.google.com/EmbeddedSetup"},
		{"https://accounts.google.com/EmbeddedSetup?flow=1&client_id=secret", "https://accounts.google.com/EmbeddedSetup"},
		{"https://accounts.google.com/EmbeddedSetup#step1", "https://accounts.google.com/EmbeddedSetup"},
		{"https://accounts.google.com/EmbeddedSetup?flow=1#step1", "https://accounts.google.com/EmbeddedSetup"},
		{"http://localhost:8080/auth?token=sensitive_val", "http://localhost:8080/auth"},
	}

	for _, tt := range tests {
		got := stripQueryAndFragment(tt.input)
		if got != tt.expected {
			t.Errorf("stripQueryAndFragment(%q) = %q; expected %q", tt.input, got, tt.expected)
		}
	}
}

func TestChromiumCookieSource_DebugLogging(t *testing.T) {
	const secretToken = "SECRET_TOKEN_DO_NOT_LEAK"
	const secretQuery = "secret_auth_code_12345"

	// Swap global logger buffer for test isolation
	originalLogger := GetGlobalLogger()
	testBuffer := NewLogBuffer(100)
	SetGlobalLogger(testBuffer)
	defer SetGlobalLogger(originalLogger)

	cookies := []*network.Cookie{
		{
			Name:     "NID",
			Value:    "some_nid_val",
			Domain:   ".google.com",
			HTTPOnly: false,
		},
		{
			Name:     "__Host-GAPS",
			Value:    "gaps_secret_value",
			Domain:   "accounts.google.com",
			HTTPOnly: true,
		},
		{
			Name:     "oauth_token",
			Value:    secretToken,
			Domain:   "accounts.google.com",
			HTTPOnly: true,
		},
	}

	rawURL := "https://accounts.google.com/EmbeddedSetup?code=" + secretQuery + "#frag1"

	snapshotFetcher := func(_ context.Context) ([]*network.Cookie, string, error) {
		return cookies, rawURL, nil
	}

	source := NewChromiumCookieSourceWithSnapshot(snapshotFetcher, 10*time.Millisecond)

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	token, err := source.WaitForCookie(ctx, "oauth_token")
	if err != nil {
		t.Fatalf("WaitForCookie failed: %v", err)
	}
	if len(token) != len(secretToken) {
		t.Fatalf("expected token of length %d, got length %d", len(secretToken), len(token))
	}

	entries := testBuffer.GetEntries(10, "")
	if len(entries) == 0 {
		t.Fatal("expected at least one log entry to be emitted")
	}

	// Verify the log entry fields and that secrets are NEVER present
	foundDebugLog := false
	for _, entry := range entries {
		if entry.Level != LogLevelDebug || entry.Category != LogCategoryAuth {
			continue
		}
		foundDebugLog = true
		msg := entry.UserMessage

		// Must include count
		if !strings.Contains(msg, "count=3") {
			t.Errorf("expected msg to contain 'count=3', got: %s", msg)
		}
		// Must include cookie names with domain
		if !strings.Contains(msg, "NID (.google.com)") || !strings.Contains(msg, "__Host-GAPS (accounts.google.com)") {
			t.Errorf("expected msg to contain cookie names with domain, got: %s", msg)
		}
		// Must list httpOnly cookies
		if !strings.Contains(msg, "httpOnly=[__Host-GAPS oauth_token]") {
			t.Errorf("expected msg to contain httpOnly list, got: %s", msg)
		}
		// Must indicate oauth_token was found
		if !strings.Contains(msg, "oauth_token_found=true") {
			t.Errorf("expected msg to contain 'oauth_token_found=true', got: %s", msg)
		}
		// Must contain sanitized URL without query or fragment
		if !strings.Contains(msg, "url=https://accounts.google.com/EmbeddedSetup") {
			t.Errorf("expected msg to contain clean URL, got: %s", msg)
		}

		// STRICT: NEVER contain secret values or query strings
		if strings.Contains(msg, secretToken) {
			t.Errorf("CRITICAL SECURITY ERROR: log contains secret token value!")
		}
		if strings.Contains(msg, secretQuery) || strings.Contains(msg, "code=") || strings.Contains(msg, "#frag1") {
			t.Errorf("CRITICAL SECURITY ERROR: log contains URL query or fragment: %s", msg)
		}
		if strings.Contains(msg, "some_nid_val") || strings.Contains(msg, "gaps_secret_value") {
			t.Errorf("CRITICAL SECURITY ERROR: log contains cookie values: %s", msg)
		}
	}

	if !foundDebugLog {
		t.Fatal("expected to find a DEBUG Auth log entry")
	}
}

func TestChromiumCookieSource_RateLimiting(t *testing.T) {
	originalLogger := GetGlobalLogger()
	testBuffer := NewLogBuffer(100)
	SetGlobalLogger(testBuffer)
	defer SetGlobalLogger(originalLogger)

	cookies := []*network.Cookie{
		{
			Name:     "test_cookie",
			Value:    "val",
			Domain:   "example.com",
			HTTPOnly: false,
		},
	}

	snapshotFetcher := func(_ context.Context) ([]*network.Cookie, string, error) {
		return cookies, "https://example.com/login", nil
	}

	source := NewChromiumCookieSourceWithSnapshot(snapshotFetcher, 10*time.Millisecond)

	ctx := context.Background()

	// Call check 10 times consecutively in rapid succession (within milliseconds)
	for i := 0; i < 10; i++ {
		source.check(ctx, "oauth_token")
	}

	entries := testBuffer.GetEntries(50, LogLevelDebug)
	// Because of the 5-second rate limit, only 1 log entry should have been emitted
	if len(entries) != 1 {
		t.Fatalf("expected exactly 1 rate-limited debug log entry for 10 rapid polls, got %d", len(entries))
	}
}

func TestChromiumCookieSource_URLFallback(t *testing.T) {
	const secretToken = "url_captured_oauth_token_val_123"

	// Fetcher returns empty cookies but the completion redirect URL contains the token
	snapshotFetcher := func(_ context.Context) ([]*network.Cookie, string, error) {
		return []*network.Cookie{}, "https://accounts.google.com/o/oauth2/programmatic_auth?oauth_token=" + secretToken, nil
	}

	source := NewChromiumCookieSourceWithSnapshot(snapshotFetcher, 10*time.Millisecond)

	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
	defer cancel()

	tok, err := source.WaitForCookie(ctx, "oauth_token")
	if err != nil {
		t.Fatalf("expected token from URL fallback, got error: %v", err)
	}
	if tok != secretToken {
		t.Fatalf("expected token %q, got %q", secretToken, tok)
	}
}

func TestSelectSignInTarget(t *testing.T) {
	tests := []struct {
		name       string
		targets    []*target.Info
		expectedID target.ID
		expectedOK bool
	}{
		{
			name:       "empty targets",
			targets:    []*target.Info{},
			expectedID: "",
			expectedOK: false,
		},
		{
			name: "only about:blank present",
			targets: []*target.Info{
				{TargetID: "t-blank", Type: "page", URL: "about:blank"},
			},
			expectedID: "",
			expectedOK: false,
		},
		{
			name: "other domains and browser targets present",
			targets: []*target.Info{
				{TargetID: "t-browser", Type: "browser", URL: ""},
				{TargetID: "t-newtab", Type: "page", URL: "chrome://newtab"},
				{TargetID: "t-other", Type: "page", URL: "https://example.com"},
			},
			expectedID: "",
			expectedOK: false,
		},
		{
			name: "EmbeddedSetup page target present",
			targets: []*target.Info{
				{TargetID: "t-blank", Type: "page", URL: "about:blank"},
				{TargetID: "t-signin", Type: "page", URL: "https://accounts.google.com/EmbeddedSetup?flow=1"},
			},
			expectedID: "t-signin",
			expectedOK: true,
		},
		{
			name: "multiple accounts.google.com targets picks first",
			targets: []*target.Info{
				{TargetID: "t-first", Type: "page", URL: "https://accounts.google.com/EmbeddedSetup"},
				{TargetID: "t-second", Type: "page", URL: "https://accounts.google.com/CheckCookie"},
			},
			expectedID: "t-first",
			expectedOK: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			id, ok := SelectSignInTarget(tt.targets)
			if ok != tt.expectedOK || id != tt.expectedID {
				t.Errorf("SelectSignInTarget() = (%q, %v); expected (%q, %v)", id, ok, tt.expectedID, tt.expectedOK)
			}
		})
	}
}

func TestWaitForSignInTarget(t *testing.T) {
	t.Run("no page yet then appears within timeout", func(t *testing.T) {
		callCount := 0
		fetcher := func(_ context.Context) ([]*target.Info, error) {
			callCount++
			if callCount < 3 {
				// Initially only about:blank is present
				return []*target.Info{
					{TargetID: "blank", Type: "page", URL: "about:blank"},
				}, nil
			}
			// Appears on 3rd call
			return []*target.Info{
				{TargetID: "blank", Type: "page", URL: "about:blank"},
				{TargetID: "google-signin", Type: "page", URL: "https://accounts.google.com/EmbeddedSetup"},
			}, nil
		}

		ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
		defer cancel()

		id, err := WaitForSignInTarget(ctx, fetcher, 10*time.Millisecond)
		if err != nil {
			t.Fatalf("WaitForSignInTarget failed: %v", err)
		}
		if id != "google-signin" {
			t.Fatalf("expected target id 'google-signin', got %q", id)
		}
	})

	t.Run("timeout when only about:blank present", func(t *testing.T) {
		fetcher := func(_ context.Context) ([]*target.Info, error) {
			return []*target.Info{
				{TargetID: "blank", Type: "page", URL: "about:blank"},
			}, nil
		}

		ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
		defer cancel()

		_, err := WaitForSignInTarget(ctx, fetcher, 10*time.Millisecond)
		if !errors.Is(err, context.DeadlineExceeded) {
			t.Fatalf("expected context.DeadlineExceeded, got: %v", err)
		}
	})
}

func TestChromiumCookieSource_TabClosedMidWait(t *testing.T) {
	// Simulate: tab was closed mid-wait, but browser process is still running.
	// Cookie lookup at browser level must keep waiting and succeed once oauth_token appears.
	callCount := 0
	procDone := make(chan struct{}) // process stays alive

	fakeToken := "TOKEN_LEN_16_ABC"
	fetcher := func(_ context.Context) ([]*network.Cookie, string, error) {
		callCount++
		if callCount == 1 {
			// Tab is present initially, no token
			return []*network.Cookie{}, "https://accounts.google.com/EmbeddedSetup", nil
		}
		if callCount == 2 {
			// Tab was closed mid-wait! URL is empty, no token yet
			return []*network.Cookie{}, "", nil
		}
		// Token appears at browser level on 3rd poll
		return []*network.Cookie{
			{Name: "oauth_token", Value: fakeToken, Domain: "accounts.google.com", HTTPOnly: true},
		}, "", nil
	}

	src := NewChromiumCookieSourceWithSnapshot(fetcher, 10*time.Millisecond)
	src.SetProcessExitChan(procDone)

	ctx, cancel := context.WithTimeout(context.Background(), 1*time.Second)
	defer cancel()

	token, err := src.WaitForCookie(ctx, "oauth_token")
	if err != nil {
		t.Fatalf("expected cookie wait to succeed even after tab closed mid-wait, got: %v", err)
	}
	if len(token) != len(fakeToken) {
		t.Fatalf("expected token length %d, got %d", len(fakeToken), len(token))
	}
}

func TestChromiumCookieSource_BrowserProcessExited(t *testing.T) {
	// Simulate: user closed the browser process before sign-in finished.
	// Must return "Browser was closed before sign-in finished" and NOT hang.
	procDone := make(chan struct{})

	fetcher := func(_ context.Context) ([]*network.Cookie, string, error) {
		return []*network.Cookie{}, "https://accounts.google.com/EmbeddedSetup", nil
	}

	src := NewChromiumCookieSourceWithSnapshot(fetcher, 10*time.Millisecond)
	src.SetProcessExitChan(procDone)

	// Trigger browser process exit after 20ms
	go func() {
		time.Sleep(20 * time.Millisecond)
		close(procDone)
	}()

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	_, err := src.WaitForCookie(ctx, "oauth_token")
	if err == nil {
		t.Fatal("expected error when browser process exited, got nil")
	}
	if !strings.Contains(err.Error(), "Browser was closed before sign-in finished") {
		t.Fatalf("expected 'Browser was closed before sign-in finished' error, got: %v", err)
	}
}

func TestChromiumCookieSource_TabClosed(t *testing.T) {
	// Simulate: user closed the sign-in tab while the browser itself is still running.
	// Must return "Browser tab was closed before sign-in finished" immediately and NOT hang.
	fetcher := func(_ context.Context) ([]*network.Cookie, string, error) {
		return nil, "", errors.New("Browser tab was closed before sign-in finished")
	}

	src := NewChromiumCookieSourceWithSnapshot(fetcher, 10*time.Millisecond)

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()

	_, err := src.WaitForCookie(ctx, "oauth_token")
	if err == nil {
		t.Fatal("expected error when tab was closed, got nil")
	}
	if !strings.Contains(err.Error(), "closed") {
		t.Fatalf("expected closed error, got: %v", err)
	}
}

type fakeShutdownProcess struct {
	mu      sync.Mutex
	order   []string
	done    chan struct{}
	killed  bool
	tempDir string
}

func (p *fakeShutdownProcess) record(action string) {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.order = append(p.order, action)
	if action == "kill" && p.done != nil {
		select {
		case <-p.done:
		default:
			close(p.done)
		}
	}
}

func (p *fakeShutdownProcess) Kill() error {
	p.record("kill")
	p.killed = true
	return nil
}

func (p *fakeShutdownProcess) Wait() error {
	if p.done != nil {
		<-p.done
	}
	p.record("wait_done")
	return nil
}

func (p *fakeShutdownProcess) Pid() int {
	return 9999
}

func TestCloseBrowserGracefully_AllFourExitPaths(t *testing.T) {
	paths := []struct {
		name          string
		cdpClosesProc bool
	}{
		{name: "success_path", cdpClosesProc: true},
		{name: "cancel_path", cdpClosesProc: false},
		{name: "timeout_path", cdpClosesProc: false},
		{name: "error_path", cdpClosesProc: false},
	}

	for _, tc := range paths {
		t.Run(tc.name, func(t *testing.T) {
			tempDir, err := os.MkdirTemp("", "lumen-shutdown-test-*")
			if err != nil {
				t.Fatalf("failed to create temp dir: %v", err)
			}
			marker := filepath.Join(tempDir, "marker.txt")
			_ = os.WriteFile(marker, []byte("ok"), 0600)

			proc := &fakeShutdownProcess{
				done:    make(chan struct{}),
				tempDir: tempDir,
			}

			procDone := make(chan struct{})
			go func() {
				_ = proc.Wait()
				close(procDone)
			}()

			cdpCloser := func(_ context.Context) error {
				proc.record("cdp_close")
				if tc.cdpClosesProc {
					// Simulate browser process exiting upon receiving Browser.close
					select {
					case <-proc.done:
					default:
						close(proc.done)
					}
				}
				return nil
			}

			// Run graceful shutdown with 20ms wait timeout for fast unit tests
			closeErr := CloseBrowserGracefullyWithOptions(cdpCloser, proc, procDone, tempDir, 20*time.Millisecond)
			if closeErr != nil {
				t.Fatalf("expected clean shutdown, got error: %v", closeErr)
			}

			proc.mu.Lock()
			order := append([]string(nil), proc.order...)
			proc.mu.Unlock()

			// 1. Verify cdp_close was requested first
			if len(order) == 0 || order[0] != "cdp_close" {
				t.Errorf("expected 'cdp_close' to be first action, got: %v", order)
			}

			// 2. If cdp close didn't exit the process, kill was called
			if !tc.cdpClosesProc {
				foundKill := false
				for _, a := range order {
					if a == "kill" {
						foundKill = true
						break
					}
				}
				if !foundKill {
					t.Errorf("expected 'kill' to be called when process didn't exit, order: %v", order)
				}
			}

			// 3. Verify temp profile directory was deleted AFTER the process exited
			if _, statErr := os.Stat(tempDir); !os.IsNotExist(statErr) {
				t.Errorf("expected temp dir %s to be deleted after process exit, statErr: %v", tempDir, statErr)
			}
		})
	}
}
