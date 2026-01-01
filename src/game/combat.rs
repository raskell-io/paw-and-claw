use bevy::prelude::*;

use super::{GridPosition, Unit, FactionMember, Terrain, Tile, GameMap, Commanders, CoBonuses};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<AttackEvent>()
            .add_event::<CaptureEvent>()
            .add_systems(Update, (process_attacks, process_captures));
    }
}

/// Event fired when a unit attacks another
#[derive(Event)]
pub struct AttackEvent {
    pub attacker: Entity,
    pub defender: Entity,
}

/// Event fired when a unit attempts to capture a tile
#[derive(Event)]
pub struct CaptureEvent {
    pub unit: Entity,
    pub tile: Entity,
}

/// Calculate damage based on Advance Wars-like formula
/// Now includes CO bonuses for attack and defense
pub fn calculate_damage(
    attacker: &Unit,
    defender: &Unit,
    defender_terrain: Terrain,
    attacker_co: &CoBonuses,
    defender_co: &CoBonuses,
) -> i32 {
    let attacker_stats = attacker.unit_type.stats();
    let defender_stats = defender.unit_type.stats();

    // Base damage formula (simplified Advance Wars)
    // Damage = AttackPower * CO_Attack * (AttackerHP% / 100) * ((100 - DefenseBonus) / 100)
    let attack_power = (attacker_stats.attack as f32 * attacker_co.attack) as i32;
    let attacker_hp_modifier = attacker.hp_percentage();

    // Defense from unit type + terrain, modified by CO
    let base_defense = (defender_stats.defense as f32 * defender_co.defense) as i32;
    let terrain_defense = defender_terrain.defense_bonus() * 10;
    let total_defense = (base_defense + terrain_defense).min(200);
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
    mut commanders: ResMut<Commanders>,
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

        // Get CO bonuses for both factions
        let attacker_co = commanders.get_bonuses(attacker_faction.faction);
        let defender_co = commanders.get_bonuses(defender_faction.faction);

        // Get defender's terrain
        let defender_terrain = map
            .get(defender_pos.x, defender_pos.y)
            .unwrap_or(Terrain::Grass);

        // Calculate and apply damage (with CO bonuses)
        let damage = calculate_damage(&attacker_unit, &defender_unit, defender_terrain, &attacker_co, &defender_co);
        defender_unit.hp -= damage;

        info!(
            "{} attacks {} for {} damage! (HP: {} -> {})",
            attacker_unit.unit_type.name(),
            defender_unit.unit_type.name(),
            damage,
            defender_unit.hp + damage,
            defender_unit.hp
        );

        // Charge power meters (damage dealt/taken generates charge)
        let charge_amount = (damage as u32) / 10;
        commanders.charge(attacker_faction.faction, charge_amount);
        commanders.charge(defender_faction.faction, charge_amount / 2); // Less for taking damage

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
                    .unwrap_or(Terrain::Grass);

                let counter_damage = calculate_damage(&defender_unit, &attacker_unit, attacker_terrain, &defender_co, &attacker_co);
                attacker_unit.hp -= counter_damage;

                info!(
                    "{} counter-attacks for {} damage! (HP: {})",
                    defender_unit.unit_type.name(),
                    counter_damage,
                    attacker_unit.hp
                );

                // Charge power meters for counter-attack
                let counter_charge = (counter_damage as u32) / 10;
                commanders.charge(defender_faction.faction, counter_charge);
                commanders.charge(attacker_faction.faction, counter_charge / 2);

                if attacker_unit.hp <= 0 {
                    info!("{} destroyed by counter-attack!", attacker_unit.unit_type.name());
                    commands.entity(event.attacker).despawn();
                }
            }
        }
    }
}

fn process_captures(
    mut events: EventReader<CaptureEvent>,
    units: Query<(&Unit, &FactionMember)>,
    mut tiles: Query<(&mut Tile, &mut Sprite)>,
) {
    for event in events.read() {
        let Ok((unit, faction_member)) = units.get(event.unit) else {
            continue;
        };

        let Ok((mut tile, mut sprite)) = tiles.get_mut(event.tile) else {
            continue;
        };

        // Can only capture capturable terrain
        if !tile.terrain.is_capturable() {
            warn!("Cannot capture non-capturable terrain!");
            continue;
        }

        // Can't capture your own tiles
        if tile.owner == Some(faction_member.faction) {
            warn!("Cannot capture your own tile!");
            continue;
        }

        // Calculate capture points based on unit HP (like Advance Wars)
        // Full HP = full capture power, half HP = half capture power
        let capture_power = unit.hp;

        // If a different faction is capturing, reset progress
        if tile.capturing_faction != Some(faction_member.faction) {
            tile.capture_progress = 0;
            tile.capturing_faction = Some(faction_member.faction);
        }

        // Add capture progress
        tile.capture_progress += capture_power;

        let required = tile.terrain.capture_points();
        info!(
            "{:?} capturing {:?} ({}/{} points)",
            faction_member.faction,
            tile.terrain,
            tile.capture_progress,
            required
        );

        // Check if capture is complete
        if tile.capture_progress >= required {
            tile.owner = Some(faction_member.faction);
            tile.capture_progress = 0;
            tile.capturing_faction = None;

            // Update tile color to show new ownership
            let base_color = tile.terrain.color();
            let faction_color = faction_member.faction.color();
            sprite.color = blend_color(base_color, faction_color, 0.3);

            info!(
                "{:?} captured by {:?}!",
                tile.terrain, faction_member.faction
            );
        }
    }
}

/// Blend two colors together (used for capture visuals)
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
