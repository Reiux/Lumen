package core

import (
	"context"
	"errors"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/chromedp/cdproto/network"
	"github.com/chromedp/cdproto/storage"
	"github.com/chromedp/cdproto/target"
	"github.com/chromedp/chromedp"
	"github.com/chromedp/chromedp/remote"
)

const (
	// AndroidPixel8UserAgent is the Pixel 8 User Agent required by EmbeddedSetup.
	AndroidPixel8UserAgent = "Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.6613.127 Mobile Safari/537.36"

	// EmbeddedSetupEndpoint is the Google EmbeddedSetup URL for device sign-in.
	EmbeddedSetupEndpoint = "https://accounts.google.com/EmbeddedSetup"

	// DefaultSignInTimeout is the maximum duration to wait for user sign-in.
	DefaultSignInTimeout = 5 * time.Minute

	// DefaultPollInterval is how often to poll the browser cookie jar.
	DefaultPollInterval = 500 * time.Millisecond
)

// CookieSource represents an entity that can wait for and retrieve a named cookie.
type CookieSource interface {
	WaitForCookie(ctx context.Context, name string) (string, error)
}

// CookieSourceFunc is an adapter to allow the use of ordinary functions as CookieSource.
type CookieSourceFunc func(ctx context.Context, name string) (string, error)

func (f CookieSourceFunc) WaitForCookie(ctx context.Context, name string) (string, error) {
	return f(ctx, name)
}

// stripQueryAndFragment removes query parameters and URL fragments to prevent leaking tokens or state in logs.
func stripQueryAndFragment(rawURL string) string {
	if rawURL == "" {
		return ""
	}
	u, err := url.Parse(rawURL)
	if err != nil {
		if idx := strings.IndexAny(rawURL, "?#"); idx != -1 {
			return rawURL[:idx]
		}
		return rawURL
	}
	u.RawQuery = ""
	u.Fragment = ""
	return u.String()
}

// SelectSignInTarget finds the first page target opened with accounts.google.com.
func SelectSignInTarget(targets []*target.Info) (target.ID, bool) {
	for _, t := range targets {
		if t == nil || t.Type != "page" {
			continue
		}
		u, err := url.Parse(t.URL)
		if err != nil {
			continue
		}
		if strings.EqualFold(u.Hostname(), "accounts.google.com") {
			return t.TargetID, true
		}
	}
	return "", false
}

// WaitForSignInTarget polls targetFetcher until SelectSignInTarget finds a matching page target or ctx expires.
func WaitForSignInTarget(ctx context.Context, targetFetcher func(context.Context) ([]*target.Info, error), pollInterval time.Duration) (target.ID, error) {
	if pollInterval <= 0 {
		pollInterval = 100 * time.Millisecond
	}
	ticker := time.NewTicker(pollInterval)
	defer ticker.Stop()

	if targets, err := targetFetcher(ctx); err == nil {
		if id, ok := SelectSignInTarget(targets); ok {
			return id, nil
		}
	}

	for {
		select {
		case <-ctx.Done():
			return "", ctx.Err()
		case <-ticker.C:
			targets, err := targetFetcher(ctx)
			if err != nil {
				continue
			}
			if id, ok := SelectSignInTarget(targets); ok {
				return id, nil
			}
		}
	}
}

// ChromiumCookieSource polls CDP network cookies until the target cookie is found.
type ChromiumCookieSource struct {
	fetcher         func(ctx context.Context) ([]*network.Cookie, error)
	snapshotFetcher func(ctx context.Context) ([]*network.Cookie, string, error)
	urlFetcher      func(ctx context.Context) string
	pollInterval    time.Duration
	procDone        <-chan struct{}
	mu              sync.Mutex
	lastLogTime     time.Time
}

// NewChromiumCookieSource creates a CookieSource for Chromium-based browsers via CDP.
func NewChromiumCookieSource(fetcher func(ctx context.Context) ([]*network.Cookie, error), pollInterval time.Duration) *ChromiumCookieSource {
	if pollInterval <= 0 {
		pollInterval = DefaultPollInterval
	}
	return &ChromiumCookieSource{
		fetcher:      fetcher,
		pollInterval: pollInterval,
	}
}

// NewChromiumCookieSourceWithSnapshot creates a CookieSource that also fetches the page URL.
func NewChromiumCookieSourceWithSnapshot(snapshotFetcher func(ctx context.Context) ([]*network.Cookie, string, error), pollInterval time.Duration) *ChromiumCookieSource {
	if pollInterval <= 0 {
		pollInterval = DefaultPollInterval
	}
	return &ChromiumCookieSource{
		snapshotFetcher: snapshotFetcher,
		pollInterval:    pollInterval,
	}
}

// SetURLFetcher sets an optional callback to retrieve the current page URL when using a standard fetcher.
func (s *ChromiumCookieSource) SetURLFetcher(f func(ctx context.Context) string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.urlFetcher = f
}

// SetProcessExitChan wires a notification channel that closes when the browser process exits.
func (s *ChromiumCookieSource) SetProcessExitChan(done <-chan struct{}) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.procDone = done
}

func (s *ChromiumCookieSource) WaitForCookie(ctx context.Context, name string) (string, error) {
	ticker := time.NewTicker(s.pollInterval)
	defer ticker.Stop()

	s.mu.Lock()
	procDone := s.procDone
	s.mu.Unlock()

	select {
	case <-procDone:
		return "", errors.New("Browser was closed before sign-in finished")
	default:
	}

	if tok, found, err := s.check(ctx, name); err != nil {
		return "", err
	} else if found {
		return tok, nil
	}

	for {
		select {
		case <-ctx.Done():
			return "", ctx.Err()
		case <-procDone:
			return "", errors.New("Browser was closed before sign-in finished")
		case <-ticker.C:
			select {
			case <-procDone:
				return "", errors.New("Browser was closed before sign-in finished")
			default:
			}
			tok, found, err := s.check(ctx, name)
			if err != nil {
				return "", err
			}
			if found {
				return tok, nil
			}
		}
	}
}

func (s *ChromiumCookieSource) check(ctx context.Context, name string) (string, bool, error) {
	if s.snapshotFetcher == nil && s.fetcher == nil {
		return "", false, nil
	}

	var cookies []*network.Cookie
	var pageURL string
	var err error

	if s.snapshotFetcher != nil {
		cookies, pageURL, err = s.snapshotFetcher(ctx)
	} else if s.fetcher != nil {
		cookies, err = s.fetcher(ctx)
		s.mu.Lock()
		if s.urlFetcher != nil {
			pageURL = s.urlFetcher(ctx)
		}
		s.mu.Unlock()
	}

	if err != nil {
		errStr := strings.ToLower(err.Error())
		if strings.Contains(errStr, "closed") || strings.Contains(errStr, "eof") || strings.Contains(errStr, "broken pipe") || errors.Is(err, context.Canceled) {
			return "", false, err
		}
		return "", false, nil
	}

	var targetValue string
	found := false
	hasOAuthToken := false
	namesWithDomain := make([]string, 0, len(cookies))
	var httpOnlyList []string

	for _, c := range cookies {
		if c == nil {
			continue
		}
		namesWithDomain = append(namesWithDomain, fmt.Sprintf("%s (%s)", c.Name, c.Domain))
		if c.HTTPOnly {
			httpOnlyList = append(httpOnlyList, c.Name)
		}
		isTargetCookie := strings.EqualFold(c.Name, name) || (strings.EqualFold(name, "oauth_token") && strings.EqualFold(c.Name, "Token"))
		if (strings.EqualFold(c.Name, "oauth_token") || strings.EqualFold(c.Name, "Token")) && c.Value != "" {
			hasOAuthToken = true
		}
		if !found && isTargetCookie && c.Value != "" {
			targetValue = c.Value
			found = true
		}
	}

	// Also check page URL for oauth_token or Token parameter/fragment
	if !found && pageURL != "" {
		if tok, ok := ExtractOAuthToken(pageURL); ok && tok != "" {
			targetValue = tok
			found = true
			hasOAuthToken = true
		}
	}

	// Rate-limited debug log: at most once every 5 seconds (or immediately when target is found)
	now := time.Now()
	s.mu.Lock()
	shouldLog := s.lastLogTime.IsZero() || now.Sub(s.lastLogTime) >= 5*time.Second || found
	if shouldLog {
		s.lastLogTime = now
	}
	s.mu.Unlock()

	if shouldLog {
		cleanURL := stripQueryAndFragment(pageURL)
		GetGlobalLogger().AddEntry(LogEntry{
			Timestamp:   now,
			Level:       LogLevelDebug,
			Category:    LogCategoryAuth,
			UserMessage: fmt.Sprintf("Cookie poll: count=%d, cookies=%v, httpOnly=%v, oauth_token_found=%t, url=%s",
				len(cookies),
				namesWithDomain,
				httpOnlyList,
				hasOAuthToken,
				cleanURL,
			),
		})
	}

	if found {
		return targetValue, true, nil
	}
	return "", false, nil
}

// BrowserProcess abstracts the running browser process for lifecycle control.
type BrowserProcess interface {
	Kill() error
	Wait() error
	Pid() int
}

type osBrowserProcess struct {
	cmd *exec.Cmd
}

func (p *osBrowserProcess) Pid() int {
	if p.cmd != nil && p.cmd.Process != nil {
		return p.cmd.Process.Pid
	}
	return 0
}

func (p *osBrowserProcess) Kill() error {
	if p.cmd != nil && p.cmd.Process != nil {
		_ = killProcessGroup(p.cmd.Process.Pid)
		return p.cmd.Process.Kill()
	}
	return nil
}

func (p *osBrowserProcess) Wait() error {
	if p.cmd != nil {
		return p.cmd.Wait()
	}
	return nil
}

// ProcessRunner starts a browser process.
type ProcessRunner func(ctx context.Context, name string, args ...string) (BrowserProcess, error)

// BrowserSignInOptions configures an automated sign-in session.
type BrowserSignInOptions struct {
	Browser       SupportedBrowser
	BrowserPath   string
	Timeout       time.Duration
	PollInterval  time.Duration
	CookieSource  CookieSource
	ProcessRunner ProcessRunner
	TempDirFunc   func() (string, error)
	OnTempDir     func(dir string) // Test hook to capture temp directory path
	OnCloseFailed func()           // Called if the browser window could not be closed automatically
}

// CloseBrowserGracefully shuts down the browser:
// 1. Sends Browser.close over CDP (with a 3s timeout) if cdpClose is provided.
// 2. Waits up to 3s for the browser process to exit.
// 3. If still running after 3s, kills the process and its process group.
// 4. Deletes tempDir strictly after process has exited.
func CloseBrowserGracefully(
	cdpClose func(ctx context.Context) error,
	proc BrowserProcess,
	procDone <-chan struct{},
	tempDir string,
) error {
	return CloseBrowserGracefullyWithOptions(cdpClose, proc, procDone, tempDir, 3*time.Second)
}

// CloseBrowserGracefullyWithOptions executes the graceful browser shutdown sequence with a configurable wait timeout.
func CloseBrowserGracefullyWithOptions(
	cdpClose func(ctx context.Context) error,
	proc BrowserProcess,
	procDone <-chan struct{},
	tempDir string,
	waitTimeout time.Duration,
) error {
	if waitTimeout <= 0 {
		waitTimeout = 3 * time.Second
	}
	var closeErr error

	// 1. First send Browser.close over CDP (with a 3s timeout)
	processExited := false
	if cdpClose != nil {
		closeCtx, cancelClose := context.WithTimeout(context.Background(), waitTimeout)
		_ = cdpClose(closeCtx)
		cancelClose()

		// 2. Wait up to waitTimeout for the process to exit following CDP close
		if procDone != nil {
			select {
			case <-procDone:
				processExited = true
			case <-time.After(waitTimeout):
			}
		}
	} else if procDone != nil {
		// If proc already exited on its own, check non-blocking
		select {
		case <-procDone:
			processExited = true
		default:
		}
	}

	// 3. If process has not exited after waitTimeout, kill it and its process group
	if !processExited && proc != nil {
		if err := proc.Kill(); err != nil {
			closeErr = fmt.Errorf("failed to kill browser process: %w", err)
		}
		if procDone != nil {
			select {
			case <-procDone:
				processExited = true
			case <-time.After(waitTimeout):
				if closeErr == nil {
					closeErr = errors.New("browser process did not exit within timeout after kill")
				}
			}
		}
	}

	// 4. Delete temp profile dir strictly AFTER the process has exited
	if tempDir != "" {
		for i := 0; i < 20; i++ {
			if rmErr := os.RemoveAll(tempDir); rmErr == nil || os.IsNotExist(rmErr) {
				break
			}
			time.Sleep(100 * time.Millisecond)
		}
	}

	return closeErr
}

// RunBrowserSignIn discovers the browser and performs the automated sign-in flow.
func RunBrowserSignIn(ctx context.Context, browserPath string) (string, error) {
	return RunBrowserSignInWithOptions(ctx, BrowserSignInOptions{
		BrowserPath: browserPath,
	})
}

// RunBrowserSignInWithOptions executes the automated sign-in session with options.
func RunBrowserSignInWithOptions(ctx context.Context, opts BrowserSignInOptions) (string, error) {
	if opts.BrowserPath == "" && opts.ProcessRunner == nil {
		return "", errors.New("no browser executable specified")
	}

	timeout := opts.Timeout
	if timeout <= 0 {
		timeout = DefaultSignInTimeout
	}

	pollInterval := opts.PollInterval
	if pollInterval <= 0 {
		pollInterval = DefaultPollInterval
	}

	timeoutCtx, cancelTimeout := context.WithTimeout(ctx, timeout)
	defer cancelTimeout()

	// 1. Create fresh isolated temporary profile directory
	var tempDir string
	var err error
	if opts.TempDirFunc != nil {
		tempDir, err = opts.TempDirFunc()
	} else {
		tempDir, err = os.MkdirTemp("", "lumen-browser-auth-*")
	}
	if err != nil {
		return "", fmt.Errorf("failed to create temporary browser profile: %w", err)
	}

	if opts.OnTempDir != nil {
		opts.OnTempDir(tempDir)
	}

	// 2. Obtain a free port on 127.0.0.1 for the DevTools protocol
	port, err := getFreePort()
	if err != nil {
		_ = os.RemoveAll(tempDir)
		return "", fmt.Errorf("failed to reserve port for browser debugging: %w", err)
	}

	// 3. Prepare browser arguments
	args := []string{
		"--user-data-dir=" + tempDir,
		"--remote-debugging-port=" + strconv.Itoa(port),
		"--remote-debugging-address=127.0.0.1",
		"--no-first-run",
		"--no-default-browser-check",
		"--user-agent=" + AndroidPixel8UserAgent,
		EmbeddedSetupEndpoint,
	}

	// 4. Launch browser process
	runner := opts.ProcessRunner
	if runner == nil {
		runner = func(runCtx context.Context, name string, cmdArgs ...string) (BrowserProcess, error) {
			cmd := exec.CommandContext(runCtx, name, cmdArgs...)
			prepareCmdSysProcAttr(cmd)
			if startErr := cmd.Start(); startErr != nil {
				return nil, startErr
			}
			return &osBrowserProcess{cmd: cmd}, nil
		}
	}

	proc, err := runner(timeoutCtx, opts.BrowserPath, args...)
	if err != nil {
		_ = os.RemoveAll(tempDir)
		return "", fmt.Errorf("failed to start browser process: %w", err)
	}

	procDone := make(chan struct{})
	go func() {
		_ = proc.Wait()
		close(procDone)
	}()

	var cdpCloser func(context.Context) error
	defer func() {
		closeErr := CloseBrowserGracefully(cdpCloser, proc, procDone, tempDir)
		if closeErr != nil && opts.OnCloseFailed != nil {
			opts.OnCloseFailed()
		}
	}()

	// 5. Establish cookie source
	cookieSource := opts.CookieSource
	if cookieSource == nil {
		cdpFetcher, closer, cdpCancel, cdpErr := initCDPCookieSource(timeoutCtx, port)
		if cdpErr != nil {
			return "", cdpErr
		}
		defer cdpCancel()
		cdpCloser = closer
		src := NewChromiumCookieSourceWithSnapshot(cdpFetcher, pollInterval)
		src.SetProcessExitChan(procDone)
		cookieSource = src
	} else if chromSrc, ok := cookieSource.(*ChromiumCookieSource); ok {
		chromSrc.SetProcessExitChan(procDone)
	}

	// 6. Wait for oauth_token cookie
	token, err := cookieSource.WaitForCookie(timeoutCtx, "oauth_token")
	if err != nil {
		if errors.Is(err, context.Canceled) {
			return "", context.Canceled
		}
		if errors.Is(err, context.DeadlineExceeded) {
			return "", errors.New("sign-in timed out. Please try again or use manual token entry")
		}
		return "", err
	}

	return token, nil
}

// initCDPCookieSource waits for the browser's DevTools endpoint to accept connections
// and configures a chromedp remote session attached to the EXISTING Google sign-in page target.
func initCDPCookieSource(ctx context.Context, port int) (func(ctx context.Context) ([]*network.Cookie, string, error), func(ctx context.Context) error, context.CancelFunc, error) {
	baseURL := fmt.Sprintf("http://127.0.0.1:%d", port)

	// Wait up to 10 seconds for the browser DevTools HTTP server to respond
	ready := false
	deadline := time.Now().Add(10 * time.Second)
	for time.Now().Before(deadline) {
		select {
		case <-ctx.Done():
			return nil, nil, nil, ctx.Err()
		default:
		}

		req, err := http.NewRequestWithContext(ctx, http.MethodGet, baseURL+"/json/version", nil)
		if err == nil {
			client := &http.Client{Timeout: 500 * time.Millisecond}
			resp, reqErr := client.Do(req)
			if reqErr == nil {
				_ = resp.Body.Close()
				if resp.StatusCode == http.StatusOK {
					ready = true
					break
				}
			}
		}
		time.Sleep(100 * time.Millisecond)
	}

	if !ready {
		return nil, nil, nil, errors.New("timed out waiting for browser DevTools interface to start")
	}

	allocCtx, cancelAlloc := remote.NewAllocator(ctx, baseURL)

	// Create root browser context with chromedp to manage browser session and query targets
	browserCtx, cancelBrowser := chromedp.NewContext(allocCtx)

	// Stop creating a new target: locate existing page target opened with EmbeddedSetup
	targetID, err := WaitForSignInTarget(ctx, func(tctx context.Context) ([]*target.Info, error) {
		return chromedp.Targets(browserCtx)
	}, 100*time.Millisecond)
	if err != nil {
		cancelBrowser()
		cancelAlloc()
		return nil, nil, nil, fmt.Errorf("failed to locate Google sign-in page target: %w", err)
	}

	// Attach to the existing target by ID - no new window (about:blank) is opened!
	targetCtx, cancelTarget := chromedp.NewContext(browserCtx, chromedp.WithTargetID(targetID))

	var targetMu sync.Mutex

	cleanup := func() {
		targetMu.Lock()
		if cancelTarget != nil {
			cancelTarget()
		}
		targetMu.Unlock()
		cancelBrowser()
		cancelAlloc()
	}

	closer := func(closeCtx context.Context) error {
		// Close all open page targets so the browser window disappears immediately
		targets, _ := chromedp.Targets(browserCtx)
		for _, t := range targets {
			if t != nil && t.Type == "page" {
				_, _ = chromedp.CallBrowser(browserCtx, target.CloseTarget, target.CloseTargetParams{
					TargetID: t.TargetID,
				})
			}
		}
		_ = chromedp.Cancel(browserCtx)
		return nil
	}

	// Poll cookies and monitor tab/browser lifecycle
	source := func(pollCtx context.Context) ([]*network.Cookie, string, error) {
		// 1. Check whether the sign-in tab or browser has been closed
		targets, tErr := chromedp.Targets(browserCtx)
		if tErr != nil {
			return nil, "", fmt.Errorf("Browser was closed before sign-in finished: %w", tErr)
		}
		targetFound := false
		hasPage := false
		var alternateTargetID target.ID
		for _, t := range targets {
			if t != nil && t.Type == "page" {
				hasPage = true
				if t.TargetID == targetID {
					targetFound = true
				} else if strings.Contains(t.URL, "accounts.google.com") || strings.Contains(t.URL, "google.com") {
					alternateTargetID = t.TargetID
				}
			}
		}
		if !hasPage {
			return nil, "", errors.New("Browser was closed before sign-in finished")
		}

		targetMu.Lock()
		if !targetFound {
			if alternateTargetID != "" {
				// Target swapped during navigation (e.g. process swap or redirect) - reattach
				if cancelTarget != nil {
					cancelTarget()
				}
				targetID = alternateTargetID
				targetCtx, cancelTarget = chromedp.NewContext(browserCtx, chromedp.WithTargetID(targetID))
			} else {
				targetMu.Unlock()
				return nil, "", errors.New("Browser tab was closed before sign-in finished")
			}
		}
		activeTargetCtx := targetCtx
		targetMu.Unlock()

		var cookies []*network.Cookie
		var pageURL string

		// 1. Fetch cookies from page target storage
		sRes, sErr := chromedp.Call(activeTargetCtx, storage.GetCookies, storage.GetCookiesParams{})
		if sErr == nil {
			cookies = sRes.Cookies
		}

		// 2. Also fetch network cookies for Google sign-in endpoints to catch any partition/host-specific cookies
		nRes, nErr := chromedp.Call(activeTargetCtx, network.GetCookies, network.GetCookiesParams{
			URLs: []string{
				"https://accounts.google.com",
				"https://accounts.google.com/EmbeddedSetup",
				"https://google.com",
			},
		})
		if nErr == nil && len(nRes.Cookies) > 0 {
			seen := make(map[string]bool, len(cookies)+len(nRes.Cookies))
			for _, c := range cookies {
				seen[c.Name+"#"+c.Domain+"#"+c.Path] = true
			}
			for _, c := range nRes.Cookies {
				key := c.Name + "#" + c.Domain + "#" + c.Path
				if !seen[key] {
					seen[key] = true
					cookies = append(cookies, c)
				}
			}
		}

		// 3. Fallback to browser-level cookies if target storage/network returned empty or failed
		if sErr != nil && nErr != nil && len(cookies) == 0 {
			bRes, bErr := chromedp.CallBrowser(browserCtx, storage.GetCookies, storage.GetCookiesParams{})
			if bErr == nil {
				cookies = bRes.Cookies
			} else {
				return nil, "", sErr
			}
		}

		// 4. Check all open page targets for pageURL, especially if one has oauth_token in query/fragment
		for _, t := range targets {
			if t != nil && t.Type == "page" && t.URL != "" && t.URL != "about:blank" {
				if _, ok := ExtractOAuthToken(t.URL); ok {
					pageURL = t.URL
					break
				}
				if strings.Contains(t.URL, "accounts.google.com") && pageURL == "" {
					pageURL = t.URL
				}
			}
		}
		if pageURL == "" {
			for _, t := range targets {
				if t != nil && t.Type == "page" && t.URL != "" && t.URL != "about:blank" {
					pageURL = t.URL
					break
				}
			}
		}

		return cookies, pageURL, nil
	}

	return source, closer, cleanup, nil
}

func getFreePort() (int, error) {
	addr, err := net.ResolveTCPAddr("tcp", "127.0.0.1:0")
	if err != nil {
		return 0, err
	}
	l, err := net.ListenTCP("tcp", addr)
	if err != nil {
		return 0, err
	}
	defer l.Close()
	return l.Addr().(*net.TCPAddr).Port, nil
}
