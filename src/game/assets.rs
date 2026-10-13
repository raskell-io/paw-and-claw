use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use std::collections::HashMap;

use super::{CommanderId, Faction, UnitType, Terrain, GameData};

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

/// Handles to every optional sprite. Paths follow `assets/sprites/manifest.json`;
/// anything that doesn't exist is simply absent and callers fall back to tokens
/// and flat colors.
#[derive(Resource, Default)]
pub struct SpriteAssets {
    pub unit_sprites: HashMap<(Faction, UnitType), Handle<Image>>,
    /// 64x64 battle-scene sprites (2-frame idle strips)
    pub battle_sprites: HashMap<(Faction, UnitType), Handle<Image>>,
    pub terrain_tiles: HashMap<Terrain, Handle<Image>>,
    pub terrain_features: HashMap<Terrain, Handle<Image>>,
    /// Buildings in an army's colors (`terrain/features/{faction}/{id}.png`);
    /// the plain feature sprite is the neutral one
    pub owned_features: HashMap<(Terrain, Faction), Handle<Image>>,
    pub portraits: HashMap<CommanderId, Handle<Image>>,
    /// Full-body commander art (`figures/{id}.png`, 80x144, transparent)
    pub figures: HashMap<CommanderId, Handle<Image>>,
    /// Every requested sprite has finished loading (or failed)
    pub loading_complete: bool,
}

impl SpriteAssets {
    pub fn unit_sprite_path(game_data: &GameData, faction: Faction, unit_type: UnitType) -> String {
        format!(
            "sprites/units/{}/{}.png",
            game_data.faction(faction).asset_folder,
            game_data.unit(unit_type).asset_name
        )
    }

    pub fn battle_sprite_path(game_data: &GameData, faction: Faction, unit_type: UnitType) -> String {
        Self::unit_sprite_path(game_data, faction, unit_type).replacen("sprites/units/", "sprites/battle/", 1)
    }

    pub fn terrain_tile_path(game_data: &GameData, terrain: Terrain) -> String {
        format!("sprites/terrain/tiles/{}.png", game_data.terrain(terrain).asset_name)
    }

    pub fn terrain_feature_path(game_data: &GameData, terrain: Terrain) -> String {
        format!("sprites/terrain/features/{}.png", game_data.terrain(terrain).asset_name)
    }

    pub fn owned_feature_path(game_data: &GameData, terrain: Terrain, faction: Faction) -> String {
        format!("sprites/terrain/features/{}/{}.png", game_data.faction(faction).asset_folder, game_data.terrain(terrain).asset_name)
    }

    pub fn portrait_path(id: CommanderId) -> String {
        format!("sprites/portraits/{}.png", super::modding::commander_to_key(id))
    }

    pub fn figure_path(id: CommanderId) -> String {
        format!("sprites/figures/{}.png", super::modding::commander_to_key(id))
    }

    /// A handle is usable once its image has actually loaded
    fn ready(images: &Assets<Image>, handle: Option<&Handle<Image>>) -> Option<Handle<Image>> {
        handle.filter(|h| images.contains(*h)).cloned()
    }

    pub fn unit_sprite(&self, images: &Assets<Image>, faction: Faction, unit_type: UnitType) -> Option<Handle<Image>> {
        Self::ready(images, self.unit_sprites.get(&(faction, unit_type)))
    }

    pub fn battle_sprite(&self, images: &Assets<Image>, faction: Faction, unit_type: UnitType) -> Option<Handle<Image>> {
        Self::ready(images, self.battle_sprites.get(&(faction, unit_type)))
    }

    pub fn tile_sprite(&self, images: &Assets<Image>, terrain: Terrain) -> Option<Handle<Image>> {
        Self::ready(images, self.terrain_tiles.get(&terrain))
    }

    pub fn feature_sprite(&self, images: &Assets<Image>, terrain: Terrain) -> Option<Handle<Image>> {
        Self::ready(images, self.terrain_features.get(&terrain))
    }

    /// A building in its owner's colors, or the neutral sprite when unowned
    /// (or when there's no colored version)
    pub fn owned_feature_sprite(&self, images: &Assets<Image>, terrain: Terrain, owner: Option<Faction>) -> Option<Handle<Image>> {
        owner.and_then(|faction| Self::ready(images, self.owned_features.get(&(terrain, faction))))
            .or_else(|| self.feature_sprite(images, terrain))
    }

    /// Whether this building has versions in army colors (then it needs no pennant)
    pub fn has_owned_features(&self, terrain: Terrain) -> bool {
        self.owned_features.keys().any(|(t, _)| *t == terrain)
    }

    pub fn portrait(&self, images: &Assets<Image>, id: CommanderId) -> Option<Handle<Image>> {
        Self::ready(images, self.portraits.get(&id))
    }

    pub fn figure(&self, images: &Assets<Image>, id: CommanderId) -> Option<Handle<Image>> {
        Self::ready(images, self.figures.get(&id))
    }

    fn all_handles(&self) -> impl Iterator<Item = &Handle<Image>> {
        self.unit_sprites.values()
            .chain(self.battle_sprites.values())
            .chain(self.terrain_tiles.values())
            .chain(self.terrain_features.values())
            .chain(self.owned_features.values())
            .chain(self.portraits.values())
            .chain(self.figures.values())
    }
}

/// Whether a sprite file is worth requesting, so missing art (a commander
/// without a figure yet) doesn't fill the log with "path not found" errors.
/// Native builds look at the disk; the web build can't, so it checks the list
/// of sprite files that build.rs made at compile time.
fn sprite_exists(path: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        bevy::asset::io::file::FileAssetReader::get_base_path().join("assets").join(path).exists()
    }
    #[cfg(target_arch = "wasm32")]
    {
        include_str!(concat!(env!("OUT_DIR"), "/sprite_files.txt")).lines().any(|file| file == path)
    }
}

/// Request every sprite that exists at startup
fn start_asset_loading(asset_server: Res<AssetServer>, mut sprites: ResMut<SpriteAssets>, game_data: Res<GameData>) {
    let load = |path: String| sprite_exists(&path).then(|| asset_server.load(path));

    for &faction in Faction::all() {
        for &unit_type in UnitType::all() {
            if let Some(handle) = load(SpriteAssets::unit_sprite_path(&game_data, faction, unit_type)) {
                sprites.unit_sprites.insert((faction, unit_type), handle);
            }
            if let Some(handle) = load(SpriteAssets::battle_sprite_path(&game_data, faction, unit_type)) {
                sprites.battle_sprites.insert((faction, unit_type), handle);
            }
        }
    }
    for &terrain in Terrain::all() {
        if let Some(handle) = load(SpriteAssets::terrain_tile_path(&game_data, terrain)) {
            sprites.terrain_tiles.insert(terrain, handle);
        }
        if game_data.terrain_has_feature(terrain) {
            if let Some(handle) = load(SpriteAssets::terrain_feature_path(&game_data, terrain)) {
                sprites.terrain_features.insert(terrain, handle);
            }
            if game_data.terrain_capturable(terrain) {
                for &faction in Faction::all() {
                    if let Some(handle) = load(SpriteAssets::owned_feature_path(&game_data, terrain, faction)) {
                        sprites.owned_features.insert((terrain, faction), handle);
                    }
                }
            }
        }
    }
    for &id in CommanderId::all() {
        if let Some(handle) = load(SpriteAssets::portrait_path(id)) {
            sprites.portraits.insert(id, handle);
        }
        if let Some(handle) = load(SpriteAssets::figure_path(id)) {
            sprites.figures.insert(id, handle);
        }
    }

    info!(
        "Requested sprites: {} units, {} terrain tiles, {} terrain features, {} portraits",
        sprites.unit_sprites.len(), sprites.terrain_tiles.len(), sprites.terrain_features.len(), sprites.portraits.len()
    );
}

/// Flag when every requested sprite has resolved, so battles can start with
/// all available art in place
fn check_asset_loading_state(asset_server: Res<AssetServer>, mut sprites: ResMut<SpriteAssets>) {
    if sprites.loading_complete {
        return;
    }
    let mut loaded = 0;
    let mut failed = 0;
    for handle in sprites.all_handles() {
        match asset_server.get_load_state(handle) {
            Some(bevy::asset::LoadState::Loaded) => loaded += 1,
            Some(bevy::asset::LoadState::Failed(_)) => failed += 1,
            _ => return,
        }
    }
    sprites.loading_complete = true;
    info!("Sprite loading complete: {loaded} loaded, {failed} failed");
}
