//! The campaign: story missions played in order.
//!
//! Missions are data (`assets/data/campaign.ron`, part of `GameData`): the
//! armies and their commanders, a map drawn as text, the objective and the
//! story told before, during and after the battle. A mission is an ordinary
//! battle with `BattleMap::mission` set, so it saves and loads like any
//! other. Clearing a mission unlocks the next; `CampaignProgress` remembers
//! which are cleared and in how few days.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{
    get_builtin_map, read_setting, write_setting, BattleConfig, BattleMap, CommanderId, Faction, GameData,
    GameResult, MapData, MapId, Terrain, TurnStartEvent, TurnState, UnitType, WeatherType, ARMY_SLOTS,
};
use crate::states::GameState;

pub struct CampaignPlugin;

impl Plugin for CampaignPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CampaignProgress::read())
            .init_resource::<Dialogue>()
            .add_systems(Update, (day_dialogue, mission_result, tick_dialogue).run_if(in_state(GameState::Battle)))
            .add_systems(OnExit(GameState::Battle), end_dialogue);
    }
}

// ============================================================================
// DATA (campaign.ron)
// ============================================================================

/// The whole campaign: its missions in the order they're played
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CampaignData {
    pub name: String,
    #[serde(default)]
    pub tagline: String,
    pub missions: Vec<MissionData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionData {
    /// Stable id: progress and saves refer to the mission by it
    pub key: String,
    pub name: String,
    /// A sentence or two for the mission list
    pub summary: String,
    /// The armies in turn order; the first is army 1 on the map
    pub armies: Vec<MissionArmy>,
    pub map: MissionMap,
    #[serde(default)]
    pub objective: Objective,
    #[serde(default)]
    pub fog: bool,
    /// Weather that holds for the whole mission (none: the usual weather)
    #[serde(default)]
    pub weather: Option<WeatherType>,
    /// Spoken before the first turn
    #[serde(default)]
    pub briefing: Vec<Line>,
    /// Spoken when the player's turn starts on these days (from day 2)
    #[serde(default)]
    pub days: Vec<(u32, Vec<Line>)>,
    /// Spoken after winning, and after losing
    #[serde(default)]
    pub victory: Vec<Line>,
    #[serde(default)]
    pub defeat: Vec<Line>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissionArmy {
    pub faction: Faction,
    pub commander: CommanderId,
    /// Played by the person; every other army is the computer's
    #[serde(default)]
    pub player: bool,
    /// Armies with the same team number are allies
    pub team: u8,
    /// Funds it starts with (income comes on top, from day 1)
    #[serde(default)]
    pub funds: u32,
}

/// How the player wins. Losing is always the same: routed, or the HQ taken
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Objective {
    /// Rout every enemy army or take its HQ
    #[default]
    Defeat,
    /// Still be standing when this many days have passed
    Survive(u32),
}

impl Objective {
    pub fn describe(&self) -> String {
        match self {
            Objective::Defeat => "Rout the enemy or take their HQ".to_string(),
            Objective::Survive(days) => format!("Hold out for {days} days"),
        }
    }
}

/// One spoken line: who says it, and what
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line(pub CommanderId, pub String);

/// A mission's battlefield
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MissionMap {
    /// One of the built-in maps, as skirmish plays it
    Builtin(MapId),
    /// A map drawn as text. Each row is a line of tiles separated by spaces:
    /// a terrain letter (see `terrain_from_char`), then the number of the
    /// army that owns it, if any (`H1` is army 1's HQ, `O` a neutral outpost).
    /// Units are (army, unit, x, y), counting from the top-left tile (0, 0).
    Layout {
        rows: Vec<String>,
        #[serde(default)]
        units: Vec<(u8, UnitType, i32, i32)>,
    },
}

/// The letters of a text map
pub fn terrain_from_char(c: char) -> Option<Terrain> {
    Some(match c {
        '.' => Terrain::Grass,
        ',' => Terrain::TallGrass,
        'T' => Terrain::Thicket,
        'B' => Terrain::Brambles,
        '=' => Terrain::Log,
        '^' => Terrain::Boulder,
        'h' => Terrain::Hollow,
        '~' => Terrain::Creek,
        'w' => Terrain::Pond,
        'L' => Terrain::LilyPads,
        ':' => Terrain::Shore,
        'H' => Terrain::Base,
        'W' => Terrain::Workshop,
        'D' => Terrain::Dock,
        'R' => Terrain::Roost,
        'O' => Terrain::Outpost,
        'S' => Terrain::Storehouse,
        _ => return None,
    })
}

impl MissionData {
    /// The army the person plays
    pub fn player(&self) -> Faction {
        self.armies.iter().find(|a| a.player).or(self.armies.first()).map_or(Faction::Western, |a| a.faction)
    }

    /// The battlefield, with the map's army slots still as placeholders
    /// (`MapData::with_armies` hands them to the mission's armies)
    pub fn map_data(&self) -> Result<MapData, String> {
        let mut map = match &self.map {
            MissionMap::Builtin(id) => get_builtin_map(*id),
            MissionMap::Layout { rows, units } => parse_layout(rows, units)?,
        };
        map.name = self.name.clone();
        map.description = self.summary.clone();
        Ok(map)
    }

    /// The days of a holdout mission
    pub fn day_limit(&self) -> Option<u32> {
        match self.objective {
            Objective::Survive(days) => Some(days),
            Objective::Defeat => None,
        }
    }
}

fn parse_layout(rows: &[String], units: &[(u8, UnitType, i32, i32)]) -> Result<MapData, String> {
    let width = rows.first().map_or(0, |row| row.split_whitespace().count());
    if width == 0 {
        return Err("the map has no tiles".to_string());
    }
    let mut map = MapData::new("", width as u32, rows.len() as u32);
    for (y, row) in rows.iter().enumerate() {
        let cells: Vec<&str> = row.split_whitespace().collect();
        if cells.len() != width {
            return Err(format!("map row {} has {} tiles, the first has {width}", y + 1, cells.len()));
        }
        for (x, cell) in cells.iter().enumerate() {
            let mut chars = cell.chars();
            let terrain = chars.next().and_then(terrain_from_char)
                .ok_or_else(|| format!("map row {}, tile {}: unknown terrain '{cell}'", y + 1, x + 1))?;
            map.set_terrain(x as i32, y as i32, terrain);
            match (chars.next(), chars.next()) {
                (None, _) => {}
                (Some(owner @ '1'..='4'), None) => {
                    map.add_property(x as i32, y as i32, ARMY_SLOTS[owner as usize - '1' as usize]);
                }
                _ => return Err(format!("map row {}, tile {}: '{cell}' should be a terrain letter and an army number", y + 1, x + 1)),
            }
        }
    }
    for &(army, unit, x, y) in units {
        if !(1..=4).contains(&army) {
            return Err(format!("unit at ({x}, {y}) belongs to army {army}; armies are 1 to 4"));
        }
        map.add_unit(unit, ARMY_SLOTS[army as usize - 1], x, y);
    }
    Ok(map)
}

impl CampaignData {
    pub fn mission(&self, key: &str) -> Option<&MissionData> {
        self.missions.iter().find(|m| m.key == key)
    }

    pub fn index_of(&self, key: &str) -> Option<usize> {
        self.missions.iter().position(|m| m.key == key)
    }

    /// A mod's missions: one with a known key replaces that mission, new ones
    /// follow the last (mods load on desktop only)
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub fn merge(&mut self, other: CampaignData) {
        for mission in other.missions {
            match self.index_of(&mission.key) {
                Some(i) => self.missions[i] = mission,
                None => self.missions.push(mission),
            }
        }
    }

    /// Everything that would keep a mission from being played
    pub fn problems(&self, game_data: &GameData) -> Vec<String> {
        let mut problems = Vec::new();
        for (i, mission) in self.missions.iter().enumerate() {
            let mut problem = |text: String| problems.push(format!("mission '{}': {text}", mission.key));
            if self.missions[..i].iter().any(|m| m.key == mission.key) {
                problem("its key is used twice".to_string());
            }
            let armies = &mission.armies;
            if !(2..=4).contains(&armies.len()) {
                problem(format!("has {} armies; a battle takes 2 to 4", armies.len()));
                continue;
            }
            if armies.iter().filter(|a| a.player).count() != 1 {
                problem("exactly one army must be the player's".to_string());
            }
            if armies.iter().enumerate().any(|(i, a)| armies[..i].iter().any(|b| b.faction == a.faction)) {
                problem("two armies share a faction".to_string());
            }
            if armies.iter().all(|a| a.team == armies[0].team) {
                problem("every army is on the same team".to_string());
            }
            if mission.objective == Objective::Survive(0) || mission.days.iter().any(|(day, _)| *day < 2) {
                problem("days count from 1; day lines start on day 2 (day 1 is the briefing)".to_string());
            }
            let map = match mission.map_data() {
                Ok(map) => map,
                Err(e) => {
                    problem(e);
                    continue;
                }
            };
            let slot = |faction: Faction| ARMY_SLOTS.iter().position(|&s| s == faction);
            for p in &map.properties {
                let terrain = map.get_terrain(p.x, p.y).unwrap_or(Terrain::Grass);
                if !game_data.terrain_capturable(terrain) {
                    problem(format!("({}, {}) has an owner but {} can't be owned", p.x, p.y, game_data.terrain_name(terrain)));
                }
                if slot(p.owner).is_some_and(|s| s >= armies.len()) {
                    problem(format!("({}, {}) belongs to an army the mission doesn't have", p.x, p.y));
                }
            }
            for (n, unit) in map.units.iter().enumerate() {
                let name = game_data.unit_name(unit.unit_type);
                match map.get_terrain(unit.x, unit.y) {
                    None => problem(format!("{name} at ({}, {}) is off the map", unit.x, unit.y)),
                    Some(terrain) if !game_data.is_passable(terrain, game_data.unit_class(unit.unit_type)) => {
                        problem(format!("{name} at ({}, {}) can't stand on {}", unit.x, unit.y, game_data.terrain_name(terrain)));
                    }
                    Some(_) => {}
                }
                if slot(unit.faction).is_some_and(|s| s >= armies.len()) {
                    problem(format!("{name} at ({}, {}) belongs to an army the mission doesn't have", unit.x, unit.y));
                }
                if map.units[..n].iter().any(|other| (other.x, other.y) == (unit.x, unit.y)) {
                    problem(format!("two units stand on ({}, {})", unit.x, unit.y));
                }
            }
            // Every army needs something on the field: units, or an HQ to hold
            for (index, _) in armies.iter().enumerate() {
                let army = ARMY_SLOTS[index];
                if !map.units.iter().any(|u| u.faction == army) && map.hq_of(army).is_none() {
                    problem(format!("army {} has no units and no HQ", index + 1));
                }
            }
        }
        problems
    }
}

// ============================================================================
// PROGRESS
// ============================================================================

/// Which missions are cleared, kept with the saves (`campaign` in storage)
#[derive(Resource, Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct CampaignProgress {
    /// Mission key to the fewest days it was won in
    #[serde(default)]
    pub cleared: HashMap<String, u32>,
}

/// Storage name of the campaign progress
pub const PROGRESS_KEY: &str = "campaign";

impl CampaignProgress {
    pub fn read() -> Self {
        read_setting(PROGRESS_KEY).and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
    }

    /// Note a win; returns whether it's a first clear or a new best
    pub fn record(&mut self, key: &str, days: u32) -> bool {
        let better = self.cleared.get(key).is_none_or(|&best| days < best);
        if better {
            self.cleared.insert(key.to_string(), days);
        }
        better
    }

    fn write(&self) {
        let result = serde_json::to_string(self).map_err(|e| e.to_string())
            .and_then(|json| write_setting(PROGRESS_KEY, &json));
        if let Err(e) = result {
            error!("Couldn't store campaign progress: {e}");
        }
    }

    pub fn best(&self, key: &str) -> Option<u32> {
        self.cleared.get(key).copied()
    }

    /// A mission opens once the one before it is cleared (`PAW_UNLOCK=1`
    /// opens them all, for development)
    pub fn unlocked(&self, campaign: &CampaignData, index: usize) -> bool {
        index == 0
            || unlock_all()
            || campaign.missions.get(index - 1).is_some_and(|before| self.cleared.contains_key(&before.key))
    }

    /// The mission to play next: the first one not cleared yet, if any is left
    pub fn next(&self, campaign: &CampaignData) -> Option<usize> {
        campaign.missions.iter().position(|m| !self.cleared.contains_key(&m.key))
    }
}

fn unlock_all() -> bool {
    static UNLOCK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *UNLOCK.get_or_init(|| std::env::var_os("PAW_UNLOCK").is_some())
}

// ============================================================================
// DIALOGUE
// ============================================================================

/// The conversation on screen, if any. While it runs the board is modal and
/// the turn banner (and with it the computer) waits.
#[derive(Resource, Default)]
pub struct Dialogue {
    pub lines: Vec<Line>,
    pub index: usize,
    /// Seconds the current line has been up (its text types itself out)
    pub elapsed: f32,
}

impl Dialogue {
    /// Letters typed per second
    pub const SPEED: f32 = 55.0;

    pub fn start(&mut self, lines: &[Line]) {
        *self = Dialogue { lines: lines.to_vec(), index: 0, elapsed: 0.0 };
    }

    pub fn active(&self) -> bool {
        self.index < self.lines.len()
    }

    pub fn current(&self) -> Option<&Line> {
        self.lines.get(self.index)
    }

    /// How many letters of the current line show so far
    pub fn typed(&self) -> usize {
        (self.elapsed * Self::SPEED) as usize
    }

    /// Confirm: finish typing the line, or go on to the next
    pub fn advance(&mut self) {
        let Some(Line(_, text)) = self.current() else { return };
        if self.typed() < text.chars().count() {
            self.elapsed = 1.0e6;
        } else {
            self.index += 1;
            self.elapsed = 0.0;
        }
    }

    pub fn skip(&mut self) {
        self.index = self.lines.len();
    }
}

fn tick_dialogue(time: Res<Time>, mut dialogue: ResMut<Dialogue>) {
    if dialogue.active() {
        dialogue.elapsed += time.delta_secs();
    }
}

fn end_dialogue(mut dialogue: ResMut<Dialogue>) {
    dialogue.skip();
}

/// The mission being played, if the battle is one
fn current_mission<'a>(battle_map: &BattleMap, game_data: &'a GameData) -> Option<&'a MissionData> {
    game_data.campaign.mission(battle_map.mission.as_deref()?)
}

/// Lines for the day that just started on the player's turn
fn day_dialogue(
    mut events: MessageReader<TurnStartEvent>,
    battle_map: Res<BattleMap>,
    game_data: Res<GameData>,
    turn_state: Res<TurnState>,
    mut dialogue: ResMut<Dialogue>,
) {
    let Some(event) = events.read().last() else { return };
    let Some(mission) = current_mission(&battle_map, &game_data) else { return };
    if event.faction != mission.player() {
        return;
    }
    if let Some((_, lines)) = mission.days.iter().find(|(day, _)| *day == turn_state.turn_number) {
        dialogue.start(lines);
    }
}

/// When a mission ends: a win is written to the progress, and the closing
/// lines play before the result screen
fn mission_result(
    game_result: Res<GameResult>,
    battle_map: Res<BattleMap>,
    battle_config: Res<BattleConfig>,
    turn_state: Res<TurnState>,
    game_data: Res<GameData>,
    mut progress: ResMut<CampaignProgress>,
    mut dialogue: ResMut<Dialogue>,
    mut was_over: Local<bool>,
) {
    let over = game_result.game_over;
    if !std::mem::replace(&mut *was_over, over) && over {
        let Some(mission) = current_mission(&battle_map, &game_data) else { return };
        let Some(winner) = game_result.winner else { return };
        if battle_config.allied(mission.player(), winner) {
            // A holdout is won when the last day has passed, not on the day after
            let days = mission.day_limit().map_or(turn_state.turn_number, |limit| turn_state.turn_number.min(limit));
            if progress.record(&mission.key, days) {
                progress.write();
            }
            dialogue.start(&mission.victory);
        } else {
            dialogue.start(&mission.defeat);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_campaign_is_playable() {
        let data = GameData::load_defaults();
        assert!(!data.campaign.missions.is_empty());
        assert_eq!(data.campaign.problems(&data), Vec::<String>::new());
    }

    #[test]
    fn every_mission_has_two_sides_and_a_story() {
        let data = GameData::load_defaults();
        for mission in &data.campaign.missions {
            let factions: Vec<Faction> = mission.armies.iter().map(|a| a.faction).collect();
            let map = mission.map_data().unwrap().with_armies(&factions);
            let player_team = mission.armies.iter().find(|a| a.player).unwrap().team;
            for army in &mission.armies {
                assert!(map.units.iter().any(|u| u.faction == army.faction) || map.hq_of(army.faction).is_some(),
                    "{}: {:?} has nothing on the field", mission.key, army.faction);
            }
            assert!(mission.armies.iter().any(|a| a.team != player_team), "{}: no enemy", mission.key);
            assert!(!mission.briefing.is_empty() && !mission.victory.is_empty(), "{}: needs a briefing and an ending", mission.key);
        }
    }

    #[test]
    fn a_text_map_reads_terrain_owners_and_units() {
        let rows = vec!["H1 . T ~".to_string(), "W1 = O H2".to_string()];
        let map = parse_layout(&rows, &[(1, UnitType::Scout, 1, 0), (2, UnitType::Ironclad, 2, 1)]).unwrap();
        assert_eq!((map.width, map.height), (4, 2));
        assert_eq!(map.get_terrain(3, 0), Some(Terrain::Creek));
        assert_eq!(map.get_terrain(2, 1), Some(Terrain::Outpost));
        assert_eq!(map.hq_of(ARMY_SLOTS[0]), Some((0, 0)));
        assert_eq!(map.hq_of(ARMY_SLOTS[1]), Some((3, 1)));
        assert_eq!(map.properties.len(), 3, "the neutral outpost has no owner");
        assert_eq!(map.units.len(), 2);
        assert_eq!(map.units[1].faction, ARMY_SLOTS[1]);

        assert!(parse_layout(&["H1 .".to_string(), ".".to_string()], &[]).is_err(), "ragged rows");
        assert!(parse_layout(&["H9 .".to_string()], &[]).is_err(), "army 9");
        assert!(parse_layout(&["? .".to_string()], &[]).is_err(), "unknown letter");
    }

    #[test]
    fn missions_unlock_in_order_and_keep_the_best_time() {
        let data = GameData::load_defaults();
        let campaign = &data.campaign;
        let mut progress = CampaignProgress::default();
        assert!(progress.unlocked(campaign, 0));
        assert_eq!(progress.next(campaign), Some(0));
        if campaign.missions.len() > 1 && !unlock_all() {
            assert!(!progress.unlocked(campaign, 1));
        }
        let first = &campaign.missions[0].key;
        assert!(progress.record(first, 9));
        assert!(!progress.record(first, 12), "a slower win doesn't replace the record");
        assert!(progress.record(first, 7));
        assert_eq!(progress.best(first), Some(7));
        if campaign.missions.len() > 1 {
            assert!(progress.unlocked(campaign, 1));
            assert_eq!(progress.next(campaign), Some(1));
        }
        let json = serde_json::to_string(&progress).unwrap();
        assert_eq!(serde_json::from_str::<CampaignProgress>(&json).unwrap(), progress);
    }

    #[test]
    fn dialogue_types_a_line_then_moves_on() {
        let mut dialogue = Dialogue::default();
        assert!(!dialogue.active());
        dialogue.start(&[Line(CommanderId::Dusty, "Hello there".into()), Line(CommanderId::Shell, "Hm".into())]);
        assert!(dialogue.active());
        // The first press shows the whole line, the second goes on
        dialogue.advance();
        assert_eq!(dialogue.index, 0);
        dialogue.advance();
        assert_eq!(dialogue.index, 1);
        dialogue.skip();
        assert!(!dialogue.active());
    }

    #[test]
    fn a_mod_replaces_missions_by_key_and_adds_new_ones() {
        let data = GameData::load_defaults();
        let mut campaign = data.campaign.clone();
        let count = campaign.missions.len();
        let mut changed = campaign.missions[0].clone();
        changed.name = "Changed".to_string();
        let mut added = campaign.missions[0].clone();
        added.key = "brand_new".to_string();
        campaign.merge(CampaignData { name: String::new(), tagline: String::new(), missions: vec![changed, added] });
        assert_eq!(campaign.missions.len(), count + 1);
        assert_eq!(campaign.missions[0].name, "Changed");
        assert_eq!(campaign.missions.last().unwrap().key, "brand_new");
    }
}
