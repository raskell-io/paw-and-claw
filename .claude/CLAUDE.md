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
- `factions.ron` - 5 factions with names, colors, cost modifiers
- `units.ron` - 20 unit types with full stats
- `terrain.ron` - 13 terrain types with properties
- `commanders.ron` - 15 commanders with CO powers

### Core Module
- `src/game/modding.rs` - Data structures and loading system
- `GameData` resource provides lookup methods

### Build Targets
- **WASM**: All RON embedded via `include_str!` - single artifact
- **Native**: Embedded defaults + mod override support

### Pending Work
- Refactor game systems to use `GameData` lookups instead of hardcoded enum methods
- Currently `Faction::name()`, `UnitType::cost()` etc. are still hardcoded

See `modding.md` for detailed documentation.

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
