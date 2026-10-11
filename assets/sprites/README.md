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

## Scale: the forest floor

The armies are small animals, so the map is the woodland floor at their scale,
not an Advance Wars landscape. Terrain features are **bushes, tufts of tall
grass, pebbles, fallen twigs, stumps, creeks and puddles**, never mountains,
whole forests or castles. A "boulder" is a big pebble, a "thicket" is a bush,
and buildings are burrows, twig towers and acorn granaries. Every feature
prompt carries this through the `features` style in the manifest.

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

Each style in the manifest has a `pixellab` block with the settings that
worked, and `prompts` prints them with every prompt.

**Unit map sprites** use **Create image → Pro → Pro Flash** at 32×32 with the
faction's anchor (`anchors/{faction}-map.png`) as the **style image**, one unit
per call (5 generations, about 40 seconds). Start each description with the
unit's name ("Recon unit. …") so the downloaded files can be told apart, then
"A single tiny map unit sprite … facing right: {subject}. {faction look}. Match
the style image exactly …". Keep the raw download in
`originals/units/{faction}/`, stand ground and naval units on row 30 (fliers
stay as drawn), and make the idle sheet with `idle_frames.py` (`--feet 3` for
walkers, `--feet 0` for boats and fliers, which bob whole). The first sprite,
the Western Scout, was made with Pixen (1 generation, no style image), with:
transparent background, direction East, view Low top-down, Low detail, Black
outline.

Lessons from the first tests:
- **16×16 doesn't work.** The model can't draw a readable creature at that size.
  32×32 still gives chunky pixels at board scale.
- The detailed 64×64 look ("Highly detailed") is great for battle sprites but
  too fine for the map.
- Check the downloaded PNG really is transparent. If it isn't, run
  `python3 tools/sprites/remove_bg.py IN.png OUT.png`, which clears the flat
  background up to the sprite's outline.

### Terrain the Dual Strike way

- **Ground tiles are drawn by code**, not generated:
  `python3 tools/sprites/ground_tiles.py` writes all 15 tiles in a fixed,
  bright palette (meadow, sand, creek, pond, flagstones).
  Generated ground came out far too busy to repeat across a map.
- **Tiles can be strips of variants** (32px columns), and the game picks one per tile:
  - Meadow tiles have 4 variants (plain, clover, flowers, both), picked by position.
  - `log.png` (shown in game as "Trail") is a 16-column atlas indexed by which
    neighbors are trail or a base (1 N, 2 E, 4 S, 8 W), so paths join up. A
    trail tile with no trail neighbors shows the fallen-log sprite instead.
  - `shore.png` is indexed the same way by which sides face water, so the sand
    runs only along the waterline.
- **The board is flat** (land and shore height 4, water 2 in `terrain.ron`),
  and tiles butt together with no grid gaps. Tile sides are shaded darker.
  Height comes from features.
- **Features stand on plain meadow:** every land feature's ground tile is
  ordinary grass, so the sprite alone says what's there. Owned properties
  aren't tinted; their pennant shows the owner.
- **Features match the bush.** The bush (thicket) is drawn by code:
  `python3 tools/sprites/nature_features.py`. Every other feature (pebble,
  log, stump, brambles, tall grass, lookout, granary, burrow-home base) is
  generated with **Create image → Pro → Pro Flash**, with the bush as the
  **style image** (copy palette, outline, detail and shading). That's 5–6
  generations a call, so ask for a sheet: one 128×64 call gave a 4×2 grid of
  objects. The objects don't sit on the grid, and ones at the edge can come
  out clipped, so slice by the gaps between them, fit each into 32×32
  standing on the bottom row, and soften the outline. Keep the raw sheet in
  `assets/sprites/originals/features/`. Code-drawn versions of these looked
  like programmer art next to the bush; Pixen (no style image) drifted in style.
- **Features have no black outlines.** Dual Strike outlines terrain in darker
  shades of the object's own colors, so it stays subtle while units, which keep
  dark outlines, stand out. Generate with "Selective outline", then run
  `python3 tools/sprites/soften_outline.py RAW.png assets/sprites/terrain/features/NAME.png`,
  which recolors any remaining near-black pixels. Keep the raw download in
  `assets/sprites/originals/features/` so the step can be re-tuned.
- **Features don't bring their own ground.** PixelLab draws trees, mountains
  and buildings on a little grass-and-earth platform no matter the prompt or
  view setting (tested: Low top-down, None, Side). Strip it with
  `python3 tools/sprites/strip_ground.py RAW.png OUT.png --cut N --grass-from M`
  (`--preview` prints the rows to choose N and M), then soften the outline.
  Current values: forest `--cut 27 --grass-from 22`, mountain `--cut 24 --grass-from 20`,
  base `--cut 29 --grass-from 22`.
- Forests and mountains stand on plain grass (no separate forest-floor tile),
  so the feature carries the look, as in Dual Strike.
- **Coastlines are autotiled in the game:** each water tile gets sand and foam
  along every side (and corner) that touches land, built once per neighbor
  pattern and cached (`shoreline_texture` in `src/game/map.rs`).

### Unit animation

- A unit image whose width is a whole multiple of its height is a strip of
  frames. All units play their frames in sync, as in Advance Wars: 3 fps at
  rest, 8 fps when selected or moving. Finished units stand still.
- Until real frames exist, `python3 tools/sprites/idle_frames.py IN.png OUT.png`
  makes the classic 2-frame idle: the body bobs one pixel while the feet stay
  planted. Keep the single-frame original in `anchors/`.
- PixelLab's image-to-animation tools need a paid plan, so the trial can't
  make frames.

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
