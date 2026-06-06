#!/usr/bin/env python3
"""Post-process src-tauri/icons/icon.png: remove white matte, emit ICO."""

from __future__ import annotations

import struct
import sys
import zlib
from collections import deque
from pathlib import Path

try:
    from PIL import Image
except ImportError as exc:  # pragma: no cover
    raise SystemExit("Pillow is required: pip install pillow") from exc

ROOT = Path(__file__).resolve().parents[1]
ICON_DIR = ROOT / "src-tauri" / "icons"
SOURCE = ICON_DIR / "icon.png"
OUTPUT_PNG = ICON_DIR / "icon.png"
OUTPUT_ICO = ICON_DIR / "icon.ico"

# Pixels at or above this brightness are treated as background when reached from edges.
WHITE_THRESHOLD = 245
# Slightly darker matte / export fringe.
GRAY_MATTE_THRESHOLD = 230


def is_background_pixel(r: int, g: int, b: int, a: int) -> bool:
    if a <= 8:
        return True

    minimum = min(r, g, b)
    maximum = max(r, g, b)
    spread = maximum - minimum

    if minimum >= WHITE_THRESHOLD:
        return True

    # Near-neutral light gray (common JPEG/PNG export matte).
    if minimum >= GRAY_MATTE_THRESHOLD and spread <= 18:
        return True

    return False


def remove_background(image: Image.Image) -> Image.Image:
    rgba = image.convert("RGBA")
    width, height = rgba.size
    pixels = rgba.load()
    visited = [[False] * width for _ in range(height)]
    queue: deque[tuple[int, int]] = deque()

    def try_enqueue(x: int, y: int) -> None:
        if x < 0 or y < 0 or x >= width or y >= height or visited[y][x]:
            return
        r, g, b, a = pixels[x, y]
        if not is_background_pixel(r, g, b, a):
            return
        visited[y][x] = True
        queue.append((x, y))

    for x in range(width):
        try_enqueue(x, 0)
        try_enqueue(x, height - 1)
    for y in range(height):
        try_enqueue(0, y)
        try_enqueue(width - 1, y)

    while queue:
        x, y = queue.popleft()
        r, g, b, _ = pixels[x, y]
        pixels[x, y] = (r, g, b, 0)
        try_enqueue(x - 1, y)
        try_enqueue(x + 1, y)
        try_enqueue(x - 1, y - 1)
        try_enqueue(x + 1, y - 1)
        try_enqueue(x, y - 1)
        try_enqueue(x, y + 1)

    return rgba


def write_ico(path: Path, png_path: Path) -> None:
    png_data = png_path.read_bytes()
    entry = struct.pack(
        "<BBBBHHII",
        0,
        0,
        0,
        0,
        256,
        256,
        len(png_data),
        6 + 16,
    )
    path.write_bytes(b"\x00\x00\x01\x00\x01\x00" + entry + png_data)


def main() -> int:
    if not SOURCE.exists():
        print(f"Missing source icon: {SOURCE}", file=sys.stderr)
        return 1

    image = Image.open(SOURCE)
    cleaned = remove_background(image)
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    cleaned.save(OUTPUT_PNG, format="PNG", optimize=True)
    write_ico(OUTPUT_ICO, OUTPUT_PNG)

    transparent = sum(1 for _, _, _, a in cleaned.getdata() if a == 0)
    total = cleaned.size[0] * cleaned.size[1]
    print(f"Wrote {OUTPUT_PNG}")
    print(f"Wrote {OUTPUT_ICO}")
    print(f"Transparent pixels: {transparent}/{total} ({100 * transparent / total:.1f}%)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
