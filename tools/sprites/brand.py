#!/usr/bin/env python3
"""Draw the boot-sequence brand art.

- `brand/stamp_ink.png`: the raskell.io mascot as a red hanko-style stamp
  impression: the mascot's dark outline and shapes become ink, the rest is
  left bare, and the edge is roughened like a real stamp on washi
- `brand/stamp_hand.png`: a chunky cartoon hand (white glove, WarioWare-ish)
  gripping a wooden stamp handle, seen from the side, the stamp face at the
  bottom
- `brand/washi.png`: tileable white Japanese paper with soft fibres

Usage:
  python3 tools/sprites/brand.py MASCOT_PNG [OUT_DIR]   # default: assets/sprites/brand

MASCOT_PNG is the raskell.io mascot (raskell-mascot from www.raskell.io,
converted to PNG). Standard library only.
"""

import math
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from remove_bg import read_png, write_png  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
INK = (196, 32, 38)


def stamp_ink(mascot_path, size=256):
    w, h, px = read_png(mascot_path)
    rng = random.Random(7)
    out = []
    for y in range(size):
        for x in range(size):
            sx, sy = int(x * w / size), int(y * h / size)
            r, g, b, a = px[sy * w + sx]
            # A round seal: a solid ring, the mascot carved inside it
            cx, cy = x - size / 2, y - size / 2
            radius = math.hypot(cx, cy) / (size / 2)
            if radius > 0.99:
                out.append((0, 0, 0, 0))
                continue
            if radius > 0.92:
                shade = 0.85 + 0.15 * rng.random()
                out.append(tuple(int(c * shade) for c in INK) + (int(235 * shade),) if rng.random() > 0.08 else (0, 0, 0, 0))
                continue
            if a < 128:
                out.append((0, 0, 0, 0))
                continue
            lum = 0.3 * r + 0.59 * g + 0.11 * b
            # Dark parts (outline, face marks, backpack shadow) and mid tones carry ink;
            # the light fur stays bare paper, like a carved stamp
            inked = lum < 150
            # A worn, uneven impression: sparse gaps, denser at the edge
            cx, cy = x - size / 2, y - size / 2
            edge = math.hypot(cx, cy) / (size / 2)
            gap = rng.random() < (0.015 + 0.12 * max(0.0, edge - 0.75))
            if inked and not gap:
                shade = 0.85 + 0.15 * rng.random()
                out.append(tuple(int(c * shade) for c in INK) + (int(235 * shade),))
            else:
                out.append((0, 0, 0, 0))
    return size, size, out


def hand(width=180, height=300):
    """Glove from above gripping a turned wooden handle, the stamp block below."""
    img = [[(0, 0, 0, 0)] * width for _ in range(height)]
    OUT = (40, 30, 34, 255)
    GLOVE, GLOVE_SHADE = (250, 250, 246, 255), (214, 214, 222, 255)
    WOOD, WOOD_DARK, WOOD_LIGHT = (176, 112, 62, 255), (122, 74, 40, 255), (214, 156, 96, 255)
    RUBBER = (150, 30, 34, 255)

    def ellipse(cx, cy, rx, ry, fill, shade=None):
        for y in range(max(0, int(cy - ry)), min(height, int(cy + ry) + 1)):
            for x in range(max(0, int(cx - rx)), min(width, int(cx + rx) + 1)):
                d = ((x - cx) / rx) ** 2 + ((y - cy) / ry) ** 2
                if d <= 1:
                    lit = (x - cx) / rx + (y - cy) / ry < -0.4
                    img[y][x] = fill if (shade is None or lit or d < 0.55) else shade

    def rect(x0, y0, x1, y1, fill):
        for y in range(max(0, y0), min(height, y1)):
            for x in range(max(0, x0), min(width, x1)):
                img[y][x] = fill

    cx = width // 2
    # Stamp block at the bottom: wood with a red rubber face
    rect(cx - 62, 214, cx + 62, 262, WOOD)
    rect(cx - 62, 214, cx + 62, 222, WOOD_LIGHT)
    rect(cx - 62, 250, cx + 62, 262, WOOD_DARK)
    rect(cx - 58, 262, cx + 58, 276, RUBBER)
    # Turned handle rising from the block
    rect(cx - 16, 140, cx + 16, 214, WOOD)
    rect(cx - 16, 140, cx - 8, 214, WOOD_LIGHT)
    rect(cx + 8, 140, cx + 16, 214, WOOD_DARK)
    ellipse(cx, 196, 24, 10, WOOD, WOOD_DARK)
    ellipse(cx, 120, 30, 26, WOOD, WOOD_DARK)  # the knob
    # Glove wrapped around the knob: a big palm and four fat fingers
    ellipse(cx, 92, 62, 52, GLOVE, GLOVE_SHADE)
    for k, fx in enumerate((-42, -14, 14, 42)):
        ellipse(cx + fx, 136 + (6 if k in (0, 3) else 0), 16, 24, GLOVE, GLOVE_SHADE)
    ellipse(cx + 66, 104, 18, 26, GLOVE, GLOVE_SHADE)  # thumb
    # Cuff
    rect(cx - 52, 26, cx + 52, 52, GLOVE_SHADE)
    rect(cx - 52, 26, cx + 52, 32, GLOVE)
    rect(cx - 40, 0, cx + 40, 28, (238, 198, 160, 255))  # wrist

    # Chunky outline around everything
    filled = [[img[y][x][3] > 0 for x in range(width)] for y in range(height)]
    for y in range(height):
        for x in range(width):
            if not filled[y][x]:
                continue
            for dy in range(-3, 4):
                for dx in range(-3, 4):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < width and 0 <= ny < height and not filled[ny][nx] and dx * dx + dy * dy <= 9:
                        img[ny][nx] = OUT
    # Finger creases
    for fx in (-28, 0, 28):
        for y in range(118, 156):
            x = cx + fx
            img[y][x] = OUT
            img[y][x + 1] = OUT
    return width, height, [p for row in img for p in row]


def washi(size=256):
    rng = random.Random(3)
    base = [[[246, 242, 232] for _ in range(size)] for _ in range(size)]
    # Long soft fibres in random directions, wrapping at the edges (tileable)
    for _ in range(420):
        x, y = rng.uniform(0, size), rng.uniform(0, size)
        angle = rng.uniform(0, math.pi)
        length = rng.uniform(10, 46)
        tone = rng.choice((-14, -9, 8))
        for t in range(int(length)):
            px = int(x + math.cos(angle) * t) % size
            py = int(y + math.sin(angle) * t + math.sin(t / 6) * 1.5) % size
            for c in range(3):
                base[py][px][c] = max(0, min(255, base[py][px][c] + tone))
    # Fine grain
    for y in range(size):
        for x in range(size):
            g = rng.randint(-3, 3)
            base[y][x] = [max(0, min(255, c + g)) for c in base[y][x]]
    return size, size, [tuple(c) + (255,) for row in base for c in [row[x] for x in range(size)] for _ in [0]] if False else \
        [tuple(base[y][x]) + (255,) for y in range(size) for x in range(size)]


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    out = Path(sys.argv[2]) if len(sys.argv) > 2 else REPO / "assets" / "sprites" / "brand"
    out.mkdir(parents=True, exist_ok=True)
    write_png(out / "stamp_ink.png", *stamp_ink(sys.argv[1]))
    write_png(out / "stamp_hand.png", *hand())
    write_png(out / "washi.png", *washi())
    print(f"Wrote stamp_ink, stamp_hand and washi to {out}")


if __name__ == "__main__":
    main()
