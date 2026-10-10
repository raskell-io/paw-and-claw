#!/usr/bin/env python3
"""Draw the terrain features at woodland-animal scale.

The map is the forest floor seen by small animals: bushes, brambles, pebbles,
fallen twigs and stumps rather than forests and mountains. These are built from
simple shapes, each shaded as a little sphere lit from the upper left, using
color ramps that sit next to the ground palette in `ground_tiles.py`, with
colored outlines (each shape's own darkest shade) like Dual Strike terrain.
Generators struggle to keep such simple objects consistent; drawing them here
keeps the whole floor in one palette.

The small outbuildings (twig lookout, acorn granary) are drawn the same way so
they share the palette; the base and the units are generated (see
assets/sprites/README.md).

Usage:
  python3 tools/sprites/nature_features.py [OUT_DIR]   # default: assets/sprites/terrain/features

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

# Ramps: outline, shadow, base, light, highlight (light comes from the upper left)
# Foliage is a deeper green than the plains so bushes read against the grass
LEAF = [(28, 74, 46), (48, 114, 56), (74, 150, 64), (112, 186, 84), (166, 220, 124)]
BRAMBLE_LEAF = [(44, 76, 44), (72, 110, 58), (100, 142, 68), (136, 172, 86), (176, 204, 120)]
STONE = [(72, 76, 92), (118, 122, 138), (158, 162, 172), (198, 200, 206), (232, 234, 238)]
MOSS = [(52, 104, 52), (84, 150, 66), (118, 186, 82), (160, 218, 110), (200, 240, 150)]
BARK = [(72, 44, 30), (112, 72, 46), (150, 102, 62), (186, 134, 84), (214, 170, 116)]
HEARTWOOD = [(120, 84, 52), (176, 132, 84), (210, 170, 112), (232, 200, 146), (246, 226, 182)]
BERRY = [(96, 20, 40), (150, 30, 56), (200, 50, 70), (236, 96, 110), (252, 170, 176)]
CAP = [(110, 30, 26), (170, 46, 36), (214, 70, 52), (240, 120, 96), (252, 200, 180)]
ACORN = [(98, 56, 30), (150, 90, 46), (192, 128, 68), (222, 166, 100), (242, 204, 148)]
ACORN_CAP = [(64, 42, 28), (98, 66, 42), (130, 94, 60), (162, 124, 82), (192, 156, 110)]
GRASS_BLADE = [(52, 110, 50), (76, 146, 60), (104, 178, 72), (140, 206, 96), (180, 230, 130)]
BURROW = (34, 24, 20, 255)

LIGHT = (-0.55, -0.75)


def canvas():
    return [[CLEAR] * SIZE for _ in range(SIZE)]


def shade(ramp, nx, ny, nz):
    """Pick a ramp tone for a surface normal: lambert term, quantized."""
    lx, ly = LIGHT
    lz = math.sqrt(max(0.0, 1 - lx * lx - ly * ly))
    lum = nx * lx + ny * ly + nz * lz
    if lum > 0.82:
        return ramp[4]
    if lum > 0.45:
        return ramp[3]
    if lum > 0.05:
        return ramp[2]
    return ramp[1]


def blob(img, cx, cy, rx, ry, ramp, outline=True, wobble=0.0, seed=0):
    """A shaded ellipsoid drawn over what's there. `wobble` roughens the edge.

    Where it overlaps earlier shapes, only its lower rim gets a line, in the
    ramp's shadow tone: overlapping lobes read as soft folds, not hard rings.
    The outer silhouette is closed later by `outline_all`.
    """
    phase = (seed * 2.399) % (2 * math.pi)
    inside = set()
    for y in range(SIZE):
        for x in range(SIZE):
            nx, ny = (x + 0.5 - cx) / rx, (y + 0.5 - cy) / ry
            if wobble:
                theta = math.atan2(ny, nx)
                edge = 1 + wobble * (math.sin(5 * theta + phase) * 0.6 + math.sin(9 * theta + 2 * phase) * 0.4)
            else:
                edge = 1.0
            d = (nx * nx + ny * ny) / (edge * edge)
            if d <= 1.0:
                inside.add((x, y))
                img[y][x] = shade(ramp, nx, ny, math.sqrt(max(0.0, 1 - d))) + (255,)
    if outline:
        for x, y in inside:
            lower = (y + 0.5 - cy) / ry > 0.15
            if lower and any((x + dx, y + dy) not in inside for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))):
                img[y][x] = ramp[1] + (255,)
    return inside


def texture(img, ramp, area, rng, count, leaf=True):
    """Scatter small flecks: highlights on lit tones, shadows on dark ones."""
    cells = sorted(area)
    for _ in range(count):
        x, y = rng.choice(cells)
        tone = img[y][x][:3]
        if tone == ramp[0]:
            continue
        lit = ramp.index(tone) >= 3 if tone in ramp else False
        color = (ramp[4] if lit else ramp[1]) + (255,)
        img[y][x] = color
        if leaf and (x + 1, y - 1) in area and img[y - 1][x + 1][:3] != ramp[0]:
            img[y - 1][x + 1] = color


def outline_all(img, color_of):
    """Close the silhouette: opaque pixels touching transparency get an outline."""
    edges = []
    for y in range(SIZE):
        for x in range(SIZE):
            if img[y][x][3] and any(
                not (0 <= x + dx < SIZE and 0 <= y + dy < SIZE) or img[y + dy][x + dx][3] == 0
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))
            ):
                edges.append((x, y))
    for x, y in edges:
        img[y][x] = color_of(img[y][x]) + (255,)


def darkest(ramps):
    """Map any tone to the outline color of the ramp it belongs to."""
    lookup = {tone: ramp[0] for ramp in ramps for tone in ramp}
    return lambda p: lookup.get(p[:3], p[:3])


def ground_on_bottom(img):
    """Shift the drawing down so it stands on the bottom row."""
    rows = [y for y in range(SIZE) if any(p[3] for p in img[y])]
    shift = SIZE - 1 - rows[-1] if rows else 0
    return [[CLEAR] * SIZE for _ in range(shift)] + img[:SIZE - shift]


def bush(ramp=LEAF, berries=False, seed="bush"):
    rng = random.Random(seed)
    img = canvas()
    # Back to front: clumps lower and nearer are drawn last
    clumps = [(10, 17, 7.5, 6.5), (22, 17, 7.5, 6.5), (16, 13, 8.5, 7.5), (9, 22, 7, 5.5), (23, 22, 7, 5.5), (16, 21, 8.5, 6.5)]
    area = set()
    for i, (cx, cy, rx, ry) in enumerate(clumps):
        area |= blob(img, cx, cy, rx, ry, ramp, wobble=0.09, seed=i + len(seed))
    texture(img, ramp, area, rng, 34)
    # A few leaves poking out of the top
    for x, y in [(12, 5), (19, 6), (26, 10), (5, 12)]:
        img[y][x] = ramp[3] + (255,)
        img[y + 1][x] = ramp[2] + (255,)
    if berries:
        for _ in range(9):
            x, y = rng.randrange(6, 26), rng.randrange(10, 25)
            if img[y][x][3] and img[y][x][:3] != ramp[0]:
                img[y][x] = BERRY[2] + (255,)
                if img[y - 1][x][3]:
                    img[y - 1][x] = BERRY[4] + (255,)
        # Thorns: dark ticks on the silhouette
        for x, y in [(4, 18), (28, 19), (16, 6), (7, 11), (25, 12)]:
            if 0 <= y < SIZE:
                img[y][x] = ramp[0] + (255,)
    outline_all(img, darkest([ramp, BERRY]))
    return ground_on_bottom(img)


def pebble():
    img = canvas()
    rng = random.Random("pebble")
    area = blob(img, 16, 21, 12, 8.5, STONE, wobble=0.04, seed=3)
    area |= blob(img, 23, 23, 6, 5, STONE, wobble=0.05, seed=7)  # a smaller stone leaning on it
    texture(img, STONE, area, rng, 10, leaf=False)
    # Moss cap on top
    for y in range(SIZE):
        for x in range(SIZE):
            nx, ny = (x + 0.5 - 14) / 8, (y + 0.5 - 15.5) / 3.2
            if nx * nx + ny * ny <= 1.0 and img[y][x][3]:
                img[y][x] = shade(MOSS, nx, ny - 0.6, 0.6) + (255,)
    outline_all(img, darkest([STONE, MOSS]))
    return ground_on_bottom(img)


def twig():
    img = canvas()
    # A fallen log lying sideways: a capsule with a cut end on the right
    for y in range(SIZE):
        for x in range(3, 26):
            ny = (y + 0.5 - 22) / 5.5
            if abs(ny) <= 1:
                img[y][x] = shade(BARK, 0, ny, math.sqrt(1 - ny * ny)) + (255,)
    for y in range(17, 28):
        if img[y][3][3]:
            img[y][3] = BARK[0] + (255,)
    blob(img, 26, 22, 3.5, 5.5, HEARTWOOD)
    blob(img, 26, 22, 1.6, 2.6, HEARTWOOD, outline=False)
    # Bark lines and a little mushroom
    for x in (8, 14, 20):
        for y in range(19, 25):
            if img[y][x][3] and (x + y) % 3:
                img[y][x] = BARK[1] + (255,)
    blob(img, 11, 15, 4, 2.6, CAP)
    for y in range(17, 18):
        img[y][11] = HEARTWOOD[3] + (255,)
    img[14][10] = CAP[4] + (255,)
    img[15][13] = CAP[4] + (255,)
    outline_all(img, darkest([BARK, HEARTWOOD, CAP]))
    return ground_on_bottom(img)


def stump():
    img = canvas()
    # Trunk: a vertical cylinder
    for y in range(12, 31):
        for x in range(7, 26):
            nx = (x + 0.5 - 16.5) / 9.5
            if abs(nx) <= 1:
                img[y][x] = shade(BARK, nx, 0.1, math.sqrt(1 - nx * nx)) + (255,)
    # Cut top with rings
    blob(img, 16.5, 12, 9.5, 4, HEARTWOOD)
    for r in (5.5, 2.5):
        for y in range(SIZE):
            for x in range(SIZE):
                nx, ny = (x + 0.5 - 16.5) / r, (y + 0.5 - 12) / (r * 0.42)
                if 0.75 <= nx * nx + ny * ny <= 1.0:
                    img[y][x] = HEARTWOOD[1] + (255,)
    # Burrow entrance at the base
    for y in range(SIZE):
        for x in range(SIZE):
            nx, ny = (x + 0.5 - 15) / 4.5, (y + 0.5 - 29) / 5
            if nx * nx + ny * ny <= 1.0 and y <= 30:
                img[y][x] = BURROW
    # Bark streaks
    rng = random.Random("stump")
    for x in range(8, 25, 3):
        top = rng.randrange(15, 19)
        for y in range(top, top + rng.randrange(5, 10)):
            if img[y][x][3] and img[y][x] != BURROW:
                img[y][x] = BARK[1] + (255,)
    # Roots
    for x, y in [(5, 30), (6, 29), (27, 30), (26, 29)]:
        img[y][x] = BARK[2] + (255,)
    outline_all(img, darkest([BARK, HEARTWOOD]))
    return ground_on_bottom(img)


def line(img, x0, y0, x1, y1, color, width=1):
    """A straight stroke between two points."""
    steps = max(abs(x1 - x0), abs(y1 - y0), 1)
    for i in range(steps + 1):
        x = round(x0 + (x1 - x0) * i / steps)
        y = round(y0 + (y1 - y0) * i / steps)
        for w in range(width):
            if 0 <= x + w < SIZE and 0 <= y < SIZE:
                img[y][x + w] = color + (255,)


def tall_grass():
    """Tufts of long tapered blades fanning out, lit tips over a dark base."""
    img = canvas()
    # (base x, tip x, height) per blade, back to front
    blades = [
        (6, 2, 14), (8, 9, 18), (10, 15, 13), (14, 11, 22), (16, 17, 25), (18, 23, 19),
        (22, 20, 15), (24, 27, 17), (26, 30, 11), (12, 7, 10), (20, 15, 11), (17, 13, 15),
    ]
    for base, tip, height in blades:
        top = 31 - height
        for y in range(top, 32):
            t = (31 - y) / height              # 0 at the ground, 1 at the tip
            center = base + (tip - base) * t * t
            half = 1.6 * (1 - t) + 0.3
            for x in range(SIZE):
                dx = x + 0.5 - center
                if abs(dx) <= half:
                    tone = 1 if t < 0.25 else 2 if t < 0.6 else 3
                    if dx < 0 and tone < 4:
                        tone += 1                  # the side facing the light
                    img[y][x] = GRASS_BLADE[tone] + (255,)
    outline_all(img, darkest([GRASS_BLADE]))
    return ground_on_bottom(img)


def lookout():
    """A little lookout tower lashed together from twigs, with a leaf roof."""
    img = canvas()
    # Legs, splayed, and a cross brace
    line(img, 8, 31, 11, 13, BARK[1], 2)
    line(img, 23, 31, 20, 13, BARK[1], 2)
    line(img, 9, 30, 11, 13, BARK[3])
    line(img, 10, 28, 21, 19, BARK[2])
    line(img, 10, 19, 21, 28, BARK[2])
    # Platform
    for x in range(7, 26):
        img[13][x] = HEARTWOOD[2] + (255,)
        img[14][x] = BARK[1] + (255,)
    for x in range(7, 26, 3):
        img[13][x] = HEARTWOOD[1] + (255,)
    # Hut with a window
    for y in range(7, 13):
        for x in range(11, 22):
            img[y][x] = (BARK[3] if x < 13 else BARK[2]) + (255,)
    for y in range(9, 11):
        for x in range(15, 18):
            img[y][x] = BURROW
    # Leaf roof: a shaded triangle with a ragged eave
    for y in range(0, 8):
        half = 2 + y * 1.25
        for x in range(SIZE):
            dx = x + 0.5 - 16.5
            if abs(dx) <= half:
                nx = dx / max(half, 1)
                img[y][x] = shade(LEAF, nx * 0.8, -0.4 + y * 0.1, 0.6) + (255,)
    for x in range(8, 26, 2):
        img[8][x] = LEAF[1] + (255,)
    outline_all(img, darkest([BARK, HEARTWOOD, LEAF]))
    return ground_on_bottom(img)


def granary():
    """An acorn-shaped granary with a round door and seeds piled by it."""
    img = canvas()
    rng = random.Random("granary")
    area = blob(img, 16, 21, 10, 9.5, ACORN)
    texture(img, ACORN, area, rng, 8, leaf=False)
    cap = blob(img, 16, 12.5, 12, 5, ACORN_CAP, wobble=0.05, seed=4)
    # Cap scales: a crosshatch in the shadow tone
    for x, y in cap:
        if (x + y) % 3 == 0 and (x - y) % 4 == 0 and img[y][x][:3] != ACORN_CAP[0]:
            img[y][x] = ACORN_CAP[1] + (255,)
    for y in range(4, 8):
        img[y][16] = ACORN_CAP[2] + (255,)
        img[y][17] = ACORN_CAP[1] + (255,)
    # Door with a wooden frame
    blob(img, 16, 28.5, 4.5, 5.5, HEARTWOOD, outline=False)
    for y in range(SIZE):
        for x in range(SIZE):
            nx, ny = (x + 0.5 - 16) / 3, (y + 0.5 - 29) / 4.2
            if nx * nx + ny * ny <= 1.0:
                img[y][x] = BURROW
    # Seeds by the door
    blob(img, 7, 29, 2.5, 2, HEARTWOOD)
    blob(img, 25, 29.5, 2.5, 2, HEARTWOOD)
    blob(img, 27, 27.5, 2, 1.6, HEARTWOOD)
    outline_all(img, darkest([ACORN, ACORN_CAP, HEARTWOOD]))
    return ground_on_bottom(img)


FEATURES = {
    "thicket": lambda: bush(),
    "brambles": lambda: bush(BRAMBLE_LEAF, berries=True, seed="brambles"),
    "boulder": pebble,
    "log": twig,
    "hollow": stump,
    "tall_grass": tall_grass,
    "outpost": lookout,
    "storehouse": granary,
}


def main():
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else REPO / "assets" / "sprites" / "terrain" / "features"
    out.mkdir(parents=True, exist_ok=True)
    for name, draw in FEATURES.items():
        img = draw()
        write_png(out / f"{name}.png", SIZE, SIZE, [p for row in img for p in row])
    print(f"Wrote {len(FEATURES)} nature features to {out}")


if __name__ == "__main__":
    main()
