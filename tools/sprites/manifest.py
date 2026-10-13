#!/usr/bin/env python3
"""Sprite manifest tool for Paw & Claw.

The manifest (assets/sprites/manifest.json) is the single source of truth for
every generated sprite: shared style, faction palettes, and a short subject per
unit / terrain / commander (its portrait and full-body figure). Full prompts are rendered from those parts so the
whole set stays consistent.

Commands:
  render              Rebuild every "prompt" field from style + subject (rewrites the manifest)
  check               Verify the manifest matches the game data and prompts are up to date
  status [--faction F]  Show which sprite files exist and which are missing
  prompts [--faction F] [--kind units|battle|tiles|features|portraits|figures] [--missing]
                      Print prompts (with target path and size) ready to paste into the generator

Standard library only. Run from anywhere: paths are resolved from the repo root.
"""

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MANIFEST = ROOT / "assets" / "sprites" / "manifest.json"
DATA = ROOT / "assets" / "data"
ASSETS = ROOT / "assets"


def load():
    return json.loads(MANIFEST.read_text())


def save(manifest):
    MANIFEST.write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + "\n")


# ---------------------------------------------------------------------------
# Prompt rendering
# ---------------------------------------------------------------------------

def faction_clause(faction):
    c = faction["colors"]
    return (f"{faction['name']} faction: {faction['look']}. Colors: {c['primary']['name']} {c['primary']['hex']} "
            f"and {c['secondary']['name']} {c['secondary']['hex']}, with {c['accent']['name']} {c['accent']['hex']} accents")


def render_prompt(style, subject, faction=None):
    parts = [style["prefix"], subject]
    if faction is not None:
        parts.append(faction_clause(faction))
    parts.append(style["suffix"])
    cleaned = (p.strip().rstrip(".") for p in parts if p)
    return ". ".join(p[0].upper() + p[1:] for p in cleaned) + "."


def entries(manifest):
    """Yield (kind, key, faction_key, entry, style, path) for every sprite in the manifest."""
    styles = manifest["style"]
    factions = manifest["factions"]
    for unit in manifest["units"]:
        for fkey, sprite in unit["sprites"].items():
            folder = factions[fkey]["asset_folder"]
            # Same subject, two renderings: the small map sprite and the detailed battle sprite
            yield "units", unit["id"], fkey, sprite, styles["units"], ASSETS / "sprites" / "units" / folder / f"{unit['id']}.png"
            yield "battle", unit["id"], fkey, sprite, styles["battle"], ASSETS / "sprites" / "battle" / folder / f"{unit['id']}.png"
    for tile in manifest["terrain"]["tiles"]:
        yield "tiles", tile["id"], None, tile, styles["tiles"], ASSETS / "sprites" / "terrain" / "tiles" / f"{tile['id']}.png"
    for feature in manifest["terrain"]["features"]:
        yield "features", feature["id"], None, feature, styles["features"], ASSETS / "sprites" / "terrain" / "features" / f"{feature['id']}.png"
    for co in manifest["commanders"]:
        yield "portraits", co["id"], co["faction"], co, styles["portraits"], ASSETS / "sprites" / "portraits" / f"{co['id']}.png"
        # The full-body intro art: the same subject, styled by the commander's own portrait
        yield "figures", co["id"], co["faction"], co, styles["figures"], ASSETS / "sprites" / "figures" / f"{co['id']}.png"


def prompt_field(kind):
    return {"battle": "battle_prompt", "figures": "figure_prompt"}.get(kind, "prompt")


def expected_prompt(manifest, kind, fkey, entry, style):
    faction = manifest["factions"][fkey] if fkey else None
    return render_prompt(style, entry["subject"], faction)


def cmd_render(_args):
    manifest = load()
    changed = 0
    for kind, key, fkey, entry, style, _path in entries(manifest):
        prompt = expected_prompt(manifest, kind, fkey, entry, style)
        if entry.get(prompt_field(kind)) != prompt:
            entry[prompt_field(kind)] = prompt
            changed += 1
    save(manifest)
    print(f"Rendered prompts ({changed} changed)")


# ---------------------------------------------------------------------------
# Consistency checks against the game data
# ---------------------------------------------------------------------------

def ron_entries(file_name, top_key):
    """Map of entry key -> entry text for a top-level RON map like `units: { "scout": ( ... ) }`."""
    text = (DATA / file_name).read_text()
    text = re.sub(r"//[^\n]*", "", text)
    body = text[text.index(f"{top_key}:"):]
    out = {}
    for m in re.finditer(r'^\s{8}"([a-z_]+)":\s*\(', body, re.M):
        start = m.end()
        depth, i = 1, start
        while depth and i < len(body):
            depth += {"(": 1, ")": -1}.get(body[i], 0)
            i += 1
        out[m.group(1)] = body[start:i]
    return out


def ron_field(entry_text, field):
    m = re.search(rf'\b{field}:\s*(?:"([^"]*)"|\'(.)\'|([A-Za-z_]+|-?[0-9.]+))', entry_text)
    return next((g for g in m.groups() if g is not None), None) if m else None


def cmd_check(_args):
    manifest = load()
    problems = []

    units_ron = ron_entries("units.ron", "units")
    factions_ron = ron_entries("factions.ron", "factions")
    terrain_ron = ron_entries("terrain.ron", "terrain")
    commanders_ron = ron_entries("commanders.ron", "commanders")

    factions = manifest["factions"]
    if set(factions) != set(factions_ron):
        problems.append(f"factions differ: manifest {sorted(factions)} vs factions.ron {sorted(factions_ron)}")
    for fkey, f in factions.items():
        folder = ron_field(factions_ron.get(fkey, ""), "asset_folder")
        if folder and folder != f["asset_folder"]:
            problems.append(f"faction {fkey}: asset_folder {f['asset_folder']} != factions.ron {folder}")

    unit_ids = [u["id"] for u in manifest["units"]]
    if set(unit_ids) != set(units_ron):
        problems.append(f"units differ: missing {sorted(set(units_ron) - set(unit_ids))}, extra {sorted(set(unit_ids) - set(units_ron))}")
    symbols = {}
    for unit in manifest["units"]:
        ron = units_ron.get(unit["id"], "")
        for field in ("name", "symbol", "description"):
            if ron and ron_field(ron, field) != unit[field]:
                problems.append(f"unit {unit['id']}: {field} '{unit[field]}' != units.ron '{ron_field(ron, field)}'")
        if ron and ron_field(ron, "asset_name") != unit["id"]:
            problems.append(f"unit {unit['id']}: units.ron asset_name should be '{unit['id']}'")
        symbols.setdefault(unit["symbol"], []).append(unit["id"])
        missing = set(factions) - set(unit["sprites"])
        if missing:
            problems.append(f"unit {unit['id']}: no sprite for {sorted(missing)}")
    for symbol, ids in symbols.items():
        if len(ids) > 1:
            problems.append(f"symbol '{symbol}' used by {ids}")

    tile_ids = {t["id"] for t in manifest["terrain"]["tiles"]}
    if tile_ids != set(terrain_ron):
        problems.append(f"terrain tiles differ from terrain.ron: {sorted(tile_ids ^ set(terrain_ron))}")
    with_features = {k for k, v in terrain_ron.items() if float(ron_field(v, "feature_height") or 0) > 0}
    feature_ids = {f["id"] for f in manifest["terrain"]["features"]}
    if feature_ids != with_features:
        problems.append(f"terrain features differ from terrain.ron feature_height > 0: {sorted(feature_ids ^ with_features)}")

    co_ids = {c["id"] for c in manifest["commanders"]}
    if co_ids != set(commanders_ron):
        problems.append(f"commanders differ: {sorted(co_ids ^ set(commanders_ron))}")

    for kind, key, fkey, entry, style, _ in entries(manifest):
        if entry.get(prompt_field(kind)) != expected_prompt(manifest, kind, fkey, entry, style):
            problems.append(f"{kind} {key}{'/' + fkey if fkey else ''}: prompt out of date (run `render`)")

    if problems:
        print("\n".join(f"✗ {p}" for p in problems))
        print(f"\n{len(problems)} problem(s)")
        sys.exit(1)
    count = sum(1 for _ in entries(manifest))
    print(f"✓ Manifest consistent with game data ({count} sprites)")


# ---------------------------------------------------------------------------
# Status and prompt listing
# ---------------------------------------------------------------------------

def selected(manifest, args):
    for kind, key, fkey, entry, style, path in entries(manifest):
        if getattr(args, "faction", None) and fkey != args.faction:
            continue
        if getattr(args, "kind", None) and kind != args.kind:
            continue
        yield kind, key, fkey, entry, style, path


def cmd_status(args):
    manifest = load()
    totals = {}
    missing = []
    for kind, key, fkey, entry, style, path in selected(manifest, args):
        group = f"{kind}/{fkey}" if kind in ("units", "battle") else kind
        have, total = totals.get(group, (0, 0))
        exists = path.exists()
        totals[group] = (have + exists, total + 1)
        if not exists:
            missing.append(path.relative_to(ROOT))
    for group, (have, total) in sorted(totals.items()):
        bar = "█" * have + "·" * (total - have)
        print(f"{group:<18} {have:>3}/{total:<3} {bar}")
    done = sum(h for h, _ in totals.values())
    print(f"\n{done}/{sum(t for _, t in totals.values())} sprites present")
    if args.verbose and missing:
        print("\nMissing:\n" + "\n".join(f"  {p}" for p in missing))


def cmd_prompts(args):
    manifest = load()
    for kind, key, fkey, entry, style, path in selected(manifest, args):
        if args.missing and path.exists():
            continue
        name = entry.get("name", key)
        w, h = style["size"]
        print(f"## {kind}: {name}{' (' + fkey + ')' if fkey else ''}")
        print(f"file: {path.relative_to(ROOT)}   size: {w}x{h}")
        settings = style.get("pixellab")
        if settings:
            print("pixellab: " + ", ".join(f"{k}={v}" for k, v in settings.items()))
        print(entry[prompt_field(kind)])
        print()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("render").set_defaults(func=cmd_render)
    sub.add_parser("check").set_defaults(func=cmd_check)
    status = sub.add_parser("status")
    status.add_argument("--faction")
    status.add_argument("--kind", choices=["units", "battle", "tiles", "features", "portraits", "figures"])
    status.add_argument("-v", "--verbose", action="store_true", help="list missing files")
    status.set_defaults(func=cmd_status)
    prompts = sub.add_parser("prompts")
    prompts.add_argument("--faction")
    prompts.add_argument("--kind", choices=["units", "battle", "tiles", "features", "portraits", "figures"])
    prompts.add_argument("--missing", action="store_true", help="only sprites without a file yet")
    prompts.set_defaults(func=cmd_prompts)
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
