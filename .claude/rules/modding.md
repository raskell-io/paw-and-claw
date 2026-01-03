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
- `GameData` resource provides lookup methods
- Use `GameData::get_*()` methods for moddable values

## Guidelines

### For User-Facing Strings (names, descriptions)
```rust
// GOOD: Use GameData for moddable content
if let Some(data) = game_data.get_unit(unit.unit_type) {
    ui.label(&data.name);
}

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
