use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Faction, FactionMember, GameMap, TILE_SIZE, UnitShadow, Billboard};

pub struct UnitPlugin;

impl Plugin for UnitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
            update_hp_displays,
            animate_unit_movement,
            update_moved_unit_visuals,
        ));
    }
}

/// Component to track unit's visual state for moved indicator
#[derive(Component)]
pub struct UnitVisuals {
    /// The unit's base color (faction color)
    pub base_color: Color,
    /// Handle to the unit's material for updating
    pub material_handle: Handle<StandardMaterial>,
}

/// Component for smooth unit movement animation along a path
#[derive(Component)]
pub struct UnitAnimation {
    /// Waypoints to move through (world positions)
    pub waypoints: Vec<Vec3>,
    /// Current waypoint index (moving toward waypoints[current_waypoint])
    pub current_waypoint: usize,
    /// Progress within current segment (0.0 to 1.0)
    pub segment_progress: f32,
    /// Animation speed (segments per second, higher = faster)
    pub speed: f32,
}

impl UnitAnimation {
    /// Create a new movement animation with a single destination (backwards compatible)
    pub fn new(start: Vec3, end: Vec3) -> Self {
        Self {
            waypoints: vec![start, end],
            current_waypoint: 1,
            segment_progress: 0.0,
            speed: 5.0, // Complete each tile in ~0.2 seconds
        }
    }

    /// Create a new movement animation following a path of waypoints
    pub fn from_path(waypoints: Vec<Vec3>) -> Self {
        Self {
            waypoints,
            current_waypoint: 1,
            segment_progress: 0.0,
            speed: 5.0,
        }
    }

    /// Check if animation is complete (reached final waypoint)
    pub fn is_complete(&self) -> bool {
        self.current_waypoint >= self.waypoints.len()
    }

    /// Get current start position (previous waypoint)
    fn current_start(&self) -> Vec3 {
        self.waypoints.get(self.current_waypoint.saturating_sub(1))
            .copied()
            .unwrap_or(Vec3::ZERO)
    }

    /// Get current target position
    fn current_target(&self) -> Vec3 {
        self.waypoints.get(self.current_waypoint)
            .copied()
            .unwrap_or(self.current_start())
    }

    /// Get final destination
    pub fn final_position(&self) -> Vec3 {
        self.waypoints.last().copied().unwrap_or(Vec3::ZERO)
    }
}

/// System to animate unit movement smoothly along waypoints
fn animate_unit_movement(
    mut commands: Commands,
    time: Res<Time>,
    mut units: Query<(Entity, &mut Transform, &mut UnitAnimation)>,
) {
    for (entity, mut transform, mut animation) in units.iter_mut() {
        if animation.is_complete() {
            // Animation done - snap to final position and remove component
            let final_pos = animation.final_position();
            transform.translation.x = final_pos.x;
            transform.translation.z = final_pos.z;
            commands.entity(entity).remove::<UnitAnimation>();
            continue;
        }

        // Update segment progress
        animation.segment_progress += time.delta_secs() * animation.speed;

        // Check if we've completed the current segment
        while animation.segment_progress >= 1.0 && !animation.is_complete() {
            animation.segment_progress -= 1.0;
            animation.current_waypoint += 1;
        }

        if animation.is_complete() {
            // Finished all segments
            let final_pos = animation.final_position();
            transform.translation.x = final_pos.x;
            transform.translation.z = final_pos.z;
            commands.entity(entity).remove::<UnitAnimation>();
        } else {
            // Interpolate within current segment
            let start = animation.current_start();
            let target = animation.current_target();

            // Smooth interpolation using smoothstep for nice easing
            let t = animation.segment_progress.min(1.0);
            let t_smooth = t * t * (3.0 - 2.0 * t);

            // Lerp position (only X and Z, preserve Y height)
            transform.translation.x = start.x + (target.x - start.x) * t_smooth;
            transform.translation.z = start.z + (target.z - start.z) * t_smooth;
        }
    }
}

/// System to gray out moved units (like Advance Wars)
fn update_moved_unit_visuals(
    units: Query<(&Unit, &UnitVisuals), Changed<Unit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (unit, visuals) in units.iter() {
        if let Some(material) = materials.get_mut(&visuals.material_handle) {
            if unit.moved {
                // Gray out the unit - desaturate and darken
                let base = visuals.base_color.to_srgba();
                // Convert to grayscale and darken
                let gray = (base.red * 0.3 + base.green * 0.5 + base.blue * 0.2) * 0.6;
                material.base_color = Color::srgba(gray, gray, gray, base.alpha);
            } else {
                // Restore original color
                material.base_color = visuals.base_color;
            }
        }
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
                max_stamina: 99,
                max_ammo: 0,  // Claws/teeth - unlimited
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
                max_stamina: 70,
                max_ammo: 3,  // Heavy weapon durability
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
                max_stamina: 80,
                max_ammo: 0,  // Light weapon - unlimited
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
                max_stamina: 70,
                max_ammo: 9,
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
                max_stamina: 50,
                max_ammo: 8,
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
                max_stamina: 99,
                max_ammo: 9,
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
                max_stamina: 60,
                max_ammo: 9,
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
                max_stamina: 50,
                max_ammo: 6,
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
                max_stamina: 50,
                max_ammo: 6,
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
                max_stamina: 50,
                max_ammo: 6,
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
                max_stamina: 70,
                max_ammo: 0,  // No weapon
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
                max_stamina: 70,
                max_ammo: 0,  // No weapon
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
                max_stamina: 99,
                max_ammo: 0,  // No weapon
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
                max_stamina: 99,
                max_ammo: 6,
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
                max_stamina: 99,
                max_ammo: 9,
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
                max_stamina: 99,
                max_ammo: 9,
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
                max_stamina: 99,
                max_ammo: 0,  // No weapon
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
                max_stamina: 99,
                max_ammo: 9,
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
                max_stamina: 70,
                max_ammo: 6,
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
                max_stamina: 99,
                max_ammo: 9,
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

    /// Get unit production cost (base cost before CO modifiers)
    pub fn cost(&self) -> u32 {
        match self {
            // Foot units
            UnitType::Scout => 10,
            UnitType::Shocktrooper => 30,
            // Wheeled
            UnitType::Recon => 40,
            UnitType::Supplier => 50,
            // Treads
            UnitType::Ironclad => 70,
            UnitType::Juggernaut => 120,
            UnitType::Behemoth => 160,
            UnitType::Flak => 80,
            UnitType::Carrier => 50,
            // Artillery
            UnitType::Siege => 60,
            UnitType::Barrage => 90,
            UnitType::Stinger => 120,
            // Air
            UnitType::Ferrier => 50,
            UnitType::Skywing => 90,
            UnitType::Raptor => 120,
            UnitType::Talon => 200,
            // Naval
            UnitType::Barge => 60,
            UnitType::Frigate => 80,
            UnitType::Lurker => 100,
            UnitType::Dreadnought => 280,
        }
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
    pub max_stamina: u32,         // Movement stamina (consumed per tile moved)
    pub max_ammo: u32,            // Weapon uses (ammo for ranged, durability for melee, 0 = unlimited)
}

/// Component for unit entities
#[derive(Component, Debug, Clone)]
pub struct Unit {
    pub unit_type: UnitType,
    pub hp: i32,
    pub stamina: u32,
    pub ammo: u32,
    pub moved: bool,
    pub attacked: bool,
    /// Cargo - for transport units, stores the type and HP of the loaded unit
    /// We store unit info rather than Entity to avoid complex entity relationships
    pub cargo: Option<CargoUnit>,
}

/// Represents a unit being carried by a transport
#[derive(Debug, Clone)]
pub struct CargoUnit {
    pub unit_type: UnitType,
    pub hp: i32,
    pub stamina: u32,
    pub ammo: u32,
}

impl CargoUnit {
    pub fn from_unit(unit: &Unit) -> Self {
        Self {
            unit_type: unit.unit_type,
            hp: unit.hp,
            stamina: unit.stamina,
            ammo: unit.ammo,
        }
    }

    pub fn to_unit(&self) -> Unit {
        Unit {
            unit_type: self.unit_type,
            hp: self.hp,
            stamina: self.stamina,
            ammo: self.ammo,
            moved: true, // Unloaded units can't move again this turn
            attacked: false,
            cargo: None,
        }
    }
}

impl Unit {
    pub fn new(unit_type: UnitType) -> Self {
        let stats = unit_type.stats();
        Self {
            unit_type,
            hp: stats.max_hp,
            stamina: stats.max_stamina,
            ammo: stats.max_ammo,
            moved: false,
            attacked: false,
            cargo: None,
        }
    }

    /// Check if this unit can carry other units
    pub fn is_transport(&self) -> bool {
        matches!(self.unit_type, UnitType::Carrier | UnitType::Ferrier | UnitType::Barge)
    }

    /// Check if this unit can be loaded into a transport
    pub fn can_be_transported(&self) -> bool {
        matches!(self.unit_type, UnitType::Scout | UnitType::Shocktrooper)
    }

    /// Check if this transport has cargo
    pub fn has_cargo(&self) -> bool {
        self.cargo.is_some()
    }

    pub fn hp_percentage(&self) -> f32 {
        self.hp as f32 / self.unit_type.stats().max_hp as f32
    }

    pub fn stamina_percentage(&self) -> f32 {
        let stats = self.unit_type.stats();
        if stats.max_stamina == 0 { return 1.0; }
        self.stamina as f32 / stats.max_stamina as f32
    }

    pub fn ammo_percentage(&self) -> f32 {
        let stats = self.unit_type.stats();
        if stats.max_ammo == 0 { return 1.0; }
        self.ammo as f32 / stats.max_ammo as f32
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

    // Create a vertical quad mesh for the unit (billboard will rotate it to face camera)
    let unit_size = Vec2::new(TILE_SIZE * 0.7, TILE_SIZE * 0.6);

    // Determine Y height based on unit class
    // Position so bottom of sprite is at ground level (sprite is centered on transform)
    let stats = unit_type.stats();
    let ground_offset = unit_size.y / 2.0 + 1.0;  // Half sprite height + small gap
    let unit_height = match stats.class {
        UnitClass::Air | UnitClass::AirTransport => ground_offset + 20.0,  // Float above ground
        _ => ground_offset,  // Ground units: bottom at ground level
    };

    // Get color from sprite assets or use faction color as fallback
    let unit_color = match sprite_assets.get_unit_sprite(images, faction, unit_type) {
        super::SpriteSource::Image(_handle) => faction.color(), // TODO: Use texture when available
        super::SpriteSource::Procedural { color, .. } => color,
    };

    let unit_mesh = meshes.add(Rectangle::new(unit_size.x, unit_size.y));

    // Create material with handle so we can update it for moved indicator
    let unit_material = materials.add(StandardMaterial {
        base_color: unit_color,
        unlit: true,  // No lighting effects, flat color
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    // Unit as 3D mesh with billboard behavior
    commands.spawn((
        Mesh3d(unit_mesh.clone()),
        MeshMaterial3d(unit_material.clone()),
        // Offset units forward (negative Z) so they render in front of terrain features
        Transform::from_xyz(world_pos.x, unit_height, world_pos.z - 10.0),
        Unit::new(unit_type),
        GridPosition::new(x, y),
        FactionMember { faction },
        Billboard,  // Face the camera
        UnitVisuals {
            base_color: unit_color,
            material_handle: unit_material,
        },
    )).with_children(|parent| {
        // Dark outline/border around the unit for contrast
        // Positioned slightly in front so it's visible as a frame
        let border_mesh = meshes.add(Rectangle::new(unit_size.x + 6.0, unit_size.y + 6.0));
        parent.spawn((
            Mesh3d(border_mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.1, 0.1, 0.1, 0.9),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_xyz(0.0, 0.0, -1.0),  // Behind the unit mesh
        ));

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

/// Spawn a unit with existing state (for loading saves)
pub fn spawn_unit_with_state(
    commands: &mut Commands,
    map: &GameMap,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    faction: Faction,
    unit: Unit,
    x: i32,
    y: i32,
) {
    let grid_pos = GridPosition::new(x, y);
    let world_pos = grid_pos.to_world(map);

    let unit_type = unit.unit_type;
    let unit_size = Vec2::new(TILE_SIZE * 0.7, TILE_SIZE * 0.6);

    let stats = unit_type.stats();
    let ground_offset = unit_size.y / 2.0 + 1.0;
    let unit_height = match stats.class {
        UnitClass::Air | UnitClass::AirTransport => ground_offset + 20.0,
        _ => ground_offset,
    };

    let unit_color = match sprite_assets.get_unit_sprite(images, faction, unit_type) {
        super::SpriteSource::Image(_handle) => faction.color(),
        super::SpriteSource::Procedural { color, .. } => color,
    };

    let unit_mesh = meshes.add(Rectangle::new(unit_size.x, unit_size.y));

    // Create material with handle so we can update it for moved indicator
    let unit_material = materials.add(StandardMaterial {
        base_color: unit_color,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });

    commands.spawn((
        Mesh3d(unit_mesh.clone()),
        MeshMaterial3d(unit_material.clone()),
        // Offset units forward (negative Z) so they render in front of terrain features
        Transform::from_xyz(world_pos.x, unit_height, world_pos.z - 10.0),
        unit,  // Use provided unit with existing state
        GridPosition::new(x, y),
        FactionMember { faction },
        Billboard,
        UnitVisuals {
            base_color: unit_color,
            material_handle: unit_material,
        },
    )).with_children(|parent| {
        let border_mesh = meshes.add(Rectangle::new(unit_size.x + 6.0, unit_size.y + 6.0));
        parent.spawn((
            Mesh3d(border_mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: Color::srgba(0.1, 0.1, 0.1, 0.9),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                cull_mode: None,
                ..default()
            })),
            Transform::from_xyz(0.0, 0.0, -1.0),
        ));

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
            if let Ok(mut text) = hp_displays.get_mut(child) {
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
