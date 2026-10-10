#!/usr/bin/env python3
"""Turn a single map sprite into an Advance Wars style 2-frame idle sheet.

AW map units march in place: the body bobs while the feet stay planted. Frame 1
is the original; frame 2 shifts everything above the feet down by one pixel.
The result is a horizontal strip (2W x H) that the game plays as an animation
(any PNG whose width is a whole multiple of its height is treated as frames).

Use this until hand-drawn or generated idle frames exist; a real sheet in the
same format simply replaces it.

Usage:
  python3 tools/sprites/idle_frames.py IN.png [OUT.png] [--feet 3]

Standard library only.
"""

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from remove_bg import read_png, write_png  # noqa: E402


def bob_frame(width, height, pixels, feet_rows):
    rows = [pixels[y * width:(y + 1) * width] for y in range(height)]
    opaque = [y for y in range(height) if any(p[3] for p in rows[y])]
    if not opaque:
        return list(pixels)
    bottom = opaque[-1]
    split = bottom - feet_rows + 1  # rows from here down are the planted feet
    out = [list(r) for r in rows]
    for y in range(split - 1, -1, -1):
        out[y + 1] = list(rows[y]) if y + 1 < split else merge(rows[y], rows[y + 1])
    out[0] = [(0, 0, 0, 0)] * width
    return [p for r in out for p in r]


def merge(upper, lower):
    """Where the shifted body meets the feet, the body draws over them."""
    return [u if u[3] else l for u, l in zip(upper, lower)]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("input")
    parser.add_argument("output", nargs="?")
    parser.add_argument("--feet", type=int, default=3, help="rows at the bottom that stay planted")
    args = parser.parse_args()

    width, height, pixels = read_png(args.input)
    if width != height:
        raise SystemExit(f"expected a single square frame, got {width}x{height}")
    frames = [pixels, bob_frame(width, height, pixels, args.feet)]
    sheet = [frames[x // width][y * width + x % width] for y in range(height) for x in range(width * len(frames))]
    write_png(args.output or args.input, width * len(frames), height, sheet)
    print(f"Wrote {len(frames)}-frame idle sheet ({width * len(frames)}x{height})")


if __name__ == "__main__":
    main()
