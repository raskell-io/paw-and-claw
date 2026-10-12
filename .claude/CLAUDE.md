# Paw & Claw - Project Context

## Overview
Turn-based tactics game inspired by **Advance Wars**, built with Bevy 0.17 (Rust).

**Art style and setting** inspired by **Root: A Game of Woodland Might and Right** - anthropomorphic woodland creatures in a territorial conflict.

## Factions
5 factions, mirroring Advance Wars structure:
- **Western Frontier** - Cunning survivalists, main protagonist faction (like Orange Star)
- **Northern Realm** - Stalwart defenders (like Blue Moon)
- **Southern Pride** - Mighty beasts (like Green Earth)
- **Eastern Empire** - Disciplined armies (like Yellow Comet)
- **Nether Dominion** - Antagonist faction, subterranean swarm (like Black Hole)

## Tech Stack
- **Engine**: Bevy 0.17
- **Renderer**: wgpu (default, WebGPU-based)
- **Platforms**: Windows, macOS, Linux (Web planned)

## Architecture
- 3D board with 2D billboarded sprites (Advance Wars: Dual Strike style)
- ECS-based game logic
- **Data-driven design** for moddability

## Modding System (Implemented)

The game uses RON files for moddable content. For WASM builds, data is embedded at compile time. For native builds, mods can override data from `mods/` directory.

### Data Files (assets/data/)
- `factions.ron` - 5 factions: names, colors, animals, sprite folders. Factions are cosmetic; every gameplay effect comes from commanders
- `units.ron` - 21 unit types with full stats (`dive_stamina`/`hits_divers` for diving)
- `terrain.ron` - 17 terrain types with properties (`builds` and `resupplies` list the unit classes a property makes and serves)
- `commanders.ron` - 28 commanders copying the Advance Wars 2 / Dual Strike COs (abilities, CO Power, Super CO Power)
- `damage_tables.ron` - attacker vs defender base damage (missing entry = can't attack)
- `movement_costs.ron` - movement cost per terrain and unit class
- `audio.ron` - sound effects and music by id (files under `assets/audio/`)

### Core Module
- `src/game/modding.rs` - Data structures, loading, validation
- `GameData` resource is the single source of truth for game data. Enums like
  `UnitType`/`Terrain`/`Faction`/`CommanderId` are identifiers only; never add
  hardcoded stats/names to them. Pass `&GameData` to functions that need data.

### Build Targets
- **WASM**: All RON embedded via `include_str!` - single artifact
- **Native**: Embedded defaults + mod override support

### Web build
- `tools/web/build.sh [OUT_DIR]` builds the browser version (WebGL2, profile `web-release`) into
  a static folder: wasm + JS from `wasm-bindgen`, `web/index.html`, runtime assets
- The wasm-bindgen CLI must match the `wasm-bindgen` version in Cargo.lock
  (`cargo install wasm-bindgen-cli --version <it> --locked`); `wasm-opt` shrinks it if installed
- `.github/workflows/pages.yml` builds and publishes it to GitHub Pages on pushes to `main`
- Test locally: `python3 -m http.server -d dist`, then open http://localhost:8000

See `modding.md` for detailed documentation.

### Audio
- `tools/audio/sfx.py` synthesizes every sound effect into `assets/audio/sfx/*.wav`, aiming for
  the Game Boy Advance (sampled-instrument feel, not NES squares): FM bells and brass,
  plucked strings, filtered saws and noise, then an 18 kHz / 8-bit output stage. Re-run it
  after editing a sound
- `game/audio.rs` plays them: send `PlaySfx("id")`, or let it react to game events.
  Menu rows queue their own tick/confirm sounds (`theme::queue_sound`)
- Music is composed in code (`tools/audio/music.py NAME…` renders OGGs into `assets/audio/music/`):
  CO themes (`co_<key>`, 18 so far; a CO without one plays another theme of its army), one CO Power
  theme per army (`power_<army>`), the title, intro, battle-start, victory and defeat jingles.
  `audio::choose_music` picks what plays

### Saves
- `game/save.rs`: a save is a whole battle (`SaveGame`, versioned; older versions are refused).
  Three manual slots plus an autosave (start of each human turn, after each of their units acts);
  saving only at quiet moments of a human turn (`BattleSnapshot::can_save`)
- Desktop: `saves/save_{auto,1,2,3}.json`. Web: localStorage `paw_and_claw_save_*` (the installed
  home-screen app has its own storage, apart from the browser tab's), so the web build can export
  every slot to one backup file and import it (`pawExportSaves`/`pawImportSaves` in `web/index.html`)
- Loading sets `PendingLoad`; `apply_pending_load` (Battle state) rebuilds the board with the normal
  spawn code and restores the state, then the turn resumes with its Day banner
- UI: `ui/saves.rs` (slot screen, toasts, F5/F9), Continue on the title screen, map menu Options

### Battles and maps
- Skirmish: 2-4 armies, each played by a person (hot seat) or the computer, free-for-all or
  2 vs 2 (armies 1 & 3 vs 2 & 4). See the Armies notes in `.claude/rules/game.md`
- The map editor saves maps with the saves (`saves/map_<name>.json`, localStorage on the web);
  `CustomMaps` lists them and setup offers them once every army has an HQ (`map_problem`)

### Sprites
- `assets/sprites/manifest.json` is the source of truth for all generated sprites
  (style, faction palettes, one subject per unit/terrain/portrait)
- `tools/sprites/manifest.py render|check|status|prompts` builds prompts, checks them
  against the game data, and tracks which PNGs exist
- Full workflow: `assets/sprites/README.md`. Art is medieval woodland (no guns/engines)
- `tools/sprites/brand.py MASCOT_PNG` draws the boot art in `assets/sprites/brand/` (the raskell.io
  mascot as a red stamp seal, the cartoon hand and stamp, washi paper); the mascot comes from
  www.raskell.io (`static/raskell-mascot.avif`, converted to PNG)

---

## Portability Goal

Keep codebase clean and platform-agnostic. Console (Switch) is a future possibility but not a "flip a switch" workflow - requires porting work under Nintendo's developer program.

**Current focus**: Write portable code now, worry about console later.

See `portability.md` for guidelines.

### Key Principles
1. **Gamepad-first UX** - All gameplay works with controller
2. **Conservative GPU** - Basic 3D, StandardMaterial, billboards
3. **No platform hacks** - Use Bevy's abstractions
4. **Resolution flexible** - UI scales to various screens
