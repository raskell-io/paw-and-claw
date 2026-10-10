use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use std::collections::{HashMap, HashSet};

use super::{Faction, UnitType, Terrain, GameData};

/// System parameter that bundles sprite assets and mesh/material stores for 3D rendering
#[derive(SystemParam)]
pub struct SpriteAssetsParam<'w> {
    pub assets: Res<'w, SpriteAssets>,
    pub images: Res<'w, Assets<Image>>,
    pub meshes: ResMut<'w, Assets<Mesh>>,
    pub materials: ResMut<'w, Assets<StandardMaterial>>,
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
    /// Procedural fallback color
    Procedural { color: Color },
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
    pub fn unit_sprite_path(game_data: &GameData, faction: Faction, unit_type: UnitType) -> String {
        format!(
            "sprites/units/{}/{}.png",
            game_data.faction(faction).asset_folder,
            game_data.unit(unit_type).asset_name
        )
    }

    /// Get terrain tile path
    pub fn terrain_tile_path(game_data: &GameData, terrain: Terrain) -> String {
        format!("sprites/terrain/tiles/{}.png", game_data.terrain(terrain).asset_name)
    }

    /// Get terrain feature path
    pub fn terrain_feature_path(game_data: &GameData, terrain: Terrain) -> String {
        format!("sprites/terrain/features/{}.png", game_data.terrain(terrain).asset_name)
    }

    /// Get unit sprite or fallback to procedural
    pub fn get_unit_sprite(
        &self,
        images: &Assets<Image>,
        game_data: &GameData,
        faction: Faction,
        unit_type: UnitType,
    ) -> SpriteSource {
        let key = (faction, unit_type);
        let path = Self::unit_sprite_path(game_data, faction, unit_type);

        if let Some(handle) = self.unit_sprites.get(&key) {
            // Check if the asset is actually loaded and not failed
            if images.contains(handle) && !self.failed_assets.contains(&path) {
                return SpriteSource::Image(handle.clone());
            }
        }

        // Fallback to procedural sprite
        SpriteSource::Procedural {
            color: game_data.faction_color(faction),
        }
    }


    /// Get terrain feature sprite or fallback to procedural
    pub fn get_terrain_feature_sprite(
        &self,
        images: &Assets<Image>,
        game_data: &GameData,
        terrain: Terrain,
    ) -> SpriteSource {
        let path = Self::terrain_feature_path(game_data, terrain);

        if let Some(handle) = self.terrain_features.get(&terrain) {
            if images.contains(handle) && !self.failed_assets.contains(&path) {
                return SpriteSource::Image(handle.clone());
            }
        }

        // Fallback to procedural sprite
        let (color, _) = super::sprites::get_procedural_feature_params(terrain);
        SpriteSource::Procedural { color }
    }


    /// Mark an asset path as failed
    pub fn mark_failed(&mut self, path: String) {
        self.failed_assets.insert(path);
    }
}

/// System to start loading all game assets at startup
fn start_asset_loading(
    asset_server: Res<AssetServer>,
    mut sprite_assets: ResMut<SpriteAssets>,
    game_data: Res<GameData>,
) {
    info!("Starting asset loading...");

    // Load unit sprites for all faction/unit combinations
    for faction in Faction::all() {
        for unit_type in UnitType::all() {
            let path = SpriteAssets::unit_sprite_path(&game_data, *faction, *unit_type);
            let handle = asset_server.load(&path);
            sprite_assets.unit_sprites.insert((*faction, *unit_type), handle);
        }
    }

    // Load terrain tile sprites
    for terrain in Terrain::all() {
        let path = SpriteAssets::terrain_tile_path(&game_data, *terrain);
        let handle = asset_server.load(&path);
        sprite_assets.terrain_tiles.insert(*terrain, handle);
    }

    // Load terrain feature sprites (only for terrain with features)
    for terrain in Terrain::all() {
        if game_data.terrain_has_feature(*terrain) {
            let path = SpriteAssets::terrain_feature_path(&game_data, *terrain);
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
    game_data: Res<GameData>,
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
                let path = SpriteAssets::unit_sprite_path(&game_data, *faction, *unit_type);
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
                let path = SpriteAssets::terrain_tile_path(&game_data, *terrain);
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
                let path = SpriteAssets::terrain_feature_path(&game_data, *terrain);
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
                let path = SpriteAssets::unit_sprite_path(&game_data, *faction, *unit_type);
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
                let path = SpriteAssets::terrain_tile_path(&game_data, *terrain);
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
                let path = SpriteAssets::terrain_feature_path(&game_data, *terrain);
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
