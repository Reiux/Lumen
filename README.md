# Lumen

Unofficial desktop client for Google Photos with a Material 3 Expressive interface. Lumen is a redesign fork of [gotohp](https://github.com/xob0t/gotohp) with tonal color roles, large rounded shapes, and smooth motion. Upload and auth logic are preserved from upstream.

![Lumen Demo](readme_assets/app_demo.gif)

Lumen lets you back up photo and video libraries to Google Photos directly from your desktop. It supports quota-free uploads, Apple Live Photo pairing, background folder synchronization, upload bandwidth throttling, and local deduplication.

> **Disclaimer:** Unofficial client. Not affiliated with, endorsed by, or supported by Google LLC. Google Photos is a trademark of Google LLC.

---

## Features

- **Unlimited Uploads:** Upload original media without consuming your Google storage quota by default (or toggle quota usage when needed).
- **Material 3 Expressive UI:** Built with Google's Material 3 Expressive design language, featuring saturated color palettes, spatial spring motion curves, and dark theme support.
- **Continuous Sliding Navigation:** Hardware-accelerated 60fps horizontal viewport track that seamlessly glides between Upload, Settings, and About views with zero style recalculation overhead.
- **Tablet & Desktop Collapsible Rail:** Responsive navigation rail that expands on wide desktop screens (≥860px) and collapses into a compact 76px icon rail on narrower windows, with manual toggle support.
- **Tonal Drop Zone & Media Picker:** Single unified drop zone with shape-morphing drag feedback, auto-album organization, and a floating Extended FAB for native file/folder selection.
- **Apple Live Photos:** Automatically detects and pairs `.heic`/`.jpg` images with their matching `.mov` clips using QuickTime metadata or filename stems.
- **Account-Keyed Hash Cache:** Deduplicates uploads locally using isolated `(account email, SHA-256 hash, size)` caching, populated strictly upon Google's confirmed upload receipt.
- **Live Bandwidth Throttling:** Discrete log-scale bandwidth limits (Unlimited, 0.5 to 50 MB/s) applied live across active upload threads.
- **Folder Watching (Auto-Backup):** Background watcher monitors chosen folders, verifies file write completion, and uploads to bound accounts with backlog confirmation.
- **System Tray & Desktop Notifications:** Minimizes to the system tray on close with single-instance window focusing, and sends silent batch summary notifications upon completion.
- **Multi-Account Management:** Store, switch, and remove multiple Google accounts with custom profile avatars and token-binding support for captured Android sessions.
- **Recent Albums Browser:** Select from recently accessed albums or auto-create albums based on local folder directory names.
- **Dedicated About View:** Direct access to repository links, build details, architecture overview, and legal disclaimers.
- **Standalone CLI:** Headless command-line binary (`lumen-cli`) for headless servers, scripts, and cron jobs.

---

## Downloads

Download prebuilt binaries for Linux, macOS, and Windows from the [Releases](https://github.com/Reiux/Lumen/releases/latest) page.

---

## CLI Usage

The standalone binary `lumen-cli` runs without GUI dependencies. The desktop binary `lumen` also accepts these subcommands directly.

```shell
# Upload photos recursively using 5 threads
lumen-cli upload /path/to/photos --recursive --threads 5

# Upload directly to an album (creates the album if it does not exist)
lumen-cli upload /path/to/trip --album "Tokyo 2026"

# Pair Apple Live Photos
lumen-cli upload /path/to/camera --pair-live-photos

# Use Storage Saver quality instead of original quality
lumen-cli upload /path/to/photos --saver

# Add credentials
lumen-cli creds add "<oauth_token cookie value>"
lumen-cli creds add "androidId=..."
lumen-cli creds add - < token.txt

# List and switch active account
lumen-cli creds list
lumen-cli creds set user@gmail.com
```

The CLI shares stored credentials with the GUI via the user configuration file. All other upload settings (threads, quota, quality, album) are controlled via command-line flags so that automated scripts remain predictable regardless of GUI preferences.

Run `lumen-cli --help` or `lumen-cli upload --help` for the full list of options.

---

## Sign In

### Option 1: Embedded Setup (Recommended)

1. Open Lumen and click **Connect Account** (or **Add Google account**).
2. Click **Open Google sign-in** to launch the sign-in prompt in your default browser.
3. Sign in to your account and click **I agree** on the Google Embedded Setup page. The page may continue loading indefinitely; this is expected.
4. Open your browser Developer Tools (`F12` or right-click -> Inspect).
5. Navigate to **Application** (or **Storage**) -> **Cookies** -> `https://accounts.google.com`.
6. Copy the value of the `oauth_token` cookie.
7. Paste the token into Lumen and click **Connect account**.

From the CLI, open the Embedded Setup URL in a browser and add the token:
```shell
lumen-cli creds add "<oauth_token value>"
```

### Option 2: ReVanced (No root required)

<details>
  <summary><strong>View instructions</strong></summary>

1. Install Google Photos ReVanced on your Android device or emulator:
   - Install [GmsCore](https://github.com/ReVanced/GmsCore/releases).
   - Install a patched Google Photos APK (via [Morphe-AutoBuilds](https://github.com/RookieEnough/Morphe-AutoBuilds/releases) or patch it yourself using ReVanced Manager).
2. Enable USB debugging and connect your device to your PC via ADB.
3. Open a terminal on your PC and run:

   **Linux / macOS:**
   ```shell
   adb logcat | grep "auth%2Fphotos.native"
   ```

   **Windows (Command Prompt):**
   ```cmd
   adb logcat | FINDSTR "auth%2Fphotos.native"
   ```

4. If you already had an account logged in, remove the Google account from GmsCore first.
5. Open Google Photos ReVanced on the device and sign in.
6. A log line containing the authentication parameters will appear in your terminal.
7. Copy the string starting from `androidId=` through to the end of that line.
8. Paste that string into Lumen or run `lumen-cli creds add "androidId=..."`.

</details>

### Option 3: Official APK (Root required)

<details>
  <summary><strong>View instructions</strong></summary>

1. Ensure you have a rooted Android device or emulator with ADB root access.
2. Connect your device via ADB and install [HTTP Toolkit](https://httptoolkit.com).
3. In HTTP Toolkit, choose **Android Device via ADB** intercept mode.
4. Filter intercepted traffic using:
   ```text
   contains(https://www.googleapis.com/auth/photos.native)
   ```
   *(For older Google Photos versions, try `contains(www.googleapis.com%2Fauth%2Fplus.photos.readwrite)`).*
5. Open the official Google Photos app on your device and log in.
6. When the matching request appears in HTTP Toolkit, copy the request body text.
7. Paste the body string into Lumen.
8. If Lumen prompts for a token binding key, keep the device connected with ADB root enabled and click **Read from ADB**. Lumen will read `lstBindingKeyAlias` from the Android AccountManager and attach it to the credential.

#### Troubleshooting
- **No request intercepted:** Log out and log back in, or use HTTP Toolkit's `Android App via Frida` interception method.
- **Token binding key not found:** Confirm root is granted to ADB shell (`adb root`) and the target Google account is present on the device.

</details>

---

## Building from Source

### Prerequisites

- **Go:** 1.24 or higher
- **Node.js:** 20.x or higher with `npm`
- **C Compiler:** `gcc` or `clang`
- **Linux dependencies (Linux only):** `gtk3` / `gtk4`, `webkit2gtk-4.1` (or `webkit2gtk-4.0`), `pkg-config`
  - Arch/CachyOS: `sudo pacman -S webkit2gtk-4.1 gtk3 base-devel`
  - Debian/Ubuntu: `sudo apt-get install libgtk-3-dev libwebkit2gtk-4.1-dev build-essential`
  - Fedora: `sudo dnf install gtk3-devel webkit2gtk4.1-devel gcc`
- **Wails v3 CLI (optional for packaging):**
  ```shell
  go install github.com/wailsapp/wails/v3/cmd/wails3@latest
  ```

### Build Steps

1. **Clone the repository:**
   ```shell
   git clone https://github.com/Reiux/Lumen.git
   cd Lumen
   ```

2. **Build frontend assets:**
   ```shell
   cd frontend
   npm install
   npm run build
   cd ..
   ```

3. **Build the desktop GUI application:**
   ```shell
   go build -o bin/lumen .
   ```

4. **Build the standalone CLI:**
   ```shell
   go build -tags cli -o bin/lumen-cli .
   ```

Alternatively, if you have [Task](https://taskfile.dev) installed:
```shell
task build
```

---

## Configuration

Lumen stores configuration and saved accounts in your system configuration directory:
- **Linux:** `~/.config/gotohp.config`
- **macOS:** `~/Library/Application Support/gotohp.config`
- **Windows:** `%APPDATA%\gotohp.config`

To use a portable configuration file next to the binary, create an empty `gotohp.config` file in the same directory as the executable.

---

## Credits & License

- Original project and upload logic: [gotohp](https://github.com/xob0t/gotohp) by [xob0t](https://github.com/xob0t), released under the MIT License.
- Redesigned interface & new features: Lumen contributors.
- Licensed under the [MIT License](LICENSE). Copyright (c) 2024-2026 xob0t and Lumen contributors.
