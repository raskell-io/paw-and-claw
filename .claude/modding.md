# Modding Architecture

Goal: All game content moddable without recompiling Rust code.

## Implementation Status

### Completed ✅

**Data files** (`assets/data/`) are the single source of truth. There are no
hardcoded fallbacks in Rust; game logic, AI, rendering and UI all read `GameData`.
- `factions.ron` - 5 factions (Eastern, Northern, Western, Southern, Nether)
- `units.ron` - 20 unit types with full stats
- `terrain.ron` - 17 terrain types with properties (`builds` and `resupplies` list the unit classes a property makes and serves)
- `commanders.ron` - 28 commanders, one per Advance Wars 2 / Dual Strike CO, each with always-on abilities, a CO Power and a Super CO Power
- `damage_tables.ron` - base damage % per attacker/defender pair
- `movement_costs.ron` - movement cost per terrain/unit class

**Loading system (`src/game/modding.rs`):**
- RON parsing with `ron` crate
- WASM embedding via `include_str!` - single artifact build
- Native mod loading from `mods/` at startup
- `GameData::validate()` runs at startup and refuses to start if any enum
  variant lacks data, so lookups (`unit_stats`, `terrain`, `commander`, ...) never fail
- Tests in `modding.rs` check the embedded data stays complete and consistent

### Pending ⏳
- [ ] Maps as RON files

---

## Directory Structure

```
assets/
├── data/
│   ├── factions.ron       # ✅ Faction definitions
│   ├── commanders.ron     # ✅ CO stats and abilities
│   ├── units.ron          # ✅ Unit types and stats
│   ├── terrain.ron        # ✅ Terrain types and properties
│   ├── damage_tables.ron  # ✅ Attacker vs defender base damage
│   ├── movement_costs.ron # ✅ Terrain vs unit class movement cost
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
Each commander copies an Advance Wars CO's abilities (`based_on`). See the
header of the file for every modifier and effect.
```ron
(
    commanders: {
        "talon_co": (
            name: "Talon",
            faction: Western,
            personality: Aggressive,
            description: "Direct combat units are strong; indirect ones weak.",
            based_on: "Max",
            abilities: (attack: [(DirectNonFoot, 20), (Indirect, -10)], range: [(Indirect, -1)]),
            power: Some((
                name: "Dive",
                description: "Direct units +10% attack, +1 movement.",
                stars: 3,
                modifiers: (attack: [(DirectNonFoot, 10)], movement: [(DirectNonFoot, 1)]),
            )),
            super_power: (
                name: "Talon Strike",
                description: "Direct units +30% attack, +2 movement.",
                stars: 6,
                modifiers: (attack: [(DirectNonFoot, 30)], movement: [(DirectNonFoot, 2)]),
                effects: [],
            ),
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
            defense: 1,              // AW2 defense stars
            capturable: false,
            capture_points: 0,
            income: 0,               // Funds per turn when owned (same scale as unit cost)
            color: (0.55, 0.75, 0.35),
            feature_height: 0.0,     // 0 = no vertical feature sprite
            tile_height: 4.0,
            asset_name: "grass",
        ),
    }
)
```

Movement costs live in `movement_costs.ron`, not here.

### damage_tables.ron
```ron
(
    tables: {
        "scout": {           // attacker
            "scout": 55,     // defender: base damage %
            "ironclad": 5,
        },
    }
)
```
**A missing entry means the attacker cannot target that unit at all**: no attack
option, no counter-attack. Damage uses the AW2 formula in `combat.rs`.

### movement_costs.ron
```ron
(
    costs: {
        "pond": {            // terrain
            "foot": 99,      // unit class: movement points (99 = impassable)
            "naval": 1,
        },
    }
)
```
Every terrain must list every unit class (enforced by validation).

## WASM vs Native

### WASM Builds
- All RON files embedded at compile time via `include_str!`
- Single self-contained artifact
- No runtime file loading
- No mod support

### Native Builds
- RON files embedded as defaults
- `mods/` directory scanned at startup, mods applied in alphabetical folder order
  (later mods override earlier)
- Factions, units, terrain and commanders override **whole entries** (a unit
  entry must be complete, but can be brand new data for an existing key)
- Damage tables and movement costs merge **per row cell**, so a mod can change a
  single matchup
- Invalid mod files are skipped with a warning; incomplete data after merging
  stops the game at startup with a list of what's missing

## Rust Code Guidelines

### DO: Use GameData resource for lookups
```rust
fn display_unit(
    game_data: Res<GameData>,
    unit: &Unit,
) {
    // Good: moddable. Lookups are infallible once validation has passed.
    println!("{}", game_data.unit_name(unit.unit_type));
    let stats = game_data.unit_stats(unit.unit_type);
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

The mod's `units.ron` only needs the units you want to change, but each listed
unit must be a complete entry (copy it from `assets/data/units.ron` and edit):
```ron
(
    units: {
        "scout": (
            name: "Scout",
            // ... every field, with the changes you want
        ),
    }
)
```

A balance tweak to one matchup only needs that cell:
```ron
// mods/my-balance-mod/damage_tables.ron
(tables: { "scout": { "ironclad": 15 } })
```
