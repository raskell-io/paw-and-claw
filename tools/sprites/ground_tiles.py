#!/usr/bin/env python3
"""Draw the ground tiles in the Advance Wars Dual Strike style.

AW keeps ground calm and simple: bright flat plains with a few tufts, sandy
shoals, and water with light ripple lines. Forests, mountains and buildings
are separate feature sprites standing on plain ground. Generators tend to
over-detail ground textures, so these are drawn here instead: deterministic,
palette-locked and seamless.

Coastlines are not baked in: the game composites shore edges onto water tiles
based on their neighbors (see `src/game/map.rs`).

Usage:
  python3 tools/sprites/ground_tiles.py [OUT_DIR]   # default: assets/sprites/terrain/tiles

Standard library only.
"""

import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from remove_bg import write_png  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
SIZE = 32

# Palette (Dual Strike-like: saturated, few tones)
PLAINS = [(124, 200, 90), (110, 184, 78), (152, 220, 110)]       # base, shade, highlight
TALL = [(104, 178, 72), (88, 158, 62), (140, 206, 96)]
FOREST_FLOOR = [(96, 164, 70), (82, 146, 60), (126, 190, 88)]
BRAMBLE = [(118, 150, 70), (98, 128, 58), (146, 176, 88)]
DIRT = [(214, 182, 118), (190, 156, 96), (232, 206, 146)]
DARK_EARTH = [(150, 120, 80), (126, 98, 64), (176, 146, 100)]
SAND = [(236, 214, 140), (214, 190, 116), (248, 232, 176)]
RIVER = [(84, 160, 232), (64, 136, 212), (170, 214, 248)]
SEA = [(58, 120, 214), (46, 100, 190), (140, 190, 244)]
FLAGSTONE = [(200, 190, 170), (168, 156, 136), (224, 216, 200)]


def blank(color):
    return [[color] * SIZE for _ in range(SIZE)]


def put(img, x, y, color):
    img[y % SIZE][x % SIZE] = color  # wraps, so tiles stay seamless


def speckle(img, rng, color, count):
    for _ in range(count):
        put(img, rng.randrange(SIZE), rng.randrange(SIZE), color)


def tufts(img, rng, palette, count):
    """Little 3-pixel grass tufts: a light top over a darker base."""
    _, shade, light = palette
    for _ in range(count):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        put(img, x, y, light)
        put(img, x - 1, y + 1, light)
        put(img, x + 1, y + 1, light)
        put(img, x, y + 1, shade)


def ripples(img, rng, palette, count):
    """Short horizontal highlight dashes, like AW's water shimmer."""
    _, shade, light = palette
    for _ in range(count):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        length = rng.choice((3, 4, 5))
        for i in range(length):
            put(img, x + i, y, light)
        put(img, x + 1, y + 1, shade)


def stones(img, rng, palette, count):
    base, shade, light = palette
    for _ in range(count):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        put(img, x, y, light)
        put(img, x + 1, y, base)
        put(img, x, y + 1, shade)
        put(img, x + 1, y + 1, shade)


def flagstones(img, palette):
    base, shade, light = palette
    for y in range(SIZE):
        for x in range(SIZE):
            row = y // 8
            offset = 4 if row % 2 else 0
            if y % 8 == 0 or (x + offset) % 8 == 0:
                img[y][x] = shade
            elif y % 8 == 1:
                img[y][x] = light


def tile(name):
    rng = random.Random(name)  # stable per terrain
    if name == "grass":
        img = blank(PLAINS[0]); speckle(img, rng, PLAINS[1], 18); tufts(img, rng, PLAINS, 4)
    elif name == "tall_grass":
        img = blank(TALL[0]); speckle(img, rng, TALL[1], 26); tufts(img, rng, TALL, 9)
    elif name == "thicket":
        img = blank(FOREST_FLOOR[0]); speckle(img, rng, FOREST_FLOOR[1], 30); tufts(img, rng, FOREST_FLOOR, 5)
    elif name == "brambles":
        img = blank(BRAMBLE[0]); speckle(img, rng, BRAMBLE[1], 30); tufts(img, rng, BRAMBLE, 7)
    elif name == "log":
        img = blank(DIRT[0]); speckle(img, rng, DIRT[1], 24); stones(img, rng, DIRT, 3)
    elif name == "hollow":
        img = blank(DARK_EARTH[0]); speckle(img, rng, DARK_EARTH[1], 28); stones(img, rng, DARK_EARTH, 3)
    elif name in ("boulder", "outpost", "storehouse"):
        # Features stand on plain ground, as in AW
        img = blank(PLAINS[0]); speckle(img, rng, PLAINS[1], 18); tufts(img, rng, PLAINS, 3)
    elif name == "base":
        img = blank(FLAGSTONE[0]); flagstones(img, FLAGSTONE)
    elif name == "shore":
        img = blank(SAND[0]); speckle(img, rng, SAND[1], 26); speckle(img, rng, SAND[2], 14)
    elif name == "creek":
        img = blank(RIVER[0]); speckle(img, rng, RIVER[1], 20); ripples(img, rng, RIVER, 5)
    elif name == "pond":
        img = blank(SEA[0]); speckle(img, rng, SEA[1], 24); ripples(img, rng, SEA, 6)
    else:
        raise SystemExit(f"unknown terrain {name}")
    return img


TERRAIN = ["grass", "tall_grass", "thicket", "brambles", "log", "boulder", "hollow",
           "creek", "pond", "shore", "base", "outpost", "storehouse"]


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO / "assets" / "sprites" / "terrain" / "tiles"
    out.mkdir(parents=True, exist_ok=True)
    for name in TERRAIN:
        img = tile(name)
        write_png(out / f"{name}.png", SIZE, SIZE, [c + (255,) for row in img for c in row])
    print(f"Wrote {len(TERRAIN)} ground tiles to {out}")


if __name__ == "__main__":
    main()
