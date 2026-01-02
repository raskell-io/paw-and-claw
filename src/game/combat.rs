use bevy::prelude::*;

use super::{GridPosition, Unit, FactionMember, Terrain, Tile, GameMap, Commanders, CoBonuses, Weather};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<AttackEvent>()
            .add_event::<CaptureEvent>()
            .add_event::<JoinEvent>()
            .add_systems(Update, (process_attacks, process_captures, process_joins));
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

/// Event fired when two units join/merge
#[derive(Event)]
pub struct JoinEvent {
    pub source: Entity,  // Unit that moved (will be despawned)
    pub target: Entity,  // Unit being joined into (will receive HP/ammo/stamina)
}

/// Calculate damage based on Advance Wars-like formula
/// Includes CO bonuses and weather effects for attack and defense
pub fn calculate_damage(
    attacker: &Unit,
    defender: &Unit,
    defender_terrain: Terrain,
    attacker_co: &CoBonuses,
    defender_co: &CoBonuses,
    weather: &Weather,
) -> i32 {
    let attacker_stats = attacker.unit_type.stats();
    let defender_stats = defender.unit_type.stats();
    let weather_effects = weather.effects();

    // Base damage formula (simplified Advance Wars)
    // Damage = AttackPower * CO_Attack * Weather_Attack * (AttackerHP% / 100) * ((100 - DefenseBonus) / 100)
    let attack_power = (attacker_stats.attack as f32
        * attacker_co.attack
        * weather_effects.attack_multiplier) as i32;
    let attacker_hp_modifier = attacker.hp_percentage();

    // Defense from unit type + terrain, modified by CO and weather
    let base_defense = (defender_stats.defense as f32
        * defender_co.defense
        * weather_effects.defense_multiplier) as i32;

    // Terrain defense - check if weather negates cover (thickets, brambles, etc.)
    let terrain_defense = if (defender_terrain == Terrain::Thicket || defender_terrain == Terrain::Brambles)
        && !weather.forests_provide_cover()
    {
        0 // Rain negates vegetation cover
    } else {
        defender_terrain.defense_bonus() * 10
    };

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
    weather: Res<Weather>,
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

        // Check ammo - can't attack without ammo (if unit uses ammo)
        let attacker_stats = attacker_unit.unit_type.stats();
        if attacker_stats.max_ammo > 0 && attacker_unit.ammo == 0 {
            warn!("No ammo to attack!");
            continue;
        }

        // Deduct ammo if unit uses ammo
        if attacker_stats.max_ammo > 0 {
            attacker_unit.ammo = attacker_unit.ammo.saturating_sub(1);
            info!("Ammo: {} -> {}", attacker_unit.ammo + 1, attacker_unit.ammo);
        }

        // Get CO bonuses for both factions
        let attacker_co = commanders.get_bonuses(attacker_faction.faction);
        let defender_co = commanders.get_bonuses(defender_faction.faction);

        // Get defender's terrain
        let defender_terrain = map
            .get(defender_pos.x, defender_pos.y)
            .unwrap_or(Terrain::Grass);

        // Calculate and apply damage (with CO bonuses and weather effects)
        let damage = calculate_damage(&attacker_unit, &defender_unit, defender_terrain, &attacker_co, &defender_co, &weather);
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
            // Can only counter if: has attack power, is melee (range 1), and has ammo (if uses ammo)
            let can_counter = counter_stats.attack > 0
                && counter_stats.attack_range.0 == 1
                && (counter_stats.max_ammo == 0 || defender_unit.ammo > 0);

            if can_counter {
                // Deduct ammo for counter-attack if unit uses ammo
                if counter_stats.max_ammo > 0 {
                    defender_unit.ammo = defender_unit.ammo.saturating_sub(1);
                }

                let attacker_terrain = map
                    .get(attacker_pos.x, attacker_pos.y)
                    .unwrap_or(Terrain::Grass);

                let counter_damage = calculate_damage(&defender_unit, &attacker_unit, attacker_terrain, &defender_co, &attacker_co, &weather);
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

/// Process unit joining/merging
fn process_joins(
    mut events: EventReader<JoinEvent>,
    mut commands: Commands,
    mut units: Query<&mut Unit>,
) {
    for event in events.read() {
        let Ok([source_unit, mut target_unit]) = units.get_many_mut([event.source, event.target]) else {
            warn!("Failed to get units for join!");
            continue;
        };

        // Both units must be same type (this should have been checked before sending the event)
        if source_unit.unit_type != target_unit.unit_type {
            warn!("Cannot join different unit types!");
            continue;
        }

        let stats = target_unit.unit_type.stats();

        // Combine HP (capped at max)
        let combined_hp = (target_unit.hp + source_unit.hp).min(stats.max_hp);

        // Combine stamina and ammo (capped at max)
        let combined_stamina = (target_unit.stamina + source_unit.stamina).min(stats.max_stamina);
        let combined_ammo = (target_unit.ammo + source_unit.ammo).min(stats.max_ammo);

        info!(
            "Joining {} units: HP {}+{}->{}, Stamina {}+{}->{}, Ammo {}+{}->{}",
            target_unit.unit_type.name(),
            target_unit.hp, source_unit.hp, combined_hp,
            target_unit.stamina, source_unit.stamina, combined_stamina,
            target_unit.ammo, source_unit.ammo, combined_ammo
        );

        // Apply combined stats to target
        target_unit.hp = combined_hp;
        target_unit.stamina = combined_stamina;
        target_unit.ammo = combined_ammo;

        // Despawn source unit
        commands.entity(event.source).despawn();
    }
}
