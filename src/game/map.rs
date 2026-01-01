use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::Faction;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMap>()
            .add_systems(Startup, spawn_test_map);
    }
}

/// Terrain types scaled for woodland creatures
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terrain {
    // === GROUND TERRAIN ===
    /// Open grass - easy movement, no cover
    Grass,
    /// Tall grass - some concealment, slightly slower
    TallGrass,
    /// Dense shrubs and bushes - good cover, slow movement
    Thicket,
    /// Thorny brambles - excellent defense, very slow
    Brambles,
    /// Fallen log - like a road, fast movement along it
    Log,
    /// Large boulder - high defense, hard to traverse
    Boulder,
    /// Hollow stump or log - provides shelter and cover
    Hollow,

    // === WATER TERRAIN ===
    /// Shallow creek - slows ground units
    Creek,
    /// Deep pond - impassable for ground, naval units only
    Pond,
    /// Muddy shore/bank - slow movement, no defense
    Shore,

    // === SPECIAL TERRAIN ===
    /// Fortified base - spawn point, high defense
    Base,
    /// Small fortified outpost - can be captured for income
    Outpost,
    /// Supply cache - can be captured for resources
    Storehouse,
}

impl Terrain {
    pub fn name(&self) -> &'static str {
        match self {
            Terrain::Grass => "Grass",
            Terrain::TallGrass => "Tall Grass",
            Terrain::Thicket => "Thicket",
            Terrain::Brambles => "Brambles",
            Terrain::Log => "Log",
            Terrain::Boulder => "Boulder",
            Terrain::Hollow => "Hollow",
            Terrain::Creek => "Creek",
            Terrain::Pond => "Pond",
            Terrain::Shore => "Shore",
            Terrain::Base => "Base",
            Terrain::Outpost => "Outpost",
            Terrain::Storehouse => "Storehouse",
        }
    }

    pub fn defense_bonus(&self) -> i32 {
        match self {
            Terrain::Grass => 0,
            Terrain::TallGrass => 1,
            Terrain::Thicket => 2,
            Terrain::Brambles => 3,
            Terrain::Log => 0,
            Terrain::Boulder => 4,
            Terrain::Hollow => 3,
            Terrain::Creek => 0,
            Terrain::Pond => 0,
            Terrain::Shore => 0,
            Terrain::Base => 4,
            Terrain::Outpost => 2,
            Terrain::Storehouse => 1,
        }
    }

    pub fn movement_cost(&self) -> u32 {
        match self {
            Terrain::Grass => 1,
            Terrain::TallGrass => 1,
            Terrain::Thicket => 2,
            Terrain::Brambles => 3,
            Terrain::Log => 1,       // Fast movement on logs
            Terrain::Boulder => 3,
            Terrain::Hollow => 1,
            Terrain::Creek => 2,
            Terrain::Pond => 99,     // Impassable for ground
            Terrain::Shore => 2,
            Terrain::Base => 1,
            Terrain::Outpost => 1,
            Terrain::Storehouse => 1,
        }
    }

    /// Whether this terrain can be captured
    pub fn is_capturable(&self) -> bool {
        matches!(self, Terrain::Base | Terrain::Outpost | Terrain::Storehouse)
    }

    /// Whether this terrain blocks ground movement entirely
    #[allow(dead_code)]
    pub fn blocks_ground(&self) -> bool {
        matches!(self, Terrain::Pond)
    }

    /// Capture points required to take this terrain (0 = not capturable)
    pub fn capture_points(&self) -> i32 {
        match self {
            Terrain::Base => 20,
            Terrain::Outpost => 20,
            Terrain::Storehouse => 20,
            _ => 0,
        }
    }

    /// Income generated per turn when owned
    pub fn income_value(&self) -> u32 {
        match self {
            Terrain::Base => 10,
            Terrain::Outpost => 10,
            Terrain::Storehouse => 5,
            _ => 0,
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Terrain::Grass => Color::srgb(0.55, 0.75, 0.35),      // Light green
            Terrain::TallGrass => Color::srgb(0.45, 0.65, 0.30),  // Medium green
            Terrain::Thicket => Color::srgb(0.25, 0.50, 0.20),    // Dark green
            Terrain::Brambles => Color::srgb(0.35, 0.40, 0.25),   // Olive brown-green
            Terrain::Log => Color::srgb(0.55, 0.40, 0.25),        // Brown
            Terrain::Boulder => Color::srgb(0.50, 0.50, 0.55),    // Gray
            Terrain::Hollow => Color::srgb(0.40, 0.30, 0.20),     // Dark brown
            Terrain::Creek => Color::srgb(0.40, 0.60, 0.80),      // Light blue
            Terrain::Pond => Color::srgb(0.25, 0.45, 0.70),       // Deeper blue
            Terrain::Shore => Color::srgb(0.60, 0.55, 0.40),      // Sandy brown
            Terrain::Base => Color::srgb(0.65, 0.45, 0.30),       // Fortified brown
            Terrain::Outpost => Color::srgb(0.55, 0.50, 0.40),    // Stone gray-brown
            Terrain::Storehouse => Color::srgb(0.50, 0.45, 0.35), // Weathered wood
        }
    }

    #[allow(dead_code)]
    pub fn symbol(&self) -> &'static str {
        match self {
            Terrain::Grass => ".",
            Terrain::TallGrass => "\"",
            Terrain::Thicket => "#",
            Terrain::Brambles => "%",
            Terrain::Log => "=",
            Terrain::Boulder => "O",
            Terrain::Hollow => "U",
            Terrain::Creek => "~",
            Terrain::Pond => "w",
            Terrain::Shore => ",",
            Terrain::Base => "B",
            Terrain::Outpost => "P",
            Terrain::Storehouse => "S",
        }
    }
}

/// A single tile on the map
#[derive(Component, Debug, Clone, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: Terrain,
    pub position: IVec2,
    pub owner: Option<Faction>,  // Faction that owns this tile
    pub capture_progress: i32,   // Current capture progress (0 = not being captured)
    pub capturing_faction: Option<Faction>, // Faction currently capturing
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
        let tiles = vec![vec![Terrain::Grass; width as usize]; height as usize];
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

    // Add some terrain variety - woodland creature scale

    // Thicket (dense bushes) - left side cover
    game_map.set(2, 2, Terrain::Thicket);
    game_map.set(2, 3, Terrain::Thicket);
    game_map.set(3, 2, Terrain::Thicket);
    game_map.set(3, 3, Terrain::TallGrass);

    // Boulders - center obstacles
    game_map.set(5, 3, Terrain::Boulder);
    game_map.set(5, 4, Terrain::Boulder);

    // Creek running through - water obstacle
    game_map.set(6, 5, Terrain::Creek);
    game_map.set(7, 5, Terrain::Creek);
    game_map.set(8, 5, Terrain::Creek);
    game_map.set(5, 5, Terrain::Shore);
    game_map.set(9, 5, Terrain::Shore);

    // Pond - impassable water
    game_map.set(8, 3, Terrain::Pond);
    game_map.set(8, 2, Terrain::Shore);
    game_map.set(9, 3, Terrain::Shore);

    // Fallen log - fast movement path
    game_map.set(3, 5, Terrain::Log);
    game_map.set(4, 5, Terrain::Log);
    game_map.set(5, 6, Terrain::Log);

    // Brambles - defensive position
    game_map.set(9, 4, Terrain::Brambles);
    game_map.set(10, 4, Terrain::Brambles);

    // Hollow stump - cover
    game_map.set(6, 2, Terrain::Hollow);

    // Bases (spawn points)
    game_map.set(1, 1, Terrain::Base);
    game_map.set(10, 6, Terrain::Base);

    // Capturable points
    game_map.set(4, 1, Terrain::Outpost);
    game_map.set(7, 6, Terrain::Outpost);
    game_map.set(6, 0, Terrain::Storehouse);

    // Calculate offset to center the map
    let offset_x = -(game_map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_y = -(game_map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Spawn tile entities
    for y in 0..game_map.height {
        for x in 0..game_map.width {
            let terrain = game_map.tiles[y as usize][x as usize];
            let world_x = x as f32 * TILE_SIZE + offset_x;
            let world_y = y as f32 * TILE_SIZE + offset_y;

            // Set initial owners for bases
            let owner = match (x, y) {
                (1, 1) => Some(Faction::Eastern),   // Eastern base
                (10, 6) => Some(Faction::Northern), // Northern base
                _ => None,
            };

            // Color tile based on owner
            let tile_color = if let Some(faction) = owner {
                blend_color(terrain.color(), faction.color(), 0.3)
            } else {
                terrain.color()
            };

            commands.spawn((
                Sprite {
                    color: tile_color,
                    custom_size: Some(Vec2::splat(TILE_SIZE - 2.0)),
                    ..default()
                },
                Transform::from_xyz(world_x, world_y, 0.0),
                Tile {
                    terrain,
                    position: IVec2::new(x as i32, y as i32),
                    owner,
                    capture_progress: 0,
                    capturing_faction: None,
                },
            ));
        }
    }
}

/// Blend two colors together
fn blend_color(base: Color, tint: Color, amount: f32) -> Color {
    let base_rgba = base.to_srgba();
    let tint_rgba = tint.to_srgba();
    Color::srgba(
        base_rgba.red * (1.0 - amount) + tint_rgba.red * amount,
        base_rgba.green * (1.0 - amount) + tint_rgba.green * amount,
        base_rgba.blue * (1.0 - amount) + tint_rgba.blue * amount,
        1.0,
    )
}
