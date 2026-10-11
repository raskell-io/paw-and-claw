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

/// Configuration for the current battle - tracks which faction is player vs AI
#[derive(Resource)]
pub struct BattleConfig {
    pub player_faction: Faction,
    pub ai_faction: Faction,
}

impl Default for BattleConfig {
    fn default() -> Self {
        Self {
            player_faction: Faction::Eastern,
            ai_faction: Faction::Northern,
        }
    }
}

impl BattleConfig {
    pub fn new(player_faction: Faction, ai_faction: Faction) -> Self {
        Self { player_faction, ai_faction }
    }

    pub fn is_ai_faction(&self, faction: Faction) -> bool {
        faction == self.ai_faction
    }

    /// Returns the faction that plays after the given one
    pub fn next_faction(&self, current: Faction) -> Faction {
        if current == self.player_faction {
            self.ai_faction
        } else {
            self.player_faction
        }
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
    if next == battle_config.player_faction {
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

fn start_turn_banner(
    mut events: MessageReader<TurnStartEvent>,
    mut banner: ResMut<TurnBanner>,
    turn_state: Res<TurnState>,
    battle_config: Res<BattleConfig>,
    tiles: Query<&Tile>,
    units: Query<(&GridPosition, &FactionMember), With<Unit>>,
    mut cursor: ResMut<super::GridCursor>,
) {
    let Some(event) = events.read().last() else { return };
    let first = turn_state.turn_number == 1 && event.faction == battle_config.player_faction;
    *banner = TurnBanner { faction: Some(event.faction), day: turn_state.turn_number, elapsed: 0.0, intro: first };
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

fn tick_turn_banner(time: Res<Time>, mut banner: ResMut<TurnBanner>) {
    if banner.playing() {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VictoryType {
    #[default]
    None,
    Elimination,   // All enemy units destroyed
    HQCapture,     // Enemy HQ (base) captured
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
    Move,       // Unit selected, choosing where to move
    Action,     // Unit moved, choosing action (attack/wait)
    Animating,  // Playing animation
}

impl TurnState {
}

fn check_victory_condition(
    units: Query<(&Unit, &FactionMember)>,
    tiles: Query<&Tile>,
    mut game_result: ResMut<GameResult>,
    battle_config: Res<BattleConfig>,
) {
    // Skip if game is already over
    if game_result.game_over {
        return;
    }

    let player = battle_config.player_faction;
    let ai = battle_config.ai_faction;

    // Count units per faction
    let mut player_units = 0;
    let mut ai_units = 0;

    for (_unit, faction) in units.iter() {
        if faction.faction == player {
            player_units += 1;
        } else if faction.faction == ai {
            ai_units += 1;
        }
        if !game_result.fielded.contains(&faction.faction) {
            game_result.fielded.push(faction.faction);
        }
    }
    let routed = |count: i32, faction: Faction| count == 0 && game_result.fielded.contains(&faction);

    // Check elimination victory: an army that had units and lost them all
    if routed(player_units, player) && ai_units > 0 {
        game_result.game_over = true;
        game_result.winner = Some(ai);
        game_result.victory_type = VictoryType::Elimination;
        info!("{:?} wins by elimination!", ai);
        return;
    } else if routed(ai_units, ai) && player_units > 0 {
        game_result.game_over = true;
        game_result.winner = Some(player);
        game_result.victory_type = VictoryType::Elimination;
        info!("{:?} wins by elimination!", player);
        return;
    }

    // Check HQ capture victory - count bases owned by each faction
    let player_bases = tiles.iter()
        .filter(|t| t.terrain == Terrain::Base && t.owner == Some(player))
        .count();
    let ai_bases = tiles.iter()
        .filter(|t| t.terrain == Terrain::Base && t.owner == Some(ai))
        .count();

    // If one faction owns all bases (2+), they win by HQ capture
    let total_bases = tiles.iter().filter(|t| t.terrain == Terrain::Base).count();

    if total_bases >= 2 {
        if player_bases == total_bases {
            game_result.game_over = true;
            game_result.winner = Some(player);
            game_result.victory_type = VictoryType::HQCapture;
            info!("{:?} wins by capturing all HQs!", player);
        } else if ai_bases == total_bases {
            game_result.game_over = true;
            game_result.winner = Some(ai);
            game_result.victory_type = VictoryType::HQCapture;
            info!("{:?} wins by capturing all HQs!", ai);
        }
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

            let daily = (stats.daily_stamina as i32 + co.daily_stamina(unit.unit_type, &game_data)).max(0) as u32;
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
