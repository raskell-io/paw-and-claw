use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::render::mesh::Mesh3d;
use bevy::pbr::MeshMaterial3d;
use std::collections::{HashMap, HashSet, VecDeque};

use super::{GameMap, GridPosition, Unit, FactionMember, TurnState, TurnPhase, AttackEvent, Tile, Terrain, TILE_SIZE, GameResult, Commanders, Weather, UnitAnimation, Faction};
use crate::states::GameState;

/// Bundled read-only game state for systems with many parameters
#[derive(SystemParam)]
pub struct GameStateContext<'w> {
    pub game_result: Res<'w, GameResult>,
    pub commanders: Res<'w, Commanders>,
    pub weather: Res<'w, Weather>,
}

/// Marker component for movement highlight mesh entities
#[derive(Component)]
pub struct MovementHighlightMesh;

/// Marker component for attack target highlight mesh entities
#[derive(Component)]
pub struct AttackHighlightMesh;

/// Marker component for path indicator mesh entities
#[derive(Component)]
pub struct PathIndicatorMesh;

/// Input mode for controlling units
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Resource)]
pub enum InputMode {
    #[default]
    Auto,       // Automatically detect based on last input
    Mouse,      // Mouse/pointer based controls
    Keyboard,   // WASD/Arrow key controls
    Touch,      // Touch screen controls (for mobile/tablet)
}

impl InputMode {
    pub fn name(&self) -> &'static str {
        match self {
            InputMode::Auto => "Auto-Detect",
            InputMode::Mouse => "Mouse",
            InputMode::Keyboard => "Keyboard",
            InputMode::Touch => "Touch",
        }
    }

    pub fn cycle(&self) -> InputMode {
        match self {
            InputMode::Auto => InputMode::Mouse,
            InputMode::Mouse => InputMode::Keyboard,
            InputMode::Keyboard => InputMode::Touch,
            InputMode::Touch => InputMode::Auto,
        }
    }
}

/// Resource to track the movement path being drawn
#[derive(Resource, Default)]
pub struct MovementPath {
    /// The path as a list of grid positions (starting from unit position)
    pub path: Vec<IVec2>,
    /// Whether the player is currently drawing a path
    pub drawing: bool,
    /// Total movement cost of the current path
    pub total_cost: u32,
    /// Whether mouse/touch is currently held down for dragging
    pub dragging: bool,
}

impl MovementPath {
    pub fn clear(&mut self) {
        self.path.clear();
        self.drawing = false;
        self.total_cost = 0;
        self.dragging = false;
    }

    pub fn start(&mut self, start_pos: IVec2) {
        self.path.clear();
        self.path.push(start_pos);
        self.drawing = true;
        self.total_cost = 0;
    }

    /// Try to extend the path to the given position
    /// Returns true if successful, false if invalid move
    pub fn try_extend(&mut self, pos: IVec2, map: &GameMap, valid_tiles: &HashSet<(i32, i32)>, tile_costs: &HashMap<(i32, i32), u32>) -> bool {
        if self.path.is_empty() {
            return false;
        }

        // Check if position is in valid movement range
        if !valid_tiles.contains(&(pos.x, pos.y)) {
            return false;
        }

        // Check if already in path (backtracking)
        if let Some(idx) = self.path.iter().position(|&p| p == pos) {
            // Truncate path to this position (backtrack)
            self.path.truncate(idx + 1);
            self.recalculate_cost(map);
            return true;
        }

        // Check if adjacent to last position
        let last = *self.path.last().unwrap();
        let dx = (pos.x - last.x).abs();
        let dy = (pos.y - last.y).abs();

        if (dx == 1 && dy == 0) || (dx == 0 && dy == 1) {
            // Adjacent - add to path
            self.path.push(pos);
            self.total_cost += tile_costs.get(&(pos.x, pos.y)).copied().unwrap_or(1);
            return true;
        }

        false
    }

    fn recalculate_cost(&mut self, map: &GameMap) {
        self.total_cost = 0;
        for pos in self.path.iter().skip(1) {  // Skip starting position
            let terrain = map.get(pos.x, pos.y).unwrap_or(Terrain::Grass);
            self.total_cost += terrain.movement_cost();
        }
    }

    /// Get the final destination of the path
    pub fn destination(&self) -> Option<IVec2> {
        self.path.last().copied()
    }

    /// Check if path has more than just the starting position
    pub fn has_movement(&self) -> bool {
        self.path.len() > 1
    }
}

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
            .init_resource::<InputMode>()
            .init_resource::<MovementPath>()
            // Split systems into smaller groups to avoid tuple size limits
            .add_systems(Update, (
                handle_keyboard_input,
                handle_keyboard_path_drawing,
                handle_camera_movement,
                handle_camera_zoom,
                handle_camera_angle_toggle,
                update_camera_angle,
            ).run_if(in_state(GameState::Battle)))
            .add_systems(Update, (
                handle_click_input,
                handle_path_drawing,
                spawn_movement_highlight_meshes,
                spawn_path_indicator_meshes,
            ).run_if(in_state(GameState::Battle)))
            .add_systems(Update, (
                draw_grid_cursor,
                draw_action_targets,
                draw_resource_warnings,
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
            perspective_pitch: std::f32::consts::FRAC_PI_3,  // 60 degrees from horizontal (more top-down)
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
    pub can_join: bool,            // Whether the unit can join another unit
    pub join_target: Option<Entity>, // The unit entity that can be joined
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
    pub tile_costs: HashMap<(i32, i32), u32>,  // Cost to reach each tile (for stamina)
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
/// Movement is capped by both the movement stat and available stamina
pub fn calculate_movement_range(
    start: &GridPosition,
    movement: u32,
    map: &GameMap,
    units: &HashMap<(i32, i32), Entity>,
) -> HashSet<(i32, i32)> {
    let (reachable, _) = calculate_movement_range_with_costs(start, movement, map, units);
    reachable
}

/// Unit info for movement calculation
pub struct UnitInfo {
    pub entity: Entity,
    pub faction: Faction,
    pub unit_type: super::UnitType,
    pub hp: i32,
}

/// Calculate reachable tiles and their costs using BFS
/// Returns (reachable tiles, cost to reach each tile)
/// Now takes UnitInfo to allow moving onto friendly same-type units for joining
pub fn calculate_movement_range_with_costs(
    start: &GridPosition,
    movement: u32,
    map: &GameMap,
    units: &HashMap<(i32, i32), Entity>,
) -> (HashSet<(i32, i32)>, HashMap<(i32, i32), u32>) {
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

                // Can't move through units (but can stop on friendly same-type for joining)
                // For now, just block all movement through occupied tiles
                // The join check happens at the destination
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
    (reachable, visited)
}

/// Calculate reachable tiles including those occupied by joinable friendly units
pub fn calculate_movement_range_with_joins(
    start: &GridPosition,
    movement: u32,
    map: &GameMap,
    moving_unit_type: super::UnitType,
    moving_faction: Faction,
    all_units: &[(Entity, (i32, i32), Faction, super::UnitType)],
) -> (HashSet<(i32, i32)>, HashMap<(i32, i32), u32>) {
    let mut reachable = HashSet::new();
    let mut visited = HashMap::new();
    let mut queue = VecDeque::new();

    // Build a set of blocked positions (enemy units)
    // and joinable positions (friendly same-type units)
    let mut blocked: HashSet<(i32, i32)> = HashSet::new();
    let mut joinable: HashSet<(i32, i32)> = HashSet::new();

    for (_, (x, y), faction, unit_type) in all_units {
        if (*x, *y) == (start.x, start.y) {
            continue; // Skip self
        }
        if *faction == moving_faction && *unit_type == moving_unit_type {
            joinable.insert((*x, *y));
        } else {
            blocked.insert((*x, *y));
        }
    }

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

                // Can't move through blocked tiles (enemy units)
                if blocked.contains(&(nx, ny)) {
                    continue;
                }

                // Can move onto joinable tiles, but can't move THROUGH them
                let is_joinable = joinable.contains(&(nx, ny));

                if new_cost <= movement {
                    let should_visit = visited
                        .get(&(nx, ny))
                        .map(|&prev_cost| new_cost < prev_cost)
                        .unwrap_or(true);

                    if should_visit {
                        visited.insert((nx, ny), new_cost);
                        // If this is a joinable tile, add it to reachable but don't continue BFS from it
                        if !is_joinable {
                            queue.push_back((nx, ny, new_cost));
                        }
                    }
                }
            }
        }
    }

    (reachable, visited)
}

/// Calculate effective movement range (limited by stamina)
pub fn effective_movement(base_movement: u32, stamina: u32) -> u32 {
    base_movement.min(stamina)
}

/// Handle keyboard input for cursor movement and actions
fn handle_keyboard_input(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<GridCursor>,
    mut highlights: ResMut<MovementHighlights>,
    mut pending_action: ResMut<PendingAction>,
    mut turn_state: ResMut<TurnState>,
    mut units: Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    tiles: Query<(Entity, &Tile)>,
    map: Res<GameMap>,
    mut attack_events: EventWriter<AttackEvent>,
    game_result: Res<GameResult>,
    commanders: Res<Commanders>,
    weather: Res<Weather>,
    mut movement_path: ResMut<MovementPath>,
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
        highlights.tile_costs.clear();
        highlights.attack_targets.clear();
        movement_path.clear();
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
                highlights.tile_costs.clear();
                highlights.attack_targets.clear();
                movement_path.clear();
                return;
            }

            // Try to move to cursor position
            if highlights.tiles.contains(&(cursor.x, cursor.y)) {
                // Check if target has a joinable unit (friendly same-type)
                let selected_unit_info = units.get(selected_entity).map(|(_, _, _, f, u)| (f.faction, u.unit_type));
                let joinable_unit = units.iter()
                    .find(|(e, p, _, f, u)| {
                        *e != selected_entity
                            && p.x == cursor.x && p.y == cursor.y
                            && selected_unit_info.map_or(false, |(sf, su)| f.faction == sf && u.unit_type == su)
                    })
                    .map(|(e, _, _, _, _)| e);

                // Check if target is occupied by non-joinable unit
                let target_blocked = units.iter()
                    .any(|(e, p, _, _, _)| e != selected_entity && p.x == cursor.x && p.y == cursor.y && joinable_unit != Some(e));

                if !target_blocked {
                    let new_pos = GridPosition::new(cursor.x, cursor.y);

                    // Get movement cost for stamina deduction
                    // Use path total cost if available, otherwise fall back to tile cost
                    let move_cost = if movement_path.total_cost > 0 {
                        movement_path.total_cost
                    } else {
                        highlights.tile_costs.get(&(cursor.x, cursor.y)).copied().unwrap_or(1)
                    };

                    // Move the unit with animation
                    let faction_copy;
                    let unit_copy;
                    let start_pos;
                    if let Ok((_, mut grid_pos, transform, faction, mut unit)) = units.get_mut(selected_entity) {
                        start_pos = transform.translation;
                        grid_pos.x = cursor.x;
                        grid_pos.y = cursor.y;
                        // Calculate end position (preserve Y height)
                        let new_world_pos = grid_pos.to_world(&map);
                        let end_pos = Vec3::new(new_world_pos.x, start_pos.y, new_world_pos.z);
                        // Add animation component for smooth movement
                        commands.entity(selected_entity).insert(UnitAnimation::new(start_pos, end_pos));
                        unit.moved = true;
                        // Deduct stamina based on path cost
                        unit.stamina = unit.stamina.saturating_sub(move_cost);
                        faction_copy = faction.clone();
                        unit_copy = unit.clone();
                        info!("Moved unit via path to ({}, {}), stamina {} -> {} (path cost: {})", cursor.x, cursor.y, unit.stamina + move_cost, unit.stamina, move_cost);
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

                    // Check if can join (moved onto friendly same-type unit)
                    let (can_join, join_target) = if let Some(join_entity) = joinable_unit {
                        (true, Some(join_entity))
                    } else {
                        (false, None)
                    };

                    highlights.selected_unit = None;
                    highlights.tiles.clear();
                    highlights.tile_costs.clear();
                    highlights.attack_targets.clear();
                    movement_path.clear();

                    // Enter Action phase if there are targets, can capture, or can join
                    if (!targets.is_empty() && !unit_copy.attacked) || can_capture || can_join {
                        pending_action.unit = Some(selected_entity);
                        pending_action.targets = targets;
                        pending_action.can_capture = can_capture;
                        pending_action.capture_tile = capture_tile;
                        pending_action.can_join = can_join;
                        pending_action.join_target = join_target;
                        turn_state.phase = TurnPhase::Action;
                        info!("Entering action phase: {} targets, can_capture: {}, can_join: {}", pending_action.targets.len(), can_capture, can_join);
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
                    let stats = unit.unit_type.stats();
                    let co_bonuses = commanders.get_bonuses(turn_state.current_faction);
                    let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
                    let weather_movement = weather.apply_movement(base_movement);
                    // Limit movement by stamina
                    let total_movement = effective_movement(weather_movement, unit.stamina);

                    // Build unit list for join-aware movement
                    let all_unit_info: Vec<_> = units.iter()
                        .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type))
                        .collect();
                    let (tiles, tile_costs) = calculate_movement_range_with_joins(
                        &pos, total_movement, &map, unit.unit_type, faction.faction, &all_unit_info
                    );

                    // Calculate attack targets
                    let all_units: Vec<_> = units.iter()
                        .map(|(e, p, _, f, _)| (e, p.clone(), f.clone()))
                        .collect();
                    let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units);

                    found_unit = Some((entity, tiles, tile_costs, attack_targets));
                    break;
                }
            }
            if let Some((entity, tiles, tile_costs, attack_targets)) = found_unit {
                highlights.selected_unit = Some(entity);
                highlights.tiles = tiles;
                highlights.tile_costs = tile_costs;
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
    mut commands: Commands,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut units: Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    tiles: Query<(Entity, &Tile)>,
    mut highlights: ResMut<MovementHighlights>,
    mut pending_action: ResMut<PendingAction>,
    mut production_state: ResMut<ProductionState>,
    mut cursor: ResMut<GridCursor>,
    mut turn_state: ResMut<TurnState>,
    map: Res<GameMap>,
    mut attack_events: EventWriter<AttackEvent>,
    game_ctx: GameStateContext,
    mut movement_path: ResMut<MovementPath>,
) {
    // Don't process input if game is over
    if game_ctx.game_result.game_over {
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
            highlights.tile_costs.clear();
            highlights.attack_targets.clear();
            movement_path.clear();
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
                highlights.tile_costs.clear();
                highlights.attack_targets.clear();
                movement_path.clear();
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
                    let co_bonuses = game_ctx.commanders.get_bonuses(turn_state.current_faction);
                    let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
                    let total_movement = game_ctx.weather.apply_movement(base_movement);
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

            // Check if target has a joinable unit (friendly same-type)
            let selected_unit_info = units.get(selected_entity).map(|(_, _, _, f, u)| (f.faction, u.unit_type));
            let joinable_unit = units.iter()
                .find(|(e, p, _, f, u)| {
                    *e != selected_entity
                        && p.x == grid_x && p.y == grid_y
                        && selected_unit_info.map_or(false, |(sf, su)| f.faction == sf && u.unit_type == su)
                })
                .map(|(e, _, _, _, _)| e);

            // Move the unit with animation
            let new_pos = GridPosition::new(grid_x, grid_y);

            // Use path total cost if available, otherwise fall back to tile cost
            let move_cost = if movement_path.total_cost > 0 {
                movement_path.total_cost
            } else {
                highlights.tile_costs.get(&(grid_x, grid_y)).copied().unwrap_or(1)
            };

            let faction_copy;
            let unit_copy;
            if let Ok((_, mut grid_pos, transform, faction, mut unit)) = units.get_mut(selected_entity) {
                let start_pos = transform.translation;
                grid_pos.x = grid_x;
                grid_pos.y = grid_y;
                // Calculate end position (preserve Y height)
                let new_world_pos = grid_pos.to_world(&map);
                let end_pos = Vec3::new(new_world_pos.x, start_pos.y, new_world_pos.z);
                // Add animation component for smooth movement
                commands.entity(selected_entity).insert(UnitAnimation::new(start_pos, end_pos));
                unit.moved = true;
                // Deduct stamina based on path cost
                unit.stamina = unit.stamina.saturating_sub(move_cost);
                faction_copy = faction.clone();
                unit_copy = unit.clone();
                info!("Moved unit via path to ({}, {}), stamina {} -> {} (path cost: {})", grid_x, grid_y, unit.stamina + move_cost, unit.stamina, move_cost);
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

            // Check if can join (moved onto friendly same-type unit)
            let (can_join, join_target) = if let Some(join_entity) = joinable_unit {
                (true, Some(join_entity))
            } else {
                (false, None)
            };

            highlights.selected_unit = None;
            highlights.tiles.clear();
            highlights.tile_costs.clear();
            highlights.attack_targets.clear();
            movement_path.clear();

            // Enter Action phase if there are targets, can capture, or can join
            if (!targets.is_empty() && !unit_copy.attacked) || can_capture || can_join {
                pending_action.unit = Some(selected_entity);
                pending_action.targets = targets;
                pending_action.can_capture = can_capture;
                pending_action.capture_tile = capture_tile;
                pending_action.can_join = can_join;
                pending_action.join_target = join_target;
                turn_state.phase = TurnPhase::Action;
                info!("Entering action phase: {} targets, can_capture: {}, can_join: {}", pending_action.targets.len(), can_capture, can_join);
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
            let stats = unit.unit_type.stats();
            let co_bonuses = game_ctx.commanders.get_bonuses(turn_state.current_faction);
            let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
            let weather_movement = game_ctx.weather.apply_movement(base_movement);
            // Limit movement by stamina
            let total_movement = effective_movement(weather_movement, unit.stamina);

            // Build unit list for join-aware movement
            let all_unit_info: Vec<_> = units.iter()
                .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type))
                .collect();
            let (move_tiles, move_costs) = calculate_movement_range_with_joins(
                &pos, total_movement, &map, unit.unit_type, faction.faction, &all_unit_info
            );

            // Calculate attack targets
            let all_units: Vec<_> = units.iter()
                .map(|(e, p, _, f, _)| (e, p.clone(), f.clone()))
                .collect();
            let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units);

            select_unit = Some((entity, move_tiles, move_costs, attack_targets, pos.x, pos.y));
            break;
        }
    }

    if let Some((entity, tiles, tile_costs, attack_targets, x, y)) = select_unit {
        highlights.selected_unit = Some(entity);
        highlights.tiles = tiles;
        highlights.tile_costs = tile_costs;
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
    highlights.tile_costs.clear();
    highlights.attack_targets.clear();
    movement_path.clear();
    production_state.active = false;
}

/// Spawn mesh entities for movement and attack highlights
fn spawn_movement_highlight_meshes(
    mut commands: Commands,
    highlights: Res<MovementHighlights>,
    map: Res<GameMap>,
    units: Query<(&GridPosition, &FactionMember)>,
    existing_move_highlights: Query<Entity, With<MovementHighlightMesh>>,
    existing_attack_highlights: Query<Entity, With<AttackHighlightMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Only update when highlights change
    if !highlights.is_changed() {
        return;
    }

    // Despawn old highlight meshes
    for entity in existing_move_highlights.iter() {
        commands.entity(entity).despawn_recursive();
    }
    for entity in existing_attack_highlights.iter() {
        commands.entity(entity).despawn_recursive();
    }

    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Create movement highlight material (semi-transparent blue)
    let move_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 0.5, 1.0, 0.4),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // Create attack highlight material (semi-transparent red)
    let attack_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.2, 0.2, 0.5),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // Spawn movement range highlight meshes
    for &(x, y) in &highlights.tiles {
        let world_x = x as f32 * TILE_SIZE + offset_x;
        let world_z = y as f32 * TILE_SIZE + offset_z;

        // Get tile height to place highlight on top of tile
        let tile_height = map.get(x, y)
            .map(|t| t.tile_height())
            .unwrap_or(4.0);

        // Create a flat plane mesh for this tile
        let plane_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(TILE_SIZE * 0.95 / 2.0)));

        commands.spawn((
            Mesh3d(plane_mesh),
            MeshMaterial3d(move_material.clone()),
            Transform::from_xyz(world_x, tile_height + 0.1, world_z),
            MovementHighlightMesh,
        ));
    }

    // Spawn attack target highlight meshes
    for target_entity in &highlights.attack_targets {
        if let Ok((pos, _)) = units.get(*target_entity) {
            let world_x = pos.x as f32 * TILE_SIZE + offset_x;
            let world_z = pos.y as f32 * TILE_SIZE + offset_z;

            // Get tile height
            let tile_height = map.get(pos.x, pos.y)
                .map(|t| t.tile_height())
                .unwrap_or(4.0);

            let plane_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(TILE_SIZE * 0.95 / 2.0)));

            commands.spawn((
                Mesh3d(plane_mesh),
                MeshMaterial3d(attack_material.clone()),
                Transform::from_xyz(world_x, tile_height + 0.15, world_z),
                AttackHighlightMesh,
            ));
        }
    }
}

/// Handle mouse/touch path drawing
fn handle_path_drawing(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    map: Res<GameMap>,
    highlights: Res<MovementHighlights>,
    mut movement_path: ResMut<MovementPath>,
    input_mode: Res<InputMode>,
) {
    // Skip if using keyboard mode exclusively
    if *input_mode == InputMode::Keyboard {
        return;
    }

    // Only handle path drawing when a unit is selected
    if highlights.selected_unit.is_none() || highlights.tiles.is_empty() {
        if movement_path.drawing {
            movement_path.clear();
        }
        return;
    }

    let Ok(window) = windows.get_single() else { return };
    let Ok((camera, camera_transform)) = camera_query.get_single() else { return };

    let Some(cursor_pos) = screen_to_grid(window, camera, camera_transform, &map) else {
        return;
    };

    // Start dragging on mouse down
    if mouse_button.just_pressed(MouseButton::Left) {
        // Check if clicking on the selected unit's position or within movement range
        if highlights.tiles.contains(&(cursor_pos.x, cursor_pos.y)) ||
           movement_path.path.first() == Some(&cursor_pos) {
            if movement_path.path.is_empty() {
                // Find unit start position (first tile that was the unit's original pos)
                // For now, if clicking in range, start path from there
                movement_path.start(cursor_pos);
            }
            movement_path.dragging = true;
        }
    }

    // Extend path while dragging
    if movement_path.dragging && mouse_button.pressed(MouseButton::Left) {
        if let Some(last_pos) = movement_path.path.last().copied() {
            if cursor_pos != last_pos {
                movement_path.try_extend(cursor_pos, &map, &highlights.tiles, &highlights.tile_costs);
            }
        }
    }

    // Stop dragging on mouse release
    if mouse_button.just_released(MouseButton::Left) {
        movement_path.dragging = false;
    }
}

/// Handle keyboard path drawing (WASD/Arrow keys)
fn handle_keyboard_path_drawing(
    keyboard: Res<ButtonInput<KeyCode>>,
    map: Res<GameMap>,
    highlights: Res<MovementHighlights>,
    mut movement_path: ResMut<MovementPath>,
    mut cursor: ResMut<GridCursor>,
    input_mode: Res<InputMode>,
    turn_state: Res<TurnState>,
) {
    // Skip during non-select phases
    if turn_state.phase != TurnPhase::Select {
        return;
    }

    // Only handle when a unit is selected and drawing path
    if highlights.selected_unit.is_none() {
        return;
    }

    // Skip if using mouse mode exclusively
    if *input_mode == InputMode::Mouse {
        return;
    }

    // Initialize path if not started
    if !movement_path.drawing && highlights.selected_unit.is_some() {
        // Find the unit's starting position from cursor or first available
        if highlights.tiles.contains(&(cursor.x, cursor.y)) {
            movement_path.start(IVec2::new(cursor.x, cursor.y));
        }
    }

    // Direction input
    let mut dx = 0i32;
    let mut dy = 0i32;

    if keyboard.just_pressed(KeyCode::KeyW) || keyboard.just_pressed(KeyCode::ArrowUp) {
        dy = -1;  // Grid Y decreases going up visually
    }
    if keyboard.just_pressed(KeyCode::KeyS) || keyboard.just_pressed(KeyCode::ArrowDown) {
        dy = 1;
    }
    if keyboard.just_pressed(KeyCode::KeyA) || keyboard.just_pressed(KeyCode::ArrowLeft) {
        dx = -1;
    }
    if keyboard.just_pressed(KeyCode::KeyD) || keyboard.just_pressed(KeyCode::ArrowRight) {
        dx = 1;
    }

    if dx != 0 || dy != 0 {
        if let Some(last_pos) = movement_path.path.last().copied() {
            let new_pos = IVec2::new(last_pos.x + dx, last_pos.y + dy);

            // Try to extend path
            if movement_path.try_extend(new_pos, &map, &highlights.tiles, &highlights.tile_costs) {
                // Update cursor to follow path
                cursor.x = new_pos.x;
                cursor.y = new_pos.y;
                cursor.visible = true;
            }
        }
    }
}

/// Create an arrow mesh pointing in the +X direction (triangle)
fn create_arrow_mesh() -> Mesh {
    use bevy::render::mesh::PrimitiveTopology;

    // Arrow triangle vertices (pointing in +X direction, lying on XZ plane)
    // Made larger for visibility - about 1/3 of tile size
    let arrow_size = TILE_SIZE * 0.35;
    let arrow_width = TILE_SIZE * 0.25;

    let positions = vec![
        [arrow_size, 0.0, 0.0],           // Tip (front)
        [-arrow_size * 0.4, 0.0, arrow_width],   // Back left
        [-arrow_size * 0.4, 0.0, -arrow_width],  // Back right
    ];

    let normals = vec![
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];

    let uvs = vec![
        [0.5, 0.0],
        [0.0, 1.0],
        [1.0, 1.0],
    ];

    let indices = bevy::render::mesh::Indices::U32(vec![0, 1, 2]);

    Mesh::new(PrimitiveTopology::TriangleList, bevy::render::render_asset::RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(indices)
}

/// Spawn mesh entities for path indicators
fn spawn_path_indicator_meshes(
    mut commands: Commands,
    movement_path: Res<MovementPath>,
    map: Res<GameMap>,
    existing_path_meshes: Query<Entity, With<PathIndicatorMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Only update when path changes
    if !movement_path.is_changed() {
        return;
    }

    // Despawn old path meshes
    for entity in existing_path_meshes.iter() {
        commands.entity(entity).despawn_recursive();
    }

    if movement_path.path.len() < 2 {
        return;
    }

    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Path line material (bright cyan/teal)
    let path_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.9, 0.8, 0.85),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // Destination marker material (brighter, slightly different color)
    let dest_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 1.0, 0.6, 0.9),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // Create arrow mesh once
    let arrow_mesh = meshes.add(create_arrow_mesh());

    // Draw path with arrows between tiles
    for i in 0..movement_path.path.len() {
        let pos = movement_path.path[i];
        let world_x = pos.x as f32 * TILE_SIZE + offset_x;
        let world_z = pos.y as f32 * TILE_SIZE + offset_z;

        let tile_height = map.get(pos.x, pos.y)
            .map(|t| t.tile_height())
            .unwrap_or(4.0);

        let is_destination = i == movement_path.path.len() - 1;
        let is_start = i == 0;

        // Draw connecting arrow to next tile (skip for destination)
        if !is_destination {
            let next_pos = movement_path.path[i + 1];
            let next_world_x = next_pos.x as f32 * TILE_SIZE + offset_x;
            let next_world_z = next_pos.y as f32 * TILE_SIZE + offset_z;
            let next_tile_height = map.get(next_pos.x, next_pos.y)
                .map(|t| t.tile_height())
                .unwrap_or(4.0);

            // Calculate midpoint for arrow placement
            let mid_x = (world_x + next_world_x) / 2.0;
            let mid_z = (world_z + next_world_z) / 2.0;
            // Place arrows well above the movement highlight tiles (which are at +0.1)
            let mid_y = (tile_height + next_tile_height) / 2.0 + 1.0;

            // Calculate direction and rotation
            let dx = next_world_x - world_x;
            let dz = next_world_z - world_z;
            let angle = dz.atan2(dx);

            // Spawn arrow pointing toward next tile
            commands.spawn((
                Mesh3d(arrow_mesh.clone()),
                MeshMaterial3d(path_material.clone()),
                Transform::from_xyz(mid_x, mid_y, mid_z)
                    .with_rotation(Quat::from_rotation_y(-angle + std::f32::consts::FRAC_PI_2)),
                PathIndicatorMesh,
            ));

            // Also draw a thin connecting line beneath the arrow
            let length = (dx * dx + dz * dz).sqrt() * 0.4;
            let line_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::new(length / 2.0, 3.0)));

            commands.spawn((
                Mesh3d(line_mesh),
                MeshMaterial3d(path_material.clone()),
                Transform::from_xyz(mid_x, mid_y - 0.1, mid_z)
                    .with_rotation(Quat::from_rotation_y(-angle)),
                PathIndicatorMesh,
            ));
        }

        // Draw destination marker (larger, different color)
        if is_destination {
            // Destination: larger marker well above the tile
            let dest_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(TILE_SIZE * 0.35)));
            commands.spawn((
                Mesh3d(dest_mesh),
                MeshMaterial3d(dest_material.clone()),
                Transform::from_xyz(world_x, tile_height + 1.2, world_z),
                PathIndicatorMesh,
            ));

            // Add a diamond/cross shape for visibility
            let cross_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::new(TILE_SIZE * 0.45, 5.0)));
            commands.spawn((
                Mesh3d(cross_mesh.clone()),
                MeshMaterial3d(dest_material.clone()),
                Transform::from_xyz(world_x, tile_height + 1.25, world_z),
                PathIndicatorMesh,
            ));
            commands.spawn((
                Mesh3d(cross_mesh),
                MeshMaterial3d(dest_material.clone()),
                Transform::from_xyz(world_x, tile_height + 1.25, world_z)
                    .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
                PathIndicatorMesh,
            ));
        } else if !is_start {
            // Small dot for intermediate path tiles - raised above movement highlights
            let dot_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(TILE_SIZE * 0.12)));
            commands.spawn((
                Mesh3d(dot_mesh),
                MeshMaterial3d(path_material.clone()),
                Transform::from_xyz(world_x, tile_height + 0.8, world_z),
                PathIndicatorMesh,
            ));
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

/// Draw warning indicators on units with low stamina/ammo
fn draw_resource_warnings(
    mut gizmos: Gizmos,
    map: Res<GameMap>,
    turn_state: Res<TurnState>,
    units: Query<(&Unit, &GridPosition, &FactionMember)>,
    time: Res<Time>,
) {
    // Only show warnings for current player's units
    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Pulse effect for warnings
    let pulse = (time.elapsed_secs() * 3.0).sin() * 0.3 + 0.7;

    for (unit, pos, faction) in units.iter() {
        // Only show for current faction's unmoved units
        if faction.faction != turn_state.current_faction || unit.moved {
            continue;
        }

        let stats = unit.unit_type.stats();
        let low_stamina = stats.max_stamina > 0 && unit.stamina <= stats.max_stamina / 3;
        let low_ammo = stats.max_ammo > 0 && unit.ammo <= stats.max_ammo / 3;
        let no_ammo = stats.max_ammo > 0 && unit.ammo == 0;
        let exhausted = stats.max_stamina > 0 && unit.stamina == 0;

        if !low_stamina && !low_ammo {
            continue;
        }

        let world_x = pos.x as f32 * TILE_SIZE + offset_x;
        let world_z = pos.y as f32 * TILE_SIZE + offset_z;

        // Determine warning color and intensity
        let (color, size) = if exhausted || no_ammo {
            // Critical - red pulsing
            (Color::srgba(1.0, 0.2, 0.2, pulse * 0.8), 8.0)
        } else {
            // Warning - orange pulsing
            (Color::srgba(1.0, 0.6, 0.1, pulse * 0.6), 6.0)
        };

        // Draw warning diamond above unit
        let y_offset = 20.0; // Above the unit sprite
        let center = Vec3::new(world_x, y_offset, world_z);

        // Draw a diamond shape (rotated square)
        let half = size / 2.0;
        gizmos.line(
            center + Vec3::new(0.0, 0.0, -half),
            center + Vec3::new(half, 0.0, 0.0),
            color,
        );
        gizmos.line(
            center + Vec3::new(half, 0.0, 0.0),
            center + Vec3::new(0.0, 0.0, half),
            color,
        );
        gizmos.line(
            center + Vec3::new(0.0, 0.0, half),
            center + Vec3::new(-half, 0.0, 0.0),
            color,
        );
        gizmos.line(
            center + Vec3::new(-half, 0.0, 0.0),
            center + Vec3::new(0.0, 0.0, -half),
            color,
        );

        // Draw exclamation mark inside for critical
        if exhausted || no_ammo {
            gizmos.line(
                center + Vec3::new(0.0, 0.0, -half + 2.0),
                center + Vec3::new(0.0, 0.0, half - 3.0),
                color,
            );
            gizmos.sphere(
                Isometry3d::from_translation(center + Vec3::new(0.0, 0.0, half - 1.5)),
                1.0,
                color,
            );
        }
    }
}
