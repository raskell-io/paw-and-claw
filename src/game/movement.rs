use bevy::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};

use super::{GameMap, GridPosition, Unit, FactionMember, TILE_SIZE};
use crate::states::GameState;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MovementHighlights>()
            .add_systems(Update, (
                handle_unit_selection,
                update_movement_highlights,
                handle_movement_click,
            ).run_if(in_state(GameState::Battle)));
    }
}

/// Resource to track movement range highlights
#[derive(Resource, Default)]
pub struct MovementHighlights {
    pub tiles: HashSet<(i32, i32)>,
    pub selected_unit: Option<Entity>,
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

fn handle_unit_selection(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    units: Query<(Entity, &GridPosition, &FactionMember, &Unit)>,
    mut highlights: ResMut<MovementHighlights>,
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

    // Check if we clicked on a unit
    for (entity, pos, _faction, unit) in units.iter() {
        if pos.x == grid_x && pos.y == grid_y && !unit.moved {
            // Select this unit
            highlights.selected_unit = Some(entity);

            // Build unit positions map
            let unit_positions: HashMap<(i32, i32), Entity> = units
                .iter()
                .map(|(e, p, _, _)| ((p.x, p.y), e))
                .collect();

            // Calculate movement range
            let stats = unit.unit_type.stats();
            highlights.tiles = calculate_movement_range(pos, stats.movement, &map, &unit_positions);

            info!("Selected unit at ({}, {}), can reach {} tiles", grid_x, grid_y, highlights.tiles.len());
            return;
        }
    }

    // Clicked on empty space - deselect
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

fn handle_movement_click(
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    mut units: Query<(&mut GridPosition, &mut Transform, &mut Unit)>,
    mut highlights: ResMut<MovementHighlights>,
    map: Res<GameMap>,
) {
    if !mouse_button.just_pressed(MouseButton::Right) {
        return;
    }

    let Some(selected_entity) = highlights.selected_unit else {
        return;
    };

    let window = windows.single();
    let (camera, camera_transform) = cameras.single();

    let Some(cursor_position) = window.cursor_position() else {
        return;
    };

    let Ok(world_position) = camera.viewport_to_world_2d(camera_transform, cursor_position) else {
        return;
    };

    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0;
    let offset_y = -(map.height as f32 * TILE_SIZE) / 2.0;

    let grid_x = ((world_position.x - offset_x) / TILE_SIZE).floor() as i32;
    let grid_y = ((world_position.y - offset_y) / TILE_SIZE).floor() as i32;

    // Check if target is in movement range
    if !highlights.tiles.contains(&(grid_x, grid_y)) {
        return;
    }

    // Move the unit
    if let Ok((mut grid_pos, mut transform, mut unit)) = units.get_mut(selected_entity) {
        grid_pos.x = grid_x;
        grid_pos.y = grid_y;

        let new_world_pos = grid_pos.to_world(&map);
        transform.translation = new_world_pos;

        unit.moved = true;

        info!("Moved unit to ({}, {})", grid_x, grid_y);
    }

    // Clear selection
    highlights.selected_unit = None;
    highlights.tiles.clear();
}
