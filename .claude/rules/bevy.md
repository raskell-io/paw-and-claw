---
paths: src/**/*.rs
---

# Bevy Engine Guidelines

## Version
- Using Bevy 0.17
- bevy_egui 0.38 for UI

## ECS Patterns
- Systems should be small and focused on one responsibility
- Use Bevy's built-in scheduling (Update, FixedUpdate, etc.)
- Events/Messages for decoupled communication between systems
- Resources for global state

## UI Systems
- UI systems must run in `EguiPrimaryContextPass` schedule (not `Update`)
- Check `EguiWantsInput` before processing mouse clicks in game systems
- Always guard egui context access with `let Ok(ctx) = contexts.ctx_mut() else { return };`

## Common Patterns
```rust
// Query for entities with components
fn my_system(query: Query<&Transform, With<Player>>) { }

// Mutable access
fn update_system(mut query: Query<&mut Health>) { }

// Events (use MessageReader in 0.17)
fn handle_events(mut events: MessageReader<MyEvent>) { }
```

## Deprecations (0.17)
- `EventReader` -> `MessageReader`
- `add_event::<T>()` -> `add_message::<T>()`
- `Timer::finished()` -> `Timer::is_finished()`
