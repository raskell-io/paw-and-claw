# Paw & Claw Game Rules

## Project Overview
Turn-based tactics game inspired by Advance Wars, built with Bevy (Rust).

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
└── ui/              # egui UI code
    └── mod.rs       # All UI systems
```

## Key Types
- `Faction` - Eastern, Northern, Western, Southern, Nether, Wanderer
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
