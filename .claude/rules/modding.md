---
paths: src/**/*.rs, assets/data/**/*.ron
---

# Modding System Rules

## Data Files
- Game data lives in `assets/data/*.ron`
- Use RON format (Rusty Object Notation)
- All RON files are embedded for WASM via `include_str!`

## GameData Resource
- `src/game/modding.rs` defines the modding system
- `GameData` resource is the single source of truth; lookups are infallible
  (`unit_stats`, `terrain`, `faction`, `commander`, `movement_cost`, ...)
- Never add hardcoded stats or names to `UnitType`/`Terrain`/`Faction`/`CommanderId`;
  pass `&GameData` into functions that need data
- Factions are cosmetic only (name, color, animals, sprites). Gameplay effects
  (prices, stats, powers) belong to commanders, as in Advance Wars

## Guidelines

### For User-Facing Strings (names, descriptions)
```rust
// GOOD: Use GameData for moddable content
ui.label(game_data.unit_name(unit.unit_type));

// AVOID: Hardcoded strings
ui.label(unit.unit_type.name());  // Not moddable!
```

### For Game Logic (non-moddable)
```rust
// OK: Enum methods for mechanics
impl UnitClass {
    pub fn is_ground(&self) -> bool {
        matches!(self, UnitClass::Foot | UnitClass::Wheels | UnitClass::Treads)
    }
}
```

## RON File Structure

All RON files use HashMap-style format:
```ron
(
    items: {
        "key": (
            field: value,
        ),
    }
)
```

## Serde Derives
When adding new types used in RON, add serde derives:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MyEnum { ... }
```
