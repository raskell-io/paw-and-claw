#!/usr/bin/env python3
"""Draw the ground tiles in the Advance Wars Dual Strike style.

AW keeps ground calm and simple: bright flat plains with a few tufts, sandy
shoals, and water with light ripple lines. Forests, mountains and buildings
are separate feature sprites standing on plain ground. Generators tend to
over-detail ground textures, so these are drawn here instead: deterministic,
palette-locked and seamless.

A tile PNG may be a horizontal strip of 32x32 variants. The game picks one
per tile (see `src/game/map.rs`):

- most ground tiles have a few variants (flowers, clover) picked by position,
  so a field of grass doesn't repeat one stamp
- `log.png` is the trail atlas: 16 columns, one per combination of trail
  neighbors (bit 1 north, 2 east, 4 south, 8 west), so paths join up, then two
  plank-bridge columns (16 east-west, 17 north-south) for trail over water
- `shore.png` is the bank atlas, indexed the same way by which sides face
  water: sand runs along those sides and fades into grass inland

Coastlines are not baked in: the game composites shore edges onto water tiles
based on their neighbors.

Usage:
  python3 tools/sprites/ground_tiles.py [OUT_DIR]   # default: assets/sprites/terrain/tiles

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

# Palette (Dual Strike-like: saturated, few tones)
PLAINS = [(124, 200, 90), (110, 184, 78), (152, 220, 110)]       # base, shade, highlight
SAND = [(236, 214, 140), (214, 190, 116), (248, 232, 176)]
RIVER = [(84, 160, 232), (64, 136, 212), (170, 214, 248)]
SEA = [(58, 120, 214), (46, 100, 190), (140, 190, 244)]
FLAGSTONE = [(200, 190, 170), (168, 156, 136), (224, 216, 200)]
TRAIL = [(206, 168, 108), (178, 140, 86), (226, 194, 138)]
TRAIL_EDGE = (150, 132, 70)  # where trodden dirt meets the grass
PETALS = [(250, 248, 236), (252, 220, 96), (236, 160, 196)]
CLOVER = (96, 170, 72)
PAD = [(46, 120, 62), (78, 160, 74), (124, 196, 96)]  # rim, leaf, highlight
PLANK = [(150, 102, 62), (186, 134, 84), (214, 170, 116)]
RAIL = (112, 72, 46)

VARIANTS = 4


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


def flowers(img, rng, count):
    """Tiny four-petal flowers with a bright center, all of one kind per tile."""
    petal = rng.choice(PETALS)
    for _ in range(count):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        for dx, dy in ((0, -1), (1, 0), (0, 1), (-1, 0)):
            put(img, x + dx, y + dy, petal)
        put(img, x, y, PETALS[1] if petal != PETALS[1] else PETALS[0])


def clover(img, rng, count):
    """Small darker leaf patches."""
    for _ in range(count):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        for dx, dy in ((0, 0), (1, 0), (0, 1), (1, 1), (2, 1), (1, 2)):
            if rng.random() < 0.85:
                put(img, x + dx, y + dy, CLOVER)


def meadow(rng, variant, palette=PLAINS, tuft_count=4):
    """Plain grass; variants add clover, then flowers, so fields look picked, not stamped."""
    img = blank(palette[0])
    speckle(img, rng, palette[1], 18)
    tufts(img, rng, palette, tuft_count)
    if variant == 1:
        clover(img, rng, 3)
    elif variant == 2:
        flowers(img, rng, 3)
    elif variant == 3:
        clover(img, rng, 1)
        flowers(img, rng, 1)
    return img


def trail(rng, mask):
    """Grass with a trodden dirt path running from the middle to each connected side.

    The path is 12px wide where it leaves the tile, so neighbors meet; the middle
    is a round patch, so bends and junctions come out soft.
    """
    img = meadow(rng, 0, tuft_count=2)
    half = 6
    center = SIZE / 2 - 0.5
    arms = [(0, -1, 1), (1, 0, 2), (0, 1, 4), (-1, 0, 8)]

    def on_path(x, y):
        dx, dy = x - center, y - center
        if dx * dx + dy * dy < half * half:
            return True
        for ax, ay, bit in arms:
            if not mask & bit:
                continue
            along = dx * ax + dy * ay   # distance out from the center along the arm
            across = abs(dx * ay - dy * ax)
            if along >= 0 and across < half:
                return True
        return False

    path = {(x, y) for y in range(SIZE) for x in range(SIZE) if on_path(x, y)}
    for x, y in path:
        img[y][x] = TRAIL[0]
    for x, y in path:
        neighbors = [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
        inside = [(nx, ny) for nx, ny in neighbors if 0 <= nx < SIZE and 0 <= ny < SIZE]
        if any(n not in path for n in inside):
            img[y][x] = TRAIL_EDGE
    # Wear: lighter packed earth, darker grit and the odd pebble
    cells = sorted(path)
    for _ in range(14):
        x, y = rng.choice(cells)
        if img[y][x] == TRAIL[0]:
            img[y][x] = TRAIL[1]
    for _ in range(8):
        x, y = rng.choice(cells)
        if img[y][x] == TRAIL[0]:
            img[y][x] = TRAIL[2]
    return img


def bank(rng, mask):
    """Grass with a sandy bank along each side that faces water (see the module docs).

    The band's wobble repeats a whole number of times along the edge, so it
    meets the next bank tile at the same depth.
    """
    img = meadow(rng, 0, tuft_count=2)
    edges = [(1, lambda x, y: (y, x)), (2, lambda x, y: (SIZE - 1 - x, y)),
             (4, lambda x, y: (SIZE - 1 - y, x)), (8, lambda x, y: (x, y))]

    def depth(x, y):
        best = None
        for bit, measure in edges:
            if mask & bit:
                d, along = measure(x, y)
                limit = 6 + 1.2 * math.sin(2 * math.pi * along / SIZE)
                best = max(best or -99, limit - d)
        return best

    for y in range(SIZE):
        for x in range(SIZE):
            d = depth(x, y)
            if d is None or d < 0:
                continue
            img[y][x] = SAND[1] if d < 1 else SAND[0]
    if mask:
        for _ in range(14):
            x, y = rng.randrange(SIZE), rng.randrange(SIZE)
            if img[y][x] == SAND[0]:
                img[y][x] = SAND[2] if rng.random() < 0.6 else SAND[1]
    else:
        speckle(img, rng, SAND[0], 10)
    return img


def lily_pads(rng):
    """Deep water with a few round pads (each with its notch) and one water lily."""
    img = blank(SEA[0]); speckle(img, rng, SEA[1], 20); ripples(img, rng, SEA, 3)
    for cx, cy, r in [(9, 10, 4.5), (21, 8, 3.5), (20, 21, 5), (8, 23, 3)]:
        notch = rng.uniform(0, 6.28)
        for y in range(SIZE):
            for x in range(SIZE):
                dx, dy = x + 0.5 - cx, y + 0.5 - cy
                d = math.hypot(dx, dy)
                if d > r:
                    continue
                angle = math.atan2(dy, dx) - notch
                if abs(math.atan2(math.sin(angle), math.cos(angle))) < 0.35 and d > 0.8:
                    continue  # the wedge cut into every lily pad
                lit = dx + dy < -r * 0.6
                img[y][x] = PAD[0] if d > r - 1 else PAD[2] if lit else PAD[1]
    for dx, dy in ((0, -1), (1, 0), (0, 1), (-1, 0)):
        img[20 + dy][20 + dx] = PETALS[2]
    img[20][20] = PETALS[1]
    return img


def bridge(rng, north_south):
    """Plank bridge over deep water, boards across the way of travel, log rails each side."""
    img = blank(SEA[0]); speckle(img, rng, SEA[1], 20); ripples(img, rng, SEA, 4)
    for y in range(SIZE):
        for x in range(SIZE):
            along, across = (y, x) if north_south else (x, y)
            if not 8 <= across <= 24:
                continue
            if across in (8, 23):
                color = RAIL
            elif across == 24:
                color = SEA[1]  # the deck's shadow on the water
            elif along % 4 == 3:
                color = PLANK[0]  # gaps between boards
            else:
                color = PLANK[2] if across == 9 else PLANK[1]
            img[y][x] = color
    return img


def tile(name, variant=0):
    rng = random.Random(f"{name}{variant}")  # stable per terrain and variant
    if name == "grass":
        img = meadow(rng, variant)
    elif name == "log":
        img = trail(rng, variant) if variant < 16 else bridge(rng, north_south=variant == 17)
    elif name in ("tall_grass", "thicket", "brambles", "boulder", "hollow", "workshop", "outpost", "storehouse"):
        # Features stand on plain ground, as in AW: the sprite says what's there
        img = meadow(rng, variant, tuft_count=3)
    elif name == "base":
        img = blank(FLAGSTONE[0]); flagstones(img, FLAGSTONE)
    elif name == "shore":
        img = bank(rng, variant)
    elif name == "creek":
        img = blank(RIVER[0]); speckle(img, rng, RIVER[1], 20); ripples(img, rng, RIVER, 5)
    elif name == "pond":
        img = blank(SEA[0]); speckle(img, rng, SEA[1], 24); ripples(img, rng, SEA, 6)
    elif name == "lily_pads":
        img = lily_pads(rng)
    else:
        raise SystemExit(f"unknown terrain {name}")
    return img


def columns(name):
    """How many variants a tile's strip has (see the module docs)."""
    if name == "log":
        return 18
    if name == "shore":
        return 16
    if name in ("grass", "tall_grass", "thicket", "brambles", "boulder", "hollow", "workshop", "outpost", "storehouse"):
        return VARIANTS
    return 1


TERRAIN = ["grass", "tall_grass", "thicket", "brambles", "log", "boulder", "hollow",
           "creek", "pond", "lily_pads", "shore", "base", "workshop", "outpost", "storehouse"]


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO / "assets" / "sprites" / "terrain" / "tiles"
    out.mkdir(parents=True, exist_ok=True)
    for name in TERRAIN:
        variants = [tile(name, v) for v in range(columns(name))]
        width = SIZE * len(variants)
        pixels = [variants[x // SIZE][y][x % SIZE] + (255,) for y in range(SIZE) for x in range(width)]
        write_png(out / f"{name}.png", width, SIZE, pixels)
    print(f"Wrote {len(TERRAIN)} ground tiles to {out}")


if __name__ == "__main__":
    main()
