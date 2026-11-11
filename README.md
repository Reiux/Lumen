# Lumen (Archived Go Edition)

**Fast, open-source desktop backup client for Google Photos (v1.x Legacy Series).**

![Downloads](https://img.shields.io/github/downloads/Reiux/Lumen/total?style=for-the-badge)
![License](https://img.shields.io/github/license/Reiux/Lumen?style=for-the-badge)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20macOS%20%7C%20Linux-blue?style=for-the-badge)

![Lumen](readme_assets/screenshot_upload.png)

> [!NOTE]
> **Disclaimer:** Lumen is an independent open-source project and is not affiliated with, endorsed by, or supported by Google LLC. Google Photos is a trademark of Google LLC.
>
> *Looking for the active Rust + Tauri 2 rewrite? Check out the **[default rust branch](https://github.com/Reiux/Lumen)**.*  
> *Looking for the Android app? Check out **[Lumen for Android](https://github.com/Reiux/lumen-android)**.*

> [!IMPORTANT]
> **Archived Branch:**  
> This branch preserves the archived Go/Wails codebase (v1.1.0 and earlier). For the actively maintained, high-performance desktop client, check out the **[rust branch](https://github.com/Reiux/Lumen)**.

Lumen is an unofficial desktop client for Google Photos (built with Wails and Vue 3 for the v1.x series). It's a fork of [gotohp](https://github.com/xob0t/gotohp) with a redesigned Material 3 Expressive interface. Core upload and authentication logic come from upstream.

### How it works

Lumen lets you back up photos and videos straight from your desktop to Google Photos. It talks directly to Google's mobile upload endpoints with device profile headers, so your media can upload in full original quality without eating your storage quota (or you can switch to Storage Saver / normal quota mode in the settings if you prefer).

Everything runs locally on your machine. No middleman servers, no extra accounts, no ads, and no tracking.

---

## Features

- **Original quality uploads:** Back up photos and videos at full resolution without counting against your Google storage, or switch to Storage Saver / standard quota mode whenever you want.
- **Automatic sign-in:** Lumen opens a Chromium browser (Chrome, Brave, Edge, etc.), picks up your sign-in and closes it. Manual token entry is available as a fallback.
- **Material 3 Expressive interface:** Color themes, light and dark mode, and a navigation rail that adapts to the window size.
- **Albums:** Upload into an existing album by pasting its key or link, give it a nickname, or create albums named after your folders. Reusing a name reuses the album.
- **Folder watcher:** Watch folders in the background and upload new photos as they are added, with an option to include subfolders.
- **Duplicate skipping:** Hashes of uploaded files are cached locally, so files you already backed up are skipped.
- **Apple Live Photos:** Pairs `.heic` and `.jpg` photos with their matching `.mov` clips.
- **Bandwidth limiter:** Limit uploads from 0.5 MB/s to 50 MB/s, or leave it unlimited.
- **Multiple accounts:** Add and switch between Google accounts.
- **Drag and drop:** Drop files or folders into the window, or use the picker.
- **System tray:** Keeps running in the background and notifies you when an upload finishes.
- **Standalone CLI:** A headless `lumen-cli` binary for scripts, servers and cron jobs.

---

## Screenshots

| Upload & Progress | Live Activity & Logs |
| :---: | :---: |
| <img src="readme_assets/screenshot_upload.png" width="100%" alt="Upload View" /> | <img src="readme_assets/screenshot_activity.png" width="100%" alt="Activity View" /> |

| Watched Folders | Multi-Account Switching |
| :---: | :---: |
| <img src="readme_assets/screenshot_folders.png" width="100%" alt="Watched Folders" /> | <img src="readme_assets/screenshot_accounts.png" width="100%" alt="Accounts Manager" /> |

| Material 3 Expressive Settings | About & Updates |
| :---: | :---: |
| <img src="readme_assets/screenshot_settings.png" width="100%" alt="Settings View" /> | <img src="readme_assets/screenshot_about.png" width="100%" alt="About View" /> |

---

## Security & Account Access

**Your password is never requested, stored, or seen by Lumen.**

When you sign in, Google gives the app an **OAuth token**.
- It works like a temporary key card that lets Lumen put photos into your Google Photos library.
- Uploads go straight from your computer to Google. No outside server touches your data.
- The token is stored in your operating system's secure credential manager (Linux Secret Service, Windows Credential Manager, macOS Keychain), not in a plain file.
- Lumen contains no analytics, telemetry or crash reporting.

---

## Account Safety & Risks

Lumen is an unofficial tool and isn't affiliated with Google. It uploads photos using mobile endpoints that aren't part of an official public API.

- **Terms of Service:** Using unofficial clients might violate Google's Terms of Service. Google can log you out, rate-limit, or restrict your account at any time.
- **Gmail & Drive access:** If Google flags or restricts your account, you could lose access to other Google services linked to that email.
- **Use a secondary account:** Don't use your main personal email, or anything tied to school, work, or banking. Use a throwaway or secondary account you're fine losing.
- **Keep your local copies:** Never treat Google Photos as your only backup. Keep the original files on your drive until you've verified everything.
- **Avoid repeated sign-ins:** If Google logs you out or asks for verification, don't spam sign-in attempts. Lumen automatically pauses when it runs into auth errors.

---

## Sign In

### Option 1: Automatic sign-in (recommended)

1. Open Lumen and click **Connect Account**.
2. Lumen opens a Chromium browser (Chrome, Brave, Edge, Chromium, etc.). Sign in to your Google account and accept the prompt.
3. Lumen captures the OAuth credentials, closes the browser, and terminates the temporary process.

### Option 2: Manual token

1. Open Lumen and click **Connect Account**, then choose manual entry.
2. Open the Google sign-in page in any browser and sign in. Click **I agree** on the Google Embedded Setup page.
3. Open Developer Tools (`F12`), go to **Application** (or **Storage**) -> **Cookies** -> `https://accounts.google.com`.
4. Copy the value of the `oauth_token` cookie.
5. Paste it into Lumen and click **Connect account**.

From the CLI:
```shell
lumen-cli creds add "<oauth_token value>"
```

### Option 3: ReVanced (No root required)

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

### Option 4: Official APK (Root required)

<details>
  <summary><strong>View instructions</strong></summary>

1. Make sure you have a rooted Android device or emulator with ADB root access.
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
8. If Lumen asks for a token binding key, keep the device connected with ADB root enabled and click **Read from ADB**. Lumen will read `lstBindingKeyAlias` from the Android AccountManager and attach it to the credential.

#### Troubleshooting
- **No request intercepted:** Log out and log back in, or use HTTP Toolkit's `Android App via Frida` interception method.
- **Token binding key not found:** Confirm root is granted to ADB shell (`adb root`) and the target Google account is present on the device.

</details>

---

## CLI Usage

The standalone binary `lumen-cli` runs without any GUI dependencies. The desktop binary `lumen` also accepts these commands.

```shell
# Upload photos recursively using 5 threads
lumen-cli upload /path/to/photos --recursive --threads 5

# Upload to an album (by name, creating it if needed; an album key or link also works)
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

The CLI shares stored credentials with the app. Upload options are controlled by command-line flags so scripts run predictably. Run `lumen-cli --help` or `lumen-cli upload --help` for all options.

---

## Building from Source

### Prerequisites

- **Go:** `1.26.4` or later
- **Node.js:** `24` or later (Node 20.19+ and 22.13+ also supported)
- **C compiler:** `gcc` or `clang`
- **Linux packages (Debian/Ubuntu):**
  ```shell
  sudo apt-get install -y build-essential gcc pkg-config libgtk-4-dev libwebkitgtk-6.0-dev
  ```
- **Wails v3 CLI (optional):**
  ```shell
  go install github.com/wailsapp/wails/v3/cmd/wails3@v3.0.0-beta.33
  ```

### Build steps

```shell
git clone https://github.com/Reiux/Lumen.git
cd Lumen

# Frontend
cd frontend && npm install && npm run build && cd ..

# Desktop app
go build -o bin/lumen .

# Standalone CLI
go build -tags cli -o bin/lumen-cli .
```

Or with [Task](https://taskfile.dev): `task build`

---

## Configuration

Lumen stores settings and saved accounts in:
- **Linux:** `~/.config/lumen/lumen.config`
- **macOS:** `~/Library/Application Support/lumen/lumen.config`
- **Windows:** `%APPDATA%\lumen\lumen.config`

To run portably, place an empty `lumen.config` file next to the binary.

---

## Credits & License

- Original project and upload logic: [gotohp](https://github.com/xob0t/gotohp) by [xob0t](https://github.com/xob0t).
- UI redesign and enhancements: [Reiux](https://github.com/Reiux).
- Licensed under the [MIT License](LICENSE).
