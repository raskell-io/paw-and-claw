use bevy::prelude::*;
use std::collections::HashMap;

use super::{Faction, FactionMember, Unit, Tile, Terrain, Commanders, GridPosition, MovementHighlights, GameData, UnitType};

pub struct TurnPlugin;

impl Plugin for TurnPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TurnState>()
            .init_resource::<FactionFunds>()
            .init_resource::<GameResult>()
            .init_resource::<BattleConfig>()
            .init_resource::<TurnBanner>()
            .init_resource::<HandOver>()
            .add_message::<TurnStartEvent>()
            .add_message::<EndTurnRequest>()
            .add_systems(Update, (
                check_victory_condition,
                pass_turn,
                (generate_income, resupply_units).chain(),
                (start_turn_banner, tick_turn_banner).chain(),
            ));
    }
}

/// One army in a battle: who leads it and who plays it
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Army {
    pub faction: Faction,
    /// Played by the computer
    pub ai: bool,
    /// Armies on the same team are allies: they never fight and win together
    pub team: u8,
    /// Routed or lost its HQ: out of the battle
    #[serde(default)]
    pub defeated: bool,
    /// Its units and portraits face left (it starts on the right of the map)
    #[serde(default)]
    pub facing_left: bool,
}

/// The armies in the current battle, in turn order (2 to 4)
#[derive(Resource, Clone, Debug)]
pub struct BattleConfig {
    pub armies: Vec<Army>,
    /// Advance Wars versus rule: when this many days have passed, the side
    /// holding the most properties wins
    pub day_limit: Option<u32>,
    /// The home perspective: the first army a person plays (or the first army
    /// when only the computer plays). Single-player screens show this side.
    pub player_faction: Faction,
}

impl Default for BattleConfig {
    fn default() -> Self {
        Self::new(Faction::Eastern, Faction::Northern)
    }
}

impl BattleConfig {
    /// A one-on-one battle: a person against the computer
    pub fn new(player_faction: Faction, ai_faction: Faction) -> Self {
        Self::from_armies(vec![
            Army { faction: player_faction, ai: false, team: 0, defeated: false, facing_left: false },
            Army { faction: ai_faction, ai: true, team: 1, defeated: false, facing_left: true },
        ])
    }

    pub fn from_armies(armies: Vec<Army>) -> Self {
        let player_faction = armies.iter().find(|a| !a.ai).or(armies.first()).map_or(Faction::Eastern, |a| a.faction);
        Self { armies, player_faction, day_limit: None }
    }

    pub fn army(&self, faction: Faction) -> Option<&Army> {
        self.armies.iter().find(|a| a.faction == faction)
    }

    pub fn army_mut(&mut self, faction: Faction) -> Option<&mut Army> {
        self.armies.iter_mut().find(|a| a.faction == faction)
    }

    pub fn is_ai_faction(&self, faction: Faction) -> bool {
        self.army(faction).is_some_and(|a| a.ai)
    }

    /// More than one person plays (they take turns on one device)
    pub fn hot_seat(&self) -> bool {
        self.armies.iter().filter(|a| !a.ai).count() > 1
    }

    /// Same army or same team
    pub fn allied(&self, a: Faction, b: Faction) -> bool {
        a == b || matches!((self.army(a), self.army(b)), (Some(x), Some(y)) if x.team == y.team)
    }

    /// Armies still in the battle
    pub fn alive(&self) -> impl Iterator<Item = &Army> {
        self.armies.iter().filter(|a| !a.defeated)
    }

    /// Armies still in the battle that fight this one
    pub fn enemies_of(&self, faction: Faction) -> Vec<Faction> {
        self.alive().filter(|a| !self.allied(a.faction, faction)).map(|a| a.faction).collect()
    }

    /// The first enemy still standing (for screens that show one opponent)
    pub fn rival(&self, faction: Faction) -> Faction {
        self.enemies_of(faction).first().copied()
            .or_else(|| self.armies.iter().map(|a| a.faction).find(|&f| f != faction))
            .unwrap_or(faction)
    }

    /// Whether this army's art faces left (the armies on the right of the map)
    pub fn facing_left(&self, faction: Faction) -> bool {
        self.army(faction).is_some_and(|a| a.facing_left)
    }

    /// The army that plays after this one, skipping defeated armies
    pub fn next_faction(&self, current: Faction) -> Faction {
        let n = self.armies.len();
        let at = self.armies.iter().position(|a| a.faction == current).unwrap_or(0);
        (1..=n).map(|step| &self.armies[(at + step) % n]).find(|a| !a.defeated).map_or(current, |a| a.faction)
    }

    /// "Day 12" or "Day 12 / 30" with a day limit
    pub fn day_label(&self, day: u32) -> String {
        match self.day_limit {
            Some(limit) => format!("Day {day} / {limit}"),
            None => format!("Day {day}"),
        }
    }

    /// Whose eyes the board is seen through on this army's turn: its own when a
    /// person plays it, otherwise the person who played last, so a computer
    /// turn never shows anyone what another player sees (fog of war, hidden HP)
    pub fn viewer(&self, current: Faction) -> Faction {
        if !self.is_ai_faction(current) {
            return current;
        }
        let n = self.armies.len();
        let at = self.armies.iter().position(|a| a.faction == current).unwrap_or(0);
        (1..n).map(|back| &self.armies[(at + n - back) % n])
            .find(|a| !a.ai && !a.defeated)
            .map_or(self.player_faction, |a| a.faction)
    }
}

/// Switch to the next faction's turn: the day advances when it comes back to
/// the army that moves first. Callers should reset units before calling this.
pub fn advance_turn(
    turn_state: &mut TurnState,
    battle_config: &BattleConfig,
    highlights: &mut MovementHighlights,
) {
    let next = battle_config.next_faction(turn_state.current_faction);
    // A new day starts when play wraps around to the front of the turn order
    let index = |f: Faction| battle_config.armies.iter().position(|a| a.faction == f).unwrap_or(0);
    if index(next) <= index(turn_state.current_faction) {
        turn_state.turn_number += 1;
    }
    turn_state.current_faction = next;
    turn_state.phase = TurnPhase::Select;

    // Clear selection
    highlights.selected_unit = None;
    highlights.tiles.clear();
    highlights.attack_targets.clear();
}

/// Ask to end the current army's turn (from the map menu, the HUD button or the AI)
#[derive(Message)]
pub struct EndTurnRequest;

/// The one place a turn ends: the army's units are ready again for its next
/// turn, play passes to the other army, and its turn starts
pub fn pass_turn(
    mut requests: MessageReader<EndTurnRequest>,
    mut turn_state: ResMut<TurnState>,
    mut highlights: ResMut<MovementHighlights>,
    battle_config: Res<BattleConfig>,
    game_result: Res<GameResult>,
    mut units: Query<(&mut Unit, &FactionMember)>,
    mut turn_start: MessageWriter<TurnStartEvent>,
) {
    // Several requests in one frame still end only one turn
    if requests.read().count() == 0 || game_result.game_over {
        return;
    }
    for (mut unit, faction) in units.iter_mut() {
        if faction.faction == turn_state.current_faction {
            unit.moved = false;
            unit.attacked = false;
            unit.exhausted = false;
        }
    }
    let ended = turn_state.current_faction;
    advance_turn(&mut turn_state, &battle_config, &mut highlights);
    info!("{:?} ended its turn; day {}, {:?} to move", ended, turn_state.turn_number, turn_state.current_faction);
    turn_start.write(TurnStartEvent { faction: turn_state.current_faction });
}

/// How long the match intro (map name and the two commanders) plays, in seconds
pub const MATCH_INTRO_SECS: f32 = 2.8;
/// How long the "Day N" banner plays at the start of every turn, in seconds
pub const DAY_BANNER_SECS: f32 = 1.6;

/// Advance Wars style turn presentation: the match opens with the map and the
/// two commanders, and every turn opens with a "Day N" banner in the army's
/// colour. While it plays, the board ignores input and the AI waits.
#[derive(Resource, Default)]
pub struct TurnBanner {
    pub faction: Option<Faction>,
    pub day: u32,
    /// Seconds since the banner started
    pub elapsed: f32,
    /// Whether the match intro plays before the day banner (the battle's first turn)
    pub intro: bool,
}

impl TurnBanner {
    pub fn total(&self) -> f32 {
        if self.intro { MATCH_INTRO_SECS + DAY_BANNER_SECS } else { DAY_BANNER_SECS }
    }

    pub fn playing(&self) -> bool {
        self.faction.is_some() && self.elapsed < self.total()
    }
}

/// Hot seat with fog of war: before a person's turn the board stays covered
/// until they say they have the device, so nobody sees another's fog
#[derive(Resource, Default)]
pub struct HandOver {
    pub faction: Option<Faction>,
}

fn start_turn_banner(
    mut events: MessageReader<TurnStartEvent>,
    mut banner: ResMut<TurnBanner>,
    mut hand_over: ResMut<HandOver>,
    fog: Res<super::FogOfWar>,
    turn_state: Res<TurnState>,
    battle_config: Res<BattleConfig>,
    tiles: Query<&Tile>,
    units: Query<(&GridPosition, &FactionMember), With<Unit>>,
    mut cursor: ResMut<super::GridCursor>,
) {
    let Some(event) = events.read().last() else { return };
    let first = turn_state.turn_number == 1 && battle_config.armies.first().is_some_and(|a| a.faction == event.faction);
    *banner = TurnBanner { faction: Some(event.faction), day: turn_state.turn_number, elapsed: 0.0, intro: first };
    hand_over.faction = (battle_config.hot_seat() && fog.enabled && !battle_config.is_ai_faction(event.faction))
        .then_some(event.faction);
    // The cursor (and so the camera) goes to the army's HQ, else one of its units
    let hq = tiles.iter()
        .find(|t| t.owner == Some(event.faction) && t.terrain == Terrain::Base)
        .map(|t| (t.position.x, t.position.y));
    let unit = units.iter().find(|(_, f)| f.faction == event.faction).map(|(p, _)| (p.x, p.y));
    if let Some((x, y)) = hq.or(unit) {
        cursor.x = x;
        cursor.y = y;
        cursor.follow = true;
    }
}

fn tick_turn_banner(time: Res<Time>, mut banner: ResMut<TurnBanner>, hand_over: Res<HandOver>) {
    // The banner waits behind the hot-seat cover
    if banner.playing() && hand_over.faction.is_none() {
        banner.elapsed += time.delta_secs();
    }
}

/// Result of the game - tracks win/lose state
#[derive(Resource, Default)]
pub struct GameResult {
    pub game_over: bool,
    pub winner: Option<Faction>,
    pub victory_type: VictoryType,
    /// Armies that have had units on the board this battle. Only they can be
    /// routed, so maps that start empty (like AW's Spann Island) don't end at
    /// the first unit built.
    pub fielded: Vec<Faction>,
    /// Armies that have held an HQ this battle (and are out once they hold none)
    pub had_hq: Vec<Faction>,
    /// Where each army's HQ stands, to know who took it
    pub hq_positions: Vec<(Faction, IVec2)>,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VictoryType {
    #[default]
    None,
    Elimination,   // All enemy units destroyed
    HQCapture,     // Enemy HQ (base) captured
    DayLimit,      // Held the most properties when the day limit ran out
}

/// Event fired when a faction's turn starts
#[derive(Message)]
pub struct TurnStartEvent {
    pub faction: Faction,
}

/// Tracks funds for each faction
#[derive(Resource, Default)]
pub struct FactionFunds {
    funds: HashMap<Faction, u32>,
}

impl FactionFunds {
    pub fn get(&self, faction: Faction) -> u32 {
        *self.funds.get(&faction).unwrap_or(&0)
    }

    pub fn add(&mut self, faction: Faction, amount: u32) {
        *self.funds.entry(faction).or_insert(0) += amount;
    }

    pub fn spend(&mut self, faction: Faction, amount: u32) -> bool {
        let current = self.funds.entry(faction).or_insert(0);
        if *current >= amount {
            *current -= amount;
            true
        } else {
            false
        }
    }
}

/// Tracks turn state
#[derive(Resource)]
pub struct TurnState {
    pub current_faction: Faction,
    pub turn_number: u32,
    pub phase: TurnPhase,
}

impl Default for TurnState {
    fn default() -> Self {
        Self {
            current_faction: Faction::Eastern,
            turn_number: 1,
            phase: TurnPhase::Select,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPhase {
    Select,     // Selecting a unit
    #[allow(dead_code)] // selection goes straight to Action for now
    Move,       // Unit selected, choosing where to move
    Action,     // Unit moved, choosing action (attack/wait)
    Animating,  // Playing animation
}

impl TurnState {
}

fn check_victory_condition(
    turn_state: Res<TurnState>,
    mut commands: Commands,
    units: Query<(Entity, &Unit, &FactionMember, &GridPosition)>,
    mut tiles: Query<&mut Tile>,
    mut game_result: ResMut<GameResult>,
    mut battle_config: ResMut<BattleConfig>,
    mut defeated_units: MessageWriter<super::UnitDefeated>,
) {
    if game_result.game_over || battle_config.armies.is_empty() {
        return;
    }

    // Armies that have had units, and that have held an HQ, this battle
    for (_, _, member, _) in units.iter() {
        if !game_result.fielded.contains(&member.faction) {
            game_result.fielded.push(member.faction);
        }
    }
    for tile in tiles.iter() {
        if let Some(owner) = tile.owner.filter(|_| tile.terrain == Terrain::Base) {
            if !game_result.had_hq.contains(&owner) {
                game_result.had_hq.push(owner);
                game_result.hq_positions.push((owner, tile.position));
            }
        }
    }

    // An army is out when it's routed (had units, has none) or lost its HQ
    let factions: Vec<Faction> = battle_config.alive().map(|a| a.faction).collect();
    for faction in factions {
        let units_left = units.iter().filter(|(_, _, m, _)| m.faction == faction).count();
        let routed = units_left == 0 && game_result.fielded.contains(&faction);
        let holds_hq = tiles.iter().any(|t| t.terrain == Terrain::Base && t.owner == Some(faction));
        let lost_hq = game_result.had_hq.contains(&faction) && !holds_hq;
        if !routed && !lost_hq {
            continue;
        }
        info!("{:?} is out of the battle ({})", faction, if routed { "routed" } else { "HQ captured" });
        if let Some(army) = battle_config.army_mut(faction) {
            army.defeated = true;
        }
        game_result.victory_type = if routed { VictoryType::Elimination } else { VictoryType::HQCapture };
        // Advance Wars: an army whose HQ falls leaves the field, and its
        // properties go to whoever took the HQ (or stand empty when routed)
        let conqueror = if lost_hq {
            let hq_spots: Vec<IVec2> = game_result.hq_positions.iter().filter(|(f, _)| *f == faction).map(|(_, p)| *p).collect();
            tiles.iter().find(|t| hq_spots.contains(&t.position)).and_then(|t| t.owner).filter(|&owner| owner != faction)
        } else {
            None
        };
        for mut tile in tiles.iter_mut() {
            if tile.owner == Some(faction) {
                tile.owner = conqueror;
            }
            if tile.capturing_faction == Some(faction) {
                tile.capturing_faction = None;
                tile.capture_progress = 0;
            }
        }
        for (entity, _, member, pos) in units.iter() {
            if member.faction == faction {
                defeated_units.write(super::UnitDefeated { position: *pos });
                commands.entity(entity).despawn();
            }
        }
    }

    // Day limit: once the last day is over, the team holding the most
    // properties wins (army value breaks a tie)
    if battle_config.day_limit.is_some_and(|limit| turn_state.turn_number > limit) {
        let mut best: Option<(u8, (usize, u32), Faction)> = None;
        for army in battle_config.alive() {
            let team: Vec<Faction> = battle_config.alive().filter(|a| a.team == army.team).map(|a| a.faction).collect();
            let properties = tiles.iter().filter(|t| t.owner.is_some_and(|o| team.contains(&o))).count();
            let value: u32 = units.iter().filter(|(_, _, m, _)| team.contains(&m.faction)).map(|(_, u, _, _)| u.hp.max(0) as u32).sum();
            if best.is_none_or(|(_, score, _)| (properties, value) > score) {
                best = Some((army.team, (properties, value), army.faction));
            }
        }
        game_result.game_over = true;
        game_result.winner = best.map(|(_, _, faction)| faction);
        game_result.victory_type = VictoryType::DayLimit;
        info!("Day limit reached: {:?} wins on properties", game_result.winner);
        return;
    }

    // The battle ends when only one team is left standing
    let mut teams: Vec<u8> = battle_config.alive().map(|a| a.team).collect();
    teams.sort();
    teams.dedup();
    if teams.len() <= 1 {
        game_result.game_over = true;
        game_result.winner = battle_config.alive().next().map(|a| a.faction);
        if game_result.victory_type == VictoryType::None {
            game_result.victory_type = VictoryType::Elimination;
        }
        info!("{:?} wins ({:?})", game_result.winner, game_result.victory_type);
    }
}

/// Generate income from owned properties when turn start event fires
fn generate_income(
    mut events: MessageReader<TurnStartEvent>,
    tiles: Query<&Tile>,
    mut funds: ResMut<FactionFunds>,
    commanders: Res<Commanders>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        // Each owned property pays its income, plus the CO's bonus per property (AW Sasha)
        let co_bonuses = commanders.get_bonuses(event.faction, &game_data);
        let income: u32 = tiles.iter()
            .filter(|t| t.owner == Some(event.faction))
            .map(|t| (game_data.terrain_income(t.terrain) as i32 + co_bonuses.mods.income).max(0) as u32)
            .sum();
        if income > 0 {
            funds.add(event.faction, income);
            info!("{:?} receives {} income (total: {})", event.faction, income, funds.get(event.faction));
        }
    }
}

/// Advance Wars repair: 2 displayed HP (20 internal, capped at full), for 10%
/// of the unit's cost per displayed HP gained, in whole HP only; with too
/// little money it heals 1 HP, or none. Returns (new hp, cost).
pub fn repair(hp: i32, max_hp: i32, unit_cost: u32, funds: u32, extra: i32) -> (i32, u32) {
    let shown = |hp: i32| ((hp as f32) / 10.0).ceil() as i32;
    for steps in (1..=2 + extra.max(0)).rev() {
        let healed = (hp + steps * 10).min(max_hp);
        let gained = (shown(healed) - shown(hp)).max(0) as u32;
        let cost = gained * unit_cost / 10;
        if healed > hp && cost <= funds {
            return (healed, cost);
        }
    }
    (hp, 0)
}

/// Start-of-turn upkeep for the army whose turn begins, in Advance Wars order
/// (after income): units on their own properties that serve them
/// (`resupplies` in terrain.ron) are repaired and refilled, supply units refill
/// their neighbors, then fliers and boats that weren't refilled burn their
/// daily stamina and crash or sink at 0
fn resupply_units(
    mut events: MessageReader<TurnStartEvent>,
    mut commands: Commands,
    mut units: Query<(Entity, &mut Unit, &GridPosition, &FactionMember)>,
    tiles: Query<&Tile>,
    mut funds: ResMut<FactionFunds>,
    mut defeated: MessageWriter<super::UnitDefeated>,
    commanders: Res<Commanders>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        let co = commanders.get_bonuses(event.faction, &game_data);
        let properties: HashMap<(i32, i32), Terrain> = tiles.iter()
            .filter(|t| t.owner == Some(event.faction))
            .map(|t| ((t.position.x, t.position.y), t.terrain))
            .collect();
        let suppliers: Vec<(i32, i32)> = units.iter()
            .filter(|(_, u, _, f)| f.faction == event.faction && u.unit_type == UnitType::Supplier)
            .map(|(_, _, p, _)| (p.x, p.y))
            .collect();

        for (entity, mut unit, pos, faction) in units.iter_mut() {
            if faction.faction != event.faction {
                continue;
            }
            // A paralyzing power (AW Von Bolt) costs the unit this turn
            if unit.paralyzed {
                unit.paralyzed = false;
                unit.exhausted = true;
            }
            let stats = game_data.unit_stats(unit.unit_type);
            let on_property = properties.get(&(pos.x, pos.y))
                .is_some_and(|&terrain| game_data.terrain_resupplies(terrain, unit.unit_type));
            let beside_supplier = suppliers.iter()
                .any(|&(x, y)| (x - pos.x).abs() + (y - pos.y).abs() == 1);

            if on_property {
                let (hp, cost) = repair(unit.hp, stats.max_hp, stats.cost, funds.get(event.faction), co.mods.repair);
                if hp > unit.hp {
                    funds.spend(event.faction, cost);
                    info!("{} repaired: HP {} -> {} (cost {})", game_data.unit_name(unit.unit_type), unit.hp, hp, cost);
                    unit.hp = hp;
                }
            }
            if on_property || beside_supplier {
                unit.stamina = stats.max_stamina;
                unit.ammo = stats.max_ammo;
                continue;
            }

            // A submerged diver burns its dive rate instead (AW Sub: 5 a day)
            let base = if unit.submerged { stats.dive_stamina } else { stats.daily_stamina };
            let daily = (base as i32 + co.daily_stamina(unit.unit_type, &game_data)).max(0) as u32;
            if stats.daily_stamina > 0 {
                unit.stamina = unit.stamina.saturating_sub(daily);
                if unit.stamina == 0 {
                    info!("{} ran out of stamina and was lost", game_data.unit_name(unit.unit_type));
                    defeated.write(super::UnitDefeated { position: *pos });
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn four_armies(teams: bool) -> BattleConfig {
        let factions = [Faction::Western, Faction::Northern, Faction::Southern, Faction::Eastern];
        BattleConfig::from_armies(factions.iter().enumerate().map(|(i, &faction)| Army {
            faction, ai: i == 1 || i == 3, team: if teams { (i % 2) as u8 } else { i as u8 }, defeated: false, facing_left: i % 2 == 1,
        }).collect())
    }

    #[test]
    fn turns_go_round_every_army_and_skip_the_defeated() {
        let mut config = four_armies(false);
        assert_eq!(config.next_faction(Faction::Western), Faction::Northern);
        assert_eq!(config.next_faction(Faction::Eastern), Faction::Western, "wraps to the first army");
        config.army_mut(Faction::Northern).unwrap().defeated = true;
        assert_eq!(config.next_faction(Faction::Western), Faction::Southern);
        assert_eq!(config.enemies_of(Faction::Western), vec![Faction::Southern, Faction::Eastern]);
    }

    #[test]
    fn teams_are_allies_and_computer_turns_show_the_last_person() {
        let config = four_armies(true);
        assert!(config.allied(Faction::Western, Faction::Southern));
        assert!(!config.allied(Faction::Western, Faction::Northern));
        assert!(config.hot_seat());
        // The computer's turns show what the person who just played sees
        assert_eq!(config.viewer(Faction::Northern), Faction::Western);
        assert_eq!(config.viewer(Faction::Eastern), Faction::Southern);
        assert_eq!(config.viewer(Faction::Southern), Faction::Southern);
    }

    #[test]
    fn when_the_day_limit_runs_out_the_most_properties_win() {
        let data = GameData::load_defaults();
        let mut config = BattleConfig::new(Faction::Western, Faction::Southern);
        config.day_limit = Some(5);
        let mut app = App::new();
        app.add_message::<super::super::UnitDefeated>()
            .insert_resource(config)
            .insert_resource(TurnState { current_faction: Faction::Western, turn_number: 5, phase: TurnPhase::Select })
            .init_resource::<GameResult>()
            .add_systems(Update, check_victory_condition);
        let tile = |x: i32, owner: Faction, terrain: Terrain| Tile {
            terrain, position: IVec2::new(x, 0), owner: Some(owner), capture_progress: 0, capturing_faction: None,
        };
        for (x, owner, terrain) in [(0, Faction::Western, Terrain::Base), (1, Faction::Western, Terrain::Outpost),
            (2, Faction::Western, Terrain::Outpost), (3, Faction::Southern, Terrain::Base)] {
            app.world_mut().spawn(tile(x, owner, terrain));
        }
        for (x, faction) in [(0, Faction::Western), (3, Faction::Southern)] {
            app.world_mut().spawn((Unit::new(UnitType::Scout, &data), FactionMember { faction }, GridPosition::new(x, 1)));
        }
        // Still within the limit: play on
        app.update();
        assert!(!app.world().resource::<GameResult>().game_over);
        // The last day is over
        app.world_mut().resource_mut::<TurnState>().turn_number = 6;
        app.update();
        let result = app.world().resource::<GameResult>();
        assert!(result.game_over);
        assert_eq!((result.winner, result.victory_type), (Some(Faction::Western), VictoryType::DayLimit));
    }

    #[test]
    fn a_turn_ends_once_per_request_and_the_day_turns_over() {
        let data = GameData::load_defaults();
        let mut app = App::new();
        app.add_message::<EndTurnRequest>()
            .add_message::<TurnStartEvent>()
            .insert_resource(BattleConfig::new(Faction::Western, Faction::Southern))
            .insert_resource(TurnState { current_faction: Faction::Western, turn_number: 1, phase: TurnPhase::Select })
            .init_resource::<MovementHighlights>()
            .init_resource::<GameResult>()
            .add_systems(Update, pass_turn);
        let mut unit = Unit::new(UnitType::Scout, &data);
        unit.moved = true;
        unit.exhausted = true;
        let scout = app.world_mut().spawn((unit, FactionMember { faction: Faction::Western })).id();

        // Two requests in one frame (say the button and the map menu) end one turn
        app.world_mut().write_message(EndTurnRequest);
        app.world_mut().write_message(EndTurnRequest);
        app.update();
        let turn = app.world().resource::<TurnState>();
        assert_eq!((turn.current_faction, turn.turn_number), (Faction::Southern, 1));
        let unit = app.world().get::<Unit>(scout).unwrap();
        assert!(!unit.moved && !unit.exhausted, "the army's units are ready for its next turn");

        app.world_mut().write_message(EndTurnRequest);
        app.update();
        let turn = app.world().resource::<TurnState>();
        assert_eq!((turn.current_faction, turn.turn_number), (Faction::Western, 2));
        assert_eq!(app.world().resource::<Messages<TurnStartEvent>>().len(), 2);
    }

    #[test]
    fn repair_is_two_hp_for_ten_percent_each() {
        // Infantry (1000) at 6 HP heals to 8 for 200
        assert_eq!(repair(60, 100, 1000, 5000, 0), (80, 200));
        // 9 HP (85 internal) tops up to full for one HP's worth
        assert_eq!(repair(85, 100, 7000, 5000, 0), (100, 700));
        // Too little money for two HP heals one, for none heals nothing
        assert_eq!(repair(50, 100, 7000, 1000, 0), (60, 700));
        assert_eq!(repair(50, 100, 7000, 600, 0), (50, 0));
        // Already full: no change, no charge
        assert_eq!(repair(100, 100, 7000, 5000, 0), (100, 0));
        // AW Rachel repairs one more
        assert_eq!(repair(50, 100, 1000, 5000, 1), (80, 300));
    }

    #[test]
    fn funds_start_empty_and_refuse_overspending() {
        let mut funds = FactionFunds::default();
        assert_eq!(funds.get(Faction::Western), 0);
        funds.add(Faction::Western, 1000);
        assert!(!funds.spend(Faction::Western, 1001));
        assert_eq!(funds.get(Faction::Western), 1000);
        assert!(funds.spend(Faction::Western, 1000));
        assert_eq!(funds.get(Faction::Western), 0);
    }
}
