use bevy::prelude::*;
use std::collections::HashMap;

use super::{Faction, FactionMember, Unit, Tile, Terrain, Commanders, GridPosition};

pub struct TurnPlugin;

impl Plugin for TurnPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TurnState>()
            .init_resource::<FactionFunds>()
            .init_resource::<GameResult>()
            .add_event::<TurnStartEvent>()
            .add_systems(Update, (check_victory_condition, generate_income, resupply_units));
    }
}

/// Result of the game - tracks win/lose state
#[derive(Resource, Default)]
pub struct GameResult {
    pub game_over: bool,
    pub winner: Option<Faction>,
    pub victory_type: VictoryType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VictoryType {
    #[default]
    None,
    Elimination,   // All enemy units destroyed
    HQCapture,     // Enemy HQ (base) captured
}

/// Event fired when a faction's turn starts
#[derive(Event)]
pub struct TurnStartEvent {
    pub faction: Faction,
    #[allow(dead_code)]
    pub income: u32,
}

/// Tracks funds for each faction
#[derive(Resource)]
pub struct FactionFunds {
    funds: HashMap<Faction, u32>,
}

impl Default for FactionFunds {
    fn default() -> Self {
        let mut funds = HashMap::new();
        funds.insert(Faction::Eastern, 100);
        funds.insert(Faction::Northern, 100);
        funds.insert(Faction::Western, 0);
        funds.insert(Faction::Southern, 0);
        funds.insert(Faction::Wanderer, 0);
        Self { funds }
    }
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
#[allow(dead_code)]
pub enum TurnPhase {
    Select,     // Selecting a unit
    Move,       // Unit selected, choosing where to move
    Action,     // Unit moved, choosing action (attack/wait)
    Animating,  // Playing animation
}

impl TurnState {
    #[allow(dead_code)]
    pub fn end_turn<'a>(&mut self, units: impl Iterator<Item = &'a mut Unit>) {
        // Reset all units of current faction
        for unit in units {
            unit.reset_turn();
        }

        // Switch to other faction (simple 2-player for now)
        self.current_faction = match self.current_faction {
            Faction::Eastern => Faction::Northern,
            Faction::Northern => {
                self.turn_number += 1;
                Faction::Eastern
            }
            _ => Faction::Eastern,
        };

        self.phase = TurnPhase::Select;
    }
}

fn check_victory_condition(
    units: Query<(&Unit, &FactionMember)>,
    tiles: Query<&Tile>,
    mut game_result: ResMut<GameResult>,
) {
    // Skip if game is already over
    if game_result.game_over {
        return;
    }

    // Count units per faction
    let mut eastern_units = 0;
    let mut northern_units = 0;

    for (_unit, faction) in units.iter() {
        match faction.faction {
            Faction::Eastern => eastern_units += 1,
            Faction::Northern => northern_units += 1,
            _ => {}
        }
    }

    // Check elimination victory
    if eastern_units == 0 && northern_units > 0 {
        game_result.game_over = true;
        game_result.winner = Some(Faction::Northern);
        game_result.victory_type = VictoryType::Elimination;
        info!("Northern Realm wins by elimination!");
        return;
    } else if northern_units == 0 && eastern_units > 0 {
        game_result.game_over = true;
        game_result.winner = Some(Faction::Eastern);
        game_result.victory_type = VictoryType::Elimination;
        info!("Eastern Empire wins by elimination!");
        return;
    }

    // Check HQ capture victory - count bases owned by each faction
    let eastern_bases = tiles.iter()
        .filter(|t| t.terrain == Terrain::Base && t.owner == Some(Faction::Eastern))
        .count();
    let northern_bases = tiles.iter()
        .filter(|t| t.terrain == Terrain::Base && t.owner == Some(Faction::Northern))
        .count();

    // If one faction owns all bases (2+), they win by HQ capture
    let total_bases = tiles.iter().filter(|t| t.terrain == Terrain::Base).count();

    if total_bases >= 2 {
        if eastern_bases == total_bases {
            game_result.game_over = true;
            game_result.winner = Some(Faction::Eastern);
            game_result.victory_type = VictoryType::HQCapture;
            info!("Eastern Empire wins by capturing all HQs!");
        } else if northern_bases == total_bases {
            game_result.game_over = true;
            game_result.winner = Some(Faction::Northern);
            game_result.victory_type = VictoryType::HQCapture;
            info!("Northern Realm wins by capturing all HQs!");
        }
    }
}

/// Generate income from owned properties when turn start event fires
fn generate_income(
    mut events: EventReader<TurnStartEvent>,
    tiles: Query<&Tile>,
    mut funds: ResMut<FactionFunds>,
    commanders: Res<Commanders>,
) {
    for event in events.read() {
        // Calculate base income from owned properties
        let base_income: u32 = tiles.iter()
            .filter(|t| t.owner == Some(event.faction))
            .map(|t| t.terrain.income_value())
            .sum();

        // Apply CO income bonus
        let co_bonuses = commanders.get_bonuses(event.faction);
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

/// Resupply units on friendly bases and storehouses at turn start
fn resupply_units(
    mut events: EventReader<TurnStartEvent>,
    mut units: Query<(&mut Unit, &GridPosition, &FactionMember)>,
    tiles: Query<&Tile>,
) {
    for event in events.read() {
        // Build a map of supply buildings owned by this faction
        let supply_buildings: HashMap<(i32, i32), Terrain> = tiles.iter()
            .filter(|t| t.owner == Some(event.faction))
            .filter(|t| matches!(t.terrain, Terrain::Base | Terrain::Storehouse))
            .map(|t| ((t.position.x, t.position.y), t.terrain))
            .collect();

        // Resupply units on these buildings
        for (mut unit, pos, faction) in units.iter_mut() {
            // Only resupply units of the active faction
            if faction.faction != event.faction {
                continue;
            }

            // Check if unit is on a supply building
            if let Some(terrain) = supply_buildings.get(&(pos.x, pos.y)) {
                let stats = unit.unit_type.stats();
                let old_stamina = unit.stamina;
                let old_ammo = unit.ammo;

                // Restore stamina and ammo to max
                unit.stamina = stats.max_stamina;
                unit.ammo = stats.max_ammo;

                // Log if anything was resupplied
                if old_stamina < stats.max_stamina || old_ammo < stats.max_ammo {
                    info!(
                        "{} resupplied at {:?}: stamina {}->{}, ammo {}->{}",
                        unit.unit_type.name(),
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
