#!/usr/bin/env python3
"""Write crude placeholder sprites to exercise the rendering pipeline without real art.

Usage:
  python3 tools/sprites/placeholders.py OUT_DIR [--factions western,northern]
  BEVY_ASSET_ROOT=OUT_DIR cargo run

Writes OUT_DIR/assets/sprites/... following the manifest's paths. Never point
OUT_DIR at the repo root: placeholders must not mix with real art.
Standard library only.
"""
import argparse, json, random, struct, zlib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
parser.add_argument("out_dir", type=Path)
parser.add_argument("--factions", default="western,northern", help="comma-separated faction keys")
args = parser.parse_args()
if args.out_dir.resolve() == REPO:
    parser.error("refusing to write placeholders into the repo; pick a separate directory")
root = args.out_dir
manifest = json.loads((REPO / "assets" / "sprites" / "manifest.json").read_text())

def png(path, w, h, px):
    raw = b"".join(b"\0" + bytes(c for p in px[y*w:(y+1)*w] for c in p) for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))

hexrgb = lambda h: tuple(int(h[i:i+2], 16) for i in (1, 3, 5))
INK = (40, 28, 20, 255)

def unit(color, accent, flying):
    w = h = 48; px = [(0, 0, 0, 0)] * (w * h)
    def put(x, y, c):
        if 0 <= x < w and 0 <= y < h: px[y*w + x] = c
    base = 40 if not flying else 30
    # body, head, ear, spear: crude creature facing right
    for y in range(h):
        for x in range(w):
            body = ((x-22)/9)**2 + ((y-(base-10))/11)**2 <= 1
            head = ((x-30)/7)**2 + ((y-(base-24))/7)**2 <= 1
            ear = abs(x-28) + abs(y-(base-32)) <= 3
            if body or head or ear:
                put(x, y, (*color, 255))
    for y in range(h):  # outline
        for x in range(w):
            if px[y*w+x][3] == 0 and any(0 <= x+dx < w and 0 <= y+dy < h and px[(y+dy)*w+x+dx][3] == 255 and px[(y+dy)*w+x+dx] != INK
                                         for dx, dy in ((1,0),(-1,0),(0,1),(0,-1))):
                px[y*w+x] = INK
    for y in range(base-34, base-2):  # spear shaft in accent color
        put(38, y, (*accent, 255))
    put(33, base-25, INK)  # eye
    if flying:
        for x in range(8, 40):
            put(x, base - 2, (*accent, 255)); put(x, base - 1, INK)
    return px

def tile(color, seed):
    random.seed(seed); w = h = 32
    return [tuple(max(0, min(255, c + random.randint(-14, 14))) for c in color) + (255,) for _ in range(w*h)]

def feature(color):
    w = h = 48; px = [(0, 0, 0, 0)] * (w * h)
    for y in range(h):
        for x in range(w):
            crown = ((x-24)/16)**2 + ((y-18)/14)**2 <= 1
            trunk = 21 <= x <= 27 and 28 <= y <= 46
            if crown: px[y*w+x] = (*color, 255)
            elif trunk: px[y*w+x] = (110, 75, 45, 255)
    return px

def shrink(px, src=48, dst=32):
    """Nearest-neighbor resize so placeholders match the 32px map grid."""
    return [px[(y * src // dst) * src + (x * src // dst)] for y in range(dst) for x in range(dst)]

for u in manifest["units"]:
    for fkey in args.factions.split(","):
        f = manifest["factions"][fkey]["colors"]
        flying = u["class"] in ("Air", "AirTransport")
        png(root / f"assets/sprites/units/{fkey}/{u['id']}.png", 32, 32,
            shrink(unit(hexrgb(f["primary"]["hex"]), hexrgb(f["accent"]["hex"]), flying)))
colors = {"grass": (120, 170, 80), "tall_grass": (100, 150, 70), "thicket": (60, 100, 50), "brambles": (80, 90, 60),
          "log": (140, 110, 80), "boulder": (130, 130, 135), "hollow": (90, 70, 55), "creek": (100, 150, 200),
          "pond": (60, 110, 170), "shore": (190, 170, 130), "base": (150, 130, 110), "outpost": (150, 135, 115), "storehouse": (160, 125, 90)}
for i, t in enumerate(manifest["terrain"]["tiles"]):
    png(root / f"assets/sprites/terrain/tiles/{t['id']}.png", 32, 32, tile(colors[t["id"]], i))
for t in manifest["terrain"]["features"]:
    png(root / f"assets/sprites/terrain/features/{t['id']}.png", 32, 32, shrink(feature((50, 110, 50) if t["id"] in ("thicket", "brambles") else (140, 120, 100))))
print(f"Wrote placeholder sprites to {root}/assets/sprites")
