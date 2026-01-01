# Paw & Claw

A turn-based tactics game featuring animal factions from different continents battling for territory. High-resolution pixel art aesthetic with deep tactical gameplay.

## Concept

Animal kingdoms from across the world clash in strategic warfare. Each faction brings unique creatures and fighting styles based on their geographic origins.

### Factions

| Faction | Theme | Animals | Playstyle |
|---------|-------|---------|-----------|
| **Eastern Empire** | East Asian wildlife | Tanuki, Red Panda, Crane, Kitsune | Swarm tactics, quantity over quality |
| **Northern Realm** | European wildlife | Badger, Hedgehog, Stoat, Owl, Fox | Balanced forces, strong defense |
| **Southern Frontier** | American wildlife | Raccoon, Opossum, Coyote, Hawk | Guerrilla tactics, high mobility |
| **The Wanderer** | Lone wolf | Wolf | Single powerful agent |

### Units

| Unit | Class | Role | Cost |
|------|-------|------|------|
| Scout | Foot | Cheap infantry, captures buildings | 1000 |
| Shocktrooper | Foot | Heavy infantry, anti-armor | 3000 |
| Recon | Treads | Fast scout vehicle | 4000 |
| Ironclad | Treads | Main battle armor | 7000 |
| Siege | Treads | Long-range bombardment | 6000 |
| Skywing | Air | Versatile air unit | 9000 |
| Talon | Air | Heavy air striker | 12000 |
| Carrier | Transport | Moves foot units | 5000 |
| Supplier | Transport | Resupply and repair | 5000 |

### Terrain

- **Clearing** - Open ground, no bonuses
- **Forest** - Defense bonus, slows ground units
- **Mountain** - High defense, limited access
- **River** - Movement penalty
- **Road** - Fast movement
- **Base** - Spawn units, high defense
- **Village** - Capture for income

## Art Direction

**Style**: High-resolution pixel art (32x32 or 48x48 tiles)
- Hand-crafted pixel sprites with limited color palettes
- Faction-specific color schemes (Red/Gold for Eastern, Blue/Silver for Northern, Green/Brown for Southern)
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

- **Left-click**: Select unit
- **Right-click**: Move selected unit / Attack enemy
- **ESC**: Deselect / Cancel

## Tech Stack

- [Bevy](https://bevyengine.org/) - Game engine (Rust)
- [bevy_egui](https://github.com/mvlabat/bevy_egui) - UI
- WebAssembly for browser distribution

## License

MIT License

## Status

Early development - core mechanics prototype.
