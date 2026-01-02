use bevy::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

use super::{GameMap, GridPosition, Unit, FactionMember, TurnState, TurnPhase, AttackEvent, Tile, Terrain, TILE_SIZE, GameResult, Commanders, Weather};
use crate::states::GameState;

/// Convert screen position to grid coordinates using ray-plane intersection
/// Returns None if the ray doesn't hit the ground plane or is outside the map
pub fn screen_to_grid(
    window: &Window,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    map: &GameMap,
) -> Option<IVec2> {
    let cursor_pos = window.cursor_position()?;
    let ray = camera.viewport_to_world(camera_transform, cursor_pos).ok()?;

    // Intersect with Y=0 ground plane
    // Ray equation: P = origin + t * direction
    // Plane equation: Y = 0
    // Solve for t: origin.y + t * direction.y = 0
    if ray.direction.y.abs() < 0.0001 {
        return None; // Ray parallel to ground
    }
    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 {
        return None; // Intersection behind camera
    }
    let hit = ray.origin + ray.direction * t;

    // Convert world position to grid coordinates
    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0;

    let gx = ((hit.x - offset_x) / TILE_SIZE).floor() as i32;
    let gy = ((hit.z - offset_z) / TILE_SIZE).floor() as i32;  // World Z -> Grid Y

    if gx >= 0 && gx < map.width as i32 && gy >= 0 && gy < map.height as i32 {
        Some(IVec2::new(gx, gy))
    } else {
        None
    }
}

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementHighlights>()
            .init_resource::<GridCursor>()
            .init_resource::<PendingAction>()
            .init_resource::<ProductionState>()
            .init_resource::<CameraZoom>()
            .init_resource::<CameraAngle>()
            .add_systems(Update, (
                handle_keyboard_input,
                handle_camera_movement,
                handle_camera_zoom,
                handle_camera_angle_toggle,
                update_camera_angle,
                handle_click_input,
                update_movement_highlights,
                draw_grid_cursor,
                draw_action_targets,
            ).run_if(in_state(GameState::Battle)));
    }
}

/// Camera zoom settings
#[derive(Resource)]
pub struct CameraZoom {
    pub current: f32,
    pub min: f32,
    pub max: f32,
    pub speed: f32,
}

impl Default for CameraZoom {
    fn default() -> Self {
        Self {
            current: 1.0,
            min: 0.3,   // Zoomed in close
            max: 2.5,   // Zoomed out far
            speed: 0.15,
        }
    }
}

/// Camera angle mode - 3D perspective or 2D top-down
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CameraMode {
    #[default]
    Perspective,  // Angled 3D view (Advance Wars style)
    TopDown,      // Directly overhead 2D view
}

/// Resource to control camera angle with smooth transitions
#[derive(Resource)]
pub struct CameraAngle {
    pub mode: CameraMode,
    pub transition_progress: f32,  // 0.0 = perspective, 1.0 = top-down
    pub transition_speed: f32,
    /// Base height for perspective mode
    pub perspective_height: f32,
    pub perspective_pitch: f32,  // Angle in radians
    /// Base height for top-down mode
    pub top_down_height: f32,
    /// The point on the ground (XZ plane) that the camera is looking at
    pub look_at: Vec2,
}

impl Default for CameraAngle {
    fn default() -> Self {
        Self {
            mode: CameraMode::Perspective,
            transition_progress: 0.0,
            transition_speed: 3.0,  // Transition takes ~0.33 seconds
            perspective_height: 400.0,
            perspective_pitch: std::f32::consts::FRAC_PI_6,  // 30 degrees from horizontal
            top_down_height: 600.0,
            look_at: Vec2::ZERO,  // Start looking at center
        }
    }
}

/// Tracks a unit that has moved and is waiting for action selection
#[derive(Resource, Default)]
pub struct PendingAction {
    pub unit: Option<Entity>,
    pub targets: HashSet<Entity>,  // Enemies that can be attacked from new position
    pub can_capture: bool,         // Whether the unit can capture the tile it's on
    pub capture_tile: Option<Entity>, // The tile entity that can be captured
}

/// Tracks when production menu should be shown
#[derive(Resource, Default)]
pub struct ProductionState {
    pub active: bool,
    pub base_entity: Option<Entity>,
    pub base_position: (i32, i32),
}

/// Resource to track movement range and attack target highlights
#[derive(Resource, Default)]
pub struct MovementHighlights {
    pub tiles: HashSet<(i32, i32)>,
    pub attack_targets: HashSet<Entity>,  // Enemy units that can be attacked
    pub selected_unit: Option<Entity>,
}

/// Grid cursor for keyboard navigation
#[derive(Resource)]
pub struct GridCursor {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
}

impl Default for GridCursor {
    fn default() -> Self {
        Self {
            x: 5,
            y: 4,
            visible: false,
        }
    }
}

/// Calculate which enemies can be attacked from current position
pub fn calculate_attack_targets(
    attacker: &Unit,
    attacker_pos: &GridPosition,
    attacker_faction: &FactionMember,
    units: &[(Entity, GridPosition, FactionMember)],
) -> HashSet<Entity> {
    let mut targets = HashSet::new();
    let stats = attacker.unit_type.stats();

    // Skip if unit can't attack
    if stats.attack == 0 {
        return targets;
    }

    let (min_range, max_range) = stats.attack_range;

    for (entity, pos, faction) in units {
        // Can't attack own faction
        if faction.faction == attacker_faction.faction {
            continue;
        }

        let distance = attacker_pos.distance_to(pos);
        if distance >= min_range && distance <= max_range {
            targets.insert(*entity);
        }
    }

    targets
}

/// Calculate reachable tiles using BFS
pub fn calculate_movement_range(
    start: &GridPosition,
    movement: u32,
    map: &GameMap,
    units: &HashMap<(i32, i32), Entity>,
) -> HashSet<(i32, i32)> {
    let mut reachable = HashSet::new();
    let mut visited = HashMap::new();
    let mut queue = VecDeque::new();

    queue.push_back((start.x, start.y, 0u32));
    visited.insert((start.x, start.y), 0);

    let directions = [(0, 1), (0, -1), (1, 0), (-1, 0)];

    while let Some((x, y, cost)) = queue.pop_front() {
        if cost <= movement {
            reachable.insert((x, y));
        }

        for (dx, dy) in directions {
            let nx = x + dx;
            let ny = y + dy;

            if let Some(terrain) = map.get(nx, ny) {
                let move_cost = terrain.movement_cost();
                let new_cost = cost + move_cost;

                // Can't move through enemy units (simplified - just check if any unit is there)
                if units.contains_key(&(nx, ny)) && (nx, ny) != (start.x, start.y) {
                    continue;
                }

                if new_cost <= movement {
                    let should_visit = visited
                        .get(&(nx, ny))
                        .map(|&prev_cost| new_cost < prev_cost)
                        .unwrap_or(true);

                    if should_visit {
                        visited.insert((nx, ny), new_cost);
                        queue.push_back((nx, ny, new_cost));
                    }
                }
            }
        }
    }

    // Remove starting position from reachable (can't stay in place... or can we?)
    // Actually, let's keep it - unit can choose not to move
    reachable
}

/// Handle keyboard input for cursor movement and actions
fn handle_keyboard_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<GridCursor>,
    mut highlights: ResMut<MovementHighlights>,
    mut pending_action: ResMut<PendingAction>,
    mut turn_state: ResMut<TurnState>,
    mut units: Query<(Entity, &mut GridPosition, &mut Transform, &FactionMember, &mut Unit)>,
    tiles: Query<(Entity, &Tile)>,
    map: Res<GameMap>,
    mut attack_events: EventWriter<AttackEvent>,
    game_result: Res<GameResult>,
    commanders: Res<Commanders>,
    weather: Res<Weather>,
) {
    // Don't process input if game is over
    if game_result.game_over {
        return;
    }

    // If in Action phase, keyboard is handled by UI
    if turn_state.phase == TurnPhase::Action {
        // ESC cancels action and deselects (unit already moved, so mark as done)
        if keyboard.just_pressed(KeyCode::Escape) {
            if let Some(entity) = pending_action.unit {
                if let Ok((_, _, _, _, mut unit)) = units.get_mut(entity) {
                    unit.attacked = false; // Wait action
                }
            }
            pending_action.unit = None;
            pending_action.targets.clear();
            turn_state.phase = TurnPhase::Select;
        }
        return;
    }

    // Show cursor when using keyboard
    let mut moved_cursor = false;

    // Cursor movement with WASD or Arrow keys (with Shift for cursor, without for camera)
    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    if shift_held || cursor.visible {
        if keyboard.just_pressed(KeyCode::KeyW) || keyboard.just_pressed(KeyCode::ArrowUp) {
            cursor.y = (cursor.y + 1).min(map.height as i32 - 1);
            moved_cursor = true;
        }
        if keyboard.just_pressed(KeyCode::KeyS) || keyboard.just_pressed(KeyCode::ArrowDown) {
            cursor.y = (cursor.y - 1).max(0);
            moved_cursor = true;
        }
        if keyboard.just_pressed(KeyCode::KeyA) || keyboard.just_pressed(KeyCode::ArrowLeft) {
            cursor.x = (cursor.x - 1).max(0);
            moved_cursor = true;
        }
        if keyboard.just_pressed(KeyCode::KeyD) || keyboard.just_pressed(KeyCode::ArrowRight) {
            cursor.x = (cursor.x + 1).min(map.width as i32 - 1);
            moved_cursor = true;
        }
    }

    if moved_cursor {
        cursor.visible = true;
    }

    // ESC to deselect
    if keyboard.just_pressed(KeyCode::Escape) {
        highlights.selected_unit = None;
        highlights.tiles.clear();
        highlights.attack_targets.clear();
        cursor.visible = false;
        return;
    }

    // Space or Enter to select/move/attack
    if keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::Enter) {
        cursor.visible = true;

        if let Some(selected_entity) = highlights.selected_unit {
            // Check if cursor is on an attack target (before moving)
            let target_entity = units.iter()
                .find(|(e, p, _, _, _)| p.x == cursor.x && p.y == cursor.y && highlights.attack_targets.contains(e))
                .map(|(e, _, _, _, _)| e);

            if let Some(target) = target_entity {
                // Attack!
                attack_events.send(AttackEvent {
                    attacker: selected_entity,
                    defender: target,
                });

                // Mark as attacked and clear selection
                if let Ok((_, _, _, _, mut unit)) = units.get_mut(selected_entity) {
                    unit.attacked = true;
                    unit.moved = true;
                }
                highlights.selected_unit = None;
                highlights.tiles.clear();
                highlights.attack_targets.clear();
                return;
            }

            // Try to move to cursor position
            if highlights.tiles.contains(&(cursor.x, cursor.y)) {
                // Check we're not moving onto another unit
                let target_occupied = units.iter()
                    .any(|(e, p, _, _, _)| e != selected_entity && p.x == cursor.x && p.y == cursor.y);

                if !target_occupied {
                    let new_pos = GridPosition::new(cursor.x, cursor.y);

                    // Move the unit
                    let faction_copy;
                    let unit_copy;
                    if let Ok((_, mut grid_pos, mut transform, faction, mut unit)) = units.get_mut(selected_entity) {
                        grid_pos.x = cursor.x;
                        grid_pos.y = cursor.y;
                        transform.translation = grid_pos.to_world(&map);
                        unit.moved = true;
                        faction_copy = faction.clone();
                        unit_copy = unit.clone();
                        info!("Moved unit to ({}, {})", cursor.x, cursor.y);
                    } else {
                        return;
                    }

                    // Calculate attack targets from new position
                    let all_units: Vec<_> = units.iter()
                        .map(|(e, p, _, f, _)| (e, p.clone(), f.clone()))
                        .collect();
                    let targets = calculate_attack_targets(&unit_copy, &new_pos, &faction_copy, &all_units);

                    // Check if unit can capture the tile it moved to
                    let (can_capture, capture_tile) = if unit_copy.unit_type.stats().can_capture {
                        tiles.iter()
                            .find(|(_, t)| t.position.x == cursor.x && t.position.y == cursor.y)
                            .map(|(e, tile)| {
                                let capturable = tile.terrain.is_capturable() && tile.owner != Some(faction_copy.faction);
                                (capturable, if capturable { Some(e) } else { None })
                            })
                            .unwrap_or((false, None))
                    } else {
                        (false, None)
                    };

                    highlights.selected_unit = None;
                    highlights.tiles.clear();
                    highlights.attack_targets.clear();

                    // Enter Action phase if there are targets or can capture
                    if (!targets.is_empty() && !unit_copy.attacked) || can_capture {
                        pending_action.unit = Some(selected_entity);
                        pending_action.targets = targets;
                        pending_action.can_capture = can_capture;
                        pending_action.capture_tile = capture_tile;
                        turn_state.phase = TurnPhase::Action;
                        info!("Entering action phase: {} targets, can_capture: {}", pending_action.targets.len(), can_capture);
                    }
                }
            }
        } else {
            // Try to select unit at cursor
            let mut found_unit = None;
            for (entity, pos, _, faction, unit) in units.iter() {
                if pos.x == cursor.x && pos.y == cursor.y
                    && !unit.moved
                    && faction.faction == turn_state.current_faction
                {
                    let unit_positions: HashMap<(i32, i32), Entity> = units
                        .iter()
                        .map(|(e, p, _, _, _)| ((p.x, p.y), e))
                        .collect();
                    let stats = unit.unit_type.stats();
                    let co_bonuses = commanders.get_bonuses(turn_state.current_faction);
                    let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
                    let total_movement = weather.apply_movement(base_movement);
                    let tiles = calculate_movement_range(&pos, total_movement, &map, &unit_positions);

                    // Calculate attack targets
                    let all_units: Vec<_> = units.iter()
                        .map(|(e, p, _, f, _)| (e, p.clone(), f.clone()))
                        .collect();
                    let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units);

                    found_unit = Some((entity, tiles, attack_targets));
                    break;
                }
            }
            if let Some((entity, tiles, attack_targets)) = found_unit {
                highlights.selected_unit = Some(entity);
                highlights.tiles = tiles;
                highlights.attack_targets = attack_targets;
                info!("Selected unit at ({}, {})", cursor.x, cursor.y);
            }
        }
    }
}

/// Handle camera panning with WASD/Arrow keys (when shift not held and cursor not active)
/// Camera moves on XZ plane in 3D space
fn handle_camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    cursor: Res<GridCursor>,
    mut angle: ResMut<CameraAngle>,
) {
    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // Only pan camera when cursor not visible and shift not held
    if cursor.visible || shift_held {
        return;
    }

    let speed = 300.0 * time.delta_secs();

    let mut movement = Vec2::ZERO;

    // In top-down mode, up/down map to -Z/+Z directly
    // In perspective mode, "up" moves the look-at point away (negative Z)
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        movement.y -= 1.0;  // Move look-at point "up" (negative Z in world)
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        movement.y += 1.0;  // Move look-at point "down" (positive Z in world)
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        movement.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        movement.x += 1.0;
    }

    if movement != Vec2::ZERO {
        angle.look_at += movement.normalize() * speed;
    }
}

/// Handle camera zoom with scroll wheel, touchpad pinch, and keyboard (Q/E or +/-)
/// Only updates zoom.current - actual camera positioning is done by update_camera_angle
fn handle_camera_zoom(
    mut scroll_events: EventReader<bevy::input::mouse::MouseWheel>,
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut zoom: ResMut<CameraZoom>,
) {
    let mut zoom_delta = 0.0;

    // Mouse wheel / touchpad scroll (handles both line and pixel scrolling)
    for event in scroll_events.read() {
        // Touchpad pinch gestures come through as MouseScrollUnit::Pixel
        // Mouse wheel comes through as MouseScrollUnit::Line
        let scroll_amount = match event.unit {
            bevy::input::mouse::MouseScrollUnit::Line => event.y * 3.0,
            bevy::input::mouse::MouseScrollUnit::Pixel => event.y * 0.05,
        };
        zoom_delta -= scroll_amount * zoom.speed;
    }

    // Keyboard zoom (Q to zoom in, E to zoom out)
    let keyboard_speed = 2.0 * time.delta_secs();
    if keyboard.pressed(KeyCode::KeyQ) || keyboard.pressed(KeyCode::Equal) || keyboard.pressed(KeyCode::NumpadAdd) {
        zoom_delta -= keyboard_speed;
    }
    if keyboard.pressed(KeyCode::KeyE) || keyboard.pressed(KeyCode::Minus) || keyboard.pressed(KeyCode::NumpadSubtract) {
        zoom_delta += keyboard_speed;
    }

    if zoom_delta.abs() > 0.0001 {
        // Update zoom level with clamping - camera position updated by update_camera_angle
        zoom.current = (zoom.current + zoom_delta).clamp(zoom.min, zoom.max);
    }
}

/// Handle keyboard toggle for camera angle (T key or Tab)
fn handle_camera_angle_toggle(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut angle: ResMut<CameraAngle>,
) {
    // Toggle with T or Tab key
    if keyboard.just_pressed(KeyCode::KeyT) || keyboard.just_pressed(KeyCode::Tab) {
        angle.mode = match angle.mode {
            CameraMode::Perspective => CameraMode::TopDown,
            CameraMode::TopDown => CameraMode::Perspective,
        };
        angle.look_at = Vec2::ZERO;  // Re-center on toggle
        info!("Camera mode: {:?}", angle.mode);
    }
}

/// Smoothly transition camera between perspective and top-down views
/// Also handles zoom by adjusting camera height
fn update_camera_angle(
    time: Res<Time>,
    mut angle: ResMut<CameraAngle>,
    zoom: Res<CameraZoom>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(mut camera_transform) = camera_query.get_single_mut() else { return };

    // Update transition progress based on mode
    let target = match angle.mode {
        CameraMode::Perspective => 0.0,
        CameraMode::TopDown => 1.0,
    };

    let delta = time.delta_secs() * angle.transition_speed;
    if (angle.transition_progress - target).abs() > 0.001 {
        if angle.transition_progress < target {
            angle.transition_progress = (angle.transition_progress + delta).min(target);
        } else {
            angle.transition_progress = (angle.transition_progress - delta).max(target);
        }
    } else {
        angle.transition_progress = target;
    }

    // Smooth easing function (smoothstep)
    let t = angle.transition_progress;
    let t_smooth = t * t * (3.0 - 2.0 * t);

    // Interpolate between perspective and top-down
    // Calculate target pitch (0 = horizontal, PI/2 = looking straight down)
    let perspective_pitch = angle.perspective_pitch;
    let top_down_pitch = std::f32::consts::FRAC_PI_2 - 0.001;  // Nearly straight down

    let current_pitch = perspective_pitch + (top_down_pitch - perspective_pitch) * t_smooth;

    // Calculate height based on mode transition and zoom
    let base_height = angle.perspective_height + (angle.top_down_height - angle.perspective_height) * t_smooth;
    let height = base_height * zoom.current;

    // Calculate Z offset for perspective mode (camera behind the look-at point)
    // In top-down (t=1), Z offset is 0; in perspective (t=0), camera is behind
    let z_offset = (height / perspective_pitch.tan()) * (1.0 - t_smooth);

    // Position camera based on look_at point
    camera_transform.translation = Vec3::new(
        angle.look_at.x,
        height,
        angle.look_at.y + z_offset,
    );

    // Update rotation: pitch around X axis
    camera_transform.rotation = Quat::from_euler(
        EulerRot::XYZ,
        -current_pitch,  // Pitch (tilt down)
        0.0,             // Yaw (no horizontal rotation)
        0.0,             // Roll
    );
}

/// Handle mouse click for selection and movement
fn handle_click_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut units: Query<(Entity, &mut GridPosition, &mut Transform, &FactionMember, &mut Unit)>,
    tiles: Query<(Entity, &Tile)>,
    mut highlights: ResMut<MovementHighlights>,
    mut pending_action: ResMut<PendingAction>,
    mut production_state: ResMut<ProductionState>,
    mut cursor: ResMut<GridCursor>,
    mut turn_state: ResMut<TurnState>,
    map: Res<GameMap>,
    mut attack_events: EventWriter<AttackEvent>,
    game_result: Res<GameResult>,
    commanders: Res<Commanders>,
    weather: Res<Weather>,
) {
    // Don't process input if game is over
    if game_result.game_over {
        return;
    }

    // In Action phase, clicks on targets are handled here
    if turn_state.phase == TurnPhase::Action {
        if !mouse_button.just_pressed(MouseButton::Left) {
            return;
        }

        let window = windows.single();
        let (camera, camera_transform) = cameras.single();

        let Some(grid_pos) = screen_to_grid(window, camera, camera_transform, &map) else {
            return;
        };
        let grid_x = grid_pos.x;
        let grid_y = grid_pos.y;

        // Check if clicking on an attack target
        if let Some(acting_entity) = pending_action.unit {
            let target_entity = units.iter()
                .find(|(e, p, _, _, _)| p.x == grid_x && p.y == grid_y && pending_action.targets.contains(e))
                .map(|(e, _, _, _, _)| e);

            if let Some(target) = target_entity {
                // Attack!
                attack_events.send(AttackEvent {
                    attacker: acting_entity,
                    defender: target,
                });

                if let Ok((_, _, _, _, mut unit)) = units.get_mut(acting_entity) {
                    unit.attacked = true;
                }

                pending_action.unit = None;
                pending_action.targets.clear();
                turn_state.phase = TurnPhase::Select;
                info!("Attacked from action menu");
            }
        }
        return;
    }

    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    let window = windows.single();
    let (camera, camera_transform) = cameras.single();

    let Some(grid_pos) = screen_to_grid(window, camera, camera_transform, &map) else {
        return;
    };
    let grid_x = grid_pos.x;
    let grid_y = grid_pos.y;

    // Update cursor position to click location
    cursor.x = grid_x;
    cursor.y = grid_y;
    cursor.visible = false; // Hide keyboard cursor when using mouse

    // If we have a selected unit, try to attack or move
    if let Some(selected_entity) = highlights.selected_unit {
        // Check if clicking on an attack target (before moving)
        let target_entity = units.iter()
            .find(|(e, p, _, _, _)| p.x == grid_x && p.y == grid_y && highlights.attack_targets.contains(e))
            .map(|(e, _, _, _, _)| e);

        if let Some(target) = target_entity {
            // Attack!
            attack_events.send(AttackEvent {
                attacker: selected_entity,
                defender: target,
            });

            // Mark as attacked and clear selection
            if let Ok((_, _, _, _, mut unit)) = units.get_mut(selected_entity) {
                unit.attacked = true;
                unit.moved = true;
            }
            highlights.selected_unit = None;
            highlights.tiles.clear();
            highlights.attack_targets.clear();
            return;
        }

        if highlights.tiles.contains(&(grid_x, grid_y)) {
            // Check if clicking on the same unit (deselect) or empty tile (move)
            let clicking_on_selected = units.iter()
                .any(|(e, p, _, _, _)| e == selected_entity && p.x == grid_x && p.y == grid_y);

            if clicking_on_selected {
                // Clicking on selected unit - deselect
                highlights.selected_unit = None;
                highlights.tiles.clear();
                highlights.attack_targets.clear();
                return;
            }

            // Check if clicking on another friendly unit (switch selection)
            let mut switch_to = None;
            for (entity, pos, _, faction, unit) in units.iter() {
                if pos.x == grid_x && pos.y == grid_y
                    && !unit.moved
                    && faction.faction == turn_state.current_faction
                    && entity != selected_entity
                {
                    let unit_positions: HashMap<(i32, i32), Entity> = units
                        .iter()
                        .map(|(e, p, _, _, _)| ((p.x, p.y), e))
                        .collect();
                    let stats = unit.unit_type.stats();
                    let co_bonuses = commanders.get_bonuses(turn_state.current_faction);
                    let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
                    let total_movement = weather.apply_movement(base_movement);
                    let tiles = calculate_movement_range(&pos, total_movement, &map, &unit_positions);

                    // Calculate attack targets
                    let all_units: Vec<_> = units.iter()
                        .map(|(e, p, _, f, _)| (e, p.clone(), f.clone()))
                        .collect();
                    let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units);

                    switch_to = Some((entity, tiles, attack_targets));
                    break;
                }
            }

            if let Some((entity, tiles, attack_targets)) = switch_to {
                highlights.selected_unit = Some(entity);
                highlights.tiles = tiles;
                highlights.attack_targets = attack_targets;
                info!("Switched selection to unit at ({}, {})", grid_x, grid_y);
                return;
            }

            // Move the unit
            let new_pos = GridPosition::new(grid_x, grid_y);
            let faction_copy;
            let unit_copy;
            if let Ok((_, mut grid_pos, mut transform, faction, mut unit)) = units.get_mut(selected_entity) {
                grid_pos.x = grid_x;
                grid_pos.y = grid_y;
                transform.translation = grid_pos.to_world(&map);
                unit.moved = true;
                faction_copy = faction.clone();
                unit_copy = unit.clone();
                info!("Moved unit to ({}, {})", grid_x, grid_y);
            } else {
                return;
            }

            // Calculate attack targets from new position
            let all_units: Vec<_> = units.iter()
                .map(|(e, p, _, f, _)| (e, p.clone(), f.clone()))
                .collect();
            let targets = calculate_attack_targets(&unit_copy, &new_pos, &faction_copy, &all_units);

            // Check if unit can capture the tile it moved to
            let (can_capture, capture_tile) = if unit_copy.unit_type.stats().can_capture {
                tiles.iter()
                    .find(|(_, t)| t.position.x == grid_x && t.position.y == grid_y)
                    .map(|(e, tile)| {
                        let capturable = tile.terrain.is_capturable() && tile.owner != Some(faction_copy.faction);
                        (capturable, if capturable { Some(e) } else { None })
                    })
                    .unwrap_or((false, None))
            } else {
                (false, None)
            };

            highlights.selected_unit = None;
            highlights.tiles.clear();
            highlights.attack_targets.clear();

            // Enter Action phase if there are targets or can capture
            if (!targets.is_empty() && !unit_copy.attacked) || can_capture {
                pending_action.unit = Some(selected_entity);
                pending_action.targets = targets;
                pending_action.can_capture = can_capture;
                pending_action.capture_tile = capture_tile;
                turn_state.phase = TurnPhase::Action;
                info!("Entering action phase: {} targets, can_capture: {}", pending_action.targets.len(), can_capture);
            }
            return;
        }
    }

    // No unit selected or clicked outside movement range - try to select a unit
    let mut select_unit = None;
    for (entity, pos, _, faction, unit) in units.iter() {
        if pos.x == grid_x && pos.y == grid_y
            && !unit.moved
            && faction.faction == turn_state.current_faction
        {
            let unit_positions: HashMap<(i32, i32), Entity> = units
                .iter()
                .map(|(e, p, _, _, _)| ((p.x, p.y), e))
                .collect();
            let stats = unit.unit_type.stats();
            let co_bonuses = commanders.get_bonuses(turn_state.current_faction);
            let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
            let total_movement = weather.apply_movement(base_movement);
            let move_tiles = calculate_movement_range(&pos, total_movement, &map, &unit_positions);

            // Calculate attack targets
            let all_units: Vec<_> = units.iter()
                .map(|(e, p, _, f, _)| (e, p.clone(), f.clone()))
                .collect();
            let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units);

            select_unit = Some((entity, move_tiles, attack_targets, pos.x, pos.y));
            break;
        }
    }

    if let Some((entity, tiles, attack_targets, x, y)) = select_unit {
        highlights.selected_unit = Some(entity);
        highlights.tiles = tiles;
        highlights.attack_targets = attack_targets;
        info!("Selected unit at ({}, {}), can reach {} tiles", x, y, highlights.tiles.len());
        return;
    }

    // Check if clicking on an owned base with no unit - open production menu
    let unit_positions: HashSet<(i32, i32)> = units
        .iter()
        .map(|(_, p, _, _, _)| (p.x, p.y))
        .collect();

    for (tile_entity, tile) in tiles.iter() {
        if tile.position.x == grid_x && tile.position.y == grid_y
            && tile.terrain == Terrain::Base
            && tile.owner == Some(turn_state.current_faction)
            && !unit_positions.contains(&(grid_x, grid_y))
        {
            production_state.active = true;
            production_state.base_entity = Some(tile_entity);
            production_state.base_position = (grid_x, grid_y);
            info!("Opened production menu at base ({}, {})", grid_x, grid_y);
            return;
        }
    }

    // Clicked on empty space or enemy unit - deselect
    highlights.selected_unit = None;
    highlights.tiles.clear();
    highlights.attack_targets.clear();
    production_state.active = false;
}

fn update_movement_highlights(
    highlights: Res<MovementHighlights>,
    mut gizmos: Gizmos,
    map: Res<GameMap>,
    units: Query<(&GridPosition, &FactionMember)>,
) {
    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Rotation to lay the rectangle flat on the XZ plane
    let flat_rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    // Draw movement range (blue) - multiple layers for visibility
    for &(x, y) in &highlights.tiles {
        let world_x = x as f32 * TILE_SIZE + offset_x;
        let world_z = y as f32 * TILE_SIZE + offset_z;

        // Outer border (bright)
        gizmos.rect(
            Isometry3d::new(Vec3::new(world_x, 0.02, world_z), flat_rotation),
            Vec2::splat(TILE_SIZE - 2.0),
            Color::srgba(0.3, 0.7, 1.0, 0.9),
        );
        // Middle fill
        gizmos.rect(
            Isometry3d::new(Vec3::new(world_x, 0.03, world_z), flat_rotation),
            Vec2::splat(TILE_SIZE - 6.0),
            Color::srgba(0.2, 0.5, 0.9, 0.7),
        );
        // Inner highlight
        gizmos.rect(
            Isometry3d::new(Vec3::new(world_x, 0.04, world_z), flat_rotation),
            Vec2::splat(TILE_SIZE - 10.0),
            Color::srgba(0.4, 0.8, 1.0, 0.5),
        );
    }

    // Draw attack targets (red)
    for target_entity in &highlights.attack_targets {
        if let Ok((pos, _)) = units.get(*target_entity) {
            let world_x = pos.x as f32 * TILE_SIZE + offset_x;
            let world_z = pos.y as f32 * TILE_SIZE + offset_z;

            // Red border for attack target
            gizmos.rect(
                Isometry3d::new(Vec3::new(world_x, 0.05, world_z), flat_rotation),
                Vec2::splat(TILE_SIZE - 2.0),
                Color::srgba(1.0, 0.2, 0.2, 0.8),
            );
            // Inner red highlight
            gizmos.rect(
                Isometry3d::new(Vec3::new(world_x, 0.06, world_z), flat_rotation),
                Vec2::splat(TILE_SIZE - 6.0),
                Color::srgba(1.0, 0.3, 0.3, 0.4),
            );
        }
    }
}

/// Draw the grid cursor when using keyboard navigation
fn draw_grid_cursor(
    cursor: Res<GridCursor>,
    mut gizmos: Gizmos,
    map: Res<GameMap>,
) {
    if !cursor.visible {
        return;
    }

    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    let world_x = cursor.x as f32 * TILE_SIZE + offset_x;
    let world_z = cursor.y as f32 * TILE_SIZE + offset_z;

    // Rotation to lay the rectangle flat on the XZ plane
    let flat_rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    // Draw cursor outline
    gizmos.rect(
        Isometry3d::new(Vec3::new(world_x, 0.08, world_z), flat_rotation),
        Vec2::splat(TILE_SIZE - 2.0),
        Color::srgb(1.0, 1.0, 0.0), // Yellow cursor
    );

    // Draw inner highlight
    gizmos.rect(
        Isometry3d::new(Vec3::new(world_x, 0.09, world_z), flat_rotation),
        Vec2::splat(TILE_SIZE - 6.0),
        Color::srgba(1.0, 1.0, 0.0, 0.3),
    );
}

/// Draw highlights for attack targets during Action phase
fn draw_action_targets(
    pending_action: Res<PendingAction>,
    turn_state: Res<TurnState>,
    mut gizmos: Gizmos,
    map: Res<GameMap>,
    units: Query<&GridPosition>,
) {
    // Only draw during Action phase
    if turn_state.phase != TurnPhase::Action {
        return;
    }

    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Rotation to lay the rectangle flat on the XZ plane
    let flat_rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);

    // Draw attack targets with pulsing red highlight
    for target_entity in &pending_action.targets {
        if let Ok(pos) = units.get(*target_entity) {
            let world_x = pos.x as f32 * TILE_SIZE + offset_x;
            let world_z = pos.y as f32 * TILE_SIZE + offset_z;

            // Bright red outer border
            gizmos.rect(
                Isometry3d::new(Vec3::new(world_x, 0.10, world_z), flat_rotation),
                Vec2::splat(TILE_SIZE - 2.0),
                Color::srgba(1.0, 0.1, 0.1, 0.95),
            );
            // Middle red
            gizmos.rect(
                Isometry3d::new(Vec3::new(world_x, 0.11, world_z), flat_rotation),
                Vec2::splat(TILE_SIZE - 6.0),
                Color::srgba(1.0, 0.2, 0.2, 0.7),
            );
            // Inner highlight
            gizmos.rect(
                Isometry3d::new(Vec3::new(world_x, 0.12, world_z), flat_rotation),
                Vec2::splat(TILE_SIZE - 10.0),
                Color::srgba(1.0, 0.4, 0.4, 0.5),
            );
        }
    }
}
