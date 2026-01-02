use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Faction, FactionMember, GameMap, TILE_SIZE, UnitShadow, Billboard};

pub struct UnitPlugin;

impl Plugin for UnitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, update_hp_displays);
    }
}

/// Unit categories for movement and targeting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnitClass {
    Foot,       // Infantry-type, can capture
    Wheels,     // Fast on roads, slow off-road (Recon, Missiles)
    Treads,     // Armored ground vehicles
    Air,        // Flying units (copters and planes)
    Naval,      // Ships
    Transport,  // Non-combat carriers (ground)
    AirTransport, // Non-combat air carriers
    NavalTransport, // Non-combat naval carriers
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
    /// Recon - Fast wheeled vehicle, good vision
    Recon,
    /// Ironclad - Main battle armor, strong all-around
    Ironclad,
    /// Juggernaut - Heavy battle armor, powerful but slow
    Juggernaut,
    /// Behemoth - Super-heavy armor, devastating firepower
    Behemoth,
    /// Flak - Anti-air vehicle, shreds aircraft
    Flak,
    /// Siege - Indirect fire, medium range bombardment
    Siege,
    /// Barrage - Long-range rocket artillery
    Barrage,
    /// Stinger - Long-range anti-air missiles
    Stinger,

    // === GROUND SUPPORT ===
    /// Carrier - Transports foot units
    Carrier,
    /// Supplier - Resupplies and repairs adjacent units
    Supplier,

    // === AIR UNITS ===
    /// Ferrier - Transport helicopter, carries foot units
    Ferrier,
    /// Skywing - Light attack helicopter, versatile
    Skywing,
    /// Raptor - Air superiority fighter, dominates skies
    Raptor,
    /// Talon - Heavy bomber, devastating ground attacks
    Talon,

    // === NAVAL UNITS ===
    /// Barge - Naval transport, carries ground units
    Barge,
    /// Frigate - Fast naval unit, anti-air and anti-sub
    Frigate,
    /// Lurker - Submarine, stealth attacks
    Lurker,
    /// Dreadnought - Battleship, massive indirect fire
    Dreadnought,
}

impl UnitType {
    /// Get unit statistics - balanced for tactical depth (based on Advance Wars 2)
    pub fn stats(&self) -> UnitStats {
        match self {
            // ========== FOOT UNITS ==========
            // Scout: Cheap, mobile infantry. Can capture. (Infantry equivalent)
            UnitType::Scout => UnitStats {
                max_hp: 100,
                attack: 55,
                defense: 100,
                movement: 3,
                attack_range: (1, 1),
                vision: 2,
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
                vision: 2,
                can_capture: true,
                cost: 3000,
                class: UnitClass::Foot,
            },

            // ========== GROUND VEHICLES ==========
            // Recon: Fast, cheap, good for scouting. (Recon equivalent)
            UnitType::Recon => UnitStats {
                max_hp: 100,
                attack: 70,
                defense: 130,
                movement: 8,
                attack_range: (1, 1),
                vision: 5,  // High vision - scout unit
                can_capture: false,
                cost: 4000,
                class: UnitClass::Wheels,
            },
            // Ironclad: Main battle unit, good attack and defense. (Tank equivalent)
            UnitType::Ironclad => UnitStats {
                max_hp: 100,
                attack: 75,
                defense: 160,
                movement: 6,
                attack_range: (1, 1),
                vision: 3,
                can_capture: false,
                cost: 7000,
                class: UnitClass::Treads,
            },
            // Juggernaut: Heavy tank, powerful but expensive. (Md. Tank equivalent)
            UnitType::Juggernaut => UnitStats {
                max_hp: 100,
                attack: 105,
                defense: 190,
                movement: 5,
                attack_range: (1, 1),
                vision: 2,
                can_capture: false,
                cost: 16000,
                class: UnitClass::Treads,
            },
            // Behemoth: Super-heavy tank, devastating. (Neotank equivalent)
            UnitType::Behemoth => UnitStats {
                max_hp: 100,
                attack: 125,
                defense: 220,
                movement: 6,
                attack_range: (1, 1),
                vision: 2,
                can_capture: false,
                cost: 22000,
                class: UnitClass::Treads,
            },
            // Flak: Anti-air vehicle, strong vs aircraft. (Anti-Air equivalent)
            UnitType::Flak => UnitStats {
                max_hp: 100,
                attack: 105, // vs air, lower vs ground
                defense: 140,
                movement: 6,
                attack_range: (1, 1),
                vision: 3,
                can_capture: false,
                cost: 8000,
                class: UnitClass::Treads,
            },
            // Siege: Indirect fire, medium range. (Artillery equivalent)
            UnitType::Siege => UnitStats {
                max_hp: 100,
                attack: 90,
                defense: 50,
                movement: 5,
                attack_range: (2, 3),
                vision: 1,  // Low vision - artillery
                can_capture: false,
                cost: 6000,
                class: UnitClass::Treads,
            },
            // Barrage: Long-range rockets. (Rockets equivalent)
            UnitType::Barrage => UnitStats {
                max_hp: 100,
                attack: 115,
                defense: 50,
                movement: 5,
                attack_range: (3, 5),
                vision: 1,  // Low vision - artillery
                can_capture: false,
                cost: 15000,
                class: UnitClass::Treads,
            },
            // Stinger: Long-range anti-air missiles. (Missiles equivalent)
            UnitType::Stinger => UnitStats {
                max_hp: 100,
                attack: 120, // vs air only
                defense: 50,
                movement: 4,
                attack_range: (3, 5),
                vision: 5,  // Radar - high vision
                can_capture: false,
                cost: 12000,
                class: UnitClass::Wheels,
            },

            // ========== GROUND SUPPORT ==========
            // Carrier: Transports foot units. (APC equivalent)
            UnitType::Carrier => UnitStats {
                max_hp: 100,
                attack: 0,
                defense: 70,
                movement: 6,
                attack_range: (0, 0),
                vision: 2,
                can_capture: false,
                cost: 5000,
                class: UnitClass::Transport,
            },
            // Supplier: Mobile resupply. (APC supply role)
            UnitType::Supplier => UnitStats {
                max_hp: 100,
                attack: 0,
                defense: 70,
                movement: 6,
                attack_range: (0, 0),
                vision: 2,
                can_capture: false,
                cost: 5000,
                class: UnitClass::Transport,
            },

            // ========== AIR UNITS ==========
            // Ferrier: Transport helicopter. (T-Copter equivalent)
            UnitType::Ferrier => UnitStats {
                max_hp: 100,
                attack: 0,
                defense: 50,
                movement: 6,
                attack_range: (0, 0),
                vision: 4,  // Good air vision
                can_capture: false,
                cost: 5000,
                class: UnitClass::AirTransport,
            },
            // Skywing: Versatile attack helicopter. (B-Copter equivalent)
            UnitType::Skywing => UnitStats {
                max_hp: 100,
                attack: 65,
                defense: 70,
                movement: 6,
                attack_range: (1, 1),
                vision: 4,  // Good air vision
                can_capture: false,
                cost: 9000,
                class: UnitClass::Air,
            },
            // Raptor: Air superiority fighter. (Fighter equivalent)
            UnitType::Raptor => UnitStats {
                max_hp: 100,
                attack: 100, // vs air, weak vs ground
                defense: 80,
                movement: 9,
                attack_range: (1, 1),
                vision: 5,  // High air vision
                can_capture: false,
                cost: 20000,
                class: UnitClass::Air,
            },
            // Talon: Heavy bomber. (Bomber equivalent)
            UnitType::Talon => UnitStats {
                max_hp: 100,
                attack: 115, // vs ground only
                defense: 90,
                movement: 7,
                attack_range: (1, 1),
                vision: 3,
                can_capture: false,
                cost: 22000,
                class: UnitClass::Air,
            },

            // ========== NAVAL UNITS ==========
            // Barge: Naval transport. (Lander equivalent)
            UnitType::Barge => UnitStats {
                max_hp: 100,
                attack: 0,
                defense: 60,
                movement: 6,
                attack_range: (0, 0),
                vision: 2,
                can_capture: false,
                cost: 12000,
                class: UnitClass::NavalTransport,
            },
            // Frigate: Fast naval, anti-air/sub. (Cruiser equivalent)
            UnitType::Frigate => UnitStats {
                max_hp: 100,
                attack: 85,
                defense: 80,
                movement: 6,
                attack_range: (1, 1),
                vision: 5,  // Radar
                can_capture: false,
                cost: 18000,
                class: UnitClass::Naval,
            },
            // Lurker: Submarine, stealth. (Sub equivalent)
            UnitType::Lurker => UnitStats {
                max_hp: 100,
                attack: 95,
                defense: 60,
                movement: 5,
                attack_range: (1, 1),
                vision: 3,
                can_capture: false,
                cost: 20000,
                class: UnitClass::Naval,
            },
            // Dreadnought: Battleship, indirect fire. (Battleship equivalent)
            UnitType::Dreadnought => UnitStats {
                max_hp: 100,
                attack: 130,
                defense: 100,
                movement: 5,
                attack_range: (2, 6),
                vision: 4,
                can_capture: false,
                cost: 28000,
                class: UnitClass::Naval,
            },
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            // Foot
            UnitType::Scout => "Scout",
            UnitType::Shocktrooper => "Shocktrooper",
            // Ground vehicles
            UnitType::Recon => "Recon",
            UnitType::Ironclad => "Ironclad",
            UnitType::Juggernaut => "Juggernaut",
            UnitType::Behemoth => "Behemoth",
            UnitType::Flak => "Flak",
            UnitType::Siege => "Siege",
            UnitType::Barrage => "Barrage",
            UnitType::Stinger => "Stinger",
            // Ground support
            UnitType::Carrier => "Carrier",
            UnitType::Supplier => "Supplier",
            // Air
            UnitType::Ferrier => "Ferrier",
            UnitType::Skywing => "Skywing",
            UnitType::Raptor => "Raptor",
            UnitType::Talon => "Talon",
            // Naval
            UnitType::Barge => "Barge",
            UnitType::Frigate => "Frigate",
            UnitType::Lurker => "Lurker",
            UnitType::Dreadnought => "Dreadnought",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            // Foot
            UnitType::Scout => "Light foot soldier. Cheap and can capture buildings.",
            UnitType::Shocktrooper => "Heavy foot soldier. Strong against armor.",
            // Ground vehicles
            UnitType::Recon => "Fast scout vehicle. Great mobility on roads.",
            UnitType::Ironclad => "Main battle armor. Powerful and well-protected.",
            UnitType::Juggernaut => "Heavy battle armor. Devastating firepower.",
            UnitType::Behemoth => "Super-heavy armor. The ultimate ground unit.",
            UnitType::Flak => "Anti-air vehicle. Shreds aircraft.",
            UnitType::Siege => "Medium-range artillery. Indirect fire.",
            UnitType::Barrage => "Long-range rockets. Massive area damage.",
            UnitType::Stinger => "Long-range anti-air missiles. Locks down airspace.",
            // Ground support
            UnitType::Carrier => "Transports foot units across the battlefield.",
            UnitType::Supplier => "Resupplies ammunition and repairs nearby units.",
            // Air
            UnitType::Ferrier => "Transport helicopter. Carries foot units.",
            UnitType::Skywing => "Attack helicopter. Versatile against ground and air.",
            UnitType::Raptor => "Air superiority fighter. Dominates the skies.",
            UnitType::Talon => "Heavy bomber. Devastating ground attacks.",
            // Naval
            UnitType::Barge => "Naval transport. Carries ground units across water.",
            UnitType::Frigate => "Fast warship. Anti-air and anti-submarine.",
            UnitType::Lurker => "Submarine. Stealth attacks on ships.",
            UnitType::Dreadnought => "Battleship. Massive long-range bombardment.",
        }
    }

    pub fn symbol(&self) -> &'static str {
        match self {
            // Foot
            UnitType::Scout => "Sc",
            UnitType::Shocktrooper => "Sh",
            // Ground vehicles
            UnitType::Recon => "Rc",
            UnitType::Ironclad => "Ir",
            UnitType::Juggernaut => "Jg",
            UnitType::Behemoth => "Bh",
            UnitType::Flak => "Fk",
            UnitType::Siege => "Si",
            UnitType::Barrage => "Br",
            UnitType::Stinger => "St",
            // Ground support
            UnitType::Carrier => "Ca",
            UnitType::Supplier => "Su",
            // Air
            UnitType::Ferrier => "Fe",
            UnitType::Skywing => "Sw",
            UnitType::Raptor => "Rp",
            UnitType::Talon => "Ta",
            // Naval
            UnitType::Barge => "Ba",
            UnitType::Frigate => "Fr",
            UnitType::Lurker => "Lu",
            UnitType::Dreadnought => "Dr",
        }
    }

    #[allow(dead_code)]
    pub fn class(&self) -> UnitClass {
        self.stats().class
    }

    /// Get the asset file name for this unit type
    pub fn asset_file_name(&self) -> &'static str {
        match self {
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

    /// Get all unit type variants
    pub fn all() -> &'static [UnitType] {
        &[
            UnitType::Scout,
            UnitType::Shocktrooper,
            UnitType::Recon,
            UnitType::Ironclad,
            UnitType::Juggernaut,
            UnitType::Behemoth,
            UnitType::Flak,
            UnitType::Siege,
            UnitType::Barrage,
            UnitType::Stinger,
            UnitType::Carrier,
            UnitType::Supplier,
            UnitType::Ferrier,
            UnitType::Skywing,
            UnitType::Raptor,
            UnitType::Talon,
            UnitType::Barge,
            UnitType::Frigate,
            UnitType::Lurker,
            UnitType::Dreadnought,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct UnitStats {
    pub max_hp: i32,
    pub attack: i32,
    pub defense: i32,
    pub movement: u32,
    pub attack_range: (u32, u32),  // (min, max) range
    pub vision: u32,              // Vision range for fog of war
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

    #[allow(dead_code)]
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

    /// Convert grid position to world coordinates (3D: grid Y -> world Z)
    pub fn to_world(&self, map: &GameMap) -> Vec3 {
        let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
        let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
        Vec3::new(
            self.x as f32 * TILE_SIZE + offset_x,
            0.0,  // Ground level (Y is up in 3D)
            self.y as f32 * TILE_SIZE + offset_z,  // Grid Y -> World Z
        )
    }

    pub fn distance_to(&self, other: &GridPosition) -> u32 {
        ((self.x - other.x).abs() + (self.y - other.y).abs()) as u32
    }
}

/// Marker for selected unit
#[derive(Component)]
#[allow(dead_code)]
pub struct Selected;

/// Marker for HP display text (child of unit)
#[derive(Component)]
pub struct HpDisplay;

/// Marker for unit type symbol text (child of unit)
#[derive(Component)]
pub struct UnitSymbol;

pub fn spawn_unit(
    commands: &mut Commands,
    map: &GameMap,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    faction: Faction,
    unit_type: UnitType,
    x: i32,
    y: i32,
) {
    let grid_pos = GridPosition::new(x, y);
    let world_pos = grid_pos.to_world(map);

    // Determine Y height based on unit class (air units float higher)
    let stats = unit_type.stats();
    let unit_height = match stats.class {
        UnitClass::Air | UnitClass::AirTransport => 24.0,  // Float above ground
        _ => 12.0,  // Ground units sit at eye level
    };

    // Get color from sprite assets or use faction color as fallback
    let unit_color = match sprite_assets.get_unit_sprite(images, faction, unit_type) {
        super::SpriteSource::Image(_handle) => faction.color(), // TODO: Use texture when available
        super::SpriteSource::Procedural { color, .. } => color,
    };

    // Create a vertical quad mesh for the unit (billboard will rotate it to face camera)
    let unit_size = Vec2::new(TILE_SIZE * 0.7, TILE_SIZE * 0.6);
    let unit_mesh = meshes.add(Rectangle::new(unit_size.x, unit_size.y));

    // Unit as 3D mesh with billboard behavior
    commands.spawn((
        Mesh3d(unit_mesh),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: unit_color,
            unlit: true,  // No lighting effects, flat color
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        Transform::from_xyz(world_pos.x, unit_height, world_pos.z),
        Unit::new(unit_type),
        GridPosition::new(x, y),
        FactionMember { faction },
        Billboard,  // Face the camera
    )).with_children(|parent| {
        // Shadow on the ground (flat oval)
        let shadow_mesh = meshes.add(Ellipse::new(TILE_SIZE * 0.25, TILE_SIZE * 0.08));
        parent.spawn((
            Mesh3d(shadow_mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.0, 0.0, 0.0, 0.3),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_xyz(0.0, -unit_height + 0.05, 0.0)
                .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
            UnitShadow,
        ));
    });
}

/// Update HP display text for all units
fn update_hp_displays(
    units: Query<(&Unit, &Children)>,
    mut hp_displays: Query<&mut Text2d, With<HpDisplay>>,
) {
    for (unit, children) in units.iter() {
        for child in children.iter() {
            if let Ok(mut text) = hp_displays.get_mut(*child) {
                // HP 1-10 display (ceiling of HP/10)
                let hp_display = ((unit.hp as f32) / 10.0).ceil() as i32;
                let hp_display = hp_display.clamp(1, 10);
                // Don't show "10", show nothing or could show a different indicator
                if hp_display == 10 {
                    **text = String::new();
                } else {
                    **text = hp_display.to_string();
                }
            }
        }
    }
}
