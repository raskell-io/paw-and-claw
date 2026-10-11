//! Modding system for Paw & Claw
//!
//! Loads game data from RON files, allowing customization of:
//! - Faction names, colors, descriptions
//! - Unit names, stats, descriptions
//! - Terrain names, properties, colors
//! - Commander names, abilities, stats
//!
//! For WASM builds, default data is embedded at compile time.
//! For native builds, data is loaded from filesystem and can be overridden by mods.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{Faction, UnitType, UnitClass, Terrain, CommanderId, AiPersonality, PowerEffect};

pub struct ModdingPlugin;

impl Plugin for ModdingPlugin {
    fn build(&self, app: &mut App) {
        let mut data = GameData::load_defaults();
        data.load_mods();
        if let Err(problems) = data.validate() {
            for problem in &problems {
                error!("Game data: {}", problem);
            }
            panic!("Game data is incomplete ({} problem(s)), see log above", problems.len());
        }
        app.insert_resource(data);
    }
}

// ============================================================================
// EMBEDDED DEFAULT DATA (for WASM builds)
// ============================================================================

/// Default factions data embedded at compile time
const DEFAULT_FACTIONS_RON: &str = include_str!("../../assets/data/factions.ron");

/// Default units data embedded at compile time
const DEFAULT_UNITS_RON: &str = include_str!("../../assets/data/units.ron");

/// Default terrain data embedded at compile time
const DEFAULT_TERRAIN_RON: &str = include_str!("../../assets/data/terrain.ron");

/// Default commanders data embedded at compile time
const DEFAULT_COMMANDERS_RON: &str = include_str!("../../assets/data/commanders.ron");

/// Default damage tables embedded at compile time
const DEFAULT_DAMAGE_TABLES_RON: &str = include_str!("../../assets/data/damage_tables.ron");

/// Default movement costs embedded at compile time
const DEFAULT_MOVEMENT_COSTS_RON: &str = include_str!("../../assets/data/movement_costs.ron");

// ============================================================================
// DATA STRUCTURES
// ============================================================================

/// Moddable faction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactionData {
    pub name: String,
    pub description: String,
    /// RGB color values (0.0-1.0)
    pub color: [f32; 3],
    /// Representative animals for flavor
    pub animals: Vec<String>,
    /// Asset folder name for sprites
    pub asset_folder: String,
}

/// Moddable unit data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitData {
    pub name: String,
    pub description: String,
    /// Single character symbol for map display
    pub symbol: char,
    /// Asset filename (without extension)
    pub asset_name: String,
    /// Unit statistics
    pub stats: UnitStatsData,
}

/// Unit statistics that can be modified
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitStatsData {
    pub max_hp: i32,
    pub attack: i32,
    pub defense: i32,
    pub movement: u32,
    /// (min_range, max_range)
    pub attack_range: (u32, u32),
    pub vision: u32,
    pub can_capture: bool,
    pub cost: u32,
    pub class: UnitClass,
    pub max_stamina: u32,
    pub max_ammo: u32,
}

/// Moddable terrain data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainData {
    pub name: String,
    pub description: String,
    /// Defense bonus (0-4 typically)
    pub defense: u32,
    /// Can be captured by foot units
    pub capturable: bool,
    /// Capture points required (if capturable)
    pub capture_points: u32,
    /// Income generated per turn (if owned)
    pub income: u32,
    /// RGB color for procedural rendering
    pub color: [f32; 3],
    /// Height for 3D features (0 = flat ground)
    pub feature_height: f32,
    /// Tile height offset for 3D rendering
    pub tile_height: f32,
    /// Asset filename (without extension)
    pub asset_name: String,
    /// Unit classes an owned, empty tile of this terrain can build (Advance
    /// Wars: factories build ground units, ports boats, airports aircraft)
    #[serde(default)]
    pub builds: Vec<UnitClass>,
    /// Unit classes an owned tile of this terrain heals and resupplies
    #[serde(default)]
    pub resupplies: Vec<UnitClass>,
}

/// Moddable CO power data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoPowerData {
    pub name: String,
    pub description: String,
    pub effect: PowerEffect,
}

/// Moddable commander data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommanderData {
    pub name: String,
    pub faction: Faction,
    pub personality: AiPersonality,
    pub description: String,
    /// Attack multiplier (1.0 = normal)
    pub attack_bonus: f32,
    /// Defense multiplier (1.0 = normal)
    pub defense_bonus: f32,
    /// Movement bonus (0 = normal)
    pub movement_bonus: i32,
    /// Income multiplier (1.0 = normal)
    pub income_bonus: f32,
    /// Vision bonus (0 = normal)
    pub vision_bonus: u32,
    /// Unit cost multiplier (1.0 = normal)
    pub cost_modifier: f32,
    /// Terrain movement cost reduction
    pub terrain_cost_reduction: u32,
    /// CO Power definition
    pub power: CoPowerData,
    /// Power meter cost to activate
    pub power_cost: u32,
}

// ============================================================================
// DATA REGISTRIES
// ============================================================================

/// Container for all faction data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FactionsRegistry {
    pub factions: HashMap<String, FactionData>,
}

/// Container for all unit data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UnitsRegistry {
    pub units: HashMap<String, UnitData>,
}

/// Container for all terrain data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerrainRegistry {
    pub terrain: HashMap<String, TerrainData>,
}

/// Container for all commander data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandersRegistry {
    pub commanders: HashMap<String, CommanderData>,
}

/// Container for damage tables (unit vs unit base damage percentages)
/// The outer key is the attacker unit type, inner key is defender unit type
/// Values are base damage percentages (0-100+, where 100 = can one-shot at full HP)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DamageTablesRegistry {
    pub tables: HashMap<String, HashMap<String, u32>>,
}

/// Container for movement costs (terrain vs unit class)
/// The outer key is terrain type, inner key is unit class
/// Values are movement point costs (99 = impassable)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MovementCostsRegistry {
    pub costs: HashMap<String, HashMap<String, u32>>,
}

// ============================================================================
// GAME DATA RESOURCE
// ============================================================================

/// Central resource containing all loaded game data
#[derive(Resource)]
pub struct GameData {
    pub factions: FactionsRegistry,
    pub units: UnitsRegistry,
    pub terrain: TerrainRegistry,
    pub commanders: CommandersRegistry,
    pub damage_tables: DamageTablesRegistry,
    pub movement_costs: MovementCostsRegistry,
}

impl GameData {
    /// Load default data from embedded RON files
    pub fn load_defaults() -> Self {
        let factions = ron::from_str(DEFAULT_FACTIONS_RON)
            .expect("Failed to parse embedded factions.ron");
        let units = ron::from_str(DEFAULT_UNITS_RON)
            .expect("Failed to parse embedded units.ron");
        let terrain = ron::from_str(DEFAULT_TERRAIN_RON)
            .expect("Failed to parse embedded terrain.ron");
        let commanders = ron::from_str(DEFAULT_COMMANDERS_RON)
            .expect("Failed to parse embedded commanders.ron");
        let damage_tables = ron::from_str(DEFAULT_DAMAGE_TABLES_RON)
            .expect("Failed to parse embedded damage_tables.ron");
        let movement_costs = ron::from_str(DEFAULT_MOVEMENT_COSTS_RON)
            .expect("Failed to parse embedded movement_costs.ron");

        Self {
            factions,
            units,
            terrain,
            commanders,
            damage_tables,
            movement_costs,
        }
    }

    /// Load mods from the mods directory (native builds only).
    /// Each mod is a subdirectory whose RON files override or add entries.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_mods(&mut self) {
        use std::fs;
        use std::path::Path;

        let mods_dir = Path::new("mods");
        if !mods_dir.exists() {
            info!("No mods directory found, using default data");
            return;
        }

        let Ok(entries) = fs::read_dir(mods_dir) else {
            warn!("Failed to read mods directory");
            return;
        };

        // Sort so override order is deterministic across platforms
        let mut mod_dirs: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        mod_dirs.sort();

        for path in &mod_dirs {
            let mod_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
            info!("Loading mod: {}", mod_name);

            merge_mod_file(&path.join("factions.ron"), |r: FactionsRegistry| {
                self.factions.factions.extend(r.factions)
            });
            merge_mod_file(&path.join("units.ron"), |r: UnitsRegistry| {
                self.units.units.extend(r.units)
            });
            merge_mod_file(&path.join("terrain.ron"), |r: TerrainRegistry| {
                self.terrain.terrain.extend(r.terrain)
            });
            merge_mod_file(&path.join("commanders.ron"), |r: CommandersRegistry| {
                self.commanders.commanders.extend(r.commanders)
            });
            // Tables merge per row so a mod can tweak a single matchup
            merge_mod_file(&path.join("damage_tables.ron"), |r: DamageTablesRegistry| {
                for (attacker, row) in r.tables {
                    self.damage_tables.tables.entry(attacker).or_default().extend(row);
                }
            });
            merge_mod_file(&path.join("movement_costs.ron"), |r: MovementCostsRegistry| {
                for (terrain, row) in r.costs {
                    self.movement_costs.costs.entry(terrain).or_default().extend(row);
                }
            });
        }

        info!("Loaded {} mod(s)", mod_dirs.len());
    }

    /// No-op for WASM builds
    #[cfg(target_arch = "wasm32")]
    pub fn load_mods(&mut self) {}

    /// Check that every enum variant has data. Accessors rely on this,
    /// so it must pass before the resource is used.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();
        for &faction in Faction::all() {
            if !self.factions.factions.contains_key(faction_to_key(faction)) {
                problems.push(format!("missing faction '{}'", faction_to_key(faction)));
            }
        }
        for &unit_type in UnitType::all() {
            if !self.units.units.contains_key(unit_type_to_key(unit_type)) {
                problems.push(format!("missing unit '{}'", unit_type_to_key(unit_type)));
            }
        }
        for &terrain in Terrain::all() {
            let key = terrain_to_key(terrain);
            if !self.terrain.terrain.contains_key(key) {
                problems.push(format!("missing terrain '{}'", key));
            }
            for &class in UnitClass::all() {
                if self.get_movement_cost(terrain, class).is_none() {
                    problems.push(format!("missing movement cost for '{}' on '{}'", unit_class_to_key(class), key));
                }
            }
        }
        for &id in CommanderId::all() {
            match self.commanders.commanders.get(commander_to_key(id)) {
                None => problems.push(format!("missing commander '{}'", commander_to_key(id))),
                Some(co) if co.power_cost == 0 => {
                    problems.push(format!("commander '{}' has power_cost 0", commander_to_key(id)))
                }
                Some(_) => {}
            }
        }
        let unit_keys: Vec<&str> = UnitType::all().iter().map(|&u| unit_type_to_key(u)).collect();
        for (attacker, row) in &self.damage_tables.tables {
            if !unit_keys.contains(&attacker.as_str()) {
                problems.push(format!("damage table has unknown attacker '{}'", attacker));
            }
            for defender in row.keys() {
                if !unit_keys.contains(&defender.as_str()) {
                    problems.push(format!("damage table '{}' has unknown defender '{}'", attacker, defender));
                }
            }
        }
        if problems.is_empty() { Ok(()) } else { Err(problems) }
    }

    // ========================================================================
    // LOOKUP METHODS
    // ========================================================================
    // Every enum variant is guaranteed to have data once `validate()` has
    // passed (enforced by ModdingPlugin), so these lookups never fail.

    pub fn faction(&self, faction: Faction) -> &FactionData {
        &self.factions.factions[faction_to_key(faction)]
    }

    pub fn unit(&self, unit_type: UnitType) -> &UnitData {
        &self.units.units[unit_type_to_key(unit_type)]
    }

    pub fn unit_stats(&self, unit_type: UnitType) -> &UnitStatsData {
        &self.unit(unit_type).stats
    }

    pub fn terrain(&self, terrain: Terrain) -> &TerrainData {
        &self.terrain.terrain[terrain_to_key(terrain)]
    }

    pub fn commander(&self, id: CommanderId) -> &CommanderData {
        &self.commanders.commanders[commander_to_key(id)]
    }

    // ========================================================================
    // CONVENIENCE LOOKUP METHODS
    // ========================================================================

    pub fn faction_name(&self, faction: Faction) -> &str {
        &self.faction(faction).name
    }

    pub fn faction_color(&self, faction: Faction) -> Color {
        let [r, g, b] = self.faction(faction).color;
        Color::srgb(r, g, b)
    }


    pub fn unit_name(&self, unit_type: UnitType) -> &str {
        &self.unit(unit_type).name
    }

    pub fn unit_class(&self, unit_type: UnitType) -> UnitClass {
        self.unit_stats(unit_type).class
    }

    /// What it costs to build a unit under a commander's cost modifier. Every
    /// army pays the same base price; only commanders change it.
    pub fn production_cost(&self, unit_type: UnitType, co_cost_modifier: f32) -> u32 {
        let base = self.unit_stats(unit_type).cost as f32;
        (base * co_cost_modifier).round() as u32
    }

    pub fn terrain_name(&self, terrain: Terrain) -> &str {
        &self.terrain(terrain).name
    }

    pub fn terrain_defense(&self, terrain: Terrain) -> u32 {
        self.terrain(terrain).defense
    }

    /// Whether this terrain has a vertical feature sprite (trees, rocks, buildings)
    pub fn terrain_has_feature(&self, terrain: Terrain) -> bool {
        self.terrain(terrain).feature_height > 0.0
    }

    pub fn terrain_color(&self, terrain: Terrain) -> Color {
        let [r, g, b] = self.terrain(terrain).color;
        Color::srgb(r, g, b)
    }

    pub fn terrain_capturable(&self, terrain: Terrain) -> bool {
        self.terrain(terrain).capturable
    }

    /// Whether an owned, empty tile of this terrain can build units
    pub fn terrain_produces(&self, terrain: Terrain) -> bool {
        !self.terrain(terrain).builds.is_empty()
    }

    /// Whether this terrain can build a given unit type
    pub fn terrain_builds(&self, terrain: Terrain, unit_type: UnitType) -> bool {
        self.terrain(terrain).builds.contains(&self.unit_class(unit_type))
    }

    /// Every unit type this terrain can build, cheapest first
    pub fn buildable_units(&self, terrain: Terrain) -> Vec<UnitType> {
        let mut units: Vec<UnitType> = UnitType::all().iter().copied()
            .filter(|&unit_type| self.terrain_builds(terrain, unit_type))
            .collect();
        units.sort_by_key(|&unit_type| self.unit_stats(unit_type).cost);
        units
    }

    /// Whether an owned tile of this terrain heals and resupplies a unit type
    pub fn terrain_resupplies(&self, terrain: Terrain, unit_type: UnitType) -> bool {
        self.terrain(terrain).resupplies.contains(&self.unit_class(unit_type))
    }

    pub fn terrain_capture_points(&self, terrain: Terrain) -> u32 {
        self.terrain(terrain).capture_points
    }

    pub fn terrain_income(&self, terrain: Terrain) -> u32 {
        self.terrain(terrain).income
    }

    pub fn commander_name(&self, id: CommanderId) -> &str {
        &self.commander(id).name
    }

    // === DAMAGE TABLE LOOKUPS ===

    /// Get base damage percentage for attacker vs defender matchup
    /// Returns None if no entry exists (unit cannot attack that target)
    pub fn get_base_damage(&self, attacker: UnitType, defender: UnitType) -> Option<u32> {
        let attacker_key = unit_type_to_key(attacker);
        let defender_key = unit_type_to_key(defender);

        self.damage_tables.tables
            .get(attacker_key)
            .and_then(|targets| targets.get(defender_key))
            .copied()
    }

    /// Check if attacker can damage defender (has a damage table entry)
    pub fn can_damage(&self, attacker: UnitType, defender: UnitType) -> bool {
        self.get_base_damage(attacker, defender).is_some()
    }

    // === MOVEMENT COST LOOKUPS ===

    fn get_movement_cost(&self, terrain: Terrain, unit_class: UnitClass) -> Option<u32> {
        self.movement_costs.costs
            .get(terrain_to_key(terrain))
            .and_then(|classes| classes.get(unit_class_to_key(unit_class)))
            .copied()
    }

    /// Movement points a unit class spends to enter a terrain (99+ = impassable)
    pub fn movement_cost(&self, terrain: Terrain, unit_class: UnitClass) -> u32 {
        self.get_movement_cost(terrain, unit_class).unwrap_or(IMPASSABLE)
    }

    pub fn is_passable(&self, terrain: Terrain, unit_class: UnitClass) -> bool {
        self.movement_cost(terrain, unit_class) < IMPASSABLE
    }
}

/// Movement cost value meaning a unit class cannot enter a terrain
pub const IMPASSABLE: u32 = 99;

/// Parse a mod's RON file (if present) and hand it to `merge`
#[cfg(not(target_arch = "wasm32"))]
fn merge_mod_file<T: serde::de::DeserializeOwned>(path: &std::path::Path, merge: impl FnOnce(T)) {
    if !path.exists() {
        return;
    }
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            warn!("Failed to read {}: {}", path.display(), e);
            return;
        }
    };
    match ron::from_str::<T>(&content) {
        Ok(data) => {
            info!("  Applied {}", path.display());
            merge(data);
        }
        Err(e) => warn!("Failed to parse {}: {}", path.display(), e),
    }
}

// ============================================================================
// ENUM TO KEY CONVERSIONS
// ============================================================================

fn faction_to_key(faction: Faction) -> &'static str {
    match faction {
        Faction::Eastern => "eastern",
        Faction::Northern => "northern",
        Faction::Western => "western",
        Faction::Southern => "southern",
        Faction::Nether => "nether",
    }
}

fn unit_type_to_key(unit_type: UnitType) -> &'static str {
    match unit_type {
        UnitType::Scout => "scout",
        UnitType::Shocktrooper => "shocktrooper",
        UnitType::Recon => "recon",
        UnitType::Ironclad => "ironclad",
        UnitType::Juggernaut => "juggernaut",
        UnitType::Behemoth => "behemoth",
        UnitType::Flak => "flak",
        UnitType::Siege => "siege",
        UnitType::Barrage => "barrage",
        UnitType::Stinger => "stinger",
        UnitType::Carrier => "carrier",
        UnitType::Supplier => "supplier",
        UnitType::Ferrier => "ferrier",
        UnitType::Skywing => "skywing",
        UnitType::Raptor => "raptor",
        UnitType::Talon => "talon",
        UnitType::Barge => "barge",
        UnitType::Frigate => "frigate",
        UnitType::Lurker => "lurker",
        UnitType::Dreadnought => "dreadnought",
    }
}

fn terrain_to_key(terrain: Terrain) -> &'static str {
    match terrain {
        Terrain::Grass => "grass",
        Terrain::TallGrass => "tall_grass",
        Terrain::Thicket => "thicket",
        Terrain::Brambles => "brambles",
        Terrain::Log => "log",
        Terrain::Boulder => "boulder",
        Terrain::Hollow => "hollow",
        Terrain::Creek => "creek",
        Terrain::Pond => "pond",
        Terrain::LilyPads => "lily_pads",
        Terrain::Shore => "shore",
        Terrain::Base => "base",
        Terrain::Workshop => "workshop",
        Terrain::Dock => "dock",
        Terrain::Roost => "roost",
        Terrain::Outpost => "outpost",
        Terrain::Storehouse => "storehouse",
    }
}

/// Data key for a commander (also its portrait file name)
pub fn commander_to_key(id: CommanderId) -> &'static str {
    match id {
        CommanderId::Kira => "kira",
        CommanderId::Tanuki => "tanuki",
        CommanderId::Sensei => "sensei",
        CommanderId::Grimjaw => "grimjaw",
        CommanderId::Frost => "frost",
        CommanderId::Bjorn => "bjorn",
        CommanderId::Bandit => "bandit",
        CommanderId::Talon => "talon_co",  // Avoid conflict with unit
        CommanderId::Dusty => "dusty",
        CommanderId::Lionheart => "lionheart",
        CommanderId::Tusker => "tusker",
        CommanderId::Prowler => "prowler",
        CommanderId::Burrower => "burrower",
        CommanderId::Hivemind => "hivemind",
        CommanderId::Dredge => "dredge",
    }
}

fn unit_class_to_key(class: UnitClass) -> &'static str {
    match class {
        UnitClass::Foot => "foot",
        UnitClass::Wheels => "wheels",
        UnitClass::Treads => "treads",
        UnitClass::Air => "air",
        UnitClass::Naval => "naval",
        UnitClass::Transport => "transport",
        UnitClass::AirTransport => "air_transport",
        UnitClass::NavalTransport => "naval_transport",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_data_is_complete() {
        let data = GameData::load_defaults();
        if let Err(problems) = data.validate() {
            panic!("embedded data incomplete:\n{}", problems.join("\n"));
        }
    }

    #[test]
    fn validate_reports_missing_entries() {
        let mut data = GameData::load_defaults();
        data.units.units.remove("scout");
        data.damage_tables.tables.entry("scout".into()).or_default().insert("dragon".into(), 50);
        let problems = data.validate().unwrap_err();
        assert!(problems.iter().any(|p| p.contains("missing unit 'scout'")));
        assert!(problems.iter().any(|p| p.contains("unknown defender 'dragon'")));
    }

    #[test]
    fn every_faction_has_commanders() {
        let data = GameData::load_defaults();
        for &faction in Faction::all() {
            assert!(!CommanderId::for_faction(faction, &data).is_empty(), "{faction:?} has no COs");
        }
    }

    #[test]
    fn units_that_attack_have_damage_entries() {
        let data = GameData::load_defaults();
        for &unit_type in UnitType::all() {
            let has_targets = UnitType::all().iter().any(|&t| data.can_damage(unit_type, t));
            let is_transport = matches!(
                data.unit_class(unit_type),
                UnitClass::Transport | UnitClass::AirTransport | UnitClass::NavalTransport
            );
            assert_eq!(has_targets, !is_transport, "{unit_type:?}");
        }
    }

    #[test]
    fn income_and_prices_share_a_scale() {
        // One turn of a single base's income should buy at least one infantry
        let data = GameData::load_defaults();
        let scout = data.unit_stats(UnitType::Scout).cost;
        assert!(data.terrain_produces(Terrain::Base) && data.terrain_produces(Terrain::Workshop));
        assert!(!data.terrain_produces(Terrain::Outpost) && !data.terrain_produces(Terrain::Storehouse));
        assert!(data.terrain_builds(Terrain::Workshop, UnitType::Ironclad));
        assert!(!data.terrain_builds(Terrain::Workshop, UnitType::Frigate));
        assert!(data.terrain_builds(Terrain::Dock, UnitType::Frigate));
        assert!(data.terrain_builds(Terrain::Roost, UnitType::Skywing));
        assert!(!data.terrain_builds(Terrain::Roost, UnitType::Scout));
        assert!(data.terrain_resupplies(Terrain::Storehouse, UnitType::Scout));
        assert!(!data.terrain_resupplies(Terrain::Storehouse, UnitType::Frigate));
        assert!(data.terrain_resupplies(Terrain::Dock, UnitType::Frigate));
        assert!(data.terrain_income(Terrain::Base) >= scout);
        assert!(data.terrain_income(Terrain::Base) < data.unit_stats(UnitType::Ironclad).cost);
    }

    #[test]
    fn prices_are_advance_wars_prices_changed_only_by_commanders() {
        let data = GameData::load_defaults();
        assert_eq!(data.production_cost(UnitType::Scout, 1.0), 1000);
        assert_eq!(data.production_cost(UnitType::Ironclad, 1.0), 7000);
        assert_eq!(data.production_cost(UnitType::Ironclad, 0.9), 6300);
    }

    #[test]
    fn movement_costs_respect_unit_class() {
        let data = GameData::load_defaults();
        assert!(!data.is_passable(Terrain::Pond, UnitClass::Foot));
        assert!(data.is_passable(Terrain::Pond, UnitClass::Naval));
        assert!(data.is_passable(Terrain::Pond, UnitClass::Air));
        assert!(!data.is_passable(Terrain::Grass, UnitClass::Naval));
        assert_eq!(data.movement_cost(Terrain::Grass, UnitClass::Foot), 1);
    }
}
