#!/usr/bin/env python3
"""Build every army's map unit sprites from one roster by swapping team colors.

Advance Wars draws all armies with the same unit designs and tells them apart
by color. This does the same: it takes the raw Western roster in
`assets/sprites/originals/units/western/`, repaints the team cloth (the green
pixels) in each army's color from `assets/data/factions.ron`, stands the unit
on the ground and makes its 2-frame idle sheet, writing
`assets/sprites/units/{faction}/{id}.png`.

Western gets the same treatment, so its sage green becomes the same vivid kind
of team color as the others. Brown leather, fur, skin, gold trim and the dark
outline keep their colors.

An army that gets its own generated roster later (its own animals) is left out
by dropping it from --factions; its sprites are then built from its own
originals instead (see assets/sprites/README.md).

Usage:
  python3 tools/sprites/team_colors.py                      # all five armies
  python3 tools/sprites/team_colors.py --factions eastern northern

Standard library only.
"""

import argparse
import colorsys
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from idle_frames import bob_frame  # noqa: E402
from remove_bg import read_png, write_png  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "assets" / "sprites" / "originals" / "units" / "western"
OUT = REPO / "assets" / "sprites" / "units"
FACTIONS = ["western", "northern", "southern", "eastern", "nether"]

# Units that fly hover where they were drawn; the rest stand on this row
AIR = {"ferrier", "skywing", "raptor", "talon"}
NAVAL = {"barge", "frigate", "lurker", "dreadnought"}
GROUND_ROW = 30

# Hue band (degrees) of the team cloth in the source roster: sage to leaf green
TEAM_HUES = (55, 170)


def faction_colors():
    """Each faction's RGB color (0-1) from factions.ron."""
    text = (REPO / "assets" / "data" / "factions.ron").read_text()
    colors = {}
    for key, body in re.findall(r'"(\w+)":\s*\((.*?)\n\s*\),', text, re.S):
        rgb = re.search(r"color:\s*\(([\d.]+),\s*([\d.]+),\s*([\d.]+)\)", body)
        if rgb:
            colors[key] = tuple(float(v) for v in rgb.groups())
    return colors


def is_team(r, g, b):
    h, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
    return TEAM_HUES[0] <= h * 360 <= TEAM_HUES[1] and s > 0.12 and 0.08 < l < 0.92


def recolor(pixel, target):
    """Repaint a team pixel in the target hue, keeping its light-to-dark tone."""
    r, g, b, a = pixel
    if not a or not is_team(r, g, b):
        return pixel
    _, l, s = colorsys.rgb_to_hls(r / 255, g / 255, b / 255)
    th, tl, ts = colorsys.rgb_to_hls(*target)
    # Vivid like an AW army, but shadows stay a little duller than highlights
    sat = min(0.9, max(ts * 0.9, s * 1.6, 0.45) * (0.8 + 0.2 * l))
    # Pull the tones toward the team color's own lightness, so yellow reads as
    # yellow and not olive, while keeping the ramp
    light = min(0.92, l * 0.7 + tl * 0.45)
    nr, ng, nb = colorsys.hls_to_rgb(th, light, sat)
    return (round(nr * 255), round(ng * 255), round(nb * 255), a)


def build(unit_id, faction, target):
    w, h, pixels = read_png(SOURCE / f"{unit_id}.png")
    pixels = [recolor(p, target) for p in pixels]
    rows = [y for y in range(h) if any(pixels[y * w + x][3] for x in range(w))]
    shift = 0 if unit_id in AIR else max(0, GROUND_ROW - rows[-1])
    grounded = [(0, 0, 0, 0)] * (w * h)
    for y in range(h - shift):
        grounded[(y + shift) * w:(y + shift + 1) * w] = pixels[y * w:(y + 1) * w]
    # Walkers bob above planted feet; boats and fliers bob whole
    feet = 0 if unit_id in AIR or unit_id in NAVAL else 3
    frames = [grounded, bob_frame(w, h, grounded, feet)]
    sheet = [frames[x // w][y * w + x % w] for y in range(h) for x in range(w * 2)]
    out = OUT / faction / f"{unit_id}.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    write_png(out, w * 2, h, sheet)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--factions", nargs="+", default=FACTIONS, choices=FACTIONS)
    args = parser.parse_args()
    colors = faction_colors()
    units = sorted(p.stem for p in SOURCE.glob("*.png"))
    for faction in args.factions:
        for unit_id in units:
            build(unit_id, faction, colors[faction])
    print(f"Wrote {len(units)} units for {', '.join(args.factions)}")


if __name__ == "__main__":
    main()
