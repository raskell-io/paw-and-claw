# Modding Architecture

Goal: All game content moddable without recompiling Rust code.

## Implementation Status

### Completed ✅

**Data Files Created:**
- `assets/data/factions.ron` - 6 factions (Eastern, Northern, Western, Southern, Nether, Wanderer)
- `assets/data/units.ron` - 20 unit types with full stats
- `assets/data/terrain.ron` - 13 terrain types with properties
- `assets/data/commanders.ron` - 15 commanders with CO powers

**Loading System (`src/game/modding.rs`):**
- RON parsing with `ron` crate
- WASM embedding via `include_str!` - single artifact build
- Native mod loading from `mods/` directory
- `GameData` resource with lookup methods

### Pending ⏳

**Refactor game systems to use `GameData`:**
- [ ] Replace `Faction::name()` calls with `GameData::get_faction()`
- [ ] Replace `UnitType` stat methods with `GameData::get_unit()`
- [ ] Replace `Terrain` property methods with `GameData::get_terrain()`
- [ ] Replace `Commander` lookups with `GameData::get_commander()`

**Additional data files:**
- [ ] Maps as RON files
- [ ] Damage tables (unit vs unit effectiveness)
- [ ] Movement cost tables (unit class vs terrain)

---

## Directory Structure

```
assets/
├── data/
│   ├── factions.ron       # ✅ Faction definitions
│   ├── commanders.ron     # ✅ CO stats and abilities
│   ├── units.ron          # ✅ Unit types and stats
│   ├── terrain.ron        # ✅ Terrain types and properties
│   └── maps/              # ⏳ Map files (planned)
├── sprites/
│   ├── units/
│   │   ├── eastern/       # Per-faction unit sprites
│   │   ├── northern/
│   │   └── default/       # Fallback sprites
│   ├── terrain/
│   ├── ui/
│   └── portraits/         # CO portraits
└── audio/                  # Future

mods/                       # Native builds only
└── my-mod/
    ├── factions.ron       # Override specific factions
    ├── commanders.ron     # Override specific COs
    └── ...
```

## Data Format (RON)

RON (Rusty Object Notation) - Rust-native, readable, supports comments.

### factions.ron
```ron
(
    factions: {
        "eastern": (
            name: "Eastern Empire",
            description: "Disciplined armies of the rising sun.",
            color: [0.9, 0.3, 0.3],  // RGB
            unit_cost_modifier: 0.85,
            animals: ["Tanuki", "Red Panda", "Crane"],
            asset_folder: "eastern",
        ),
    }
)
```

### units.ron
```ron
(
    units: {
        "scout": (
            name: "Scout",
            description: "Light foot soldier, cheap and captures buildings",
            symbol: 'S',
            asset_name: "scout",
            stats: (
                max_hp: 100,
                attack: 55,
                defense: 100,
                movement: 3,
                attack_range: (1, 1),
                vision: 2,
                can_capture: true,
                cost: 1000,
                class: Foot,
                max_stamina: 99,
                max_ammo: 0,
            ),
        ),
    }
)
```

### commanders.ron
```ron
(
    commanders: {
        "kira": (
            name: "Kira",
            faction: Eastern,
            personality: Aggressive,
            description: "A bold commander who leads from the front.",
            attack_bonus: 1.1,
            defense_bonus: 1.0,
            movement_bonus: 0,
            income_bonus: 1.0,
            vision_bonus: 0,
            cost_modifier: 1.0,
            terrain_cost_reduction: 0,
            power: (
                name: "Blitz",
                description: "All units gain +1 movement and +20% attack",
                effect: StatBoost(attack: 1.2, defense: 1.0, movement: 1),
            ),
            power_cost: 100,
        ),
    }
)
```

### terrain.ron
```ron
(
    terrain: {
        "grass": (
            name: "Grass",
            description: "Open grass - easy movement, no cover",
            defense: 1,
            movement_cost: 1,
            capturable: false,
            capture_points: 0,
            income: 0,
            color: [0.35, 0.55, 0.25],
            feature_height: 0.0,
            tile_height: 0.0,
            asset_name: "grass",
        ),
    }
)
```

## WASM vs Native

### WASM Builds
- All RON files embedded at compile time via `include_str!`
- Single self-contained artifact
- No runtime file loading
- No mod support

### Native Builds
- RON files embedded as defaults
- `mods/` directory scanned at startup
- Mod files override base game entries
- Multiple mods can be loaded (later mods override earlier)

## Rust Code Guidelines

### DO: Use GameData resource for lookups
```rust
fn display_unit(
    game_data: Res<GameData>,
    unit: &Unit,
) {
    // Good: moddable
    if let Some(data) = game_data.get_unit(unit.unit_type) {
        println!("{}", data.name);
    }
}
```

### AVOID: Hardcoded enum methods for display
```rust
// Avoid for user-facing strings:
match unit_type {
    UnitType::Scout => "Scout",  // Not moddable!
}
```

### Enum methods are OK for game logic
```rust
// OK: game mechanics that shouldn't change
impl UnitClass {
    pub fn can_traverse_water(&self) -> bool {
        matches!(self, UnitClass::Naval | UnitClass::Air)
    }
}
```

## Future: Scripting

For complex moddable behavior (CO powers, special abilities):
- CO power effects currently defined as enum variants in RON
- Could add Lua/Rhai scripting for custom effects
- Or: expand effect type vocabulary in Rust

## Mod Loading (Native)

Create a mod by adding a folder to `mods/`:

```
mods/
└── my-balance-mod/
    └── units.ron
```

The mod's `units.ron` only needs entries you want to override:
```ron
(
    units: {
        "scout": (
            name: "Scout",
            // ... override with buffed stats
            stats: (
                attack: 70,  // was 55
                // ... rest of stats
            ),
        ),
    }
)
```
