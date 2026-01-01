# Paw & Claw

A turn-based tactics game featuring animal factions from different continents battling for territory. High-resolution pixel art aesthetic with deep tactical gameplay.

## Concept

Animal kingdoms from across the world clash in strategic warfare. Each faction brings unique creatures and fighting styles based on their geographic origins.

### Factions

| Faction | Theme | Animals | Playstyle |
|---------|-------|---------|-----------|
| **Eastern Empire** | East Asian wildlife | Tanuki, Red Panda, Crane, Kitsune | Swarm tactics, quantity over quality |
| **Northern Realm** | European wildlife | Badger, Hedgehog, Stoat, Owl, Fox | Balanced forces, strong defense |
| **Western Frontier** | American wildlife | Raccoon, Opossum, Coyote, Hawk | Guerrilla tactics, high mobility |
| **Southern Pride** | African wildlife | Lion, Elephant, Rhino, Hyena, Cheetah | Raw power, territorial control |
| **The Wanderer** | Lone wolf | Wolf | Single powerful agent |

### Units

#### Foot Units
| Unit | Role | Cost |
|------|------|------|
| Scout | Cheap infantry, captures buildings | 1000 |
| Shocktrooper | Heavy infantry, anti-armor | 3000 |

#### Ground Vehicles
| Unit | Role | Cost |
|------|------|------|
| Recon | Fast scout, great on roads | 4000 |
| Ironclad | Main battle armor | 7000 |
| Juggernaut | Heavy armor, devastating firepower | 16000 |
| Behemoth | Super-heavy armor, ultimate ground unit | 22000 |
| Flak | Anti-air vehicle | 8000 |
| Siege | Medium-range artillery (2-3) | 6000 |
| Barrage | Long-range rockets (3-5) | 15000 |
| Stinger | Long-range anti-air missiles (3-5) | 12000 |

#### Ground Support
| Unit | Role | Cost |
|------|------|------|
| Carrier | Transports foot units | 5000 |
| Supplier | Resupply and repair | 5000 |

#### Air Units
| Unit | Role | Cost |
|------|------|------|
| Ferrier | Transport helicopter | 5000 |
| Skywing | Attack helicopter, versatile | 9000 |
| Raptor | Air superiority fighter | 20000 |
| Talon | Heavy bomber | 22000 |

#### Naval Units
| Unit | Role | Cost |
|------|------|------|
| Barge | Naval transport | 12000 |
| Frigate | Fast warship, anti-air/sub | 18000 |
| Lurker | Submarine, stealth attacks | 20000 |
| Dreadnought | Battleship, long-range (2-6) | 28000 |

### Terrain (Woodland Scale)

| Terrain | Defense | Move Cost | Description |
|---------|---------|-----------|-------------|
| **Grass** | 0 | 1 | Open ground, easy movement |
| **Tall Grass** | 1 | 1 | Some concealment |
| **Thicket** | 2 | 2 | Dense bushes, good cover |
| **Brambles** | 3 | 3 | Thorny, excellent defense |
| **Log** | 0 | 1 | Fallen tree, fast travel |
| **Boulder** | 4 | 3 | Large rocks, high defense |
| **Hollow** | 3 | 1 | Stump shelter, good cover |
| **Creek** | 0 | 2 | Shallow water crossing |
| **Pond** | 0 | - | Deep water, impassable |
| **Shore** | 0 | 2 | Muddy bank |
| **Base** | 4 | 1 | Fortified HQ, spawn point |
| **Outpost** | 2 | 1 | Capturable position |
| **Storehouse** | 1 | 1 | Supply cache, capturable |

## Art Direction

**Style**: High-resolution pixel art (32x32 or 48x48 tiles)
- Hand-crafted pixel sprites with limited color palettes
- Faction-specific color schemes (Red for Eastern, Blue for Northern, Green for Western, Gold for Southern)
- Expressive animal characters with distinct silhouettes
- Terrain tiles with rich detail and seasonal variations

## Game Modes

- **Skirmish** - 1v1 or 2v2 battles on custom maps
- **Roguelike** - Procedural campaign with persistent progression
- **Campaign** - Story-driven missions (planned)
- **Map Editor** - Create and share custom battlefields

## Building

### Native (Desktop)

```sh
cargo run
```

### WASM (Web)

```sh
# Install tools (first time only)
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli

# Build
cargo build --release --target wasm32-unknown-unknown

# Generate JS bindings
wasm-bindgen --out-dir ./web --target web \
    ./target/wasm32-unknown-unknown/release/paw_and_claw.wasm

# Serve locally
cd web && python3 -m http.server 8080
```

Then open http://localhost:8080

## Controls

### Mouse
- **Left-click unit**: Select unit (shows movement range)
- **Left-click tile**: Move selected unit
- **Left-click selected**: Deselect

### Keyboard
- **WASD / Arrows**: Pan camera
- **Shift + WASD/Arrows**: Move grid cursor
- **Space / Enter**: Select unit at cursor / Confirm move
- **ESC**: Deselect / Cancel

## Tech Stack

- [Bevy](https://bevyengine.org/) - Game engine (Rust)
- [bevy_egui](https://github.com/mvlabat/bevy_egui) - UI
- WebAssembly for browser distribution

## License

MIT License

## Status

Early development - core mechanics prototype.
