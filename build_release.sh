#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=== Building Lumen Release Binaries ==="
cargo build --release --workspace

echo ""
echo "=== Build Complete ==="
echo "CLI binary: target/release/lumen-cli ($(du -h target/release/lumen-cli | cut -f1))"
echo "GUI binary: target/release/lumen-tauri ($(du -h target/release/lumen-tauri | cut -f1))"
