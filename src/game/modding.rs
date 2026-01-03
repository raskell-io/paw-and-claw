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

use super::{Faction, UnitType, UnitClass, Terrain, CommanderId, AiPersonality};

pub struct ModdingPlugin;

impl Plugin for ModdingPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameData>();
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
    /// Unit cost multiplier (1.0 = normal)
    pub unit_cost_modifier: f32,
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
    pub max_hp: u32,
    pub attack: u32,
    pub defense: u32,
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
    /// Movement cost for ground units
    pub movement_cost: u32,
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
}

/// CO Power effect types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PowerEffectData {
    /// Boost stats for this turn
    StatBoost {
        attack: f32,
        defense: f32,
        movement: i32,
    },
    /// Gain bonus funds
    BonusFunds {
        multiplier: f32,
    },
    /// Reveal map and boost attack
    RevealAndBoost {
        attack_boost: f32,
    },
    /// Boost defense and heal units
    DefenseAndHeal {
        defense: f32,
        heal: u32,
    },
    /// Spawn free units at bases
    FreeUnits {
        unit_type: UnitType,
        count: u32,
    },
    /// Grant extra movement
    ExtraMove {
        bonus: i32,
    },
    /// Steal funds from enemy
    StealFunds {
        percentage: f32,
    },
    /// Ignore terrain movement costs
    IgnoreTerrain,
}

/// Moddable CO power data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoPowerData {
    pub name: String,
    pub description: String,
    pub effect: PowerEffectData,
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
    pub vision_bonus: i32,
    /// Unit cost multiplier (1.0 = normal)
    pub cost_modifier: f32,
    /// Terrain movement cost reduction
    pub terrain_cost_reduction: i32,
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
    /// Whether mods have been loaded
    pub mods_loaded: bool,
    /// List of loaded mod names
    pub loaded_mods: Vec<String>,
}

impl Default for GameData {
    fn default() -> Self {
        Self::load_defaults()
    }
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

        Self {
            factions,
            units,
            terrain,
            commanders,
            mods_loaded: false,
            loaded_mods: Vec::new(),
        }
    }

    /// Load mods from the mods directory (native builds only)
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_mods(&mut self) {
        use std::fs;
        use std::path::Path;

        let mods_dir = Path::new("mods");
        if !mods_dir.exists() {
            info!("No mods directory found, using default data");
            return;
        }

        // Scan for mod directories
        let Ok(entries) = fs::read_dir(mods_dir) else {
            warn!("Failed to read mods directory");
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let mod_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            info!("Loading mod: {}", mod_name);

            // Load faction overrides
            let factions_path = path.join("factions.ron");
            if factions_path.exists() {
                if let Ok(content) = fs::read_to_string(&factions_path) {
                    match ron::from_str::<FactionsRegistry>(&content) {
                        Ok(mod_factions) => {
                            for (id, data) in mod_factions.factions {
                                info!("  Overriding faction: {}", id);
                                self.factions.factions.insert(id, data);
                            }
                        }
                        Err(e) => warn!("Failed to parse {}: {}", factions_path.display(), e),
                    }
                }
            }

            // Load unit overrides
            let units_path = path.join("units.ron");
            if units_path.exists() {
                if let Ok(content) = fs::read_to_string(&units_path) {
                    match ron::from_str::<UnitsRegistry>(&content) {
                        Ok(mod_units) => {
                            for (id, data) in mod_units.units {
                                info!("  Overriding unit: {}", id);
                                self.units.units.insert(id, data);
                            }
                        }
                        Err(e) => warn!("Failed to parse {}: {}", units_path.display(), e),
                    }
                }
            }

            // Load terrain overrides
            let terrain_path = path.join("terrain.ron");
            if terrain_path.exists() {
                if let Ok(content) = fs::read_to_string(&terrain_path) {
                    match ron::from_str::<TerrainRegistry>(&content) {
                        Ok(mod_terrain) => {
                            for (id, data) in mod_terrain.terrain {
                                info!("  Overriding terrain: {}", id);
                                self.terrain.terrain.insert(id, data);
                            }
                        }
                        Err(e) => warn!("Failed to parse {}: {}", terrain_path.display(), e),
                    }
                }
            }

            // Load commander overrides
            let commanders_path = path.join("commanders.ron");
            if commanders_path.exists() {
                if let Ok(content) = fs::read_to_string(&commanders_path) {
                    match ron::from_str::<CommandersRegistry>(&content) {
                        Ok(mod_commanders) => {
                            for (id, data) in mod_commanders.commanders {
                                info!("  Overriding commander: {}", id);
                                self.commanders.commanders.insert(id, data);
                            }
                        }
                        Err(e) => warn!("Failed to parse {}: {}", commanders_path.display(), e),
                    }
                }
            }

            self.loaded_mods.push(mod_name);
        }

        self.mods_loaded = true;
        info!("Loaded {} mod(s)", self.loaded_mods.len());
    }

    /// No-op for WASM builds
    #[cfg(target_arch = "wasm32")]
    pub fn load_mods(&mut self) {
        // Mods not supported on WASM
        self.mods_loaded = true;
    }

    // ========================================================================
    // LOOKUP METHODS
    // ========================================================================

    /// Get faction data by enum
    pub fn get_faction(&self, faction: Faction) -> Option<&FactionData> {
        let key = faction_to_key(faction);
        self.factions.factions.get(key)
    }

    /// Get unit data by enum
    pub fn get_unit(&self, unit_type: UnitType) -> Option<&UnitData> {
        let key = unit_type_to_key(unit_type);
        self.units.units.get(key)
    }

    /// Get terrain data by enum
    pub fn get_terrain(&self, terrain: Terrain) -> Option<&TerrainData> {
        let key = terrain_to_key(terrain);
        self.terrain.terrain.get(key)
    }

    /// Get commander data by enum
    pub fn get_commander(&self, id: CommanderId) -> Option<&CommanderData> {
        let key = commander_to_key(id);
        self.commanders.commanders.get(key)
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
        Terrain::Shore => "shore",
        Terrain::Base => "base",
        Terrain::Outpost => "outpost",
        Terrain::Storehouse => "storehouse",
    }
}

fn commander_to_key(id: CommanderId) -> &'static str {
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
