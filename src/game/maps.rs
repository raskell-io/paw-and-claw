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
    SoleHarbor,
    PivotIsle,
    RidgeIsland,
    TwinIsle,
    MojiIsland,
    DuoFalls,
    ToilFerry,
    PointStormy,
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
            MapId::SoleHarbor => "Sole Harbor",
            MapId::PivotIsle => "Pivot Isle",
            MapId::RidgeIsland => "Ridge Island",
            MapId::TwinIsle => "Twin Isle",
            MapId::MojiIsland => "Moji Island",
            MapId::DuoFalls => "Duo Falls",
            MapId::ToilFerry => "Toil Ferry",
            MapId::PointStormy => "Point Stormy",
            MapId::Custom(_) => "Custom Map",
        }
    }

    pub fn all_builtin() -> Vec<MapId> {
        vec![
            MapId::SpannIsland,
            MapId::SoleHarbor,
            MapId::PivotIsle,
            MapId::RidgeIsland,
            MapId::TwinIsle,
            MapId::MojiIsland,
            MapId::DuoFalls,
            MapId::ToilFerry,
            MapId::PointStormy,
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
    let mut map = match id {
        MapId::Woodland => create_woodland_map(),
        MapId::RiverCrossing => create_river_crossing_map(),
        MapId::TwinBases => create_twin_bases_map(),
        MapId::Fortress => create_fortress_map(),
        MapId::IslandChain => create_island_chain_map(),
        MapId::MountainPass => create_mountain_pass_map(),
        MapId::MarshLands => create_marsh_lands_map(),
        MapId::AncientRuins => create_ancient_ruins_map(),
        MapId::SpannIsland => create_spann_island_map(),
        MapId::SoleHarbor => create_sole_harbor_map(),
        MapId::PivotIsle => create_pivot_isle_map(),
        MapId::RidgeIsland => create_ridge_island_map(),
        MapId::TwinIsle => create_twin_isle_map(),
        MapId::MojiIsland => create_moji_island_map(),
        MapId::DuoFalls => create_duo_falls_map(),
        MapId::ToilFerry => create_toil_ferry_map(),
        MapId::PointStormy => create_point_stormy_map(),
        MapId::Custom(_) => create_woodland_map(), // Fallback
    };
    add_workshops_beside_hqs(&mut map);
    map
}

/// As in Advance Wars, HQs don't build units. Any army with an HQ but no
/// workshop gets one on the nearest open land beside its HQ, so every map
/// stays playable.
fn add_workshops_beside_hqs(map: &mut MapData) {
    let hqs: Vec<(i32, i32, Faction)> = map.properties.iter()
        .filter(|p| map.get_terrain(p.x, p.y) == Some(Terrain::Base))
        .map(|p| (p.x, p.y, p.owner))
        .collect();
    for (hx, hy, owner) in hqs {
        let has_workshop = map.properties.iter()
            .any(|p| p.owner == owner && map.get_terrain(p.x, p.y) == Some(Terrain::Workshop));
        if has_workshop {
            continue;
        }
        let open = |map: &MapData, x: i32, y: i32| {
            matches!(map.get_terrain(x, y), Some(Terrain::Grass | Terrain::TallGrass))
                && !map.units.iter().any(|u| (u.x, u.y) == (x, y))
        };
        let spot = (1..=3).flat_map(|r| (-r..=r).flat_map(move |dy| (-r..=r).map(move |dx| (hx + dx, hy + dy))))
            .find(|&(x, y)| open(map, x, y));
        if let Some((x, y)) = spot {
            map.set_terrain(x, y, Terrain::Workshop);
            map.add_property(x, y, owner);
        }
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

/// Sole Harbor, the fourth Advance Wars War Room map (also in AW2 and Dual
/// Strike). Tile for tile like the original, with the same translation as
/// Spann Island; shoals are shores and its three ports are docks. The first
/// army holds the mainland in the north-west, the second the peninsula in the
/// south-east. Both start with no units.
fn create_sole_harbor_map() -> MapData {
    let mut map = map_from_rows("Sole Harbor", &[
        "T.To....wwwrwwwwwwwwww",
        ".Bo...SPwwwwwwwrwwwrww",
        "S=To.Swwwwrwwwssssswww",
        "T=o...wwwrwww..S.S.Pwr",
        ".=oo.FwwwwwwwS=====.ww",
        "T=ToF.STTTFT..=.oo=Sww",
        ".=So===========Too=.ww",
        "S=oo=FTTSTTTF.SToS=.wr",
        "T=oS=TwwwwwwwwTSoo=Sww",
        ".====FsrwwwrwwTooS=.ww",
        ".S.T..swwwwsswToT.=Sww",
        "wsssswwwrww.F.S.S.F.wr",
        "wwwwwwwrwwwP.F.BTF.Tww",
        "wrwwwwwwwwwwwwwwwwwwww",
        "wwwrwwwwwwwwwrwwwwwrww",
    ]);
    map.description = "A narrow land bridge links the mainland to a crowded peninsula, with docks on both coasts.".to_string();

    for (x, y) in [(1, 1), (7, 1), (0, 2), (2, 6), (0, 7), (5, 4), (4, 5), (5, 9)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(15, 12), (11, 12), (14, 11), (16, 11), (13, 12), (17, 12)] {
        map.add_property(x, y, Faction::Northern);
    }

    map
}

/// Pivot Isle, an Advance Wars War Room map (also in AW2 and Dual Strike),
/// transcribed from the Dual Strike version. Islands joined by trails that
/// bridge the water, with shoals as shores, ports as docks and airports as
/// roosts. The first army starts in the north-west, the second on the small
/// south-eastern island with a roost of its own. No starting units.
fn create_pivot_isle_map() -> MapData {
    let mut map = map_from_rows("Pivot Isle", &[
        "..T..wrwwwwrwwwwwwwww",
        ".B.FPwwwwwwwwwwwwwwrw",
        "T====wwsA======srwwww",
        ".F=FAwws=SToTS=swwwww",
        "T.=.wwws=FoooF=swwwrw",
        "T.=.wwws=SToTS=swsssw",
        "ww=wwrws======Pww..Tw",
        "ww=wwwww=wwwwwwwwA.Aw",
        "wr=wwwww=wwwwwwwww=ww",
        "ww=wwwwr=wwrwsTSSS.Tw",
        ".T=.Pwww=wwwws===FBFw",
        "===============SSS.Tw",
        ".SF.T..SSwwwwwssssssw",
        "T.T.AS.TTT.wwwwwwwwww",
    ]);
    map.description = "Bridges pivot around a mountain island; docks and roosts reward a sea and air game.".to_string();

    for (x, y) in [(1, 1), (3, 1), (1, 3), (3, 3), (2, 12), (1, 12)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(18, 10), (17, 10), (19, 10), (17, 11), (19, 7)] {
        map.add_property(x, y, Faction::Northern);
    }

    map
}

/// Ridge Island, an Advance Wars War Room map (also in AW2 and Dual Strike).
/// A land map: one island split by a ridge of boulders and thickets, all
/// terrain translated as on Spann Island. The first army starts in the south,
/// the second in the north-east. No starting units.
fn create_ridge_island_map() -> MapData {
    let mut map = map_from_rows("Ridge Island", &[
        "wwwwwwwwwwwwwwwwwww",
        "wwwwwooT...TTwwwwww",
        "www.oooTSSS.TTTwwww",
        "ww...ooT......TTwww",
        "wwTSFToTTSS.FBFTwww",
        "ww.T=.T.=====F...ww",
        "www.=.o.=....=..Tww",
        "wwwS=Soo=STTS=.STww",
        "www.=.ToTTTTT=STTww",
        "wwT.=..TTooooToooww",
        "wwTF=....oo..=Tooww",
        "ww..BF=F======Towww",
        "ww...TT.......T.www",
        "www...TT.SSSF..wwww",
        "wwwwww.TTTT..wwwwww",
        "wwwwwwwwwwwwwwwwwww",
    ]);
    map.description = "An all-land island fight across a ridge of boulders and thickets.".to_string();

    for (x, y) in [(4, 11), (3, 10), (5, 11), (7, 11), (3, 7), (9, 13), (10, 13)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(13, 4), (12, 4), (14, 4), (13, 5), (9, 2), (10, 2), (9, 4), (10, 4), (12, 7), (15, 7), (14, 8)] {
        map.add_property(x, y, Faction::Northern);
    }

    map
}

/// Twin Isle, an Advance Wars War Room map (also in AW2 and Dual Strike).
/// Two mirrored islands, each ringed by a trail around a long boulder ridge,
/// joined by a narrow neck of storehouses. Terrain translated as on Spann
/// Island. The first army holds the northern island, the second the southern
/// one, which starts with eight storehouses. No starting units.
fn create_twin_isle_map() -> MapData {
    let mut map = map_from_rows("Twin Isle", &[
        "wwwwwwwwwwwwwwwwwwwwww",
        "wT..TTTTSS.TTTT.TT.www",
        "w.B===============Twww",
        "wT=FoooooSSoooooF=Swww",
        "wT=FooooooooooooF=Swww",
        "wS================Twww",
        "wTTTSS.TTTT..TTT..Twww",
        "wwwwwwww.SSS..wwwwwwww",
        "wwwwwwww..SSS.wwwwwwww",
        "wwwTTT..TT..TTT..T.S.w",
        "www.================Sw",
        "wwwS=FooooooooooooF=Tw",
        "wwwS=FooSSooooSSooF=.w",
        "www.===============B.w",
        "wwwTTTTTSST.TSS.TTTTTw",
        "wwwwwwwwwwwwwwwwwwwwww",
    ]);
    map.description = "Two long islands joined by a narrow neck of storehouses; races around the ridges.".to_string();

    for (x, y) in [(2, 2), (3, 3), (3, 4), (1, 5)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(19, 13), (18, 11), (18, 12), (8, 12), (9, 12), (14, 12), (15, 12), (8, 14), (9, 14), (13, 14), (14, 14)] {
        map.add_property(x, y, Faction::Northern);
    }

    map
}

/// Moji Island, an Advance Wars War Room map. A ring-shaped island around an
/// inner bay, terrain translated as on Spann Island. The first army holds the
/// northern shore, with a storehouse blocking the trail below its base; the
/// second sits on the peninsula in the middle of the bay. No starting units.
fn create_moji_island_map() -> MapData {
    let mut map = map_from_rows("Moji Island", &[
        "wwwwwwwwwwwwwwwwwwwwwwwwww",
        "wwwwwwww............wwwwww",
        "wwwwww................wwww",
        "wwwww.FFSTT.SBS.SF.TToowww",
        "wwwww.=======S====TTToowww",
        "wwww.S=.....TT...TooTTooww",
        "wwww..=STT.wwww.ToooFTTTww",
        "www..T=TTwwwwwwwwwoooTTTww",
        "www...=T.wwwwwwwwwwoTTTTww",
        "wwwT..=..www.TT.wwwTTTTTww",
        "wwwTSS=.www.T....TTTTTTTww",
        "wwwT..=.wwwTFBFT....TTTTww",
        "wwwTT.=Swww..F===.....TTww",
        "wwwTT.=..www..T.=S..SSS.ww",
        "wwwT.S=T.www...S=.......ww",
        "wwwT.T=T.wwwwww.=..SS..www",
        "wwwT..=.wwwwwww.=S....wwww",
        "wwww.SFSww.T...S=....wwwww",
        "wwwwT.======SS===..wwwwwww",
        "wwwwTTT.ww..T....wwwwwwwww",
        "wwwwwTTwwwwwwwwwwwwwwwwwww",
        "wwwwwwwwwwwwwwwwwwwwwwwwww",
    ]);
    map.description = "A bull's-eye island: one army on the rim, the other at the heart of the bay.".to_string();

    for (x, y) in [(13, 3), (6, 3), (7, 3), (8, 3), (12, 3), (14, 3), (16, 3), (17, 3), (13, 4)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(13, 11), (12, 11), (14, 11), (13, 12), (17, 13), (15, 14)] {
        map.add_property(x, y, Faction::Northern);
    }

    map
}

/// Duo Falls, an Advance Wars War Room map. A long island split by two
/// creeks that fall into lagoons, each crossed by a single trail bridge;
/// the small two-tile lake in the western woods is a pond. Airports are
/// roosts. The first army starts in the west, the second in the east. No
/// starting units.
fn create_duo_falls_map() -> MapData {
    let mut map = map_from_rows("Duo Falls", &[
        "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
        "wwwwwww....~...wwwwww..~.T.wwwwww",
        "www..TT.SS.~.S..wwwTTSF~.STT..www",
        "wTTT===========.www.======..TT.ww",
        "wTTT=T...TT~.T=.wwwS=.T~.=....S.w",
        "wTTF=F.SS..~..=SwwwT=Swww=FBF.TTw",
        "wT..B....Twww.=Twww.=.www===..TTw",
        "w..S=S.TTTwwwS=FwwwS=Twww.A.A.TTw",
        "w..A=A.wwTwww.=..w..=.www....SS.w",
        "w.SS=SSTTTwwwT=.S.AT=SwwwSS..o.ww",
        "ww....TTTTwww.=======TwwwT.oowwww",
        "www.TTTTwwwwww.TTFTTTwwww.wwwwwww",
        "wwwwwwwwwwwwwwwwwwwwwwwwwwwwwwwww",
    ]);
    map.description = "Two waterfalls cut a long island into thirds; each creek has one bridge.".to_string();

    for (x, y) in [(4, 6), (3, 5), (5, 5), (3, 7), (5, 7), (13, 7), (15, 7), (3, 8), (5, 8), (2, 9), (3, 9), (5, 9), (6, 9)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(27, 5), (22, 2), (26, 5), (28, 5), (26, 7), (29, 8), (30, 8), (25, 9), (26, 9)] {
        map.add_property(x, y, Faction::Northern);
    }

    map
}

/// Toil Ferry, an Advance Wars War Room map. Two coasts and three shore-ringed
/// islands across a wide channel, so docks and roosts matter: shoals are
/// shores, ports docks and airports roosts. The first army starts in the
/// south-west, the second in the north-east. No starting units.
fn create_toil_ferry_map() -> MapData {
    let mut map = map_from_rows("Toil Ferry", &[
        "T.T.TswwwwwoTTTTTTToTTT..T",
        "S.AsswwswwwwwTSSooTT...S..",
        ".PTswws.swwwwwssw.A=FBF==S",
        ".wswwwsTFswwwwwwwww=PF.A=.",
        "swwwwwws.AswwwwwwwwwwTT.=.",
        "wwwrwwwwsswwwswwwrwwwww.=S",
        "wwwwwwwwwrwwsTsswwwwwwwS=.",
        "wwwwwrwwwwrwsT.oswwwwwwT=S",
        "wwwwwwwwwwwwsSFooswwrww.=.",
        "www..wwwwwwwws.ooTswwwwo=.",
        ".P.TTS..wwwwwwssoPwwwwsT=T",
        "S=======Twwwwwws..swwws.=S",
        ".SF..o.=..swwwwwsAswws===.",
        "..TTooT=FA.swrwwwswwwsF.TT",
        ".BToooT===A.swwwwwwwsT.ASo",
        "TToooooSF=.PwwwwwrwwwPoooo",
    ]);
    map.description = "A wide channel dotted with islands; whoever rules the water rules the ferry.".to_string();

    for (x, y) in [(1, 14), (1, 10), (5, 10), (0, 11), (1, 12), (2, 12), (8, 13), (9, 13), (7, 15), (8, 15)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(21, 2), (23, 1), (18, 2), (20, 2), (22, 2), (25, 2), (20, 3), (21, 3), (23, 3), (25, 5), (23, 6), (25, 7)] {
        map.add_property(x, y, Faction::Northern);
    }

    map
}

/// Point Stormy, an Advance Wars War Room map. A northern headland and a
/// southern mainland around a bay, joined by a long trail down the east;
/// shoals are shores, ports docks and airports roosts, and the two small
/// lakes are ponds. The first army starts on the headland in the north-west,
/// the second on the bay's southern shore with two docks and two roosts. No
/// starting units.
fn create_point_stormy_map() -> MapData {
    let mut map = map_from_rows("Point Stormy", &[
        "wwwwwwwwwwwwwwwwwwwwwwwww",
        "wwwww...SS...Towwwwwwwwww",
        "wwTT.....TTTTT...wwwwwwww",
        "wT.....TTTwwTT.....wwwwww",
        "wS.oo.S.F=======SS...wwww",
        "wwA..F.B.F..sss=======www",
        "wwwww.F.ST.swwws..STT=Tww",
        "wwwwwsssssswwwwwwS.oo=TTw",
        "wwwwwwwwwwwwwwwwwwwoo=TTw",
        "wwssswwwwwwwwwrwwwwoo=TTw",
        "ww...wwwwwrwwwwwrwwo.=T.w",
        "w..F.PwwwwwwwwwwwwwwPF=Sw",
        "w.A.F=.swrwwwrwwwwww..=.w",
        "w...T=FswwwwwwwrwwsT.A=.S",
        "wwTTT=.swwwwwwwwwwsTT.=ST",
        "wwTTT=.swwTP.P.wwsTTSS=.T",
        "w.TTS=Sww.AFBFAwwsTwwT=TT",
        "w.T..=Sww...F.swws.TTT=.T",
        "w....=..w...=.ssss.T..=.T",
        ".TTT.=T....S=T....S.SS=.T",
        "TTTTT==================..",
        "TTTTT..SS...TSTT..TTTT...",
    ]);
    map.description = "A headland against a crowded mainland across a stormy bay, linked by one long trail.".to_string();

    for (x, y) in [(7, 5), (6, 4), (8, 4), (5, 5), (9, 5), (6, 6), (8, 6), (18, 6), (17, 7)] {
        map.add_property(x, y, Faction::Eastern);
    }
    for (x, y) in [(12, 16), (4, 12), (6, 13), (11, 15), (13, 15), (10, 16), (11, 16), (13, 16), (14, 16), (12, 17)] {
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

    /// Size, then (terrain, owner, count) checks for one imported map.
    fn assert_layout(id: MapId, size: (u32, u32), expected: &[(Terrain, Option<Faction>, usize)]) {
        let map = get_builtin_map(id);
        assert_eq!((map.width, map.height), size, "{}", map.name);
        assert!(map.units.is_empty(), "{}: both armies start with nothing", map.name);
        for &(terrain, owner, n) in expected {
            let found = (0..map.height as i32).flat_map(|y| (0..map.width as i32).map(move |x| (x, y)))
                .filter(|&(x, y)| map.get_terrain(x, y) == Some(terrain))
                .filter(|&(x, y)| map.properties.iter().find(|p| (p.x, p.y) == (x, y)).map(|p| p.owner) == owner)
                .count();
            assert_eq!(found, n, "{}: {:?} owned by {:?}", map.name, terrain, owner);
        }
    }

    #[test]
    fn sole_harbor_matches_the_war_room_original() {
        let (e, n) = (Some(Faction::Eastern), Some(Faction::Northern));
        assert_layout(MapId::SoleHarbor, (22, 15), &[
            (Terrain::Base, e, 1), (Terrain::Base, n, 1),
            (Terrain::Workshop, e, 3), (Terrain::Workshop, n, 2), (Terrain::Workshop, None, 5),
            (Terrain::Dock, e, 1), (Terrain::Dock, n, 1), (Terrain::Dock, None, 1),
            (Terrain::Storehouse, e, 3), (Terrain::Storehouse, n, 2), (Terrain::Storehouse, None, 16),
        ]);
    }

    #[test]
    fn pivot_isle_matches_the_war_room_original() {
        let (e, n) = (Some(Faction::Eastern), Some(Faction::Northern));
        assert_layout(MapId::PivotIsle, (21, 14), &[
            (Terrain::Base, e, 1), (Terrain::Base, n, 1),
            (Terrain::Workshop, e, 4), (Terrain::Workshop, n, 2), (Terrain::Workshop, None, 2),
            (Terrain::Dock, None, 3),
            (Terrain::Roost, n, 1), (Terrain::Roost, None, 4),
            (Terrain::Storehouse, e, 1), (Terrain::Storehouse, n, 1), (Terrain::Storehouse, None, 12),
        ]);
    }

    #[test]
    fn ridge_island_matches_the_war_room_original() {
        let (e, n) = (Some(Faction::Eastern), Some(Faction::Northern));
        assert_layout(MapId::RidgeIsland, (19, 16), &[
            (Terrain::Base, e, 1), (Terrain::Base, n, 1),
            (Terrain::Workshop, e, 3), (Terrain::Workshop, n, 3), (Terrain::Workshop, None, 2),
            (Terrain::Storehouse, e, 3), (Terrain::Storehouse, n, 7), (Terrain::Storehouse, None, 5),
        ]);
    }

    #[test]
    fn twin_isle_matches_the_war_room_original() {
        let (e, n) = (Some(Faction::Eastern), Some(Faction::Northern));
        assert_layout(MapId::TwinIsle, (22, 16), &[
            (Terrain::Base, e, 1), (Terrain::Base, n, 1),
            (Terrain::Workshop, e, 2), (Terrain::Workshop, n, 2), (Terrain::Workshop, None, 4),
            (Terrain::Storehouse, e, 1), (Terrain::Storehouse, n, 8), (Terrain::Storehouse, None, 18),
        ]);
    }

    #[test]
    fn moji_island_matches_the_war_room_original() {
        let (e, n) = (Some(Faction::Eastern), Some(Faction::Northern));
        assert_layout(MapId::MojiIsland, (26, 22), &[
            (Terrain::Base, e, 1), (Terrain::Base, n, 1),
            (Terrain::Workshop, e, 3), (Terrain::Workshop, n, 3), (Terrain::Workshop, None, 2),
            (Terrain::Storehouse, e, 5), (Terrain::Storehouse, n, 2), (Terrain::Storehouse, None, 17),
        ]);
    }

    #[test]
    fn duo_falls_matches_the_war_room_original() {
        let (e, n) = (Some(Faction::Eastern), Some(Faction::Northern));
        assert_layout(MapId::DuoFalls, (33, 13), &[
            (Terrain::Base, e, 1), (Terrain::Base, n, 1),
            (Terrain::Workshop, e, 3), (Terrain::Workshop, n, 3), (Terrain::Workshop, None, 1),
            (Terrain::Roost, e, 2), (Terrain::Roost, n, 1), (Terrain::Roost, None, 2),
            (Terrain::Storehouse, e, 7), (Terrain::Storehouse, n, 4), (Terrain::Storehouse, None, 14),
        ]);
    }

    #[test]
    fn toil_ferry_matches_the_war_room_original() {
        let (e, n) = (Some(Faction::Eastern), Some(Faction::Northern));
        assert_layout(MapId::ToilFerry, (26, 16), &[
            (Terrain::Base, e, 1), (Terrain::Base, n, 1),
            (Terrain::Workshop, e, 3), (Terrain::Workshop, n, 3), (Terrain::Workshop, None, 3),
            (Terrain::Dock, e, 1), (Terrain::Dock, n, 1), (Terrain::Dock, None, 4),
            (Terrain::Roost, e, 1), (Terrain::Roost, n, 2), (Terrain::Roost, None, 5),
            (Terrain::Storehouse, e, 4), (Terrain::Storehouse, n, 5), (Terrain::Storehouse, None, 6),
        ]);
    }

    #[test]
    fn point_stormy_matches_the_war_room_original() {
        let (e, n) = (Some(Faction::Eastern), Some(Faction::Northern));
        assert_layout(MapId::PointStormy, (25, 22), &[
            (Terrain::Base, e, 1), (Terrain::Base, n, 1),
            (Terrain::Workshop, e, 4), (Terrain::Workshop, n, 5), (Terrain::Workshop, None, 2),
            (Terrain::Dock, n, 2), (Terrain::Dock, None, 2),
            (Terrain::Roost, n, 2), (Terrain::Roost, None, 3),
            (Terrain::Storehouse, e, 4), (Terrain::Storehouse, None, 20),
        ]);
    }

    #[test]
    fn every_army_can_build_on_every_map() {
        let data = crate::game::GameData::load_defaults();
        for id in MapId::all_builtin() {
            let map = get_builtin_map(id);
            for army in [Faction::Eastern, Faction::Northern] {
                let builds = map.properties.iter()
                    .filter(|p| p.owner == army)
                    .any(|p| map.get_terrain(p.x, p.y).is_some_and(|t| data.terrain_produces(t)));
                assert!(builds, "{}: {:?} has nowhere to build", map.name, army);
            }
        }
        assert!(!data.terrain_produces(Terrain::Base), "HQs don't build");
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
