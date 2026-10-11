#!/usr/bin/env python3
"""Draw battle effect sprite sheets.

- `effects/poof.png`: a unit defeated. Advance Wars blows units up; woodland
  armies vanish in a puff of dust with a few leaves, in 6 frames of 32x32
  (a flash, a growing cloud, then wisps), played once by the game.
- `effects/heal.png`: a unit healed by a power: green glints and leaves rise
  around it (6 frames).
- `effects/blast.png`: a unit hit by a power: a dark violet burst with sparks
  (6 frames).
- `effects/rockfall.png`: a strike power lands: a boulder drops and shatters
  in dust (8 frames).

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


GLINT = [(40, 120, 60), (120, 220, 110), (220, 255, 190)]
BURST = [(40, 20, 60), (120, 60, 160), (220, 150, 255), (255, 240, 255)]
ROCK = [(70, 66, 74), (120, 118, 128), (170, 168, 176), (214, 212, 220)]


def glint(img, x, y, r, color, core):
    """A small four-pointed sparkle."""
    for d in range(-r, r + 1):
        for cx, cy in ((x + d, y), (x, y + d)):
            if 0 <= cx < SIZE and 0 <= cy < SIZE:
                img[cy][cx] = (core if d == 0 else color) + (255,)


def heal_frame(i):
    rng = random.Random("heal")
    img = canvas()
    # Sparkles and leaves spawn low and drift up, brightest mid-way
    for k in range(7):
        x0 = rng.randrange(6, 26)
        y0 = rng.randrange(20, 30)
        delay = rng.random() * 2
        t = i - delay
        if t < 0 or t > 4.5:
            continue
        y = round(y0 - t * 4)
        r = 2 if 1 <= t <= 3 else 1
        if k % 3 == 0:
            for dx, dy in ((0, 0), (1, 0), (0, 1)):  # a leaf
                if 0 <= y + dy < SIZE:
                    img[y + dy][x0 + dx] = LEAVES[k % 2] + (255,)
        else:
            glint(img, x0, y, r, GLINT[1], GLINT[2])
    return img


def blast_frame(i):
    img = canvas()
    cx, cy = 16, 18
    radius = [3, 7, 10, 11, 9, 6][i]
    for y in range(SIZE):
        for x in range(SIZE):
            d = math.hypot(x + 0.5 - cx, y + 0.5 - cy)
            ring = abs(d - radius)
            if i < 4 and d < radius - 2:
                tone = BURST[2] if d < radius * 0.4 else BURST[1]
                if i >= 2 and d < radius - 4:
                    continue  # the burst hollows out
                img[y][x] = tone + (255,)
            elif ring < 1.2:
                img[y][x] = (BURST[3] if i < 3 else BURST[2]) + (255,)
            elif ring < 2.0 and i < 5:
                img[y][x] = BURST[0] + (255,)
    # Sparks flying out
    for k in range(8):
        a = k * math.pi / 4 + 0.3
        r = radius + 2 + i
        x, y = round(cx + math.cos(a) * r), round(cy + math.sin(a) * r * 0.8)
        if 0 <= x < SIZE and 0 <= y < SIZE and i >= 1:
            img[y][x] = BURST[3] + (255,)
    return img


def rockfall_frame(i):
    img = canvas()
    ground = 26
    if i < 4:
        # The boulder falls, with a speed streak above it
        cy = [2, 9, 16, 22][i]
        for y in range(SIZE):
            for x in range(SIZE):
                d = math.hypot(x + 0.5 - 16, (y + 0.5 - cy) * 1.1)
                if d <= 6:
                    lit = (x - 16) + (y - cy) < -3
                    img[y][x] = (ROCK[3] if lit else ROCK[2] if d < 4 else ROCK[1]) + (255,)
        for y in range(max(0, cy - 12), max(0, cy - 6)):
            for x in (13, 16, 19):
                img[y][x] = (230, 230, 240, 255)
        outline(img)
        for y in range(SIZE):
            for x in range(SIZE):
                if img[y][x][:3] == DUST[0]:
                    img[y][x] = ROCK[0] + (255,)
        return img
    # Impact: shards fly and dust billows
    t = i - 4
    for cx, r in [(9, 4.5 + t), (23, 4.5 + t), (16, 5.5 + t)]:
        puff(img, cx, ground - 2 - t, r * (1.0 if t < 3 else 0.7))
    outline(img)
    for k in range(6):
        a = math.pi + k * math.pi / 5
        x = round(16 + math.cos(a) * (6 + t * 4))
        y = round(ground - 4 + math.sin(a) * (4 + t * 3))
        if 0 <= x < SIZE - 1 and 0 <= y < SIZE - 1:
            for dx, dy in ((0, 0), (1, 0), (0, 1)):
                img[y + dy][x + dx] = ROCK[1] + (255,)
    return img


def write_strip(out, name, frames):
    width = SIZE * len(frames)
    pixels = [frames[x // SIZE][y][x % SIZE] for y in range(SIZE) for x in range(width)]
    write_png(out / f"{name}.png", width, SIZE, pixels)


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO / "assets" / "sprites" / "effects"
    out.mkdir(parents=True, exist_ok=True)
    write_strip(out, "poof", [frame(i) for i in range(6)])
    write_strip(out, "heal", [heal_frame(i) for i in range(6)])
    write_strip(out, "blast", [blast_frame(i) for i in range(6)])
    write_strip(out, "rockfall", [rockfall_frame(i) for i in range(8)])
    print(f"Wrote poof, heal, blast and rockfall to {out}")


if __name__ == "__main__":
    main()
