#!/usr/bin/env python3
"""Draw battle effect sprite sheets.

- `effects/poof.png`: a unit defeated. Advance Wars blows units up; woodland
  armies vanish in a puff of dust with a few leaves, in 6 frames of 32x32
  (a flash, a growing cloud, then wisps), played once by the game.

Drawn in code like the ground tiles, in the same chunky style with colored
outlines.

Usage:
  python3 tools/sprites/effects.py [OUT_DIR]   # default: assets/sprites/effects

Standard library only.
"""

import math
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from remove_bg import write_png  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
SIZE = 32
CLEAR = (0, 0, 0, 0)

DUST = [(110, 96, 78), (176, 164, 140), (222, 212, 188), (248, 242, 226)]  # outline, shade, base, light
FLASH = [(255, 250, 220), (255, 226, 130)]
LEAVES = [(74, 150, 64), (112, 186, 84), (150, 102, 62)]


def canvas():
    return [[CLEAR] * SIZE for _ in range(SIZE)]


def puff(img, cx, cy, r):
    """A round dust ball lit from the upper left."""
    for y in range(SIZE):
        for x in range(SIZE):
            dx, dy = x + 0.5 - cx, y + 0.5 - cy
            d = math.hypot(dx, dy)
            if d > r:
                continue
            lit = (dx + dy) / max(r, 1) < -0.5
            dark = (dx + dy) / max(r, 1) > 0.6
            img[y][x] = (DUST[3] if lit else DUST[1] if dark else DUST[2]) + (255,)


def outline(img):
    edges = [(x, y) for y in range(SIZE) for x in range(SIZE)
             if img[y][x][3] and any(not (0 <= x + dx < SIZE and 0 <= y + dy < SIZE) or not img[y + dy][x + dx][3]
                                     for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))]
    for x, y in edges:
        if img[y][x][:3] in DUST:
            img[y][x] = DUST[0] + (255,)


def flash(img, cx, cy, r):
    """A four-pointed spark."""
    for y in range(SIZE):
        for x in range(SIZE):
            dx, dy = abs(x + 0.5 - cx), abs(y + 0.5 - cy)
            if dx * dy < r * 0.6 and dx + dy < r * 1.3 or math.hypot(dx, dy) < r * 0.45:
                img[y][x] = (FLASH[0] if math.hypot(dx, dy) < r * 0.35 else FLASH[1]) + (255,)


def leaves(img, rng, count, spread, cy):
    for _ in range(count):
        a = rng.uniform(0, 2 * math.pi)
        x = round(16 + math.cos(a) * spread)
        y = round(cy + math.sin(a) * spread * 0.8)
        color = rng.choice(LEAVES) + (255,)
        for dx, dy in ((0, 0), (1, 0), (0, 1)):
            if 0 <= x + dx < SIZE and 0 <= y + dy < SIZE:
                img[y + dy][x + dx] = color


def frame(i):
    rng = random.Random(f"poof{i}")
    img = canvas()
    base_y = 22  # where the unit stood
    if i == 0:
        flash(img, 16, base_y - 4, 7)
        return img
    # Cloud grows over frames 1-3, then breaks into wisps that drift up
    grow = [0, 0.55, 0.85, 1.0, 1.0, 0.8][i]
    lift = [0, 0, 1, 2, 4, 6][i]
    balls = [(16, base_y - 4, 7), (10, base_y, 5.5), (22, base_y, 5.5), (12, base_y - 9, 5), (21, base_y - 9, 5), (16, base_y + 2, 5)]
    if i >= 4:
        balls = balls[1::2] if i == 4 else balls[3:5]
    for cx, cy, r in balls:
        puff(img, cx + (cx - 16) * 0.15 * i, cy - lift, r * grow)
    outline(img)
    if i == 1:
        flash(img, 16, base_y - 5, 4)
    if 2 <= i <= 4:
        leaves(img, rng, 5, 9 + i * 2, base_y - 5 - lift)
    return img


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO / "assets" / "sprites" / "effects"
    out.mkdir(parents=True, exist_ok=True)
    frames = [frame(i) for i in range(6)]
    width = SIZE * len(frames)
    pixels = [frames[x // SIZE][y][x % SIZE] for y in range(SIZE) for x in range(width)]
    write_png(out / "poof.png", width, SIZE, pixels)
    print(f"Wrote {len(frames)}-frame poof to {out}")


if __name__ == "__main__":
    main()
