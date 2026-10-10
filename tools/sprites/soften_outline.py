#!/usr/bin/env python3
"""Replace a sprite's black outline with darker shades of its own colors.

Advance Wars Dual Strike draws terrain features (forests, mountains, buildings)
with colored outlines, not black ones, so they sit quietly in the map while
units, which do keep dark outlines, stand out. Generators tend to add black
contours; this recolors every near-black pixel to a darkened version of the
dominant neighboring color.

Usage:
  python3 tools/sprites/soften_outline.py IN.png [OUT.png] [--darken 0.55] [--threshold 60]

Standard library only.
"""

import argparse
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from remove_bg import read_png, write_png  # noqa: E402


def is_dark(p, threshold):
    return p[3] > 0 and max(p[:3]) <= threshold


def soften(width, height, pixels, darken, threshold):
    out = list(pixels)
    changed = 0
    for y in range(height):
        for x in range(width):
            p = pixels[y * width + x]
            if not is_dark(p, threshold):
                continue
            # Look outward until we find colored (non-outline) neighbors
            neighbors = Counter()
            for radius in (1, 2, 3):
                for dy in range(-radius, radius + 1):
                    for dx in range(-radius, radius + 1):
                        nx, ny = x + dx, y + dy
                        if 0 <= nx < width and 0 <= ny < height:
                            q = pixels[ny * width + nx]
                            if q[3] > 0 and not is_dark(q, threshold):
                                neighbors[q[:3]] += 1
                if neighbors:
                    break
            if not neighbors:
                continue
            base = neighbors.most_common(1)[0][0]
            out[y * width + x] = tuple(int(c * darken) for c in base) + (p[3],)
            changed += 1
    return out, changed


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input")
    parser.add_argument("output", nargs="?")
    parser.add_argument("--darken", type=float, default=0.55, help="outline brightness relative to the neighbor color")
    parser.add_argument("--threshold", type=int, default=60, help="max channel value that counts as outline black")
    args = parser.parse_args()
    width, height, pixels = read_png(args.input)
    out, changed = soften(width, height, pixels, args.darken, args.threshold)
    write_png(args.output or args.input, width, height, out)
    print(f"{width}x{height}: recolored {changed} outline pixels")


if __name__ == "__main__":
    main()
