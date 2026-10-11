use bevy::prelude::*;
use rand::Rng;

use super::{GridPosition, Unit, FactionMember, Terrain, Tile, GameMap, Commanders, CoBonuses, Weather, UnitType, CargoUnit, spawn_unit_with_state, SpriteAssets, GameData, UnitClass};

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<AttackEvent>()
            .add_message::<CaptureEvent>()
            .add_message::<JoinEvent>()
            .add_message::<ResupplyEvent>()
            .add_message::<LoadEvent>()
            .add_message::<UnloadEvent>()
            .add_systems(Update, (process_attacks, process_captures, process_joins, process_resupply, process_load, process_unload))
            .add_systems(Update, abandon_interrupted_captures);
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
    pub transport_pos: (i32, i32),  // Position of the transport (the passenger moved onto it)
    pub passenger: Entity,           // The unit being loaded
}

/// Event fired when a unit is unloaded from a transport
#[derive(Message)]
pub struct UnloadEvent {
    pub transport: Entity,  // The transport unit
    pub cargo_index: usize, // Which passenger to drop
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
    attacker_terrain: Terrain,
    defender: &Unit,
    defender_terrain: Terrain,
    attacker_co: &CoBonuses,
    defender_co: &CoBonuses,
    _weather: &Weather,
    game_data: &GameData,
) -> Option<(f32, f32)> {
    // The weapon it would fire: primary if it can hit and has ammo, else machine gun
    let (base, _) = game_data.weapon(attacker.unit_type, attacker.ammo, defender.unit_type)?;
    let base_damage_percent = base as f32;

    // CO firepower in AW2 points (100 = normal). As in Advance Wars, weather
    // never changes damage; only COs do.
    let aco = attacker_co.attack(attacker.unit_type, attacker_terrain, game_data) * 100.0;

    // Attacker HP on 1-10 scale (AW2 uses display HP)
    let ahp = (attacker.hp as f32 / 10.0).ceil().max(1.0);
    let hp_scale = ahp / 10.0;

    // Terrain defense stars (0-4 in AW2, more with some COs); fliers get none
    let flying = matches!(game_data.unit_class(defender.unit_type), UnitClass::Air | UnitClass::AirTransport);
    let terrain_stars = if flying { 0.0 } else { defender_co.terrain_stars(game_data.terrain_defense(defender_terrain)) };

    // CO defense in AW2 points
    let vs_indirect = game_data.unit_stats(attacker.unit_type).attack_range.0 > 1;
    let dco = defender_co.defense(defender.unit_type, vs_indirect, game_data) * 100.0;

    // Defender HP on 1-10 scale
    let dhp = (defender.hp as f32 / 10.0).ceil().max(1.0);

    // (200 - (DCO + DTR * DHP)) / 100
    let defense_component = ((200.0 - (dco + terrain_stars * dhp)) / 100.0).max(0.0);

    let base_final = base_damage_percent * (aco / 100.0) * hp_scale * defense_component;

    // Each point of luck adds this much damage (luck is added before the HP and
    // defense scaling)
    let luck_scale = hp_scale * defense_component;

    Some((base_final, luck_scale))
}

/// Calculate damage dealt by an attack, including the luck roll: up to the
/// attacker CO's good luck added, up to its bad luck taken away
pub fn calculate_damage(
    attacker: &Unit,
    attacker_terrain: Terrain,
    defender: &Unit,
    defender_terrain: Terrain,
    attacker_co: &CoBonuses,
    defender_co: &CoBonuses,
    weather: &Weather,
    game_data: &GameData,
) -> i32 {
    let Some((base_final, luck_scale)) = damage_before_luck(attacker, attacker_terrain, defender, defender_terrain,
        attacker_co, defender_co, weather, game_data)
    else {
        return 0;
    };
    let mut rng = rand::thread_rng();
    let good = rng.gen_range(0..=attacker_co.luck()) as f32;
    let bad = rng.gen_range(0..=attacker_co.bad_luck()) as f32;

    // Advance Wars rounds damage down
    (base_final + (good - bad) * luck_scale).max(0.0).floor() as i32
}

/// Calculate damage estimate for UI display (returns min, max)
pub fn estimate_damage(
    attacker: &Unit,
    attacker_terrain: Terrain,
    defender: &Unit,
    defender_terrain: Terrain,
    attacker_co: &CoBonuses,
    defender_co: &CoBonuses,
    weather: &Weather,
    game_data: &GameData,
) -> (i32, i32) {
    let Some((base_final, luck_scale)) = damage_before_luck(attacker, attacker_terrain, defender, defender_terrain,
        attacker_co, defender_co, weather, game_data)
    else {
        return (0, 0);
    };
    let min_damage = (base_final - attacker_co.bad_luck() as f32 * luck_scale).max(0.0).floor() as i32;
    let max_damage = (base_final + attacker_co.luck() as f32 * luck_scale).max(0.0).floor() as i32;
    (min_damage, max_damage)
}

/// Check if attacker can attack defender: different factions, a weapon that
/// can hit it, and in range (with the attacker CO's range change, AW Grit, Max)
pub fn can_attack(
    attacker: &Unit,
    attacker_pos: &GridPosition,
    attacker_faction: &FactionMember,
    defender: &Unit,
    defender_pos: &GridPosition,
    defender_faction: &FactionMember,
    range_bonus: i32,
    game_data: &GameData,
) -> bool {
    // Can't attack own units
    if attacker_faction.faction == defender_faction.faction {
        return false;
    }

    // A weapon that can hit this target and has ammo, or a machine gun
    if game_data.weapon(attacker.unit_type, attacker.ammo, defender.unit_type).is_none() {
        return false;
    }

    let distance = attacker_pos.distance_to(defender_pos);
    let (min_range, max_range) = game_data.unit_stats(attacker.unit_type).attack_range;
    let max_range = if min_range > 1 { (max_range as i32 + range_bonus).max(min_range as i32) as u32 } else { max_range };
    distance >= min_range && distance <= max_range
}

/// CO power charge for losing `hp_lost` displayed HP of a unit, as in Advance
/// Wars: by the funds value lost (1 charge per 100 funds). The army that took
/// the hit gets this, the army that dealt it half.
pub fn power_charge(hp_lost: i32, unit_type: UnitType, game_data: &GameData) -> u32 {
    hp_lost.max(0) as u32 * game_data.unit_stats(unit_type).cost / 10 / 100
}

fn process_attacks(
    mut events: MessageReader<AttackEvent>,
    mut commands: Commands,
    mut units: Query<(&mut Unit, &GridPosition, &FactionMember)>,
    map: Res<GameMap>,
    mut commanders: ResMut<Commanders>,
    mut funds: ResMut<super::FactionFunds>,
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
        let attacker_co = commanders.get_bonuses(attacker_faction.faction, &game_data);
        let defender_co = commanders.get_bonuses(defender_faction.faction, &game_data);
        let attacker_range = attacker_co.range(attacker_unit.unit_type, &game_data);
        let defender_range = defender_co.range(defender_unit.unit_type, &game_data);

        // Verify attack is valid
        if !can_attack(&attacker_unit, attacker_pos, attacker_faction, &defender_unit, defender_pos,
            defender_faction, attacker_range, &game_data)
        {
            warn!("Invalid attack!");
            continue;
        }
        let attacker_terrain = map.get(attacker_pos.x, attacker_pos.y).unwrap_or(Terrain::Grass);
        let defender_terrain = map.get(defender_pos.x, defender_pos.y).unwrap_or(Terrain::Grass);
        let attacker_side = (attacker_terrain, &attacker_co, attacker_faction.faction);
        let defender_side = (defender_terrain, &defender_co, defender_faction.faction);
        let can_counter = |defender: &Unit, attacker: &Unit| {
            defender.hp > 0 && can_attack(defender, defender_pos, defender_faction, attacker, attacker_pos,
                attacker_faction, defender_range, &game_data)
        };

        // AW Sonja's Counter Break: the defender strikes back first
        let counter_first = defender_co.mods.counter_first && can_counter(&defender_unit, &attacker_unit);
        if counter_first {
            strike(&mut defender_unit, defender_side, &mut attacker_unit, attacker_side, defender_co.counter(),
                &mut commanders, &mut funds, &weather, &game_data);
        }
        if attacker_unit.hp > 0 {
            strike(&mut attacker_unit, attacker_side, &mut defender_unit, defender_side, 1.0,
                &mut commanders, &mut funds, &weather, &game_data);
            attacker_unit.attacked = true;
            // Counter-attack: the defender survived and can reach the attacker
            // (indirect units never can: they can't fire at adjacent units)
            if !counter_first && can_counter(&defender_unit, &attacker_unit) {
                strike(&mut defender_unit, defender_side, &mut attacker_unit, attacker_side, defender_co.counter(),
                    &mut commanders, &mut funds, &weather, &game_data);
            }
        }

        for (unit, pos, entity) in [(&defender_unit, defender_pos, event.defender), (&attacker_unit, attacker_pos, event.attacker)] {
            if unit.hp <= 0 {
                info!("{} destroyed!", game_data.unit_name(unit.unit_type));
                defeated.write(super::UnitDefeated { position: *pos });
                commands.entity(entity).despawn();
            }
        }
    }
}

/// One side fires at the other: damage (times `multiplier` for counters), ammo,
/// CO power charge and any funds the shooter's CO earns from damage
fn strike(
    shooter: &mut Unit,
    (shooter_terrain, shooter_co, shooter_army): (Terrain, &CoBonuses, super::Faction),
    target: &mut Unit,
    (target_terrain, target_co, target_army): (Terrain, &CoBonuses, super::Faction),
    multiplier: f32,
    commanders: &mut Commanders,
    funds: &mut super::FactionFunds,
    weather: &Weather,
    game_data: &GameData,
) {
    let uses_ammo = game_data.weapon(shooter.unit_type, shooter.ammo, target.unit_type)
        .is_some_and(|(_, uses_ammo)| uses_ammo);
    let damage = (calculate_damage(shooter, shooter_terrain, target, target_terrain, shooter_co, target_co, weather, game_data) as f32
        * multiplier).floor() as i32;
    if uses_ammo {
        shooter.ammo = shooter.ammo.saturating_sub(1);
    }
    let shown = target.display_hp();
    target.hp -= damage;
    let lost = shown - target.display_hp();
    info!("{} hits {} for {} damage (HP {} -> {})", game_data.unit_name(shooter.unit_type),
        game_data.unit_name(target.unit_type), damage, target.hp + damage, target.hp);

    // The side that was hit charges fully, the shooter half
    let charge = power_charge(lost, target.unit_type, game_data);
    commanders.charge(target_army, charge, game_data);
    commanders.charge(shooter_army, charge / 2, game_data);

    // AW Sasha's War Bonds: funds worth part of the damage dealt
    if shooter_co.mods.funds_from_damage > 0 {
        let value = lost.max(0) as u32 * game_data.unit_stats(target.unit_type).cost / 10;
        funds.add(shooter_army, value * shooter_co.mods.funds_from_damage as u32 / 100);
    }
}

fn process_captures(
    mut events: MessageReader<CaptureEvent>,
    units: Query<(&Unit, &FactionMember)>,
    mut tiles: Query<(&mut Tile, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game_data: Res<GameData>,
    commanders: Res<Commanders>,
    mut play: MessageWriter<super::PlaySfx>,
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

        // Advance Wars: each capture removes the unit's displayed HP from the
        // property's 20 points, so a healthy unit takes two turns
        let capture_power = unit.display_hp()
            * commanders.get_bonuses(faction_member.faction, &game_data).capture_percent() / 100;

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
        play.write(super::PlaySfx(if tile.capture_progress >= required { "capture_done" } else { "capture_tick" }));
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

/// Advance Wars: a capture in progress is lost as soon as the capturing unit
/// leaves the property or is destroyed
fn abandon_interrupted_captures(
    mut tiles: Query<&mut Tile>,
    units: Query<(&GridPosition, &FactionMember), With<Unit>>,
) {
    for mut tile in tiles.iter_mut() {
        let Some(capturer) = tile.capturing_faction else { continue };
        let held = units.iter().any(|(pos, faction)| {
            pos.x == tile.position.x && pos.y == tile.position.y && faction.faction == capturer
        });
        if !held {
            tile.capture_progress = 0;
            tile.capturing_faction = None;
        }
    }
}

/// Process unit joining/merging
fn process_joins(
    mut events: MessageReader<JoinEvent>,
    mut commands: Commands,
    mut units: Query<(&mut Unit, &FactionMember)>,
    mut funds: ResMut<super::FactionFunds>,
    game_data: Res<GameData>,
) {
    for event in events.read() {
        let Ok([(source_unit, faction), (mut target_unit, _)]) = units.get_many_mut([event.source, event.target]) else {
            warn!("Failed to get units for join!");
            continue;
        };

        // Both units must be same type (this should have been checked before sending the event)
        if source_unit.unit_type != target_unit.unit_type {
            warn!("Cannot join different unit types!");
            continue;
        }

        let stats = game_data.unit_stats(target_unit.unit_type);

        // Combine HP (capped at max). Advance Wars refunds the displayed HP
        // that didn't fit, at 10% of the unit's cost each
        let combined_hp = (target_unit.hp + source_unit.hp).min(stats.max_hp);
        let excess = (target_unit.display_hp() + source_unit.display_hp() - 10).max(0) as u32;
        if excess > 0 {
            funds.add(faction.faction, excess * stats.cost / 10);
        }

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
        let Ok((_, passenger_unit, _, passenger_faction)) = units.get(event.passenger) else {
            warn!("Failed to get passenger unit!");
            continue;
        };
        let (cargo, faction, passenger_type) =
            (CargoUnit::from_unit(&passenger_unit), passenger_faction.faction, passenger_unit.unit_type);

        // The friendly transport the passenger moved onto, with room for it
        let transport_entity = units.iter()
            .find(|(entity, unit, pos, f)| {
                *entity != event.passenger
                    && (pos.x, pos.y) == event.transport_pos
                    && f.faction == faction
                    && unit.can_carry(passenger_type, &game_data)
            })
            .map(|(entity, ..)| entity);
        let Some(transport_entity) = transport_entity else {
            warn!("No transport with room at {:?}!", event.transport_pos);
            continue;
        };
        let Ok((_, mut transport_unit, _, _)) = units.get_mut(transport_entity) else { continue };
        info!("{} boarded {}", game_data.unit_name(cargo.unit_type), game_data.unit_name(transport_unit.unit_type));
        transport_unit.cargo.push(cargo);
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
        let (ux, uy) = event.position;
        let occupied = units.iter().any(|(_, pos, _)| pos.x == ux && pos.y == uy);
        let Ok((mut transport_unit, _, transport_faction)) = units.get_mut(event.transport) else {
            warn!("Failed to get transport unit!");
            continue;
        };
        let Some(cargo) = transport_unit.cargo.get(event.cargo_index).cloned() else {
            warn!("Transport has no passenger {}!", event.cargo_index);
            continue;
        };
        let passable = map.get(ux, uy).is_some_and(|t| game_data.is_passable(t, game_data.unit_class(cargo.unit_type)));
        if !passable || occupied {
            warn!("Cannot unload onto ({}, {})!", ux, uy);
            continue;
        }
        transport_unit.cargo.remove(event.cargo_index);
        transport_unit.attacked = true;
        let faction = transport_faction.faction;

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

    /// Run one capture by a unit with `hp` (0-100) on a neutral storehouse,
    /// returning the tile afterwards
    fn capture_world(hp: i32) -> (App, Entity, Entity) {
        capture_world_with(hp, None)
    }

    fn capture_world_with(hp: i32, co: Option<crate::game::CommanderId>) -> (App, Entity, Entity) {
        let data = GameData::load_defaults();
        let mut app = App::new();
        app.add_message::<CaptureEvent>()
            .add_message::<super::super::PlaySfx>()
            .insert_resource(Assets::<StandardMaterial>::default())
            .insert_resource({
                let mut commanders = Commanders::default();
                commanders.active.clear();
                if let Some(co) = co {
                    commanders.set_commander(Faction::Western, co);
                }
                commanders
            })
            .add_systems(Update, process_captures);
        let mut scout = Unit::new(UnitType::Scout, &data);
        scout.hp = hp;
        app.insert_resource(data);
        let unit = app.world_mut().spawn((scout, FactionMember { faction: Faction::Western }, GridPosition::new(0, 0))).id();
        let tile = app.world_mut().spawn((
            Tile { terrain: Terrain::Storehouse, position: IVec2::ZERO, owner: None, capture_progress: 0, capturing_faction: None },
            MeshMaterial3d::<StandardMaterial>(Handle::default()),
        )).id();
        (app, unit, tile)
    }

    fn capture(app: &mut App, unit: Entity, tile: Entity) -> Tile {
        app.world_mut().write_message(CaptureEvent { unit, tile });
        app.update();
        app.world().get::<Tile>(tile).unwrap().clone()
    }

    #[test]
    fn tanks_use_machine_guns_on_infantry_and_when_out_of_ammo() {
        let gd = GameData::load_defaults();
        // Cannon vs a vehicle uses ammo; the machine gun vs infantry doesn't
        assert_eq!(gd.weapon(UnitType::Ironclad, 9, UnitType::Recon), Some((85, true)));
        assert_eq!(gd.weapon(UnitType::Ironclad, 9, UnitType::Scout), Some((75, false)));
        // Out of ammo: still shoots infantry, can't hurt vehicles
        assert_eq!(gd.weapon(UnitType::Ironclad, 0, UnitType::Scout), Some((75, false)));
        assert_eq!(gd.weapon(UnitType::Ironclad, 0, UnitType::Recon), None);
        // Infantry has only a machine gun and no ammo to run out of
        assert_eq!(gd.weapon(UnitType::Scout, 0, UnitType::Scout), Some((55, false)));
    }

    #[test]
    fn power_charges_by_funds_value_lost() {
        let gd = GameData::load_defaults();
        // A whole Militia (1000) is worth 10 charge, a whole Charger (7000) 70
        assert_eq!(power_charge(10, UnitType::Scout, &gd), 10);
        assert_eq!(power_charge(10, UnitType::Ironclad, &gd), 70);
        assert_eq!(power_charge(3, UnitType::Ironclad, &gd), 21);
    }

    #[test]
    fn full_health_unit_captures_in_two_turns() {
        let (mut app, unit, tile) = capture_world(100);
        let first = capture(&mut app, unit, tile);
        assert_eq!((first.owner, first.capture_progress), (None, 10), "10 HP takes 10 of the 20 points");
        let second = capture(&mut app, unit, tile);
        assert_eq!(second.owner, Some(Faction::Western));
    }

    #[test]
    fn sami_style_commanders_capture_faster() {
        // Bandit plays like AW Sami: 150% capture, so 10 HP takes 15 points
        let (mut app, unit, tile) = capture_world_with(100, Some(crate::game::CommanderId::Bandit));
        assert_eq!(capture(&mut app, unit, tile).capture_progress, 15);
    }

    #[test]
    fn wounded_unit_captures_slower() {
        let (mut app, unit, tile) = capture_world(45); // shows 5 HP
        for _ in 0..3 {
            assert_eq!(capture(&mut app, unit, tile).owner, None);
        }
        assert_eq!(capture(&mut app, unit, tile).owner, Some(Faction::Western), "5 HP needs four turns");
    }

    #[test]
    fn infantry_vs_infantry_on_plains_matches_aw2() {
        // AW2 reference: full HP infantry vs infantry on plains deals 49-57%.
        // (55 + luck) * 1.0 * (200 - (100 + 1 * 10)) / 100
        let (gd, co, weather) = setup();
        let scout = unit(UnitType::Scout, &gd);
        let (min, max) = estimate_damage(&scout, Terrain::Grass, &scout, Terrain::Grass, &co, &co, &weather, &gd);
        assert_eq!((min, max), (49, 57), "AW floors damage");
    }

    #[test]
    fn damage_table_drives_matchups() {
        let (gd, co, weather) = setup();
        let scout = unit(UnitType::Scout, &gd);
        let tank = unit(UnitType::Ironclad, &gd);
        let (scout_on_tank, _) = estimate_damage(&scout, Terrain::Grass, &tank, Terrain::Log, &co, &co, &weather, &gd);
        let (tank_on_scout, _) = estimate_damage(&tank, Terrain::Grass, &scout, Terrain::Log, &co, &co, &weather, &gd);
        assert_eq!(scout_on_tank, 5);
        assert_eq!(tank_on_scout, 75);
    }

    #[test]
    fn terrain_cover_reduces_damage() {
        let (gd, co, weather) = setup();
        let tank = unit(UnitType::Ironclad, &gd);
        let scout = unit(UnitType::Scout, &gd);
        let (open, _) = estimate_damage(&tank, Terrain::Grass, &scout, Terrain::Log, &co, &co, &weather, &gd);
        let (fortified, _) = estimate_damage(&tank, Terrain::Grass, &scout, Terrain::Base, &co, &co, &weather, &gd);
        assert!(fortified < open, "base ({fortified}) should protect better than a log ({open})");
    }

    #[test]
    fn damaged_attacker_hits_proportionally_weaker() {
        let (gd, co, weather) = setup();
        let mut tank = unit(UnitType::Ironclad, &gd);
        let scout = unit(UnitType::Scout, &gd);
        let (full, _) = estimate_damage(&tank, Terrain::Grass, &scout, Terrain::Log, &co, &co, &weather, &gd);
        tank.hp = 50;
        let (half, _) = estimate_damage(&tank, Terrain::Grass, &scout, Terrain::Log, &co, &co, &weather, &gd);
        assert!((half - full / 2).abs() <= 1, "half HP hits about half as hard ({half} vs {full})");
    }

    #[test]
    fn co_attack_bonus_increases_damage() {
        let (gd, co, weather) = setup();
        let scout = unit(UnitType::Scout, &gd);
        let boosted = CoBonuses {
            mods: crate::game::CoModifiers { attack: vec![(crate::game::UnitFilter::All, 20)], ..Default::default() },
            ..CoBonuses::none()
        };
        let (normal, _) = estimate_damage(&scout, Terrain::Grass, &scout, Terrain::Log, &co, &co, &weather, &gd);
        let (strong, _) = estimate_damage(&scout, Terrain::Grass, &scout, Terrain::Log, &boosted, &co, &weather, &gd);
        assert_eq!(strong, (normal as f32 * 1.2).round() as i32);
    }

    #[test]
    fn weather_never_changes_damage() {
        let (gd, co, clear) = setup();
        let tank = unit(UnitType::Ironclad, &gd);
        let scout = unit(UnitType::Scout, &gd);
        let clear_damage = estimate_damage(&tank, Terrain::Grass, &scout, Terrain::Thicket, &co, &co, &clear, &gd);
        for weather in [WeatherType::Rain, WeatherType::Snow, WeatherType::Sandstorm, WeatherType::Fog] {
            let damage = estimate_damage(&tank, Terrain::Grass, &scout, Terrain::Thicket, &co, &co, &Weather::new(weather), &gd);
            assert_eq!(damage, clear_damage, "{weather:?}");
        }
    }

    #[test]
    fn fliers_get_no_terrain_defense() {
        let (gd, co, weather) = setup();
        let flak = unit(UnitType::Flak, &gd);
        let flier = unit(UnitType::Skywing, &gd);
        let open = estimate_damage(&flak, Terrain::Grass, &flier, Terrain::Grass, &co, &co, &weather, &gd);
        let covered = estimate_damage(&flak, Terrain::Grass, &flier, Terrain::Boulder, &co, &co, &weather, &gd);
        assert_eq!(open, covered);
    }

    #[test]
    fn rolled_damage_stays_within_estimate() {
        let (gd, co, weather) = setup();
        let tank = unit(UnitType::Ironclad, &gd);
        let scout = unit(UnitType::Scout, &gd);
        let (min, max) = estimate_damage(&tank, Terrain::Grass, &scout, Terrain::Grass, &co, &co, &weather, &gd);
        for _ in 0..200 {
            let dmg = calculate_damage(&tank, Terrain::Grass, &scout, Terrain::Grass, &co, &co, &weather, &gd);
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
        assert_eq!(estimate_damage(&scout, Terrain::Grass, &raptor, Terrain::Grass, &co, &co, &weather, &gd), (0, 0));

        let east = FactionMember { faction: Faction::Eastern };
        let north = FactionMember { faction: Faction::Northern };
        let here = GridPosition::new(0, 0);
        let adjacent = GridPosition::new(1, 0);
        assert!(!can_attack(&scout, &here, &east, &raptor, &adjacent, &north, 0, &gd));
    }

    #[test]
    fn can_attack_checks_faction_and_range() {
        let (gd, _, _) = setup();
        let scout = unit(UnitType::Scout, &gd);
        let siege = unit(UnitType::Siege, &gd);
        let east = FactionMember { faction: Faction::Eastern };
        let north = FactionMember { faction: Faction::Northern };
        let origin = GridPosition::new(0, 0);

        assert!(can_attack(&scout, &origin, &east, &scout, &GridPosition::new(1, 0), &north, 0, &gd));
        assert!(!can_attack(&scout, &origin, &east, &scout, &GridPosition::new(1, 0), &east, 0, &gd), "friendly fire");
        assert!(!can_attack(&scout, &origin, &east, &scout, &GridPosition::new(2, 0), &north, 0, &gd), "out of range");

        // Artillery has a minimum range of 2
        assert!(!can_attack(&siege, &origin, &east, &scout, &GridPosition::new(1, 0), &north, 0, &gd));
        assert!(can_attack(&siege, &origin, &east, &scout, &GridPosition::new(2, 1), &north, 0, &gd));
    }
}
