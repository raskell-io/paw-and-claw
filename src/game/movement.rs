use bevy::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

use super::{GameMap, GridPosition, Unit, FactionMember, TurnState, TILE_SIZE};
use crate::states::GameState;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementHighlights>()
            .init_resource::<GridCursor>()
            .add_systems(Update, (
                handle_keyboard_input,
                handle_camera_movement,
                handle_click_input,
                update_movement_highlights,
                draw_grid_cursor,
            ).run_if(in_state(GameState::Battle)));
    }
}

/// Resource to track movement range highlights
#[derive(Resource, Default)]
pub struct MovementHighlights {
    pub tiles: HashSet<(i32, i32)>,
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
    mut units: Query<(Entity, &mut GridPosition, &mut Transform, &FactionMember, &mut Unit)>,
    turn_state: Res<TurnState>,
    map: Res<GameMap>,
) {
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
        cursor.visible = false;
        return;
    }

    // Space or Enter to select/move
    if keyboard.just_pressed(KeyCode::Space) || keyboard.just_pressed(KeyCode::Enter) {
        cursor.visible = true;

        if let Some(selected_entity) = highlights.selected_unit {
            // Try to move to cursor position
            if highlights.tiles.contains(&(cursor.x, cursor.y)) {
                // Check we're not moving onto another unit
                let target_occupied = units.iter()
                    .any(|(e, p, _, _, _)| e != selected_entity && p.x == cursor.x && p.y == cursor.y);

                if !target_occupied {
                    if let Ok((_, mut grid_pos, mut transform, _, mut unit)) = units.get_mut(selected_entity) {
                        grid_pos.x = cursor.x;
                        grid_pos.y = cursor.y;
                        transform.translation = grid_pos.to_world(&map);
                        unit.moved = true;
                        info!("Moved unit to ({}, {})", cursor.x, cursor.y);
                    }
                    highlights.selected_unit = None;
                    highlights.tiles.clear();
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
                    let tiles = calculate_movement_range(&pos, stats.movement, &map, &unit_positions);
                    found_unit = Some((entity, tiles));
                    break;
                }
            }
            if let Some((entity, tiles)) = found_unit {
                highlights.selected_unit = Some(entity);
                highlights.tiles = tiles;
                info!("Selected unit at ({}, {})", cursor.x, cursor.y);
            }
        }
    }
}

/// Handle camera panning with WASD/Arrow keys (when shift not held and cursor not active)
fn handle_camera_movement(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    cursor: Res<GridCursor>,
    mut camera_query: Query<&mut Transform, With<Camera2d>>,
) {
    let shift_held = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // Only pan camera when cursor not visible and shift not held
    if cursor.visible || shift_held {
        return;
    }

    let mut camera_transform = camera_query.single_mut();
    let speed = 300.0 * time.delta_secs();

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        camera_transform.translation.y += speed;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        camera_transform.translation.y -= speed;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        camera_transform.translation.x -= speed;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        camera_transform.translation.x += speed;
    }
}

/// Handle mouse click for selection and movement
fn handle_click_input(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut units: Query<(Entity, &mut GridPosition, &mut Transform, &FactionMember, &mut Unit)>,
    mut highlights: ResMut<MovementHighlights>,
    mut cursor: ResMut<GridCursor>,
    turn_state: Res<TurnState>,
    map: Res<GameMap>,
) {
    if !mouse_button.just_pressed(MouseButton::Left) {
        return;
    }

    let window = windows.single();
    let (camera, camera_transform) = cameras.single();

    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    let Ok(world_position) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    // Convert world position to grid position
    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0;
    let offset_y = -(map.height as f32 * TILE_SIZE) / 2.0;

    let grid_x = ((world_position.x - offset_x) / TILE_SIZE).floor() as i32;
    let grid_y = ((world_position.y - offset_y) / TILE_SIZE).floor() as i32;

    // Update cursor position to click location
    cursor.x = grid_x;
    cursor.y = grid_y;
    cursor.visible = false; // Hide keyboard cursor when using mouse

    // If we have a selected unit, try to move it
    if let Some(selected_entity) = highlights.selected_unit {
        if highlights.tiles.contains(&(grid_x, grid_y)) {
            // Check if clicking on the same unit (deselect) or empty tile (move)
            let clicking_on_selected = units.iter()
                .any(|(e, p, _, _, _)| e == selected_entity && p.x == grid_x && p.y == grid_y);

            if clicking_on_selected {
                // Clicking on selected unit - deselect
                highlights.selected_unit = None;
                highlights.tiles.clear();
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
                    let tiles = calculate_movement_range(&pos, stats.movement, &map, &unit_positions);
                    switch_to = Some((entity, tiles));
                    break;
                }
            }

            if let Some((entity, tiles)) = switch_to {
                highlights.selected_unit = Some(entity);
                highlights.tiles = tiles;
                info!("Switched selection to unit at ({}, {})", grid_x, grid_y);
                return;
            }

            // Move the unit
            if let Ok((_, mut grid_pos, mut transform, _, mut unit)) = units.get_mut(selected_entity) {
                grid_pos.x = grid_x;
                grid_pos.y = grid_y;
                transform.translation = grid_pos.to_world(&map);
                unit.moved = true;
                info!("Moved unit to ({}, {})", grid_x, grid_y);
            }

            highlights.selected_unit = None;
            highlights.tiles.clear();
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
            let tiles = calculate_movement_range(&pos, stats.movement, &map, &unit_positions);
            select_unit = Some((entity, tiles, pos.x, pos.y));
            break;
        }
    }

    if let Some((entity, tiles, x, y)) = select_unit {
        highlights.selected_unit = Some(entity);
        highlights.tiles = tiles;
        info!("Selected unit at ({}, {}), can reach {} tiles", x, y, highlights.tiles.len());
        return;
    }

    // Clicked on empty space or enemy unit - deselect
    highlights.selected_unit = None;
    highlights.tiles.clear();
}

fn update_movement_highlights(
    highlights: Res<MovementHighlights>,
    mut gizmos: Gizmos,
    map: Res<GameMap>,
) {
    if highlights.tiles.is_empty() {
        return;
    }

    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_y = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    for &(x, y) in &highlights.tiles {
        let world_x = x as f32 * TILE_SIZE + offset_x;
        let world_y = y as f32 * TILE_SIZE + offset_y;

        gizmos.rect_2d(
            Isometry2d::from_translation(Vec2::new(world_x, world_y)),
            Vec2::splat(TILE_SIZE - 4.0),
            Color::srgba(0.2, 0.6, 1.0, 0.4),
        );
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
    let offset_y = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    let world_x = cursor.x as f32 * TILE_SIZE + offset_x;
    let world_y = cursor.y as f32 * TILE_SIZE + offset_y;

    // Draw cursor outline
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(world_x, world_y)),
        Vec2::splat(TILE_SIZE - 2.0),
        Color::srgb(1.0, 1.0, 0.0), // Yellow cursor
    );

    // Draw inner highlight
    gizmos.rect_2d(
        Isometry2d::from_translation(Vec2::new(world_x, world_y)),
        Vec2::splat(TILE_SIZE - 6.0),
        Color::srgba(1.0, 1.0, 0.0, 0.3),
    );
}
