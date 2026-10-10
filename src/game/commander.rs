use bevy::prelude::*;
use std::collections::HashMap;

use super::{
    Faction, UnitType, Unit, FactionMember, GridPosition,
    FactionFunds, spawn_unit, FogOfWar, GameMap, Tile, Terrain, SpriteAssets,
    BattleConfig, GameData, CommanderData,
};

pub struct CommanderPlugin;

impl Plugin for CommanderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Commanders>()
            .add_message::<PowerActivatedEvent>()
            .add_systems(Update, (
                clear_power_at_turn_end,
                apply_power_effects,
            ));
    }
}

// ============================================================================
// COMMANDER DEFINITIONS
// ============================================================================

/// Unique identifier for each CO
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CommanderId {
    // Eastern Empire
    Kira,
    Tanuki,
    Sensei,
    // Northern Realm
    Grimjaw,
    Frost,
    Bjorn,
    // Western Frontier
    Bandit,
    Talon,
    Dusty,
    // Southern Pride
    Lionheart,
    Tusker,
    Prowler,
    // Nether Dominion
    Burrower,
    Hivemind,
    Dredge,
}

impl CommanderId {
    pub fn all() -> &'static [CommanderId] {
        &[
            CommanderId::Kira, CommanderId::Tanuki, CommanderId::Sensei,
            CommanderId::Grimjaw, CommanderId::Frost, CommanderId::Bjorn,
            CommanderId::Bandit, CommanderId::Talon, CommanderId::Dusty,
            CommanderId::Lionheart, CommanderId::Tusker, CommanderId::Prowler,
            CommanderId::Burrower, CommanderId::Hivemind, CommanderId::Dredge,
        ]
    }

    /// Get all COs belonging to a faction (as defined in commanders.ron)
    pub fn for_faction(faction: Faction, game_data: &GameData) -> Vec<CommanderId> {
        Self::all()
            .iter()
            .copied()
            .filter(|&id| game_data.commander(id).faction == faction)
            .collect()
    }
}

/// Effects that CO powers can have
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum PowerEffect {
    /// Boost attack/defense/movement for this turn
    StatBoost {
        attack: f32,
        defense: f32,
        movement: i32,
    },
    /// Gain bonus funds (multiplier of current funds)
    BonusFunds {
        multiplier: f32,
    },
    /// Reveal map and boost attack
    RevealAndBoost {
        attack_boost: f32,
    },
    /// Defense boost and heal all units (heal is in HP, 10 = 1 displayed HP)
    DefenseAndHeal {
        defense: f32,
        heal: i32,
    },
    /// Spawn free units at all bases
    FreeUnits {
        unit_type: UnitType,
    },
    /// Allow all units to move again
    ExtraMove,
    /// Steal funds from enemy and boost attack
    StealFunds {
        steal_percent: f32,
        attack_boost: f32,
    },
    /// Ignore terrain movement costs this turn
    IgnoreTerrain,
}

// ============================================================================
// ACTIVE COMMANDER STATE
// ============================================================================

/// Resource tracking active commanders and power meters
#[derive(Resource)]
pub struct Commanders {
    /// Active CO per faction
    pub active: HashMap<Faction, CommanderId>,
    /// Power meter per faction (0 to power_cost)
    pub power_meter: HashMap<Faction, u32>,
    /// Whether CO power is active this turn (for stat boosts)
    pub power_active: HashMap<Faction, bool>,
    /// Cached power effect when activated (for systems to read)
    pub active_effect: HashMap<Faction, PowerEffect>,
}

impl Default for Commanders {
    fn default() -> Self {
        let mut active = HashMap::new();
        let mut power_meter = HashMap::new();
        let mut power_active = HashMap::new();

        // Default COs for each faction
        active.insert(Faction::Eastern, CommanderId::Kira);
        active.insert(Faction::Northern, CommanderId::Grimjaw);
        active.insert(Faction::Western, CommanderId::Bandit);
        active.insert(Faction::Southern, CommanderId::Lionheart);
        active.insert(Faction::Nether, CommanderId::Burrower);

        // Starting power (0)
        power_meter.insert(Faction::Eastern, 0);
        power_meter.insert(Faction::Northern, 0);
        power_meter.insert(Faction::Western, 0);
        power_meter.insert(Faction::Southern, 0);
        power_meter.insert(Faction::Nether, 0);

        // No powers active
        power_active.insert(Faction::Eastern, false);
        power_active.insert(Faction::Northern, false);
        power_active.insert(Faction::Western, false);
        power_active.insert(Faction::Southern, false);
        power_active.insert(Faction::Nether, false);

        Self {
            active,
            power_meter,
            power_active,
            active_effect: HashMap::new(),
        }
    }
}

impl Commanders {
    /// Set the active CO for a faction
    pub fn set_commander(&mut self, faction: Faction, co: CommanderId) {
        self.active.insert(faction, co);
        self.power_meter.insert(faction, 0);
        self.power_active.insert(faction, false);
    }

    /// Get the active commander ID for a faction
    pub fn get_active(&self, faction: Faction) -> CommanderId {
        self.active.get(&faction).copied().unwrap_or(CommanderId::Kira)
    }

    /// Get the active commander's data for a faction
    pub fn get_commander<'a>(&self, faction: Faction, game_data: &'a GameData) -> Option<&'a CommanderData> {
        self.active.get(&faction).map(|&id| game_data.commander(id))
    }

    /// Add charge to power meter (from dealing/taking damage)
    pub fn charge(&mut self, faction: Faction, amount: u32, game_data: &GameData) {
        let current = self.power_meter.get(&faction).copied().unwrap_or(0);
        if let Some(co) = self.get_commander(faction, game_data) {
            let new_value = (current + amount).min(co.power_cost);
            self.power_meter.insert(faction, new_value);
        }
    }

    /// Get current power charge
    pub fn get_charge(&self, faction: Faction) -> u32 {
        self.power_meter.get(&faction).copied().unwrap_or(0)
    }

    /// Get power cost for faction's CO
    pub fn get_power_cost(&self, faction: Faction, game_data: &GameData) -> u32 {
        self.get_commander(faction, game_data).map(|c| c.power_cost).unwrap_or(100)
    }

    /// Check if power can be activated
    pub fn can_activate(&self, faction: Faction, game_data: &GameData) -> bool {
        let charge = self.get_charge(faction);
        let cost = self.get_power_cost(faction, game_data);
        charge >= cost && !self.is_power_active(faction)
    }

    /// Check if power is currently active
    pub fn is_power_active(&self, faction: Faction) -> bool {
        self.power_active.get(&faction).copied().unwrap_or(false)
    }

    /// Activate CO power - returns the effect to apply
    pub fn activate_power(&mut self, faction: Faction, game_data: &GameData) -> Option<PowerEffect> {
        if !self.can_activate(faction, game_data) {
            return None;
        }

        if let Some(co) = self.get_commander(faction, game_data) {
            self.power_meter.insert(faction, 0);
            self.power_active.insert(faction, true);
            self.active_effect.insert(faction, co.power.effect.clone());
            info!("{} activated {}!", co.name, co.power.name);
            return Some(co.power.effect.clone());
        }
        None
    }

    /// Get the currently active power effect for a faction
    pub fn get_active_effect(&self, faction: Faction) -> Option<&PowerEffect> {
        if self.is_power_active(faction) {
            self.active_effect.get(&faction)
        } else {
            None
        }
    }

    /// Clear power active state (called at end of turn)
    pub fn clear_power(&mut self, faction: Faction) {
        self.power_active.insert(faction, false);
        self.active_effect.remove(&faction);
    }

    /// Get computed bonuses for a faction (passive + active power)
    pub fn get_bonuses(&self, faction: Faction, game_data: &GameData) -> CoBonuses {
        let co = match self.get_commander(faction, game_data) {
            Some(c) => c,
            None => return CoBonuses::none(),
        };

        let mut bonuses = CoBonuses {
            attack: co.attack_bonus,
            defense: co.defense_bonus,
            movement: co.movement_bonus,
            income: co.income_bonus,
            vision: co.vision_bonus,
            cost: co.cost_modifier,
        };

        // Apply active power bonuses if power is active
        if let Some(effect) = self.get_active_effect(faction) {
            match effect {
                PowerEffect::StatBoost { attack, defense, movement } => {
                    bonuses.attack *= attack;
                    bonuses.defense *= defense;
                    bonuses.movement += movement;
                }
                PowerEffect::RevealAndBoost { attack_boost } => {
                    bonuses.attack *= attack_boost;
                }
                PowerEffect::DefenseAndHeal { defense, .. } => {
                    bonuses.defense *= defense;
                }
                PowerEffect::StealFunds { attack_boost, .. } => {
                    bonuses.attack *= attack_boost;
                }
                _ => {}
            }
        }

        bonuses
    }
}

/// Computed bonuses from active CO (passive + power effects)
#[derive(Clone)]
pub struct CoBonuses {
    pub attack: f32,
    pub defense: f32,
    pub movement: i32,
    pub income: f32,
    pub vision: u32,
    pub cost: f32,
}

impl Default for CoBonuses {
    fn default() -> Self {
        Self::none()
    }
}

impl CoBonuses {
    /// Create default bonuses (no CO)
    pub fn none() -> Self {
        Self {
            attack: 1.0,
            defense: 1.0,
            movement: 0,
            income: 1.0,
            vision: 0,
            cost: 1.0,
        }
    }
}

// ============================================================================
// EVENTS
// ============================================================================

/// Event fired when a CO power is activated
#[derive(Message)]
pub struct PowerActivatedEvent {
    pub faction: Faction,
    pub effect: PowerEffect,
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Clear power active state at the end of each turn
fn clear_power_at_turn_end(
    mut commanders: ResMut<Commanders>,
    turn_state: Res<super::TurnState>,
    mut last_faction: Local<Option<Faction>>,
) {
    // Detect faction change (turn ended)
    let current = turn_state.current_faction;
    if let Some(last) = *last_faction {
        if last != current {
            // Turn changed - clear the previous faction's power
            commanders.clear_power(last);
        }
    }
    *last_faction = Some(current);
}

/// Apply CO power effects when activated
fn apply_power_effects(
    mut events: MessageReader<PowerActivatedEvent>,
    mut units: Query<(&mut Unit, &FactionMember, &GridPosition)>,
    mut funds: ResMut<FactionFunds>,
    mut fog: ResMut<FogOfWar>,
    mut commands: Commands,
    map: Res<GameMap>,
    tiles: Query<&Tile>,
    sprite_assets: Res<SpriteAssets>,
    images: Res<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    battle_config: Res<BattleConfig>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        info!("Applying power effect for {:?}", event.faction);

        match &event.effect {
            PowerEffect::StatBoost { attack: _, defense: _, movement: _ } => {
                // StatBoost is handled passively through get_bonuses()
                info!("StatBoost power active - bonuses will be applied automatically");
            }

            PowerEffect::BonusFunds { multiplier } => {
                // Add bonus funds based on current funds
                let current = funds.get(event.faction);
                let bonus = (current as f32 * multiplier).round() as u32;
                funds.add(event.faction, bonus);
                info!("{:?} gained {} bonus funds (Gold Rush)!", event.faction, bonus);
            }

            PowerEffect::RevealAndBoost { attack_boost: _ } => {
                // Reveal entire map by disabling fog temporarily
                // The attack boost is handled through get_bonuses()
                // For now, mark all tiles as explored
                for x in 0..map.width as i32 {
                    for y in 0..map.height as i32 {
                        fog.mark_explored(x, y);
                    }
                }
                info!("Fog Piercer activated - map revealed and attack boosted!");
            }

            PowerEffect::DefenseAndHeal { defense: _, heal } => {
                // Heal all units of this faction
                let heal_amount = *heal;
                for (mut unit, faction, _pos) in units.iter_mut() {
                    if faction.faction == event.faction {
                        let max_hp = game_data.unit_stats(unit.unit_type).max_hp;
                        let old_hp = unit.hp;
                        unit.hp = (unit.hp + heal_amount).min(max_hp);
                        if unit.hp > old_hp {
                            info!("Healed unit: {} -> {} HP", old_hp, unit.hp);
                        }
                    }
                }
                info!("Iron Wall activated - all units healed and defense boosted!");
            }

            PowerEffect::FreeUnits { unit_type } => {
                // Spawn free units at all owned bases without units
                let unit_positions: std::collections::HashSet<(i32, i32)> = units
                    .iter()
                    .map(|(_, _, pos)| (pos.x, pos.y))
                    .collect();

                for tile in tiles.iter() {
                    if tile.terrain == Terrain::Base
                        && tile.owner == Some(event.faction)
                        && !unit_positions.contains(&(tile.position.x, tile.position.y))
                    {
                        spawn_unit(
                            &mut commands,
                            &map,
                            &mut meshes,
                            &mut materials,
                            &sprite_assets,
                            &images,
                            &game_data,
                            event.faction,
                            *unit_type,
                            tile.position.x,
                            tile.position.y,
                        );
                        info!("Spawned free {:?} at ({}, {})", unit_type, tile.position.x, tile.position.y);
                    }
                }
                info!("Endless Horde activated - free units spawned!");
            }

            PowerEffect::ExtraMove => {
                // Reset moved/exhausted flags on all units of this faction
                for (mut unit, faction, _pos) in units.iter_mut() {
                    if faction.faction == event.faction {
                        unit.moved = false;
                        unit.exhausted = false;
                    }
                }
                info!("Charge! activated - all units can move again!");
            }

            PowerEffect::StealFunds { steal_percent, attack_boost: _ } => {
                // Determine enemy faction and steal their funds
                let enemy_faction = battle_config.next_faction(event.faction);

                let enemy_funds = funds.get(enemy_faction);
                let stolen = (enemy_funds as f32 * steal_percent).round() as u32;

                if stolen > 0 && funds.spend(enemy_faction, stolen) {
                    funds.add(event.faction, stolen);
                    info!("Heist! Stole {} funds from {:?}!", stolen, enemy_faction);
                } else {
                    info!("Heist attempted but enemy has no funds!");
                }
                // Attack boost is handled through get_bonuses()
            }

            PowerEffect::IgnoreTerrain => {
                // Terrain cost ignorance is handled in movement calculations
                // by checking if the faction's CO power is active
                info!("Undermine activated - all units ignore terrain movement costs!");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_commander_means_neutral_bonuses() {
        let data = GameData::load_defaults();
        let mut commanders = Commanders::default();
        commanders.active.clear();
        let bonuses = commanders.get_bonuses(Faction::Eastern, &data);
        assert_eq!((bonuses.attack, bonuses.defense, bonuses.cost, bonuses.income), (1.0, 1.0, 1.0, 1.0));
    }

    #[test]
    fn passive_bonuses_come_from_data() {
        let data = GameData::load_defaults();
        let mut commanders = Commanders::default();
        commanders.set_commander(Faction::Eastern, CommanderId::Kira);
        let bonuses = commanders.get_bonuses(Faction::Eastern, &data);
        assert_eq!(bonuses.attack, data.commander(CommanderId::Kira).attack_bonus);
    }

    #[test]
    fn power_charges_activates_and_stacks_with_passive() {
        let data = GameData::load_defaults();
        let mut commanders = Commanders::default();
        commanders.set_commander(Faction::Eastern, CommanderId::Kira);
        let cost = commanders.get_power_cost(Faction::Eastern, &data);

        commanders.charge(Faction::Eastern, cost - 1, &data);
        assert!(!commanders.can_activate(Faction::Eastern, &data));
        commanders.charge(Faction::Eastern, 1000, &data);
        assert_eq!(commanders.get_charge(Faction::Eastern), cost, "meter caps at power cost");

        let effect = commanders.activate_power(Faction::Eastern, &data).expect("power should activate");
        assert_eq!(commanders.get_charge(Faction::Eastern), 0);
        let PowerEffect::StatBoost { attack, .. } = effect else { panic!("Kira's Blitz is a StatBoost") };
        let kira = data.commander(CommanderId::Kira);
        let bonuses = commanders.get_bonuses(Faction::Eastern, &data);
        assert!((bonuses.attack - kira.attack_bonus * attack).abs() < 1e-6);

        commanders.clear_power(Faction::Eastern);
        assert_eq!(commanders.get_bonuses(Faction::Eastern, &data).attack, kira.attack_bonus);
    }
}
