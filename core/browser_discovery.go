package core

import (
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
)

// FindBrowser locates an installed Chromium-based browser (Chrome, Edge, Brave, Chromium)
// on the system in a pure, injectable manner.
// It accepts goos (e.g. "windows", "darwin", "linux"), lookPath for PATH lookups,
// fileExists for filesystem checks, and env for environment variables.
func FindBrowser(
	goos string,
	lookPath func(string) (string, error),
	fileExists func(string) bool,
	env func(string) string,
) (string, bool) {
	if lookPath == nil {
		lookPath = exec.LookPath
	}
	if fileExists == nil {
		fileExists = func(p string) bool {
			fi, err := os.Stat(p)
			return err == nil && !fi.IsDir()
		}
	}
	if env == nil {
		env = os.Getenv
	}

	switch strings.ToLower(goos) {
	case "windows":
		return findBrowserWindows(lookPath, fileExists, env)
	case "darwin":
		return findBrowserDarwin(lookPath, fileExists, env)
	case "linux":
		return findBrowserLinux(lookPath, fileExists)
	default:
		// Attempt Linux-style PATH lookup as generic fallback
		return findBrowserLinux(lookPath, fileExists)
	}
}

func findBrowserWindows(
	lookPath func(string) (string, error),
	fileExists func(string) bool,
	env func(string) string,
) (string, bool) {
	progFiles := env("ProgramFiles")
	if progFiles == "" {
		progFiles = `C:\Program Files`
	}
	progFilesX86 := env("ProgramFiles(x86)")
	if progFilesX86 == "" {
		progFilesX86 = `C:\Program Files (x86)`
	}
	localAppData := env("LocalAppData")
	if localAppData == "" {
		localAppData = `C:\Users\Default\AppData\Local`
	}

	joinWin := func(base, sub string) string {
		base = strings.TrimRight(base, `\/`)
		sub = strings.TrimLeft(sub, `\/`)
		return base + `\` + strings.ReplaceAll(sub, `/`, `\`)
	}

	candidates := []string{
		// Microsoft Edge
		joinWin(progFilesX86, `Microsoft\Edge\Application\msedge.exe`),
		joinWin(progFiles, `Microsoft\Edge\Application\msedge.exe`),

		// Google Chrome
		joinWin(progFiles, `Google\Chrome\Application\chrome.exe`),
		joinWin(progFilesX86, `Google\Chrome\Application\chrome.exe`),
		joinWin(localAppData, `Google\Chrome\Application\chrome.exe`),

		// Brave
		joinWin(progFiles, `BraveSoftware\Brave-Browser\Application\brave.exe`),
		joinWin(progFilesX86, `BraveSoftware\Brave-Browser\Application\brave.exe`),
		joinWin(localAppData, `BraveSoftware\Brave-Browser\Application\brave.exe`),

		// Chromium
		joinWin(progFiles, `Chromium\Application\chrome.exe`),
		joinWin(localAppData, `Chromium\Application\chrome.exe`),
	}

	for _, path := range candidates {
		if fileExists(path) {
			return path, true
		}
	}

	// Try PATH search on Windows
	for _, name := range []string{"msedge.exe", "chrome.exe", "brave.exe", "chromium.exe"} {
		if p, err := lookPath(name); err == nil && p != "" {
			return p, true
		}
	}

	return "", false
}

func findBrowserDarwin(
	_ func(string) (string, error),
	fileExists func(string) bool,
	env func(string) string,
) (string, bool) {
	candidates := []string{
		"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
		"/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
		"/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
		"/Applications/Chromium.app/Contents/MacOS/Chromium",
	}

	if home := env("HOME"); home != "" {
		candidates = append(candidates,
			filepath.Join(home, "Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
			filepath.Join(home, "Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
			filepath.Join(home, "Applications/Brave Browser.app/Contents/MacOS/Brave Browser"),
			filepath.Join(home, "Applications/Chromium.app/Contents/MacOS/Chromium"),
		)
	}

	for _, path := range candidates {
		if fileExists(path) {
			return path, true
		}
	}

	return "", false
}

func findBrowserLinux(
	lookPath func(string) (string, error),
	fileExists func(string) bool,
) (string, bool) {
	names := []string{
		"google-chrome",
		"google-chrome-stable",
		"chromium",
		"chromium-browser",
		"brave-browser",
		"microsoft-edge",
		"microsoft-edge-stable",
	}

	for _, name := range names {
		if p, err := lookPath(name); err == nil && p != "" {
			return p, true
		}
	}

	// Fallback to standard absolute paths
	fixedPaths := []string{
		"/usr/bin/google-chrome",
		"/usr/bin/google-chrome-stable",
		"/usr/bin/chromium",
		"/usr/bin/chromium-browser",
		"/usr/bin/brave-browser",
		"/usr/bin/microsoft-edge",
		"/snap/bin/chromium",
		"/var/lib/flatpak/exports/bin/com.google.Chrome",
		"/var/lib/flatpak/exports/bin/org.chromium.Chromium",
		"/var/lib/flatpak/exports/bin/com.brave.Browser",
		"/var/lib/flatpak/exports/bin/com.microsoft.Edge",
	}

	for _, path := range fixedPaths {
		if fileExists(path) {
			return path, true
		}
	}

	return "", false
}

// FindSystemBrowser detects an installed Chromium browser using the host runtime environment.
func FindSystemBrowser() (string, bool) {
	return FindBrowser(
		runtime.GOOS,
		exec.LookPath,
		func(p string) bool {
			fi, err := os.Stat(p)
			return err == nil && !fi.IsDir()
		},
		os.Getenv,
	)
}

// BrowserType represents the automation engine type needed for a browser.
type BrowserType string

const (
	BrowserTypeChromium BrowserType = "chromium"
	BrowserTypeUnknown  BrowserType = "unknown"
)

// SupportedBrowser represents a located browser executable and its protocol driver type.
type SupportedBrowser struct {
	Path string
	Type BrowserType
}

// ClassifyBrowser determines whether a browser executable path is Chromium-based.
func ClassifyBrowser(path string) BrowserType {
	base := strings.ToLower(filepath.Base(path))

	// Check for Chromium-derived browsers
	if strings.Contains(base, "chrome") || strings.Contains(base, "chromium") ||
		strings.Contains(base, "brave") || strings.Contains(base, "edge") ||
		strings.Contains(base, "msedge") {
		return BrowserTypeChromium
	}

	return BrowserTypeUnknown
}

// extractExecFromDesktopFile extracts the executable command from a .desktop file content.
func extractExecFromDesktopFile(content []byte) string {
	lines := strings.Split(string(content), "\n")
	inDesktopEntry := false
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if strings.HasPrefix(trimmed, "[") && strings.HasSuffix(trimmed, "]") {
			inDesktopEntry = (trimmed == "[Desktop Entry]")
			continue
		}
		if inDesktopEntry && strings.HasPrefix(trimmed, "Exec=") {
			val := strings.TrimPrefix(trimmed, "Exec=")
			val = strings.TrimSpace(val)
			var bin string
			if strings.HasPrefix(val, "\"") {
				if end := strings.Index(val[1:], "\""); end != -1 {
					bin = val[1 : end+1]
				}
			}
			if bin == "" {
				fields := strings.Fields(val)
				if len(fields) > 0 {
					bin = fields[0]
				}
			}
			return strings.Trim(bin, "\"'")
		}
	}
	return ""
}

// FindDefaultBrowserLinux finds the default browser on Linux by querying xdg-settings or xdg-mime.
func FindDefaultBrowserLinux(
	lookPath func(string) (string, error),
	fileExists func(string) bool,
	readFile func(string) ([]byte, error),
	env func(string) string,
	runCmd func(string, ...string) (string, error),
) (SupportedBrowser, bool) {
	if runCmd == nil {
		runCmd = func(name string, args ...string) (string, error) {
			out, err := exec.Command(name, args...).Output()
			return strings.TrimSpace(string(out)), err
		}
	}
	if lookPath == nil {
		lookPath = exec.LookPath
	}
	if fileExists == nil {
		fileExists = func(p string) bool {
			fi, err := os.Stat(p)
			return err == nil && !fi.IsDir()
		}
	}
	if readFile == nil {
		readFile = os.ReadFile
	}
	if env == nil {
		env = os.Getenv
	}

	// 1. Try xdg-settings get default-web-browser
	desktopFile, err := runCmd("xdg-settings", "get", "default-web-browser")
	desktopFile = strings.TrimSpace(desktopFile)
	if err != nil || desktopFile == "" {
		// 2. Fallback to xdg-mime query default x-scheme-handler/http
		mimeOut, mimeErr := runCmd("xdg-mime", "query", "default", "x-scheme-handler/http")
		if mimeErr == nil && strings.TrimSpace(mimeOut) != "" {
			desktopFile = strings.TrimSpace(mimeOut)
		}
	}

	if desktopFile == "" {
		return SupportedBrowser{}, false
	}

	// Check if the desktop entry has a direct binary candidate name (e.g. zen.desktop -> zen-browser)
	checkDirectName := func(entry string) (string, bool) {
		name := strings.TrimSuffix(entry, ".desktop")
		candidates := []string{name}
		if strings.HasPrefix(name, "zen") {
			candidates = append([]string{"zen-browser", "zen"}, candidates...)
		}
		for _, c := range candidates {
			if p, err := lookPath(c); err == nil && p != "" {
				return p, true
			}
		}
		return "", false
	}

	// 3. Search for the .desktop file in standard desktop directories
	searchDirs := []string{
		"/usr/share/applications",
		"/usr/local/share/applications",
		"/var/lib/flatpak/exports/share/applications",
	}
	if home := env("HOME"); home != "" {
		searchDirs = append([]string{
			filepath.Join(home, ".local/share/applications"),
			filepath.Join(home, ".local/share/flatpak/exports/share/applications"),
		}, searchDirs...)
	}

	var resolvedBin string
	for _, dir := range searchDirs {
		dtPath := filepath.Join(dir, desktopFile)
		if fileExists(dtPath) {
			if content, readErr := readFile(dtPath); readErr == nil {
				execCmd := extractExecFromDesktopFile(content)
				if execCmd != "" {
					if fileExists(execCmd) {
						resolvedBin = execCmd
						break
					} else if p, err := lookPath(execCmd); err == nil && p != "" {
						resolvedBin = p
						break
					}
				}
			}
		}
	}

	if resolvedBin == "" {
		if p, ok := checkDirectName(desktopFile); ok {
			resolvedBin = p
		}
	}

	if resolvedBin != "" {
		bType := ClassifyBrowser(resolvedBin)
		if bType != BrowserTypeUnknown {
			return SupportedBrowser{Path: resolvedBin, Type: bType}, true
		}
	}

	return SupportedBrowser{}, false
}

// FindDefaultBrowser locates the default browser for the host OS.
func FindDefaultBrowser(
	goos string,
	lookPath func(string) (string, error),
	fileExists func(string) bool,
	readFile func(string) ([]byte, error),
	env func(string) string,
	runCmd func(string, ...string) (string, error),
) (SupportedBrowser, bool) {
	switch strings.ToLower(goos) {
	case "linux":
		return FindDefaultBrowserLinux(lookPath, fileExists, readFile, env, runCmd)
	default:
		return SupportedBrowser{}, false
	}
}

// FindSystemSupportedBrowser discovers the best available Chromium-based browser for automated sign-in.
// It prioritizes the user's OS default browser if it is Chromium-based.
// If the default browser is not Chromium-based or not found, it falls back to installed Chromium-family browsers.
func FindSystemSupportedBrowser() (SupportedBrowser, bool) {
	// 1. Try OS default browser first (Chromium only)
	if b, ok := FindDefaultBrowser(
		runtime.GOOS,
		exec.LookPath,
		func(p string) bool {
			fi, err := os.Stat(p)
			return err == nil && !fi.IsDir()
		},
		os.ReadFile,
		os.Getenv,
		func(name string, args ...string) (string, error) {
			out, err := exec.Command(name, args...).Output()
			return strings.TrimSpace(string(out)), err
		},
	); ok && b.Type == BrowserTypeChromium {
		return b, true
	}

	// 2. Fall back to any installed Chromium-family browser (Chrome, Brave, Edge, Chromium)
	if p, ok := FindSystemBrowser(); ok {
		return SupportedBrowser{Path: p, Type: BrowserTypeChromium}, true
	}

	return SupportedBrowser{}, false
}

// DisplayName returns a human-friendly name for the browser (e.g. "Brave", "Google Chrome").
func (b SupportedBrowser) DisplayName() string {
	base := strings.ToLower(filepath.Base(b.Path))
	full := strings.ToLower(b.Path)
	if strings.Contains(base, "brave") || strings.Contains(full, "brave") {
		return "Brave"
	}
	if strings.Contains(base, "chrome") || strings.Contains(full, "chrome") {
		return "Google Chrome"
	}
	if strings.Contains(base, "edge") || strings.Contains(full, "edge") {
		return "Microsoft Edge"
	}
	if strings.Contains(base, "chromium") || strings.Contains(full, "chromium") {
		return "Chromium"
	}
	return "Chromium"
}
