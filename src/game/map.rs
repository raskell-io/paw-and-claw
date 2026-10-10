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
    sprite_assets: &super::SpriteAssets,
    images: &mut Assets<Image>,
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

    // Water tiles get shore edges toward land; each variant is built once
    let mut shore_cache: std::collections::HashMap<(Terrain, u8), Option<Handle<Image>>> = Default::default();
    let tiles = game_map.tiles.clone();
    let terrain_at = |x: i32, y: i32| -> Option<Terrain> {
        (x >= 0 && y >= 0).then(|| tiles.get(y as usize).and_then(|row| row.get(x as usize)).copied()).flatten()
    };

    // Spawn tile entities as 3D cuboid meshes
    for y in 0..game_map.height {
        for x in 0..game_map.width {
            let terrain = game_map.tiles[y as usize][x as usize];
            let world_x = x as f32 * TILE_SIZE + offset_x;
            let world_z = y as f32 * TILE_SIZE + offset_z;  // Grid Y -> World Z

            let owner = property_owners.get(&(x as i32, y as i32)).copied();

            let mut texture = sprite_assets.tile_sprite(images, terrain);
            if let Some(base) = texture.clone().filter(|_| is_water(terrain)) {
                let mask = shore_mask(x as i32, y as i32, &terrain_at);
                if mask != 0 {
                    texture = shore_cache
                        .entry((terrain, mask))
                        .or_insert_with(|| shoreline_texture(images, &base, mask))
                        .clone()
                        .or(texture);
                }
            }
            let tile_color = tile_tint(game_data, terrain, owner, texture.is_some());

            // Strip textures hold variants: the trail and bank atlases are indexed
            // by which neighbors they join, other ground picks a variant by position
            let trail = trail_mask(x as i32, y as i32, &terrain_at);
            let columns = texture.as_ref().and_then(|t| images.get(t)).map_or(1, |image| {
                (image.width() / image.height().max(1)).max(1)
            });
            let bridge = bridge_span(x as i32, y as i32, &terrain_at);
            let column = match terrain {
                Terrain::Log if bridge.is_some() => BRIDGE_COLUMN + bridge.unwrap_or(0),
                Terrain::Log => trail as u32,
                Terrain::Shore => water_sides(x as i32, y as i32, &terrain_at) as u32,
                _ => tile_variant(x, y),
            } % columns;
            let uv_transform = bevy::math::Affine2::from_scale_angle_translation(
                Vec2::new(1.0 / columns as f32, 1.0), 0.0, Vec2::new(column as f32 / columns as f32, 0.0));

            // Get tile height based on terrain type
            let tile_height = game_data.terrain(terrain).tile_height;

            // Tiles butt together with no grid gaps, as in Advance Wars
            let tile_mesh = meshes.add(tile_cuboid(TILE_SIZE, tile_height));

            commands.spawn((
                Mesh3d(tile_mesh),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: tile_color,
                    base_color_texture: texture,
                    uv_transform,
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

            // Trees, rocks and buildings stand on their tile when there's a sprite for them.
            // A log that's part of a trail is just the trail; only a lone one lies there.
            let lone = terrain != Terrain::Log || (trail == 0 && bridge.is_none());
            if let Some(feature) = sprite_assets.feature_sprite(images, terrain).filter(|_| lone) {
                super::spawn_terrain_feature(commands, meshes, materials, game_data, feature,
                    x, y, tile_height, terrain, owner, offset_x, offset_z);
            }
        }
    }
}

/// Which sides a trail (log) tile continues toward: other trail tiles and bases
/// (as AW roads lead to HQs but pass cities by). Bits match the trail atlas
/// columns: 1 N, 2 E, 4 S, 8 W.
fn trail_mask(x: i32, y: i32, terrain_at: &impl Fn(i32, i32) -> Option<Terrain>) -> u8 {
    if terrain_at(x, y) != Some(Terrain::Log) {
        return 0;
    }
    [(0, -1, 1), (1, 0, 2), (0, 1, 4), (-1, 0, 8)].into_iter()
        .filter(|&(dx, dy, _)| matches!(terrain_at(x + dx, y + dy),
            Some(Terrain::Log | Terrain::Base)))
        .fold(0, |mask, (_, _, bit)| mask | bit)
}

/// Which sides of a tile face water, as bank atlas columns: 1 N, 2 E, 4 S, 8 W
fn water_sides(x: i32, y: i32, terrain_at: &impl Fn(i32, i32) -> Option<Terrain>) -> u8 {
    [(0, -1, 1), (1, 0, 2), (0, 1, 4), (-1, 0, 8)].into_iter()
        .filter(|&(dx, dy, _)| terrain_at(x + dx, y + dy).is_some_and(is_water))
        .fold(0, |mask, (_, _, bit)| mask | bit)
}

/// First plank-bridge column in the trail atlas (east-west, then north-south)
const BRIDGE_COLUMN: u32 = 16;

/// A trail tile with water on two opposite sides is a bridge, as AW draws a road
/// over sea or river. Returns 0 when it runs east-west, 1 when north-south.
fn bridge_span(x: i32, y: i32, terrain_at: &impl Fn(i32, i32) -> Option<Terrain>) -> Option<u32> {
    if terrain_at(x, y) != Some(Terrain::Log) {
        return None;
    }
    let water = |dx: i32, dy: i32| terrain_at(x + dx, y + dy).is_some_and(is_water);
    if water(0, -1) && water(0, 1) {
        Some(0)
    } else if water(-1, 0) && water(1, 0) {
        Some(1)
    } else {
        None
    }
}

/// A stable pseudo-random variant index for a tile, so neighbors rarely match
fn tile_variant(x: u32, y: u32) -> u32 {
    let h = x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663);
    (h ^ (h >> 13)).wrapping_mul(0x5bd1_e995) >> 16
}

fn is_water(terrain: Terrain) -> bool {
    matches!(terrain, Terrain::Pond | Terrain::Creek | Terrain::LilyPads)
}

// Neighbor bits for shoreline autotiling: sides, then corners
const SHORE_N: u8 = 1;
const SHORE_E: u8 = 2;
const SHORE_S: u8 = 4;
const SHORE_W: u8 = 8;
const SHORE_NE: u8 = 16;
const SHORE_SE: u8 = 32;
const SHORE_SW: u8 = 64;
const SHORE_NW: u8 = 128;

/// Which sides and corners of a water tile border land. Off-map counts as
/// water, and a corner only counts when both sides next to it are water
/// (otherwise the side edges already cover it). Grid -y is north.
fn shore_mask(x: i32, y: i32, terrain_at: &impl Fn(i32, i32) -> Option<Terrain>) -> u8 {
    // Bridges span the water, so the shore runs under them
    let land = |dx: i32, dy: i32| {
        terrain_at(x + dx, y + dy).is_some_and(|t| !is_water(t)) && bridge_span(x + dx, y + dy, terrain_at).is_none()
    };
    let mut mask = 0;
    for (dx, dy, bit) in [(0, -1, SHORE_N), (1, 0, SHORE_E), (0, 1, SHORE_S), (-1, 0, SHORE_W)] {
        if land(dx, dy) {
            mask |= bit;
        }
    }
    for (dx, dy, bit, sides) in [
        (1, -1, SHORE_NE, SHORE_N | SHORE_E),
        (1, 1, SHORE_SE, SHORE_S | SHORE_E),
        (-1, 1, SHORE_SW, SHORE_S | SHORE_W),
        (-1, -1, SHORE_NW, SHORE_N | SHORE_W),
    ] {
        if mask & sides == 0 && land(dx, dy) {
            mask |= bit;
        }
    }
    mask
}

/// Copy a water texture and paint an Advance Wars style shore along the masked
/// edges: a strip of sand meeting the water with a light foam line
fn shoreline_texture(images: &mut Assets<Image>, base: &Handle<Image>, mask: u8) -> Option<Handle<Image>> {
    let mut image = images.get(base)?.clone();
    let (w, h) = (image.width() as i32, image.height() as i32);
    let data = image.data.as_mut()?;
    if data.len() != (w * h * 4) as usize {
        return None; // not RGBA8
    }
    const SAND: [u8; 4] = [236, 214, 140, 255];
    const SAND_SHADE: [u8; 4] = [214, 190, 116, 255];
    const FOAM: [u8; 4] = [236, 246, 255, 255];
    let band = (w / 16).max(1); // sand width scales with the tile (2px at 32px)

    // Distance (in pixels) from each pixel to the nearest masked edge/corner
    let distance = |x: i32, y: i32| -> i32 {
        let mut d = i32::MAX;
        if mask & SHORE_N != 0 { d = d.min(y); }
        if mask & SHORE_S != 0 { d = d.min(h - 1 - y); }
        if mask & SHORE_W != 0 { d = d.min(x); }
        if mask & SHORE_E != 0 { d = d.min(w - 1 - x); }
        if mask & SHORE_NW != 0 { d = d.min(x.max(y)); }
        if mask & SHORE_NE != 0 { d = d.min((w - 1 - x).max(y)); }
        if mask & SHORE_SW != 0 { d = d.min(x.max(h - 1 - y)); }
        if mask & SHORE_SE != 0 { d = d.min((w - 1 - x).max(h - 1 - y)); }
        d
    };
    for y in 0..h {
        for x in 0..w {
            let d = distance(x, y);
            let color = if d < band - 1 || (d == band - 1 && (x + y) % 3 != 0) {
                Some(if d == 0 { SAND } else { SAND_SHADE })
            } else if d < band {
                Some(SAND)
            } else if d == band {
                Some(FOAM)
            } else {
                None
            };
            if let Some(color) = color {
                let i = ((y * w + x) * 4) as usize;
                data[i..i + 4].copy_from_slice(&color);
            }
        }
    }
    Some(images.add(image))
}

/// Material color for a tile. Textured tiles are drawn as-is: their property's
/// pennant shows the owner. Flat ones use the terrain color, with owned
/// properties tinted toward the owner's faction color.
pub fn tile_tint(game_data: &GameData, terrain: Terrain, owner: Option<Faction>, textured: bool) -> Color {
    if textured {
        return Color::WHITE;
    }
    let base = game_data.terrain_color(terrain);
    match owner {
        Some(faction) => blend_color(base, game_data.faction_color(faction), 0.3),
        None => base,
    }
}

/// A tile block whose top shows the whole texture and whose sides continue it as
/// a thin strip, so a ground texture reads as solid earth rather than stretching
fn tile_cuboid(width: f32, height: f32) -> Mesh {
    let mut mesh = Mesh::from(Cuboid::new(width, height, width));
    let positions: Vec<[f32; 3]> = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(p)) => p.clone(),
        _ => return mesh,
    };
    let normals: Vec<[f32; 3]> = match mesh.attribute(Mesh::ATTRIBUTE_NORMAL) {
        Some(bevy::mesh::VertexAttributeValues::Float32x3(n)) => n.clone(),
        _ => return mesh,
    };
    let half = width / 2.0;
    let uvs: Vec<[f32; 2]> = positions.iter().zip(&normals).map(|(p, n)| {
        if n[1].abs() > 0.5 {
            [(p[0] + half) / width, (p[2] + half) / width]
        } else {
            let along = if n[0].abs() > 0.5 { p[2] } else { p[0] };
            [(along + half) / width, (height / 2.0 - p[1]) / width]
        }
    }).collect();
    // Sides are shaded darker and warmer, like the earthen banks of AW terrain
    let colors: Vec<[f32; 4]> = normals.iter().map(|n| {
        if n[1] > 0.5 { [1.0, 1.0, 1.0, 1.0] } else { [0.62, 0.52, 0.46, 1.0] }
    }).collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
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
    /// Lily pads on deep water - naval units only, with a little cover (AW reef)
    LilyPads,
    /// Muddy shore/bank - slow movement, no defense
    Shore,

    // === SPECIAL TERRAIN ===
    /// Fortified base - the army's headquarters, spawn point, high defense
    Base,
    /// Workshop - builds units and earns income, but losing it doesn't lose the battle (AW factory)
    Workshop,
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
            Terrain::LilyPads,
            Terrain::Shore,
            Terrain::Base,
            Terrain::Workshop,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn mask_for(rows: &[&str], x: i32, y: i32) -> u8 {
        let grid: Vec<Vec<Terrain>> = rows.iter()
            .map(|r| r.chars().map(|c| if c == '~' { Terrain::Pond } else { Terrain::Grass }).collect())
            .collect();
        let at = |x: i32, y: i32| (x >= 0 && y >= 0).then(|| grid.get(y as usize).and_then(|r| r.get(x as usize)).copied()).flatten();
        shore_mask(x, y, &at)
    }

    #[test]
    fn trails_join_neighbors_and_properties() {
        let rows = ["=.B", "===", ".=O"];
        let at = |x: i32, y: i32| -> Option<Terrain> {
            let c = rows.get(y as usize)?.chars().nth(x as usize)?;
            Some(match c { '=' => Terrain::Log, 'B' => Terrain::Base, 'O' => Terrain::Outpost, _ => Terrain::Grass })
        };
        assert_eq!(trail_mask(1, 1, &at), 2 | 4 | 8);
        assert_eq!(trail_mask(2, 1, &at), 1 | 8, "leads to the base, not the outpost");
        assert_eq!(trail_mask(0, 0, &at), 4);
        assert_eq!(trail_mask(1, 0, &at), 0, "grass is never a trail");
    }

    #[test]
    fn trail_over_water_is_a_bridge() {
        let rows = ["~=~", ".=.", "~~~", "=.="];
        let at = |x: i32, y: i32| -> Option<Terrain> {
            let c = rows.get(y as usize)?.chars().nth(x as usize)?;
            Some(match c { '=' => Terrain::Log, '~' => Terrain::Pond, _ => Terrain::Grass })
        };
        assert_eq!(bridge_span(1, 0, &at), Some(1), "water east and west: runs north-south");
        assert_eq!(bridge_span(1, 1, &at), None, "on land it's a trail");
        assert_eq!(bridge_span(0, 3, &at), None, "water on one side only");
        assert_eq!(shore_mask(0, 0, &at) & SHORE_E, 0, "no beach drawn toward a bridge");
    }

    #[test]
    fn open_water_has_no_shore() {
        assert_eq!(mask_for(&["~~~", "~~~", "~~~"], 1, 1), 0);
    }

    #[test]
    fn map_edge_counts_as_water() {
        assert_eq!(mask_for(&["~~", "~~"], 0, 0), 0);
    }

    #[test]
    fn sides_facing_land_get_shore() {
        // Land to the north and west of the center
        assert_eq!(mask_for(&[".. ", ".~~", ".~~"].map(|r| r.trim_end()), 1, 1) & 0x0F, SHORE_N | SHORE_W);
    }

    #[test]
    fn corners_only_when_both_sides_are_water() {
        // Land only diagonally (north-east)
        assert_eq!(mask_for(&["~~.", "~~~", "~~~"], 1, 1), SHORE_NE);
        // Land north and north-east: the side covers the corner
        assert_eq!(mask_for(&["~..", "~~~", "~~~"], 1, 1), SHORE_N);
    }
}
