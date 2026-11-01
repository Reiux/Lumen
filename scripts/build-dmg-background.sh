#!/usr/bin/env bash
# ==============================================================================
# build-dmg-background.sh
#
# Exports Lumen macOS DMG installer backgrounds from SVG:
#   - build/darwin/dmg/background.png    (540x380, standard resolution)
#   - build/darwin/dmg/background@2x.png (1080x760, Retina resolution)
#
# NOTE ON RETINA MULTI-RESOLUTION TIFF:
# macOS `tiffutil` is macOS-only. When building on a macOS host or runner,
# you can assemble a multi-resolution HiDPI TIFF asset with:
#   tiffutil -cathidpicheck \
#     build/darwin/dmg/background.png \
#     build/darwin/dmg/background@2x.png \
#     -out build/darwin/dmg/background.tiff
#
# DESIGN RULE FOR ICON SAFE AREAS:
# Keep the areas centered at (151, 164) and (388, 164) (approx 128x128 each plus
# 24px label room: x: 87..215 and x: 324..452, y: 100..252) strictly flat
# (#EEF2F6) so Finder icon labels remain legible with high contrast.
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SVG_IN="${REPO_ROOT}/build/darwin/dmg/background.svg"
OUT_DIR="${REPO_ROOT}/build/darwin/dmg"

if [[ ! -f "${SVG_IN}" ]]; then
  echo "Error: Source SVG not found at ${SVG_IN}" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"

echo "Exporting macOS DMG backgrounds from ${SVG_IN}..."

if command -v rsvg-convert >/dev/null 2>&1; then
  echo "Using rsvg-convert to render PNGs..."
  rsvg-convert -w 540 -h 380 -o "${OUT_DIR}/background.png" "${SVG_IN}"
  rsvg-convert -w 1080 -h 760 -o "${OUT_DIR}/background@2x.png" "${SVG_IN}"
elif command -v magick >/dev/null 2>&1; then
  echo "Using ImageMagick (magick) to render PNGs..."
  magick -density 144 -background none "${SVG_IN}" -resize 540x380 "${OUT_DIR}/background.png"
  magick -density 288 -background none "${SVG_IN}" -resize 1080x760 "${OUT_DIR}/background@2x.png"
elif command -v inkscape >/dev/null 2>&1; then
  echo "Using Inkscape to render PNGs..."
  inkscape "${SVG_IN}" --export-filename="${OUT_DIR}/background.png" -w 540 -h 380
  inkscape "${SVG_IN}" --export-filename="${OUT_DIR}/background@2x.png" -w 1080 -h 760
else
  echo "Error: No supported SVG renderer found (rsvg-convert, ImageMagick, or inkscape required)." >&2
  exit 1
fi

echo "Generated:"
echo "  - ${OUT_DIR}/background.png (540x380)"
echo "  - ${OUT_DIR}/background@2x.png (1080x760)"

# Check if tiffutil is available for macOS multi-resolution HiDPI TIFF
if command -v tiffutil >/dev/null 2>&1; then
  echo "Generating multi-resolution Retina TIFF via tiffutil..."
  tiffutil -cathidpicheck \
    "${OUT_DIR}/background.png" \
    "${OUT_DIR}/background@2x.png" \
    -out "${OUT_DIR}/background.tiff"
  echo "  - ${OUT_DIR}/background.tiff"
else
  echo "Notice: tiffutil is macOS-only and not available on this host."
  echo "        Run on macOS if a combined .tiff is needed:"
  echo "        tiffutil -cathidpicheck ${OUT_DIR}/background.png ${OUT_DIR}/background@2x.png -out ${OUT_DIR}/background.tiff"
fi

echo "Done."
