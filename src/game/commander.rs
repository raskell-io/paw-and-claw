use bevy::prelude::*;
use std::collections::HashMap;

use super::{
    Faction, UnitType, AiPersonality, Unit, FactionMember, GridPosition,
    FactionFunds, spawn_unit, FogOfWar, GameMap, Tile, Terrain,
};

pub struct CommanderPlugin;

impl Plugin for CommanderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Commanders>()
            .add_event::<PowerActivatedEvent>()
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommanderId {
    // Eastern Empire
    Kira,
    Tanuki,
    Sensei,
    // Northern Realm
    Grimjaw,
    Frost,
    Bjorn,
}

impl CommanderId {
    /// Get the commander data for this ID
    pub fn data(&self) -> Commander {
        match self {
            // === EASTERN EMPIRE ===
            CommanderId::Kira => Commander {
                id: *self,
                name: "Kira",
                faction: Faction::Eastern,
                personality: AiPersonality::Aggressive,
                description: "A bold commander who leads from the front. Her aggressive tactics inspire troops to fight harder.",
                attack_bonus: 1.1,      // +10% attack
                defense_bonus: 1.0,
                movement_bonus: 0,
                income_bonus: 1.0,
                vision_bonus: 0,
                cost_modifier: 1.0,
                power: CoPower {
                    name: "Blitz",
                    description: "All units gain +1 movement and +20% attack this turn",
                    effect: PowerEffect::StatBoost {
                        attack: 1.2,
                        defense: 1.0,
                        movement: 1,
                    },
                },
                power_cost: 100,
            },
            CommanderId::Tanuki => Commander {
                id: *self,
                name: "Tanuki",
                faction: Faction::Eastern,
                personality: AiPersonality::Economic,
                description: "A shrewd merchant-commander who knows the value of gold. Properties under his control generate more income.",
                attack_bonus: 1.0,
                defense_bonus: 1.0,
                movement_bonus: 0,
                income_bonus: 1.15,     // +15% income
                vision_bonus: 0,
                cost_modifier: 1.0,
                power: CoPower {
                    name: "Gold Rush",
                    description: "Gain 50% of current funds instantly",
                    effect: PowerEffect::BonusFunds { multiplier: 0.5 },
                },
                power_cost: 120,
            },
            CommanderId::Sensei => Commander {
                id: *self,
                name: "Sensei",
                faction: Faction::Eastern,
                personality: AiPersonality::Aggressive, // Balanced, but defaults to aggressive
                description: "An ancient master whose wisdom reveals hidden truths. Units under his command see farther.",
                attack_bonus: 1.0,
                defense_bonus: 1.0,
                movement_bonus: 0,
                income_bonus: 1.0,
                vision_bonus: 1,        // +1 vision
                cost_modifier: 1.0,
                power: CoPower {
                    name: "Fog Piercer",
                    description: "Reveal entire map and gain +30% attack for 1 turn",
                    effect: PowerEffect::RevealAndBoost {
                        attack_boost: 1.3,
                    },
                },
                power_cost: 150,
            },

            // === NORTHERN REALM ===
            CommanderId::Grimjaw => Commander {
                id: *self,
                name: "Grimjaw",
                faction: Faction::Northern,
                personality: AiPersonality::Defensive,
                description: "A grizzled veteran who has never lost a defensive battle. His troops dig in like stone.",
                attack_bonus: 1.0,
                defense_bonus: 1.15,    // +15% defense
                movement_bonus: 0,
                income_bonus: 1.0,
                vision_bonus: 0,
                cost_modifier: 1.0,
                power: CoPower {
                    name: "Iron Wall",
                    description: "All units gain +40% defense and heal 2 HP",
                    effect: PowerEffect::DefenseAndHeal {
                        defense: 1.4,
                        heal: 20,
                    },
                },
                power_cost: 100,
            },
            CommanderId::Frost => Commander {
                id: *self,
                name: "Frost",
                faction: Faction::Northern,
                personality: AiPersonality::Swarm,
                description: "A commander who believes in strength through numbers. Units are cheaper to produce.",
                attack_bonus: 1.0,
                defense_bonus: 1.0,
                movement_bonus: 0,
                income_bonus: 1.0,
                vision_bonus: 0,
                cost_modifier: 0.9,     // -10% unit cost
                power: CoPower {
                    name: "Endless Horde",
                    description: "All bases produce a free Scout",
                    effect: PowerEffect::FreeUnits { unit_type: UnitType::Scout },
                },
                power_cost: 140,
            },
            CommanderId::Bjorn => Commander {
                id: *self,
                name: "Bjorn",
                faction: Faction::Northern,
                personality: AiPersonality::Aggressive,
                description: "A swift commander who strikes before the enemy can react. Ground units move faster.",
                attack_bonus: 1.0,
                defense_bonus: 1.0,
                movement_bonus: 1,      // +1 movement
                income_bonus: 1.0,
                vision_bonus: 0,
                cost_modifier: 1.0,
                power: CoPower {
                    name: "Charge!",
                    description: "All units can move again this turn",
                    effect: PowerEffect::ExtraMove,
                },
                power_cost: 180,
            },
        }
    }

    /// Alias for data() - get the commander
    pub fn get_commander(&self) -> Commander {
        self.data()
    }

    /// Get all COs for a faction
    pub fn for_faction(faction: Faction) -> Vec<CommanderId> {
        match faction {
            Faction::Eastern => vec![CommanderId::Kira, CommanderId::Tanuki, CommanderId::Sensei],
            Faction::Northern => vec![CommanderId::Grimjaw, CommanderId::Frost, CommanderId::Bjorn],
            _ => vec![], // Other factions not implemented yet
        }
    }
}

/// Full commander data
#[derive(Clone)]
pub struct Commander {
    pub id: CommanderId,
    pub name: &'static str,
    pub faction: Faction,
    pub personality: AiPersonality,
    pub description: &'static str,

    // Passive bonuses (always active)
    pub attack_bonus: f32,
    pub defense_bonus: f32,
    pub movement_bonus: i32,
    pub income_bonus: f32,
    pub vision_bonus: u32,
    pub cost_modifier: f32,

    // CO Power
    pub power: CoPower,
    pub power_cost: u32,
}

/// CO Power definition
#[derive(Clone)]
pub struct CoPower {
    pub name: &'static str,
    pub description: &'static str,
    pub effect: PowerEffect,
}

/// Effects that CO powers can have
#[derive(Clone)]
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
    /// Defense boost and heal all units
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

        // Default COs
        active.insert(Faction::Eastern, CommanderId::Kira);
        active.insert(Faction::Northern, CommanderId::Grimjaw);

        // Starting power (0)
        power_meter.insert(Faction::Eastern, 0);
        power_meter.insert(Faction::Northern, 0);

        // No powers active
        power_active.insert(Faction::Eastern, false);
        power_active.insert(Faction::Northern, false);

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

    /// Get the active commander for a faction
    pub fn get_commander(&self, faction: Faction) -> Option<Commander> {
        self.active.get(&faction).map(|id| id.data())
    }

    /// Add charge to power meter (from dealing/taking damage)
    pub fn charge(&mut self, faction: Faction, amount: u32) {
        let current = self.power_meter.get(&faction).copied().unwrap_or(0);
        if let Some(co) = self.get_commander(faction) {
            let new_value = (current + amount).min(co.power_cost);
            self.power_meter.insert(faction, new_value);
        }
    }

    /// Get current power charge
    pub fn get_charge(&self, faction: Faction) -> u32 {
        self.power_meter.get(&faction).copied().unwrap_or(0)
    }

    /// Get power cost for faction's CO
    pub fn get_power_cost(&self, faction: Faction) -> u32 {
        self.get_commander(faction).map(|c| c.power_cost).unwrap_or(100)
    }

    /// Check if power can be activated
    pub fn can_activate(&self, faction: Faction) -> bool {
        let charge = self.get_charge(faction);
        let cost = self.get_power_cost(faction);
        charge >= cost && !self.is_power_active(faction)
    }

    /// Check if power is currently active
    pub fn is_power_active(&self, faction: Faction) -> bool {
        self.power_active.get(&faction).copied().unwrap_or(false)
    }

    /// Activate CO power - returns the effect to apply
    pub fn activate_power(&mut self, faction: Faction) -> Option<PowerEffect> {
        if !self.can_activate(faction) {
            return None;
        }

        if let Some(co) = self.get_commander(faction) {
            self.power_meter.insert(faction, 0);
            self.power_active.insert(faction, true);
            self.active_effect.insert(faction, co.power.effect.clone());
            info!("{} activated {}!", co.name, co.power.name);
            return Some(co.power.effect);
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
    pub fn get_bonuses(&self, faction: Faction) -> CoBonuses {
        let co = match self.get_commander(faction) {
            Some(c) => c,
            None => return CoBonuses::default(),
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
                _ => {}
            }
        }

        bonuses
    }
}

/// Computed bonuses from active CO (passive + power effects)
#[derive(Default, Clone)]
pub struct CoBonuses {
    pub attack: f32,
    pub defense: f32,
    pub movement: i32,
    pub income: f32,
    pub vision: u32,
    pub cost: f32,
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
#[derive(Event)]
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
    mut events: EventReader<PowerActivatedEvent>,
    mut units: Query<(&mut Unit, &FactionMember, &GridPosition)>,
    mut funds: ResMut<FactionFunds>,
    mut fog: ResMut<FogOfWar>,
    mut commands: Commands,
    map: Res<GameMap>,
    tiles: Query<&Tile>,
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
                        let max_hp = unit.unit_type.stats().max_hp;
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
                // Reset moved flag on all units of this faction
                for (mut unit, faction, _pos) in units.iter_mut() {
                    if faction.faction == event.faction {
                        unit.moved = false;
                    }
                }
                info!("Charge! activated - all units can move again!");
            }
        }
    }
}
