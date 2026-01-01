use bevy::prelude::*;

use super::{GameMap, GridPosition, Unit, FactionMember, Terrain};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<AttackEvent>()
            .add_systems(Update, process_attacks);
    }
}

/// Event fired when a unit attacks another
#[derive(Event)]
pub struct AttackEvent {
    pub attacker: Entity,
    pub defender: Entity,
}

/// Calculate damage based on Advance Wars-like formula
pub fn calculate_damage(
    attacker: &Unit,
    defender: &Unit,
    defender_terrain: Terrain,
) -> i32 {
    let attacker_stats = attacker.unit_type.stats();
    let defender_stats = defender.unit_type.stats();

    // Base damage formula (simplified Advance Wars)
    // Damage = AttackPower * (AttackerHP% / 100) * ((100 - DefenseBonus) / 100)
    let attack_power = attacker_stats.attack;
    let attacker_hp_modifier = attacker.hp_percentage();

    // Defense from unit type + terrain
    let terrain_defense = defender_terrain.defense_bonus() * 10;
    let total_defense = (defender_stats.defense + terrain_defense).min(200);
    let defense_modifier = (200.0 - total_defense as f32) / 200.0;

    let base_damage = attack_power as f32 * attacker_hp_modifier * defense_modifier;

    // Add some randomness (90% - 110%)
    let random_modifier = 1.0; // TODO: Add actual randomness

    (base_damage * random_modifier).round() as i32
}

/// Check if attacker can attack defender
pub fn can_attack(
    attacker: &Unit,
    attacker_pos: &GridPosition,
    attacker_faction: &FactionMember,
    defender_pos: &GridPosition,
    defender_faction: &FactionMember,
) -> bool {
    // Can't attack own units
    if attacker_faction.faction == defender_faction.faction {
        return false;
    }

    // Check range
    let distance = attacker_pos.distance_to(defender_pos);
    let stats = attacker.unit_type.stats();
    let (min_range, max_range) = stats.attack_range;

    // Can't attack if no attack power
    if stats.attack == 0 {
        return false;
    }

    distance >= min_range && distance <= max_range
}

fn process_attacks(
    mut events: EventReader<AttackEvent>,
    mut commands: Commands,
    mut units: Query<(&mut Unit, &GridPosition, &FactionMember)>,
    map: Res<GameMap>,
) {
    for event in events.read() {
        let Ok([(mut attacker_unit, attacker_pos, attacker_faction),
               (mut defender_unit, defender_pos, defender_faction)])
            = units.get_many_mut([event.attacker, event.defender]) else {
            continue;
        };

        // Verify attack is valid
        if !can_attack(&attacker_unit, attacker_pos, attacker_faction, defender_pos, defender_faction) {
            warn!("Invalid attack!");
            continue;
        }

        // Get defender's terrain
        let defender_terrain = map
            .get(defender_pos.x, defender_pos.y)
            .unwrap_or(Terrain::Clearing);

        // Calculate and apply damage
        let damage = calculate_damage(&attacker_unit, &defender_unit, defender_terrain);
        defender_unit.hp -= damage;

        info!(
            "{} attacks {} for {} damage! (HP: {} -> {})",
            attacker_unit.unit_type.name(),
            defender_unit.unit_type.name(),
            damage,
            defender_unit.hp + damage,
            defender_unit.hp
        );

        // Mark attacker as having attacked
        attacker_unit.attacked = true;

        // Check if defender is destroyed
        if defender_unit.hp <= 0 {
            info!("{} destroyed!", defender_unit.unit_type.name());
            commands.entity(event.defender).despawn();
        } else {
            // Counter-attack (if defender has direct attack and attacker is in range)
            let counter_stats = defender_unit.unit_type.stats();
            if counter_stats.attack > 0 && counter_stats.attack_range.0 == 1 {
                let attacker_terrain = map
                    .get(attacker_pos.x, attacker_pos.y)
                    .unwrap_or(Terrain::Clearing);

                let counter_damage = calculate_damage(&defender_unit, &attacker_unit, attacker_terrain);
                attacker_unit.hp -= counter_damage;

                info!(
                    "{} counter-attacks for {} damage! (HP: {})",
                    defender_unit.unit_type.name(),
                    counter_damage,
                    attacker_unit.hp
                );

                if attacker_unit.hp <= 0 {
                    info!("{} destroyed by counter-attack!", attacker_unit.unit_type.name());
                    commands.entity(event.attacker).despawn();
                }
            }
        }
    }
}
