use bevy::prelude::*;
use bevy::render::mesh::Mesh3d;
use serde::{Deserialize, Serialize};

use super::{Faction, spawn_unit, spawn_terrain_feature};
use super::maps::{MapData, SelectedMap, get_builtin_map};

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMap>()
            .init_resource::<SelectedMap>();
    }
}

/// Spawn the map based on the selected map ID
pub fn spawn_map_from_selection(
    commands: &mut Commands,
    game_map: &mut ResMut<GameMap>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    selected: &SelectedMap,
) -> MapData {
    let map_data = get_builtin_map(selected.map_id);
    spawn_map_from_data(commands, game_map, meshes, materials, sprite_assets, images, &map_data);
    map_data
}

/// Spawn map entities from MapData
pub fn spawn_map_from_data(
    commands: &mut Commands,
    game_map: &mut ResMut<GameMap>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    map_data: &MapData,
) {
    // Update GameMap resource
    game_map.width = map_data.width;
    game_map.height = map_data.height;
    game_map.tiles = map_data.terrain.clone();

    // Offsets to center the map (grid Y -> world Z)
    let offset_x = -(game_map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(game_map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Create shared tile mesh (flat quad on XZ plane)
    let tile_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat((TILE_SIZE - 2.0) / 2.0)));

    // Build property ownership lookup
    let property_owners: std::collections::HashMap<(i32, i32), Faction> = map_data
        .properties
        .iter()
        .map(|p| ((p.x, p.y), p.owner))
        .collect();

    // Spawn tile entities as 3D meshes on XZ plane
    for y in 0..game_map.height {
        for x in 0..game_map.width {
            let terrain = game_map.tiles[y as usize][x as usize];
            let world_x = x as f32 * TILE_SIZE + offset_x;
            let world_z = y as f32 * TILE_SIZE + offset_z;  // Grid Y -> World Z

            let owner = property_owners.get(&(x as i32, y as i32)).copied();

            let tile_color = if let Some(faction) = owner {
                blend_color(terrain.color(), faction.color(), 0.3)
            } else {
                terrain.color()
            };

            commands.spawn((
                Mesh3d(tile_mesh.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: tile_color,
                    unlit: true,  // Keep flat shading like original
                    ..default()
                })),
                Transform::from_xyz(world_x, 0.0, world_z),
                Tile {
                    terrain,
                    position: IVec2::new(x as i32, y as i32),
                    owner,
                    capture_progress: 0,
                    capturing_faction: None,
                },
            ));

            // Spawn terrain feature sprite for terrains with vertical elements
            if terrain.has_feature() {
                spawn_terrain_feature(commands, sprite_assets, images, x, y, terrain, owner, offset_x, offset_z);
            }
        }
    }
}

/// Spawn units from MapData
pub fn spawn_units_from_data(
    commands: &mut Commands,
    game_map: &GameMap,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    map_data: &MapData,
) {
    for placement in &map_data.units {
        spawn_unit(
            commands,
            game_map,
            sprite_assets,
            images,
            placement.faction,
            placement.unit_type,
            placement.x,
            placement.y,
        );
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

    /// Whether this terrain has a vertical feature sprite (trees, rocks, buildings)
    pub fn has_feature(&self) -> bool {
        matches!(self,
            Terrain::Thicket | Terrain::Brambles | Terrain::Boulder |
            Terrain::Hollow | Terrain::Log | Terrain::Base |
            Terrain::Outpost | Terrain::Storehouse
        )
    }

    /// Height of the feature sprite in pixels (for visual rendering)
    pub fn feature_height(&self) -> f32 {
        match self {
            Terrain::Thicket => 32.0,    // Medium trees
            Terrain::Brambles => 24.0,   // Shorter bushes
            Terrain::Boulder => 28.0,    // Rock formations
            Terrain::Hollow => 36.0,     // Stump/cave entrance
            Terrain::Log => 16.0,        // Low fallen log
            Terrain::Base => 48.0,       // Tall building
            Terrain::Outpost => 40.0,    // Medium building
            Terrain::Storehouse => 32.0, // Small building
            _ => 0.0,
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

    /// Get the asset file name for this terrain type
    pub fn asset_file_name(&self) -> &'static str {
        match self {
            Terrain::Grass => "grass",
            Terrain::TallGrass => "tall_grass",
            Terrain::Thicket => "thicket",
            Terrain::Brambles => "brambles",
            Terrain::Log => "log",
            Terrain::Boulder => "boulder",
            Terrain::Hollow => "hollow",
            Terrain::Creek => "creek",
            Terrain::Pond => "pond",
            Terrain::Shore => "shore",
            Terrain::Base => "base",
            Terrain::Outpost => "outpost",
            Terrain::Storehouse => "storehouse",
        }
    }

    /// Get all terrain variants
    pub fn all() -> &'static [Terrain] {
        &[
            Terrain::Grass,
            Terrain::TallGrass,
            Terrain::Thicket,
            Terrain::Brambles,
            Terrain::Log,
            Terrain::Boulder,
            Terrain::Hollow,
            Terrain::Creek,
            Terrain::Pond,
            Terrain::Shore,
            Terrain::Base,
            Terrain::Outpost,
            Terrain::Storehouse,
        ]
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
