use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Faction, FactionMember, GameMap, TILE_SIZE};

pub struct UnitPlugin;

impl Plugin for UnitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_test_units.after(super::spawn_test_map));
    }
}

/// Unit categories for movement and targeting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnitClass {
    Foot,       // Infantry-type, can capture
    Treads,     // Armored ground vehicles
    Air,        // Flying units
    Transport,  // Non-combat carriers
}

/// Unit types - original names, classic tactics game balance
/// Stats are tuned for balanced turn-based tactical combat
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnitType {
    // === FOOT UNITS ===
    /// Scout - Light foot soldier, cheap and captures buildings
    Scout,
    /// Shocktrooper - Heavy foot soldier, anti-armor capability
    Shocktrooper,

    // === GROUND VEHICLES ===
    /// Ironclad - Main battle armor, strong all-around
    Ironclad,
    /// Recon - Fast wheeled vehicle, good vision
    Recon,
    /// Siege - Indirect fire, long range bombardment
    Siege,

    // === AIR UNITS ===
    /// Skywing - Light air unit, versatile attacker
    Skywing,
    /// Talon - Heavy air unit, anti-ground specialist
    Talon,

    // === SUPPORT ===
    /// Carrier - Transports foot units
    Carrier,
    /// Supplier - Resupplies and repairs adjacent units
    Supplier,
}

impl UnitType {
    /// Get unit statistics - balanced for tactical depth
    pub fn stats(&self) -> UnitStats {
        match self {
            // Scout: Cheap, mobile infantry. Can capture. (Infantry equivalent)
            UnitType::Scout => UnitStats {
                max_hp: 100,
                attack: 55,
                defense: 100,
                movement: 3,
                attack_range: (1, 1),
                can_capture: true,
                cost: 1000,
                class: UnitClass::Foot,
            },
            // Shocktrooper: Slower but hits harder, anti-armor. (Mech equivalent)
            UnitType::Shocktrooper => UnitStats {
                max_hp: 100,
                attack: 65,
                defense: 110,
                movement: 2,
                attack_range: (1, 1),
                can_capture: true,
                cost: 3000,
                class: UnitClass::Foot,
            },
            // Ironclad: Main battle unit, good attack and defense. (Tank equivalent)
            UnitType::Ironclad => UnitStats {
                max_hp: 100,
                attack: 75,
                defense: 160,
                movement: 6,
                attack_range: (1, 1),
                can_capture: false,
                cost: 7000,
                class: UnitClass::Treads,
            },
            // Recon: Fast, cheap, good for scouting. (Recon equivalent)
            UnitType::Recon => UnitStats {
                max_hp: 100,
                attack: 70,
                defense: 130,
                movement: 8,
                attack_range: (1, 1),
                can_capture: false,
                cost: 4000,
                class: UnitClass::Treads,
            },
            // Siege: Indirect fire, cannot move and attack same turn. (Artillery equivalent)
            UnitType::Siege => UnitStats {
                max_hp: 100,
                attack: 90,
                defense: 50,
                movement: 5,
                attack_range: (2, 3),
                can_capture: false,
                cost: 6000,
                class: UnitClass::Treads,
            },
            // Skywing: Versatile air unit. (B-Copter equivalent)
            UnitType::Skywing => UnitStats {
                max_hp: 100,
                attack: 55,
                defense: 70,
                movement: 6,
                attack_range: (1, 1),
                can_capture: false,
                cost: 9000,
                class: UnitClass::Air,
            },
            // Talon: Heavy air, strong vs ground. (Bomber-lite)
            UnitType::Talon => UnitStats {
                max_hp: 100,
                attack: 75,
                defense: 80,
                movement: 7,
                attack_range: (1, 1),
                can_capture: false,
                cost: 12000,
                class: UnitClass::Air,
            },
            // Carrier: Transports foot units. (APC equivalent)
            UnitType::Carrier => UnitStats {
                max_hp: 100,
                attack: 0,
                defense: 70,
                movement: 6,
                attack_range: (0, 0),
                can_capture: false,
                cost: 5000,
                class: UnitClass::Transport,
            },
            // Supplier: Mobile resupply. (APC supply role)
            UnitType::Supplier => UnitStats {
                max_hp: 100,
                attack: 0,
                defense: 70,
                movement: 5,
                attack_range: (0, 0),
                can_capture: false,
                cost: 5000,
                class: UnitClass::Transport,
            },
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            UnitType::Scout => "Scout",
            UnitType::Shocktrooper => "Shocktrooper",
            UnitType::Ironclad => "Ironclad",
            UnitType::Recon => "Recon",
            UnitType::Siege => "Siege",
            UnitType::Skywing => "Skywing",
            UnitType::Talon => "Talon",
            UnitType::Carrier => "Carrier",
            UnitType::Supplier => "Supplier",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            UnitType::Scout => "Light foot soldier. Cheap and can capture buildings.",
            UnitType::Shocktrooper => "Heavy foot soldier. Strong against armor.",
            UnitType::Ironclad => "Main battle armor. Powerful and well-protected.",
            UnitType::Recon => "Fast scout vehicle. Great mobility.",
            UnitType::Siege => "Long-range bombardment. Cannot counter-attack.",
            UnitType::Skywing => "Versatile air unit. Attacks ground and air.",
            UnitType::Talon => "Heavy air striker. Devastating against ground forces.",
            UnitType::Carrier => "Transports foot units across the battlefield.",
            UnitType::Supplier => "Resupplies ammunition and repairs nearby units.",
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            UnitType::Scout => "Sc",
            UnitType::Shocktrooper => "Sh",
            UnitType::Ironclad => "Ir",
            UnitType::Recon => "Rc",
            UnitType::Siege => "Si",
            UnitType::Skywing => "Sw",
            UnitType::Talon => "Ta",
            UnitType::Carrier => "Ca",
            UnitType::Supplier => "Su",
        }
    }

    pub fn class(&self) -> UnitClass {
        self.stats().class
    }
}

#[derive(Debug, Clone)]
pub struct UnitStats {
    pub max_hp: i32,
    pub attack: i32,
    pub defense: i32,
    pub movement: u32,
    pub attack_range: (u32, u32),  // (min, max) range
    pub can_capture: bool,
    pub cost: u32,
    pub class: UnitClass,
}

/// Component for unit entities
#[derive(Component, Debug, Clone)]
pub struct Unit {
    pub unit_type: UnitType,
    pub hp: i32,
    pub moved: bool,
    pub attacked: bool,
}

impl Unit {
    pub fn new(unit_type: UnitType) -> Self {
        let stats = unit_type.stats();
        Self {
            unit_type,
            hp: stats.max_hp,
            moved: false,
            attacked: false,
        }
    }

    pub fn hp_percentage(&self) -> f32 {
        self.hp as f32 / self.unit_type.stats().max_hp as f32
    }

    pub fn reset_turn(&mut self) {
        self.moved = false;
        self.attacked = false;
    }
}

/// Grid position component
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPosition {
    pub x: i32,
    pub y: i32,
}

impl GridPosition {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn to_world(&self, map: &GameMap) -> Vec3 {
        let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
        let offset_y = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
        Vec3::new(
            self.x as f32 * TILE_SIZE + offset_x,
            self.y as f32 * TILE_SIZE + offset_y,
            1.0,
        )
    }

    pub fn distance_to(&self, other: &GridPosition) -> u32 {
        ((self.x - other.x).abs() + (self.y - other.y).abs()) as u32
    }
}

/// Marker for selected unit
#[derive(Component)]
pub struct Selected;

fn spawn_test_units(mut commands: Commands, game_map: Res<GameMap>) {
    // Spawn Eastern Empire units (red)
    spawn_unit(&mut commands, &game_map, Faction::Eastern, UnitType::Scout, 1, 1);
    spawn_unit(&mut commands, &game_map, Faction::Eastern, UnitType::Scout, 2, 1);
    spawn_unit(&mut commands, &game_map, Faction::Eastern, UnitType::Ironclad, 1, 2);

    // Spawn Northern Realm units (blue)
    spawn_unit(&mut commands, &game_map, Faction::Northern, UnitType::Scout, 10, 6);
    spawn_unit(&mut commands, &game_map, Faction::Northern, UnitType::Skywing, 9, 5);
    spawn_unit(&mut commands, &game_map, Faction::Northern, UnitType::Siege, 10, 5);
}

fn spawn_unit(
    commands: &mut Commands,
    map: &GameMap,
    faction: Faction,
    unit_type: UnitType,
    x: i32,
    y: i32,
) {
    let grid_pos = GridPosition::new(x, y);
    let world_pos = grid_pos.to_world(map);

    commands.spawn((
        Sprite {
            color: faction.color(),
            custom_size: Some(Vec2::splat(TILE_SIZE * 0.7)),
            ..default()
        },
        Transform::from_translation(world_pos),
        Unit::new(unit_type),
        GridPosition::new(x, y),
        FactionMember { faction },
    ));
}
