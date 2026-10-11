use bevy::prelude::*;
use rand::Rng;

use super::{GridPosition, Unit, FactionMember, Terrain, Tile, GameMap, Commanders, CoBonuses, Weather, UnitType, CargoUnit, spawn_unit_with_state, SpriteAssets, GameData};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AttackEvent>()
            .add_message::<CaptureEvent>()
            .add_message::<JoinEvent>()
            .add_message::<ResupplyEvent>()
            .add_message::<LoadEvent>()
            .add_message::<UnloadEvent>()
            .add_systems(Update, (process_attacks, process_captures, process_joins, process_resupply, process_load, process_unload));
    }
}

/// Event fired when a unit attacks another
#[derive(Message)]
pub struct AttackEvent {
    pub attacker: Entity,
    pub defender: Entity,
}

/// Event fired when a unit attempts to capture a tile
#[derive(Message)]
pub struct CaptureEvent {
    pub unit: Entity,
    pub tile: Entity,
}

/// Event fired when two units join/merge
#[derive(Message)]
pub struct JoinEvent {
    pub source: Entity,  // Unit that moved (will be despawned)
    pub target: Entity,  // Unit being joined into (will receive HP/ammo/stamina)
}

/// Event fired when a Supplier unit resupplies adjacent friendly units
#[derive(Message)]
pub struct ResupplyEvent {
    pub supplier: Entity,  // The Supplier unit performing resupply
}

/// Event fired when a unit is loaded into a transport
#[derive(Message)]
pub struct LoadEvent {
    pub transport_pos: (i32, i32),  // Position of the transport
    pub passenger: Entity,           // The unit being loaded
}

/// Event fired when a unit is unloaded from a transport
#[derive(Message)]
pub struct UnloadEvent {
    pub transport: Entity,  // The transport unit
    pub position: (i32, i32),  // Where to unload the passenger
}

/// Damage before luck, plus the maximum luck bonus, from the Advance Wars 2 formula
///
/// AW2 Formula:
/// Damage = (B * ACO/100 + Luck) * (AHP/10) * ((200 - (DCO + DTR * DHP)) / 100)
///
/// Where:
/// - B = Base damage from damage chart (0-180%)
/// - ACO = Attacker's CO attack bonus (100 = normal, 110 = +10%)
/// - AHP = Attacker's displayed HP (1-10)
/// - DCO = Defender's CO defense bonus (100 = normal)
/// - DTR = Defender's terrain stars (0-4)
/// - DHP = Defender's displayed HP (1-10)
/// - Luck = Random 0-9
///
/// The result is a percentage of a full-health unit, which maps directly onto
/// our 100 HP scale.
///
/// Returns None when the damage table has no entry (attacker cannot hit defender).
fn damage_before_luck(
    attacker: &Unit,
    defender: &Unit,
    defender_terrain: Terrain,
    attacker_co: &CoBonuses,
    defender_co: &CoBonuses,
    weather: &Weather,
    game_data: &GameData,
) -> Option<(f32, f32)> {
    // Base damage from damage tables (B in AW2 formula)
    let base_damage_percent = game_data.get_base_damage(attacker.unit_type, defender.unit_type)? as f32;

    let weather_effects = weather.effects();

    // Convert CO attack bonus to AW2 scale (1.0 = 100, 1.1 = 110)
    let aco = attacker_co.attack * 100.0 * weather_effects.attack_multiplier;

    // Attacker HP on 1-10 scale (AW2 uses display HP)
    let ahp = (attacker.hp as f32 / 10.0).ceil().max(1.0);

    // Damaged units hit proportionally weaker: 10 HP = full, 1 HP = 10%
    let hp_scale = ahp / 10.0;

    // Terrain defense stars (0-4 in AW2)
    // Weather can negate vegetation cover
    let terrain_stars = if (defender_terrain == Terrain::Thicket || defender_terrain == Terrain::Brambles)
        && !weather.forests_provide_cover()
    {
        0.0 // Rain negates vegetation cover
    } else {
        game_data.terrain_defense(defender_terrain) as f32
    };

    // Convert CO defense bonus to AW2 scale
    let dco = defender_co.defense * 100.0 * weather_effects.defense_multiplier;

    // Defender HP on 1-10 scale
    let dhp = (defender.hp as f32 / 10.0).ceil().max(1.0);

    // Calculate defense component: (200 - (DCO + DTR * DHP)) / 100
    // At 100% defense and 0 terrain, this equals 1.0
    // At 100% defense and 4 stars with 10 HP, this equals 0.6 (40% reduction)
    let defense_component = (200.0 - (dco + terrain_stars * dhp)) / 100.0;
    let defense_component = defense_component.max(0.1); // Minimum 10% damage gets through

    let base_final = base_damage_percent * (aco / 100.0) * hp_scale * defense_component;

    // AW2 Luck: 0-9 added before the HP and defense scaling
    let max_luck = 9.0 * hp_scale * defense_component;

    Some((base_final, max_luck))
}

/// Calculate damage dealt by an attack, including a random luck roll
pub fn calculate_damage(
    attacker: &Unit,
    defender: &Unit,
    defender_terrain: Terrain,
    attacker_co: &CoBonuses,
    defender_co: &CoBonuses,
    weather: &Weather,
    game_data: &GameData,
) -> i32 {
    let Some((base_final, max_luck)) =
        damage_before_luck(attacker, defender, defender_terrain, attacker_co, defender_co, weather, game_data)
    else {
        return 0;
    };

    let luck_roll = rand::thread_rng().gen_range(0..=9) as f32;
    let luck_damage = luck_roll * (max_luck / 9.0);

    (base_final + luck_damage).max(0.0).round() as i32
}

/// Calculate damage estimate for UI display (returns min, max)
/// Uses the same formula as calculate_damage but without randomness
pub fn estimate_damage(
    attacker: &Unit,
    defender: &Unit,
    defender_terrain: Terrain,
    attacker_co: &CoBonuses,
    defender_co: &CoBonuses,
    weather: &Weather,
    game_data: &GameData,
) -> (i32, i32) {
    let Some((base_final, max_luck)) =
        damage_before_luck(attacker, defender, defender_terrain, attacker_co, defender_co, weather, game_data)
    else {
        return (0, 0);
    };

    let min_damage = base_final.max(0.0).round() as i32;
    let max_damage = (base_final + max_luck).max(0.0).round() as i32;

    (min_damage, max_damage)
}

/// Check if attacker can attack defender: different factions, defender in
/// range, and the damage table says the attacker can hit that unit type
pub fn can_attack(
    attacker: &Unit,
    attacker_pos: &GridPosition,
    attacker_faction: &FactionMember,
    defender: &Unit,
    defender_pos: &GridPosition,
    defender_faction: &FactionMember,
    game_data: &GameData,
) -> bool {
    // Can't attack own units
    if attacker_faction.faction == defender_faction.faction {
        return false;
    }

    if !game_data.can_damage(attacker.unit_type, defender.unit_type) {
        return false;
    }

    let distance = attacker_pos.distance_to(defender_pos);
    let (min_range, max_range) = game_data.unit_stats(attacker.unit_type).attack_range;
    distance >= min_range && distance <= max_range
}

fn process_attacks(
    mut events: MessageReader<AttackEvent>,
    mut commands: Commands,
    mut units: Query<(&mut Unit, &GridPosition, &FactionMember)>,
    map: Res<GameMap>,
    mut commanders: ResMut<Commanders>,
    weather: Res<Weather>,
    game_data: Res<GameData>,
    mut defeated: MessageWriter<super::UnitDefeated>,
) {
    for event in events.read() {
        let Ok([(mut attacker_unit, attacker_pos, attacker_faction),
               (mut defender_unit, defender_pos, defender_faction)])
            = units.get_many_mut([event.attacker, event.defender]) else {
            continue;
        };

        // Verify attack is valid
        if !can_attack(&attacker_unit, attacker_pos, attacker_faction, &defender_unit, defender_pos, defender_faction, &game_data) {
            warn!("Invalid attack!");
            continue;
        }

        // Check ammo - can't attack without ammo (if unit uses ammo)
        let attacker_max_ammo = game_data.unit_stats(attacker_unit.unit_type).max_ammo;
        if attacker_max_ammo > 0 && attacker_unit.ammo == 0 {
            warn!("No ammo to attack!");
            continue;
        }

        // Deduct ammo if unit uses ammo
        if attacker_max_ammo > 0 {
            attacker_unit.ammo = attacker_unit.ammo.saturating_sub(1);
            info!("Ammo: {} -> {}", attacker_unit.ammo + 1, attacker_unit.ammo);
        }

        // Get CO bonuses for both factions
        let attacker_co = commanders.get_bonuses(attacker_faction.faction, &game_data);
        let defender_co = commanders.get_bonuses(defender_faction.faction, &game_data);

        // Get defender's terrain
        let defender_terrain = map
            .get(defender_pos.x, defender_pos.y)
            .unwrap_or(Terrain::Grass);

        // Calculate and apply damage (with CO bonuses and weather effects)
        let damage = calculate_damage(&attacker_unit, &defender_unit, defender_terrain, &attacker_co, &defender_co, &weather, &game_data);
        defender_unit.hp -= damage;

        info!(
            "{} attacks {} for {} damage! (HP: {} -> {})",
            game_data.unit_name(attacker_unit.unit_type),
            game_data.unit_name(defender_unit.unit_type),
            damage,
            defender_unit.hp + damage,
            defender_unit.hp
        );

        // Charge power meters (damage dealt/taken generates charge)
        let charge_amount = (damage as u32) / 10;
        commanders.charge(attacker_faction.faction, charge_amount, &game_data);
        commanders.charge(defender_faction.faction, charge_amount / 2, &game_data); // Less for taking damage

        // Mark attacker as having attacked
        attacker_unit.attacked = true;

        // Check if defender is destroyed
        if defender_unit.hp <= 0 {
            info!("{} destroyed!", game_data.unit_name(defender_unit.unit_type));
            defeated.write(super::UnitDefeated { position: *defender_pos });
            commands.entity(event.defender).despawn();
        } else {
            // Counter-attack (if defender can reach attacker)
            // Can only counter if:
            // 1. The damage table lets the defender hit the attacker
            // 2. Attacker is within defender's attack range (for counter)
            // 3. Has ammo (if uses ammo)
            // Note: Ranged/indirect units (min_range > 1) typically can't counter at all
            // because they can't fire at adjacent units
            let counter_max_ammo = game_data.unit_stats(defender_unit.unit_type).max_ammo;
            let can_counter = can_attack(&defender_unit, defender_pos, defender_faction, &attacker_unit, attacker_pos, attacker_faction, &game_data)
                && (counter_max_ammo == 0 || defender_unit.ammo > 0);

            if can_counter {
                // Deduct ammo for counter-attack if unit uses ammo
                if counter_max_ammo > 0 {
                    defender_unit.ammo = defender_unit.ammo.saturating_sub(1);
                }

                let attacker_terrain = map
                    .get(attacker_pos.x, attacker_pos.y)
                    .unwrap_or(Terrain::Grass);

                let counter_damage = calculate_damage(&defender_unit, &attacker_unit, attacker_terrain, &defender_co, &attacker_co, &weather, &game_data);
                attacker_unit.hp -= counter_damage;

                info!(
                    "{} counter-attacks for {} damage! (HP: {})",
                    game_data.unit_name(defender_unit.unit_type),
                    counter_damage,
                    attacker_unit.hp
                );

                // Charge power meters for counter-attack
                let counter_charge = (counter_damage as u32) / 10;
                commanders.charge(defender_faction.faction, counter_charge, &game_data);
                commanders.charge(attacker_faction.faction, counter_charge / 2, &game_data);

                if attacker_unit.hp <= 0 {
                    info!("{} destroyed by counter-attack!", game_data.unit_name(attacker_unit.unit_type));
                    defeated.write(super::UnitDefeated { position: *attacker_pos });
                    commands.entity(event.attacker).despawn();
                }
            }
        }
    }
}

fn process_captures(
    mut events: MessageReader<CaptureEvent>,
    units: Query<(&Unit, &FactionMember)>,
    mut tiles: Query<(&mut Tile, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        let Ok((unit, faction_member)) = units.get(event.unit) else {
            continue;
        };

        let Ok((mut tile, tile_material)) = tiles.get_mut(event.tile) else {
            warn!("Capture target {:?} is not a tile", event.tile);
            continue;
        };

        // Can only capture capturable terrain
        if !game_data.terrain_capturable(tile.terrain) {
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

        let required = game_data.terrain_capture_points(tile.terrain) as i32;
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
            if let Some(material) = materials.get_mut(&tile_material.0) {
                let textured = material.base_color_texture.is_some();
                material.base_color = super::tile_tint(&game_data, tile.terrain, Some(faction_member.faction), textured);
            }

            info!(
                "{:?} captured by {:?}!",
                tile.terrain, faction_member.faction
            );
        }
    }
}

/// Process unit joining/merging
fn process_joins(
    mut events: MessageReader<JoinEvent>,
    mut commands: Commands,
    mut units: Query<&mut Unit>,
    game_data: Res<GameData>,
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

        let stats = game_data.unit_stats(target_unit.unit_type);

        // Combine HP (capped at max)
        let combined_hp = (target_unit.hp + source_unit.hp).min(stats.max_hp);

        // Combine stamina and ammo (capped at max)
        let combined_stamina = (target_unit.stamina + source_unit.stamina).min(stats.max_stamina);
        let combined_ammo = (target_unit.ammo + source_unit.ammo).min(stats.max_ammo);

        info!(
            "Joining {} units: HP {}+{}->{}, Stamina {}+{}->{}, Ammo {}+{}->{}",
            game_data.unit_name(target_unit.unit_type),
            target_unit.hp, source_unit.hp, combined_hp,
            target_unit.stamina, source_unit.stamina, combined_stamina,
            target_unit.ammo, source_unit.ammo, combined_ammo
        );

        // Apply combined stats to target
        target_unit.hp = combined_hp;
        target_unit.stamina = combined_stamina;
        target_unit.ammo = combined_ammo;
        target_unit.exhausted = true;  // Unit's turn is complete after joining

        // Despawn source unit (with children like shadows/borders)
        commands.entity(event.source).despawn();
    }
}

/// Process Supplier resupply action - resupplies all adjacent friendly units
fn process_resupply(
    mut events: MessageReader<ResupplyEvent>,
    mut units: Query<(&mut Unit, &GridPosition, &FactionMember)>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        // Get supplier info first
        let (supplier_pos, supplier_faction) = {
            let Ok((supplier_unit, pos, faction)) = units.get(event.supplier) else {
                warn!("Failed to get supplier unit!");
                continue;
            };

            // Verify this is actually a Supplier unit
            if supplier_unit.unit_type != UnitType::Supplier {
                warn!("Resupply event on non-Supplier unit!");
                continue;
            }

            (GridPosition::new(pos.x, pos.y), faction.faction)
        };

        // Find all adjacent friendly units and resupply them
        let adjacent_positions = [
            (supplier_pos.x - 1, supplier_pos.y),
            (supplier_pos.x + 1, supplier_pos.y),
            (supplier_pos.x, supplier_pos.y - 1),
            (supplier_pos.x, supplier_pos.y + 1),
        ];

        let mut resupplied_count = 0;

        for (mut unit, pos, faction) in units.iter_mut() {
            // Skip if not adjacent
            if !adjacent_positions.contains(&(pos.x, pos.y)) {
                continue;
            }

            // Skip if not same faction
            if faction.faction != supplier_faction {
                continue;
            }

            let stats = game_data.unit_stats(unit.unit_type);
            let old_stamina = unit.stamina;
            let old_ammo = unit.ammo;

            // Restore stamina and ammo to max
            unit.stamina = stats.max_stamina;
            unit.ammo = stats.max_ammo;

            // Log if anything was resupplied
            if old_stamina < stats.max_stamina || old_ammo < stats.max_ammo {
                info!(
                    "Supplier resupplied {}: stamina {}->{}, ammo {}->{}",
                    game_data.unit_name(unit.unit_type),
                    old_stamina,
                    unit.stamina,
                    old_ammo,
                    unit.ammo
                );
                resupplied_count += 1;
            }
        }

        if resupplied_count > 0 {
            info!("Supplier resupplied {} adjacent units!", resupplied_count);
        } else {
            info!("No units nearby to resupply.");
        }

        // Mark the supplier as having acted
        if let Ok((mut supplier_unit, _, _)) = units.get_mut(event.supplier) {
            supplier_unit.attacked = true;  // Using attacked flag to indicate action taken
        }
    }
}

/// Process loading a unit into a transport
fn process_load(
    mut events: MessageReader<LoadEvent>,
    mut commands: Commands,
    mut units: Query<(Entity, &mut Unit, &GridPosition, &FactionMember)>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        // Get passenger info first
        let passenger_info = {
            let Ok((_, passenger_unit, _, passenger_faction)) = units.get(event.passenger) else {
                warn!("Failed to get passenger unit!");
                continue;
            };

            if !passenger_unit.can_be_transported() {
                warn!("{} cannot be transported!", game_data.unit_name(passenger_unit.unit_type));
                continue;
            }

            (CargoUnit::from_unit(&passenger_unit), passenger_faction.faction)
        };

        // Find transport at the given position
        let transport_entity = units.iter()
            .find(|(_, unit, pos, faction)| {
                pos.x == event.transport_pos.0 &&
                pos.y == event.transport_pos.1 &&
                unit.is_transport() &&
                !unit.has_cargo() &&
                faction.faction == passenger_info.1
            })
            .map(|(entity, _, _, _)| entity);

        let Some(transport_entity) = transport_entity else {
            warn!("No valid transport found at position {:?}!", event.transport_pos);
            continue;
        };

        // Now get transport and load the passenger
        let Ok((_, mut transport_unit, _, _)) = units.get_mut(transport_entity) else {
            warn!("Failed to get transport unit!");
            continue;
        };

        // Load the passenger
        let cargo = passenger_info.0.clone();
        transport_unit.cargo = Some(cargo.clone());
        transport_unit.attacked = true; // Loading ends the transport's action

        info!(
            "{} loaded into {}",
            game_data.unit_name(cargo.unit_type),
            game_data.unit_name(transport_unit.unit_type)
        );

        // Despawn the passenger entity (it's now stored as cargo)
        commands.entity(event.passenger).despawn();
    }
}

/// Process unloading a unit from a transport
fn process_unload(
    mut events: MessageReader<UnloadEvent>,
    mut commands: Commands,
    mut units: Query<(&mut Unit, &GridPosition, &FactionMember)>,
    map: Res<GameMap>,
    sprite_assets: Res<SpriteAssets>,
    images: Res<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        let (cargo, faction) = {
            let Ok((mut transport_unit, _transport_pos, transport_faction)) = units.get_mut(event.transport) else {
                warn!("Failed to get transport unit!");
                continue;
            };

            // Verify transport has cargo
            let Some(cargo) = transport_unit.cargo.take() else {
                warn!("Transport has no cargo to unload!");
                continue;
            };

            // Mark transport as having acted
            transport_unit.attacked = true;

            (cargo, transport_faction.faction)
        };

        // Check if unload position is valid (on map and passable)
        let (ux, uy) = event.position;
        if ux < 0 || uy < 0 || ux >= map.width as i32 || uy >= map.height as i32 {
            warn!("Invalid unload position!");
            // Put cargo back (need to re-get transport)
            if let Ok((mut transport_unit, _, _)) = units.get_mut(event.transport) {
                transport_unit.cargo = Some(cargo);
            }
            continue;
        }

        // Check terrain is passable for the cargo unit
        let terrain = map.get(ux, uy).unwrap_or(Terrain::Grass);
        if !game_data.is_passable(terrain, game_data.unit_class(cargo.unit_type)) {
            warn!("Cannot unload onto impassable terrain!");
            // Put cargo back
            if let Ok((mut transport_unit, _, _)) = units.get_mut(event.transport) {
                transport_unit.cargo = Some(cargo);
            }
            continue;
        }

        // Check no other unit is at the position
        let position_occupied = units.iter().any(|(_, pos, _)| pos.x == ux && pos.y == uy);
        if position_occupied {
            warn!("Cannot unload onto occupied tile!");
            // Put cargo back
            if let Ok((mut transport_unit, _, _)) = units.get_mut(event.transport) {
                transport_unit.cargo = Some(cargo);
            }
            continue;
        }

        info!(
            "{} unloaded at ({}, {})",
            game_data.unit_name(cargo.unit_type),
            ux, uy
        );

        // Spawn the unloaded unit, keeping its HP/stamina/ammo from before loading
        spawn_unit_with_state(
            &mut commands,
            &map,
            &mut meshes,
            &mut materials,
            &sprite_assets,
            &images,
            &game_data,
            faction,
            cargo.to_unit(),
            ux,
            uy,
        );
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Faction, WeatherType};

    fn setup() -> (GameData, CoBonuses, Weather) {
        (GameData::load_defaults(), CoBonuses::none(), Weather::default())
    }

    fn unit(unit_type: UnitType, game_data: &GameData) -> Unit {
        Unit::new(unit_type, game_data)
    }

    #[test]
    fn infantry_vs_infantry_on_plains_matches_aw2() {
        // AW2 reference: full HP infantry vs infantry on plains deals 49-57%.
        // (55 + luck) * 1.0 * (200 - (100 + 1 * 10)) / 100
        let (gd, co, weather) = setup();
        let scout = unit(UnitType::Scout, &gd);
        let (min, max) = estimate_damage(&scout, &scout, Terrain::Grass, &co, &co, &weather, &gd);
        assert_eq!((min, max), (50, 58));
    }

    #[test]
    fn damage_table_drives_matchups() {
        let (gd, co, weather) = setup();
        let scout = unit(UnitType::Scout, &gd);
        let tank = unit(UnitType::Ironclad, &gd);
        let (scout_on_tank, _) = estimate_damage(&scout, &tank, Terrain::Log, &co, &co, &weather, &gd);
        let (tank_on_scout, _) = estimate_damage(&tank, &scout, Terrain::Log, &co, &co, &weather, &gd);
        assert_eq!(scout_on_tank, 5);
        assert_eq!(tank_on_scout, 75);
    }

    #[test]
    fn terrain_cover_reduces_damage() {
        let (gd, co, weather) = setup();
        let tank = unit(UnitType::Ironclad, &gd);
        let scout = unit(UnitType::Scout, &gd);
        let (open, _) = estimate_damage(&tank, &scout, Terrain::Log, &co, &co, &weather, &gd);
        let (fortified, _) = estimate_damage(&tank, &scout, Terrain::Base, &co, &co, &weather, &gd);
        assert!(fortified < open, "base ({fortified}) should protect better than a log ({open})");
    }

    #[test]
    fn damaged_attacker_hits_proportionally_weaker() {
        let (gd, co, weather) = setup();
        let mut tank = unit(UnitType::Ironclad, &gd);
        let scout = unit(UnitType::Scout, &gd);
        let (full, _) = estimate_damage(&tank, &scout, Terrain::Log, &co, &co, &weather, &gd);
        tank.hp = 50;
        let (half, _) = estimate_damage(&tank, &scout, Terrain::Log, &co, &co, &weather, &gd);
        assert_eq!(half, (full as f32 / 2.0).round() as i32);
    }

    #[test]
    fn co_attack_bonus_increases_damage() {
        let (gd, co, weather) = setup();
        let scout = unit(UnitType::Scout, &gd);
        let boosted = CoBonuses { attack: 1.2, ..CoBonuses::none() };
        let (normal, _) = estimate_damage(&scout, &scout, Terrain::Log, &co, &co, &weather, &gd);
        let (strong, _) = estimate_damage(&scout, &scout, Terrain::Log, &boosted, &co, &weather, &gd);
        assert_eq!(strong, (normal as f32 * 1.2).round() as i32);
    }

    #[test]
    fn rain_removes_forest_cover() {
        let (gd, co, clear) = setup();
        let rain = Weather::new(WeatherType::Rain);
        let tank = unit(UnitType::Ironclad, &gd);
        let scout = unit(UnitType::Scout, &gd);
        let (covered, _) = estimate_damage(&tank, &scout, Terrain::Thicket, &co, &co, &clear, &gd);
        let (exposed, _) = estimate_damage(&tank, &scout, Terrain::Thicket, &co, &co, &rain, &gd);
        assert!(exposed > covered);
    }

    #[test]
    fn rolled_damage_stays_within_estimate() {
        let (gd, co, weather) = setup();
        let tank = unit(UnitType::Ironclad, &gd);
        let scout = unit(UnitType::Scout, &gd);
        let (min, max) = estimate_damage(&tank, &scout, Terrain::Grass, &co, &co, &weather, &gd);
        for _ in 0..200 {
            let dmg = calculate_damage(&tank, &scout, Terrain::Grass, &co, &co, &weather, &gd);
            assert!((min..=max).contains(&dmg), "{dmg} outside {min}..={max}");
        }
    }

    #[test]
    fn missing_damage_table_entry_means_no_attack() {
        let (gd, co, weather) = setup();
        // Infantry cannot hit fighters in the damage tables
        assert!(!gd.can_damage(UnitType::Scout, UnitType::Raptor));
        let scout = unit(UnitType::Scout, &gd);
        let raptor = unit(UnitType::Raptor, &gd);
        assert_eq!(estimate_damage(&scout, &raptor, Terrain::Grass, &co, &co, &weather, &gd), (0, 0));

        let east = FactionMember { faction: Faction::Eastern };
        let north = FactionMember { faction: Faction::Northern };
        let here = GridPosition::new(0, 0);
        let adjacent = GridPosition::new(1, 0);
        assert!(!can_attack(&scout, &here, &east, &raptor, &adjacent, &north, &gd));
    }

    #[test]
    fn can_attack_checks_faction_and_range() {
        let (gd, _, _) = setup();
        let scout = unit(UnitType::Scout, &gd);
        let siege = unit(UnitType::Siege, &gd);
        let east = FactionMember { faction: Faction::Eastern };
        let north = FactionMember { faction: Faction::Northern };
        let origin = GridPosition::new(0, 0);

        assert!(can_attack(&scout, &origin, &east, &scout, &GridPosition::new(1, 0), &north, &gd));
        assert!(!can_attack(&scout, &origin, &east, &scout, &GridPosition::new(1, 0), &east, &gd), "friendly fire");
        assert!(!can_attack(&scout, &origin, &east, &scout, &GridPosition::new(2, 0), &north, &gd), "out of range");

        // Artillery has a minimum range of 2
        assert!(!can_attack(&siege, &origin, &east, &scout, &GridPosition::new(1, 0), &north, &gd));
        assert!(can_attack(&siege, &origin, &east, &scout, &GridPosition::new(2, 1), &north, &gd));
    }
}
