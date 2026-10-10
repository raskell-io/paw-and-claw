#!/usr/bin/env python3
"""Draw the bush (thicket) feature sprite.

The bush is built from overlapping shaded leaf clumps lit from the upper left,
with colored outlines like Dual Strike terrain. It is also the style image for
the generated terrain features (pebble, log, stump, brambles, tall grass and
the buildings), so regenerating it changes the reference for those too: see
assets/sprites/README.md.

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


def bush(ramp=LEAF, seed="bush"):
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
    outline_all(img, darkest([ramp]))
    return ground_on_bottom(img)


FEATURES = {
    "thicket": bush,
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
