# Sprite Pipeline

How Paw & Claw's sprites are specified, generated, checked and brought into the
game. Follow this whenever sprites are added, regenerated or restyled.

## Overview

```
manifest.json ──render──▶ prompts ──generate (PixelLab)──▶ PNGs ──▶ assets/sprites/... ──▶ game
      ▲                                                                    │
      └──────────────────────── check / status ◀───────────────────────────┘
```

- **`assets/sprites/manifest.json`** is the single source of truth: the shared
  style, the faction palettes, and a short *subject* for every sprite.
- **`tools/sprites/manifest.py`** turns subjects into full prompts, checks the
  manifest against the game data, and reports which sprites exist.
- The game loads PNGs by path. Missing sprites fall back to the colored
  faction tokens, so art can arrive one file at a time.

## Art direction (decided)

- **Medieval woodland.** Redwall/Root feel, with no guns or engines. Advance Wars
  roles map to medieval equivalents: tanks become armored war-beasts, artillery
  becomes catapults and trebuchets, missiles become ballistae, aircraft become
  birds with riders, ships become rafts, galleys and longships.
- **Anthropomorphic animals**, each faction with its own species and motifs
  (see `factions` in the manifest and `prompts/art-direction.md`).
- **Readable at small size:** a clear silhouette, a 1px dark outline, 3-4 tone
  shading and light from the upper left.
- **Advance Wars Dual Strike spirit on the map.** Every unit has two sprites:
  a small, chunky **map sprite** (32×32, few colors, bold outline) for the board,
  and a detailed **battle sprite** (64×64) for battle scenes. Both come from the
  same subject in the manifest.
- Units face **right** in a low top-down view. The game mirrors them as needed.

## Roster

The internal ID (left) is stable: it's used in code, saves and file names. The
display name and description live in `assets/data/units.ron` and must match the
manifest (`check` enforces this).

| ID | Name | Role | Eastern | Northern | Western | Southern | Nether |
|---|---|---|---|---|---|---|---|
| scout | Militia | Cheap infantry, captures | tanuki | hedgehog | raccoon | meerkat | mole |
| shocktrooper | Halberdier | Anti-armor infantry | snow monkey | badger | opossum | hyena | stag beetle |
| recon | Outrider | Fast scout | fox on sika deer | stoat on hare | coyote on pronghorn | meerkat on cheetah | mole on centipede |
| ironclad | Charger | Main war-beast | boar | brown bear | armadillo | warthog | rhino beetle |
| juggernaut | Warbeast | Heavy war-beast | water buffalo | elk | bison | rhinoceros | soldier ant |
| behemoth | Colossus | Strongest ground unit | pagoda tortoise | mammoth fortress | watchtower tortoise | war elephant | armored earthworm |
| flak | Hawkbane | Anti-air archers | crane | hedgehog | raccoon | secretary bird | bombardier beetle |
| siege | Catapult | Indirect fire, short | red panda | badger | opossum | hyena | mole rat |
| barrage | Trebuchet | Indirect fire, long | snow monkey | brown bear | coyote | baboon | scorpion |
| stinger | Skylance | Long-range anti-air | fox | fox | raccoon | meerkat | mole |
| carrier | Caravan | Foot transport | buffalo cart | reindeer sledge | mule wagon | zebra cart | millipede |
| supplier | Supply Cart | Resupply | tanuki | badger | squirrel | warthog | mole rat |
| ferrier | Skyferry | Air transport | crane | swan | condor | marabou stork | cave moth |
| skywing | Hawk Rider | Air-to-ground | monkey on kite | stoat on goshawk | raccoon on hawk | meerkat on eagle | mole on scarab |
| raptor | Sky Knight | Air superiority | fox on pheasant | fox on snowy owl | coyote on eagle | lion on eagle | beetle on hornet |
| talon | Bombardier | Bomber | tanuki on owl | badger on raven | opossum on vulture | hyena on vulture | mole rat on bat |
| barge | Raft | Naval transport | bamboo raft | log raft | log raft | reed raft | fungus raft |
| frigate | Galley | Anti-air/anti-diver ship | war junk | longship | keelboat | sun galley | bone galley |
| lurker | Diver | Submerged ambusher | otter | seal | beaver | crocodile | diving beetle |
| dreadnought | Flagship | Naval bombardment | atakebune | dragon longship | river fort | sun warship | leviathan |

The full subjects live in the manifest. This table is a summary.

## Sprite specs

Everything on the board shares one 32px grid, so a map sprite, a feature and a
tile all have the same pixel size, and one sprite spans one tile.

| Kind | Canvas | View | Background | Path |
|---|---|---|---|---|
| Unit map sprites | 32×32 | low top-down, facing right | transparent | `units/{faction}/{id}.png` |
| Unit battle sprites | 64×64 | low top-down, facing right | transparent | `battle/{faction}/{id}.png` |
| Terrain tiles | 32×32 | top-down, seamless | opaque | `terrain/tiles/{id}.png` |
| Terrain features | 32×32 | low top-down | transparent | `terrain/features/{id}.png` |
| Commander portraits | 64×64 | head and shoulders | `#4E321C` | `portraits/{id}.png` |

Battle sprites aren't used in game yet (no battle scenes so far); they're
generated now so each unit's two sprites match.

### PixelLab settings

Use **Create image → Pixen** (1 generation per image). Each style in the
manifest has a `pixellab` block with the settings that worked, and `prompts`
prints them with every prompt. For unit map sprites: transparent background,
direction East (facing right), view Low top-down, Low detail, Black outline,
32×32.

Lessons from the first tests:
- **16×16 doesn't work.** The model can't draw a readable creature at that size.
  32×32 still gives chunky pixels at board scale.
- The detailed 64×64 look ("Highly detailed") is great for battle sprites but
  too fine for the map.
- Check the downloaded PNG really is transparent. If it isn't, run
  `python3 tools/sprites/remove_bg.py IN.png OUT.png`, which clears the flat
  background up to the sprite's outline.

## Workflow

### 1. Edit the manifest

Change subjects, styles or palettes in `manifest.json`, then rebuild the
prompts and verify:

```sh
python3 tools/sprites/manifest.py render   # rebuild every "prompt" field
python3 tools/sprites/manifest.py check    # must pass before generating
```

If you rename a unit, change `name`, `symbol` and `description` in both the
manifest and `assets/data/units.ron`. `check` reports any mismatch.

### 2. Lock the style with anchor sprites

Consistency across about 130 sprites is the hard part. Before batch work:

1. For each faction, generate one **anchor** unit (Militia is a good choice)
   in both sizes from its prompts. Iterate until it's right.
2. Generate every other sprite for that faction with the anchor as the
   **style reference** (PixelLab's Pro model takes a style image; with Pixen,
   use the anchor as the init image).
3. Keep the approved anchors as `assets/sprites/anchors/{faction}-map.png` and
   `{faction}-battle.png`, so a later regeneration uses the same reference.

### 3. Generate

Print the prompts you need, with target path and size:

```sh
python3 tools/sprites/manifest.py prompts --faction western --kind units    # map sprites
python3 tools/sprites/manifest.py prompts --faction western --kind battle   # battle sprites
python3 tools/sprites/manifest.py prompts --missing          # only what's still missing
```

In PixelLab: paste the prompt, set the canvas size, attach the faction anchor
as the style reference, generate, and export a transparent PNG to the listed
path. Clean up stray pixels in Aseprite if needed.

### 4. Review in game

```sh
python3 tools/sprites/manifest.py status -v
PAW_SCRIPT="wait 90; key Enter; wait 30; key Enter; wait 60; shot board; quit" cargo run
```

Judge sprites on the real board at real size (`screenshots/board.png`), not
in the generator. Reject anything that doesn't read at a glance.

### How the game uses sprites

- Sprites load at startup. Native builds only request files that exist.
  "Begin Battle" waits until loading has finished, so every unit and tile
  spawns with whatever art is available.
- **Units:** drawn as camera-facing billboards. Player units face right and
  enemies are mirrored. Units with art lose their letter token and keep an HP
  pip, and stand on a faction-colored ground mark. Without art, a unit stays a
  colored card with a letter token.
- **Tiles:** the texture covers the tile top, and the sides continue it as a
  thin strip. Owned properties are tinted toward their owner's color.
- **Features** (trees, rocks, buildings) appear only when they have a sprite.
  They stand at the back of their tile, and properties fly a pennant in the
  owner's color, which updates on capture.
- **Portraits** replace the commander initial medallions in setup and the HUD.
- Textures use nearest-neighbor sampling, so pixel art stays crisp.

### Testing without real art

```sh
python3 tools/sprites/placeholders.py /tmp/paw-placeholders --factions western,northern
BEVY_ASSET_ROOT=/tmp/paw-placeholders cargo run
```

`BEVY_ASSET_ROOT` points the game at another asset folder, which is also handy
for previewing a batch of new art before copying it into `assets/`.

### 5. Record decisions

When the style changes (palette, view, size, a new faction look), update the
manifest and this README in the same commit so the set can be regenerated.

## Notes

- **Licensing:** check the generator's terms for commercial use and output
  ownership before shipping. Steam requires disclosing AI-generated content.
- **Faction colors:** the sprite palettes are the art palette from
  `prompts/palette.md`. The UI's faction colors come from
  `assets/data/factions.ron`. Western differs (bright green in the UI, sage in
  the art); align them once the sprites exist.
- `prompts/` keeps the original art direction and palette notes. The old
  per-unit prompt files were replaced by the manifest.
