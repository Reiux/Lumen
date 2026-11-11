# Building Lumen Locally

This guide covers how to build Lumen (GUI client, CLI, and custom Windows installer) locally for **Windows (x86_64 & ARM64)**, **macOS**, and **Linux**.

---

## 1. Prerequisites (All Platforms)

1. **Rust Toolchain**: Install via [rustup.rs](https://rustup.rs).
2. **Node.js & npm**: Node.js 20+ or 24.
3. **Protocol Buffers (`protoc`)**:
   - **Linux**: `sudo apt install protobuf-compiler` (or `pacman -S protobuf` / `dnf install protobuf-compiler`)
   - **macOS**: `brew install protobuf`
   - **Windows**: `winget install Google.Protobuf` or `choco install protoc`

### Build Frontend Assets
Always build the frontend first before compiling the GUI or installer:

```bash
cd frontend
npm install
npm run build
cd ..
```

---

## 2. Windows

### A. Windows x86_64 (Cross-compiling from Linux/macOS using `cargo-xwin`)

```bash
# 1. Build GUI and CLI executables
cargo xwin build --release --target x86_64-pc-windows-msvc -p lumen-tauri --bin lumen
cargo xwin build --release --target x86_64-pc-windows-msvc -p lumen-cli --bin lumen-cli

# 2. Package binaries into installer payload
tar -czf crates/lumen-installer/payload.tar.gz -C target/x86_64-pc-windows-msvc/release lumen.exe lumen-cli.exe

# 3. Build standalone setup installer
cargo xwin build --release --target x86_64-pc-windows-msvc -p lumen-installer --bin lumen-installer
```

*Output installer:* `target/x86_64-pc-windows-msvc/release/lumen-installer.exe`

### B. Windows x86_64 (Native on Windows CMD or PowerShell)

```powershell
# 1. Build GUI and CLI executables
cargo build --release -p lumen-tauri --bin lumen
cargo build --release -p lumen-cli --bin lumen-cli

# 2. Package binaries into installer payload
tar -czf crates/lumen-installer/payload.tar.gz -C target/release lumen.exe lumen-cli.exe

# 3. Build standalone setup installer
cargo build --release -p lumen-installer --bin lumen-installer
```

*Output installer:* `target/release/lumen-installer.exe`

---

### C. Windows ARM64 (Cross-compiling using `cargo-xwin`)

```bash
# 1. Build GUI and CLI executables for ARM64
cargo xwin build --release --target aarch64-pc-windows-msvc -p lumen-tauri --bin lumen
cargo xwin build --release --target aarch64-pc-windows-msvc -p lumen-cli --bin lumen-cli

# 2. Package binaries into installer payload
tar -czf crates/lumen-installer/payload.tar.gz -C target/aarch64-pc-windows-msvc/release lumen.exe lumen-cli.exe

# 3. Build standalone setup installer for ARM64
cargo xwin build --release --target aarch64-pc-windows-msvc -p lumen-installer --bin lumen-installer
```

*Output installer:* `target/aarch64-pc-windows-msvc/release/lumen-installer.exe`

### D. Windows ARM64 (Native on Windows)

```powershell
# Add target if not installed
rustup target add aarch64-pc-windows-msvc

# Build binaries
cargo build --release --target aarch64-pc-windows-msvc -p lumen-tauri --bin lumen
cargo build --release --target aarch64-pc-windows-msvc -p lumen-cli --bin lumen-cli

# Package and build installer
tar -czf crates/lumen-installer/payload.tar.gz -C target/aarch64-pc-windows-msvc/release lumen.exe lumen-cli.exe
cargo build --release --target aarch64-pc-windows-msvc -p lumen-installer --bin lumen-installer
```

---

## 3. macOS

### Prerequisites for Universal Binaries (Apple Silicon & Intel)
```bash
rustup target add x86_64-apple-darwin aarch64-apple-darwin
```

### A. Build GUI (.dmg and .app bundle)
Builds a universal macOS app and DMG installer:

```bash
cd crates/lumen-tauri
npx --prefix ../../frontend tauri build --target universal-apple-darwin
cd ../..
```

*Output DMG:* `target/universal-apple-darwin/release/bundle/dmg/`  
*Output .app:* `target/universal-apple-darwin/release/bundle/macos/Lumen.app`

*(For single architecture build, e.g. Apple Silicon only, simply run `npx --prefix ../../frontend tauri build` inside `crates/lumen-tauri`)*

### B. Build Universal CLI (`lumen-cli`)
```bash
# Build both architecture slices
cargo build --release --target x86_64-apple-darwin -p lumen-cli --bin lumen-cli
cargo build --release --target aarch64-apple-darwin -p lumen-cli --bin lumen-cli

# Combine into a single universal binary using lipo
mkdir -p target/release
lipo -create -output target/release/lumen-cli \
  target/x86_64-apple-darwin/release/lumen-cli \
  target/aarch64-apple-darwin/release/lumen-cli

chmod +x target/release/lumen-cli
```

*Output CLI:* `target/release/lumen-cli`

---

## 4. Linux

### System Dependencies
- **Ubuntu / Debian**:
  ```bash
  sudo apt update
  sudo apt install -y libwebkit2gtk-4.1-dev build-essential curl wget file libssl-dev libayatana-appindicator3-dev librsvg2-dev protobuf-compiler
  ```
- **Fedora**:
  ```bash
  sudo dnf install webkit2gtk4.1-devel openssl-devel libappindicator-gtk3-devel librsvg2-devel protobuf-compiler
  ```
- **Arch Linux**:
  ```bash
  sudo pacman -S webkit2gtk-4.1 openssl libappindicator-gtk3 librsvg protobuf
  ```

### A. Build Standalone Portable Binaries
Produces standalone portable executables (`lumen` GUI and `lumen-cli` CLI):

```bash
cargo build --release -p lumen-tauri --bin lumen
cargo build --release -p lumen-cli --bin lumen-cli
```

*Output GUI:* `target/release/lumen`  
*Output CLI:* `target/release/lumen-cli`

### B. Build Linux Packages (.deb & AppImage)
```bash
cd crates/lumen-tauri
npx --prefix ../../frontend tauri build --bundles appimage,deb
cd ../..
```

*Output AppImage:* `target/release/bundle/appimage/*.AppImage`  
*Output Debian Package:* `target/release/bundle/deb/*.deb`
