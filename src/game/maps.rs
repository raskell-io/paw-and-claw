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

    /// Built-in maps are authored with Eastern as the first army and Northern as
    /// the second. Reassign those two slots to the factions actually playing.
    pub fn with_factions(&self, first: Faction, second: Faction) -> Self {
        let remap = |faction: Faction| match faction {
            Faction::Eastern => first,
            Faction::Northern => second,
            other => other,
        };
        let mut map = self.clone();
        for unit in &mut map.units {
            unit.faction = remap(unit.faction);
        }
        for property in &mut map.properties {
            property.owner = remap(property.owner);
        }
        map
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

    /// Load map from JSON file (for custom map loading, see ROADMAP)
    #[allow(dead_code)]
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let json = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read map file: {}", e))?;
        serde_json::from_str(&json)
            .map_err(|e| format!("Failed to parse map: {}", e))
    }
}

/// Identifier for built-in maps
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapId {
    Woodland,
    RiverCrossing,
    TwinBases,
    Fortress,
    IslandChain,
    MountainPass,
    MarshLands,
    AncientRuins,
    #[default]
    SpannIsland,
    #[allow(dead_code)] // Custom map loading, see ROADMAP
    Custom(usize),
}

impl MapId {
    pub fn name(&self) -> &'static str {
        match self {
            MapId::Woodland => "Woodland Clearing",
            MapId::RiverCrossing => "River Crossing",
            MapId::TwinBases => "Twin Bases",
            MapId::Fortress => "The Fortress",
            MapId::IslandChain => "Island Chain",
            MapId::MountainPass => "Mountain Pass",
            MapId::MarshLands => "Marsh Lands",
            MapId::AncientRuins => "Ancient Ruins",
            MapId::SpannIsland => "Spann Island",
            MapId::Custom(_) => "Custom Map",
        }
    }

    pub fn all_builtin() -> Vec<MapId> {
        vec![
            MapId::SpannIsland,
            MapId::Woodland,
            MapId::RiverCrossing,
            MapId::TwinBases,
            MapId::Fortress,
            MapId::IslandChain,
            MapId::MountainPass,
            MapId::MarshLands,
            MapId::AncientRuins,
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
        MapId::IslandChain => create_island_chain_map(),
        MapId::MountainPass => create_mountain_pass_map(),
        MapId::MarshLands => create_marsh_lands_map(),
        MapId::AncientRuins => create_ancient_ruins_map(),
        MapId::SpannIsland => create_spann_island_map(),
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

/// Island Chain - Naval-focused map with multiple islands separated by water
fn create_island_chain_map() -> MapData {
    let mut map = MapData::new("Island Chain", 16, 12);
    map.description = "Islands linked by shallow water, each army with a dock and a roost. Build boats and fliers!".to_string();

    // Fill with water first
    for y in 0..12 {
        for x in 0..16 {
            map.set_terrain(x, y, Terrain::Pond);
        }
    }

    // Western island (large) - Eastern faction base
    for y in 2..6 {
        for x in 1..5 {
            map.set_terrain(x, y, Terrain::Grass);
        }
    }
    // Shore around western island
    map.set_terrain(0, 3, Terrain::Shore);
    map.set_terrain(0, 4, Terrain::Shore);
    map.set_terrain(5, 2, Terrain::Shore);
    map.set_terrain(5, 3, Terrain::Shore);
    map.set_terrain(5, 4, Terrain::Shore);
    map.set_terrain(5, 5, Terrain::Shore);
    map.set_terrain(1, 6, Terrain::Shore);
    map.set_terrain(2, 6, Terrain::Shore);
    map.set_terrain(3, 6, Terrain::Shore);
    map.set_terrain(4, 6, Terrain::Shore);
    map.set_terrain(1, 1, Terrain::Shore);
    map.set_terrain(2, 1, Terrain::Shore);
    map.set_terrain(3, 1, Terrain::Shore);
    map.set_terrain(4, 1, Terrain::Shore);

    // Eastern island (large) - Northern faction base
    for y in 6..10 {
        for x in 11..15 {
            map.set_terrain(x, y, Terrain::Grass);
        }
    }
    // Shore around eastern island
    map.set_terrain(15, 7, Terrain::Shore);
    map.set_terrain(15, 8, Terrain::Shore);
    map.set_terrain(10, 6, Terrain::Shore);
    map.set_terrain(10, 7, Terrain::Shore);
    map.set_terrain(10, 8, Terrain::Shore);
    map.set_terrain(10, 9, Terrain::Shore);
    map.set_terrain(11, 5, Terrain::Shore);
    map.set_terrain(12, 5, Terrain::Shore);
    map.set_terrain(13, 5, Terrain::Shore);
    map.set_terrain(14, 5, Terrain::Shore);
    map.set_terrain(11, 10, Terrain::Shore);
    map.set_terrain(12, 10, Terrain::Shore);
    map.set_terrain(13, 10, Terrain::Shore);
    map.set_terrain(14, 10, Terrain::Shore);

    // Central island (small) - contested point
    for y in 4..8 {
        for x in 7..10 {
            map.set_terrain(x, y, Terrain::Grass);
        }
    }
    map.set_terrain(6, 5, Terrain::Shore);
    map.set_terrain(6, 6, Terrain::Shore);
    map.set_terrain(10, 5, Terrain::Shore);
    map.set_terrain(10, 6, Terrain::Shore);
    map.set_terrain(7, 3, Terrain::Shore);
    map.set_terrain(8, 3, Terrain::Shore);
    map.set_terrain(9, 3, Terrain::Shore);
    map.set_terrain(7, 8, Terrain::Shore);
    map.set_terrain(8, 8, Terrain::Shore);
    map.set_terrain(9, 8, Terrain::Shore);

    // Terrain features on islands
    map.set_terrain(2, 3, Terrain::Thicket);
    map.set_terrain(3, 4, Terrain::TallGrass);
    map.set_terrain(12, 7, Terrain::Thicket);
    map.set_terrain(13, 8, Terrain::TallGrass);
    map.set_terrain(8, 5, Terrain::Boulder);
    map.set_terrain(8, 6, Terrain::Hollow);

    // Shallow water crossing paths (creek)
    map.set_terrain(5, 4, Terrain::Creek);
    map.set_terrain(6, 4, Terrain::Creek);
    map.set_terrain(10, 7, Terrain::Creek);
    map.set_terrain(11, 7, Terrain::Creek);

    // Bases
    map.set_terrain(2, 2, Terrain::Base);
    map.add_property(2, 2, Faction::Eastern);
    map.set_terrain(13, 9, Terrain::Base);
    map.add_property(13, 9, Faction::Northern);

    // Each army has a dock on its island's coast and a roost inland
    map.set_terrain(5, 3, Terrain::Dock);
    map.add_property(5, 3, Faction::Eastern);
    map.set_terrain(3, 5, Terrain::Roost);
    map.add_property(3, 5, Faction::Eastern);
    map.set_terrain(10, 8, Terrain::Dock);
    map.add_property(10, 8, Faction::Northern);
    map.set_terrain(13, 6, Terrain::Roost);
    map.add_property(13, 6, Faction::Northern);

    // Central island outpost (contested)
    map.set_terrain(8, 5, Terrain::Outpost);
    map.set_terrain(7, 7, Terrain::Storehouse);

    // Starting units
    map.add_unit(UnitType::Scout, Faction::Eastern, 2, 2);
    map.add_unit(UnitType::Scout, Faction::Eastern, 3, 3);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 2, 4);
    map.add_unit(UnitType::Barge, Faction::Eastern, 6, 2);

    map.add_unit(UnitType::Scout, Faction::Northern, 13, 9);
    map.add_unit(UnitType::Scout, Faction::Northern, 12, 8);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 13, 7);
    map.add_unit(UnitType::Barge, Faction::Northern, 9, 9);

    map
}

/// Mountain Pass - Narrow corridors through rocky terrain with chokepoints
fn create_mountain_pass_map() -> MapData {
    let mut map = MapData::new("Mountain Pass", 14, 10);
    map.description = "Rocky mountain terrain with narrow passes. Control the chokepoints!".to_string();

    // Fill edges with impassable boulders (mountains)
    for x in 0..14 {
        map.set_terrain(x, 0, Terrain::Boulder);
        map.set_terrain(x, 9, Terrain::Boulder);
    }

    // Central mountain range running through middle
    for y in 2..8 {
        map.set_terrain(6, y, Terrain::Boulder);
        map.set_terrain(7, y, Terrain::Boulder);
    }

    // Northern pass opening
    map.set_terrain(6, 2, Terrain::Grass);
    map.set_terrain(7, 2, Terrain::Grass);
    map.set_terrain(6, 3, Terrain::TallGrass);
    map.set_terrain(7, 3, Terrain::TallGrass);

    // Southern pass opening
    map.set_terrain(6, 6, Terrain::Grass);
    map.set_terrain(7, 6, Terrain::Grass);
    map.set_terrain(6, 7, Terrain::Grass);
    map.set_terrain(7, 7, Terrain::Grass);

    // Rocky outcroppings on western side
    map.set_terrain(2, 3, Terrain::Boulder);
    map.set_terrain(3, 4, Terrain::Boulder);
    map.set_terrain(2, 6, Terrain::Boulder);
    map.set_terrain(4, 2, Terrain::Boulder);

    // Rocky outcroppings on eastern side
    map.set_terrain(11, 3, Terrain::Boulder);
    map.set_terrain(10, 4, Terrain::Boulder);
    map.set_terrain(11, 6, Terrain::Boulder);
    map.set_terrain(9, 7, Terrain::Boulder);

    // Defensive positions (brambles in passes)
    map.set_terrain(5, 3, Terrain::Brambles);
    map.set_terrain(8, 3, Terrain::Brambles);
    map.set_terrain(5, 7, Terrain::Brambles);
    map.set_terrain(8, 7, Terrain::Brambles);

    // Cover terrain scattered around
    map.set_terrain(3, 2, Terrain::Thicket);
    map.set_terrain(4, 5, Terrain::Hollow);
    map.set_terrain(10, 2, Terrain::Thicket);
    map.set_terrain(9, 5, Terrain::Hollow);
    map.set_terrain(3, 7, Terrain::TallGrass);
    map.set_terrain(10, 7, Terrain::TallGrass);

    // Small creek through southern area
    map.set_terrain(4, 8, Terrain::Creek);
    map.set_terrain(5, 8, Terrain::Creek);
    map.set_terrain(8, 8, Terrain::Creek);
    map.set_terrain(9, 8, Terrain::Creek);
    map.set_terrain(3, 8, Terrain::Shore);
    map.set_terrain(10, 8, Terrain::Shore);

    // Bases - opposite ends
    map.set_terrain(1, 4, Terrain::Base);
    map.set_terrain(1, 5, Terrain::Base);
    map.add_property(1, 4, Faction::Eastern);
    map.add_property(1, 5, Faction::Eastern);

    map.set_terrain(12, 4, Terrain::Base);
    map.set_terrain(12, 5, Terrain::Base);
    map.add_property(12, 4, Faction::Northern);
    map.add_property(12, 5, Faction::Northern);

    // Outposts at the passes
    map.set_terrain(6, 4, Terrain::Outpost);
    map.set_terrain(7, 5, Terrain::Outpost);

    // Storehouses behind front lines
    map.set_terrain(3, 1, Terrain::Storehouse);
    map.set_terrain(10, 1, Terrain::Storehouse);

    // Starting units
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 4);
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 5);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 2, 4);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 2, 5);
    map.add_unit(UnitType::Siege, Faction::Eastern, 2, 3);
    map.add_unit(UnitType::Ironclad, Faction::Eastern, 3, 5);

    map.add_unit(UnitType::Scout, Faction::Northern, 12, 4);
    map.add_unit(UnitType::Scout, Faction::Northern, 12, 5);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 11, 4);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 11, 5);
    map.add_unit(UnitType::Siege, Faction::Northern, 11, 6);
    map.add_unit(UnitType::Ironclad, Faction::Northern, 10, 5);

    map
}

/// Marsh Lands - Swampy terrain that slows movement but provides cover
fn create_marsh_lands_map() -> MapData {
    let mut map = MapData::new("Marsh Lands", 14, 10);
    map.description = "Treacherous marshland slows vehicles. Infantry has the advantage here!".to_string();

    // Scattered ponds throughout
    map.set_terrain(3, 2, Terrain::Pond);
    map.set_terrain(4, 3, Terrain::Pond);
    map.set_terrain(6, 5, Terrain::Pond);
    map.set_terrain(7, 4, Terrain::Pond);
    map.set_terrain(9, 6, Terrain::Pond);
    map.set_terrain(10, 7, Terrain::Pond);
    map.set_terrain(5, 8, Terrain::Pond);

    // Shore around ponds
    map.set_terrain(2, 2, Terrain::Shore);
    map.set_terrain(3, 1, Terrain::Shore);
    map.set_terrain(4, 2, Terrain::Shore);
    map.set_terrain(3, 3, Terrain::Shore);
    map.set_terrain(5, 3, Terrain::Shore);
    map.set_terrain(4, 4, Terrain::Shore);
    map.set_terrain(5, 5, Terrain::Shore);
    map.set_terrain(7, 5, Terrain::Shore);
    map.set_terrain(6, 4, Terrain::Shore);
    map.set_terrain(8, 4, Terrain::Shore);
    map.set_terrain(7, 3, Terrain::Shore);
    map.set_terrain(8, 6, Terrain::Shore);
    map.set_terrain(10, 6, Terrain::Shore);
    map.set_terrain(9, 7, Terrain::Shore);
    map.set_terrain(11, 7, Terrain::Shore);
    map.set_terrain(10, 8, Terrain::Shore);
    map.set_terrain(4, 8, Terrain::Shore);
    map.set_terrain(6, 8, Terrain::Shore);
    map.set_terrain(5, 7, Terrain::Shore);
    map.set_terrain(5, 9, Terrain::Shore);

    // Creeks connecting ponds (slow but passable)
    map.set_terrain(5, 4, Terrain::Creek);
    map.set_terrain(8, 5, Terrain::Creek);
    map.set_terrain(7, 6, Terrain::Creek);
    map.set_terrain(6, 7, Terrain::Creek);
    map.set_terrain(3, 4, Terrain::Creek);
    map.set_terrain(2, 5, Terrain::Creek);

    // Tall grass (marsh reeds) - lots of cover
    for y in 1..9 {
        map.set_terrain(1, y, Terrain::TallGrass);
        map.set_terrain(12, y, Terrain::TallGrass);
    }
    map.set_terrain(6, 1, Terrain::TallGrass);
    map.set_terrain(7, 1, Terrain::TallGrass);
    map.set_terrain(6, 9, Terrain::TallGrass);
    map.set_terrain(7, 9, Terrain::TallGrass);
    map.set_terrain(9, 2, Terrain::TallGrass);
    map.set_terrain(4, 6, Terrain::TallGrass);
    map.set_terrain(9, 4, Terrain::TallGrass);

    // Thickets on dry land
    map.set_terrain(2, 7, Terrain::Thicket);
    map.set_terrain(11, 3, Terrain::Thicket);
    map.set_terrain(8, 8, Terrain::Thicket);
    map.set_terrain(5, 1, Terrain::Thicket);

    // Log bridges over some water
    map.set_terrain(7, 7, Terrain::Log);
    map.set_terrain(4, 5, Terrain::Log);

    // Dry raised areas (hollows for cover)
    map.set_terrain(2, 4, Terrain::Hollow);
    map.set_terrain(11, 5, Terrain::Hollow);
    map.set_terrain(6, 3, Terrain::Hollow);

    // Bases on dry land at edges
    map.set_terrain(0, 4, Terrain::Base);
    map.set_terrain(0, 5, Terrain::Base);
    map.add_property(0, 4, Faction::Eastern);
    map.add_property(0, 5, Faction::Eastern);

    map.set_terrain(13, 4, Terrain::Base);
    map.set_terrain(13, 5, Terrain::Base);
    map.add_property(13, 4, Faction::Northern);
    map.add_property(13, 5, Faction::Northern);

    // Outposts on small dry patches
    map.set_terrain(4, 1, Terrain::Outpost);
    map.set_terrain(9, 8, Terrain::Outpost);

    // Storehouses at edges
    map.set_terrain(0, 1, Terrain::Storehouse);
    map.set_terrain(13, 8, Terrain::Storehouse);

    // Starting units (infantry-focused due to terrain)
    map.add_unit(UnitType::Scout, Faction::Eastern, 0, 4);
    map.add_unit(UnitType::Scout, Faction::Eastern, 0, 5);
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 3);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 1, 4);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 1, 5);
    map.add_unit(UnitType::Recon, Faction::Eastern, 2, 6);

    map.add_unit(UnitType::Scout, Faction::Northern, 13, 4);
    map.add_unit(UnitType::Scout, Faction::Northern, 13, 5);
    map.add_unit(UnitType::Scout, Faction::Northern, 12, 6);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 12, 4);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 12, 5);
    map.add_unit(UnitType::Recon, Faction::Northern, 11, 4);

    map
}

/// Ancient Ruins - Crumbling structures provide defensive positions
fn create_ancient_ruins_map() -> MapData {
    let mut map = MapData::new("Ancient Ruins", 16, 12);
    map.description = "Crumbling ancient structures offer cover. Urban-style combat with lots of defensive positions!".to_string();

    // Central temple ruins - large defensive structure
    // Outer walls (brambles representing crumbling walls)
    for x in 6..11 {
        map.set_terrain(x, 4, Terrain::Brambles);
        map.set_terrain(x, 8, Terrain::Brambles);
    }
    for y in 4..9 {
        map.set_terrain(6, y, Terrain::Brambles);
        map.set_terrain(10, y, Terrain::Brambles);
    }
    // Openings in walls
    map.set_terrain(8, 4, Terrain::Grass);
    map.set_terrain(8, 8, Terrain::Grass);
    map.set_terrain(6, 6, Terrain::Grass);
    map.set_terrain(10, 6, Terrain::Grass);

    // Interior of temple - defensive positions
    map.set_terrain(7, 5, Terrain::Hollow);
    map.set_terrain(9, 5, Terrain::Hollow);
    map.set_terrain(7, 7, Terrain::Hollow);
    map.set_terrain(9, 7, Terrain::Hollow);
    map.set_terrain(8, 6, Terrain::Boulder);  // Central altar/pillar

    // Western ruins (smaller structure)
    map.set_terrain(2, 3, Terrain::Brambles);
    map.set_terrain(3, 3, Terrain::Brambles);
    map.set_terrain(2, 4, Terrain::Brambles);
    map.set_terrain(3, 4, Terrain::Hollow);
    map.set_terrain(2, 5, Terrain::Brambles);
    map.set_terrain(3, 5, Terrain::Brambles);

    // Eastern ruins (smaller structure)
    map.set_terrain(12, 6, Terrain::Brambles);
    map.set_terrain(13, 6, Terrain::Brambles);
    map.set_terrain(12, 7, Terrain::Hollow);
    map.set_terrain(13, 7, Terrain::Brambles);
    map.set_terrain(12, 8, Terrain::Brambles);
    map.set_terrain(13, 8, Terrain::Brambles);

    // Northern watchtower ruins
    map.set_terrain(4, 1, Terrain::Boulder);
    map.set_terrain(5, 1, Terrain::Hollow);
    map.set_terrain(4, 2, Terrain::Brambles);
    map.set_terrain(5, 2, Terrain::Brambles);

    // Southern watchtower ruins
    map.set_terrain(10, 10, Terrain::Boulder);
    map.set_terrain(11, 10, Terrain::Hollow);
    map.set_terrain(10, 9, Terrain::Brambles);
    map.set_terrain(11, 9, Terrain::Brambles);

    // Overgrown areas (thickets reclaiming the ruins)
    map.set_terrain(1, 7, Terrain::Thicket);
    map.set_terrain(1, 8, Terrain::Thicket);
    map.set_terrain(14, 3, Terrain::Thicket);
    map.set_terrain(14, 4, Terrain::Thicket);
    map.set_terrain(7, 1, Terrain::TallGrass);
    map.set_terrain(8, 10, Terrain::TallGrass);
    map.set_terrain(4, 9, Terrain::TallGrass);
    map.set_terrain(11, 2, Terrain::TallGrass);

    // Ancient reflecting pools (water features)
    map.set_terrain(5, 6, Terrain::Pond);
    map.set_terrain(4, 6, Terrain::Shore);
    map.set_terrain(11, 5, Terrain::Pond);
    map.set_terrain(12, 5, Terrain::Shore);

    // Crumbling stone paths (logs representing walkways)
    map.set_terrain(3, 6, Terrain::Log);
    map.set_terrain(8, 3, Terrain::Log);
    map.set_terrain(8, 9, Terrain::Log);
    map.set_terrain(12, 4, Terrain::Log);

    // Bases in ruined outbuildings
    map.set_terrain(1, 2, Terrain::Base);
    map.set_terrain(0, 3, Terrain::Base);
    map.add_property(1, 2, Faction::Eastern);
    map.add_property(0, 3, Faction::Eastern);

    map.set_terrain(14, 9, Terrain::Base);
    map.set_terrain(15, 8, Terrain::Base);
    map.add_property(14, 9, Faction::Northern);
    map.add_property(15, 8, Faction::Northern);

    // Temple outposts (valuable positions)
    map.set_terrain(7, 6, Terrain::Outpost);
    map.set_terrain(9, 6, Terrain::Outpost);

    // Storehouses in side ruins
    map.set_terrain(3, 4, Terrain::Storehouse);
    map.set_terrain(12, 7, Terrain::Storehouse);

    // Starting units
    map.add_unit(UnitType::Scout, Faction::Eastern, 1, 2);
    map.add_unit(UnitType::Scout, Faction::Eastern, 0, 3);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 2, 2);
    map.add_unit(UnitType::Shocktrooper, Faction::Eastern, 1, 4);
    map.add_unit(UnitType::Recon, Faction::Eastern, 2, 1);
    map.add_unit(UnitType::Siege, Faction::Eastern, 0, 5);

    map.add_unit(UnitType::Scout, Faction::Northern, 14, 9);
    map.add_unit(UnitType::Scout, Faction::Northern, 15, 8);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 13, 9);
    map.add_unit(UnitType::Shocktrooper, Faction::Northern, 14, 7);
    map.add_unit(UnitType::Recon, Faction::Northern, 13, 10);
    map.add_unit(UnitType::Siege, Faction::Northern, 15, 6);

    map
}

/// Build a map from rows of characters, one per tile. Property owners are
/// given separately, since the same symbol is used for neutral and owned ones.
///
/// `.` grass, `"` tall grass, `T` thicket, `#` brambles, `=` log, `o` boulder,
/// `h` hollow, `~` creek, `w` pond, `r` lily pads (AW reef), `s` shore,
/// `B` base (HQ), `F` workshop (AW factory), `P` dock (AW port), `A` roost
/// (AW airport), `O` outpost, `S` storehouse
fn map_from_rows(name: &str, rows: &[&str]) -> MapData {
    let height = rows.len() as u32;
    let width = rows.first().map_or(0, |row| row.chars().count()) as u32;
    let mut map = MapData::new(name, width, height);
    for (y, row) in rows.iter().enumerate() {
        assert_eq!(row.chars().count() as u32, width, "{name}: row {y} has the wrong width");
        for (x, symbol) in row.chars().enumerate() {
            let terrain = match symbol {
                '.' => Terrain::Grass,
                '"' => Terrain::TallGrass,
                'T' => Terrain::Thicket,
                '#' => Terrain::Brambles,
                '=' => Terrain::Log,
                'o' => Terrain::Boulder,
                'h' => Terrain::Hollow,
                '~' => Terrain::Creek,
                'w' => Terrain::Pond,
                'r' => Terrain::LilyPads,
                's' => Terrain::Shore,
                'B' => Terrain::Base,
                'F' => Terrain::Workshop,
                'P' => Terrain::Dock,
                'A' => Terrain::Roost,
                'O' => Terrain::Outpost,
                'S' => Terrain::Storehouse,
                other => panic!("{name}: unknown map symbol {other:?}"),
            };
            map.set_terrain(x as i32, y as i32, terrain);
        }
    }
    map
}

/// Spann Island, from the Advance Wars War Room (also in AW2 and Dual Strike).
/// Layout follows the original tile for tile, translated to the forest floor:
/// sea is pond, the reef is lily pads, roads are trails (and draw as plank
/// bridges where they cross water), woods are thickets, mountains are boulders
/// and the river is a creek. Each HQ is a base, factories are workshops and
/// cities are storehouses. As in the original, both armies start with no
/// units. Orange Star is the first army, Black Hole the second.
fn create_spann_island_map() -> MapData {
    let mut map = map_from_rows("Spann Island", &[
        "wwwwwwwwwwwwwwww",
        "wwwwwwwwwwwwwwww",
        "ww.T=====.TF.Fww",
        "w..T=T~.=S..BFww",
        "wSS==www=.o.=Fww",
        "w..=.www=SoS=.ww",
        "wF==SwwT=====Sww",
        "wFB....TT.ww=www",
        "wwFF......ww..ww",
        "wwwwoSoTF.=.SSww",
        "wrwwwwwwwwwwwwww",
        "wwwwwwwwwwwwwwww",
    ]);
    map.description = "The classic War Room skirmish: a small island where every road leads to a fight.".to_string();

    for (x, y) in [(2, 7), (1, 6), (1, 7), (2, 8), (3, 8), (8, 9), (4, 6)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(12, 3), (11, 2), (13, 2), (13, 3), (13, 4)] {
        map.add_property(x, y, Faction::Northern);
    }

    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_maps_are_well_formed() {
        for id in MapId::all_builtin() {
            let map = get_builtin_map(id);
            assert_eq!(map.terrain.len() as u32, map.height, "{}", map.name);
            for property in &map.properties {
                let terrain = map.get_terrain(property.x, property.y);
                assert!(
                    terrain.is_some_and(|t| matches!(t, Terrain::Base | Terrain::Workshop | Terrain::Dock
                        | Terrain::Roost | Terrain::Outpost | Terrain::Storehouse)),
                    "{}: property at ({}, {}) is {:?}", map.name, property.x, property.y, terrain
                );
            }
        }
    }

    #[test]
    fn spann_island_matches_the_war_room_original() {
        let map = get_builtin_map(MapId::SpannIsland);
        assert_eq!((map.width, map.height), (16, 12));
        assert!(map.units.is_empty(), "both armies start with nothing, as in AW");
        let count = |terrain: Terrain, owner: Option<Faction>| {
            (0..map.height as i32).flat_map(|y| (0..map.width as i32).map(move |x| (x, y)))
                .filter(|&(x, y)| map.get_terrain(x, y) == Some(terrain))
                .filter(|&(x, y)| map.properties.iter().find(|p| (p.x, p.y) == (x, y)).map(|p| p.owner) == owner)
                .count()
        };
        assert_eq!(count(Terrain::Base, Some(Faction::Eastern)), 1);
        assert_eq!(count(Terrain::Base, Some(Faction::Northern)), 1);
        assert_eq!(count(Terrain::Workshop, Some(Faction::Eastern)), 5);
        assert_eq!(count(Terrain::Workshop, Some(Faction::Northern)), 4);
        assert_eq!(count(Terrain::Storehouse, Some(Faction::Eastern)), 1);
        assert_eq!(count(Terrain::Storehouse, None), 9);
        assert_eq!(count(Terrain::LilyPads, None), 1);
    }

    #[test]
    fn with_factions_reassigns_both_armies() {
        let map = get_builtin_map(MapId::Woodland).with_factions(Faction::Western, Faction::Southern);
        assert!(map.units.iter().all(|u| matches!(u.faction, Faction::Western | Faction::Southern)));
        assert!(map.units.iter().any(|u| u.faction == Faction::Western));
        assert!(map.units.iter().any(|u| u.faction == Faction::Southern));
        assert!(map.properties.iter().all(|p| matches!(p.owner, Faction::Western | Faction::Southern)));
    }
}
