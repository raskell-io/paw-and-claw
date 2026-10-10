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
- Units face **right**, in three-quarter side view. The game mirrors them as
  needed.

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

| Kind | Canvas | View | Background | Path |
|---|---|---|---|---|
| Units | 48×48 | 3/4 side, facing right | transparent | `units/{faction}/{id}.png` |
| Terrain tiles | 32×32 | top-down, seamless | opaque | `terrain/tiles/{id}.png` |
| Terrain features | 48×48 | 3/4 from above | transparent | `terrain/features/{id}.png` |
| Commander portraits | 64×64 | head and shoulders | `#4E321C` | `portraits/{id}.png` |

PNG, no anti-aliased edges. If the generator works better at a larger size,
generate at 2× or 4× and downscale with **nearest-neighbor** only.

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
   from its prompt. Iterate until it's right.
2. Generate every other sprite for that faction with the anchor as the
   **style reference** (PixelLab supports reference images).
3. Keep the approved anchors in `assets/sprites/anchors/{faction}.png` so a
   later regeneration uses the same reference.

### 3. Generate

Print the prompts you need, with target path and size:

```sh
python3 tools/sprites/manifest.py prompts --faction western --kind units
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
