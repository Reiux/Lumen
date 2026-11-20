#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PAYLOAD_PATH="crates/lumen-installer/payload.tar.gz"
cleanup() {
  rm -f "$PAYLOAD_PATH"
}
trap cleanup EXIT

echo "=== Building Frontend Assets ==="
if [ ! -d "frontend/node_modules" ] || [ "frontend/package-lock.json" -nt "frontend/node_modules" ]; then
  npm ci --prefix frontend --engine-strict=false
fi
npm run build --prefix frontend

echo "=== Building Lumen Release Binaries ==="
cargo build --release -p lumen-cli --bin lumen-cli
cargo build --release -p lumen-tauri --bin lumen

echo "=== Packaging Installer Payload ==="
tar -czf "$PAYLOAD_PATH" -C target/release lumen lumen-cli
cargo build --release -p lumen-installer --bin lumen-installer

echo ""
echo "=== Build Complete ==="
echo "CLI binary: target/release/lumen-cli ($(du -h target/release/lumen-cli | cut -f1))"
echo "GUI binary: target/release/lumen ($(du -h target/release/lumen | cut -f1))"
echo "Installer binary: target/release/lumen-installer ($(du -h target/release/lumen-installer | cut -f1))"
