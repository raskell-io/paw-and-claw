use bevy::prelude::*;

use super::{GridPosition, Terrain, Faction, Tile, TILE_SIZE};

pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_y_depth_sorting);
    }
}

/// Constants for Y-based depth sorting
pub const Y_SORT_SCALE: f32 = 0.001;
pub const MAX_MAP_HEIGHT: f32 = 20.0;

/// Sprite layers for proper draw ordering
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SpriteLayer {
    TerrainFeature,  // z = 1.0-5.0
    GroundUnit,      // z = 6.0-8.0
    AirUnit,         // z = 10.0-12.0
}

impl SpriteLayer {
    pub fn base_z(&self) -> f32 {
        match self {
            SpriteLayer::TerrainFeature => 1.0,
            SpriteLayer::GroundUnit => 6.0,
            SpriteLayer::AirUnit => 10.0,
        }
    }
}

/// Marker for entities that need Y-based depth sorting
#[derive(Component)]
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

/// System to update Z position based on Y coordinate for depth sorting
/// Lower Y values (bottom of screen) render in front (higher Z)
fn update_y_depth_sorting(
    mut query: Query<(&mut Transform, &GridPosition, &YSortable)>,
) {
    for (mut transform, pos, sortable) in query.iter_mut() {
        let base_z = sortable.layer.base_z();
        // Higher grid Y = further from camera = lower render priority
        // So we subtract Y from MAX to invert it
        let y_offset = (MAX_MAP_HEIGHT - pos.y as f32) * Y_SORT_SCALE;
        transform.translation.z = base_z + y_offset;
    }
}

/// Spawn a terrain feature sprite for terrain types that have vertical elements
pub fn spawn_terrain_feature(
    commands: &mut Commands,
    x: u32,
    y: u32,
    terrain: Terrain,
    owner: Option<Faction>,
    offset_x: f32,
    offset_y: f32,
) {
    let world_x = x as f32 * TILE_SIZE + offset_x;
    let world_y = y as f32 * TILE_SIZE + offset_y;

    let (sprite_color, sprite_size) = get_feature_sprite_params(terrain);

    let mut entity_commands = commands.spawn((
        Sprite {
            color: sprite_color,
            custom_size: Some(sprite_size),
            anchor: bevy::sprite::Anchor::BottomCenter,
            ..default()
        },
        Transform::from_xyz(world_x, world_y - TILE_SIZE * 0.4, 1.0), // Z will be set by Y-sort
        TerrainFeature {
            terrain_type: terrain,
            grid_position: IVec2::new(x as i32, y as i32),
        },
        GridPosition::new(x as i32, y as i32),
        YSortable { layer: SpriteLayer::TerrainFeature },
    ));

    // Add faction flag for capturable buildings
    if terrain.is_capturable() {
        if let Some(faction) = owner {
            entity_commands.with_children(|parent| {
                spawn_faction_flag(parent, faction, terrain);
            });
        }
    }

    // Add building details (roof) for structures
    if matches!(terrain, Terrain::Base | Terrain::Outpost | Terrain::Storehouse) {
        entity_commands.with_children(|parent| {
            spawn_building_roof(parent, terrain);
        });
    }
}

/// Get the color and size for a terrain feature sprite
fn get_feature_sprite_params(terrain: Terrain) -> (Color, Vec2) {
    match terrain {
        Terrain::Thicket => (
            Color::srgb(0.15, 0.35, 0.12), // Dark green tree
            Vec2::new(TILE_SIZE * 0.8, 32.0)
        ),
        Terrain::Brambles => (
            Color::srgb(0.25, 0.32, 0.18), // Brown-green bushes
            Vec2::new(TILE_SIZE * 0.9, 24.0)
        ),
        Terrain::Boulder => (
            Color::srgb(0.45, 0.45, 0.50), // Gray rock
            Vec2::new(TILE_SIZE * 0.7, 28.0)
        ),
        Terrain::Hollow => (
            Color::srgb(0.35, 0.25, 0.18), // Dark brown stump
            Vec2::new(TILE_SIZE * 0.6, 36.0)
        ),
        Terrain::Log => (
            Color::srgb(0.45, 0.32, 0.20), // Brown log
            Vec2::new(TILE_SIZE * 1.2, 16.0) // Wider than tile
        ),
        Terrain::Base => (
            Color::srgb(0.50, 0.45, 0.40), // Stone building
            Vec2::new(TILE_SIZE * 0.85, 48.0)
        ),
        Terrain::Outpost => (
            Color::srgb(0.55, 0.50, 0.45), // Wooden outpost
            Vec2::new(TILE_SIZE * 0.75, 40.0)
        ),
        Terrain::Storehouse => (
            Color::srgb(0.48, 0.42, 0.35), // Shed
            Vec2::new(TILE_SIZE * 0.65, 32.0)
        ),
        _ => (Color::WHITE, Vec2::ZERO),
    }
}

/// Spawn a faction flag as a child of a building
fn spawn_faction_flag(parent: &mut ChildBuilder, faction: Faction, terrain: Terrain) {
    let flag_height = match terrain {
        Terrain::Base => 12.0,
        Terrain::Outpost => 10.0,
        Terrain::Storehouse => 8.0,
        _ => 8.0,
    };

    let building_height = terrain.feature_height();

    // Flag pole
    parent.spawn((
        Sprite {
            color: Color::srgb(0.3, 0.25, 0.2), // Brown pole
            custom_size: Some(Vec2::new(2.0, flag_height + 4.0)),
            ..default()
        },
        Transform::from_xyz(
            TILE_SIZE * 0.3,
            building_height * 0.6,
            0.05,
        ),
    ));

    // Flag banner
    parent.spawn((
        Sprite {
            color: faction.color(),
            custom_size: Some(Vec2::new(8.0, flag_height)),
            ..default()
        },
        Transform::from_xyz(
            TILE_SIZE * 0.3 + 5.0,
            building_height * 0.6 + 2.0,
            0.1,
        ),
        FactionFlag,
    ));
}

/// Spawn a roof detail for buildings
fn spawn_building_roof(parent: &mut ChildBuilder, terrain: Terrain) {
    let roof_color = match terrain {
        Terrain::Base => Color::srgb(0.55, 0.35, 0.25), // Red-brown roof
        Terrain::Outpost => Color::srgb(0.50, 0.45, 0.35), // Tan roof
        Terrain::Storehouse => Color::srgb(0.45, 0.40, 0.30), // Brown roof
        _ => return,
    };

    let (_, sprite_size) = get_feature_sprite_params(terrain);
    let height = terrain.feature_height();

    parent.spawn((
        Sprite {
            color: roof_color,
            custom_size: Some(Vec2::new(sprite_size.x + 4.0, height * 0.25)),
            ..default()
        },
        Transform::from_xyz(0.0, height * 0.85, 0.02),
    ));
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
