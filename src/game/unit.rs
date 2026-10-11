use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Faction, FactionMember, GameMap, GameData, TILE_SIZE, UnitShadow, Billboard};

pub struct UnitPlugin;

impl Plugin for UnitPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (
            update_hp_displays,
            animate_unit_movement,
            update_moved_unit_visuals,
            orient_unit_sprites,
            place_units_on_ground,
            animate_unit_sprites,
            keep_unit_bodies_true_to_size,
        ));
    }
}

/// World size of a sprite quad. Map sprites (units and features) are 32x32,
/// the same pixel grid as the 32x32 terrain tiles, so one sprite spans one tile.
pub const SPRITE_WORLD_SIZE: f32 = TILE_SIZE;

/// Marker for units drawn with a sprite (instead of a colored card + token)
#[derive(Component)]
pub struct UnitSprite;

/// Sprites are drawn facing right: the player's units keep that, enemies are
/// mirrored to face them (as in Advance Wars)
fn orient_unit_sprites(
    battle_config: Res<super::BattleConfig>,
    mut units: Query<(&FactionMember, &mut Transform), With<UnitSprite>>,
) {
    for (faction, mut transform) in units.iter_mut() {
        let facing = if faction.faction == battle_config.player_faction { 1.0 } else { -1.0 };
        if transform.scale.x != facing {
            transform.scale.x = facing;
        }
    }
}

/// Animation frames in a unit's sprite sheet (a horizontal strip of square frames)
#[derive(Component)]
pub struct SpriteFrames {
    pub count: u32,
    shown: u32,
}

/// How far a unit's visible body sits in front of its tile, toward the camera.
/// As in Advance Wars, units always draw over terrain features and buildings,
/// even a tall one standing on the tile in front.
const UNIT_DRAW_NUDGE: f32 = TILE_SIZE;

/// The visible sprite (or card) of a unit: a child of the unit entity, which
/// stays on its tile so game logic and animation use the real position
#[derive(Component)]
pub struct UnitBody;

/// Place each unit's body on the straight line from its tile to the camera, so
/// it draws in front of terrain without shifting on screen, and shrink it by
/// the same ratio it moved closer, so it keeps its size. (Nudging along the
/// camera's forward axis instead slides off-center units sideways, more the
/// farther they are from the middle of the view.)
fn keep_unit_bodies_true_to_size(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    units: Query<(&Transform, &Children), With<Unit>>,
    mut bodies: Query<&mut Transform, (With<UnitBody>, Without<Unit>)>,
) {
    let Ok(camera) = camera.single() else { return };
    for (unit, children) in units.iter() {
        let to_camera = camera.translation() - unit.translation;
        let distance = to_camera.length().max(UNIT_DRAW_NUDGE * 2.0);
        let ratio = (distance - UNIT_DRAW_NUDGE) / distance;
        // The offset in the unit's own frame (billboard rotation, mirrored facing)
        let mut local = unit.rotation.inverse() * (to_camera / distance * UNIT_DRAW_NUDGE);
        local /= unit.scale;
        for child in children.iter() {
            if let Ok(mut body) = bodies.get_mut(child) {
                body.translation = local;
                body.scale = Vec3::new(ratio, ratio, 1.0);
            }
        }
    }
}

/// Number of square frames in a sprite image laid out as a horizontal strip
pub fn frame_count(image: &Image) -> u32 {
    let (w, h) = (image.width(), image.height());
    if h > 0 && w > h && w % h == 0 { w / h } else { 1 }
}

/// Advance Wars style idle: every unit steps through its frames in sync, slowly
/// at rest and faster while selected or moving; finished units stand still
fn animate_unit_sprites(
    time: Res<Time>,
    highlights: Res<super::MovementHighlights>,
    mut units: Query<(Entity, &Unit, &UnitVisuals, &mut SpriteFrames, Has<UnitAnimation>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    const IDLE_FPS: f32 = 3.0;
    const ACTIVE_FPS: f32 = 8.0;
    let t = time.elapsed_secs();
    for (entity, unit, visuals, mut frames, moving) in units.iter_mut() {
        // Units that have acted are dimmed but keep marching in place
        let active = !unit.exhausted && (moving || highlights.selected_unit == Some(entity));
        let frame = (t * if active { ACTIVE_FPS } else { IDLE_FPS }) as u32 % frames.count;
        if frame == frames.shown {
            continue;
        }
        frames.shown = frame;
        if let Some(material) = materials.get_mut(&visuals.material_handle) {
            material.uv_transform = frame_uv(frame, frames.count);
        }
    }
}

fn frame_uv(frame: u32, count: u32) -> bevy::math::Affine2 {
    let width = 1.0 / count as f32;
    bevy::math::Affine2::from_scale_angle_translation(Vec2::new(width, 1.0), 0.0, Vec2::new(frame as f32 * width, 0.0))
}

/// Height of a unit's center above the ground it stands on (half its sprite,
/// plus a hover height for flyers)
#[derive(Component)]
pub struct GroundOffset(pub f32);

/// Keep units standing on top of whichever tile they're over, including mid-move,
/// since tiles have different heights
fn place_units_on_ground(
    map: Res<GameMap>,
    game_data: Res<GameData>,
    mut units: Query<(&mut Transform, &GroundOffset)>,
) {
    if map.width == 0 {
        return;
    }
    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    for (mut transform, ground) in units.iter_mut() {
        let x = ((transform.translation.x - offset_x) / TILE_SIZE).round() as i32;
        let y = ((transform.translation.z - offset_z) / TILE_SIZE).round() as i32;
        let Some(terrain) = map.get(x, y) else { continue };
        let target = game_data.terrain(terrain).tile_height + ground.0;
        if (transform.translation.y - target).abs() > 0.01 {
            transform.translation.y = target;
        }
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

/// Marker component: unit should be exhausted when animation completes (auto-wait)
#[derive(Component)]
pub struct AutoExhaust;

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
    /// Create a movement animation following a path of waypoints. Build them with
    /// `route_waypoints` so units walk tile by tile and never move diagonally.
    pub fn from_path(waypoints: Vec<Vec3>) -> Self {
        Self {
            waypoints,
            current_waypoint: 1,
            segment_progress: 0.0,
            speed: 5.0, // Complete each tile in ~0.2 seconds
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
    mut units: Query<(Entity, &mut Transform, &mut UnitAnimation, &mut Unit, Option<&AutoExhaust>)>,
) {
    for (entity, mut transform, mut animation, mut unit, auto_exhaust) in units.iter_mut() {
        if animation.is_complete() {
            // Animation done - snap to final position and remove component
            let final_pos = animation.final_position();
            transform.translation.x = final_pos.x;
            transform.translation.z = final_pos.z;
            // If flagged for auto-exhaust (no actions available), mark unit done
            if auto_exhaust.is_some() {
                unit.exhausted = true;
                commands.entity(entity).remove::<AutoExhaust>();
            }
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
            // If flagged for auto-exhaust (no actions available), mark unit done
            if auto_exhaust.is_some() {
                unit.exhausted = true;
                commands.entity(entity).remove::<AutoExhaust>();
            }
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

/// System to darken exhausted units (like Advance Wars)
fn update_moved_unit_visuals(
    units: Query<(&Unit, &UnitVisuals), Changed<Unit>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (unit, visuals) in units.iter() {
        if let Some(material) = materials.get_mut(&visuals.material_handle) {
            if unit.exhausted {
                // Darken the unit while preserving hue
                let base = visuals.base_color.to_srgba();
                let darken_factor = 0.68; // dimmed but still readable
                material.base_color = Color::srgba(
                    base.red * darken_factor,
                    base.green * darken_factor,
                    base.blue * darken_factor,
                    base.alpha,
                );
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

impl UnitClass {
    pub fn all() -> &'static [UnitClass] {
        &[
            UnitClass::Foot, UnitClass::Wheels, UnitClass::Treads, UnitClass::Air,
            UnitClass::Naval, UnitClass::Transport, UnitClass::AirTransport, UnitClass::NavalTransport,
        ]
    }
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

/// Component for unit entities
#[derive(Component, Debug, Clone)]
#[require(super::BattleEntity)]
pub struct Unit {
    pub unit_type: UnitType,
    pub hp: i32,
    pub stamina: u32,
    pub ammo: u32,
    /// Whether the unit has physically moved this turn
    pub moved: bool,
    /// Whether the unit has performed an action (attack, capture, etc.) this turn
    pub attacked: bool,
    /// Whether the unit's turn is complete (darkened, can't be selected)
    pub exhausted: bool,
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

    /// Rebuild the unit when unloaded. It has been carried, so it can't act this turn.
    pub fn to_unit(&self) -> Unit {
        Unit {
            unit_type: self.unit_type,
            hp: self.hp,
            stamina: self.stamina,
            ammo: self.ammo,
            moved: true,
            attacked: false,
            exhausted: true,
            cargo: None,
        }
    }
}

impl Unit {
    pub fn new(unit_type: UnitType, game_data: &GameData) -> Self {
        let stats = game_data.unit_stats(unit_type);
        Self {
            unit_type,
            hp: stats.max_hp,
            stamina: stats.max_stamina,
            ammo: stats.max_ammo,
            moved: false,
            attacked: false,
            exhausted: false,
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

    pub fn hp_percentage(&self, game_data: &GameData) -> f32 {
        self.hp as f32 / game_data.unit_stats(self.unit_type).max_hp as f32
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

/// Marker for HP display text (child of unit)
#[derive(Component)]
pub struct HpDisplay;

pub fn spawn_unit(
    commands: &mut Commands,
    map: &GameMap,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    game_data: &GameData,
    faction: Faction,
    unit_type: UnitType,
    x: i32,
    y: i32,
) {
    let unit = Unit::new(unit_type, game_data);
    spawn_unit_with_state(commands, map, meshes, materials, sprite_assets, images, game_data, faction, unit, x, y);
}

/// Spawn a unit with existing state (for loading saves)
pub fn spawn_unit_with_state(
    commands: &mut Commands,
    map: &GameMap,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    sprite_assets: &super::SpriteAssets,
    images: &Assets<Image>,
    game_data: &GameData,
    faction: Faction,
    unit: Unit,
    x: i32,
    y: i32,
) {
    let grid_pos = GridPosition::new(x, y);
    let world_pos = grid_pos.to_world(map);

    let unit_type = unit.unit_type;
    let sprite = sprite_assets.unit_sprite(images, faction, unit_type);
    let frames = sprite.as_ref().and_then(|h| images.get(h)).map_or(1, frame_count);
    let unit_size = if sprite.is_some() {
        Vec2::splat(SPRITE_WORLD_SIZE)
    } else {
        Vec2::new(TILE_SIZE * 0.7, TILE_SIZE * 0.6)
    };

    // Position so bottom of sprite is at ground level (sprite is centered on transform)
    let ground_offset = unit_size.y / 2.0 + 1.0;
    let unit_height = match game_data.unit_class(unit_type) {
        UnitClass::Air | UnitClass::AirTransport => ground_offset + 20.0,  // Float above ground
        _ => ground_offset,
    };

    // Sprites are drawn as-is (white tint); without one the card takes the faction color
    let unit_color = if sprite.is_some() { Color::WHITE } else { game_data.faction_color(faction) };

    let unit_mesh = meshes.add(Rectangle::new(unit_size.x, unit_size.y));

    // Create material with handle so we can update it for moved indicator.
    // Sprites use alpha masking so they write depth and sort correctly
    // against terrain features.
    let unit_material = materials.add(StandardMaterial {
        base_color: unit_color,
        base_color_texture: sprite.clone(),
        unlit: true,
        alpha_mode: if sprite.is_some() { AlphaMode::Mask(0.5) } else { AlphaMode::Blend },
        uv_transform: frame_uv(0, frames),
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    // Sprites stand on the ground in faction color, so sides stay readable
    let shadow_color = if sprite.is_some() {
        game_data.faction_color(faction).with_alpha(0.55)
    } else {
        Color::srgba(0.0, 0.0, 0.0, 0.3)
    };

    let ground = map.get(x, y).map_or(0.0, |t| game_data.terrain(t).tile_height);
    let mut entity = commands.spawn((
        // Tile center; the visible body is a child nudged toward the camera
        Transform::from_xyz(world_pos.x, ground + unit_height, world_pos.z),
        Visibility::default(),
        GroundOffset(unit_height),
        unit,  // Use provided unit with existing state
        GridPosition::new(x, y),
        FactionMember { faction },
        Billboard,
        UnitVisuals {
            base_color: unit_color,
            material_handle: unit_material.clone(),
        },
    ));
    if sprite.is_some() {
        entity.insert(UnitSprite);
    }
    if frames > 1 {
        entity.insert(SpriteFrames { count: frames, shown: 0 });
    }
    entity.with_children(|parent| {
        parent.spawn((
            Mesh3d(unit_mesh.clone()),
            MeshMaterial3d(unit_material.clone()),
            Transform::from_xyz(0.0, 0.0, UNIT_DRAW_NUDGE),
            UnitBody,
        ));
        // Colored cards get a dark frame for contrast; sprites have their own outline
        if sprite.is_none() {
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
                Transform::from_xyz(0.0, 0.0, UNIT_DRAW_NUDGE - 1.0),
            ));
        }

        let shadow_mesh = meshes.add(Ellipse::new(TILE_SIZE * 0.3, TILE_SIZE * 0.1));
        parent.spawn((
            Mesh3d(shadow_mesh),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: shadow_color,
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
