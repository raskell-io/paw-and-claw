use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy_egui::input::EguiWantsInput;
use std::collections::{HashMap, HashSet, VecDeque};

use super::{GameMap, GridPosition, Unit, UnitType, FactionMember, TurnState, TurnPhase, AttackEvent, Tile, Terrain, TILE_SIZE, GameResult, Commanders, Weather, UnitAnimation, Faction, GameData, UnitClass, BattleConfig, AiState, BattleEntity};
use crate::states::GameState;

/// Message to cancel a unit's move and return it to original position
#[derive(Message)]
pub struct CancelMoveEvent {
    pub unit: Entity,
    pub original_position: (i32, i32),
}

/// Set by the UI while a modal screen (setup, pause, production, game over)
/// owns input, so the board ignores keys and clicks meant for the menu
#[derive(Resource, Default)]
pub struct ModalOpen(pub bool);

fn board_input_allowed(modal: Res<ModalOpen>) -> bool {
    !modal.0
}

/// Bundled read-only game state for systems with many parameters
#[derive(SystemParam)]
pub struct GameStateContext<'w> {
    pub game_result: Res<'w, GameResult>,
    pub commanders: Res<'w, Commanders>,
    pub weather: Res<'w, Weather>,
}

/// Bundled input state for systems with many parameters
#[derive(SystemParam)]
pub struct InputState<'w> {
    pub mouse_button: Res<'w, ButtonInput<MouseButton>>,
    pub egui_wants_input: Res<'w, EguiWantsInput>,
    pub movement_path: ResMut<'w, MovementPath>,
    pub cursor: ResMut<'w, GridCursor>,
    pub cursor_repeat: ResMut<'w, CursorRepeat>,
    pub time: Res<'w, Time>,
}

/// Marker component for movement highlight mesh entities
#[derive(Component)]
#[require(BattleEntity)]
pub struct MovementHighlightMesh;

/// Marker component for attack target highlight mesh entities
#[derive(Component)]
#[require(BattleEntity)]
pub struct AttackHighlightMesh;

/// Marker component for path indicator mesh entities
#[derive(Component)]
#[require(BattleEntity)]
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
    pub fn try_extend(
        &mut self,
        pos: IVec2,
        map: &GameMap,
        valid_tiles: &HashSet<(i32, i32)>,
        _tile_costs: &HashMap<(i32, i32), u32>,
        unit_class: UnitClass,
        game_data: &GameData,
    ) -> bool {
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
            self.recalculate_cost(map, unit_class, game_data);
            return true;
        }

        // Check if adjacent to last position
        let last = *self.path.last().unwrap();
        let dx = (pos.x - last.x).abs();
        let dy = (pos.y - last.y).abs();

        if (dx == 1 && dy == 0) || (dx == 0 && dy == 1) {
            // Adjacent - add to path, paying that tile's terrain cost
            self.path.push(pos);
            self.recalculate_cost(map, unit_class, game_data);
            return true;
        }

        false
    }

    fn recalculate_cost(&mut self, map: &GameMap, unit_class: UnitClass, game_data: &GameData) {
        self.total_cost = 0;
        for pos in self.path.iter().skip(1) {  // Skip starting position
            let terrain = map.get(pos.x, pos.y).unwrap_or(Terrain::Grass);
            self.total_cost += game_data.movement_cost(terrain, unit_class);
        }
    }

    /// Replace the path with a whole route (start included), e.g. when the
    /// cursor skips past tiles it can't stop on
    pub fn reroute(&mut self, route: Vec<IVec2>, map: &GameMap, unit_class: UnitClass, game_data: &GameData) {
        self.path = route;
        self.drawing = true;
        self.recalculate_cost(map, unit_class, game_data);
    }

    /// Whether the path ends where the cursor is (otherwise moves use the cursor)
    pub fn ends_at(&self, x: i32, y: i32) -> bool {
        self.path.len() >= 2 && self.path.last() == Some(&IVec2::new(x, y))
    }

    /// Get the final destination of the path
    pub fn destination(&self) -> Option<IVec2> {
        self.path.last().copied()
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
        app.add_message::<CancelMoveEvent>()
            .init_resource::<MovementHighlights>()
            .init_resource::<GridCursor>()
            .init_resource::<CursorRepeat>()
            .init_resource::<PendingAction>()
            .init_resource::<ProductionState>()
            .init_resource::<CameraZoom>()
            .init_resource::<CameraAngle>()
            .init_resource::<InputMode>()
            .init_resource::<MovementPath>()
            // Split systems into smaller groups to avoid tuple size limits
            // Camera systems can run independently
            .init_resource::<ModalOpen>()
            .add_systems(Update, (fit_camera_to_map, update_camera_angle).run_if(in_state(GameState::Battle)))
            .add_systems(Update, (
                handle_camera_movement,
                handle_camera_zoom,
                handle_camera_angle_toggle,
            ).run_if(in_state(GameState::Battle)).run_if(board_input_allowed))
            // Input handling - registered separately due to parameter count limits
            .add_systems(Update, handle_keyboard_input.run_if(in_state(GameState::Battle)).run_if(board_input_allowed))
            .add_systems(Update, handle_keyboard_path_drawing.run_if(in_state(GameState::Battle)).run_if(board_input_allowed))
            .add_systems(Update, keep_cursor_in_view
                .after(handle_keyboard_input)
                .after(handle_keyboard_path_drawing)
                .before(update_camera_angle)
                .run_if(in_state(GameState::Battle)))
            .add_systems(Update, (
                handle_click_input,
                handle_path_drawing,
                update_cursor_from_mouse,
            ).run_if(in_state(GameState::Battle)).run_if(board_input_allowed))
            .add_systems(Update, (
                spawn_movement_highlight_meshes,
                spawn_path_indicator_meshes,
            ).run_if(in_state(GameState::Battle)))
            .add_systems(Update, (
                draw_grid_cursor,
                draw_action_targets,
                draw_resource_warnings,
            ).run_if(in_state(GameState::Battle)))
            .add_systems(Update, handle_cancel_move.run_if(in_state(GameState::Battle)))
            .add_systems(Update, check_animation_complete.run_if(in_state(GameState::Battle)));
    }
}

/// Check if unit animation is complete and transition to Action phase
fn check_animation_complete(
    mut turn_state: ResMut<TurnState>,
    pending_action: Res<PendingAction>,
    units: Query<&UnitAnimation>,
) {
    // Only check during Animating phase
    if turn_state.phase != TurnPhase::Animating {
        return;
    }

    // Get the pending unit
    let Some(unit_entity) = pending_action.unit else {
        return;
    };

    // If unit no longer has animation component, animation is complete
    if units.get(unit_entity).is_err() {
        turn_state.phase = TurnPhase::Action;
        info!("Animation complete, entering action phase");
    }
}

/// Handle cancel move messages - move unit back to original position
fn handle_cancel_move(
    mut commands: Commands,
    mut events: MessageReader<CancelMoveEvent>,
    mut units: Query<(&mut GridPosition, &mut Transform)>,
    map: Res<GameMap>,
) {
    for event in events.read() {
        if let Ok((mut pos, mut transform)) = units.get_mut(event.unit) {
            // Update grid position
            pos.x = event.original_position.0;
            pos.y = event.original_position.1;

            // Update visual transform to match
            let world_pos = pos.to_world(&map);
            transform.translation.x = world_pos.x;
            transform.translation.z = world_pos.z;

            // Remove any pending animation
            commands.entity(event.unit).remove::<UnitAnimation>();

            info!("Unit {:?} moved back to {:?}", event.unit, event.original_position);
        }
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
            max: 4.0,   // Zoomed out far (large maps)
            speed: 0.15,
        }
    }
}

/// Resource for cursor key repeat timing
#[derive(Resource)]
pub struct CursorRepeat {
    /// Time since key was first pressed
    pub held_time: f32,
    /// Time since last repeat
    pub repeat_time: f32,
    /// Initial delay before repeat starts (seconds)
    pub initial_delay: f32,
    /// Interval between repeats (seconds)
    pub repeat_interval: f32,
    /// Which direction is currently held (if any)
    pub held_direction: Option<(i32, i32)>,
}

impl Default for CursorRepeat {
    fn default() -> Self {
        Self {
            held_time: 0.0,
            repeat_time: 0.0,
            initial_delay: 0.35,
            repeat_interval: 0.08,
            held_direction: None,
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
    pub original_position: Option<(i32, i32)>, // Where unit was before moving (for cancel)
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
    pub selected_unit_class: Option<UnitClass>,  // Unit class for movement cost lookup
}

/// Grid cursor for keyboard navigation
#[derive(Resource)]
pub struct GridCursor {
    pub x: i32,
    pub y: i32,
    pub visible: bool,
    /// Moved by keys or gamepad (not the mouse): the camera scrolls to keep it in view
    pub follow: bool,
}

impl Default for GridCursor {
    fn default() -> Self {
        Self {
            x: 5,
            y: 4,
            visible: true, // Always visible like Advance Wars
            follow: false,
        }
    }
}

/// Calculate which enemies can be attacked from current position
pub fn calculate_attack_targets(
    attacker: &Unit,
    attacker_pos: &GridPosition,
    attacker_faction: &FactionMember,
    units: &[(Entity, GridPosition, FactionMember, UnitType)],
    game_data: &GameData,
) -> HashSet<Entity> {
    let mut targets = HashSet::new();
    let (min_range, max_range) = game_data.unit_stats(attacker.unit_type).attack_range;

    for (entity, pos, faction, unit_type) in units {
        // Can't attack own faction
        if faction.faction == attacker_faction.faction {
            continue;
        }

        // Damage table decides which unit types this attacker can hit
        if !game_data.can_damage(attacker.unit_type, *unit_type) {
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
    unit_class: UnitClass,
    game_data: &GameData,
) -> HashSet<(i32, i32)> {
    let (reachable, _) = calculate_movement_range_with_costs(start, movement, map, units, unit_class, game_data);
    reachable
}

/// Calculate reachable tiles and their costs using BFS
/// Returns (reachable tiles, cost to reach each tile)
/// Uses GameData for terrain movement costs based on unit class
pub fn calculate_movement_range_with_costs(
    start: &GridPosition,
    movement: u32,
    map: &GameMap,
    units: &HashMap<(i32, i32), Entity>,
    unit_class: UnitClass,
    game_data: &GameData,
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
                // Use GameData for movement cost based on unit class
                let move_cost = game_data.movement_cost(terrain, unit_class);
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

/// Cheapest orthogonal route from `start` to `goal` (both included), by the same
/// rules as the movement range: terrain costs for the unit class, and no passing
/// through occupied tiles (the goal itself may be occupied, for joining). Units
/// never cut corners, so this is the route a move animates along.
pub fn find_path(
    start: IVec2,
    goal: IVec2,
    map: &GameMap,
    occupied: &HashSet<(i32, i32)>,
    unit_class: UnitClass,
    game_data: &GameData,
) -> Option<Vec<IVec2>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    let mut best: HashMap<IVec2, u32> = HashMap::from([(start, 0)]);
    let mut came_from: HashMap<IVec2, IVec2> = HashMap::new();
    let mut open = BinaryHeap::from([Reverse((0u32, start.x, start.y))]);
    while let Some(Reverse((cost, x, y))) = open.pop() {
        let here = IVec2::new(x, y);
        if here == goal {
            let mut path = vec![goal];
            while let Some(&prev) = came_from.get(path.last()?) {
                path.push(prev);
            }
            path.reverse();
            return Some(path);
        }
        if cost > best.get(&here).copied().unwrap_or(u32::MAX) {
            continue;
        }
        for step in [IVec2::Y, IVec2::NEG_Y, IVec2::X, IVec2::NEG_X] {
            let next = here + step;
            let Some(terrain) = map.get(next.x, next.y) else { continue };
            if !game_data.is_passable(terrain, unit_class)
                || (next != goal && occupied.contains(&(next.x, next.y)))
            {
                continue;
            }
            let next_cost = cost + game_data.movement_cost(terrain, unit_class);
            if next_cost < best.get(&next).copied().unwrap_or(u32::MAX) {
                best.insert(next, next_cost);
                came_from.insert(next, here);
                open.push(Reverse((next_cost, next.x, next.y)));
            }
        }
    }
    None
}

/// The orthogonal route a player's unit takes to `goal` when no path was drawn
fn fallback_move_route(
    units: &Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    mover: Entity,
    goal: IVec2,
    unit_class: Option<UnitClass>,
    map: &GameMap,
    game_data: &GameData,
) -> Option<Vec<IVec2>> {
    let start = units.get(mover).map(|(_, p, ..)| IVec2::new(p.x, p.y)).ok()?;
    let occupied = units.iter().filter(|(e, ..)| *e != mover).map(|(_, p, ..)| (p.x, p.y)).collect();
    find_path(start, goal, map, &occupied, unit_class?, game_data)
}

/// World waypoints for moving a unit along a grid route at height `y`. Without
/// a route it goes along one axis, then the other, never diagonally.
pub fn route_waypoints(route: Option<Vec<IVec2>>, start: IVec2, goal: IVec2, y: f32, map: &GameMap) -> Vec<Vec3> {
    let route = route.unwrap_or_else(|| {
        let corner = IVec2::new(goal.x, start.y);
        if corner == start || corner == goal { vec![start, goal] } else { vec![start, corner, goal] }
    });
    route.iter().map(|p| {
        let world = GridPosition::new(p.x, p.y).to_world(map);
        Vec3::new(world.x, y, world.z)
    }).collect()
}

/// Calculate reachable tiles including those occupied by joinable friendly units
/// Uses GameData for terrain movement costs based on unit class
pub fn calculate_movement_range_with_joins(
    start: &GridPosition,
    movement: u32,
    map: &GameMap,
    moving_unit_type: super::UnitType,
    moving_faction: Faction,
    all_units: &[(Entity, (i32, i32), Faction, super::UnitType)],
    unit_class: UnitClass,
    game_data: &GameData,
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
                // Use GameData for movement cost based on unit class
                let move_cost = game_data.movement_cost(terrain, unit_class);
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
    mut highlights: ResMut<MovementHighlights>,
    mut pending_action: ResMut<PendingAction>,
    mut turn_state: ResMut<TurnState>,
    mut units: Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    tiles: Query<(Entity, &Tile)>,
    map: Res<GameMap>,
    mut attack_events: MessageWriter<AttackEvent>,
    game_ctx: GameStateContext,
    game_data: Res<GameData>,
    mut input: InputState,
    battle_config: Res<BattleConfig>,
    ai_state: Res<AiState>,
    mut production_state: ResMut<ProductionState>,
) {
    // Don't process input if game is over
    if game_ctx.game_result.game_over {
        return;
    }

    // No map yet (e.g. battle setup screen is open)
    if map.width == 0 || map.height == 0 {
        return;
    }

    // Don't process player input during AI turn
    if ai_state.enabled && battle_config.is_ai_faction(turn_state.current_faction) {
        return;
    }

    // If in Action phase, keyboard is handled by UI
    if turn_state.phase == TurnPhase::Action {
        // ESC cancels action and deselects (unit waits)
        if keyboard.just_pressed(KeyCode::Escape) {
            if let Some(entity) = pending_action.unit {
                if let Ok((_, _, _, _, mut unit)) = units.get_mut(entity) {
                    unit.attacked = false; // Wait action
                    unit.exhausted = true; // Mark turn complete
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

    if shift_held || input.cursor.visible {
        // Determine current direction being pressed
        let up = keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp);
        let down = keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown);
        let left = keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft);
        let right = keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight);

        let current_dir = match (up, down, left, right) {
            (true, false, false, false) => Some((0, -1)),  // Up
            (false, true, false, false) => Some((0, 1)),   // Down
            (false, false, true, false) => Some((-1, 0)),  // Left
            (false, false, false, true) => Some((1, 0)),   // Right
            _ => None,  // No direction or multiple
        };

        // Check if direction just changed or was just pressed
        let just_pressed_up = keyboard.just_pressed(KeyCode::KeyW) || keyboard.just_pressed(KeyCode::ArrowUp);
        let just_pressed_down = keyboard.just_pressed(KeyCode::KeyS) || keyboard.just_pressed(KeyCode::ArrowDown);
        let just_pressed_left = keyboard.just_pressed(KeyCode::KeyA) || keyboard.just_pressed(KeyCode::ArrowLeft);
        let just_pressed_right = keyboard.just_pressed(KeyCode::KeyD) || keyboard.just_pressed(KeyCode::ArrowRight);
        let any_just_pressed = just_pressed_up || just_pressed_down || just_pressed_left || just_pressed_right;

        if let Some((dx, dy)) = current_dir {
            let should_move = if any_just_pressed || input.cursor_repeat.held_direction != current_dir {
                // New key press or direction change - move immediately and reset timing
                input.cursor_repeat.held_direction = current_dir;
                input.cursor_repeat.held_time = 0.0;
                input.cursor_repeat.repeat_time = 0.0;
                true
            } else {
                // Key held - update timing and check for repeat
                input.cursor_repeat.held_time += input.time.delta_secs();
                input.cursor_repeat.repeat_time += input.time.delta_secs();

                if input.cursor_repeat.held_time >= input.cursor_repeat.initial_delay
                    && input.cursor_repeat.repeat_time >= input.cursor_repeat.repeat_interval
                {
                    input.cursor_repeat.repeat_time = 0.0;
                    true
                } else {
                    false
                }
            };

            let target = (
                (input.cursor.x + dx).clamp(0, map.width as i32 - 1),
                (input.cursor.y + dy).clamp(0, map.height as i32 - 1),
            );
            let attack_positions = || units.iter()
                .filter(|(e, ..)| highlights.attack_targets.contains(e))
                .map(|(_, p, ..)| (p.x, p.y));
            // While a route is being drawn, `handle_keyboard_path_drawing` moves the cursor
            let routing = highlights.selected_unit.is_some() && input.movement_path.drawing;
            if should_move && !routing && cursor_allowed(&highlights, target, attack_positions) {
                input.cursor.x = target.0;
                input.cursor.y = target.1;
                input.cursor.follow = true;
                moved_cursor = true;
            }
        } else {
            // No direction pressed - reset repeat state
            input.cursor_repeat.held_direction = None;
            input.cursor_repeat.held_time = 0.0;
            input.cursor_repeat.repeat_time = 0.0;
        }
    } else {
        // Cursor not visible and shift not held - reset repeat state
        input.cursor_repeat.held_direction = None;
        input.cursor_repeat.held_time = 0.0;
        input.cursor_repeat.repeat_time = 0.0;
    }

    if moved_cursor {
        input.cursor.visible = true;
    }

    // Tab or N to cycle to next unmoved unit
    if keyboard.just_pressed(KeyCode::Tab) || keyboard.just_pressed(KeyCode::KeyN) {
        // Collect all non-exhausted units for current faction, sorted by position
        let mut unmoved_units: Vec<_> = units.iter()
            .filter(|(_, _, _, faction, unit)| {
                faction.faction == turn_state.current_faction && !unit.exhausted
            })
            .map(|(entity, pos, _, _, _)| (entity, pos.x, pos.y))
            .collect();

        // Sort by Y then X for consistent ordering
        unmoved_units.sort_by_key(|(_, x, y)| (*y, *x));

        if !unmoved_units.is_empty() {
            // Find current selection index
            let current_idx = highlights.selected_unit
                .and_then(|selected| unmoved_units.iter().position(|(e, _, _)| *e == selected));

            // Get next unit (wrap around)
            let next_idx = match current_idx {
                Some(idx) => (idx + 1) % unmoved_units.len(),
                None => 0,
            };

            let (next_entity, next_x, next_y) = unmoved_units[next_idx];

            // Clear current selection
            highlights.selected_unit = None;
            highlights.selected_unit_class = None;
            highlights.tiles.clear();
            highlights.tile_costs.clear();
            highlights.attack_targets.clear();
            input.movement_path.clear();

            // Move cursor to the unit
            input.cursor.x = next_x;
            input.cursor.y = next_y;
            input.cursor.visible = true;
            input.cursor.follow = true;

            // Select the unit (same logic as Space/Enter selection)
            if let Ok((entity, pos, _, faction, unit)) = units.get(next_entity) {
                let stats = game_data.unit_stats(unit.unit_type);
                let co_bonuses = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data);
                let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
                let weather_movement = game_ctx.weather.apply_movement(base_movement);
                let total_movement = effective_movement(weather_movement, unit.stamina);

                // Build unit list for join-aware movement
                let all_unit_info: Vec<_> = units.iter()
                    .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type))
                    .collect();
                let unit_class = stats.class;
                let (tiles, tile_costs) = calculate_movement_range_with_joins(
                    &pos, total_movement, &map, unit.unit_type, faction.faction, &all_unit_info,
                    unit_class, &game_data
                );

                // Calculate attack targets
                let all_units: Vec<_> = units.iter()
                    .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                    .collect();
                let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units, &game_data);

                highlights.selected_unit = Some(entity);
                highlights.selected_unit_class = Some(unit_class);
                highlights.tiles = tiles;
                highlights.tile_costs = tile_costs;
                highlights.attack_targets = attack_targets;
                input.movement_path.start(IVec2::new(input.cursor.x, input.cursor.y));
                info!("Cycled to unit at ({}, {})", input.cursor.x, input.cursor.y);
            }
        }
    }

    // ESC to deselect
    if keyboard.just_pressed(KeyCode::Escape) {
        highlights.selected_unit = None;
        highlights.selected_unit_class = None;
        highlights.tiles.clear();
        highlights.tile_costs.clear();
        highlights.attack_targets.clear();
        input.movement_path.clear();
        input.cursor.visible = false;
        return;
    }

    // Space or Enter to select/move/attack
    if keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::Enter) {
        input.cursor.visible = true;

        if let Some(selected_entity) = highlights.selected_unit {
            // Use path destination if path exists, otherwise use cursor
            // The cursor is where the unit goes; the drawn path only says how
            let (target_x, target_y) = (input.cursor.x, input.cursor.y);

            // Check if target is an attack target (before moving)
            let target_entity = units.iter()
                .find(|(e, p, _, _, _)| p.x == target_x && p.y == target_y && highlights.attack_targets.contains(e))
                .map(|(e, _, _, _, _)| e);

            if let Some(target) = target_entity {
                // Attack!
                attack_events.write(AttackEvent {
                    attacker: selected_entity,
                    defender: target,
                });

                // Mark as attacked and exhausted
                if let Ok((_, _, _, _, mut unit)) = units.get_mut(selected_entity) {
                    unit.attacked = true;
                    unit.exhausted = true;
                }
                highlights.selected_unit = None;
                highlights.selected_unit_class = None;
                highlights.tiles.clear();
                highlights.tile_costs.clear();
                highlights.attack_targets.clear();
                input.movement_path.clear();
                return;
            }

            // Try to move to path destination or cursor position
            if highlights.tiles.contains(&(target_x, target_y)) {
                // Update cursor to match target
                input.cursor.x = target_x;
                input.cursor.y = target_y;
                // Check if target has a joinable unit (friendly same-type)
                let selected_unit_info = units.get(selected_entity).map(|(_, _, _, f, u)| (f.faction, u.unit_type));
                let joinable_unit = units.iter()
                    .find(|(e, p, _, f, u)| {
                        *e != selected_entity
                            && p.x == input.cursor.x && p.y == input.cursor.y
                            && selected_unit_info.map_or(false, |(sf, su)| f.faction == sf && u.unit_type == su)
                    })
                    .map(|(e, _, _, _, _)| e);

                // Check if target is occupied by non-joinable unit
                let target_blocked = units.iter()
                    .any(|(e, p, _, _, _)| e != selected_entity && p.x == input.cursor.x && p.y == input.cursor.y && joinable_unit != Some(e));

                if !target_blocked {
                    let new_pos = GridPosition::new(input.cursor.x, input.cursor.y);

                    // Check if staying in place (no actual movement)
                    let staying_in_place = units.get(selected_entity)
                        .map(|(_, p, _, _, _)| p.x == input.cursor.x && p.y == input.cursor.y)
                        .unwrap_or(false);

                    // Get movement cost for stamina deduction (0 if staying in place)
                    let move_cost = if staying_in_place {
                        0
                    } else if input.movement_path.total_cost > 0 {
                        input.movement_path.total_cost
                    } else {
                        highlights.tile_costs.get(&(input.cursor.x, input.cursor.y)).copied().unwrap_or(1)
                    };

                    // Move the unit (with animation if actually moving)
                    let faction_copy;
                    let unit_copy;
                    let original_pos;
                    // Without a drawn path, walk the cheapest orthogonal route (never diagonal)
                    let fallback_route = fallback_move_route(&units, selected_entity, IVec2::from((input.cursor.x, input.cursor.y)), highlights.selected_unit_class, &map, &game_data);
                    if let Ok((_, mut grid_pos, transform, faction, mut unit)) = units.get_mut(selected_entity) {
                        let start_pos = transform.translation;
                        // Save original position before moving (for cancel)
                        original_pos = (grid_pos.x, grid_pos.y);
                        grid_pos.x = input.cursor.x;
                        grid_pos.y = input.cursor.y;

                        // Only create animation if actually moving
                        if !staying_in_place {
                            // Convert path to world positions for animation
                            let waypoints: Vec<Vec3> = if input.movement_path.ends_at(grid_pos.x, grid_pos.y) {
                                input.movement_path.path.iter().map(|p| {
                                    let world_pos = GridPosition::new(p.x, p.y).to_world(&map);
                                    Vec3::new(world_pos.x, start_pos.y, world_pos.z)
                                }).collect()
                            } else {
                                let goal = IVec2::new(grid_pos.x, grid_pos.y);
                                route_waypoints(fallback_route, IVec2::from(original_pos), goal, start_pos.y, &map)
                            };

                            // Add animation component for smooth movement along path
                            commands.entity(selected_entity).insert(UnitAnimation::from_path(waypoints));
                            unit.moved = true;
                            // Deduct stamina based on path cost
                            unit.stamina = unit.stamina.saturating_sub(move_cost);
                            info!("Moved unit via path to ({}, {}), stamina {} -> {} (path cost: {})", input.cursor.x, input.cursor.y, unit.stamina + move_cost, unit.stamina, move_cost);
                        } else {
                            info!("Unit staying in place at ({}, {})", input.cursor.x, input.cursor.y);
                        }

                        faction_copy = faction.clone();
                        unit_copy = unit.clone();
                    } else {
                        return;
                    }

                    // Calculate attack targets from current/new position
                    let all_units: Vec<_> = units.iter()
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                        .collect();
                    let targets = calculate_attack_targets(&unit_copy, &new_pos, &faction_copy, &all_units, &game_data);

                    // Check if unit can capture the tile
                    let (can_capture, capture_tile) = if game_data.unit_stats(unit_copy.unit_type).can_capture {
                        tiles.iter()
                            .find(|(_, t)| t.position.x == input.cursor.x && t.position.y == input.cursor.y)
                            .map(|(e, tile)| {
                                let capturable = game_data.terrain_capturable(tile.terrain) && tile.owner != Some(faction_copy.faction);
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
                    highlights.selected_unit_class = None;
                    highlights.tiles.clear();
                    highlights.tile_costs.clear();
                    highlights.attack_targets.clear();
                    input.movement_path.clear();

                    // Set up pending action (action menu shown after animation completes)
                    pending_action.unit = Some(selected_entity);
                    pending_action.targets = if unit_copy.attacked { HashSet::new() } else { targets };
                    pending_action.can_capture = can_capture;
                    pending_action.capture_tile = capture_tile;
                    pending_action.can_join = can_join;
                    pending_action.join_target = join_target;
                    pending_action.original_position = Some(original_pos);

                    // Enter Animating phase if moving, or Action phase directly if staying in place
                    if staying_in_place {
                        turn_state.phase = TurnPhase::Action;
                        info!("Staying in place, entering action phase directly");
                    } else {
                        turn_state.phase = TurnPhase::Animating;
                        info!("Moving unit, entering animating phase");
                    }
                }
            }
        } else {
            // Try to select unit at cursor
            let mut found_unit = None;
            for (entity, pos, _, faction, unit) in units.iter() {
                if pos.x == input.cursor.x && pos.y == input.cursor.y
                    && !unit.exhausted
                    && faction.faction == turn_state.current_faction
                {
                    let stats = game_data.unit_stats(unit.unit_type);
                    let co_bonuses = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data);
                    let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
                    let weather_movement = game_ctx.weather.apply_movement(base_movement);
                    // Limit movement by stamina
                    let total_movement = effective_movement(weather_movement, unit.stamina);

                    // Build unit list for join-aware movement
                    let all_unit_info: Vec<_> = units.iter()
                        .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type))
                        .collect();
                    let unit_class = game_data.unit_stats(unit.unit_type).class;
                    let (tiles, tile_costs) = calculate_movement_range_with_joins(
                        &pos, total_movement, &map, unit.unit_type, faction.faction, &all_unit_info,
                        unit_class, &game_data
                    );

                    // Calculate attack targets
                    let all_units: Vec<_> = units.iter()
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                        .collect();
                    let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units, &game_data);

                    found_unit = Some((entity, tiles, tile_costs, attack_targets, unit_class));
                    break;
                }
            }
            if let Some((entity, tiles, tile_costs, attack_targets, unit_class)) = found_unit {
                highlights.selected_unit = Some(entity);
                highlights.selected_unit_class = Some(unit_class);
                highlights.tiles = tiles;
                highlights.tile_costs = tile_costs;
                highlights.attack_targets = attack_targets;
                // Start path from unit's position
                input.movement_path.start(IVec2::new(input.cursor.x, input.cursor.y));
                info!("Selected unit at ({}, {})", input.cursor.x, input.cursor.y);
            } else {
                // No unit found - check if cursor is on an owned empty base for production
                let unit_positions: HashSet<(i32, i32)> = units.iter()
                    .map(|(_, p, _, _, _)| (p.x, p.y))
                    .collect();
                for (tile_entity, tile) in tiles.iter() {
                    if tile.position.x == input.cursor.x && tile.position.y == input.cursor.y
                        && game_data.terrain_produces(tile.terrain)
                        && tile.owner == Some(turn_state.current_faction)
                        && !unit_positions.contains(&(input.cursor.x, input.cursor.y))
                    {
                        production_state.active = true;
                        production_state.base_entity = Some(tile_entity);
                        production_state.base_position = (input.cursor.x, input.cursor.y);
                        info!("Opened production menu at base ({}, {}) via keyboard", input.cursor.x, input.cursor.y);
                        break;
                    }
                }
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
    mut scroll_events: MessageReader<bevy::input::mouse::MouseWheel>,
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

/// Handle keyboard toggle for camera angle (T key)
/// Frame the whole board when a new map is loaded
fn fit_camera_to_map(
    map: Res<GameMap>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    projection: Query<&Projection, With<Camera3d>>,
    mut zoom: ResMut<CameraZoom>,
    mut angle: ResMut<CameraAngle>,
) {
    if !map.is_changed() || map.width == 0 || map.height == 0 {
        return;
    }
    let aspect = windows.single().map(|w| w.width() / w.height().max(1.0)).unwrap_or(16.0 / 9.0);
    let fov = match projection.single() {
        Ok(Projection::Perspective(p)) => p.fov,
        _ => std::f32::consts::FRAC_PI_6,
    };
    let height = fit_height(map.width as f32 * TILE_SIZE, map.height as f32 * TILE_SIZE, aspect, angle.perspective_pitch, fov);
    zoom.current = (height / angle.perspective_height).clamp(zoom.min, zoom.max);
    angle.look_at = Vec2::ZERO;
    angle.mode = CameraMode::Perspective;
}

/// Camera height needed to see a `width` x `depth` board (world units) with
/// room left for the HUD cards at the top and bottom of the screen
fn fit_height(width: f32, depth: f32, aspect: f32, pitch: f32, fov: f32) -> f32 {
    // Ground depth seen per unit of height between the bottom and top screen edges
    let depth_per_height = 1.0 / (pitch - fov / 2.0).tan() - 1.0 / (pitch + fov / 2.0).tan();
    // Ground width seen per unit of height at the screen center
    let width_per_height = 2.0 * (fov / 2.0).tan() * aspect / pitch.sin();
    let hud_margin = 1.45;
    (depth * hud_margin / depth_per_height).max(width * 1.1 / width_per_height)
}

fn handle_camera_angle_toggle(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut angle: ResMut<CameraAngle>,
) {
    // Toggle with T key (Tab is used for unit cycling)
    if keyboard.just_pressed(KeyCode::KeyT) {
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
    let Ok(mut camera_transform) = camera_query.single_mut() else { return };

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

/// Update cursor position from mouse hover. Only an actual mouse movement takes
/// the cursor: a resting pointer never overrides keyboard or gamepad moves.
/// When a movement path is being drawn, cursor follows the path endpoint instead
fn update_cursor_from_mouse(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    map: Res<GameMap>,
    mut input: InputState,
    input_mode: Res<InputMode>,
    mut pointer_moved: MessageReader<CursorMoved>,
    highlights: Res<MovementHighlights>,
    unit_positions: Query<(Entity, &GridPosition), With<Unit>>,
) {
    // Skip if in keyboard-only mode
    if *input_mode == InputMode::Keyboard {
        return;
    }

    if pointer_moved.read().count() == 0 {
        return;
    }

    // Don't update if egui wants input
    if input.egui_wants_input.wants_any_pointer_input() {
        return;
    }

    // If a movement path is being drawn, cursor follows the path endpoint
    if input.movement_path.drawing {
        if let Some(endpoint) = input.movement_path.path.last() {
            input.cursor.x = endpoint.x;
            input.cursor.y = endpoint.y;
            return;
        }
    }

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = cameras.single() else { return };

    if let Some(grid_pos) = screen_to_grid(window, camera, camera_transform, &map) {
        let attack_positions = || unit_positions.iter()
            .filter(|(e, _)| highlights.attack_targets.contains(e))
            .map(|(_, p)| (p.x, p.y));
        if cursor_allowed(&highlights, (grid_pos.x, grid_pos.y), attack_positions) {
            input.cursor.x = grid_pos.x;
            input.cursor.y = grid_pos.y;
        }
        input.cursor.follow = false;
    }
}

/// With a unit selected, the cursor stays on the tiles it can move to or the
/// enemies it can attack; otherwise it can go anywhere on the map
fn cursor_allowed<I: Iterator<Item = (i32, i32)>>(
    highlights: &MovementHighlights,
    tile: (i32, i32),
    attack_positions: impl FnOnce() -> I,
) -> bool {
    highlights.selected_unit.is_none()
        || highlights.tiles.contains(&tile)
        || attack_positions().any(|p| p == tile)
}

/// Tiles of map kept visible beyond a key-driven cursor before the camera scrolls
const SCROLL_LOOKAHEAD: i32 = 3;

/// Advance Wars style scrolling: while the cursor is moved by keys or gamepad,
/// keep `SCROLL_LOOKAHEAD` tiles of map visible beyond it on every side (as far
/// as the map goes, so the camera stops at the map's edge). The camera pans
/// smoothly once the cursor gets that close to the edge of the screen.
fn keep_cursor_in_view(
    cursor: Res<GridCursor>,
    map: Res<GameMap>,
    time: Res<Time>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut angle: ResMut<CameraAngle>,
) {
    if !cursor.follow || map.width == 0 || map.height == 0 {
        return;
    }
    let Ok((camera, camera_transform)) = cameras.single() else { return };
    let Some(size) = camera.logical_viewport_size() else { return };
    let screen = |x: i32, y: i32, nudge: Vec3| {
        camera.world_to_viewport(camera_transform, GridPosition::new(x, y).to_world(&map) + nudge).ok()
    };

    let (w, h) = (map.width as i32, map.height as i32);
    let (x, y) = (cursor.x, cursor.y);
    let (Some(left), Some(right), Some(top), Some(bottom)) = (
        screen((x - SCROLL_LOOKAHEAD).max(0), y, Vec3::ZERO),
        screen((x + SCROLL_LOOKAHEAD).min(w - 1), y, Vec3::ZERO),
        screen(x, (y - SCROLL_LOOKAHEAD).max(0), Vec3::ZERO),
        screen(x, (y + SCROLL_LOOKAHEAD).min(h - 1), Vec3::ZERO),
    ) else { return };

    let (Some(here), Some(east), Some(south)) = (
        screen(x, y, Vec3::ZERO),
        screen(x, y, Vec3::X * TILE_SIZE),
        screen(x, y, Vec3::Z * TILE_SIZE),
    ) else { return };

    // The area those tiles must stay inside, clear of the HUD cards. Zoomed in
    // so far that both sides can't fit, just center the cursor on that axis.
    let (min, max) = (size * Vec2::new(0.05, 0.12), size * Vec2::new(0.95, 0.88));
    let center = (min + max) / 2.0;
    let axis = |low: f32, high: f32, min: f32, max: f32, here: f32, center: f32| {
        let (under, over) = ((low - min).min(0.0), (high - max).max(0.0));
        if under < 0.0 && over > 0.0 { here - center } else { under + over }
    };
    let push = Vec2::new(
        axis(left.x, right.x, min.x, max.x, here.x, center.x),
        axis(top.y, bottom.y, min.y, max.y, here.y, center.y),
    );
    if push.length_squared() < 1.0 {
        return;
    }

    // Screen pixels per tile around the cursor, to turn the push into world units
    let per_tile = Vec2::new((east.x - here.x).abs(), (south.y - here.y).abs()).max(Vec2::ONE);
    let shift = push / per_tile * TILE_SIZE;
    let ease = 1.0 - (-16.0 * time.delta_secs()).exp();
    angle.look_at += shift * ease;
}

/// Handle mouse click for selection and movement
fn handle_click_input(
    mut commands: Commands,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut units: Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    tiles: Query<(Entity, &Tile)>,
    mut highlights: ResMut<MovementHighlights>,
    mut pending_action: ResMut<PendingAction>,
    mut production_state: ResMut<ProductionState>,
    mut turn_state: ResMut<TurnState>,
    map: Res<GameMap>,
    mut attack_events: MessageWriter<AttackEvent>,
    game_ctx: GameStateContext,
    mut input: InputState,
    game_data: Res<GameData>,
    battle_config: Res<BattleConfig>,
    ai_state: Res<AiState>,
) {
    // Don't process input if egui wants it (UI is being clicked)
    if input.egui_wants_input.wants_any_pointer_input() {
        return;
    }

    // Don't process input if game is over
    if game_ctx.game_result.game_over {
        return;
    }

    // Don't process player input during AI turn
    if ai_state.enabled && battle_config.is_ai_faction(turn_state.current_faction) {
        return;
    }

    // In Action phase, clicks on targets are handled here
    if turn_state.phase == TurnPhase::Action {
        if !input.mouse_button.just_pressed(MouseButton::Left) {
            return;
        }

        let Ok(window) = windows.single() else { return };
        let Ok((camera, camera_transform)) = cameras.single() else { return };

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
                attack_events.write(AttackEvent {
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

    if !input.mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = cameras.single() else { return };

    let Some(grid_pos) = screen_to_grid(window, camera, camera_transform, &map) else {
        return;
    };
    let grid_x = grid_pos.x;
    let grid_y = grid_pos.y;

    // Update cursor position to click location
    input.cursor.x = grid_x;
    input.cursor.y = grid_y;

    // If we have a selected unit, try to attack or move
    if let Some(selected_entity) = highlights.selected_unit {
        // Check if clicking on an attack target (before moving)
        let target_entity = units.iter()
            .find(|(e, p, _, _, _)| p.x == grid_x && p.y == grid_y && highlights.attack_targets.contains(e))
            .map(|(e, _, _, _, _)| e);

        if let Some(target) = target_entity {
            // Attack!
            attack_events.write(AttackEvent {
                attacker: selected_entity,
                defender: target,
            });

            // Mark as attacked and exhausted
            if let Ok((_, _, _, _, mut unit)) = units.get_mut(selected_entity) {
                unit.attacked = true;
                unit.exhausted = true;
            }
            highlights.selected_unit = None;
        highlights.selected_unit_class = None;
            highlights.tiles.clear();
            highlights.tile_costs.clear();
            highlights.attack_targets.clear();
            input.movement_path.clear();
            return;
        }

        if highlights.tiles.contains(&(grid_x, grid_y)) {
            // If path drawing is active AND not currently dragging, confirm the move
            // by falling through to the movement code below
            if input.movement_path.drawing && !input.movement_path.dragging {
                // Update the path endpoint to the clicked tile if it's valid
                // This allows clicking directly on a destination instead of dragging
                if let Some(destination) = input.movement_path.destination() {
                    if destination.x != grid_x || destination.y != grid_y {
                        // Clicked on a different tile than path endpoint
                        // Try to extend/retract path to this tile, or just use direct move
                        if let Some(unit_class) = highlights.selected_unit_class {
                            input.movement_path.try_extend(
                                IVec2::new(grid_x, grid_y),
                                &map,
                                &highlights.tiles,
                                &highlights.tile_costs,
                                unit_class,
                                &game_data,
                            );
                        }
                    }
                }
                // Fall through to movement execution below
            } else if input.movement_path.dragging {
                // Currently dragging, let path drawing handle it
                return;
            } else {
                // Not in path drawing mode - check for deselection
                let clicking_on_selected = units.iter()
                    .any(|(e, p, _, _, _)| e == selected_entity && p.x == grid_x && p.y == grid_y);

                if clicking_on_selected {
                    // Clicking on selected unit - deselect
                    highlights.selected_unit = None;
                    highlights.selected_unit_class = None;
                    highlights.tiles.clear();
                    highlights.tile_costs.clear();
                    highlights.attack_targets.clear();
                    input.movement_path.clear();
                    return;
                }
            }

            // Check if clicking on another friendly unit (switch selection)
            let mut switch_to = None;
            for (entity, pos, _, faction, unit) in units.iter() {
                if pos.x == grid_x && pos.y == grid_y
                    && !unit.exhausted
                    && faction.faction == turn_state.current_faction
                    && entity != selected_entity
                {
                    let unit_positions: HashMap<(i32, i32), Entity> = units
                        .iter()
                        .map(|(e, p, _, _, _)| ((p.x, p.y), e))
                        .collect();
                    let stats = game_data.unit_stats(unit.unit_type);
                    let co_bonuses = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data);
                    let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
                    let total_movement = game_ctx.weather.apply_movement(base_movement);
                    let tiles = calculate_movement_range(&pos, total_movement, &map, &unit_positions, stats.class, &game_data);

                    // Calculate attack targets
                    let all_units: Vec<_> = units.iter()
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                        .collect();
                    let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units, &game_data);

                    switch_to = Some((entity, tiles, attack_targets, stats.class));
                    break;
                }
            }

            if let Some((entity, tiles, attack_targets, unit_class)) = switch_to {
                highlights.selected_unit = Some(entity);
                highlights.selected_unit_class = Some(unit_class);
                highlights.tiles = tiles;
                highlights.attack_targets = attack_targets;
                // Start path from unit's new position
                input.movement_path.start(IVec2::new(grid_x, grid_y));
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

            // Check if staying in place (no actual movement)
            let staying_in_place = units.get(selected_entity)
                .map(|(_, p, _, _, _)| p.x == grid_x && p.y == grid_y)
                .unwrap_or(false);

            let new_pos = GridPosition::new(grid_x, grid_y);

            // Get movement cost (0 if staying in place)
            let move_cost = if staying_in_place {
                0
            } else if input.movement_path.total_cost > 0 {
                input.movement_path.total_cost
            } else {
                highlights.tile_costs.get(&(grid_x, grid_y)).copied().unwrap_or(1)
            };

            let faction_copy;
            let unit_copy;
            let original_pos;
            // Without a drawn path, walk the cheapest orthogonal route (never diagonal)
            let fallback_route = fallback_move_route(&units, selected_entity, IVec2::from((grid_x, grid_y)), highlights.selected_unit_class, &map, &game_data);
            if let Ok((_, mut grid_pos, transform, faction, mut unit)) = units.get_mut(selected_entity) {
                let start_pos = transform.translation;
                // Save original position before moving (for cancel)
                original_pos = (grid_pos.x, grid_pos.y);
                grid_pos.x = grid_x;
                grid_pos.y = grid_y;

                // Only create animation if actually moving
                if !staying_in_place {
                    // Convert path to world positions for animation
                    let waypoints: Vec<Vec3> = if input.movement_path.ends_at(grid_x, grid_y) {
                        input.movement_path.path.iter().map(|p| {
                            let world_pos = GridPosition::new(p.x, p.y).to_world(&map);
                            Vec3::new(world_pos.x, start_pos.y, world_pos.z)
                        }).collect()
                    } else {
                        let goal = IVec2::new(grid_pos.x, grid_pos.y);
                        route_waypoints(fallback_route, IVec2::from(original_pos), goal, start_pos.y, &map)
                    };

                    // Add animation component for smooth movement along path
                    commands.entity(selected_entity).insert(UnitAnimation::from_path(waypoints));
                    unit.moved = true;
                    // Deduct stamina based on path cost
                    unit.stamina = unit.stamina.saturating_sub(move_cost);
                    info!("Moved unit via path to ({}, {}), stamina {} -> {} (path cost: {})", grid_x, grid_y, unit.stamina + move_cost, unit.stamina, move_cost);
                } else {
                    info!("Unit staying in place at ({}, {})", grid_x, grid_y);
                }

                faction_copy = faction.clone();
                unit_copy = unit.clone();
            } else {
                return;
            }

            // Calculate attack targets from new position
            let all_units: Vec<_> = units.iter()
                .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                .collect();
            let targets = calculate_attack_targets(&unit_copy, &new_pos, &faction_copy, &all_units, &game_data);

            // Check if unit can capture the tile it moved to
            let (can_capture, capture_tile) = if game_data.unit_stats(unit_copy.unit_type).can_capture {
                tiles.iter()
                    .find(|(_, t)| t.position.x == grid_x && t.position.y == grid_y)
                    .map(|(e, tile)| {
                        let capturable = game_data.terrain_capturable(tile.terrain) && tile.owner != Some(faction_copy.faction);
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
        highlights.selected_unit_class = None;
            highlights.tiles.clear();
            highlights.tile_costs.clear();
            highlights.attack_targets.clear();
            input.movement_path.clear();

            // Set up pending action (action menu shown after animation completes)
            pending_action.unit = Some(selected_entity);
            pending_action.targets = if unit_copy.attacked { HashSet::new() } else { targets };
            pending_action.can_capture = can_capture;
            pending_action.capture_tile = capture_tile;
            pending_action.can_join = can_join;
            pending_action.join_target = join_target;
            pending_action.original_position = Some(original_pos);

            // Enter Animating phase if moving, or Action phase directly if staying in place
            if staying_in_place {
                turn_state.phase = TurnPhase::Action;
                info!("Staying in place, entering action phase directly");
            } else {
                turn_state.phase = TurnPhase::Animating;
                info!("Moving unit, entering animating phase");
            }
            return;
        }
    }

    // No unit selected or clicked outside movement range - try to select a unit
    let mut select_unit = None;
    for (entity, pos, _, faction, unit) in units.iter() {
        if pos.x == grid_x && pos.y == grid_y
            && !unit.exhausted
            && faction.faction == turn_state.current_faction
        {
            let stats = game_data.unit_stats(unit.unit_type);
            let co_bonuses = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data);
            let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
            let weather_movement = game_ctx.weather.apply_movement(base_movement);
            // Limit movement by stamina
            let total_movement = effective_movement(weather_movement, unit.stamina);

            // Build unit list for join-aware movement
            let all_unit_info: Vec<_> = units.iter()
                .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type))
                .collect();
            let (move_tiles, move_costs) = calculate_movement_range_with_joins(
                &pos, total_movement, &map, unit.unit_type, faction.faction, &all_unit_info,
                stats.class, &game_data
            );

            // Calculate attack targets
            let all_units: Vec<_> = units.iter()
                .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                .collect();
            let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units, &game_data);

            select_unit = Some((entity, move_tiles, move_costs, attack_targets, pos.x, pos.y, stats.class));
            break;
        }
    }

    if let Some((entity, tiles, tile_costs, attack_targets, x, y, unit_class)) = select_unit {
        highlights.selected_unit = Some(entity);
        highlights.selected_unit_class = Some(unit_class);
        highlights.tiles = tiles;
        highlights.tile_costs = tile_costs;
        highlights.attack_targets = attack_targets;
        // Start path from unit's position
        input.movement_path.start(IVec2::new(x, y));
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
            && game_data.terrain_produces(tile.terrain)
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
    highlights.selected_unit_class = None;
    highlights.tiles.clear();
    highlights.tile_costs.clear();
    highlights.attack_targets.clear();
    input.movement_path.clear();
    production_state.active = false;
}

/// Spawn mesh entities for movement and attack highlights
fn spawn_movement_highlight_meshes(
    game_data: Res<GameData>,
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
        commands.entity(entity).despawn();
    }
    for entity in existing_attack_highlights.iter() {
        commands.entity(entity).despawn();
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
            .map(|t| game_data.terrain(t).tile_height)
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
                .map(|t| game_data.terrain(t).tile_height)
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
    egui_wants_input: Res<EguiWantsInput>,
    game_data: Res<GameData>,
) {
    // Don't process input if egui wants it (UI is being clicked)
    if egui_wants_input.wants_any_pointer_input() {
        return;
    }

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

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera_query.single() else { return };

    let Some(cursor_pos) = screen_to_grid(window, camera, camera_transform, &map) else {
        return;
    };

    // Start dragging on mouse down
    if mouse_button.just_pressed(MouseButton::Left) {
        // If path already started (from unit selection), just start dragging
        if !movement_path.path.is_empty() {
            movement_path.dragging = true;
        }
        // Otherwise, check if clicking within movement range to start new path
        else if highlights.tiles.contains(&(cursor_pos.x, cursor_pos.y)) {
            movement_path.start(cursor_pos);
            movement_path.dragging = true;
        }
    }

    // Extend path while dragging
    if movement_path.dragging && mouse_button.pressed(MouseButton::Left) {
        if let Some(last_pos) = movement_path.path.last().copied() {
            if cursor_pos != last_pos {
                if let Some(unit_class) = highlights.selected_unit_class {
                    movement_path.try_extend(cursor_pos, &map, &highlights.tiles, &highlights.tile_costs, unit_class, &game_data);
                }
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
    game_data: Res<GameData>,
    time: Res<Time>,
    mut cursor_repeat: ResMut<CursorRepeat>,
    unit_positions: Query<(Entity, &GridPosition), With<Unit>>,
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

    // Determine current direction being pressed
    let up = keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp);
    let down = keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown);
    let left = keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft);
    let right = keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight);

    let current_dir: Option<(i32, i32)> = match (up, down, left, right) {
        (true, false, false, false) => Some((0, -1)),
        (false, true, false, false) => Some((0, 1)),
        (false, false, true, false) => Some((-1, 0)),
        (false, false, false, true) => Some((1, 0)),
        _ => None,
    };

    // Check if direction just changed or was just pressed
    let just_pressed_up = keyboard.just_pressed(KeyCode::KeyW) || keyboard.just_pressed(KeyCode::ArrowUp);
    let just_pressed_down = keyboard.just_pressed(KeyCode::KeyS) || keyboard.just_pressed(KeyCode::ArrowDown);
    let just_pressed_left = keyboard.just_pressed(KeyCode::KeyA) || keyboard.just_pressed(KeyCode::ArrowLeft);
    let just_pressed_right = keyboard.just_pressed(KeyCode::KeyD) || keyboard.just_pressed(KeyCode::ArrowRight);
    let any_just_pressed = just_pressed_up || just_pressed_down || just_pressed_left || just_pressed_right;

    if let Some((dx, dy)) = current_dir {
        let should_move = if any_just_pressed || cursor_repeat.held_direction != current_dir {
            // New key press or direction change - move immediately and reset timing
            cursor_repeat.held_direction = current_dir;
            cursor_repeat.held_time = 0.0;
            cursor_repeat.repeat_time = 0.0;
            true
        } else {
            // Key held - update timing and check for repeat
            cursor_repeat.held_time += time.delta_secs();
            cursor_repeat.repeat_time += time.delta_secs();

            if cursor_repeat.held_time >= cursor_repeat.initial_delay
                && cursor_repeat.repeat_time >= cursor_repeat.repeat_interval
            {
                cursor_repeat.repeat_time = 0.0;
                true
            } else {
                false
            }
        };

        if let (true, Some(unit_class)) = (should_move, highlights.selected_unit_class) {
            let step = IVec2::new(cursor.x + dx, cursor.y + dy);
            let extended = movement_path.ends_at(cursor.x, cursor.y) || movement_path.path.len() == 1
                && movement_path.path.last() == Some(&IVec2::new(cursor.x, cursor.y));
            if extended && movement_path.try_extend(step, &map, &highlights.tiles, &highlights.tile_costs, unit_class, &game_data) {
                cursor.x = step.x;
                cursor.y = step.y;
            } else {
                // Can't step there (a unit in the way, out of range): skip ahead to
                // the next tile in this direction the unit can move to or attack,
                // and draw the cheapest route to it
                let attack_positions = || unit_positions.iter()
                    .filter(|(e, _)| highlights.attack_targets.contains(e))
                    .map(|(_, p)| (p.x, p.y));
                let next = (1..map.width.max(map.height) as i32)
                    .map(|i| (cursor.x + dx * i, cursor.y + dy * i))
                    .take_while(|&(x, y)| map.get(x, y).is_some())
                    .find(|&tile| cursor_allowed(&highlights, tile, attack_positions));
                if let Some((x, y)) = next {
                    cursor.x = x;
                    cursor.y = y;
                    let start = movement_path.path.first().copied();
                    if let (Some(start), Some(mover)) = (start, highlights.selected_unit) {
                        if highlights.tiles.contains(&(x, y)) {
                            let occupied = unit_positions.iter()
                                .filter(|(e, _)| *e != mover)
                                .map(|(_, p)| (p.x, p.y))
                                .collect();
                            if let Some(route) = find_path(start, IVec2::new(x, y), &map, &occupied, unit_class, &game_data) {
                                movement_path.reroute(route, &map, unit_class, &game_data);
                            }
                        }
                    }
                }
            }
            cursor.visible = true;
            cursor.follow = true;
        }
    } else {
        // No direction pressed - reset repeat state
        cursor_repeat.held_direction = None;
        cursor_repeat.held_time = 0.0;
        cursor_repeat.repeat_time = 0.0;
    }
}

/// Create an arrow mesh pointing in the +X direction (triangle)
fn create_arrow_mesh() -> Mesh {
    // Arrow triangle vertices (pointing in +X direction, lying on XZ plane)
    // Made larger for visibility - about 1/3 of tile size
    let arrow_size = TILE_SIZE * 0.35;
    let arrow_width = TILE_SIZE * 0.25;

    // Create a Triangle3d primitive and convert to mesh
    let triangle = Triangle3d::new(
        Vec3::new(arrow_size, 0.0, 0.0),              // Tip (front)
        Vec3::new(-arrow_size * 0.4, 0.0, arrow_width),  // Back left
        Vec3::new(-arrow_size * 0.4, 0.0, -arrow_width), // Back right
    );

    Mesh::from(triangle)
}

/// Spawn mesh entities for path indicators
fn spawn_path_indicator_meshes(
    game_data: Res<GameData>,
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
        commands.entity(entity).despawn();
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
            .map(|t| game_data.terrain(t).tile_height)
            .unwrap_or(4.0);

        let is_destination = i == movement_path.path.len() - 1;
        let is_start = i == 0;

        // Draw connecting arrow to next tile (skip for destination)
        if !is_destination {
            let next_pos = movement_path.path[i + 1];
            let next_world_x = next_pos.x as f32 * TILE_SIZE + offset_x;
            let next_world_z = next_pos.y as f32 * TILE_SIZE + offset_z;
            let next_tile_height = map.get(next_pos.x, next_pos.y)
                .map(|t| game_data.terrain(t).tile_height)
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

/// Draw the grid cursor (Advance Wars style corner brackets)
fn draw_grid_cursor(
    game_data: Res<GameData>,
    cursor: Res<GridCursor>,
    mut gizmos: Gizmos,
    map: Res<GameMap>,
    time: Res<Time>,
) {
    if !cursor.visible {
        return;
    }

    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    let world_x = cursor.x as f32 * TILE_SIZE + offset_x;
    let world_z = cursor.y as f32 * TILE_SIZE + offset_z;

    // Get tile height to position cursor on top of tile
    let tile_height = map.get(cursor.x, cursor.y)
        .map(|t| game_data.terrain(t).tile_height)
        .unwrap_or(4.0);
    let y = tile_height + 0.2; // Height above tile surface

    // Subtle animation - gentle pulse
    let pulse = (time.elapsed_secs() * 4.0).sin() * 0.08 + 0.92;

    // Corner bracket dimensions
    let half_tile = (TILE_SIZE / 2.0 - 1.0) * pulse; // Distance from center to corner
    let bracket_len = TILE_SIZE * 0.35; // Length of each bracket arm

    let color = Color::srgb(1.0, 1.0, 0.0); // Bright yellow

    // Draw thicker L-shaped brackets by drawing multiple offset lines
    for offset in [0.0, 0.3, 0.6] {
        let y_off = y + offset * 0.02;

        // Top-left corner
        let tl = Vec3::new(world_x - half_tile, y_off, world_z - half_tile);
        gizmos.line(tl, Vec3::new(tl.x + bracket_len, y_off, tl.z), color);
        gizmos.line(tl, Vec3::new(tl.x, y_off, tl.z + bracket_len), color);

        // Top-right corner
        let tr = Vec3::new(world_x + half_tile, y_off, world_z - half_tile);
        gizmos.line(tr, Vec3::new(tr.x - bracket_len, y_off, tr.z), color);
        gizmos.line(tr, Vec3::new(tr.x, y_off, tr.z + bracket_len), color);

        // Bottom-left corner
        let bl = Vec3::new(world_x - half_tile, y_off, world_z + half_tile);
        gizmos.line(bl, Vec3::new(bl.x + bracket_len, y_off, bl.z), color);
        gizmos.line(bl, Vec3::new(bl.x, y_off, bl.z - bracket_len), color);

        // Bottom-right corner
        let br = Vec3::new(world_x + half_tile, y_off, world_z + half_tile);
        gizmos.line(br, Vec3::new(br.x - bracket_len, y_off, br.z), color);
        gizmos.line(br, Vec3::new(br.x, y_off, br.z - bracket_len), color);
    }
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
    game_data: Res<GameData>,
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

        let stats = game_data.unit_stats(unit.unit_type);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn map_from_rows(rows: &[&[Terrain]]) -> GameMap {
        GameMap {
            width: rows[0].len() as u32,
            height: rows.len() as u32,
            tiles: rows.iter().map(|r| r.to_vec()).collect(),
        }
    }

    fn assert_orthogonal(route: &[IVec2]) {
        for pair in route.windows(2) {
            let step = (pair[1] - pair[0]).abs();
            assert_eq!(step.x + step.y, 1, "diagonal or jumping step {:?} -> {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn drawn_path_costs_each_tiles_terrain_once() {
        use Terrain::*;
        let data = GameData::load_defaults();
        let map = map_from_rows(&[&[Grass, Grass, Thicket, Grass]]);
        let tiles: HashSet<_> = (0..4).map(|x| (x, 0)).collect();
        let mut path = MovementPath::default();
        path.start(IVec2::new(0, 0));
        for x in 1..4 {
            assert!(path.try_extend(IVec2::new(x, 0), &map, &tiles, &HashMap::new(), UnitClass::Foot, &data));
        }
        assert_eq!(path.total_cost, 1 + 2 + 1, "grass, thicket, grass");
    }

    #[test]
    fn cursor_stays_in_range_while_a_unit_is_selected() {
        let mut highlights = MovementHighlights::default();
        let enemies = || [(5, 5)].into_iter();
        assert!(cursor_allowed(&highlights, (9, 9), enemies), "free roam with nothing selected");
        highlights.selected_unit = Some(Entity::PLACEHOLDER);
        highlights.tiles.insert((1, 1));
        assert!(cursor_allowed(&highlights, (1, 1), enemies));
        assert!(cursor_allowed(&highlights, (5, 5), enemies), "an attackable enemy");
        assert!(!cursor_allowed(&highlights, (9, 9), enemies));
    }

    #[test]
    fn path_goes_around_water_in_straight_steps() {
        use Terrain::*;
        let data = GameData::load_defaults();
        let map = map_from_rows(&[
            &[Grass, Grass, Grass],
            &[Grass, Pond, Grass],
            &[Grass, Grass, Grass],
        ]);
        let route = find_path(IVec2::new(0, 1), IVec2::new(2, 1), &map, &HashSet::new(), UnitClass::Foot, &data).unwrap();
        assert_orthogonal(&route);
        assert_eq!(route.len(), 5);
        assert!(!route.contains(&IVec2::new(1, 1)));
    }

    #[test]
    fn path_avoids_occupied_tiles_but_may_end_on_one() {
        use Terrain::*;
        let data = GameData::load_defaults();
        let map = map_from_rows(&[&[Grass, Grass, Grass], &[Grass, Grass, Grass]]);
        let occupied = HashSet::from([(1, 0), (2, 0)]);
        let route = find_path(IVec2::new(0, 0), IVec2::new(2, 0), &map, &occupied, UnitClass::Foot, &data).unwrap();
        assert_orthogonal(&route);
        assert!(!route.contains(&IVec2::new(1, 0)));
        assert_eq!(route.last(), Some(&IVec2::new(2, 0)));
    }

    #[test]
    fn fallback_waypoints_never_cut_corners() {
        use Terrain::*;
        let map = map_from_rows(&[&[Grass; 3], &[Grass; 3]]);
        let points = route_waypoints(None, IVec2::new(0, 0), IVec2::new(2, 1), 0.0, &map);
        assert_eq!(points.len(), 3);
        for pair in points.windows(2) {
            assert!(pair[0].x == pair[1].x || pair[0].z == pair[1].z, "diagonal segment");
        }
    }

    #[test]
    fn movement_range_pays_terrain_costs() {
        use Terrain::*;
        let data = GameData::load_defaults();
        let map = map_from_rows(&[&[Grass, Grass, Thicket, Grass, Grass]]);
        let reach = calculate_movement_range(&GridPosition::new(0, 0), 3, &map, &HashMap::new(), UnitClass::Foot, &data);
        // grass(1) + thicket(2) = 3 movement used reaching x=2
        assert!(reach.contains(&(2, 0)));
        assert!(!reach.contains(&(3, 0)));
    }

    #[test]
    fn movement_range_blocked_by_impassable_terrain() {
        use Terrain::*;
        let data = GameData::load_defaults();
        let map = map_from_rows(&[&[Grass, Pond, Grass]]);
        let foot = calculate_movement_range(&GridPosition::new(0, 0), 5, &map, &HashMap::new(), UnitClass::Foot, &data);
        assert!(!foot.contains(&(1, 0)) && !foot.contains(&(2, 0)));
        let air = calculate_movement_range(&GridPosition::new(0, 0), 5, &map, &HashMap::new(), UnitClass::Air, &data);
        assert!(air.contains(&(2, 0)));
    }

    #[test]
    fn attack_targets_skip_friendlies_and_untargetable_units() {
        let data = GameData::load_defaults();
        let scout = Unit::new(UnitType::Scout, &data);
        let me = FactionMember { faction: Faction::Eastern };
        let mut world = World::new();
        let [enemy, friend, enemy_jet, far_enemy] = [(); 4].map(|_| world.spawn_empty().id());
        let enemy_faction = FactionMember { faction: Faction::Northern };
        let units = vec![
            (enemy, GridPosition::new(1, 0), enemy_faction.clone(), UnitType::Scout),
            (friend, GridPosition::new(0, 1), me.clone(), UnitType::Scout),
            (enemy_jet, GridPosition::new(-1, 0), enemy_faction.clone(), UnitType::Raptor),
            (far_enemy, GridPosition::new(3, 0), enemy_faction, UnitType::Scout),
        ];
        let targets = calculate_attack_targets(&scout, &GridPosition::new(0, 0), &me, &units, &data);
        assert_eq!(targets, HashSet::from([enemy]));
    }
}
