#!/usr/bin/env python3
"""Generate the Lumen macOS DMG installer background.

Usage:
  python3 make_dmg_background.py \
      --out-dir build/darwin/dmg \
      --font-bold fonts/Inter-Bold.otf --font-regular fonts/Inter-Regular.otf \
      --font-medium fonts/Inter-Medium.otf \
      --app-x 130 --app-y 190 --apps-x 410 --apps-y 190

Writes background.png (540x380) and background@2x.png (1080x760).
Requires: Pillow (pip install pillow). Deterministic, no network, no system fonts needed.
"""
import argparse
import math
import os
import sys

from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

W, H = 540, 380
SS = 4  # supersampling factor for smooth edges

BAND_TOP = (18, 18, 18)        # #121212
BAND_BOTTOM = (27, 24, 64)     # #1b1840
LIGHT = (238, 241, 247)        # #EEF1F7
SUBTITLE = (184, 192, 216)     # #B8C0D8
ARROW = (138, 148, 173)        # #8A94AD
HINT = (91, 101, 128)          # #5B6580
BLADES = [(56, 189, 248), (96, 165, 250), (129, 140, 248), (167, 139, 250)]
CORE = (255, 255, 255)

BLADE_PATH = [(54, 26), ((65, 26), (76, 36), (76, 47)),
              ((76, 52), (70, 54), (63, 54)),
              ((59, 44), (54, 35), (54, 26))]


def bezier(p0, p1, p2, p3, steps=24):
    pts = []
    for i in range(1, steps + 1):
        t = i / steps
        mt = 1 - t
        x = mt**3 * p0[0] + 3 * mt**2 * t * p1[0] + 3 * mt * t**2 * p2[0] + t**3 * p3[0]
        y = mt**3 * p0[1] + 3 * mt**2 * t * p1[1] + 3 * mt * t**2 * p2[1] + t**3 * p3[1]
        pts.append((x, y))
    return pts


def blade_points():
    pts = [BLADE_PATH[0]]
    cur = BLADE_PATH[0]
    for c1, c2, end in BLADE_PATH[1:]:
        pts += bezier(cur, c1, c2, end)
        cur = end
    return pts


def rotate(pt, angle_deg, cx=54, cy=54):
    a = math.radians(angle_deg)
    dx, dy = pt[0] - cx, pt[1] - cy
    return (cx + dx * math.cos(a) - dy * math.sin(a), cy + dx * math.sin(a) + dy * math.cos(a))


def band_edge_y(x):
    """Bottom edge of the dark band: 94 at the sides, 112 in the middle."""
    t = (x - W / 2) / (W / 2)
    return 94 + 18 * (1 - t * t)


def vertical_gradient(size, top, bottom):
    w, h = size
    img = Image.new("RGB", size)
    px = img.load()
    for y in range(h):
        t = y / max(1, h - 1)
        c = tuple(round(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        for x in range(w):
            px[x, y] = c
    return img


def render(scale, a):
    s = scale * SS
    size = (W * s, H * s)
    img = Image.new("RGB", size, LIGHT)

    # --- dark band with curved bottom edge ---
    band = vertical_gradient(size, BAND_TOP, BAND_BOTTOM)
    mask = Image.new("L", size, 0)
    md = ImageDraw.Draw(mask)
    poly = [(0, 0), (W * s, 0)]
    for x in range(W, -1, -2):
        poly.append((x * s, band_edge_y(x) * s))
    poly.append((0, band_edge_y(0) * s))
    md.polygon(poly, fill=255)
    img.paste(band, (0, 0), mask)

    # --- soft glows, clipped to the band ---
    glow = Image.new("RGB", size, (0, 0, 0))
    gd = ImageDraw.Draw(glow)
    gd.ellipse([(60 * s, -30 * s), (260 * s, 90 * s)], fill=(14, 70, 100))
    gd.ellipse([(300 * s, -10 * s), (520 * s, 100 * s)], fill=(60, 40, 110))
    glow = glow.filter(ImageFilter.GaussianBlur(28 * s))
    base = img.copy()
    lit = ImageChops.add(base, glow)
    img.paste(lit, (0, 0), mask)

    d = ImageDraw.Draw(img)

    # --- logo (transparent pinwheel) at x=40, y=20, 64x64 ---
    k = 64 / 108
    ox, oy = 40, 20
    pts = blade_points()
    for color, ang in zip(BLADES, (0, 90, 180, 270)):
        poly = [((ox + rotate(p, ang)[0] * k) * s, (oy + rotate(p, ang)[1] * k) * s) for p in pts]
        d.polygon(poly, fill=color)
    r = 7 * k
    cx, cy = ox + 54 * k, oy + 54 * k
    d.ellipse([((cx - r) * s, (cy - r) * s), ((cx + r) * s, (cy + r) * s)], fill=CORE)

    # --- fonts ---
    fb = ImageFont.truetype(a.font_bold, 38 * s)
    fr = ImageFont.truetype(a.font_regular, 15 * s)
    fm = ImageFont.truetype(a.font_medium, 12 * s)

    # --- title and subtitle (left baseline anchor) ---
    d.text((118 * s, 58 * s), "Lumen", font=fb, fill=(255, 255, 255), anchor="ls")
    d.text((118 * s, 82 * s), "Google Photos uploader", font=fr, fill=SUBTITLE, anchor="ls")

    # --- dashed arrow between the icons ---
    x0, x1, y = a.app_x + 90, a.apps_x - 90, a.app_y
    dash, gap = 6, 5
    x = x0
    while x < x1 - 8:
        d.line([(x * s, y * s), (min(x + dash, x1 - 8) * s, y * s)], fill=ARROW, width=2 * s)
        x += dash + gap
    d.line([((x1 - 8) * s, (y - 6) * s), (x1 * s, y * s)], fill=ARROW, width=2 * s)
    d.line([((x1 - 8) * s, (y + 6) * s), (x1 * s, y * s)], fill=ARROW, width=2 * s)

    # --- instruction text, two short lines so it fits between the icon zones ---
    mid = (a.app_x + a.apps_x) / 2
    d.text((mid * s, (a.app_y + 28) * s), "Drag Lumen to", font=fm, fill=HINT, anchor="ms")
    d.text((mid * s, (a.app_y + 44) * s), "Applications", font=fm, fill=HINT, anchor="ms")

    out = img.resize((W * scale, H * scale), Image.LANCZOS)
    return out


def verify(img, a):
    """Basic self-checks on the 1x image. Returns list of (name, ok)."""
    results = []
    px = img.load()

    def flat(cx, cy, half_w=80, half_h=75, tol=3):
        # safe zone must be a flat LIGHT area (below the band)
        bad = 0
        for yy in range(max(0, cy - half_h), min(H, cy + half_h)):
            for xx in range(max(0, cx - half_w), min(W, cx + half_w)):
                p = px[xx, yy]
                if any(abs(p[i] - LIGHT[i]) > tol for i in range(3)):
                    bad += 1
        return bad

    results.append(("app icon zone clean", flat(a.app_x, a.app_y) == 0))
    results.append(("applications icon zone clean", flat(a.apps_x, a.apps_y) == 0))

    # title / subtitle / logo must produce bright pixels inside the band
    def bright(x0, y0, x1, y1, thresh=200):
        n = 0
        for yy in range(y0, y1):
            for xx in range(x0, x1):
                p = px[xx, yy]
                if sum(p) / 3 > thresh:
                    n += 1
        return n

    results.append(("title visible (white pixels)", bright(118, 26, 300, 64) > 300))
    results.append(("subtitle visible", bright(118, 70, 300, 88, 150) > 120))
    results.append(("logo visible", bright(40, 20, 104, 84, 120) > 200))

    # title right edge inside canvas: measure with the real font
    f = ImageFont.truetype(a.font_bold, 38)
    tb = f.getbbox("Lumen", anchor="ls")
    results.append(("title width >= 90px", (tb[2] - tb[0]) >= 90))
    results.append(("title inside canvas", 118 + tb[2] < W - 20))

    # text between icons must not sit inside either icon zone
    fm = ImageFont.truetype(a.font_medium, 12)
    wmax = max(fm.getlength("Drag Lumen to"), fm.getlength("Applications"))
    mid = (a.app_x + a.apps_x) / 2
    left, right = mid - wmax / 2, mid + wmax / 2
    results.append(("hint text clear of icon zones",
                    left > a.app_x + 80 and right < a.apps_x - 80))
    return results


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out-dir", default=".")
    ap.add_argument("--font-bold", required=True)
    ap.add_argument("--font-regular", required=True)
    ap.add_argument("--font-medium", required=True)
    ap.add_argument("--app-x", type=int, default=130)
    ap.add_argument("--app-y", type=int, default=190)
    ap.add_argument("--apps-x", type=int, default=410)
    ap.add_argument("--apps-y", type=int, default=190)
    a = ap.parse_args()

    os.makedirs(a.out_dir, exist_ok=True)
    one = render(1, a)
    two = render(2, a)
    one.save(os.path.join(a.out_dir, "background.png"))
    two.save(os.path.join(a.out_dir, "background@2x.png"))

    ok_all = True
    for name, ok in verify(one, a):
        print(("PASS  " if ok else "FAIL  ") + name)
        ok_all &= ok
    print(f"sizes: 1x={one.size} 2x={two.size}")
    sys.exit(0 if ok_all else 1)


if __name__ == "__main__":
    main()
