use bevy::prelude::*;
use std::collections::HashMap;

use super::{
    Faction, UnitType, Unit, FactionMember, GridPosition,
    FactionFunds, GameMap, Tile, SpriteAssets, Terrain, UnitClass,
    BattleConfig, GameData, CommanderData, Weather, WeatherType,
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

/// Unique identifier for each CO. Every Advance Wars 2 / Dual Strike CO has a
/// woodland counterpart with the same abilities (see `based_on` in commanders.ron).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CommanderId {
    // Eastern Empire (Yellow Comet)
    Kira,
    Tanuki,
    Sensei,
    Goro,
    // Northern Realm (Blue Moon)
    Grimjaw,
    Frost,
    Bjorn,
    Sable,
    // Western Frontier (Orange Star)
    Bandit,
    Talon,
    Dusty,
    Clover,
    Shell,
    Ridge,
    Hollis,
    // Southern Pride (Green Earth)
    Lionheart,
    Tusker,
    Prowler,
    Rumble,
    // Nether Dominion (Black Hole)
    Burrower,
    Hivemind,
    Dredge,
    Wriggle,
    Carapace,
    Clank,
    Cinder,
    Velvet,
    Vesper,
}

impl CommanderId {
    pub fn all() -> &'static [CommanderId] {
        use CommanderId::*;
        &[
            Kira, Tanuki, Sensei, Goro,
            Grimjaw, Frost, Bjorn, Sable,
            Bandit, Talon, Dusty, Clover, Shell, Ridge, Hollis,
            Lionheart, Tusker, Prowler, Rumble,
            Burrower, Hivemind, Dredge, Wriggle, Carapace, Clank, Cinder, Velvet, Vesper,
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

/// Which units a CO modifier applies to
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum UnitFilter {
    All,
    /// Units that attack adjacent targets (max range 1)
    Direct,
    /// Direct combat units other than foot soldiers (AW Max's specialty)
    DirectNonFoot,
    /// Units that fire from range 2+
    Indirect,
    /// Foot soldiers (infantry and mechs)
    Foot,
    /// Everything but foot soldiers
    NonFoot,
    /// Ground units other than foot soldiers
    Vehicles,
    /// Every ground unit
    Ground,
    Air,
    Naval,
    /// Units that carry others
    Transports,
    /// A specific list of unit types (e.g. copters)
    Units(Vec<UnitType>),
}

impl UnitFilter {
    pub fn matches(&self, unit_type: UnitType, game_data: &GameData) -> bool {
        let stats = game_data.unit_stats(unit_type);
        let foot = matches!(stats.class, UnitClass::Foot | UnitClass::Mech);
        let air = matches!(stats.class, UnitClass::Air | UnitClass::AirTransport);
        let naval = matches!(stats.class, UnitClass::Naval | UnitClass::NavalTransport);
        let direct = stats.attack > 0 && stats.attack_range.1 == 1;
        match self {
            UnitFilter::All => true,
            UnitFilter::Direct => direct,
            UnitFilter::DirectNonFoot => direct && !foot,
            UnitFilter::Indirect => stats.attack_range.0 > 1,
            UnitFilter::Foot => foot,
            UnitFilter::NonFoot => !foot,
            UnitFilter::Vehicles => !foot && !air && !naval,
            UnitFilter::Ground => !air && !naval,
            UnitFilter::Air => air,
            UnitFilter::Naval => naval,
            UnitFilter::Transports => stats.capacity > 0,
            UnitFilter::Units(list) => list.contains(&unit_type),
        }
    }
}

/// Terrain groups some COs fight better on
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TerrainGroup {
    /// Grass and tall grass (AW plains)
    Plains,
    /// Trails (AW roads)
    Roads,
    /// Any capturable building (AW urban terrain)
    Properties,
}

impl TerrainGroup {
    pub fn matches(&self, terrain: Terrain, game_data: &GameData) -> bool {
        match self {
            TerrainGroup::Plains => matches!(terrain, Terrain::Grass | Terrain::TallGrass),
            TerrainGroup::Roads => terrain == Terrain::Log,
            TerrainGroup::Properties => game_data.terrain_capturable(terrain),
        }
    }
}

/// One layer of a CO's abilities: always on (`abilities`), or added while a
/// power is active. Percentages are in Advance Wars points: attack +20 means
/// 120% firepower. Every field is optional in commanders.ron.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CoModifiers {
    pub attack: Vec<(UnitFilter, i32)>,
    pub defense: Vec<(UnitFilter, i32)>,
    pub movement: Vec<(UnitFilter, i32)>,
    /// Extra maximum range
    pub range: Vec<(UnitFilter, i32)>,
    pub vision: Vec<(UnitFilter, i32)>,
    /// Attack bonus for units standing on this terrain
    pub attack_on: Vec<(TerrainGroup, i32)>,
    /// Attack bonus per terrain defense star under the attacker (AW Lash)
    pub attack_per_terrain_star: i32,
    /// Extra terrain defense stars, in percent (100 doubles them)
    pub terrain_stars: i32,
    /// Extra capture speed, in percent (50 = 150%)
    pub capture: i32,
    /// Unit price change, in percent (-20 = 80%)
    pub cost: i32,
    /// Extra good luck (AW luck is 0-9 on top of damage)
    pub luck: i32,
    /// Bad luck: up to this much subtracted from damage
    pub bad_luck: i32,
    /// Extra counter-attack damage, in percent
    pub counter: i32,
    /// Extra funds per owned property each turn
    pub income: i32,
    /// Change to daily stamina (fuel) use
    pub daily_stamina: Vec<(UnitFilter, i32)>,
    /// Weather that doesn't slow or blind these units
    pub weather_proof: Vec<WeatherType>,
    /// Every passable terrain costs 1 to move through
    pub flat_terrain_cost: bool,
    /// The enemy can't see these units' HP (AW Sonja)
    pub hidden_hp: bool,
    /// Units see into forests and lily pads from range
    pub see_hidden: bool,
    /// Defenders strike back before the attacker hits
    pub counter_first: bool,
    /// Funds gained from damage dealt, in percent of its value
    pub funds_from_damage: i32,
    /// Extra defense against indirect fire
    pub defense_vs_indirect: i32,
    /// Storehouses build ground units like workshops
    pub build_on_cities: bool,
    /// Extra HP repaired on properties
    pub repair: i32,
}

impl CoModifiers {
    /// Stack another layer on top of this one
    fn merge(&mut self, other: &CoModifiers) {
        self.attack.extend(other.attack.iter().cloned());
        self.defense.extend(other.defense.iter().cloned());
        self.movement.extend(other.movement.iter().cloned());
        self.range.extend(other.range.iter().cloned());
        self.vision.extend(other.vision.iter().cloned());
        self.attack_on.extend(other.attack_on.iter().cloned());
        self.daily_stamina.extend(other.daily_stamina.iter().cloned());
        self.weather_proof.extend(other.weather_proof.iter().cloned());
        self.attack_per_terrain_star += other.attack_per_terrain_star;
        self.terrain_stars += other.terrain_stars;
        self.capture += other.capture;
        self.cost += other.cost;
        self.luck += other.luck;
        self.bad_luck += other.bad_luck;
        self.counter += other.counter;
        self.income += other.income;
        self.funds_from_damage += other.funds_from_damage;
        self.defense_vs_indirect += other.defense_vs_indirect;
        self.repair += other.repair;
        self.flat_terrain_cost |= other.flat_terrain_cost;
        self.hidden_hp |= other.hidden_hp;
        self.see_hidden |= other.see_hidden;
        self.counter_first |= other.counter_first;
        self.build_on_cities |= other.build_on_cities;
    }
}

fn sum_for(list: &[(UnitFilter, i32)], unit_type: UnitType, game_data: &GameData) -> i32 {
    list.iter().filter(|(f, _)| f.matches(unit_type, game_data)).map(|(_, v)| v).sum()
}

/// Instant effects when a power is activated
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum PowerEffect {
    /// Heal all own units by this many displayed HP
    Heal(i32),
    /// Damage every enemy unit by this many displayed HP (never below 0.1 HP)
    DamageEnemies(i32),
    /// Damage enemy units standing on properties
    DamageEnemiesOnProperties(i32),
    /// Enemy units lose half their stamina (fuel)
    HalveEnemyStamina,
    /// Change the weather until this CO's next turn
    Weather(WeatherType),
    /// Multiply current funds
    MultiplyFunds(f32),
    /// Drain the enemy's power meter by this percent per 5000 funds held
    DrainEnemyPower(i32),
    /// Attack bonus for the power's duration: this percent per 1000 funds held
    AttackPerFunds(f32),
    /// Attack bonus for the power's duration: this percent per owned property
    AttackPerProperty(i32),
    /// Refill every own unit's stamina and ammo
    Resupply,
    /// Units matching the filter may move and act again
    ExtraMove(UnitFilter),
    /// Deploy a unit with this HP (internal, 0-100) on every empty owned storehouse
    Reinforce { unit: UnitType, hp: i32 },
    /// Strike the densest enemy group: `strikes` times, this many HP within `radius`
    Meteor { damage: i32, radius: i32, strikes: u32, paralyze: bool },
}

/// Which of a CO's powers is running
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PowerLevel {
    #[default]
    None,
    /// CO Power
    Power,
    /// Super CO Power
    Super,
}

/// Power meter charge per star (9000 funds of damage taken, as in Advance Wars,
/// at 1 charge per 100 funds)
pub const CHARGE_PER_STAR: u32 = 90;

// ============================================================================
// ACTIVE COMMANDER STATE
// ============================================================================

/// Resource tracking active commanders and power meters
#[derive(Resource)]
pub struct Commanders {
    /// Active CO per faction
    pub active: HashMap<Faction, CommanderId>,
    /// Power meter per faction
    pub power_meter: HashMap<Faction, u32>,
    /// Which power is running this turn
    pub power_active: HashMap<Faction, PowerLevel>,
    /// How often each faction has used a power (each use makes stars 20% dearer)
    pub power_uses: HashMap<Faction, u32>,
    /// Attack bonus worked out when a power started (funds or property based)
    pub power_attack: HashMap<Faction, i32>,
}

impl Default for Commanders {
    fn default() -> Self {
        let active = HashMap::from([
            (Faction::Eastern, CommanderId::Kira),
            (Faction::Northern, CommanderId::Grimjaw),
            (Faction::Western, CommanderId::Bandit),
            (Faction::Southern, CommanderId::Lionheart),
            (Faction::Nether, CommanderId::Burrower),
        ]);
        Self {
            active,
            power_meter: HashMap::new(),
            power_active: HashMap::new(),
            power_uses: HashMap::new(),
            power_attack: HashMap::new(),
        }
    }
}

impl Commanders {
    /// Set the active CO for a faction
    pub fn set_commander(&mut self, faction: Faction, co: CommanderId) {
        self.active.insert(faction, co);
        self.power_meter.insert(faction, 0);
        self.power_active.insert(faction, PowerLevel::None);
        self.power_uses.insert(faction, 0);
    }

    /// Get the active commander ID for a faction
    pub fn get_active(&self, faction: Faction) -> CommanderId {
        self.active.get(&faction).copied().unwrap_or(CommanderId::Kira)
    }

    /// Get the active commander's data for a faction
    pub fn get_commander<'a>(&self, faction: Faction, game_data: &'a GameData) -> Option<&'a CommanderData> {
        self.active.get(&faction).map(|&id| game_data.commander(id))
    }

    /// Charge one star costs now (AW: +20% per power used, up to +200%)
    fn star_cost(&self, faction: Faction) -> u32 {
        let uses = self.power_uses.get(&faction).copied().unwrap_or(0).min(10);
        CHARGE_PER_STAR * (5 + uses) / 5
    }

    /// Charge a power needs (None if the CO has no such power)
    pub fn power_cost(&self, faction: Faction, level: PowerLevel, game_data: &GameData) -> Option<u32> {
        let co = self.get_commander(faction, game_data)?;
        let stars = match level {
            PowerLevel::Power => co.power.as_ref()?.stars,
            PowerLevel::Super => co.super_power.stars,
            PowerLevel::None => return None,
        };
        Some(stars * self.star_cost(faction))
    }

    /// Add charge to power meter (from dealing/taking damage). Charging stops
    /// while a power runs, and the meter holds at most the Super CO Power.
    pub fn charge(&mut self, faction: Faction, amount: u32, game_data: &GameData) {
        if self.active_power(faction) != PowerLevel::None {
            return;
        }
        let Some(full) = self.power_cost(faction, PowerLevel::Super, game_data) else { return };
        let current = self.get_charge(faction);
        self.power_meter.insert(faction, (current + amount).min(full));
    }

    /// Get current power charge
    pub fn get_charge(&self, faction: Faction) -> u32 {
        self.power_meter.get(&faction).copied().unwrap_or(0)
    }

    /// Whether a power can be activated now
    pub fn can_activate(&self, faction: Faction, level: PowerLevel, game_data: &GameData) -> bool {
        self.active_power(faction) == PowerLevel::None
            && self.power_cost(faction, level, game_data).is_some_and(|cost| self.get_charge(faction) >= cost)
    }

    /// Which power is running for a faction
    pub fn active_power(&self, faction: Faction) -> PowerLevel {
        self.power_active.get(&faction).copied().unwrap_or_default()
    }

    /// Activate a power: spends its charge and returns its instant effects
    pub fn activate_power(&mut self, faction: Faction, level: PowerLevel, game_data: &GameData) -> Option<Vec<PowerEffect>> {
        if !self.can_activate(faction, level, game_data) {
            return None;
        }
        let cost = self.power_cost(faction, level, game_data)?;
        let co = self.get_commander(faction, game_data)?;
        let power = if level == PowerLevel::Super { &co.super_power } else { co.power.as_ref()? };
        info!("{} activated {}!", co.name, power.name);
        let effects = power.effects.clone();
        self.power_meter.insert(faction, self.get_charge(faction) - cost);
        self.power_active.insert(faction, level);
        *self.power_uses.entry(faction).or_default() += 1;
        Some(effects)
    }

    /// Clear power active state (called at end of turn)
    pub fn clear_power(&mut self, faction: Faction) {
        self.power_active.insert(faction, PowerLevel::None);
        self.power_attack.remove(&faction);
    }

    /// The faction's CO abilities right now: always-on plus the running power
    pub fn get_bonuses(&self, faction: Faction, game_data: &GameData) -> CoBonuses {
        let Some(co) = self.get_commander(faction, game_data) else {
            return CoBonuses::none();
        };
        let mut mods = co.abilities.clone();
        let level = self.active_power(faction);
        let power = match level {
            PowerLevel::Power => co.power.as_ref(),
            PowerLevel::Super => Some(&co.super_power),
            PowerLevel::None => None,
        };
        if let Some(power) = power {
            mods.merge(&power.modifiers);
        }
        CoBonuses {
            mods,
            powered: level != PowerLevel::None,
            power_attack: self.power_attack.get(&faction).copied().unwrap_or(0),
        }
    }
}

/// A faction's CO abilities in effect (always-on plus any running power).
/// Queries take the unit (and terrain) they're about, since AW COs boost some
/// unit types and not others.
#[derive(Clone, Debug, Default)]
pub struct CoBonuses {
    pub mods: CoModifiers,
    /// A power is running: every unit gets +10 attack and +10 defense (AW)
    pub powered: bool,
    /// Attack bonus fixed when the power started (funds or property based)
    pub power_attack: i32,
}

impl CoBonuses {
    /// No CO: plain Advance Wars stats
    pub fn none() -> Self {
        Self::default()
    }

    fn power_boost(&self) -> i32 {
        if self.powered { 10 } else { 0 }
    }

    /// Firepower multiplier (1.0 = 100%) for a unit on `terrain`
    pub fn attack(&self, unit_type: UnitType, terrain: Terrain, game_data: &GameData) -> f32 {
        let on_terrain: i32 = self.mods.attack_on.iter()
            .filter(|(group, _)| group.matches(terrain, game_data))
            .map(|(_, v)| v)
            .sum();
        let stars = self.mods.attack_per_terrain_star * game_data.terrain_defense(terrain) as i32;
        let total = 100 + sum_for(&self.mods.attack, unit_type, game_data) + on_terrain + stars
            + self.power_boost() + self.power_attack;
        total.max(0) as f32 / 100.0
    }

    /// Defense multiplier (1.0 = 100%); `vs_indirect` when the attacker fires from range
    pub fn defense(&self, unit_type: UnitType, vs_indirect: bool, game_data: &GameData) -> f32 {
        let indirect = if vs_indirect { self.mods.defense_vs_indirect } else { 0 };
        let total = 100 + sum_for(&self.mods.defense, unit_type, game_data) + self.power_boost() + indirect;
        total.max(0) as f32 / 100.0
    }

    /// Terrain defense stars for this CO's units
    pub fn terrain_stars(&self, stars: u32) -> f32 {
        stars as f32 * (100 + self.mods.terrain_stars) as f32 / 100.0
    }

    pub fn movement(&self, unit_type: UnitType, game_data: &GameData) -> i32 {
        sum_for(&self.mods.movement, unit_type, game_data)
    }

    /// A unit's movement after its CO and the weather
    pub fn total_movement(&self, unit_type: UnitType, weather: &Weather, game_data: &GameData) -> u32 {
        let base = (game_data.unit_stats(unit_type).movement as i32 + self.movement(unit_type, game_data)).max(1) as u32;
        if self.mods.weather_proof.contains(&weather.current) { base } else { weather.apply_movement(base) }
    }

    pub fn range(&self, unit_type: UnitType, game_data: &GameData) -> i32 {
        sum_for(&self.mods.range, unit_type, game_data)
    }

    /// A unit's vision after its CO and the weather
    pub fn total_vision(&self, unit_type: UnitType, weather: &Weather, game_data: &GameData) -> u32 {
        let base = (game_data.unit_stats(unit_type).vision as i32 + sum_for(&self.mods.vision, unit_type, game_data)).max(1) as u32;
        if self.mods.weather_proof.contains(&weather.current) { base } else { weather.apply_vision(base) }
    }

    /// Capture speed in percent
    pub fn capture_percent(&self) -> i32 {
        (100 + self.mods.capture).max(0)
    }

    /// Unit price multiplier
    pub fn cost(&self) -> f32 {
        (100 + self.mods.cost).max(10) as f32 / 100.0
    }

    /// Highest good-luck roll (AW: 9)
    pub fn luck(&self) -> i32 {
        (9 + self.mods.luck).max(0)
    }

    /// Highest bad-luck roll
    pub fn bad_luck(&self) -> i32 {
        self.mods.bad_luck.max(0)
    }

    /// Counter-attack damage multiplier
    pub fn counter(&self) -> f32 {
        (100 + self.mods.counter) as f32 / 100.0
    }

    /// Change to a unit's daily stamina (fuel) use
    pub fn daily_stamina(&self, unit_type: UnitType, game_data: &GameData) -> i32 {
        sum_for(&self.mods.daily_stamina, unit_type, game_data)
    }
}

// ============================================================================
// EVENTS
// ============================================================================

/// Event fired when a CO power is activated
#[derive(Message)]
pub struct PowerActivatedEvent {
    pub faction: Faction,
    pub level: PowerLevel,
    pub effects: Vec<PowerEffect>,
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// Powers last one turn: clear them when the faction's turn ends
fn clear_power_at_turn_end(
    mut commanders: ResMut<Commanders>,
    turn_state: Res<super::TurnState>,
    mut last_faction: Local<Option<Faction>>,
) {
    let current = turn_state.current_faction;
    if let Some(last) = *last_faction {
        if last != current {
            commanders.clear_power(last);
        }
    }
    *last_faction = Some(current);
}

/// Damage a unit by displayed HP; powers never destroy (AW leaves 0.1 HP)
fn power_damage(unit: &mut Unit, hp: i32) {
    unit.hp = (unit.hp - hp * 10).max(1);
}

/// Apply a power's instant effects when it's activated
fn apply_power_effects(
    mut events: MessageReader<PowerActivatedEvent>,
    mut units: Query<(&mut Unit, &FactionMember, &GridPosition)>,
    mut funds: ResMut<FactionFunds>,
    mut commanders: ResMut<Commanders>,
    mut weather: ResMut<Weather>,
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
        let faction = event.faction;
        let enemy = battle_config.next_faction(faction);
        info!("{:?} power ({:?}): {} effects", faction, event.level, event.effects.len());
        for effect in &event.effects {
            match effect {
                PowerEffect::Heal(hp) => {
                    for (mut unit, f, _) in units.iter_mut().filter(|(_, f, _)| f.faction == faction) {
                        let max = game_data.unit_stats(unit.unit_type).max_hp;
                        unit.hp = (unit.hp + hp * 10).min(max);
                        let _ = f;
                    }
                }
                PowerEffect::DamageEnemies(hp) => {
                    for (mut unit, _, _) in units.iter_mut().filter(|(_, f, _)| f.faction != faction) {
                        power_damage(&mut unit, *hp);
                    }
                }
                PowerEffect::DamageEnemiesOnProperties(hp) => {
                    let properties: std::collections::HashSet<(i32, i32)> = tiles.iter()
                        .filter(|t| game_data.terrain_capturable(t.terrain))
                        .map(|t| (t.position.x, t.position.y))
                        .collect();
                    for (mut unit, _, pos) in units.iter_mut().filter(|(_, f, _)| f.faction != faction) {
                        if properties.contains(&(pos.x, pos.y)) {
                            power_damage(&mut unit, *hp);
                        }
                    }
                }
                PowerEffect::HalveEnemyStamina => {
                    for (mut unit, _, _) in units.iter_mut().filter(|(_, f, _)| f.faction != faction) {
                        unit.stamina /= 2;
                    }
                }
                PowerEffect::Weather(kind) => {
                    weather.set(*kind);
                    weather.turns_remaining = 2; // until this CO's next turn
                }
                PowerEffect::MultiplyFunds(factor) => {
                    let current = funds.get(faction);
                    funds.add(faction, (current as f32 * (factor - 1.0)).round() as u32);
                }
                PowerEffect::DrainEnemyPower(percent_per_5000) => {
                    let full = commanders.power_cost(enemy, PowerLevel::Super, &game_data).unwrap_or(0);
                    let drain = full * (funds.get(faction) / 5000) * (*percent_per_5000 as u32) / 100;
                    let left = commanders.get_charge(enemy).saturating_sub(drain);
                    commanders.power_meter.insert(enemy, left);
                }
                PowerEffect::AttackPerFunds(percent_per_1000) => {
                    let bonus = (funds.get(faction) as f32 / 1000.0 * percent_per_1000).round() as i32;
                    *commanders.power_attack.entry(faction).or_default() += bonus;
                }
                PowerEffect::AttackPerProperty(percent) => {
                    let owned = tiles.iter().filter(|t| t.owner == Some(faction)).count() as i32;
                    *commanders.power_attack.entry(faction).or_default() += owned * percent;
                }
                PowerEffect::Resupply => {
                    for (mut unit, _, _) in units.iter_mut().filter(|(_, f, _)| f.faction == faction) {
                        let stats = game_data.unit_stats(unit.unit_type);
                        unit.stamina = stats.max_stamina;
                        unit.ammo = stats.max_ammo;
                    }
                }
                PowerEffect::ExtraMove(filter) => {
                    for (mut unit, _, _) in units.iter_mut().filter(|(_, f, _)| f.faction == faction) {
                        if filter.matches(unit.unit_type, &game_data) {
                            unit.moved = false;
                            unit.attacked = false;
                            unit.exhausted = false;
                        }
                    }
                }
                PowerEffect::Reinforce { unit: unit_type, hp } => {
                    let occupied: std::collections::HashSet<(i32, i32)> = units.iter().map(|(_, _, p)| (p.x, p.y)).collect();
                    for tile in tiles.iter() {
                        let spot = (tile.position.x, tile.position.y);
                        if tile.owner == Some(faction) && tile.terrain == Terrain::Storehouse && !occupied.contains(&spot) {
                            let mut unit = Unit::new(*unit_type, &game_data);
                            unit.hp = *hp;
                            super::spawn_unit_with_state(&mut commands, &map, &mut meshes, &mut materials,
                                &sprite_assets, &images, &game_data, faction, unit, spot.0, spot.1);
                        }
                    }
                }
                PowerEffect::Meteor { damage, radius, strikes, paralyze } => {
                    for _ in 0..*strikes {
                        // Aim where the most enemy value would be lost
                        let targets: Vec<(i32, i32, i32, u32)> = units.iter()
                            .filter(|(_, f, _)| f.faction != faction)
                            .map(|(u, _, p)| (p.x, p.y, u.hp, game_data.unit_stats(u.unit_type).cost))
                            .collect();
                        let value_at = |cx: i32, cy: i32| -> u32 {
                            targets.iter()
                                .filter(|(x, y, ..)| (x - cx).abs() + (y - cy).abs() <= *radius)
                                .map(|(_, _, hp, cost)| ((*damage * 10).min(hp - 1).max(0) as u32) * cost / 100)
                                .sum()
                        };
                        let Some((cx, cy)) = targets.iter().map(|(x, y, ..)| (*x, *y)).max_by_key(|&(x, y)| value_at(x, y)) else { break };
                        for (mut unit, _, pos) in units.iter_mut().filter(|(_, f, _)| f.faction != faction) {
                            if (pos.x - cx).abs() + (pos.y - cy).abs() <= *radius {
                                power_damage(&mut unit, *damage);
                                if *paralyze {
                                    unit.paralyzed = true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_commander_means_plain_stats() {
        let data = GameData::load_defaults();
        let mut commanders = Commanders::default();
        commanders.active.clear();
        let co = commanders.get_bonuses(Faction::Eastern, &data);
        assert_eq!(co.attack(UnitType::Scout, Terrain::Grass, &data), 1.0);
        assert_eq!(co.defense(UnitType::Scout, false, &data), 1.0);
        assert_eq!((co.cost(), co.luck(), co.bad_luck(), co.capture_percent()), (1.0, 9, 0, 100));
    }

    #[test]
    fn every_aw_co_has_a_counterpart_with_two_powers() {
        let data = GameData::load_defaults();
        assert_eq!(CommanderId::all().len(), 28);
        for &id in CommanderId::all() {
            let co = data.commander(id);
            assert!(!co.based_on.is_empty(), "{id:?} names the AW CO it plays like");
            assert!(co.super_power.stars > co.power.as_ref().map_or(0, |p| p.stars), "{id:?}");
        }
    }

    /// A small battle: Western (the CO under test) against Northern, with a few
    /// wounded units of each and an empty storehouse, run through the power system
    fn power_world(co: CommanderId) -> App {
        let data = GameData::load_defaults();
        let mut app = App::new();
        let mut commanders = Commanders::default();
        commanders.set_commander(Faction::Western, co);
        commanders.set_commander(Faction::Northern, CommanderId::Frost);
        app.add_message::<PowerActivatedEvent>()
            .insert_resource(commanders)
            .insert_resource(super::super::BattleConfig::new(Faction::Western, Faction::Northern))
            .insert_resource(FactionFunds::default())
            .insert_resource(Weather::new(WeatherType::Clear))
            .insert_resource(GameMap { width: 8, height: 8, tiles: vec![vec![Terrain::Grass; 8]; 8] })
            .insert_resource(SpriteAssets::default())
            .insert_resource(Assets::<Image>::default())
            .insert_resource(Assets::<Mesh>::default())
            .insert_resource(Assets::<StandardMaterial>::default())
            .add_systems(Update, apply_power_effects);
        let mut spawn = |unit_type, faction, x, y, hp| {
            let mut unit = Unit::new(unit_type, &data);
            unit.hp = hp;
            app.world_mut().spawn((unit, FactionMember { faction }, GridPosition::new(x, y)));
        };
        spawn(UnitType::Scout, Faction::Western, 0, 0, 50);
        spawn(UnitType::Ironclad, Faction::Western, 1, 0, 50);
        spawn(UnitType::Scout, Faction::Northern, 5, 5, 100);
        spawn(UnitType::Ironclad, Faction::Northern, 5, 6, 100);
        app.world_mut().spawn(super::super::Tile {
            terrain: Terrain::Storehouse, position: IVec2::new(3, 3), owner: Some(Faction::Western),
            capture_progress: 0, capturing_faction: None,
        });
        app.world_mut().resource_mut::<FactionFunds>().add(Faction::Western, 10_000);
        app.insert_resource(data);
        app
    }

    /// Activate a power with a full meter and run its effects
    fn fire(app: &mut App, level: PowerLevel) -> bool {
        let effects = app.world_mut().resource_scope(|world, mut commanders: Mut<Commanders>| {
            commanders.power_meter.insert(Faction::Western, 100_000);
            commanders.activate_power(Faction::Western, level, world.resource::<GameData>())
        });
        let Some(effects) = effects else { return false };
        app.world_mut().write_message(PowerActivatedEvent { faction: Faction::Western, level, effects });
        app.update();
        true
    }

    fn hp_of(app: &mut App, faction: Faction) -> Vec<i32> {
        let mut query = app.world_mut().query::<(&Unit, &FactionMember)>();
        query.iter(app.world()).filter(|(_, f)| f.faction == faction).map(|(u, _)| u.hp).collect()
    }

    #[test]
    fn every_power_of_every_commander_runs() {
        for &id in CommanderId::all() {
            for level in [PowerLevel::Power, PowerLevel::Super] {
                let mut app = power_world(id);
                let has_power = level == PowerLevel::Super || app.world().resource::<GameData>().commander(id).power.is_some();
                assert_eq!(fire(&mut app, level), has_power, "{id:?} {level:?}");
            }
        }
    }

    #[test]
    fn power_effects_change_the_battle() {
        // Andy-like heal: +2 HP to own units
        let mut app = power_world(CommanderId::Dusty);
        fire(&mut app, PowerLevel::Power);
        assert!(hp_of(&mut app, Faction::Western).iter().all(|&hp| hp == 70));

        // Hawke-like drain: enemies -2, own +2
        let mut app = power_world(CommanderId::Carapace);
        fire(&mut app, PowerLevel::Super);
        assert!(hp_of(&mut app, Faction::Northern).iter().all(|&hp| hp == 80));
        assert!(hp_of(&mut app, Faction::Western).iter().all(|&hp| hp == 70));

        // Sturm-like meteor: 8 HP around the enemy group, never below 0.1 HP
        let mut app = power_world(CommanderId::Hivemind);
        fire(&mut app, PowerLevel::Super);
        assert!(hp_of(&mut app, Faction::Northern).iter().all(|&hp| hp == 20));

        // Olaf-like snow
        let mut app = power_world(CommanderId::Frost);
        fire(&mut app, PowerLevel::Power);
        assert_eq!(app.world().resource::<Weather>().current, WeatherType::Snow);

        // Colin-like funds x1.5
        let mut app = power_world(CommanderId::Bjorn);
        fire(&mut app, PowerLevel::Power);
        assert_eq!(app.world().resource::<FactionFunds>().get(Faction::Western), 15_000);

        // Sensei-like reinforcements on the empty storehouse
        let mut app = power_world(CommanderId::Sensei);
        fire(&mut app, PowerLevel::Power);
        app.update();
        assert_eq!(hp_of(&mut app, Faction::Western).len(), 3);

        // Von Bolt-like paralysis
        let mut app = power_world(CommanderId::Vesper);
        fire(&mut app, PowerLevel::Super);
        let mut query = app.world_mut().query::<(&Unit, &FactionMember)>();
        assert!(query.iter(app.world()).filter(|(_, f)| f.faction == Faction::Northern).all(|(u, _)| u.paralyzed));
    }

    #[test]
    fn max_hits_harder_up_close_and_weaker_from_range() {
        let data = GameData::load_defaults();
        let mut commanders = Commanders::default();
        commanders.set_commander(Faction::Western, CommanderId::Talon); // plays like Max
        let co = commanders.get_bonuses(Faction::Western, &data);
        assert_eq!(co.attack(UnitType::Ironclad, Terrain::Grass, &data), 1.2);
        assert_eq!(co.attack(UnitType::Scout, Terrain::Grass, &data), 1.0, "not his foot soldiers");
        assert_eq!(co.attack(UnitType::Siege, Terrain::Grass, &data), 0.9);
        assert_eq!(co.range(UnitType::Siege, &data), -1);
    }

    #[test]
    fn powers_charge_in_stars_and_add_the_ten_percent_boost() {
        let data = GameData::load_defaults();
        let mut commanders = Commanders::default();
        commanders.set_commander(Faction::Western, CommanderId::Dusty); // plays like Andy: 3 and 6 stars
        let power = commanders.power_cost(Faction::Western, PowerLevel::Power, &data).unwrap();
        let full = commanders.power_cost(Faction::Western, PowerLevel::Super, &data).unwrap();
        assert_eq!((power, full), (3 * CHARGE_PER_STAR, 6 * CHARGE_PER_STAR));

        commanders.charge(Faction::Western, 10_000, &data);
        assert_eq!(commanders.get_charge(Faction::Western), full, "the meter holds the Super CO Power");
        let effects = commanders.activate_power(Faction::Western, PowerLevel::Power, &data).unwrap();
        assert!(matches!(effects[..], [PowerEffect::Heal(2)]));
        assert_eq!(commanders.get_charge(Faction::Western), full - power);
        let co = commanders.get_bonuses(Faction::Western, &data);
        assert_eq!(co.attack(UnitType::Scout, Terrain::Grass, &data), 1.1);
        assert_eq!(co.defense(UnitType::Scout, false, &data), 1.1);
        assert!(!commanders.can_activate(Faction::Western, PowerLevel::Super, &data), "one power per turn");

        commanders.clear_power(Faction::Western);
        assert_eq!(commanders.get_bonuses(Faction::Western, &data).attack(UnitType::Scout, Terrain::Grass, &data), 1.0);
        let next = commanders.power_cost(Faction::Western, PowerLevel::Power, &data).unwrap();
        assert_eq!(next, 3 * CHARGE_PER_STAR * 6 / 5, "each use makes stars 20% dearer");
    }
}
