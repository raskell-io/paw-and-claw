# Paw & Claw Game Rules

## Project Overview
Turn-based tactics game inspired by Advance Wars, built with Bevy (Rust).
Art style and setting inspired by Root: A Game of Woodland Might and Right.

## Code Organization
```
src/
├── main.rs          # App setup, camera, lighting
├── game/            # Game logic
│   ├── mod.rs       # Plugin registration
│   ├── map.rs       # Terrain, tiles
│   ├── unit.rs      # Unit types, stats
│   ├── faction.rs   # Factions
│   ├── commander.rs # CO abilities
│   ├── combat.rs    # Attack, capture, etc.
│   ├── movement.rs  # Input, pathfinding
│   ├── turn.rs      # Turn management
│   ├── ai.rs        # AI opponents
│   ├── fog.rs       # Fog of war
│   ├── weather.rs   # Weather effects
│   ├── modding.rs   # Data loading
│   └── save.rs      # Save/load
├── ui/              # egui UI code
│   ├── mod.rs       # Plugin, action menu, hover tracking
│   ├── theme.rs     # Palette, fonts, shared widgets (cards, menu rows, meters)
│   ├── input.rs     # MenuInput: keyboard + gamepad menu navigation
│   ├── menu.rs      # Title screen
│   ├── setup.rs     # Skirmish setup (faction, commander, map)
│   ├── hud.rs       # Battle HUD, inspect card, unit tokens
│   ├── production.rs# Build menu
│   ├── overlays.rs  # Pause, game over, battle teardown
│   └── editor.rs    # Map editor
└── dev.rs           # PAW_SCRIPT screenshot driver (native, dev only)
```

## Key Types
- `Faction` - Western (main), Northern, Southern, Eastern, Nether (antagonist)
- `UnitType` - Scout, Shocktrooper, Recon, Ironclad, etc.
- `Terrain` - Grass, Thicket, Base, Outpost, etc.
- `CommanderId` - Kira, Tanuki, Grimjaw, etc.

## Rendering
- 3D perspective camera looking down at board
- Tiles are flat quads on XZ plane
- Units are 2D sprites billboarded toward camera
- Grid coordinates: (x, y) maps to world (x, 0, z)

## Input
- Mouse for selection and commands
- WASD for camera pan
- All gameplay should work with gamepad (portability goal)

## UI Guidelines
- Build every screen from `ui/theme.rs` (parchment cards, `menu_item`, `key_hint`, `meter`);
  don't hardcode colors or use stock egui windows
- Menus must work with `MenuInput` (keyboard + gamepad), not just the mouse; use
  `theme::pointer_selects` so a resting mouse doesn't steal keyboard focus
- A screen opened by a keypress must ignore input on its first frame
- Modals set `ModalOpen` (via `update_modal_state`) so the board ignores their input
- Everything spawned for a battle gets `BattleEntity` (required by `Unit`, `Tile`, ...)
  so `overlays::reset_battle` can tear it down

## Screenshots
Drive the real game and capture screens with the dev script, e.g.:
`PAW_SCRIPT="wait 90; key Enter; wait 30; shot setup; quit" cargo run`
(steps: wait N, key NAME, click X Y, move X Y, shot NAME, gameover win|lose, quit;
output goes to `screenshots/`, which is gitignored). Captures land a few frames after
`shot`, so wait before sending the next input.
