package core

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"
)

// DefaultGitHubReleasesURL points to the latest Lumen release on GitHub.
// Requesting this without following redirects yields the latest tag in the Location header.
const DefaultGitHubReleasesURL = "https://github.com/Reiux/Lumen/releases/latest"

// DefaultGitHubAPIReleasesURL points to the GitHub REST API for release details.
const DefaultGitHubAPIReleasesURL = "https://api.github.com/repos/Reiux/Lumen/releases"

// UpdateCheckThrottleDuration is the minimum interval between automatic background update checks.
const UpdateCheckThrottleDuration = 12 * time.Hour

// UpdateInfo contains release details and whether an update is available.
type UpdateInfo struct {
	CurrentVersion string `json:"currentVersion"`
	LatestVersion  string `json:"latestVersion"`
	HasUpdate      bool   `json:"hasUpdate"`
	ReleaseURL     string `json:"releaseUrl"`
	ReleaseNotes   string `json:"releaseNotes"`
	PublishedAt    string `json:"publishedAt"`
	CheckedAt      string `json:"checkedAt"`
	Error          string `json:"error,omitempty"`
}

type githubRelease struct {
	TagName     string `json:"tag_name"`
	HTMLURL     string `json:"html_url"`
	Body        string `json:"body"`
	PublishedAt string `json:"published_at"`
}

type releaseNoteCacheEntry struct {
	ETag string
	Body string
}

var (
	releaseNotesCacheMu sync.RWMutex
	releaseNotesCache   = make(map[string]releaseNoteCacheEntry)
)

// ResetReleaseNotesCache clears cached release notes (useful in testing).
func ResetReleaseNotesCache() {
	releaseNotesCacheMu.Lock()
	defer releaseNotesCacheMu.Unlock()
	releaseNotesCache = make(map[string]releaseNoteCacheEntry)
}

// CheckForUpdatesWithURL performs the basic update check against the redirect URL
// without following the redirect, reading the tag from the Location header.
// This avoids unauthenticated GitHub REST API rate limits for the basic version check.
func CheckForUpdatesWithURL(ctx context.Context, redirectURL string, currentVersion string, manual bool, autoCheckEnabled bool) (UpdateInfo, error) {
	if !manual && !autoCheckEnabled {
		return UpdateInfo{
			CurrentVersion: currentVersion,
			HasUpdate:      false,
			CheckedAt:      time.Now().UTC().Format(time.RFC3339),
		}, nil
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, redirectURL, nil)
	if err != nil {
		return UpdateInfo{CurrentVersion: currentVersion, Error: err.Error(), CheckedAt: time.Now().UTC().Format(time.RFC3339)}, err
	}
	req.Header.Set("User-Agent", "Lumen-Desktop/"+currentVersion)
	req.Header.Set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")

	client := &http.Client{
		Timeout: 10 * time.Second,
		CheckRedirect: func(req *http.Request, via []*http.Request) error {
			// Do NOT follow redirects so we can parse the Location header
			return http.ErrUseLastResponse
		},
	}
	resp, err := client.Do(req)
	if err != nil {
		desc := "Unable to reach GitHub. Please check your internet connection."
		if ctx.Err() == context.DeadlineExceeded || strings.Contains(strings.ToLower(err.Error()), "timeout") {
			desc = "Update check timed out. Please check your internet connection."
		}
		return UpdateInfo{CurrentVersion: currentVersion, Error: desc, CheckedAt: time.Now().UTC().Format(time.RFC3339)}, nil
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusForbidden || resp.StatusCode == http.StatusTooManyRequests {
		desc := parseRateLimitMessage(resp)
		return UpdateInfo{CurrentVersion: currentVersion, Error: desc, CheckedAt: time.Now().UTC().Format(time.RFC3339)}, nil
	}
	if resp.StatusCode == http.StatusNotFound {
		desc := "No releases found on GitHub."
		return UpdateInfo{CurrentVersion: currentVersion, Error: desc, CheckedAt: time.Now().UTC().Format(time.RFC3339)}, nil
	}

	var tag string
	var releaseURL string

	// Handle redirect responses (301, 302, 303, 307, 308)
	if resp.StatusCode >= 300 && resp.StatusCode < 400 {
		loc := resp.Header.Get("Location")
		if loc == "" {
			return UpdateInfo{CurrentVersion: currentVersion, Error: "GitHub redirect missing Location header.", CheckedAt: time.Now().UTC().Format(time.RFC3339)}, nil
		}
		releaseURL = loc
		if !strings.HasPrefix(releaseURL, "http://") && !strings.HasPrefix(releaseURL, "https://") {
			releaseURL = "https://github.com" + loc
		}
		tag = extractTagFromLocation(loc)
		if tag == "" {
			return UpdateInfo{CurrentVersion: currentVersion, Error: "Could not parse release tag from GitHub redirect.", CheckedAt: time.Now().UTC().Format(time.RFC3339)}, nil
		}
	} else if resp.StatusCode == http.StatusOK {
		// Fallback for direct responses (e.g. test mocks returning JSON)
		var rel githubRelease
		if err := json.NewDecoder(resp.Body).Decode(&rel); err == nil && rel.TagName != "" {
			tag = rel.TagName
			releaseURL = rel.HTMLURL
		} else {
			return UpdateInfo{CurrentVersion: currentVersion, Error: "Unexpected response from GitHub release page.", CheckedAt: time.Now().UTC().Format(time.RFC3339)}, nil
		}
	} else {
		desc := fmt.Sprintf("GitHub returned status %d", resp.StatusCode)
		return UpdateInfo{CurrentVersion: currentVersion, Error: desc, CheckedAt: time.Now().UTC().Format(time.RFC3339)}, nil
	}

	hasUpdate := isVersionNewer(tag, currentVersion)
	return UpdateInfo{
		CurrentVersion: currentVersion,
		LatestVersion:  tag,
		HasUpdate:      hasUpdate,
		ReleaseURL:     releaseURL,
		CheckedAt:      time.Now().UTC().Format(time.RFC3339),
	}, nil
}

// extractTagFromLocation extracts the tag name following "/tag/" in a URL or path.
func extractTagFromLocation(loc string) string {
	idx := strings.Index(loc, "/tag/")
	if idx == -1 {
		return ""
	}
	tag := loc[idx+len("/tag/"):]
	if qIdx := strings.IndexAny(tag, "?#"); qIdx != -1 {
		tag = tag[:qIdx]
	}
	return strings.Trim(tag, "/")
}

// FetchReleaseNotesWithURL fetches the release notes for a specific tag using GitHub's REST API.
// It uses If-None-Match with ETag caching.
// If GitHub returns 403 or 429, it parses X-RateLimit-Reset or Retry-After and returns a friendly message.
func FetchReleaseNotesWithURL(ctx context.Context, apiBaseURL string, tag string, currentVersion string) (string, error) {
	tag = strings.TrimSpace(tag)
	if tag == "" {
		return "", errors.New("tag cannot be empty")
	}

	reqURL := apiBaseURL
	if strings.Contains(apiBaseURL, "%s") {
		reqURL = fmt.Sprintf(apiBaseURL, tag)
	} else if strings.HasSuffix(apiBaseURL, "/tags/") {
		reqURL = apiBaseURL + tag
	} else if strings.HasSuffix(apiBaseURL, "/tags") {
		reqURL = apiBaseURL + "/" + tag
	} else if !strings.HasSuffix(apiBaseURL, "/"+tag) {
		reqURL = strings.TrimRight(apiBaseURL, "/") + "/tags/" + tag
	}

	releaseNotesCacheMu.RLock()
	cached, hasCached := releaseNotesCache[tag]
	releaseNotesCacheMu.RUnlock()

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, reqURL, nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("User-Agent", "Lumen-Desktop/"+currentVersion)
	req.Header.Set("Accept", "application/vnd.github.v3+json")
	if hasCached && cached.ETag != "" {
		req.Header.Set("If-None-Match", cached.ETag)
	}

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotModified {
		if hasCached {
			return cached.Body, nil
		}
		return "", nil
	}

	if resp.StatusCode == http.StatusForbidden || resp.StatusCode == http.StatusTooManyRequests {
		return "", errors.New(parseRateLimitMessage(resp))
	}

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("GitHub API returned status %d", resp.StatusCode)
	}

	var rel githubRelease
	if err := json.NewDecoder(resp.Body).Decode(&rel); err != nil {
		return "", fmt.Errorf("failed to parse release notes: %w", err)
	}

	etag := resp.Header.Get("ETag")
	releaseNotesCacheMu.Lock()
	releaseNotesCache[tag] = releaseNoteCacheEntry{
		ETag: etag,
		Body: rel.Body,
	}
	releaseNotesCacheMu.Unlock()

	return rel.Body, nil
}

// parseRateLimitMessage constructs a friendly message from GitHub rate-limit headers.
func parseRateLimitMessage(resp *http.Response) string {
	// 1. Check X-RateLimit-Reset (Unix timestamp)
	if resetStr := resp.Header.Get("X-RateLimit-Reset"); resetStr != "" {
		if epoch, err := strconv.ParseInt(resetStr, 10, 64); err == nil {
			now := time.Now().Unix()
			diffSec := epoch - now
			minutes := int(math.Ceil(float64(diffSec) / 60.0))
			if minutes < 1 {
				minutes = 1
			}
			return fmt.Sprintf("GitHub is limiting requests right now. Try again in %d minutes.", minutes)
		}
	}

	// 2. Check Retry-After (seconds)
	if retryAfterStr := resp.Header.Get("Retry-After"); retryAfterStr != "" {
		if secs, err := strconv.Atoi(retryAfterStr); err == nil {
			minutes := int(math.Ceil(float64(secs) / 60.0))
			if minutes < 1 {
				minutes = 1
			}
			return fmt.Sprintf("GitHub is limiting requests right now. Try again in %d minutes.", minutes)
		}
	}

	return "GitHub is limiting requests right now. Try again in 60 minutes."
}

// isVersionNewer compares two semantic versions (e.g. "v1.2.0" and "v1.1.2").
func isVersionNewer(latest, current string) bool {
	latest = strings.TrimPrefix(strings.TrimSpace(latest), "v")
	current = strings.TrimPrefix(strings.TrimSpace(current), "v")
	if latest == "" || latest == current {
		return false
	}

	latestParts := parseVersionParts(latest)
	currentParts := parseVersionParts(current)

	for i := 0; i < 3; i++ {
		if latestParts[i] > currentParts[i] {
			return true
		}
		if latestParts[i] < currentParts[i] {
			return false
		}
	}
	return false
}

func parseVersionParts(v string) [3]int {
	var parts [3]int
	if idx := strings.IndexAny(v, "-+"); idx != -1 {
		v = v[:idx]
	}
	segments := strings.Split(v, ".")
	for i := 0; i < len(segments) && i < 3; i++ {
		num, _ := strconv.Atoi(segments[i])
		parts[i] = num
	}
	return parts
}

// CheckForUpdates checks for updates. If manual is true, it always contacts GitHub.
// If manual is false, it respects the AutoCheckUpdates preference and throttles to at most once per 12 hours.
func (g *ConfigManager) CheckForUpdates(manual bool) (UpdateInfo, error) {
	return g.CheckForUpdatesWithRedirectURL(DefaultGitHubReleasesURL, manual)
}

// CheckForUpdatesWithRedirectURL checks for updates against the provided redirect URL.
func (g *ConfigManager) CheckForUpdatesWithRedirectURL(redirectURL string, manual bool) (UpdateInfo, error) {
	autoCheck := g.GetAutoCheckUpdates()
	if !manual && !autoCheck {
		return UpdateInfo{
			CurrentVersion: AppVersion,
			HasUpdate:      false,
			CheckedAt:      time.Now().UTC().Format(time.RFC3339),
		}, nil
	}

	now := time.Now().Unix()
	if !manual {
		lastCheck := g.GetLastUpdateCheck()
		if lastCheck > 0 && time.Since(time.Unix(lastCheck, 0)) < UpdateCheckThrottleDuration {
			// Throttled: at most once per 12 hours. Do not make any network request.
			return UpdateInfo{
				CurrentVersion: AppVersion,
				HasUpdate:      false,
				CheckedAt:      time.Unix(lastCheck, 0).UTC().Format(time.RFC3339),
			}, nil
		}
	}

	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()

	info, err := CheckForUpdatesWithURL(ctx, redirectURL, AppVersion, manual, autoCheck)

	// Persist last-check time
	g.SetLastUpdateCheck(now)

	if !manual {
		// Automatic check on startup must be silent on failure and never retry in a loop
		if err != nil || info.Error != "" {
			return UpdateInfo{
				CurrentVersion: AppVersion,
				HasUpdate:      false,
				CheckedAt:      time.Unix(now, 0).UTC().Format(time.RFC3339),
			}, nil
		}
	}

	return info, err
}

// GetReleaseNotes fetches the release notes for a given tag.
func (g *ConfigManager) GetReleaseNotes(tag string) (string, error) {
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	return FetchReleaseNotesWithURL(ctx, DefaultGitHubAPIReleasesURL, tag, AppVersion)
}
