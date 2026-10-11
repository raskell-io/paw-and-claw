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
    /// The player's fog: enemies they can't see can't be targeted, don't
    /// block planned routes and spring traps when walked into
    pub fog: Res<'w, super::FogOfWar>,
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
    /// Every passable terrain costs 1 (the mover's CO; AW Lash, Sturm)
    pub flat_cost: bool,
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
        _valid_tiles: &HashSet<(i32, i32)>,
        tile_costs: &HashMap<(i32, i32), u32>,
        unit_class: UnitClass,
        game_data: &GameData,
    ) -> bool {
        if self.path.is_empty() {
            return false;
        }

        // Anywhere within reach, including through friendly units (only the
        // final tile must be one the unit can stop on)
        if !tile_costs.contains_key(&(pos.x, pos.y)) {
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
            self.total_cost += game_data.step_cost(terrain, unit_class, self.flat_cost);
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

    // Intersect with the plane of the tile tops (land tiles stand 4 units tall),
    // so the tile under the pointer is the one that looks under it
    const PICK_HEIGHT: f32 = 4.0;
    if ray.direction.y.abs() < 0.0001 {
        return None; // Ray parallel to ground
    }
    let t = (PICK_HEIGHT - ray.origin.y) / ray.direction.y;
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
            .init_resource::<MapMenu>()
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
                animate_highlight_gloss,
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
            // Both limits are set per map and window by `fit_camera_to_map`
            min: 0.3,
            max: 4.0,
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

/// Advance Wars map menu: opened by confirming on a tile with nothing to do
/// (no ready unit, no free building), it holds the powers and End Turn
#[derive(Resource, Default)]
pub struct MapMenu {
    pub open: bool,
}

/// Resource to track movement range and attack target highlights
#[derive(Resource, Default)]
pub struct MovementHighlights {
    pub tiles: HashSet<(i32, i32)>,
    pub tile_costs: HashMap<(i32, i32), u32>,  // Cost to reach each tile (for stamina)
    pub attack_targets: HashSet<Entity>,  // Enemy units that can be attacked
    pub selected_unit: Option<Entity>,
    pub selected_unit_class: Option<UnitClass>,  // Unit class for movement cost lookup
    /// The selected unit's CO makes every passable terrain cost 1 (AW Lash, Sturm)
    pub flat_cost: bool,
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
    co: &super::CoBonuses,
) -> HashSet<Entity> {
    let mut targets = HashSet::new();
    let (min_range, max_range) = game_data.unit_stats(attacker.unit_type).attack_range;
    // CO range changes apply to indirect units (AW Grit, Max)
    let max_range = if min_range > 1 {
        (max_range as i32 + co.range(attacker.unit_type, game_data)).max(min_range as i32) as u32
    } else {
        max_range
    };

    for (entity, pos, faction, unit_type) in units {
        // Can't attack own faction
        if faction.faction == attacker_faction.faction {
            continue;
        }

        // A weapon that can hit this unit type and has ammo, or a machine gun
        if game_data.weapon(attacker.unit_type, attacker.ammo, *unit_type).is_none() {
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
/// through `occupied` tiles, which callers fill with enemy units (friendly units
/// can be passed, as in Advance Wars). Units never cut corners, so this is the
/// route a move animates along.
pub fn find_path(
    start: IVec2,
    goal: IVec2,
    map: &GameMap,
    occupied: &HashSet<(i32, i32)>,
    unit_class: UnitClass,
    flat_cost: bool,
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
            let next_cost = cost + game_data.step_cost(terrain, unit_class, flat_cost);
            if next_cost < best.get(&next).copied().unwrap_or(u32::MAX) {
                best.insert(next, next_cost);
                came_from.insert(next, here);
                open.push(Reverse((next_cost, next.x, next.y)));
            }
        }
    }
    None
}

/// The orthogonal route a player's unit takes to `goal` when no path was drawn,
/// around the enemies the player can see
fn fallback_move_route(
    units: &Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    mover: Entity,
    goal: IVec2,
    unit_class: Option<UnitClass>,
    map: &GameMap,
    game_data: &GameData,
    fog: &super::FogOfWar,
    flat_cost: bool,
) -> Option<Vec<IVec2>> {
    let (start, side) = units.get(mover).map(|(_, p, _, f, _)| (IVec2::new(p.x, p.y), f.faction)).ok()?;
    let occupied = units.iter()
        .filter(|(_, p, _, f, _)| f.faction != side && fog.is_visible(p.x, p.y))
        .map(|(_, p, ..)| (p.x, p.y))
        .collect();
    find_path(start, goal, map, &occupied, unit_class?, flat_cost, game_data)
}

/// The tiles a move walks: the drawn route if it ends at the goal, else the
/// fallback route, else one axis then the other
fn planned_route(path: &MovementPath, fallback: Option<Vec<IVec2>>, start: IVec2, goal: IVec2) -> Vec<IVec2> {
    if path.ends_at(goal.x, goal.y) {
        return path.path.clone();
    }
    fallback.unwrap_or_else(|| {
        let corner = IVec2::new(goal.x, start.y);
        if corner == start || corner == goal { vec![start, goal] } else { vec![start, corner, goal] }
    })
}

/// Advance Wars trap: a route into an enemy hidden in fog stops on the last
/// free tile before it, and the unit's turn ends. Returns whether it sprang.
fn spring_trap(route: &mut Vec<IVec2>, hidden_enemies: &HashSet<(i32, i32)>, friends: &HashSet<(i32, i32)>) -> bool {
    let Some(hit) = route.iter().position(|p| hidden_enemies.contains(&(p.x, p.y))) else { return false };
    route.truncate(hit.max(1));
    while route.len() > 1 && route.last().is_some_and(|p| friends.contains(&(p.x, p.y))) {
        route.pop();
    }
    true
}

/// Enemy units the player can't see (traps), and the mover's own army's tiles
fn hidden_enemies_and_friends(
    units: &Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    mover: Entity,
    fog: &super::FogOfWar,
) -> (HashSet<(i32, i32)>, HashSet<(i32, i32)>) {
    let side = units.get(mover).map(|(_, _, _, f, _)| f.faction).ok();
    let mut hidden = HashSet::new();
    let mut friends = HashSet::new();
    for (entity, pos, _, faction, _) in units.iter() {
        if Some(faction.faction) == side {
            if entity != mover {
                friends.insert((pos.x, pos.y));
            }
        } else if !fog.is_visible(pos.x, pos.y) {
            hidden.insert((pos.x, pos.y));
        }
    }
    (hidden, friends)
}

/// A trapped unit walks to where it stopped and its turn ends (no action menu)
fn end_move_in_trap(
    commands: &mut Commands,
    unit: Entity,
    highlights: &mut MovementHighlights,
    path: &mut MovementPath,
    pending_action: &mut PendingAction,
    turn_state: &mut TurnState,
) {
    info!("Trap! An enemy was hiding in the fog");
    commands.entity(unit).insert(super::AutoExhaust);
    highlights.selected_unit = None;
    highlights.selected_unit_class = None;
    highlights.tiles.clear();
    highlights.tile_costs.clear();
    highlights.attack_targets.clear();
    path.clear();
    pending_action.unit = None;
    turn_state.phase = TurnPhase::Select;
}

/// Whether an owned, empty tile of this terrain builds units for this CO
/// (storehouses do too under AW Hachi's Merchant Union)
pub fn builds_here(terrain: Terrain, co: &super::CoBonuses, game_data: &GameData) -> bool {
    game_data.terrain_produces(terrain) || (co.mods.build_on_cities && terrain == Terrain::Storehouse)
}

/// Stamina a route costs (every tile after the first)
fn route_cost(route: &[IVec2], map: &GameMap, unit_class: UnitClass, flat_cost: bool, game_data: &GameData) -> u32 {
    route.iter().skip(1).filter_map(|p| map.get(p.x, p.y)).map(|t| game_data.step_cost(t, unit_class, flat_cost)).sum()
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

/// Reachable tiles under Advance Wars rules: enemy units block, friendly units
/// can be passed through but not stopped on, except to join a unit of the same
/// type when at least one of the two is wounded, or to board a transport with
/// room for the mover. `all_units` holds (entity, tile, faction, type, at full
/// HP, has room for the mover).
pub fn calculate_movement_range_with_joins(
    start: &GridPosition,
    movement: u32,
    map: &GameMap,
    moving_unit_type: super::UnitType,
    moving_faction: Faction,
    all_units: &[(Entity, (i32, i32), Faction, super::UnitType, bool, bool)],
    unit_class: UnitClass,
    flat_cost: bool,
    game_data: &GameData,
) -> (HashSet<(i32, i32)>, HashMap<(i32, i32), u32>) {
    let mover_full = all_units.iter()
        .find(|(_, tile, ..)| *tile == (start.x, start.y))
        .is_some_and(|(.., full, _)| *full);
    let mut enemies: HashSet<(i32, i32)> = HashSet::new();
    let mut friends: HashSet<(i32, i32)> = HashSet::new();
    let mut joinable: HashSet<(i32, i32)> = HashSet::new();
    for &(_, tile, faction, unit_type, full, has_room) in all_units {
        if tile == (start.x, start.y) {
            continue;
        }
        if faction != moving_faction {
            enemies.insert(tile);
        } else {
            friends.insert(tile);
            if (unit_type == moving_unit_type && !(full && mover_full)) || has_room {
                joinable.insert(tile);
            }
        }
    }

    let mut visited: HashMap<(i32, i32), u32> = HashMap::from([((start.x, start.y), 0)]);
    let mut queue = VecDeque::from([(start.x, start.y, 0u32)]);
    while let Some((x, y, cost)) = queue.pop_front() {
        for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            let next = (x + dx, y + dy);
            let Some(terrain) = map.get(next.0, next.1) else { continue };
            if enemies.contains(&next) {
                continue;
            }
            let new_cost = cost + game_data.step_cost(terrain, unit_class, flat_cost);
            if new_cost <= movement && visited.get(&next).is_none_or(|&prev| new_cost < prev) {
                visited.insert(next, new_cost);
                queue.push_back((next.0, next.1, new_cost));
            }
        }
    }
    // Every tile within reach is a destination, except friendly-occupied ones
    // (unless joining there)
    let reachable = visited.keys().copied()
        .filter(|tile| !friends.contains(tile) || joinable.contains(tile))
        .collect();
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
    mut map_menu: ResMut<MapMenu>,
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
                let weather_movement = co_bonuses.total_movement(unit.unit_type, &game_ctx.weather, &game_data);
                let total_movement = effective_movement(weather_movement, unit.stamina);

                // Build unit list for join-aware movement
                let all_unit_info: Vec<_> = units.iter()
                    .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type, u.display_hp() >= 10, u.can_carry(unit.unit_type, &game_data)))
                    .collect();
                let unit_class = stats.class;
                let (tiles, tile_costs) = calculate_movement_range_with_joins(
                    &pos, total_movement, &map, unit.unit_type, faction.faction, &all_unit_info,
                    unit_class, co_bonuses.mods.flat_terrain_cost, &game_data
                );

                // Calculate attack targets
                let all_units: Vec<_> = units.iter()
                    .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                    .collect();
                let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units, &game_data, &game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data));

                highlights.selected_unit = Some(entity);
                highlights.selected_unit_class = Some(unit_class);
                highlights.flat_cost = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data).mods.flat_terrain_cost;
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
        // The cursor stays up, as in Advance Wars: hiding it here used to hand
        // the arrow keys to camera panning with no cursor on the board
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
                let selected_unit_info = units.get(selected_entity).map(|(_, _, _, f, u)| (f.faction, u.unit_type, u.display_hp() >= 10));
                // AW: join a same-type unit when at least one of the two is wounded
                let joinable_unit = units.iter()
                    .find(|(e, p, _, f, u)| {
                        *e != selected_entity
                            && p.x == input.cursor.x && p.y == input.cursor.y
                            && selected_unit_info.map_or(false, |(sf, su, full)| f.faction == sf
                                && ((u.unit_type == su && !(full && u.display_hp() >= 10)) || u.can_carry(su, &game_data)))
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
                    let fallback_route = fallback_move_route(&units, selected_entity, IVec2::from((input.cursor.x, input.cursor.y)), highlights.selected_unit_class, &map, &game_data, &game_ctx.fog, highlights.flat_cost);
                    let (hidden_enemies, friends) = hidden_enemies_and_friends(&units, selected_entity, &game_ctx.fog);
                    let mut trapped = false;
                    if let Ok((_, mut grid_pos, transform, faction, mut unit)) = units.get_mut(selected_entity) {
                        let start_pos = transform.translation;
                        // Save original position before moving (for cancel)
                        original_pos = (grid_pos.x, grid_pos.y);
                        grid_pos.x = input.cursor.x;
                        grid_pos.y = input.cursor.y;

                        // Only create animation if actually moving
                        if !staying_in_place {
                            let start = IVec2::from(original_pos);
                            let mut route = planned_route(&input.movement_path, fallback_route, start, IVec2::new(grid_pos.x, grid_pos.y));
                            let mut move_cost = move_cost;
                            if spring_trap(&mut route, &hidden_enemies, &friends) {
                                trapped = true;
                                let stop = *route.last().unwrap_or(&start);
                                (grid_pos.x, grid_pos.y) = (stop.x, stop.y);
                                move_cost = route_cost(&route, &map, game_data.unit_class(unit.unit_type), highlights.flat_cost, &game_data);
                            }
                            let waypoints = route_waypoints(Some(route), start, start, start_pos.y, &map);

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
                    if trapped {
                        end_move_in_trap(&mut commands, selected_entity, &mut highlights, &mut input.movement_path, &mut pending_action, &mut turn_state);
                        return;
                    }

                    // Calculate attack targets from current/new position
                    let all_units: Vec<_> = units.iter()
                        .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                        .collect();
                    let targets = calculate_attack_targets(&unit_copy, &new_pos, &faction_copy, &all_units, &game_data, &game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data));

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
                    let (can_join, join_target) = if let Some(join_entity) = joinable_unit
                        .filter(|&e| units.get(e).is_ok_and(|(_, _, _, _, u)| selected_unit_info.as_ref().ok().map(|i| i.1) == Some(u.unit_type)))
                    {
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
                    // AW: indirect units (minimum range 2+) can't fire after moving
                    let indirect_moved = !staying_in_place && game_data.unit_stats(unit_copy.unit_type).attack_range.0 > 1;
                    pending_action.targets = if unit_copy.attacked || indirect_moved { HashSet::new() } else { targets };
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
                    let co_bonuses = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data);
                    let weather_movement = co_bonuses.total_movement(unit.unit_type, &game_ctx.weather, &game_data);
                    // Limit movement by stamina
                    let total_movement = effective_movement(weather_movement, unit.stamina);

                    // Build unit list for join-aware movement
                    let all_unit_info: Vec<_> = units.iter()
                        .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type, u.display_hp() >= 10, u.can_carry(unit.unit_type, &game_data)))
                        .collect();
                    let unit_class = game_data.unit_stats(unit.unit_type).class;
                    let (tiles, tile_costs) = calculate_movement_range_with_joins(
                        &pos, total_movement, &map, unit.unit_type, faction.faction, &all_unit_info,
                        unit_class, co_bonuses.mods.flat_terrain_cost, &game_data
                    );

                    // Calculate attack targets
                    let all_units: Vec<_> = units.iter()
                        .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                        .collect();
                    let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units, &game_data, &game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data));

                    found_unit = Some((entity, tiles, tile_costs, attack_targets, unit_class));
                    break;
                }
            }
            if let Some((entity, tiles, tile_costs, attack_targets, unit_class)) = found_unit {
                highlights.selected_unit = Some(entity);
                highlights.selected_unit_class = Some(unit_class);
                highlights.flat_cost = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data).mods.flat_terrain_cost;
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
                        && builds_here(tile.terrain, &game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data), &game_data)
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
                // Nothing to do here: the map menu (powers, End Turn)
                if !production_state.active {
                    map_menu.open = true;
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
/// The closest zoom shows about this many tiles across the screen, so the board
/// never gets close enough to pick out single flowers and blades of grass
const ZOOM_IN_TILES: f32 = 13.0;

/// Set the zoom limits for the map and window: zoomed fully out the whole board
/// fits with a little of the canvas around it, zoomed fully in about
/// `ZOOM_IN_TILES` span the screen. A new map starts fully out.
fn fit_camera_to_map(
    map: Res<GameMap>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    projection: Query<&Projection, With<Camera3d>>,
    mut resized: MessageReader<bevy::window::WindowResized>,
    mut zoom: ResMut<CameraZoom>,
    mut angle: ResMut<CameraAngle>,
) {
    let window_changed = resized.read().count() > 0;
    let new_map = map.is_changed();
    if !(new_map || window_changed) || map.width == 0 || map.height == 0 {
        return;
    }
    let aspect = windows.single().map(|w| w.width() / w.height().max(1.0)).unwrap_or(16.0 / 9.0);
    let fov = match projection.single() {
        Ok(Projection::Perspective(p)) => p.fov,
        _ => std::f32::consts::FRAC_PI_6,
    };
    let (width, depth) = (map.width as f32, map.height as f32);
    let zoom_for = |w: f32, d: f32| fit_height(w * TILE_SIZE, d * TILE_SIZE, aspect, angle.perspective_pitch, fov) / angle.perspective_height;
    zoom.max = zoom_for(width, depth);
    zoom.min = zoom_for(width.min(ZOOM_IN_TILES), 0.0).min(zoom.max);
    if new_map {
        zoom.current = zoom.max;
        angle.look_at = Vec2::ZERO;
        angle.mode = CameraMode::Perspective;
    } else {
        zoom.current = zoom.current.clamp(zoom.min, zoom.max);
    }
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
    map: Res<GameMap>,
    mut camera_query: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok(mut camera_transform) = camera_query.single_mut() else { return };

    // Keep the board on screen: fully zoomed out it's framed in the middle, and
    // the closer the zoom the further the view may pan, up to the board's edges
    let slack = (1.0 - zoom.current / zoom.max.max(0.001)).max(0.0);
    let reach = Vec2::new(map.width as f32, map.height as f32) * TILE_SIZE / 2.0 * slack;
    angle.look_at = angle.look_at.clamp(-reach, reach);

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
    unit_positions: Query<(Entity, &GridPosition, &FactionMember), With<Unit>>,
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

    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = cameras.single() else { return };

    if let Some(grid_pos) = screen_to_grid(window, camera, camera_transform, &map) {
        let attack_positions = || unit_positions.iter()
            .filter(|(e, ..)| highlights.attack_targets.contains(e))
            .map(|(_, p, _)| (p.x, p.y));
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
                    let unit_list: Vec<_> = units.iter()
                        .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type, u.display_hp() >= 10, u.can_carry(unit.unit_type, &game_data)))
                        .collect();
                    let stats = game_data.unit_stats(unit.unit_type);
                    let co_bonuses = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data);
                    let total_movement = effective_movement(co_bonuses.total_movement(unit.unit_type, &game_ctx.weather, &game_data), unit.stamina);
                    let (tiles, tile_costs) = calculate_movement_range_with_joins(&pos, total_movement, &map,
                        unit.unit_type, faction.faction, &unit_list, stats.class, co_bonuses.mods.flat_terrain_cost, &game_data);

                    // Calculate attack targets
                    let all_units: Vec<_> = units.iter()
                        .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                        .collect();
                    let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units, &game_data, &game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data));

                    switch_to = Some((entity, tiles, tile_costs, attack_targets, stats.class));
                    break;
                }
            }

            if let Some((entity, tiles, tile_costs, attack_targets, unit_class)) = switch_to {
                highlights.selected_unit = Some(entity);
                highlights.selected_unit_class = Some(unit_class);
                highlights.flat_cost = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data).mods.flat_terrain_cost;
                highlights.tiles = tiles;
                highlights.tile_costs = tile_costs;
                highlights.attack_targets = attack_targets;
                // Start path from unit's new position
                input.movement_path.start(IVec2::new(grid_x, grid_y));
                info!("Switched selection to unit at ({}, {})", grid_x, grid_y);
                return;
            }

            // Check if target has a joinable unit (friendly same-type)
            let selected_unit_info = units.get(selected_entity).map(|(_, _, _, f, u)| (f.faction, u.unit_type, u.display_hp() >= 10));
            // AW: join a same-type unit when at least one of the two is wounded
            let joinable_unit = units.iter()
                .find(|(e, p, _, f, u)| {
                    *e != selected_entity
                        && p.x == grid_x && p.y == grid_y
                        && selected_unit_info.map_or(false, |(sf, su, full)| f.faction == sf
                            && ((u.unit_type == su && !(full && u.display_hp() >= 10)) || u.can_carry(su, &game_data)))
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
            let fallback_route = fallback_move_route(&units, selected_entity, IVec2::from((grid_x, grid_y)), highlights.selected_unit_class, &map, &game_data, &game_ctx.fog, highlights.flat_cost);
            let (hidden_enemies, friends) = hidden_enemies_and_friends(&units, selected_entity, &game_ctx.fog);
            let mut trapped = false;
            if let Ok((_, mut grid_pos, transform, faction, mut unit)) = units.get_mut(selected_entity) {
                let start_pos = transform.translation;
                // Save original position before moving (for cancel)
                original_pos = (grid_pos.x, grid_pos.y);
                grid_pos.x = grid_x;
                grid_pos.y = grid_y;

                // Only create animation if actually moving
                if !staying_in_place {
                    let start = IVec2::from(original_pos);
                    let mut route = planned_route(&input.movement_path, fallback_route, start, IVec2::new(grid_x, grid_y));
                    let mut move_cost = move_cost;
                    if spring_trap(&mut route, &hidden_enemies, &friends) {
                        trapped = true;
                        let stop = *route.last().unwrap_or(&start);
                        (grid_pos.x, grid_pos.y) = (stop.x, stop.y);
                        move_cost = route_cost(&route, &map, game_data.unit_class(unit.unit_type), highlights.flat_cost, &game_data);
                    }
                    let waypoints = route_waypoints(Some(route), start, start, start_pos.y, &map);

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
            if trapped {
                end_move_in_trap(&mut commands, selected_entity, &mut highlights, &mut input.movement_path, &mut pending_action, &mut turn_state);
                return;
            }

            // Calculate attack targets from new position
            let all_units: Vec<_> = units.iter()
                .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                .collect();
            let targets = calculate_attack_targets(&unit_copy, &new_pos, &faction_copy, &all_units, &game_data, &game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data));

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
            let (can_join, join_target) = if let Some(join_entity) = joinable_unit
                        .filter(|&e| units.get(e).is_ok_and(|(_, _, _, _, u)| selected_unit_info.as_ref().ok().map(|i| i.1) == Some(u.unit_type)))
                    {
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
            // AW: indirect units (minimum range 2+) can't fire after moving
                    let indirect_moved = !staying_in_place && game_data.unit_stats(unit_copy.unit_type).attack_range.0 > 1;
                    pending_action.targets = if unit_copy.attacked || indirect_moved { HashSet::new() } else { targets };
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
            let weather_movement = co_bonuses.total_movement(unit.unit_type, &game_ctx.weather, &game_data);
            // Limit movement by stamina
            let total_movement = effective_movement(weather_movement, unit.stamina);

            // Build unit list for join-aware movement
            let all_unit_info: Vec<_> = units.iter()
                .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, (p.x, p.y), f.faction, u.unit_type, u.display_hp() >= 10, u.can_carry(unit.unit_type, &game_data)))
                .collect();
            let (move_tiles, move_costs) = calculate_movement_range_with_joins(
                &pos, total_movement, &map, unit.unit_type, faction.faction, &all_unit_info,
                stats.class, co_bonuses.mods.flat_terrain_cost, &game_data
            );

            // Calculate attack targets
            let all_units: Vec<_> = units.iter()
                .filter(|(_, p, _, f, _)| f.faction == turn_state.current_faction || game_ctx.fog.is_visible(p.x, p.y))
                        .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.unit_type))
                .collect();
            let attack_targets = calculate_attack_targets(&unit, &pos, &faction, &all_units, &game_data, &game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data));

            select_unit = Some((entity, move_tiles, move_costs, attack_targets, pos.x, pos.y, stats.class));
            break;
        }
    }

    if let Some((entity, tiles, tile_costs, attack_targets, x, y, unit_class)) = select_unit {
        highlights.selected_unit = Some(entity);
        highlights.selected_unit_class = Some(unit_class);
        highlights.flat_cost = game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data).mods.flat_terrain_cost;
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
            && builds_here(tile.terrain, &game_ctx.commanders.get_bonuses(turn_state.current_faction, &game_data), &game_data)
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
    mut images: ResMut<Assets<Image>>,
    mut gloss: Local<Option<HighlightGloss>>,
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

    let gloss = gloss.get_or_insert_with(|| HighlightGloss::new(&mut images, &mut materials)).clone();
    let slab = meshes.add(Cuboid::new(TILE_SIZE * 0.9, HIGHLIGHT_SLAB_HEIGHT, TILE_SIZE * 0.9));
    let mut spawn_slab = |x: i32, y: i32, material: &Handle<StandardMaterial>, marker: bool| {
        let world = GridPosition::new(x, y).to_world(&map);
        let top = map.get(x, y).map_or(4.0, |t| game_data.terrain(t).tile_height);
        let mut entity = commands.spawn((
            Mesh3d(slab.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_xyz(world.x, top + HIGHLIGHT_SLAB_HEIGHT / 2.0 + 0.05, world.z),
        ));
        if marker { entity.insert(MovementHighlightMesh); } else { entity.insert(AttackHighlightMesh); }
    };

    // Movement range: translucent blue slabs with a shine sweeping across
    for &(x, y) in &highlights.tiles {
        spawn_slab(x, y, &gloss.movement, true);
    }
    // Attack targets: the same in red
    for target_entity in &highlights.attack_targets {
        if let Ok((pos, _)) = units.get(*target_entity) {
            spawn_slab(pos.x, pos.y, &gloss.attack, false);
        }
    }
}

/// Height of the translucent range slabs on top of each tile
const HIGHLIGHT_SLAB_HEIGHT: f32 = 1.6;

/// The glossy range materials (Advance Wars style): a translucent tint with a
/// bright diagonal band that sweeps across every highlighted tile in step
#[derive(Clone)]
struct HighlightGloss {
    movement: Handle<StandardMaterial>,
    attack: Handle<StandardMaterial>,
}

impl HighlightGloss {
    /// 32px repeating texture: mostly a soft translucent white, with a bright
    /// diagonal band; tinted per use by the material color
    fn new(images: &mut Assets<Image>, materials: &mut Assets<StandardMaterial>) -> Self {
        const SIZE: u32 = 32;
        let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let d = ((x + y) % SIZE) as f32;
                // A broad, soft band (about half the tile), brightest in the middle
                let band = (1.0 - ((d - 8.0).abs() / 8.0)).max(0.0).powf(1.5);
                let alpha = 0.36 + band * 0.45;
                let white = 200.0 + band * 55.0;
                data.extend([white as u8, white as u8, white as u8, (alpha * 255.0) as u8]);
            }
        }
        let mut image = Image::new(
            bevy::render::render_resource::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
            bevy::render::render_resource::TextureDimension::D2,
            data,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        image.sampler = bevy::image::ImageSampler::Descriptor(bevy::image::ImageSamplerDescriptor {
            address_mode_u: bevy::image::ImageAddressMode::Repeat,
            address_mode_v: bevy::image::ImageAddressMode::Repeat,
            ..bevy::image::ImageSamplerDescriptor::nearest()
        });
        let texture = images.add(image);
        let material = |tint: Color| StandardMaterial {
            base_color: tint,
            base_color_texture: Some(texture.clone()),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        };
        Self {
            movement: materials.add(material(Color::srgb(0.35, 0.65, 1.0))),
            attack: materials.add(material(Color::srgb(1.0, 0.35, 0.3))),
        }
    }
}

/// Slide the shine diagonally across the range slabs
fn animate_highlight_gloss(
    time: Res<Time>,
    highlights: Query<&MeshMaterial3d<StandardMaterial>, Or<(With<MovementHighlightMesh>, With<AttackHighlightMesh>)>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // One sweep every 1.6 seconds; materials are shared, so update each once
    let offset = (time.elapsed_secs() / 1.6).fract();
    let mut seen = Vec::new();
    for material in highlights.iter() {
        if seen.contains(&material.0.id()) {
            continue;
        }
        seen.push(material.0.id());
        if let Some(material) = materials.get_mut(&material.0) {
            material.uv_transform = bevy::math::Affine2::from_translation(Vec2::new(-offset, 0.0));
        }
    }
}

/// With a unit selected, the route arrow follows the mouse (like Dual Strike's
/// stylus): hovering a reachable tile extends the route there, or redraws the
/// cheapest one when the pointer jumps. Clicking then moves (`handle_click_input`).
/// Only actual pointer movement counts, so a resting mouse never overrides a
/// route drawn with the keys.
fn handle_path_drawing(
    windows: Query<&Window>,
    camera_query: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    map: Res<GameMap>,
    highlights: Res<MovementHighlights>,
    mut movement_path: ResMut<MovementPath>,
    input_mode: Res<InputMode>,
    egui_wants_input: Res<EguiWantsInput>,
    game_data: Res<GameData>,
    mut pointer_moved: MessageReader<CursorMoved>,
    unit_positions: Query<(Entity, &GridPosition, &FactionMember), With<Unit>>,
) {
    let moved = pointer_moved.read().count() > 0;
    if egui_wants_input.wants_any_pointer_input() || *input_mode == InputMode::Keyboard {
        return;
    }
    let (Some(mover), Some(unit_class)) = (highlights.selected_unit, highlights.selected_unit_class) else {
        if movement_path.drawing {
            movement_path.clear();
        }
        return;
    };
    if !moved {
        return;
    }
    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera_query.single() else { return };
    let Some(hover) = screen_to_grid(window, camera, camera_transform, &map) else { return };
    if !highlights.tiles.contains(&(hover.x, hover.y)) || movement_path.path.last() == Some(&hover) {
        return;
    }

    let start = match movement_path.path.first() {
        Some(&start) => start,
        None => {
            let Ok((_, pos, _)) = unit_positions.get(mover) else { return };
            let start = IVec2::new(pos.x, pos.y);
            movement_path.start(start);
            start
        }
    };
    movement_path.flat_cost = highlights.flat_cost;
    if !movement_path.try_extend(hover, &map, &highlights.tiles, &highlights.tile_costs, unit_class, &game_data) {
        let side = unit_positions.get(mover).map(|(_, _, f)| f.faction).ok();
        let occupied = unit_positions.iter()
            .filter(|(_, _, f)| Some(f.faction) != side)
            .map(|(_, p, _)| (p.x, p.y))
            .collect();
        if let Some(route) = find_path(start, hover, &map, &occupied, unit_class, highlights.flat_cost, &game_data) {
            movement_path.reroute(route, &map, unit_class, &game_data);
        }
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
    unit_positions: Query<(Entity, &GridPosition, &FactionMember), With<Unit>>,
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

        movement_path.flat_cost = highlights.flat_cost;
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
                    .filter(|(e, ..)| highlights.attack_targets.contains(e))
                    .map(|(_, p, _)| (p.x, p.y));
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
                            let side = unit_positions.get(mover).map(|(_, _, f)| f.faction).ok();
                            let occupied = unit_positions.iter()
                                .filter(|(_, _, f)| Some(f.faction) != side)
                                .map(|(_, p, _)| (p.x, p.y))
                                .collect();
                            if let Some(route) = find_path(start, IVec2::new(x, y), &map, &occupied, unit_class, highlights.flat_cost, &game_data) {
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

/// The route arrow's outline as convex pieces on the ground plane (x, z): a band
/// from tile center to tile center, overlapping at the bends, then an
/// arrowhead on the destination pointing the way the route arrives.
/// `widen` grows every edge outward.
fn route_arrow_shapes(points: &[Vec2], widen: f32) -> Vec<Vec<Vec2>> {
    // Keep only the corners, so each straight run is one piece with no seams
    let mut corners = vec![points[0]];
    for i in 1..points.len() - 1 {
        let (before, after) = ((points[i] - points[i - 1]).normalize(), (points[i + 1] - points[i]).normalize());
        if before.dot(after) < 0.99 {
            corners.push(points[i]);
        }
    }
    corners.push(points[points.len() - 1]);
    let points = &corners[..];
    let half = TILE_SIZE * 0.14 + widen;
    let head_back = TILE_SIZE * 0.12 + widen;
    let head_tip = TILE_SIZE * 0.38 + widen;
    let head_half = TILE_SIZE * 0.34 + widen;
    let mut shapes = Vec::new();
    let last = points.len() - 1;
    for i in 0..last {
        let (from, mut to) = (points[i], points[i + 1]);
        let along = (to - from).normalize();
        if i + 1 == last {
            to -= along * head_back; // the shaft stops under the arrowhead
        } else {
            to += along * half; // reach past the center so bends have no notch
        }
        let side = Vec2::new(-along.y, along.x) * half;
        let start = if i == 0 { from } else { from - along * half };
        shapes.push(vec![start - side, to - side, to + side, start + side]);
    }
    let dir = (points[last] - points[last - 1]).normalize();
    let side = Vec2::new(-dir.y, dir.x);
    let base = points[last] - dir * head_back;
    shapes.push(vec![base - side * head_half, points[last] + dir * head_tip, base + side * head_half]);
    shapes
}

/// Extrude convex shapes into a solid with a rounded, bevelled top, so light
/// rolls across it like a moulded plastic piece: normals on the top bend
/// outward toward the rim and continue down the bevel into the walls.
fn extruded_mesh(shapes: &[Vec<Vec2>], bottom: f32, height: f32, bevel: f32) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut vertex = |p: Vec3, n: Vec3| {
        positions.push(p.to_array());
        normals.push(n.normalize_or_zero().to_array());
        positions.len() as u32 - 1
    };
    let top = bottom + height;
    for shape in shapes {
        // Wind counter-clockwise seen from above (+Y), so faces point outward
        let area: f32 = shape.iter().zip(shape.iter().cycle().skip(1)).map(|(a, b)| a.x * b.y - b.x * a.y).sum();
        let ring: Vec<Vec2> = if area > 0.0 { shape.iter().rev().copied().collect() } else { shape.clone() };
        let center = ring.iter().copied().sum::<Vec2>() / ring.len() as f32;
        let inset: Vec<Vec2> = ring.iter().map(|&p| {
            let to_center = center - p;
            p + to_center.normalize_or_zero() * bevel.min(to_center.length() * 0.5)
        }).collect();
        let at = |p: Vec2, y: f32| Vec3::new(p.x, y, p.y);
        let out = |p: Vec2| { let d = (p - center).normalize_or_zero(); Vec3::new(d.x, 0.0, d.y) };

        // Top: a fan from the center (facing straight up) to the inset rim (leaning out)
        let hub = vertex(at(center, top), Vec3::Y);
        let rim: Vec<u32> = inset.iter().map(|&p| vertex(at(p, top), Vec3::Y + out(p) * 0.45)).collect();
        for i in 0..ring.len() {
            indices.extend([hub, rim[i], rim[(i + 1) % ring.len()]]);
        }
        for i in 0..ring.len() {
            let j = (i + 1) % ring.len();
            // Bevel: from the leaning rim down to the outer edge (mostly sideways)
            let (a, b) = (vertex(at(inset[i], top), Vec3::Y + out(inset[i]) * 0.45), vertex(at(inset[j], top), Vec3::Y + out(inset[j]) * 0.45));
            let (c, d) = (vertex(at(ring[j], top - bevel), Vec3::Y * 0.5 + out(ring[j])), vertex(at(ring[i], top - bevel), Vec3::Y * 0.5 + out(ring[i])));
            indices.extend([a, d, c, a, c, b]);
            // Wall: straight down, facing out from this edge
            let edge = ring[j] - ring[i];
            let wall_normal = Vec3::new(edge.y, 0.0, -edge.x);
            let (e, f) = (vertex(at(ring[i], top - bevel), wall_normal), vertex(at(ring[j], top - bevel), wall_normal));
            let (g, h) = (vertex(at(ring[j], bottom), wall_normal), vertex(at(ring[i], bottom), wall_normal));
            indices.extend([e, h, g, e, g, f]);
        }
    }
    Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

/// Draw the planned route as a solid, glossy plastic arrow floating over the
/// range slabs, with a soft shadow on the ground beneath it
fn spawn_path_indicator_meshes(
    game_data: Res<GameData>,
    mut commands: Commands,
    movement_path: Res<MovementPath>,
    map: Res<GameMap>,
    existing_path_meshes: Query<Entity, With<PathIndicatorMesh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut arrow_materials: Local<Option<(Handle<StandardMaterial>, Handle<StandardMaterial>)>>,
) {
    if !movement_path.is_changed() {
        return;
    }
    for entity in existing_path_meshes.iter() {
        commands.entity(entity).despawn();
    }
    if movement_path.path.len() < 2 {
        return;
    }

    let points: Vec<Vec2> = movement_path.path.iter().map(|p| {
        let world = GridPosition::new(p.x, p.y).to_world(&map);
        Vec2::new(world.x, world.z)
    }).collect();
    let ground = movement_path.path.iter()
        .filter_map(|p| map.get(p.x, p.y))
        .map(|t| game_data.terrain(t).tile_height)
        .fold(0.0, f32::max);

    let (plastic, shadow) = arrow_materials.get_or_insert_with(|| (
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.34, 0.1),
            perceptual_roughness: 0.22,
            reflectance: 0.6,
            clearcoat: 1.0,
            clearcoat_perceptual_roughness: 0.08,
            ..default()
        }),
        materials.add(StandardMaterial {
            base_color: Color::srgba(0.05, 0.08, 0.2, 0.35),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        }),
    )).clone();

    // The arrow floats just above the range slabs
    let lift = ground + HIGHLIGHT_SLAB_HEIGHT + 1.5;
    commands.spawn((
        Mesh3d(meshes.add(extruded_mesh(&route_arrow_shapes(&points, 0.0), lift, 5.0, 2.0))),
        MeshMaterial3d(plastic),
        PathIndicatorMesh,
    ));
    // Shadow on top of the slabs, nudged toward the camera like a light from above
    let shadow_points: Vec<Vec2> = points.iter().map(|p| *p + Vec2::new(1.5, 2.5)).collect();
    commands.spawn((
        Mesh3d(meshes.add(extruded_mesh(&route_arrow_shapes(&shadow_points, 0.5), ground + HIGHLIGHT_SLAB_HEIGHT + 0.15, 0.05, 0.0))),
        MeshMaterial3d(shadow),
        PathIndicatorMesh,
    ));
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
        let map = map_from_rows(&[&[Grass, Grass, Boulder, Grass]]);
        let tiles: HashSet<_> = (0..4).map(|x| (x, 0)).collect();
        let costs: HashMap<_, _> = tiles.iter().map(|&t| (t, 1)).collect();
        let mut path = MovementPath::default();
        path.start(IVec2::new(0, 0));
        for x in 1..4 {
            assert!(path.try_extend(IVec2::new(x, 0), &map, &tiles, &costs, UnitClass::Foot, &data));
        }
        assert_eq!(path.total_cost, 1 + 2 + 1, "grass, mountain, grass");
    }

    #[test]
    fn units_pass_friends_but_not_enemies_and_join_only_when_wounded() {
        use Terrain::*;
        let data = GameData::load_defaults();
        let map = map_from_rows(&[&[Grass; 5]]);
        let unit = |x, faction, unit_type, full| (Entity::PLACEHOLDER, (x, 0), faction, unit_type, full, false);
        let range = |others: &[(Entity, (i32, i32), Faction, UnitType, bool, bool)]| {
            let mut all = vec![unit(0, Faction::Western, UnitType::Scout, true)];
            all.extend_from_slice(others);
            calculate_movement_range_with_joins(&GridPosition::new(0, 0), 3, &map, UnitType::Scout,
                Faction::Western, &all, UnitClass::Foot, false, &data).0
        };
        // A friendly pikeman: walk past it, but not stop on it
        let past_friend = range(&[unit(1, Faction::Western, UnitType::Shocktrooper, true)]);
        assert!(past_friend.contains(&(2, 0)) && past_friend.contains(&(3, 0)));
        assert!(!past_friend.contains(&(1, 0)));
        // An enemy blocks the way
        let blocked = range(&[unit(1, Faction::Southern, UnitType::Scout, true)]);
        assert!(!blocked.contains(&(2, 0)));
        // Same type: join only if one of the two is wounded
        assert!(!range(&[unit(1, Faction::Western, UnitType::Scout, true)]).contains(&(1, 0)));
        assert!(range(&[unit(1, Faction::Western, UnitType::Scout, false)]).contains(&(1, 0)));
    }

    #[test]
    fn walking_into_a_hidden_enemy_stops_before_it() {
        let route = |xs: &[i32]| xs.iter().map(|&x| IVec2::new(x, 0)).collect::<Vec<_>>();
        let hidden = HashSet::from([(3, 0)]);
        let mut path = route(&[0, 1, 2, 3, 4]);
        assert!(spring_trap(&mut path, &hidden, &HashSet::new()));
        assert_eq!(path, route(&[0, 1, 2]), "stops on the tile before the ambush");
        // Never on a friend's tile: steps back further
        let mut path = route(&[0, 1, 2, 3, 4]);
        assert!(spring_trap(&mut path, &hidden, &HashSet::from([(2, 0)])));
        assert_eq!(path, route(&[0, 1]));
        // No hidden enemy on the way: untouched
        let mut path = route(&[0, 1, 2]);
        assert!(!spring_trap(&mut path, &hidden, &HashSet::new()));
        assert_eq!(path.len(), 3);
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
        let route = find_path(IVec2::new(0, 1), IVec2::new(2, 1), &map, &HashSet::new(), UnitClass::Foot, false, &data).unwrap();
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
        let route = find_path(IVec2::new(0, 0), IVec2::new(2, 0), &map, &occupied, UnitClass::Foot, false, &data).unwrap();
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
        let map = map_from_rows(&[&[Grass, Grass, Boulder, Grass, Grass]]);
        let reach = calculate_movement_range(&GridPosition::new(0, 0), 3, &map, &HashMap::new(), UnitClass::Foot, &data);
        // grass(1) + mountain(2) = 3 movement used reaching x=2
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
        let targets = calculate_attack_targets(&scout, &GridPosition::new(0, 0), &me, &units, &data, &crate::game::CoBonuses::none());
        assert_eq!(targets, HashSet::from([enemy]));
    }
}
