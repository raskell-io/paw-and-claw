use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Faction, GameData, spawn_unit};
use super::maps::MapData;

/// Tileset theme affects the foundation/ground color
// Only Woodland is selectable so far; other themes await a theme picker
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Resource)]
pub enum TilesetTheme {
    #[default]
    Woodland,   // Forest/temperate - brown earth
    Steppe,     // Grassland/plains - tan/yellow soil
    Desert,     // Arid - sandy beige
    Tundra,     // Cold/arctic - gray permafrost
    Swamp,      // Wetland - dark muddy brown
}

impl TilesetTheme {
    pub fn ground_color(&self) -> Color {
        match self {
            TilesetTheme::Woodland => Color::srgb(0.35, 0.25, 0.15),
            TilesetTheme::Steppe => Color::srgb(0.55, 0.45, 0.30),
            TilesetTheme::Desert => Color::srgb(0.70, 0.55, 0.35),
            TilesetTheme::Tundra => Color::srgb(0.45, 0.45, 0.50),
            TilesetTheme::Swamp => Color::srgb(0.25, 0.20, 0.15),
        }
    }
}

/// Depth of the foundation layer beneath tiles
pub const FOUNDATION_DEPTH: f32 = 12.0;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameMap>()
            .init_resource::<TilesetTheme>();
    }
}

/// Spawn map entities from MapData
pub fn spawn_map_from_data(
    commands: &mut Commands,
    game_map: &mut ResMut<GameMap>,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    _sprite_assets: &super::SpriteAssets,
    _images: &Assets<Image>,
    game_data: &GameData,
    map_data: &MapData,
    tileset_theme: TilesetTheme,
) {
    // Update GameMap resource
    game_map.width = map_data.width;
    game_map.height = map_data.height;
    game_map.tiles = map_data.terrain.clone();

    // Offsets to center the map (grid Y -> world Z)
    let offset_x = -(game_map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(game_map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    // Build property ownership lookup
    let property_owners: std::collections::HashMap<(i32, i32), Faction> = map_data
        .properties
        .iter()
        .map(|p| ((p.x, p.y), p.owner))
        .collect();

    // Spawn foundation layer - single large cuboid beneath all tiles
    let map_width = game_map.width as f32 * TILE_SIZE;
    let map_depth = game_map.height as f32 * TILE_SIZE;
    let foundation_mesh = meshes.add(Cuboid::new(map_width, FOUNDATION_DEPTH, map_depth));

    commands.spawn((
        Mesh3d(foundation_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: tileset_theme.ground_color(),
            unlit: true,
            ..default()
        })),
        Transform::from_xyz(0.0, -FOUNDATION_DEPTH / 2.0, 0.0),
        BattleEntity,
    ));

    // Spawn tile entities as 3D cuboid meshes
    for y in 0..game_map.height {
        for x in 0..game_map.width {
            let terrain = game_map.tiles[y as usize][x as usize];
            let world_x = x as f32 * TILE_SIZE + offset_x;
            let world_z = y as f32 * TILE_SIZE + offset_z;  // Grid Y -> World Z

            let owner = property_owners.get(&(x as i32, y as i32)).copied();

            let tile_color = if let Some(faction) = owner {
                blend_color(game_data.terrain_color(terrain), game_data.faction_color(faction), 0.3)
            } else {
                game_data.terrain_color(terrain)
            };

            // Get tile height based on terrain type
            let tile_height = game_data.terrain(terrain).tile_height;

            // Create cuboid mesh for this tile
            let tile_mesh = meshes.add(Cuboid::new(TILE_SIZE - 2.0, tile_height, TILE_SIZE - 2.0));

            commands.spawn((
                Mesh3d(tile_mesh),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: tile_color,
                    unlit: true,
                    ..default()
                })),
                // Position so top of cuboid is at tile_height
                Transform::from_xyz(world_x, tile_height / 2.0, world_z),
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

/// Spawn units from MapData
pub fn spawn_units_from_data(
    commands: &mut Commands,
    game_map: &GameMap,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    game_data: &GameData,
    map_data: &MapData,
) {
    for placement in &map_data.units {
        spawn_unit(
            commands,
            game_map,
            meshes,
            materials,
            sprite_assets,
            images,
            game_data,
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
/// Marker for everything that belongs to the current battle, so leaving a
/// battle can clean up with one query
#[derive(Component, Default)]
pub struct BattleEntity;

#[derive(Component, Debug, Clone, Serialize, Deserialize)]
#[require(BattleEntity)]
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

    pub fn get(&self, x: i32, y: i32) -> Option<Terrain> {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            Some(self.tiles[y as usize][x as usize])
        } else {
            None
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
