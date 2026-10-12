//! Saving and loading battles.
//!
//! A save is a full snapshot of a battle at a quiet moment on a human
//! player's turn: the map, every property and unit, both armies' funds,
//! commanders and power meters, the day and the weather. Loading rebuilds the
//! board through the same spawn code as a new battle, then puts the state
//! back, so a loaded battle looks and plays exactly like the one saved.
//!
//! There are three manual slots and an autosave, written at the start of each
//! of the player's turns and after each of their units acts. Saves live in
//! `saves/` on desktop and in the browser's localStorage on the web (the
//! installed home-screen app included). Browsers keep a home-screen app's
//! storage apart from the browser tab's, so on the web every save can be
//! exported to one backup file and imported again elsewhere.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{
    reset_ai, spawn_map_from_data, spawn_unit_with_state, AiState, BattleConfig, BattleEntity, CargoUnit,
    CommanderId, Commanders, Faction, FactionFunds, FactionMember, FogOfWar, GameData, GameMap, GameResult,
    GridCursor, GridPosition, MapData, MapId, MapMenu, MovementHighlights, MovementPath, PendingAction,
    PowerLevel, ProductionState, PropertyOwnership, SpriteAssets, Terrain, Tile, TilesetTheme, TurnBanner,
    TurnPhase, TurnState, Unit, UnitAnimation, UnitType, Weather, WeatherType,
};
use crate::states::GameState;

pub struct SavePlugin;

impl Plugin for SavePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SaveGameEvent>()
            .add_message::<LoadGameEvent>()
            .add_message::<GameLoadedEvent>()
            .init_resource::<BattleMapId>()
            .init_resource::<PendingLoad>()
            .init_resource::<SaveNotice>()
            .init_resource::<ImportPending>()
            .insert_resource(SaveIndex::read())
            .add_systems(Update, (
                handle_load_game,
                apply_pending_load.run_if(in_state(GameState::Battle)),
                restore_captures,
                (autosave, handle_save_game).chain().run_if(in_state(GameState::Battle)),
                poll_import,
                tick_notice,
            ));
    }
}

// ============================================================================
// SLOTS
// ============================================================================

/// Where a save is kept
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SaveSlot {
    /// Written by the game itself, every turn and action
    Auto,
    /// One of the player's slots, 1 to `MANUAL_SLOTS`
    Manual(u8),
}

pub const MANUAL_SLOTS: u8 = 3;

impl SaveSlot {
    pub fn all() -> impl Iterator<Item = SaveSlot> {
        std::iter::once(SaveSlot::Auto).chain((1..=MANUAL_SLOTS).map(SaveSlot::Manual))
    }

    pub fn name(self) -> String {
        match self {
            SaveSlot::Auto => "Autosave".to_string(),
            SaveSlot::Manual(n) => format!("Slot {n}"),
        }
    }

    /// Storage name: `saves/<key>.json` on desktop, `paw_and_claw_<key>` in localStorage
    fn key(self) -> String {
        match self {
            SaveSlot::Auto => "save_auto".to_string(),
            SaveSlot::Manual(n) => format!("save_{n}"),
        }
    }

    fn from_key(key: &str) -> Option<SaveSlot> {
        SaveSlot::all().find(|slot| slot.key() == key)
    }
}

// ============================================================================
// MESSAGES AND RESOURCES
// ============================================================================

/// Save the battle into a slot (ignored unless the battle can be saved, see `BattleSnapshot::can_save`)
#[derive(Message)]
pub struct SaveGameEvent {
    pub slot: SaveSlot,
}

/// Load a slot: from the title screen this starts the battle
#[derive(Message)]
pub struct LoadGameEvent {
    pub slot: SaveSlot,
}

/// A saved battle is on the board again
#[derive(Message)]
pub struct GameLoadedEvent {
    pub map_id: Option<MapId>,
    pub player_faction: Faction,
    pub player_co: CommanderId,
}

/// The built-in map the battle is played on (saved so a load can name it)
#[derive(Resource, Default, Clone, Copy)]
pub struct BattleMapId(pub Option<MapId>);

/// A save read from storage, waiting for the battle state and sprites
#[derive(Resource, Default)]
pub struct PendingLoad(pub Option<SaveGame>);

/// A short message for the player ("Saved to Slot 1"), shown by the UI
#[derive(Resource, Default)]
pub struct SaveNotice {
    pub text: String,
    pub error: bool,
    /// Seconds since it appeared
    pub age: f32,
}

impl SaveNotice {
    pub const SECS: f32 = 2.6;

    fn show(&mut self, text: impl Into<String>, error: bool) {
        *self = SaveNotice { text: text.into(), error, age: 0.0 };
    }

    pub fn visible(&self) -> bool {
        !self.text.is_empty() && self.age < Self::SECS
    }
}

fn tick_notice(time: Res<Time>, mut notice: ResMut<SaveNotice>) {
    if notice.visible() {
        notice.age += time.delta_secs();
    }
}

/// What each slot holds, kept current so menus don't read storage every frame
#[derive(Resource, Default)]
pub struct SaveIndex {
    pub slots: Vec<(SaveSlot, Option<SaveSummary>)>,
}

impl SaveIndex {
    fn read() -> Self {
        let slots = SaveSlot::all().map(|slot| (slot, read_summary(slot))).collect();
        SaveIndex { slots }
    }

    pub fn refresh(&mut self) {
        *self = Self::read();
    }

    pub fn get(&self, slot: SaveSlot) -> Option<&SaveSummary> {
        self.slots.iter().find(|(s, _)| *s == slot).and_then(|(_, summary)| summary.as_ref())
    }

    /// The most recently written save
    pub fn newest(&self) -> Option<(SaveSlot, &SaveSummary)> {
        self.slots.iter()
            .filter_map(|(slot, summary)| summary.as_ref().map(|s| (*slot, s)))
            .max_by(|a, b| a.1.saved_at.total_cmp(&b.1.saved_at))
    }
}

// ============================================================================
// SAVE FORMAT
// ============================================================================

/// A whole battle
#[derive(Serialize, Deserialize, Clone)]
pub struct SaveGame {
    pub version: u32,
    pub summary: SaveSummary,
    pub map_id: Option<MapId>,
    /// Terrain rows, `[y][x]`
    pub terrain: Vec<Vec<Terrain>>,
    /// Properties that are owned or half captured
    pub tiles: Vec<SavedTile>,
    pub units: Vec<SavedUnit>,
    pub player_faction: Faction,
    pub ai_faction: Faction,
    pub current_faction: Faction,
    pub funds: HashMap<Faction, u32>,
    pub commanders: SavedCommanders,
    pub weather: SavedWeather,
    pub fog: bool,
    /// Armies that have fielded units (see `GameResult::fielded`)
    pub fielded: Vec<Faction>,
}

impl SaveGame {
    /// Saves from before this (the flat-board format) can't be loaded
    pub const VERSION: u32 = 2;
}

/// What a save menu shows about a slot
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SaveSummary {
    /// Unix time, seconds
    pub saved_at: f64,
    pub map: String,
    pub day: u32,
    /// Player first, then the opponent
    pub armies: Vec<(Faction, CommanderId)>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedTile {
    pub x: i32,
    pub y: i32,
    pub owner: Option<Faction>,
    #[serde(default)]
    pub capture_progress: i32,
    #[serde(default)]
    pub capturing_faction: Option<Faction>,
}

#[derive(Serialize, Deserialize, Clone)]
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
    pub exhausted: bool,
    #[serde(default)]
    pub paralyzed: bool,
    #[serde(default)]
    pub cargo: Vec<SavedCargoUnit>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedCargoUnit {
    pub unit_type: UnitType,
    pub hp: i32,
    pub stamina: u32,
    pub ammo: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedCommanders {
    pub active: HashMap<Faction, CommanderId>,
    pub power_meter: HashMap<Faction, u32>,
    pub power_active: HashMap<Faction, PowerLevel>,
    pub power_uses: HashMap<Faction, u32>,
    pub power_attack: HashMap<Faction, i32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedWeather {
    pub current: WeatherType,
    pub turns_remaining: u32,
    pub dynamic_weather: bool,
    pub change_chance: u32,
}

/// Just enough of a save to list it
#[derive(Deserialize)]
struct SaveHeader {
    version: u32,
    summary: SaveSummary,
}

fn read_summary(slot: SaveSlot) -> Option<SaveSummary> {
    let header: SaveHeader = serde_json::from_str(&read_setting(&slot.key())?).ok()?;
    (header.version == SaveGame::VERSION).then_some(header.summary)
}

fn parse_save(json: &str) -> Result<SaveGame, String> {
    let header: SaveHeader = serde_json::from_str(json).map_err(|_| "That save is damaged".to_string())?;
    if header.version != SaveGame::VERSION {
        return Err("That save is from an older version of the game".to_string());
    }
    serde_json::from_str(json).map_err(|_| "That save is damaged".to_string())
}

// ============================================================================
// STORAGE (localStorage on the web, `saves/` on desktop)
// ============================================================================

/// Store a small document (localStorage for WASM, `saves/<name>.json` for native)
pub fn write_setting(name: &str, json: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = web_sys::window().ok_or("No window object")?
            .local_storage().map_err(|_| "Failed to access localStorage")?
            .ok_or("localStorage not available")?;
        storage.set_item(&format!("paw_and_claw_{name}"), json)
            .map_err(|_| "The browser refused to store it (storage full or disabled)".to_string())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::create_dir_all("saves").map_err(|e| e.to_string())?;
        std::fs::write(format!("saves/{name}.json"), json).map_err(|e| e.to_string())
    }
}

/// Read a document stored by `write_setting`, if there is one
pub fn read_setting(name: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()?.local_storage().ok()??.get_item(&format!("paw_and_claw_{name}")).ok()?
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::fs::read_to_string(format!("saves/{name}.json")).ok()
    }
}

/// Remove a document stored by `write_setting`
pub fn remove_setting(name: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.remove_item(&format!("paw_and_claw_{name}"));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = std::fs::remove_file(format!("saves/{name}.json"));
    }
}

/// Seconds since the Unix epoch
pub fn now_secs() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() / 1000.0
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
    }
}

/// "just now", "5 min ago", "3 h ago", "2 days ago"
pub fn time_ago(saved_at: f64) -> String {
    let secs = (now_secs() - saved_at).max(0.0);
    match secs {
        s if s < 60.0 => "just now".to_string(),
        s if s < 3600.0 => format!("{} min ago", (s / 60.0) as u32),
        s if s < 86400.0 => format!("{} h ago", (s / 3600.0) as u32),
        s if s < 2.0 * 86400.0 => "yesterday".to_string(),
        s => format!("{} days ago", (s / 86400.0) as u32),
    }
}

// ============================================================================
// SAVING
// ============================================================================

/// Everything a save reads from the battle
#[derive(SystemParam)]
pub struct BattleSnapshot<'w, 's> {
    map: Res<'w, GameMap>,
    map_id: Res<'w, BattleMapId>,
    tiles: Query<'w, 's, &'static Tile>,
    units: Query<'w, 's, (&'static Unit, &'static GridPosition, &'static FactionMember)>,
    animating: Query<'w, 's, (), With<UnitAnimation>>,
    turn_state: Res<'w, TurnState>,
    funds: Res<'w, FactionFunds>,
    commanders: Res<'w, Commanders>,
    weather: Res<'w, Weather>,
    config: Res<'w, BattleConfig>,
    fog: Res<'w, FogOfWar>,
    result: Res<'w, GameResult>,
    ai: Res<'w, AiState>,
    banner: Res<'w, TurnBanner>,
    pending: Res<'w, PendingAction>,
}

impl BattleSnapshot<'_, '_> {
    /// Whether it's a human player's turn
    pub fn humans_turn(&self) -> bool {
        !(self.ai.enabled && self.config.is_ai_faction(self.turn_state.current_faction))
    }

    /// A battle saves only at a quiet moment of a human's turn: no unit
    /// halfway through a move, no animation, no banner, not over
    pub fn can_save(&self) -> bool {
        self.map.width > 0
            && self.humans_turn()
            && !self.result.game_over
            && self.turn_state.phase == TurnPhase::Select
            && self.pending.unit.is_none()
            && self.animating.is_empty()
            && !self.banner.playing()
    }

    fn snapshot(&self, game_data: &GameData) -> SaveGame {
        let player = self.config.player_faction;
        let opponent = self.config.ai_faction;
        let map_name = self.map_id.0.map_or("Custom Map", |id| id.name()).to_string();
        SaveGame {
            version: SaveGame::VERSION,
            summary: SaveSummary {
                saved_at: now_secs(),
                map: map_name,
                day: self.turn_state.turn_number,
                armies: vec![(player, self.commanders.get_active(player)), (opponent, self.commanders.get_active(opponent))],
            },
            map_id: self.map_id.0,
            terrain: self.map.tiles.clone(),
            tiles: self.tiles.iter()
                .filter(|t| t.owner.is_some() || t.capture_progress > 0 || game_data.terrain_capturable(t.terrain))
                .map(|t| SavedTile {
                    x: t.position.x,
                    y: t.position.y,
                    owner: t.owner,
                    capture_progress: t.capture_progress,
                    capturing_faction: t.capturing_faction,
                })
                .collect(),
            units: self.units.iter().map(|(u, pos, member)| SavedUnit {
                unit_type: u.unit_type,
                faction: member.faction,
                x: pos.x,
                y: pos.y,
                hp: u.hp,
                stamina: u.stamina,
                ammo: u.ammo,
                moved: u.moved,
                attacked: u.attacked,
                exhausted: u.exhausted,
                paralyzed: u.paralyzed,
                cargo: u.cargo.iter().map(|c| SavedCargoUnit {
                    unit_type: c.unit_type,
                    hp: c.hp,
                    stamina: c.stamina,
                    ammo: c.ammo,
                }).collect(),
            }).collect(),
            player_faction: player,
            ai_faction: opponent,
            current_faction: self.turn_state.current_faction,
            funds: Faction::all().iter().map(|&f| (f, self.funds.get(f))).collect(),
            commanders: SavedCommanders {
                active: self.commanders.active.clone(),
                power_meter: self.commanders.power_meter.clone(),
                power_active: self.commanders.power_active.clone(),
                power_uses: self.commanders.power_uses.clone(),
                power_attack: self.commanders.power_attack.clone(),
            },
            weather: SavedWeather {
                current: self.weather.current,
                turns_remaining: self.weather.turns_remaining,
                dynamic_weather: self.weather.dynamic_weather,
                change_chance: self.weather.change_chance,
            },
            fog: self.fog.enabled,
            fielded: self.result.fielded.clone(),
        }
    }
}

fn handle_save_game(
    mut events: MessageReader<SaveGameEvent>,
    battle: BattleSnapshot,
    game_data: Res<GameData>,
    mut index: ResMut<SaveIndex>,
    mut notice: ResMut<SaveNotice>,
) {
    for event in events.read() {
        let manual = event.slot != SaveSlot::Auto;
        if !battle.can_save() {
            if manual {
                notice.show("You can only save on your turn, between moves", true);
            }
            continue;
        }
        let save = battle.snapshot(&game_data);
        let result = serde_json::to_string(&save).map_err(|e| e.to_string())
            .and_then(|json| write_setting(&event.slot.key(), &json));
        match result {
            Ok(()) => {
                if manual {
                    notice.show(format!("Saved to {}", event.slot.name()), false);
                }
                index.refresh();
            }
            Err(e) => {
                error!("Failed to save to {}: {e}", event.slot.name());
                notice.show(format!("Couldn't save: {e}"), true);
            }
        }
    }
}

/// Autosave at the start of each human turn (once its banner is done) and
/// after each of their units acts
fn autosave(
    battle: BattleSnapshot,
    mut saves: MessageWriter<SaveGameEvent>,
    mut last: Local<Option<(u32, Faction, usize)>>,
) {
    if !battle.can_save() {
        return;
    }
    let turn = (battle.turn_state.turn_number, battle.turn_state.current_faction);
    let done = battle.units.iter()
        .filter(|(u, _, member)| member.faction == turn.1 && u.exhausted)
        .count();
    let changed = match *last {
        Some((day, faction, count)) => (day, faction) != turn || done != count,
        None => true,
    };
    if changed {
        saves.write(SaveGameEvent { slot: SaveSlot::Auto });
        *last = Some((turn.0, turn.1, done));
    }
}

// ============================================================================
// LOADING
// ============================================================================

fn handle_load_game(
    mut events: MessageReader<LoadGameEvent>,
    mut pending: ResMut<PendingLoad>,
    mut notice: ResMut<SaveNotice>,
    state: Res<State<GameState>>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    for event in events.read() {
        let loaded = read_setting(&event.slot.key())
            .ok_or_else(|| format!("{} is empty", event.slot.name()))
            .and_then(|json| parse_save(&json));
        match loaded {
            Ok(save) => {
                pending.0 = Some(save);
                if *state.get() != GameState::Battle {
                    next_state.set(GameState::Battle);
                }
            }
            Err(e) => notice.show(e, true),
        }
    }
}

/// Capture progress to put back on the tiles once they exist
#[derive(Resource)]
struct RestoreCaptures(Vec<SavedTile>);

/// Rebuild the saved battle: the old board goes, the saved one is spawned and
/// its state restored, and the turn resumes with its Day banner
fn apply_pending_load(
    mut commands: Commands,
    mut pending: ResMut<PendingLoad>,
    mut game_map: ResMut<GameMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    sprites: Res<SpriteAssets>,
    game_data: Res<GameData>,
    theme: Res<TilesetTheme>,
    ai: Res<AiState>,
    entities: Query<Entity, With<BattleEntity>>,
    mut loaded: MessageWriter<GameLoadedEvent>,
    mut notice: ResMut<SaveNotice>,
) {
    // Wait for sprites so every unit and tile spawns with its art
    if pending.0.is_none() || !sprites.loading_complete {
        return;
    }
    let Some(save) = pending.0.take() else { return };
    for entity in &entities {
        commands.entity(entity).despawn();
    }

    let height = save.terrain.len() as u32;
    let width = save.terrain.first().map_or(0, |row| row.len() as u32);
    let map_data = MapData {
        name: save.summary.map.clone(),
        description: String::new(),
        width,
        height,
        terrain: save.terrain.clone(),
        units: Vec::new(),
        properties: save.tiles.iter()
            .filter_map(|t| t.owner.map(|owner| PropertyOwnership { x: t.x, y: t.y, owner }))
            .collect(),
    };
    spawn_map_from_data(&mut commands, &mut game_map, &mut meshes, &mut materials, &sprites, &mut images,
        &game_data, &map_data, *theme);
    for saved in &save.units {
        let mut unit = Unit::new(saved.unit_type, &game_data);
        unit.hp = saved.hp;
        unit.stamina = saved.stamina;
        unit.ammo = saved.ammo;
        unit.moved = saved.moved;
        unit.attacked = saved.attacked;
        unit.exhausted = saved.exhausted;
        unit.paralyzed = saved.paralyzed;
        unit.cargo = saved.cargo.iter().map(|c| CargoUnit {
            unit_type: c.unit_type,
            hp: c.hp,
            stamina: c.stamina,
            ammo: c.ammo,
        }).collect();
        spawn_unit_with_state(&mut commands, &game_map, &mut meshes, &mut materials, &sprites, &images,
            &game_data, saved.faction, unit, saved.x, saved.y);
    }
    commands.insert_resource(RestoreCaptures(save.tiles.iter().filter(|t| t.capture_progress > 0).cloned().collect()));

    let day = save.summary.day;
    commands.insert_resource(BattleConfig::new(save.player_faction, save.ai_faction));
    commands.insert_resource(BattleMapId(save.map_id));
    commands.insert_resource(TurnState { current_faction: save.current_faction, turn_number: day, phase: TurnPhase::Select });
    let mut funds = FactionFunds::default();
    for (&faction, &amount) in &save.funds {
        funds.add(faction, amount);
    }
    commands.insert_resource(funds);
    commands.insert_resource(Commanders {
        active: save.commanders.active.clone(),
        power_meter: save.commanders.power_meter.clone(),
        power_active: save.commanders.power_active.clone(),
        power_uses: save.commanders.power_uses.clone(),
        power_attack: save.commanders.power_attack.clone(),
    });
    let mut weather = Weather::new(save.weather.current);
    weather.turns_remaining = save.weather.turns_remaining;
    weather.dynamic_weather = save.weather.dynamic_weather;
    weather.change_chance = save.weather.change_chance;
    commands.insert_resource(weather);
    let mut fog = FogOfWar::default();
    fog.enabled = save.fog;
    commands.insert_resource(fog);
    commands.insert_resource(GameResult { fielded: save.fielded.clone(), ..default() });
    commands.insert_resource(PendingAction::default());
    commands.insert_resource(MovementHighlights::default());
    commands.insert_resource(MovementPath::default());
    commands.insert_resource(ProductionState::default());
    commands.insert_resource(MapMenu::default());
    reset_ai(&mut commands, ai.enabled);

    // The turn picks up with its Day banner, the cursor on the army's HQ
    commands.insert_resource(TurnBanner { faction: Some(save.current_faction), day, elapsed: 0.0, intro: false });
    let hq = save.tiles.iter()
        .find(|t| t.owner == Some(save.current_faction) && save.terrain[t.y as usize][t.x as usize] == Terrain::Base)
        .map(|t| (t.x, t.y))
        .or_else(|| save.units.iter().find(|u| u.faction == save.current_faction).map(|u| (u.x, u.y)))
        .unwrap_or((width as i32 / 2, height as i32 / 2));
    commands.insert_resource(GridCursor { x: hq.0, y: hq.1, visible: true, follow: true });

    let player_co = save.commanders.active.get(&save.player_faction).copied()
        .unwrap_or_else(|| Commanders::default().get_active(save.player_faction));
    loaded.write(GameLoadedEvent { map_id: save.map_id, player_faction: save.player_faction, player_co });
    notice.show(format!("Day {day} · {}", save.summary.map), false);
    info!("Loaded a save: {} day {day}", save.summary.map);
}

fn restore_captures(mut commands: Commands, restore: Option<Res<RestoreCaptures>>, mut tiles: Query<&mut Tile>) {
    let Some(restore) = restore else { return };
    if tiles.is_empty() {
        return;
    }
    for mut tile in &mut tiles {
        if let Some(saved) = restore.0.iter().find(|t| t.x == tile.position.x && t.y == tile.position.y) {
            tile.capture_progress = saved.capture_progress;
            tile.capturing_faction = saved.capturing_faction;
        }
    }
    commands.remove_resource::<RestoreCaptures>();
}

// ============================================================================
// BACKUPS (web: export every save to a file, import it elsewhere)
// ============================================================================

/// One file holding every slot, keyed by storage name
#[derive(Serialize, Deserialize)]
struct Backup {
    paw_and_claw_backup: u32,
    saves: HashMap<String, serde_json::Value>,
}

/// Whether backups can be exported and imported here (the web build)
pub const BACKUPS: bool = cfg!(target_arch = "wasm32");

/// Waiting for the browser's file picker to hand over a backup
#[derive(Resource, Default)]
pub struct ImportPending(pub bool);

#[cfg(target_arch = "wasm32")]
mod web {
    use wasm_bindgen::prelude::*;

    // Defined by web/index.html
    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_name = pawExportSaves, catch)]
        pub fn export_saves(filename: &str, json: &str) -> Result<(), JsValue>;
        #[wasm_bindgen(js_name = pawImportSaves, catch)]
        pub fn import_saves() -> Result<(), JsValue>;
    }
}

/// Hand every save to the browser as a backup file (download or share sheet)
pub fn export_backup(notice: &mut SaveNotice) {
    let saves: HashMap<String, serde_json::Value> = SaveSlot::all()
        .filter_map(|slot| {
            let json = read_setting(&slot.key())?;
            Some((slot.key(), serde_json::from_str(&json).ok()?))
        })
        .collect();
    if saves.is_empty() {
        notice.show("There are no saves to back up yet", true);
        return;
    }
    let count = saves.len();
    let Ok(json) = serde_json::to_string(&Backup { paw_and_claw_backup: 1, saves }) else { return };
    #[cfg(target_arch = "wasm32")]
    {
        match web::export_saves("paw-and-claw-saves.json", &json) {
            Ok(()) => notice.show(format!("Backed up {count} save{}", if count == 1 { "" } else { "s" }), false),
            Err(_) => notice.show("This page can't export files; reload it and try again", true),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (json, count);
        notice.show("Saves are files in the saves folder", false);
    }
}

/// Open the browser's file picker for a backup; `poll_import` takes it from there
pub fn request_import(pending: &mut ImportPending, notice: &mut SaveNotice) {
    #[cfg(target_arch = "wasm32")]
    {
        remove_setting("import");
        match web::import_saves() {
            Ok(()) => pending.0 = true,
            Err(_) => notice.show("This page can't import files; reload it and try again", true),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = pending;
        notice.show("Saves are files in the saves folder", false);
    }
}

/// The page's file picker leaves the chosen file in storage under "import"
fn poll_import(mut pending: ResMut<ImportPending>, mut index: ResMut<SaveIndex>, mut notice: ResMut<SaveNotice>) {
    if !pending.0 {
        return;
    }
    let Some(json) = read_setting("import") else { return };
    remove_setting("import");
    pending.0 = false;
    match import_backup(&json) {
        Ok(count) => notice.show(format!("Restored {count} save{}", if count == 1 { "" } else { "s" }), false),
        Err(e) => notice.show(e, true),
    }
    index.refresh();
}

/// Write a backup's saves into their slots; a single save goes into the first free slot
fn import_backup(json: &str) -> Result<usize, String> {
    if let Ok(backup) = serde_json::from_str::<Backup>(json) {
        let mut count = 0;
        for (key, save) in backup.saves {
            let (Some(_), Ok(text)) = (SaveSlot::from_key(&key), serde_json::to_string(&save)) else { continue };
            if parse_save(&text).is_ok() {
                write_setting(&key, &text)?;
                count += 1;
            }
        }
        return if count > 0 { Ok(count) } else { Err("That backup holds no saves this version can load".to_string()) };
    }
    parse_save(json).map_err(|_| "That file isn't a Paw & Claw backup".to_string())?;
    let free = (1..=MANUAL_SLOTS).map(SaveSlot::Manual).find(|&slot| read_summary(slot).is_none())
        .ok_or("Every slot is full; free one up first")?;
    write_setting(&free.key(), json)?;
    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SaveGame {
        SaveGame {
            version: SaveGame::VERSION,
            summary: SaveSummary { saved_at: 1.0, map: "Spann Island".into(), day: 4,
                armies: vec![(Faction::Western, CommanderId::Kira), (Faction::Northern, CommanderId::Grimjaw)] },
            map_id: Some(MapId::SpannIsland),
            terrain: vec![vec![Terrain::Grass; 3]; 2],
            tiles: vec![SavedTile { x: 1, y: 0, owner: Some(Faction::Western), capture_progress: 8, capturing_faction: Some(Faction::Northern) }],
            units: vec![SavedUnit { unit_type: UnitType::Scout, faction: Faction::Northern, x: 1, y: 0, hp: 60, stamina: 50,
                ammo: 0, moved: true, attacked: false, exhausted: true, paralyzed: false, cargo: vec![] }],
            player_faction: Faction::Western,
            ai_faction: Faction::Northern,
            current_faction: Faction::Western,
            funds: HashMap::from([(Faction::Western, 4000)]),
            commanders: SavedCommanders {
                active: HashMap::from([(Faction::Western, CommanderId::Kira)]),
                power_meter: HashMap::from([(Faction::Western, 120)]),
                power_active: HashMap::new(),
                power_uses: HashMap::new(),
                power_attack: HashMap::new(),
            },
            weather: SavedWeather { current: WeatherType::Rain, turns_remaining: 2, dynamic_weather: true, change_chance: 20 },
            fog: true,
            fielded: vec![Faction::Western, Faction::Northern],
        }
    }

    #[test]
    fn a_save_reads_back_whole_and_lists_by_its_header() {
        let json = serde_json::to_string(&sample()).unwrap();
        let save = parse_save(&json).unwrap();
        assert_eq!(save.summary, sample().summary);
        assert_eq!(save.tiles[0].capture_progress, 8);
        assert!(save.units[0].exhausted);
        assert_eq!(save.funds[&Faction::Western], 4000);
        let header: SaveHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(header.summary.day, 4);
    }

    #[test]
    fn old_and_broken_saves_are_refused() {
        let mut old = serde_json::to_value(sample()).unwrap();
        old["version"] = 1.into();
        assert!(parse_save(&old.to_string()).err().unwrap().contains("older"));
        assert!(parse_save("{not json").is_err());
    }

    #[test]
    fn slots_have_stable_storage_names() {
        let keys: Vec<_> = SaveSlot::all().map(|s| s.key()).collect();
        assert_eq!(keys, ["save_auto", "save_1", "save_2", "save_3"]);
        assert_eq!(SaveSlot::from_key("save_2"), Some(SaveSlot::Manual(2)));
    }
}
