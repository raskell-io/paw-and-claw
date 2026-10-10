use bevy::prelude::*;
use std::collections::HashMap;

use super::{Faction, FactionMember, Unit, Tile, Terrain, Commanders, GridPosition, MovementHighlights, GameData};

pub struct TurnPlugin;

impl Plugin for TurnPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TurnState>()
            .init_resource::<FactionFunds>()
            .init_resource::<GameResult>()
            .init_resource::<BattleConfig>()
            .add_message::<TurnStartEvent>()
            .add_systems(Update, (check_victory_condition, generate_income, resupply_units));
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

/// Switch to the next faction's turn. Callers should reset units before calling this.
pub fn advance_turn(
    turn_state: &mut TurnState,
    battle_config: &BattleConfig,
    highlights: &mut MovementHighlights,
) {
    // Switch to next faction
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
        // Calculate base income from owned properties
        let base_income: u32 = tiles.iter()
            .filter(|t| t.owner == Some(event.faction))
            .map(|t| game_data.terrain_income(t.terrain))
            .sum();

        // Apply CO income bonus
        let co_bonuses = commanders.get_bonuses(event.faction, &game_data);
        let income = (base_income as f32 * co_bonuses.income).round() as u32;

        if income > 0 {
            funds.add(event.faction, income);
            if co_bonuses.income > 1.0 {
                info!("{:?} receives {} income (+{:.0}% CO bonus, total: {})",
                    event.faction, income, (co_bonuses.income - 1.0) * 100.0, funds.get(event.faction));
            } else {
                info!("{:?} receives {} income (total: {})",
                    event.faction, income, funds.get(event.faction));
            }
        }
    }
}

/// Resupply and heal units on friendly bases and storehouses at turn start
fn resupply_units(
    mut events: MessageReader<TurnStartEvent>,
    mut units: Query<(&mut Unit, &GridPosition, &FactionMember)>,
    tiles: Query<&Tile>,
    mut funds: ResMut<FactionFunds>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        // Build a map of supply buildings owned by this faction
        let supply_buildings: HashMap<(i32, i32), Terrain> = tiles.iter()
            .filter(|t| t.owner == Some(event.faction))
            .filter(|t| matches!(t.terrain, Terrain::Base | Terrain::Workshop | Terrain::Storehouse))
            .map(|t| ((t.position.x, t.position.y), t.terrain))
            .collect();

        // Resupply and heal units on these buildings
        for (mut unit, pos, faction) in units.iter_mut() {
            // Only resupply units of the active faction
            if faction.faction != event.faction {
                continue;
            }

            // Check if unit is on a supply building
            if let Some(terrain) = supply_buildings.get(&(pos.x, pos.y)) {
                let stats = game_data.unit_stats(unit.unit_type);
                let old_hp = unit.hp;
                let old_stamina = unit.stamina;
                let old_ammo = unit.ammo;

                // Restore stamina and ammo to max (free)
                unit.stamina = stats.max_stamina;
                unit.ammo = stats.max_ammo;

                // Heal HP (20% per turn, costs funds based on unit cost)
                // Healing rate: 20 HP per turn (like Advance Wars' 2 HP out of 10)
                let heal_amount = 20;
                let hp_needed = stats.max_hp - unit.hp;

                if hp_needed > 0 {
                    // Calculate healing cost: (heal_amount / max_hp) * unit_cost
                    let actual_heal = heal_amount.min(hp_needed);
                    let unit_cost = stats.cost;
                    let heal_cost = (actual_heal as u32 * unit_cost) / stats.max_hp as u32;

                    // Only heal if we can afford it (or heal as much as we can afford)
                    let available_funds = funds.get(event.faction);
                    if available_funds >= heal_cost {
                        unit.hp += actual_heal;
                        funds.spend(event.faction, heal_cost);
                        info!(
                            "{} healed at {:?}: HP {}->{} (cost: {})",
                            game_data.unit_name(unit.unit_type),
                            terrain,
                            old_hp,
                            unit.hp,
                            heal_cost
                        );
                    } else if available_funds > 0 {
                        // Partial healing based on available funds
                        let affordable_heal = (available_funds * stats.max_hp as u32) / unit_cost;
                        if affordable_heal > 0 {
                            let partial_heal = (affordable_heal as i32).min(hp_needed);
                            unit.hp += partial_heal;
                            let partial_cost = (partial_heal as u32 * unit_cost) / stats.max_hp as u32;
                            funds.spend(event.faction, partial_cost.max(1));
                            info!(
                                "{} partially healed at {:?}: HP {}->{} (cost: {})",
                                game_data.unit_name(unit.unit_type),
                                terrain,
                                old_hp,
                                unit.hp,
                                partial_cost
                            );
                        }
                    }
                }

                // Log resupply if anything was resupplied
                if old_stamina < stats.max_stamina || old_ammo < stats.max_ammo {
                    info!(
                        "{} resupplied at {:?}: stamina {}->{}, ammo {}->{}",
                        game_data.unit_name(unit.unit_type),
                        terrain,
                        old_stamina,
                        unit.stamina,
                        old_ammo,
                        unit.ammo
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
