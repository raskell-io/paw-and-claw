use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use std::collections::{HashMap, HashSet};

use super::{Faction, UnitType, Terrain, TILE_SIZE};

/// System parameter that bundles sprite assets and mesh/material stores for 3D rendering
#[derive(SystemParam)]
pub struct SpriteAssetsParam<'w> {
    pub assets: Res<'w, SpriteAssets>,
    pub images: Res<'w, Assets<Image>>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
}

impl<'w> SpriteAssetsParam<'w> {
    /// Get unit sprite or fallback to procedural
    pub fn get_unit_sprite(&self, faction: Faction, unit_type: UnitType) -> SpriteSource {
        self.assets.get_unit_sprite(&self.images, faction, unit_type)
    }

    /// Get terrain feature sprite or fallback to procedural
    pub fn get_terrain_feature_sprite(&self, terrain: Terrain) -> SpriteSource {
        self.assets.get_terrain_feature_sprite(&self.images, terrain)
    }
}

pub struct AssetPlugin;

impl Plugin for AssetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpriteAssets>()
            .add_systems(Startup, start_asset_loading)
            .add_systems(Update, check_asset_loading_state);
    }
}

/// Source for a sprite - either a loaded image or procedural fallback
#[derive(Clone)]
pub enum SpriteSource {
    /// Loaded image asset
    Image(Handle<Image>),
    /// Procedural fallback with color and size
    Procedural { color: Color, size: Vec2 },
}

/// Resource that caches loaded sprite handles
#[derive(Resource, Default)]
pub struct SpriteAssets {
    /// Unit sprites: (Faction, UnitType) -> Handle<Image>
    pub unit_sprites: HashMap<(Faction, UnitType), Handle<Image>>,
    /// Terrain tile sprites: Terrain -> Handle<Image>
    pub terrain_tiles: HashMap<Terrain, Handle<Image>>,
    /// Terrain feature sprites (vertical elements): Terrain -> Handle<Image>
    pub terrain_features: HashMap<Terrain, Handle<Image>>,
    /// Track which assets failed to load (for fallback)
    failed_assets: HashSet<String>,
    /// Whether initial loading is complete
    pub loading_complete: bool,
}

impl SpriteAssets {
    /// Get unit sprite path
    pub fn unit_sprite_path(faction: Faction, unit_type: UnitType) -> String {
        format!(
            "sprites/units/{}/{}.png",
            faction.asset_folder_name(),
            unit_type.asset_file_name()
        )
    }

    /// Get terrain tile path
    pub fn terrain_tile_path(terrain: Terrain) -> String {
        format!("sprites/terrain/tiles/{}.png", terrain.asset_file_name())
    }

    /// Get terrain feature path
    pub fn terrain_feature_path(terrain: Terrain) -> String {
        format!("sprites/terrain/features/{}.png", terrain.asset_file_name())
    }

    /// Get unit sprite or fallback to procedural
    pub fn get_unit_sprite(
        &self,
        images: &Assets<Image>,
        faction: Faction,
        unit_type: UnitType,
    ) -> SpriteSource {
        let key = (faction, unit_type);
        let path = Self::unit_sprite_path(faction, unit_type);

        if let Some(handle) = self.unit_sprites.get(&key) {
            // Check if the asset is actually loaded and not failed
            if images.contains(handle) && !self.failed_assets.contains(&path) {
                return SpriteSource::Image(handle.clone());
            }
        }

        // Fallback to procedural sprite
        SpriteSource::Procedural {
            color: faction.color(),
            size: Vec2::new(TILE_SIZE * 0.7, TILE_SIZE * 0.5),
        }
    }

    /// Get terrain feature sprite or fallback to procedural
    pub fn get_terrain_feature_sprite(
        &self,
        images: &Assets<Image>,
        terrain: Terrain,
    ) -> SpriteSource {
        let path = Self::terrain_feature_path(terrain);

        if let Some(handle) = self.terrain_features.get(&terrain) {
            if images.contains(handle) && !self.failed_assets.contains(&path) {
                return SpriteSource::Image(handle.clone());
            }
        }

        // Fallback to procedural sprite
        let (color, size) = get_procedural_feature_params(terrain);
        SpriteSource::Procedural { color, size }
    }

    /// Get terrain tile sprite or fallback to procedural
    pub fn get_terrain_tile_sprite(
        &self,
        images: &Assets<Image>,
        terrain: Terrain,
    ) -> SpriteSource {
        let path = Self::terrain_tile_path(terrain);

        if let Some(handle) = self.terrain_tiles.get(&terrain) {
            if images.contains(handle) && !self.failed_assets.contains(&path) {
                return SpriteSource::Image(handle.clone());
            }
        }

        // Fallback to procedural sprite
        SpriteSource::Procedural {
            color: terrain.color(),
            size: Vec2::new(TILE_SIZE, TILE_SIZE),
        }
    }

    /// Mark an asset path as failed
    pub fn mark_failed(&mut self, path: String) {
        self.failed_assets.insert(path);
    }
}

/// Get procedural fallback parameters for terrain features
fn get_procedural_feature_params(terrain: Terrain) -> (Color, Vec2) {
    match terrain {
        Terrain::Thicket => (
            Color::srgb(0.15, 0.35, 0.12), // Dark green tree
            Vec2::new(TILE_SIZE * 0.8, 32.0),
        ),
        Terrain::Brambles => (
            Color::srgb(0.25, 0.32, 0.18), // Brown-green bushes
            Vec2::new(TILE_SIZE * 0.9, 24.0),
        ),
        Terrain::Boulder => (
            Color::srgb(0.45, 0.45, 0.50), // Gray rock
            Vec2::new(TILE_SIZE * 0.7, 28.0),
        ),
        Terrain::Hollow => (
            Color::srgb(0.35, 0.25, 0.18), // Dark brown stump
            Vec2::new(TILE_SIZE * 0.6, 36.0),
        ),
        Terrain::Log => (
            Color::srgb(0.45, 0.32, 0.20), // Brown log
            Vec2::new(TILE_SIZE * 1.2, 16.0),
        ),
        Terrain::Base => (
            Color::srgb(0.50, 0.45, 0.40), // Stone building
            Vec2::new(TILE_SIZE * 0.85, 48.0),
        ),
        Terrain::Outpost => (
            Color::srgb(0.55, 0.50, 0.45), // Wooden outpost
            Vec2::new(TILE_SIZE * 0.75, 40.0),
        ),
        Terrain::Storehouse => (
            Color::srgb(0.48, 0.42, 0.35), // Shed
            Vec2::new(TILE_SIZE * 0.65, 32.0),
        ),
        _ => (Color::WHITE, Vec2::ZERO),
    }
}

/// System to start loading all game assets at startup
fn start_asset_loading(
    asset_server: Res<AssetServer>,
    mut sprite_assets: ResMut<SpriteAssets>,
) {
    info!("Starting asset loading...");

    // Load unit sprites for all faction/unit combinations
    for faction in Faction::all() {
        for unit_type in UnitType::all() {
            let path = SpriteAssets::unit_sprite_path(*faction, *unit_type);
            let handle = asset_server.load(&path);
            sprite_assets.unit_sprites.insert((*faction, *unit_type), handle);
        }
    }

    // Load terrain tile sprites
    for terrain in Terrain::all() {
        let path = SpriteAssets::terrain_tile_path(*terrain);
        let handle = asset_server.load(&path);
        sprite_assets.terrain_tiles.insert(*terrain, handle);
    }

    // Load terrain feature sprites (only for terrain with features)
    for terrain in Terrain::all() {
        if terrain.has_feature() {
            let path = SpriteAssets::terrain_feature_path(*terrain);
            let handle = asset_server.load(&path);
            sprite_assets.terrain_features.insert(*terrain, handle);
        }
    }

    info!(
        "Queued {} unit sprites, {} terrain tiles, {} terrain features",
        sprite_assets.unit_sprites.len(),
        sprite_assets.terrain_tiles.len(),
        sprite_assets.terrain_features.len()
    );
}

/// System to check loading state and detect failed assets
fn check_asset_loading_state(
    asset_server: Res<AssetServer>,
    mut sprite_assets: ResMut<SpriteAssets>,
) {
    if sprite_assets.loading_complete {
        return;
    }

    let mut all_done = true;
    let mut loaded_count = 0;
    let mut failed_count = 0;

    // Check unit sprites
    for ((faction, unit_type), handle) in sprite_assets.unit_sprites.iter() {
        match asset_server.get_load_state(handle) {
            Some(bevy::asset::LoadState::Loaded) => {
                loaded_count += 1;
            }
            Some(bevy::asset::LoadState::Failed(_)) => {
                let path = SpriteAssets::unit_sprite_path(*faction, *unit_type);
                if !sprite_assets.failed_assets.contains(&path) {
                    // Will mark as failed after iteration
                    failed_count += 1;
                }
            }
            _ => {
                all_done = false;
            }
        }
    }

    // Check terrain tiles
    for (terrain, handle) in sprite_assets.terrain_tiles.iter() {
        match asset_server.get_load_state(handle) {
            Some(bevy::asset::LoadState::Loaded) => {
                loaded_count += 1;
            }
            Some(bevy::asset::LoadState::Failed(_)) => {
                let path = SpriteAssets::terrain_tile_path(*terrain);
                if !sprite_assets.failed_assets.contains(&path) {
                    failed_count += 1;
                }
            }
            _ => {
                all_done = false;
            }
        }
    }

    // Check terrain features
    for (terrain, handle) in sprite_assets.terrain_features.iter() {
        match asset_server.get_load_state(handle) {
            Some(bevy::asset::LoadState::Loaded) => {
                loaded_count += 1;
            }
            Some(bevy::asset::LoadState::Failed(_)) => {
                let path = SpriteAssets::terrain_feature_path(*terrain);
                if !sprite_assets.failed_assets.contains(&path) {
                    failed_count += 1;
                }
            }
            _ => {
                all_done = false;
            }
        }
    }

    // Mark failed assets (we need to collect paths first to avoid borrow issues)
    let failed_unit_paths: Vec<_> = sprite_assets
        .unit_sprites
        .iter()
        .filter_map(|((faction, unit_type), handle)| {
            if let Some(bevy::asset::LoadState::Failed(_)) = asset_server.get_load_state(handle) {
                let path = SpriteAssets::unit_sprite_path(*faction, *unit_type);
                if !sprite_assets.failed_assets.contains(&path) {
                    return Some(path);
                }
            }
            None
        })
        .collect();

    let failed_tile_paths: Vec<_> = sprite_assets
        .terrain_tiles
        .iter()
        .filter_map(|(terrain, handle)| {
            if let Some(bevy::asset::LoadState::Failed(_)) = asset_server.get_load_state(handle) {
                let path = SpriteAssets::terrain_tile_path(*terrain);
                if !sprite_assets.failed_assets.contains(&path) {
                    return Some(path);
                }
            }
            None
        })
        .collect();

    let failed_feature_paths: Vec<_> = sprite_assets
        .terrain_features
        .iter()
        .filter_map(|(terrain, handle)| {
            if let Some(bevy::asset::LoadState::Failed(_)) = asset_server.get_load_state(handle) {
                let path = SpriteAssets::terrain_feature_path(*terrain);
                if !sprite_assets.failed_assets.contains(&path) {
                    return Some(path);
                }
            }
            None
        })
        .collect();

    for path in failed_unit_paths {
        sprite_assets.mark_failed(path);
    }
    for path in failed_tile_paths {
        sprite_assets.mark_failed(path);
    }
    for path in failed_feature_paths {
        sprite_assets.mark_failed(path);
    }

    if all_done && !sprite_assets.loading_complete {
        sprite_assets.loading_complete = true;
        info!(
            "Asset loading complete: {} loaded, {} failed (using procedural fallback)",
            loaded_count, failed_count
        );
    }
}
