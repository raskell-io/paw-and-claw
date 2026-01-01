use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMap>()
            .add_systems(Startup, spawn_test_map);
    }
}

/// Terrain types with their properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terrain {
    Clearing,   // Open ground, neutral
    Forest,     // Defense bonus, slows ground units
    Mountain,   // High defense, impassable for some units
    River,      // Movement penalty, needs bridges
    Road,       // Movement bonus
    Ruins,      // Special objectives
    Base,       // Can spawn units
    Village,    // Can be captured for income
}

impl Terrain {
    pub fn defense_bonus(&self) -> i32 {
        match self {
            Terrain::Clearing => 0,
            Terrain::Forest => 2,
            Terrain::Mountain => 4,
            Terrain::River => 0,
            Terrain::Road => 0,
            Terrain::Ruins => 2,
            Terrain::Base => 3,
            Terrain::Village => 1,
        }
    }

    pub fn movement_cost(&self) -> u32 {
        match self {
            Terrain::Clearing => 1,
            Terrain::Forest => 2,
            Terrain::Mountain => 3,
            Terrain::River => 3,
            Terrain::Road => 1,
            Terrain::Ruins => 1,
            Terrain::Base => 1,
            Terrain::Village => 1,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Terrain::Clearing => Color::srgb(0.6, 0.8, 0.4),
            Terrain::Forest => Color::srgb(0.2, 0.5, 0.2),
            Terrain::Mountain => Color::srgb(0.5, 0.5, 0.5),
            Terrain::River => Color::srgb(0.3, 0.5, 0.8),
            Terrain::Road => Color::srgb(0.7, 0.6, 0.4),
            Terrain::Ruins => Color::srgb(0.4, 0.4, 0.35),
            Terrain::Base => Color::srgb(0.8, 0.6, 0.4),
            Terrain::Village => Color::srgb(0.9, 0.7, 0.5),
        }
    }
}

/// A single tile on the map
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: Terrain,
    pub position: IVec2,
    pub owner: Option<u8>,  // Faction ID for capturable tiles
}

/// The game map resource
#[derive(Resource, Default, Serialize, Deserialize)]
pub struct GameMap {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<Vec<Terrain>>,
}

impl GameMap {
    pub fn new(width: u32, height: u32) -> Self {
        let tiles = vec![vec![Terrain::Clearing; width as usize]; height as usize];
        Self { width, height, tiles }
    }

    pub fn get(&self, x: i32, y: i32) -> Option<Terrain> {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            Some(self.tiles[y as usize][x as usize])
        } else {
            None
        }
    }

    pub fn set(&mut self, x: i32, y: i32, terrain: Terrain) {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            self.tiles[y as usize][x as usize] = terrain;
        }
    }
}

pub const TILE_SIZE: f32 = 48.0;

pub fn spawn_test_map(mut commands: Commands, mut game_map: ResMut<GameMap>) {
    // Create a small test map
    *game_map = GameMap::new(12, 8);

    // Add some terrain variety
    game_map.set(2, 2, Terrain::Forest);
    game_map.set(2, 3, Terrain::Forest);
    game_map.set(3, 2, Terrain::Forest);
    game_map.set(5, 3, Terrain::Mountain);
    game_map.set(5, 4, Terrain::Mountain);
    game_map.set(6, 5, Terrain::River);
    game_map.set(7, 5, Terrain::River);
    game_map.set(8, 5, Terrain::River);
    game_map.set(1, 1, Terrain::Base);
    game_map.set(10, 6, Terrain::Base);
    game_map.set(4, 1, Terrain::Village);
    game_map.set(7, 6, Terrain::Village);

    // Calculate offset to center the map
    let offset_x = -(game_map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_y = -(game_map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Spawn tile entities
    for y in 0..game_map.height {
        for x in 0..game_map.width {
            let terrain = game_map.tiles[y as usize][x as usize];
            let world_x = x as f32 * TILE_SIZE + offset_x;
            let world_y = y as f32 * TILE_SIZE + offset_y;

            commands.spawn((
                Sprite {
                    color: terrain.color(),
                    custom_size: Some(Vec2::splat(TILE_SIZE - 2.0)),
                    ..default()
                },
                Transform::from_xyz(world_x, world_y, 0.0),
                Tile {
                    terrain,
                    position: IVec2::new(x as i32, y as i32),
                    owner: None,
                },
            ));
        }
    }
}
