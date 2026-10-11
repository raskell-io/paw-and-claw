use bevy::prelude::*;

use super::{GridPosition, Terrain, Faction, GameData, TILE_SIZE};

pub struct SpritePlugin;

impl Plugin for SpritePlugin {
    fn build(&self, app: &mut App) {
        // Use billboard system instead of Y-sorting in 3D
        app.add_systems(Update, (update_billboards, sync_property_flags));
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
    let Ok(camera) = camera_query.single() else { return };

    // Get the camera's rotation - sprites should face opposite to camera's forward
    // This makes them appear flat/2D regardless of camera angle
    let camera_rotation = camera.rotation;

    for mut transform in billboards.iter_mut() {
        // Match camera rotation so sprite faces the camera directly
        transform.rotation = camera_rotation;
    }
}

/// Component for terrain feature sprites (trees, rocks, buildings)
#[derive(Component)]
#[require(super::BattleEntity)]
pub struct TerrainFeature {
    pub grid_position: IVec2,
}

/// Component for building faction flag
#[derive(Component)]
pub struct FactionFlag;

/// Component for unit shadow
#[derive(Component)]
pub struct UnitShadow;

/// Spawn a terrain feature sprite (tree, rock, building) standing on its tile.
/// Grid Y maps to world Z; `ground_height` is the top of the tile block.
pub fn spawn_terrain_feature(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    game_data: &GameData,
    texture: Handle<Image>,
    x: u32,
    y: u32,
    ground_height: f32,
    terrain: Terrain,
    owner: Option<Faction>,
    pennant: bool,
    offset_x: f32,
    offset_z: f32,
) {
    let size = super::SPRITE_WORLD_SIZE;
    let world_x = x as f32 * TILE_SIZE + offset_x;
    // Toward the back of the tile, so a unit standing here (sprite or the
    // shorter fallback card) is nearer the camera and draws in front
    let world_z = y as f32 * TILE_SIZE + offset_z - TILE_SIZE * 0.3;

    let mut feature = commands.spawn((
        Mesh3d(meshes.add(Rectangle::new(size, size))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(texture),
            unlit: true,
            alpha_mode: AlphaMode::Mask(0.5),
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(world_x, ground_height + size / 2.0, world_z),
        TerrainFeature {
            grid_position: IVec2::new(x as i32, y as i32),
        },
        GridPosition::new(x as i32, y as i32),
        Billboard,
    ));

    // Properties without art in army colors fly a pennant showing their owner
    // (kept in sync by `sync_property_flags`)
    if pennant && game_data.terrain_capturable(terrain) {
        feature.with_children(|parent| {
            parent.spawn((
                Mesh3d(meshes.add(Rectangle::new(1.5, size * 0.35))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.3, 0.22, 0.15),
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })),
                Transform::from_xyz(size * 0.38, size * 0.38, 0.5),
            ));
            parent.spawn((
                Mesh3d(meshes.add(Rectangle::new(9.0, 6.0))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: flag_color(game_data, owner),
                    unlit: true,
                    double_sided: true,
                    cull_mode: None,
                    ..default()
                })),
                Transform::from_xyz(size * 0.38 + 5.0, size * 0.5, 0.6),
                FactionFlag,
            ));
        });
    }
}

fn flag_color(game_data: &GameData, owner: Option<Faction>) -> Color {
    owner.map_or(Color::srgb(0.88, 0.84, 0.74), |f| game_data.faction_color(f))
}

/// When a property changes hands, repaint it in the new owner's colors (or
/// recolor its pennant when it has no colored art)
fn sync_property_flags(
    tiles: Query<&super::Tile, Changed<super::Tile>>,
    features: Query<(&TerrainFeature, &MeshMaterial3d<StandardMaterial>, Option<&Children>)>,
    flags: Query<&MeshMaterial3d<StandardMaterial>, (With<FactionFlag>, Without<TerrainFeature>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game_data: Res<GameData>,
    sprites: Res<super::SpriteAssets>,
    images: Res<Assets<Image>>,
) {
    for tile in tiles.iter() {
        let Some((_, feature_material, children)) = features.iter().find(|(f, _, _)| f.grid_position == tile.position) else { continue };
        if let Some(texture) = sprites.owned_feature_sprite(&images, tile.terrain, tile.owner) {
            if let Some(material) = materials.get_mut(&feature_material.0) {
                if material.base_color_texture.as_ref() != Some(&texture) {
                    material.base_color_texture = Some(texture);
                }
            }
        }
        for child in children.into_iter().flat_map(|c| c.iter()) {
            if let Some(material) = flags.get(child).ok().and_then(|m| materials.get_mut(&m.0)) {
                material.base_color = flag_color(&game_data, tile.owner);
            }
        }
    }
}

