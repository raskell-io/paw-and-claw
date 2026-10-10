#!/usr/bin/env python3
"""Remove the little ground platform generators put under terrain features.

Asked for a tree, mountain or building, PixelLab tends to draw it standing on a
small isometric patch of grass and earth. On our flat tiles that reads as a
tile on a tile. This strips it:

- every row at or below --cut is erased (the earth rim of the platform)
- below --grass-from, grass-green pixels on the outside of the silhouette are
  erased, and ones inside it (e.g. moss on a slope) are recolored from the
  nearest object pixel above, so no holes appear
- below --grass-from, only pixels still connected to the object above are kept
- the result is shifted down so the object stands on the bottom edge

Find the rows by eye: `python3 tools/sprites/strip_ground.py IN --preview` prints
the sprite with row numbers.

Usage:
  python3 tools/sprites/strip_ground.py IN.png OUT.png --cut 26 --grass-from 21
  python3 tools/sprites/strip_ground.py IN.png --preview

Standard library only.
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from remove_bg import read_png, write_png  # noqa: E402

CLEAR = (0, 0, 0, 0)


def is_grass(p):
    r, g, b, a = p
    return a > 0 and g > r + 8 and g > b + 8


def preview(width, height, pixels):
    def glyph(p):
        if p[3] == 0:
            return "  "
        if is_grass(p):
            return "gg"
        lum = (p[0] * 3 + p[1] * 6 + p[2]) // 10
        return "##" if lum < 70 else "++" if lum < 140 else ".."
    for y in range(height):
        print(f"{y:2d} " + "".join(glyph(pixels[y * width + x]) for x in range(width)))


def strip(width, height, pixels, cut, grass_from):
    rows = [list(pixels[y * width:(y + 1) * width]) for y in range(height)]
    for y in range(cut, height):
        rows[y] = [CLEAR] * width
    for y in range(grass_from, min(cut, height)):
        for x in range(width):
            p = rows[y][x]
            if not is_grass(p):
                continue
            left = any(rows[y][i][3] and not is_grass(rows[y][i]) for i in range(x))
            right = any(rows[y][i][3] and not is_grass(rows[y][i]) for i in range(x + 1, width))
            above = next((rows[j][x] for j in range(y - 1, -1, -1)
                          if rows[j][x][3] and not is_grass(rows[j][x])), None)
            # Inside the silhouette (object on both sides): fill from above
            rows[y][x] = above if (left and right and above) else CLEAR
    # Below the grass line, keep only what is still attached to the object above
    # (tree trunks, a building's front wall); the platform rim falls away
    reached = set()
    stack = [(x, grass_from) for x in range(width)
             if grass_from > 0 and rows[grass_from - 1][x][3] and grass_from < height and rows[grass_from][x][3]]
    while stack:
        x, y = stack.pop()
        if (x, y) in reached or not (0 <= x < width and grass_from <= y < height) or not rows[y][x][3]:
            continue
        reached.add((x, y))
        stack += [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
    for y in range(grass_from, height):
        for x in range(width):
            if (x, y) not in reached:
                rows[y][x] = CLEAR
    # Drop stray pixels left floating with no opaque neighbor
    for y in range(height):
        for x in range(width):
            if rows[y][x][3] and not any(
                0 <= x + dx < width and 0 <= y + dy < height and rows[y + dy][x + dx][3]
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1))
            ):
                rows[y][x] = CLEAR
    # Stand the object on the bottom edge
    opaque = [y for y in range(height) if any(p[3] for p in rows[y])]
    shift = height - 1 - opaque[-1] if opaque else 0
    rows = [[CLEAR] * width] * shift + rows[:height - shift]
    return [p for r in rows for p in r]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input")
    parser.add_argument("output", nargs="?")
    parser.add_argument("--cut", type=int, help="first row of the earth rim; it and everything below are erased")
    parser.add_argument("--grass-from", type=int, help="first row where grass-green pixels are treated as platform")
    parser.add_argument("--preview", action="store_true", help="print the sprite with row numbers and exit")
    args = parser.parse_args()
    width, height, pixels = read_png(args.input)
    if args.preview:
        preview(width, height, pixels)
        return
    if args.cut is None or args.grass_from is None or not args.output:
        parser.error("OUT, --cut and --grass-from are required (use --preview to find the rows)")
    write_png(args.output, width, height, strip(width, height, pixels, args.cut, args.grass_from))
    print(f"Stripped ground below row {args.grass_from} (cut at {args.cut}) -> {args.output}")


if __name__ == "__main__":
    main()
