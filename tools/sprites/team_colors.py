#!/usr/bin/env python3
"""Build every army's map unit sprites from one roster by swapping team colors.

Advance Wars draws all armies with the same unit designs and tells them apart
by color. This does the same: it takes the raw Western roster in
`assets/sprites/originals/units/western/`, repaints the team cloth (the green
pixels) in each army's color from `assets/data/factions.ron`, stands the unit
on the ground, centers it on its tile and makes its 2-frame idle sheet, writing
`assets/sprites/units/{faction}/{id}.png`.

Western gets the same treatment, so its sage green becomes the same vivid kind
of team color as the others. Brown leather, fur, skin, gold trim and the dark
outline keep their colors.

An army that gets its own generated roster later (its own animals) is left out
by dropping it from --factions; its sprites are then built from its own
originals instead (see assets/sprites/README.md).

Buildings work the same way: the raw war-camp buildings in
`assets/sprites/originals/features/` (canvas and flags in team green) become a
neutral version in grey and white, `terrain/features/{id}.png`, and one per
army, `terrain/features/{faction}/{id}.png`, which the game swaps in when a
property changes hands.

Usage:
  python3 tools/sprites/team_colors.py                      # all five armies' units and buildings
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
from soften_outline import soften  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
SOURCE = REPO / "assets" / "sprites" / "originals" / "units" / "western"
OUT = REPO / "assets" / "sprites" / "units"
FACTIONS = ["western", "northern", "southern", "eastern", "nether"]
BUILDINGS = ["base", "workshop", "dock", "roost", "outpost", "storehouse"]
BUILDING_SOURCE = REPO / "assets" / "sprites" / "originals" / "features"
FEATURES = REPO / "assets" / "sprites" / "terrain" / "features"
# Unowned properties: canvas in pale grey and white, like Advance Wars' neutral buildings
NEUTRAL = (0.86, 0.85, 0.82)

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
    if ts < 0.15:
        # Neutral: keep the cloth's shading, drained to a light grey
        nr, ng, nb = colorsys.hls_to_rgb(th, min(0.94, l * 0.55 + 0.42), 0.06)
        return (round(nr * 255), round(ng * 255), round(nb * 255), a)
    # Vivid like an AW army, but shadows stay a little duller than highlights
    sat = min(0.9, max(ts * 0.9, s * 1.6, 0.45) * (0.8 + 0.2 * l))
    # Pull the tones toward the team color's own lightness, so yellow reads as
    # yellow and not olive, while keeping the ramp
    light = min(0.92, l * 0.7 + tl * 0.45)
    nr, ng, nb = colorsys.hls_to_rgb(th, light, sat)
    return (round(nr * 255), round(ng * 255), round(nb * 255), a)


def center_horizontally(w, h, pixels):
    """Shift the unit so its body (the mass of its pixels) is centered.

    Generated units sit a little off center; the game mirrors enemy units, so
    an off-center sprite drifts one way for the player and the other way for
    the enemy. A weapon sticking out doesn't count for much in the average.
    """
    xs = [x for y in range(h) for x in range(w) if pixels[y * w + x][3]]
    if not xs:
        return pixels
    left, right = min(xs), max(xs)
    shift = round((w - 1) / 2 - sum(xs) / len(xs))
    shift = max(-left, min(w - 1 - right, shift))  # never push pixels off the canvas
    out = [(0, 0, 0, 0)] * (w * h)
    for y in range(h):
        for x in range(w):
            if pixels[y * w + x][3]:
                out[y * w + x + shift] = pixels[y * w + x]
    return out


def build(unit_id, faction, target):
    w, h, pixels = read_png(SOURCE / f"{unit_id}.png")
    pixels = center_horizontally(w, h, [recolor(p, target) for p in pixels])
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


def build_building(building_id, out, target):
    """A building in one army's (or the neutral) colors, standing on the bottom row, centered."""
    w, h, pixels = read_png(BUILDING_SOURCE / f"{building_id}.png")
    pixels, _ = soften(w, h, pixels, 0.55, 60)  # colored outlines, like the other terrain
    pixels = center_horizontally(w, h, [recolor(p, target) for p in pixels])
    rows = [y for y in range(h) if any(pixels[y * w + x][3] for x in range(w))]
    shift = h - 1 - rows[-1]
    grounded = [(0, 0, 0, 0)] * (w * h)
    for y in range(h - shift):
        grounded[(y + shift) * w:(y + shift + 1) * w] = pixels[y * w:(y + 1) * w]
    out.parent.mkdir(parents=True, exist_ok=True)
    write_png(out, w, h, grounded)


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

    buildings = [b for b in BUILDINGS if (BUILDING_SOURCE / f"{b}.png").exists()]
    for building_id in buildings:
        build_building(building_id, FEATURES / f"{building_id}.png", NEUTRAL)
        for faction in args.factions:
            build_building(building_id, FEATURES / faction / f"{building_id}.png", colors[faction])
    print(f"Wrote {len(buildings)} buildings, neutral and for {', '.join(args.factions)}")


if __name__ == "__main__":
    main()
