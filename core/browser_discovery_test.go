package core

import (
	"errors"
	"testing"
)

func TestFindBrowser(t *testing.T) {
	tests := []struct {
		name       string
		goos       string
		lookPath   func(string) (string, error)
		fileExists func(string) bool
		env        func(string) string
		wantPath   string
		wantOk     bool
	}{
		// --- Windows Tests ---
		{
			name: "Windows: Edge found in ProgramFiles(x86)",
			goos: "windows",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found in path")
			},
			fileExists: func(p string) bool {
				return p == `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`
			},
			env: func(k string) string {
				switch k {
				case "ProgramFiles":
					return `C:\Program Files`
				case "ProgramFiles(x86)":
					return `C:\Program Files (x86)`
				case "LocalAppData":
					return `C:\Users\TestUser\AppData\Local`
				}
				return ""
			},
			wantPath: `C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`,
			wantOk:   true,
		},
		{
			name: "Windows: Chrome found in ProgramFiles",
			goos: "windows",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found in path")
			},
			fileExists: func(p string) bool {
				return p == `C:\Program Files\Google\Chrome\Application\chrome.exe`
			},
			env: func(k string) string {
				switch k {
				case "ProgramFiles":
					return `C:\Program Files`
				case "ProgramFiles(x86)":
					return `C:\Program Files (x86)`
				}
				return ""
			},
			wantPath: `C:\Program Files\Google\Chrome\Application\chrome.exe`,
			wantOk:   true,
		},
		{
			name: "Windows: Brave found in LocalAppData",
			goos: "windows",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found in path")
			},
			fileExists: func(p string) bool {
				return p == `C:\Users\TestUser\AppData\Local\BraveSoftware\Brave-Browser\Application\brave.exe`
			},
			env: func(k string) string {
				if k == "LocalAppData" {
					return `C:\Users\TestUser\AppData\Local`
				}
				return ""
			},
			wantPath: `C:\Users\TestUser\AppData\Local\BraveSoftware\Brave-Browser\Application\brave.exe`,
			wantOk:   true,
		},
		{
			name: "Windows: Chromium found in LocalAppData",
			goos: "windows",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found in path")
			},
			fileExists: func(p string) bool {
				return p == `C:\Users\TestUser\AppData\Local\Chromium\Application\chrome.exe`
			},
			env: func(k string) string {
				if k == "LocalAppData" {
					return `C:\Users\TestUser\AppData\Local`
				}
				return ""
			},
			wantPath: `C:\Users\TestUser\AppData\Local\Chromium\Application\chrome.exe`,
			wantOk:   true,
		},
		{
			name: "Windows: None found",
			goos: "windows",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found in path")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return ""
			},
			wantPath: "",
			wantOk:   false,
		},

		// --- macOS Tests ---
		{
			name: "macOS: Chrome found in /Applications",
			goos: "darwin",
			lookPath: func(string) (string, error) {
				return "", errors.New("lookpath not used on darwin")
			},
			fileExists: func(p string) bool {
				return p == "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
			},
			env: func(string) string {
				return "/Users/testuser"
			},
			wantPath: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
			wantOk:   true,
		},
		{
			name: "macOS: Edge found in /Applications",
			goos: "darwin",
			lookPath: func(string) (string, error) {
				return "", errors.New("lookpath not used on darwin")
			},
			fileExists: func(p string) bool {
				return p == "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"
			},
			env: func(string) string {
				return ""
			},
			wantPath: "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
			wantOk:   true,
		},
		{
			name: "macOS: Brave found in user Applications",
			goos: "darwin",
			lookPath: func(string) (string, error) {
				return "", errors.New("lookpath not used on darwin")
			},
			fileExists: func(p string) bool {
				return p == "/Users/testuser/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"
			},
			env: func(k string) string {
				if k == "HOME" {
					return "/Users/testuser"
				}
				return ""
			},
			wantPath: "/Users/testuser/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
			wantOk:   true,
		},
		{
			name: "macOS: None found",
			goos: "darwin",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return "/Users/testuser"
			},
			wantPath: "",
			wantOk:   false,
		},

		// --- Linux Tests ---
		{
			name: "Linux: google-chrome found via lookPath",
			goos: "linux",
			lookPath: func(name string) (string, error) {
				if name == "google-chrome" {
					return "/usr/bin/google-chrome", nil
				}
				return "", errors.New("not found")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return ""
			},
			wantPath: "/usr/bin/google-chrome",
			wantOk:   true,
		},
		{
			name: "Linux: google-chrome-stable found when google-chrome missing",
			goos: "linux",
			lookPath: func(name string) (string, error) {
				if name == "google-chrome-stable" {
					return "/usr/bin/google-chrome-stable", nil
				}
				return "", errors.New("not found")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return ""
			},
			wantPath: "/usr/bin/google-chrome-stable",
			wantOk:   true,
		},
		{
			name: "Linux: chromium found via lookPath",
			goos: "linux",
			lookPath: func(name string) (string, error) {
				if name == "chromium" {
					return "/usr/bin/chromium", nil
				}
				return "", errors.New("not found")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return ""
			},
			wantPath: "/usr/bin/chromium",
			wantOk:   true,
		},
		{
			name: "Linux: brave-browser found via lookPath",
			goos: "linux",
			lookPath: func(name string) (string, error) {
				if name == "brave-browser" {
					return "/usr/bin/brave-browser", nil
				}
				return "", errors.New("not found")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return ""
			},
			wantPath: "/usr/bin/brave-browser",
			wantOk:   true,
		},
		{
			name: "Linux: microsoft-edge found via lookPath",
			goos: "linux",
			lookPath: func(name string) (string, error) {
				if name == "microsoft-edge" {
					return "/usr/bin/microsoft-edge", nil
				}
				return "", errors.New("not found")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return ""
			},
			wantPath: "/usr/bin/microsoft-edge",
			wantOk:   true,
		},
		{
			name: "Linux: None found in lookPath, but fallback file exists",
			goos: "linux",
			lookPath: func(string) (string, error) {
				return "", errors.New("not in path")
			},
			fileExists: func(p string) bool {
				return p == "/snap/bin/chromium"
			},
			env: func(string) string {
				return ""
			},
			wantPath: "/snap/bin/chromium",
			wantOk:   true,
		},
		{
			name: "Linux: None found anywhere",
			goos: "linux",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return ""
			},
			wantPath: "",
			wantOk:   false,
		},

		// --- Other / Unknown OS ---
		{
			name: "Unknown OS: None found",
			goos: "freebsd",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found")
			},
			fileExists: func(string) bool {
				return false
			},
			env: func(string) string {
				return ""
			},
			wantPath: "",
			wantOk:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			gotPath, gotOk := FindBrowser(tt.goos, tt.lookPath, tt.fileExists, tt.env)
			if gotOk != tt.wantOk {
				t.Fatalf("FindBrowser() ok = %v, want %v", gotOk, tt.wantOk)
			}
			if gotPath != tt.wantPath {
				t.Fatalf("FindBrowser() path = %q, want %q", gotPath, tt.wantPath)
			}
		})
	}
}

func TestClassifyBrowser(t *testing.T) {
	tests := []struct {
		path string
		want BrowserType
	}{
		{"/opt/zen-browser-bin/zen-bin", BrowserTypeUnknown},
		{"/usr/bin/zen-browser", BrowserTypeUnknown},
		{"/usr/bin/zen", BrowserTypeUnknown},
		{"/home/user/.local/share/flatpak/exports/bin/io.github.zen_browser.zen", BrowserTypeUnknown},
		{"/usr/bin/firefox", BrowserTypeUnknown},
		{"/snap/bin/firefox", BrowserTypeUnknown},
		{"/usr/bin/librewolf", BrowserTypeUnknown},
		{"/usr/bin/waterfox", BrowserTypeUnknown},
		{"/usr/bin/floorp", BrowserTypeUnknown},
		{`C:\Program Files\Mozilla Firefox\firefox.exe`, BrowserTypeUnknown},
		{"/Applications/Firefox.app/Contents/MacOS/firefox", BrowserTypeUnknown},
		{"/Applications/Zen Browser.app/Contents/MacOS/zen", BrowserTypeUnknown},

		{"/usr/bin/google-chrome", BrowserTypeChromium},
		{"/usr/bin/google-chrome-stable", BrowserTypeChromium},
		{"/usr/bin/chromium", BrowserTypeChromium},
		{"/usr/bin/chromium-browser", BrowserTypeChromium},
		{"/usr/bin/brave-browser", BrowserTypeChromium},
		{"/usr/bin/microsoft-edge", BrowserTypeChromium},
		{`C:\Program Files\Google\Chrome\Application\chrome.exe`, BrowserTypeChromium},
		{`C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe`, BrowserTypeChromium},
		{`C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe`, BrowserTypeChromium},
		{"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", BrowserTypeChromium},

		{"/usr/bin/bash", BrowserTypeUnknown},
		{"/usr/bin/curl", BrowserTypeUnknown},
		{"", BrowserTypeUnknown},
	}

	for _, tt := range tests {
		got := ClassifyBrowser(tt.path)
		if got != tt.want {
			t.Errorf("ClassifyBrowser(%q) = %v, want %v", tt.path, got, tt.want)
		}
	}
}

func TestExtractExecFromDesktopFile(t *testing.T) {
	tests := []struct {
		name    string
		content string
		want    string
	}{
		{
			name: "Standard Exec line with arguments",
			content: `[Desktop Entry]
Version=1.0
Name=Zen Browser
Exec=/opt/zen-browser-bin/zen-bin %u
Icon=zen
Terminal=false
Type=Application`,
			want: "/opt/zen-browser-bin/zen-bin",
		},
		{
			name: "Quoted Exec command",
			content: `[Desktop Entry]
Name=Chrome
Exec="/opt/google/chrome/chrome" --enable-features=UseOzonePlatform %U
Type=Application`,
			want: "/opt/google/chrome/chrome",
		},
		{
			name: "Exec without arguments",
			content: `[Desktop Entry]
Name=Firefox
Exec=firefox
Type=Application`,
			want: "firefox",
		},
		{
			name: "Exec in wrong desktop section ignored",
			content: `[Desktop Action NewTab]
Exec=firefox --new-tab
[Desktop Entry]
Exec=firefox
`,
			want: "firefox",
		},
		{
			name:    "Empty desktop file",
			content: "",
			want:    "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := extractExecFromDesktopFile([]byte(tt.content))
			if got != tt.want {
				t.Errorf("extractExecFromDesktopFile() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestFindDefaultBrowserLinux(t *testing.T) {
	tests := []struct {
		name       string
		lookPath   func(string) (string, error)
		fileExists func(string) bool
		readFile   func(string) ([]byte, error)
		env        func(string) string
		runCmd     func(string, ...string) (string, error)
		wantPath   string
		wantType   BrowserType
		wantOk     bool
	}{
		{
			name: "Zen browser via xdg-settings and desktop file is unsupported and ignored",
			lookPath: func(cmd string) (string, error) {
				return "", errors.New("not found")
			},
			fileExists: func(p string) bool {
				return p == "/usr/share/applications/zen.desktop" || p == "/opt/zen-browser-bin/zen-bin"
			},
			readFile: func(p string) ([]byte, error) {
				if p == "/usr/share/applications/zen.desktop" {
					return []byte("[Desktop Entry]\nExec=/opt/zen-browser-bin/zen-bin %u\n"), nil
				}
				return nil, errors.New("file not found")
			},
			env: func(k string) string {
				if k == "HOME" {
					return "/home/test"
				}
				return ""
			},
			runCmd: func(cmd string, args ...string) (string, error) {
				if cmd == "xdg-settings" && len(args) >= 2 && args[0] == "get" && args[1] == "default-web-browser" {
					return "zen.desktop\n", nil
				}
				return "", errors.New("command failed")
			},
			wantPath: "",
			wantType: BrowserTypeUnknown,
			wantOk:   false,
		},
		{
			name: "Chrome fallback via xdg-mime when xdg-settings fails",
			lookPath: func(cmd string) (string, error) {
				if cmd == "google-chrome-stable" {
					return "/usr/bin/google-chrome-stable", nil
				}
				return "", errors.New("not found")
			},
			fileExists: func(p string) bool {
				return p == "/usr/share/applications/google-chrome.desktop"
			},
			readFile: func(p string) ([]byte, error) {
				if p == "/usr/share/applications/google-chrome.desktop" {
					return []byte("[Desktop Entry]\nExec=google-chrome-stable %U\n"), nil
				}
				return nil, errors.New("file not found")
			},
			env: func(string) string { return "" },
			runCmd: func(cmd string, args ...string) (string, error) {
				if cmd == "xdg-settings" {
					return "", errors.New("xdg-settings error")
				}
				if cmd == "xdg-mime" && len(args) >= 3 && args[0] == "query" && args[1] == "default" {
					return "google-chrome.desktop\n", nil
				}
				return "", errors.New("command failed")
			},
			wantPath: "/usr/bin/google-chrome-stable",
			wantType: BrowserTypeChromium,
			wantOk:   true,
		},
		{
			name: "Unsupported default browser (e.g. lynx)",
			lookPath: func(cmd string) (string, error) {
				if cmd == "lynx" {
					return "/usr/bin/lynx", nil
				}
				return "", errors.New("not found")
			},
			fileExists: func(p string) bool {
				return p == "/usr/share/applications/lynx.desktop" || p == "/usr/bin/lynx"
			},
			readFile: func(p string) ([]byte, error) {
				if p == "/usr/share/applications/lynx.desktop" {
					return []byte("[Desktop Entry]\nExec=/usr/bin/lynx %u\n"), nil
				}
				return nil, errors.New("file not found")
			},
			env: func(string) string { return "" },
			runCmd: func(cmd string, args ...string) (string, error) {
				if cmd == "xdg-settings" {
					return "lynx.desktop\n", nil
				}
				return "", errors.New("failed")
			},
			wantPath: "",
			wantType: BrowserTypeUnknown,
			wantOk:   false,
		},
		{
			name: "No default browser configured",
			lookPath: func(string) (string, error) {
				return "", errors.New("not found")
			},
			fileExists: func(string) bool { return false },
			readFile:   func(string) ([]byte, error) { return nil, errors.New("not found") },
			env:        func(string) string { return "" },
			runCmd: func(string, ...string) (string, error) {
				return "", errors.New("not found")
			},
			wantPath: "",
			wantType: BrowserTypeUnknown,
			wantOk:   false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			b, ok := FindDefaultBrowserLinux(tt.lookPath, tt.fileExists, tt.readFile, tt.env, tt.runCmd)
			if ok != tt.wantOk {
				t.Fatalf("FindDefaultBrowserLinux() ok = %v, want %v", ok, tt.wantOk)
			}
			if ok {
				if b.Path != tt.wantPath {
					t.Errorf("FindDefaultBrowserLinux() path = %q, want %q", b.Path, tt.wantPath)
				}
				if b.Type != tt.wantType {
					t.Errorf("FindDefaultBrowserLinux() type = %v, want %v", b.Type, tt.wantType)
				}
			}
		})
	}
}

func TestFindSystemSupportedBrowser_Live(t *testing.T) {
	b, ok := FindSystemSupportedBrowser()
	t.Logf("FindSystemSupportedBrowser live result: ok=%v, path=%q, type=%v", ok, b.Path, b.Type)
}
