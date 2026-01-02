use bevy::prelude::*;

use super::{GridPosition, Terrain, Faction, Tile, TILE_SIZE};

pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        // Use billboard system instead of Y-sorting in 3D
        app.add_systems(Update, update_billboards);
    }
}

/// Marker component for entities that should always face the camera
#[derive(Component)]
pub struct Billboard;

/// System to make billboard sprites always face the camera directly
/// This creates a true 2D appearance where sprites are always parallel
/// to the camera's view plane, like paper cutouts facing the viewer
fn update_billboards(
    camera_query: Query<&Transform, With<Camera3d>>,
    mut billboards: Query<&mut Transform, (With<Billboard>, Without<Camera3d>)>,
) {
    let Ok(camera) = camera_query.get_single() else { return };

    // Get the camera's rotation - sprites should face opposite to camera's forward
    // This makes them appear flat/2D regardless of camera angle
    let camera_rotation = camera.rotation;

    for mut transform in billboards.iter_mut() {
        // Match camera rotation so sprite faces the camera directly
        transform.rotation = camera_rotation;
    }
}

/// Constants for legacy Y-based depth sorting (kept for reference)
#[allow(dead_code)]
pub const Y_SORT_SCALE: f32 = 0.001;
#[allow(dead_code)]
pub const MAX_MAP_HEIGHT: f32 = 20.0;

/// Sprite layers (legacy - kept for compatibility, not used in 3D mode)
#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(dead_code)]
pub enum SpriteLayer {
    TerrainFeature,
    GroundUnit,
    AirUnit,
}

#[allow(dead_code)]
impl SpriteLayer {
    pub fn base_z(&self) -> f32 {
        match self {
            SpriteLayer::TerrainFeature => 1.0,
            SpriteLayer::GroundUnit => 6.0,
            SpriteLayer::AirUnit => 10.0,
        }
    }
}

/// Legacy marker for Y-based depth sorting (not used in 3D mode)
#[derive(Component)]
#[allow(dead_code)]
pub struct YSortable {
    pub layer: SpriteLayer,
}

/// Component for terrain feature sprites (trees, rocks, buildings)
#[derive(Component)]
pub struct TerrainFeature {
    pub terrain_type: Terrain,
    pub grid_position: IVec2,
}

/// Component for building faction flag
#[derive(Component)]
pub struct FactionFlag;

/// Component for unit shadow
#[derive(Component)]
pub struct UnitShadow;

/// Spawn a terrain feature as 3D mesh for terrain types that have vertical elements
/// Uses 3D coordinates: grid Y -> world Z, feature height -> world Y
pub fn spawn_terrain_feature(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    x: u32,
    y: u32,
    terrain: Terrain,
    owner: Option<Faction>,
    offset_x: f32,
    offset_z: f32,
) {
    let world_x = x as f32 * TILE_SIZE + offset_x;
    let world_z = y as f32 * TILE_SIZE + offset_z;
    let feature_height = terrain.feature_height();

    // Get color from sprite assets or use procedural fallback
    let feature_color = match sprite_assets.get_terrain_feature_sprite(images, terrain) {
        super::SpriteSource::Image(_handle) => get_procedural_feature_color(terrain),
        super::SpriteSource::Procedural { color, .. } => color,
    };

    // Get size for this terrain feature
    let (_, size) = get_procedural_feature_params(terrain);

    // Create vertical quad mesh for the feature
    let feature_mesh = meshes.add(Rectangle::new(size.x, size.y));

    let mut entity_commands = commands.spawn((
        Mesh3d(feature_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: feature_color,
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(world_x, feature_height * 0.5, world_z),
        TerrainFeature {
            terrain_type: terrain,
            grid_position: IVec2::new(x as i32, y as i32),
        },
        GridPosition::new(x as i32, y as i32),
        Billboard,  // Face the camera
    ));

    // Add faction flag for capturable buildings
    if terrain.is_capturable() {
        if let Some(faction) = owner {
            entity_commands.with_children(|parent| {
                spawn_faction_flag_3d(parent, meshes, materials, faction, terrain);
            });
        }
    }
}

/// Get the color for a terrain feature
fn get_procedural_feature_color(terrain: Terrain) -> Color {
    match terrain {
        Terrain::Thicket => Color::srgb(0.15, 0.35, 0.12),
        Terrain::Brambles => Color::srgb(0.25, 0.32, 0.18),
        Terrain::Boulder => Color::srgb(0.45, 0.45, 0.50),
        Terrain::Hollow => Color::srgb(0.35, 0.25, 0.18),
        Terrain::Log => Color::srgb(0.45, 0.32, 0.20),
        Terrain::Base => Color::srgb(0.50, 0.45, 0.40),
        Terrain::Outpost => Color::srgb(0.55, 0.50, 0.45),
        Terrain::Storehouse => Color::srgb(0.48, 0.42, 0.35),
        _ => Color::WHITE,
    }
}

/// Get the color and size for a terrain feature
fn get_procedural_feature_params(terrain: Terrain) -> (Color, Vec2) {
    match terrain {
        Terrain::Thicket => (
            Color::srgb(0.15, 0.35, 0.12),
            Vec2::new(TILE_SIZE * 0.8, 32.0)
        ),
        Terrain::Brambles => (
            Color::srgb(0.25, 0.32, 0.18),
            Vec2::new(TILE_SIZE * 0.9, 24.0)
        ),
        Terrain::Boulder => (
            Color::srgb(0.45, 0.45, 0.50),
            Vec2::new(TILE_SIZE * 0.7, 28.0)
        ),
        Terrain::Hollow => (
            Color::srgb(0.35, 0.25, 0.18),
            Vec2::new(TILE_SIZE * 0.6, 36.0)
        ),
        Terrain::Log => (
            Color::srgb(0.45, 0.32, 0.20),
            Vec2::new(TILE_SIZE * 1.2, 16.0)
        ),
        Terrain::Base => (
            Color::srgb(0.50, 0.45, 0.40),
            Vec2::new(TILE_SIZE * 0.85, 48.0)
        ),
        Terrain::Outpost => (
            Color::srgb(0.55, 0.50, 0.45),
            Vec2::new(TILE_SIZE * 0.75, 40.0)
        ),
        Terrain::Storehouse => (
            Color::srgb(0.48, 0.42, 0.35),
            Vec2::new(TILE_SIZE * 0.65, 32.0)
        ),
        _ => (Color::WHITE, Vec2::ZERO),
    }
}

/// Spawn a faction flag as 3D mesh child of a building
fn spawn_faction_flag_3d(
    parent: &mut ChildBuilder,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    faction: Faction,
    terrain: Terrain,
) {
    let flag_height = match terrain {
        Terrain::Base => 12.0,
        Terrain::Outpost => 10.0,
        Terrain::Storehouse => 8.0,
        _ => 8.0,
    };

    let building_height = terrain.feature_height();

    // Flag pole (thin vertical rectangle)
    let pole_mesh = meshes.add(Rectangle::new(2.0, flag_height + 4.0));
    parent.spawn((
        Mesh3d(pole_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.3, 0.25, 0.2),
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(TILE_SIZE * 0.3, building_height * 0.3, 0.0),
    ));

    // Flag banner
    let flag_mesh = meshes.add(Rectangle::new(8.0, flag_height));
    parent.spawn((
        Mesh3d(flag_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: faction.color(),
            unlit: true,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(TILE_SIZE * 0.3 + 5.0, building_height * 0.3 + 2.0, 0.0),
        FactionFlag,
    ));
}

/// Legacy function kept for compatibility
#[allow(dead_code)]
fn get_feature_sprite_params(terrain: Terrain) -> (Color, Vec2) {
    get_procedural_feature_params(terrain)
}

/// Legacy spawn function - no longer used in 3D mode
#[allow(dead_code)]
fn spawn_building_roof(_parent: &mut ChildBuilder, _terrain: Terrain) {
    // Roofs are now part of the building mesh in 3D mode
}

/// Update faction flag colors when a building is captured
#[allow(dead_code)]
pub fn update_building_flags(
    tiles: Query<(&Tile, &Children), Changed<Tile>>,
    features: Query<(&TerrainFeature, &Children)>,
    mut flags: Query<&mut Sprite, With<FactionFlag>>,
) {
    for (tile, _tile_children) in tiles.iter() {
        // Find the corresponding terrain feature
        for (feature, feature_children) in features.iter() {
            if feature.grid_position == tile.position && feature.terrain_type.is_capturable() {
                // Update flag color
                for child in feature_children.iter() {
                    if let Ok(mut sprite) = flags.get_mut(*child) {
                        if let Some(owner) = tile.owner {
                            sprite.color = owner.color();
                        }
                    }
                }
            }
        }
    }
}
