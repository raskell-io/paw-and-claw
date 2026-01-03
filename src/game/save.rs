use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{
    Faction, FactionMember, GameMap, GridPosition, Terrain, Tile, TurnState, Unit, UnitType,
    FactionFunds, Commanders, CommanderId, Weather, WeatherType, TurnPhase, VictoryType,
    GameResult,
};

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<SaveGameEvent>()
            .add_event::<LoadGameEvent>()
            .add_event::<GameLoadedEvent>()
            .add_systems(Update, (handle_save_game, handle_load_game, save_load_keyboard));
    }
}

/// Event to trigger a game save
#[derive(Message)]
pub struct SaveGameEvent {
    pub slot: u32,
}

/// Event to trigger a game load
#[derive(Message)]
pub struct LoadGameEvent {
    pub slot: u32,
}

/// Event fired when game has been loaded (for systems to react)
#[derive(Message)]
pub struct GameLoadedEvent;

/// Complete serializable game state
#[derive(Serialize, Deserialize)]
pub struct SaveGameData {
    pub version: u32,
    pub map: SavedMap,
    pub tiles: Vec<SavedTile>,
    pub units: Vec<SavedUnit>,
    pub turn_state: SavedTurnState,
    pub funds: HashMap<Faction, u32>,
    pub commanders: SavedCommanders,
    pub weather: SavedWeather,
}

impl SaveGameData {
    pub const CURRENT_VERSION: u32 = 1;
}

#[derive(Serialize, Deserialize)]
pub struct SavedMap {
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<Vec<Terrain>>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedTile {
    pub x: i32,
    pub y: i32,
    pub terrain: Terrain,
    pub owner: Option<Faction>,
    pub capture_progress: i32,
    pub capturing_faction: Option<Faction>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedUnit {
    pub unit_type: UnitType,
    pub faction: Faction,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub stamina: u32,
    pub ammo: u32,
    pub moved: bool,
    pub attacked: bool,
    pub cargo: Option<SavedCargoUnit>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedCargoUnit {
    pub unit_type: UnitType,
    pub hp: i32,
    pub stamina: u32,
    pub ammo: u32,
}

#[derive(Serialize, Deserialize)]
pub struct SavedTurnState {
    pub current_faction: Faction,
    pub turn_number: u32,
    pub phase: SavedTurnPhase,
}

#[derive(Serialize, Deserialize)]
pub enum SavedTurnPhase {
    Select,
    Move,
    Action,
    Animating,
}

impl From<TurnPhase> for SavedTurnPhase {
    fn from(phase: TurnPhase) -> Self {
        match phase {
            TurnPhase::Select => SavedTurnPhase::Select,
            TurnPhase::Move => SavedTurnPhase::Move,
            TurnPhase::Action => SavedTurnPhase::Action,
            TurnPhase::Animating => SavedTurnPhase::Animating,
        }
    }
}

impl From<SavedTurnPhase> for TurnPhase {
    fn from(phase: SavedTurnPhase) -> Self {
        match phase {
            SavedTurnPhase::Select => TurnPhase::Select,
            SavedTurnPhase::Move => TurnPhase::Move,
            SavedTurnPhase::Action => TurnPhase::Action,
            SavedTurnPhase::Animating => TurnPhase::Animating,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct SavedCommanders {
    pub active: HashMap<Faction, CommanderId>,
    pub power_meter: HashMap<Faction, u32>,
    pub power_active: HashMap<Faction, bool>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedWeather {
    pub current: WeatherType,
    pub turns_remaining: u32,
    pub dynamic_weather: bool,
    pub change_chance: u32,
}

// ============================================================================
// PLATFORM-SPECIFIC STORAGE
// ============================================================================

/// Get the storage key for a save slot
fn get_save_key(slot: u32) -> String {
    format!("paw_and_claw_save_{}", slot)
}

/// Save data to storage (localStorage for WASM, filesystem for native)
#[cfg(target_arch = "wasm32")]
fn save_to_storage(slot: u32, json: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("No window object")?;
    let storage = window
        .local_storage()
        .map_err(|_| "Failed to access localStorage")?
        .ok_or("localStorage not available")?;

    let key = get_save_key(slot);
    storage
        .set_item(&key, json)
        .map_err(|_| "Failed to write to localStorage")?;

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn save_to_storage(slot: u32, json: &str) -> Result<(), String> {
    use std::fs;
    use std::path::PathBuf;

    let mut path = PathBuf::from("saves");
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push(format!("save_{}.json", slot));
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Load data from storage (localStorage for WASM, filesystem for native)
#[cfg(target_arch = "wasm32")]
fn load_from_storage(slot: u32) -> Result<String, String> {
    let window = web_sys::window().ok_or("No window object")?;
    let storage = window
        .local_storage()
        .map_err(|_| "Failed to access localStorage")?
        .ok_or("localStorage not available")?;

    let key = get_save_key(slot);
    storage
        .get_item(&key)
        .map_err(|_| "Failed to read from localStorage")?
        .ok_or_else(|| format!("No save found in slot {}", slot))
}

#[cfg(not(target_arch = "wasm32"))]
fn load_from_storage(slot: u32) -> Result<String, String> {
    use std::fs;
    use std::path::PathBuf;

    let mut path = PathBuf::from("saves");
    path.push(format!("save_{}.json", slot));
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

/// Get storage location description for logging
#[cfg(target_arch = "wasm32")]
fn storage_location(slot: u32) -> String {
    format!("localStorage[{}]", get_save_key(slot))
}

#[cfg(not(target_arch = "wasm32"))]
fn storage_location(slot: u32) -> String {
    format!("saves/save_{}.json", slot)
}

// ============================================================================
// SAVE/LOAD SYSTEMS
// ============================================================================

/// Handle save game event
fn handle_save_game(
    mut events: EventReader<SaveGameEvent>,
    game_map: Res<GameMap>,
    tiles: Query<&Tile>,
    units: Query<(&Unit, &GridPosition, &FactionMember)>,
    turn_state: Res<TurnState>,
    funds: Res<FactionFunds>,
    commanders: Res<Commanders>,
    weather: Res<Weather>,
) {
    for event in events.read() {
        // Build save data
        let save_data = SaveGameData {
            version: SaveGameData::CURRENT_VERSION,
            map: SavedMap {
                width: game_map.width,
                height: game_map.height,
                tiles: game_map.tiles.clone(),
            },
            tiles: tiles.iter().map(|t| SavedTile {
                x: t.position.x,
                y: t.position.y,
                terrain: t.terrain,
                owner: t.owner,
                capture_progress: t.capture_progress,
                capturing_faction: t.capturing_faction,
            }).collect(),
            units: units.iter().map(|(u, pos, fac)| SavedUnit {
                unit_type: u.unit_type,
                faction: fac.faction,
                x: pos.x,
                y: pos.y,
                hp: u.hp,
                stamina: u.stamina,
                ammo: u.ammo,
                moved: u.moved,
                attacked: u.attacked,
                cargo: u.cargo.as_ref().map(|c| SavedCargoUnit {
                    unit_type: c.unit_type,
                    hp: c.hp,
                    stamina: c.stamina,
                    ammo: c.ammo,
                }),
            }).collect(),
            turn_state: SavedTurnState {
                current_faction: turn_state.current_faction,
                turn_number: turn_state.turn_number,
                phase: turn_state.phase.into(),
            },
            funds: [
                (Faction::Eastern, funds.get(Faction::Eastern)),
                (Faction::Northern, funds.get(Faction::Northern)),
                (Faction::Western, funds.get(Faction::Western)),
                (Faction::Southern, funds.get(Faction::Southern)),
                (Faction::Wanderer, funds.get(Faction::Wanderer)),
            ].into_iter().collect(),
            commanders: SavedCommanders {
                active: commanders.active.clone(),
                power_meter: commanders.power_meter.clone(),
                power_active: commanders.power_active.clone(),
            },
            weather: SavedWeather {
                current: weather.current,
                turns_remaining: weather.turns_remaining,
                dynamic_weather: weather.dynamic_weather,
                change_chance: weather.change_chance,
            },
        };

        // Serialize and save
        match serde_json::to_string_pretty(&save_data) {
            Ok(json) => {
                match save_to_storage(event.slot, &json) {
                    Ok(_) => info!("Game saved to slot {} ({})", event.slot, storage_location(event.slot)),
                    Err(e) => error!("Failed to save game: {}", e),
                }
            }
            Err(e) => error!("Failed to serialize save data: {}", e),
        }
    }
}

/// Handle load game event
fn handle_load_game(
    mut commands: Commands,
    mut events: EventReader<LoadGameEvent>,
    mut loaded_events: MessageWriter<GameLoadedEvent>,
    mut game_map: ResMut<GameMap>,
    mut turn_state: ResMut<TurnState>,
    mut funds: ResMut<FactionFunds>,
    mut commanders: ResMut<Commanders>,
    mut weather: ResMut<Weather>,
    mut game_result: ResMut<GameResult>,
    tiles: Query<Entity, With<Tile>>,
    units: Query<Entity, With<Unit>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sprite_assets: Res<super::SpriteAssets>,
    images: Res<Assets<Image>>,
) {
    for event in events.read() {
        // Load from storage
        let json = match load_from_storage(event.slot) {
            Ok(j) => j,
            Err(e) => {
                error!("Failed to load save: {}", e);
                continue;
            }
        };

        // Parse save data
        let save_data: SaveGameData = match serde_json::from_str(&json) {
            Ok(d) => d,
            Err(e) => {
                error!("Failed to parse save file: {}", e);
                continue;
            }
        };

        // Despawn existing entities
        for entity in tiles.iter() {
            commands.entity(entity).despawn();
        }
        for entity in units.iter() {
            commands.entity(entity).despawn();
        }

        // Restore GameMap
        game_map.width = save_data.map.width;
        game_map.height = save_data.map.height;
        game_map.tiles = save_data.map.tiles;

        // Restore TurnState
        turn_state.current_faction = save_data.turn_state.current_faction;
        turn_state.turn_number = save_data.turn_state.turn_number;
        turn_state.phase = save_data.turn_state.phase.into();

        // Restore funds
        for (faction, amount) in save_data.funds {
            // Reset to 0 first, then add
            let current = funds.get(faction);
            if current > 0 {
                funds.spend(faction, current);
            }
            funds.add(faction, amount);
        }

        // Restore commanders
        commanders.active = save_data.commanders.active;
        commanders.power_meter = save_data.commanders.power_meter;
        commanders.power_active = save_data.commanders.power_active;

        // Restore weather
        weather.set(save_data.weather.current);
        weather.turns_remaining = save_data.weather.turns_remaining;
        weather.dynamic_weather = save_data.weather.dynamic_weather;
        weather.change_chance = save_data.weather.change_chance;

        // Reset game result (in case we're loading a saved game that was still in progress)
        game_result.game_over = false;
        game_result.winner = None;
        game_result.victory_type = VictoryType::None;

        // Spawn tiles
        let offset_x = -(game_map.width as f32 * super::TILE_SIZE) / 2.0 + super::TILE_SIZE / 2.0;
        let offset_z = -(game_map.height as f32 * super::TILE_SIZE) / 2.0 + super::TILE_SIZE / 2.0;
        let tile_mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat((super::TILE_SIZE - 2.0) / 2.0)));

        for saved_tile in &save_data.tiles {
            let world_x = saved_tile.x as f32 * super::TILE_SIZE + offset_x;
            let world_z = saved_tile.y as f32 * super::TILE_SIZE + offset_z;

            let tile_color = if let Some(faction) = saved_tile.owner {
                blend_color(saved_tile.terrain.color(), faction.color(), 0.3)
            } else {
                saved_tile.terrain.color()
            };

            commands.spawn((
                Mesh3d(tile_mesh.clone()),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: tile_color,
                    unlit: true,
                    ..default()
                })),
                Transform::from_xyz(world_x, 0.0, world_z),
                Tile {
                    terrain: saved_tile.terrain,
                    position: IVec2::new(saved_tile.x, saved_tile.y),
                    owner: saved_tile.owner,
                    capture_progress: saved_tile.capture_progress,
                    capturing_faction: saved_tile.capturing_faction,
                },
            ));

            // Spawn terrain features
            if saved_tile.terrain.has_feature() {
                super::spawn_terrain_feature(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &sprite_assets,
                    &images,
                    saved_tile.x as u32,
                    saved_tile.y as u32,
                    saved_tile.terrain,
                    saved_tile.owner,
                    offset_x,
                    offset_z,
                );
            }
        }

        // Spawn units
        for saved_unit in &save_data.units {
            let mut unit = Unit::new(saved_unit.unit_type);
            unit.hp = saved_unit.hp;
            unit.stamina = saved_unit.stamina;
            unit.ammo = saved_unit.ammo;
            unit.moved = saved_unit.moved;
            unit.attacked = saved_unit.attacked;
            unit.cargo = saved_unit.cargo.as_ref().map(|c| super::CargoUnit {
                unit_type: c.unit_type,
                hp: c.hp,
                stamina: c.stamina,
                ammo: c.ammo,
            });

            super::spawn_unit_with_state(
                &mut commands,
                &game_map,
                &mut meshes,
                &mut materials,
                &sprite_assets,
                &images,
                saved_unit.faction,
                unit,
                saved_unit.x,
                saved_unit.y,
            );
        }

        info!("Game loaded from slot {} ({})", event.slot, storage_location(event.slot));
        loaded_events.write(GameLoadedEvent);
    }
}

/// Keyboard shortcuts for save/load
fn save_load_keyboard(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut save_events: MessageWriter<SaveGameEvent>,
    mut load_events: MessageWriter<LoadGameEvent>,
) {
    // F5 = Quick Save
    if keyboard.just_pressed(KeyCode::F5) {
        save_events.write(SaveGameEvent { slot: 0 });
    }
    // F9 = Quick Load
    if keyboard.just_pressed(KeyCode::F9) {
        load_events.write(LoadGameEvent { slot: 0 });
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
