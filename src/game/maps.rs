use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::{Faction, Terrain, UnitType};

/// Complete map definition including terrain, units, and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapData {
    pub name: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    /// 2D terrain grid (row-major: [y][x])
    pub terrain: Vec<Vec<Terrain>>,
    /// Starting unit placements
    pub units: Vec<UnitPlacement>,
    /// Initial property ownership
    pub properties: Vec<PropertyOwnership>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitPlacement {
    pub unit_type: UnitType,
    pub faction: Faction,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropertyOwnership {
    pub x: i32,
    pub y: i32,
    pub owner: Faction,
}

impl MapData {
    /// Create a new empty map filled with grass
    pub fn new(name: &str, width: u32, height: u32) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            width,
            height,
            terrain: vec![vec![Terrain::Grass; width as usize]; height as usize],
            units: Vec::new(),
            properties: Vec::new(),
        }
    }

    /// Set terrain at a position
    pub fn set_terrain(&mut self, x: i32, y: i32, terrain: Terrain) {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            self.terrain[y as usize][x as usize] = terrain;
        }
    }

    /// Get terrain at a position
    pub fn get_terrain(&self, x: i32, y: i32) -> Option<Terrain> {
        if x >= 0 && y >= 0 && (x as u32) < self.width && (y as u32) < self.height {
            Some(self.terrain[y as usize][x as usize])
        } else {
            None
        }
    }

    /// Add a unit placement
    pub fn add_unit(&mut self, unit_type: UnitType, faction: Faction, x: i32, y: i32) {
        self.units.push(UnitPlacement { unit_type, faction, x, y });
    }

    /// Add property ownership
    pub fn add_property(&mut self, x: i32, y: i32, owner: Faction) {
        self.properties.push(PropertyOwnership { x, y, owner });
    }

    /// Save map to JSON file
    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize map: {}", e))?;
        fs::write(path, json)
            .map_err(|e| format!("Failed to write map file: {}", e))?;
        Ok(())
    }

    /// Load map from JSON file
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let json = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read map file: {}", e))?;
        serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse map: {}", e))
    }
}

/// Resource tracking which map is selected
#[derive(Resource, Default)]
pub struct SelectedMap {
    pub map_id: MapId,
}

/// Identifier for built-in maps
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapId {
    #[default]
    Woodland,
    RiverCrossing,
    TwinBases,
    Fortress,
    Custom(usize),
}

impl MapId {
    pub fn name(&self) -> &'static str {
        match self {
            MapId::Woodland => "Woodland Clearing",
            MapId::RiverCrossing => "River Crossing",
            MapId::TwinBases => "Twin Bases",
            MapId::Fortress => "The Fortress",
            MapId::Custom(_) => "Custom Map",
        }
    }

    pub fn all_builtin() -> Vec<MapId> {
        vec![
            MapId::Woodland,
            MapId::RiverCrossing,
            MapId::TwinBases,
            MapId::Fortress,
        ]
    }
}

// ============================================================================
// BUILT-IN MAPS
// ============================================================================

/// Get a built-in map by ID
pub fn get_builtin_map(id: MapId) -> MapData {
    match id {
        MapId::Woodland => create_woodland_map(),
        MapId::RiverCrossing => create_river_crossing_map(),
        MapId::TwinBases => create_twin_bases_map(),
        MapId::Fortress => create_fortress_map(),
        MapId::Custom(_) => create_woodland_map(), // Fallback
    }
}

/// Original test map - balanced woodland clearing
fn create_woodland_map() -> MapData {
    let mut map = MapData::new("Woodland Clearing", 12, 8);
    map.description = "A balanced map with varied terrain. Good for learning the game.".to_string();

    // Thicket (dense bushes) - left side cover
    map.set_terrain(2, 2, Terrain::Thicket);
    map.set_terrain(2, 3, Terrain::Thicket);
    map.set_terrain(3, 2, Terrain::Thicket);
    map.set_terrain(3, 3, Terrain::TallGrass);

    // Boulders - center obstacles
    map.set_terrain(5, 3, Terrain::Boulder);
    map.set_terrain(5, 4, Terrain::Boulder);

    // Creek running through - water obstacle
    map.set_terrain(6, 5, Terrain::Creek);
    map.set_terrain(7, 5, Terrain::Creek);
    map.set_terrain(8, 5, Terrain::Creek);
    map.set_terrain(5, 5, Terrain::Shore);
    map.set_terrain(9, 5, Terrain::Shore);

    // Pond - impassable water
    map.set_terrain(8, 3, Terrain::Pond);
    map.set_terrain(8, 2, Terrain::Shore);
    map.set_terrain(9, 3, Terrain::Shore);

    // Fallen log - fast movement path
    map.set_terrain(3, 5, Terrain::Log);
    map.set_terrain(4, 5, Terrain::Log);
    map.set_terrain(5, 6, Terrain::Log);

    // Brambles - defensive position
    map.set_terrain(9, 4, Terrain::Brambles);
    map.set_terrain(10, 4, Terrain::Brambles);

    // Hollow stump - cover
    map.set_terrain(6, 2, Terrain::Hollow);

    // Bases
    map.set_terrain(1, 1, Terrain::Base);
    map.set_terrain(10, 6, Terrain::Base);
    map.add_property(1, 1, Faction::Eastern);
    map.add_property(10, 6, Faction::Northern);

    // Capturable points
    map.set_terrain(4, 1, Terrain::Outpost);
    map.set_terrain(7, 6, Terrain::Outpost);
    map.set_terrain(6, 0, Terrain::Storehouse);

    // Starting units
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 1);
    map.add_unit(UnitType::Scout, Faction::Eastern, 2, 1);
    map.add_unit(UnitType::Ironclad, Faction::Eastern, 1, 2);

    map.add_unit(UnitType::Scout, Faction::Northern, 10, 6);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 9, 5);
    map.add_unit(UnitType::Siege, Faction::Northern, 10, 5);

    map
}

/// River Crossing - A river divides the map with limited crossing points
fn create_river_crossing_map() -> MapData {
    let mut map = MapData::new("River Crossing", 14, 10);
    map.description = "A river divides the battlefield. Control the bridges to win!".to_string();

    // River running vertically through center
    for y in 0..10 {
        map.set_terrain(6, y, Terrain::Pond);
        map.set_terrain(7, y, Terrain::Pond);
    }

    // Bridge crossing points (shallow water/logs)
    map.set_terrain(6, 2, Terrain::Log);
    map.set_terrain(7, 2, Terrain::Log);
    map.set_terrain(6, 7, Terrain::Log);
    map.set_terrain(7, 7, Terrain::Log);

    // Shore areas
    for y in 0..10 {
        map.set_terrain(5, y, Terrain::Shore);
        map.set_terrain(8, y, Terrain::Shore);
    }

    // Western side terrain
    map.set_terrain(2, 3, Terrain::Thicket);
    map.set_terrain(2, 4, Terrain::Thicket);
    map.set_terrain(3, 5, Terrain::Boulder);
    map.set_terrain(1, 7, Terrain::Hollow);

    // Eastern side terrain
    map.set_terrain(11, 3, Terrain::Thicket);
    map.set_terrain(11, 4, Terrain::Thicket);
    map.set_terrain(10, 5, Terrain::Boulder);
    map.set_terrain(12, 7, Terrain::Hollow);

    // Bases - opposite corners
    map.set_terrain(1, 1, Terrain::Base);
    map.set_terrain(12, 8, Terrain::Base);
    map.add_property(1, 1, Faction::Eastern);
    map.add_property(12, 8, Faction::Northern);

    // Outposts near bridges
    map.set_terrain(4, 2, Terrain::Outpost);
    map.set_terrain(9, 2, Terrain::Outpost);
    map.set_terrain(4, 7, Terrain::Outpost);
    map.set_terrain(9, 7, Terrain::Outpost);

    // Storehouses
    map.set_terrain(2, 8, Terrain::Storehouse);
    map.set_terrain(11, 1, Terrain::Storehouse);

    // Starting units
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 1);
    map.add_unit(UnitType::Scout, Faction::Eastern, 2, 2);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 1, 2);
    map.add_unit(UnitType::Recon, Faction::Eastern, 3, 1);

    map.add_unit(UnitType::Scout, Faction::Northern, 12, 8);
    map.add_unit(UnitType::Scout, Faction::Northern, 11, 7);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 12, 7);
    map.add_unit(UnitType::Recon, Faction::Northern, 10, 8);

    map
}

/// Twin Bases - Each side has two bases for higher production
fn create_twin_bases_map() -> MapData {
    let mut map = MapData::new("Twin Bases", 16, 10);
    map.description = "High production map with two bases per side. Prepare for intense battles!".to_string();

    // Central terrain features
    map.set_terrain(7, 4, Terrain::Boulder);
    map.set_terrain(8, 4, Terrain::Boulder);
    map.set_terrain(7, 5, Terrain::Boulder);
    map.set_terrain(8, 5, Terrain::Boulder);

    // Thicket corridors
    for y in 2..8 {
        map.set_terrain(4, y, Terrain::TallGrass);
        map.set_terrain(11, y, Terrain::TallGrass);
    }
    map.set_terrain(4, 4, Terrain::Thicket);
    map.set_terrain(4, 5, Terrain::Thicket);
    map.set_terrain(11, 4, Terrain::Thicket);
    map.set_terrain(11, 5, Terrain::Thicket);

    // Small ponds
    map.set_terrain(6, 1, Terrain::Pond);
    map.set_terrain(9, 8, Terrain::Pond);
    map.set_terrain(5, 1, Terrain::Shore);
    map.set_terrain(7, 1, Terrain::Shore);
    map.set_terrain(8, 8, Terrain::Shore);
    map.set_terrain(10, 8, Terrain::Shore);

    // Western bases
    map.set_terrain(1, 2, Terrain::Base);
    map.set_terrain(1, 7, Terrain::Base);
    map.add_property(1, 2, Faction::Eastern);
    map.add_property(1, 7, Faction::Eastern);

    // Eastern bases
    map.set_terrain(14, 2, Terrain::Base);
    map.set_terrain(14, 7, Terrain::Base);
    map.add_property(14, 2, Faction::Northern);
    map.add_property(14, 7, Faction::Northern);

    // Neutral outposts
    map.set_terrain(7, 2, Terrain::Outpost);
    map.set_terrain(8, 7, Terrain::Outpost);

    // Storehouses
    map.set_terrain(3, 0, Terrain::Storehouse);
    map.set_terrain(12, 9, Terrain::Storehouse);
    map.set_terrain(3, 9, Terrain::Storehouse);
    map.set_terrain(12, 0, Terrain::Storehouse);

    // Starting units (more units due to larger map)
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 2);
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 7);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 2, 2);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 2, 7);
    map.add_unit(UnitType::Ironclad, Faction::Eastern, 2, 4);

    map.add_unit(UnitType::Scout, Faction::Northern, 14, 2);
    map.add_unit(UnitType::Scout, Faction::Northern, 14, 7);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 13, 2);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 13, 7);
    map.add_unit(UnitType::Ironclad, Faction::Northern, 13, 5);

    map
}

/// The Fortress - Asymmetric map where one side defends a fortified position
fn create_fortress_map() -> MapData {
    let mut map = MapData::new("The Fortress", 14, 12);
    map.description = "Asymmetric assault map. Eastern attacks the Northern fortress!".to_string();

    // Northern fortress - heavily defended area (top right)
    // Walls of brambles
    for x in 8..13 {
        map.set_terrain(x, 8, Terrain::Brambles);
    }
    for y in 8..11 {
        map.set_terrain(8, y, Terrain::Brambles);
    }
    // Gate opening
    map.set_terrain(10, 8, Terrain::Grass);

    // Fortress interior - boulders for cover
    map.set_terrain(10, 10, Terrain::Boulder);
    map.set_terrain(11, 9, Terrain::Hollow);

    // Moat around fortress
    for x in 6..13 {
        map.set_terrain(x, 6, Terrain::Creek);
    }
    map.set_terrain(5, 6, Terrain::Shore);
    map.set_terrain(13, 6, Terrain::Shore);
    // Bridge
    map.set_terrain(9, 6, Terrain::Log);
    map.set_terrain(10, 6, Terrain::Log);

    // Eastern approach - open terrain with some cover
    map.set_terrain(3, 3, Terrain::Thicket);
    map.set_terrain(4, 3, Terrain::TallGrass);
    map.set_terrain(3, 4, Terrain::TallGrass);
    map.set_terrain(5, 5, Terrain::Boulder);
    map.set_terrain(2, 7, Terrain::Hollow);
    map.set_terrain(4, 8, Terrain::Thicket);
    map.set_terrain(5, 9, Terrain::TallGrass);

    // Eastern base (attacker)
    map.set_terrain(1, 5, Terrain::Base);
    map.set_terrain(1, 8, Terrain::Base);
    map.add_property(1, 5, Faction::Eastern);
    map.add_property(1, 8, Faction::Eastern);

    // Northern fortress base (defender)
    map.set_terrain(11, 10, Terrain::Base);
    map.add_property(11, 10, Faction::Northern);

    // Objectives
    map.set_terrain(6, 3, Terrain::Outpost);
    map.set_terrain(9, 9, Terrain::Outpost);
    map.set_terrain(12, 10, Terrain::Storehouse);
    map.add_property(9, 9, Faction::Northern);
    map.add_property(12, 10, Faction::Northern);

    // Eastern starting units (attackers - more mobile)
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 5);
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 8);
    map.add_unit(UnitType::Scout, Faction::Eastern, 2, 6);
    map.add_unit(UnitType::Recon, Faction::Eastern, 2, 5);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 2, 7);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 2, 8);
    map.add_unit(UnitType::Siege, Faction::Eastern, 3, 6);

    // Northern starting units (defenders - dug in)
    map.add_unit(UnitType::Scout, Faction::Northern, 11, 10);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 10, 9);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 11, 9);
    map.add_unit(UnitType::Siege, Faction::Northern, 12, 9);
    map.add_unit(UnitType::Ironclad, Faction::Northern, 10, 10);

    map
}
