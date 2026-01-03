# Paw & Claw - Development Roadmap

## Current State (v0.1)

### Core Gameplay ✅
- Turn-based tactics combat
- 5 factions (4 main + Nether antagonist)
- 20 unit types (foot, vehicle, air, naval)
- 15 commanders with CO powers
- Fog of war
- Weather system
- AI opponents
- Save/load system

### Technical ✅
- Bevy 0.17 engine
- 3D board with 2D billboarded sprites
- bevy_egui for UI
- RON-based modding system

---

## Phase 1: Complete Moddability

### Data-Driven Refactor
- [ ] Replace hardcoded `Faction::name()` with `GameData` lookups
- [ ] Replace hardcoded `UnitType` stats with `GameData` lookups
- [ ] Replace hardcoded `Terrain` properties with `GameData` lookups
- [ ] Replace hardcoded `Commander` data with `GameData` lookups
- [ ] Externalize damage tables to RON
- [ ] Externalize movement cost tables to RON

### Map System
- [ ] Standardize map format as RON files
- [ ] Map editor improvements
- [ ] Custom map loading

---

## Phase 2: Polish & Content

### Visual Polish
- [ ] Unit sprites (currently procedural placeholders)
- [ ] Terrain tile sprites
- [ ] Commander portraits
- [ ] UI art pass
- [ ] Attack/damage animations
- [ ] Sound effects

### Gameplay Refinement
- [ ] Balance pass on all units
- [ ] Balance pass on all commanders
- [ ] Additional maps
- [ ] Tutorial/campaign mode

### Quality of Life
- [ ] Undo move (before confirming action)
- [ ] Quick save/load
- [ ] Game speed options
- [ ] Accessibility options

---

## Phase 3: Multiplayer (Future)

### Local Multiplayer
- [ ] Hot-seat mode (already partially works)
- [ ] Split-screen consideration

### Online Multiplayer
- [ ] Networking architecture
- [ ] Lobby/matchmaking
- [ ] Replay system

---

## Phase 4: Platform Expansion (Future)

### Web (WASM)
- [ ] WebGPU build target
- [ ] Touch controls for mobile browsers

### Desktop Distribution
- [ ] Steam release preparation
- [ ] Steam Deck optimization

### Console (Long-term)
- [ ] Nintendo Switch (requires developer program)
- [ ] Gamepad-first UX already in place
- [ ] See `.claude/portability.md`

---

## Technical Debt

### Bevy 0.17 Migration Cleanup
- [ ] Replace deprecated `EventReader` with `MessageReader`
- [ ] Replace deprecated `add_event` with `add_message`
- [ ] Update deprecated egui methods

### Code Quality
- [ ] Remove unused code (dead_code warnings)
- [ ] Add unit tests for game logic
- [ ] Integration tests for save/load

---

## Non-Goals (Out of Scope)

- Real-time combat (this is turn-based)
- 3D unit models (billboarded sprites are intentional)
- MMO features
- Microtransactions
