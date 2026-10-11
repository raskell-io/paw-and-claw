//! Short battle effects drawn over the board, like the dust poof of a defeated unit

use bevy::prelude::*;

use super::{Billboard, GameData, GameMap, GridPosition, SPRITE_WORLD_SIZE, TILE_SIZE};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<UnitDefeated>()
            .add_message::<PlayEffect>()
            .add_systems(Startup, load_effect_sprites)
            .add_systems(Update, (defeats_poof, spawn_effects, animate_effects, follow_camera_line).chain());
    }
}

/// A unit's HP reached 0 on this tile
#[derive(Message)]
pub struct UnitDefeated {
    pub position: GridPosition,
}

/// The effect sheets in `assets/sprites/effects/` (tools/sprites/effects.py)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectKind {
    /// A defeated unit vanishes in dust
    Poof,
    /// A unit healed by a power
    Heal,
    /// A unit hit by a power
    Blast,
    /// A strike power lands on a tile
    Rockfall,
}

impl EffectKind {
    const ALL: [EffectKind; 4] = [EffectKind::Poof, EffectKind::Heal, EffectKind::Blast, EffectKind::Rockfall];

    fn sheet(self) -> &'static str {
        match self {
            EffectKind::Poof => "sprites/effects/poof.png",
            EffectKind::Heal => "sprites/effects/heal.png",
            EffectKind::Blast => "sprites/effects/blast.png",
            EffectKind::Rockfall => "sprites/effects/rockfall.png",
        }
    }

    fn frames(self) -> u32 {
        match self {
            EffectKind::Rockfall => 8,
            _ => 6,
        }
    }

    fn fps(self) -> f32 {
        match self {
            EffectKind::Heal => 9.0,
            _ => 12.0,
        }
    }
}

/// Play an effect on a tile, optionally after a delay (so power effects land
/// as the cut-in banner leaves)
#[derive(Message, Clone, Copy, Debug)]
pub struct PlayEffect {
    pub kind: EffectKind,
    pub position: GridPosition,
    pub delay: f32,
}

#[derive(Resource)]
struct EffectSprites(std::collections::HashMap<EffectKind, Handle<Image>>);

fn load_effect_sprites(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(EffectSprites(
        EffectKind::ALL.iter().map(|&kind| (kind, asset_server.load(kind.sheet()))).collect(),
    ));
}

/// A one-shot sprite animation that removes itself when done
#[derive(Component)]
#[require(super::BattleEntity)]
struct Effect {
    frames: u32,
    shown: u32,
    delay: Timer,
    timer: Timer,
    material: Handle<StandardMaterial>,
    /// The spot a unit on this tile stands at
    anchor: Vec3,
}

/// How far toward the camera effects are drawn: a bit more than unit bodies
/// (`UNIT_DRAW_NUDGE`), so they play over the unit they belong to
const EFFECT_DRAW_NUDGE: f32 = TILE_SIZE * 1.3;

/// Move each effect along the line from its tile to the camera and shrink it by
/// the same ratio, like unit bodies, so it sits exactly over the unit on screen
fn follow_camera_line(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut effects: Query<(&Effect, &mut Transform)>,
) {
    let Ok(camera) = camera.single() else { return };
    for (effect, mut transform) in effects.iter_mut() {
        let to_camera = camera.translation() - effect.anchor;
        let distance = to_camera.length().max(EFFECT_DRAW_NUDGE * 2.0);
        let ratio = (distance - EFFECT_DRAW_NUDGE) / distance;
        transform.translation = effect.anchor + to_camera / distance * EFFECT_DRAW_NUDGE;
        transform.scale = Vec3::new(ratio, ratio, 1.0);
    }
}

fn defeats_poof(mut defeated: MessageReader<UnitDefeated>, mut effects: MessageWriter<PlayEffect>) {
    for event in defeated.read() {
        effects.write(PlayEffect { kind: EffectKind::Poof, position: event.position, delay: 0.0 });
    }
}

fn spawn_effects(
    mut events: MessageReader<PlayEffect>,
    mut commands: Commands,
    sprites: Option<Res<EffectSprites>>,
    map: Res<GameMap>,
    game_data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(sprites) = sprites else { return };
    for event in events.read() {
        let Some(sheet) = sprites.0.get(&event.kind) else { continue };
        let frames = event.kind.frames();
        let world = event.position.to_world(&map);
        let ground = map.get(event.position.x, event.position.y).map_or(4.0, |t| game_data.terrain(t).tile_height);
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(sheet.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Mask(0.5),
            uv_transform: frame_uv(0, frames),
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Mesh3d(meshes.add(Rectangle::new(SPRITE_WORLD_SIZE, SPRITE_WORLD_SIZE))),
            MeshMaterial3d(material.clone()),
            // Where a unit on this tile stands; `follow_camera_line` draws it
            Transform::from_xyz(world.x, ground + SPRITE_WORLD_SIZE / 2.0 + 1.0, world.z),
            if event.delay > 0.0 { Visibility::Hidden } else { Visibility::Inherited },
            Billboard,
            Effect {
                frames,
                shown: 0,
                delay: Timer::from_seconds(event.delay, TimerMode::Once),
                timer: Timer::from_seconds(1.0 / event.kind.fps(), TimerMode::Repeating),
                material,
                anchor: Vec3::new(world.x, ground + SPRITE_WORLD_SIZE / 2.0 + 1.0, world.z),
            },
        ));
    }
}

fn animate_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut effects: Query<(Entity, &mut Effect, &mut Visibility)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut effect, mut visibility) in effects.iter_mut() {
        if !effect.delay.duration().is_zero() && !effect.delay.is_finished() {
            if effect.delay.tick(time.delta()).just_finished() {
                *visibility = Visibility::Inherited;
            }
            continue;
        }
        if !effect.timer.tick(time.delta()).just_finished() {
            continue;
        }
        effect.shown += 1;
        if effect.shown >= effect.frames {
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(material) = materials.get_mut(&effect.material) {
            material.uv_transform = frame_uv(effect.shown, effect.frames);
        }
    }
}

fn frame_uv(frame: u32, count: u32) -> bevy::math::Affine2 {
    let width = 1.0 / count as f32;
    bevy::math::Affine2::from_scale_angle_translation(Vec2::new(width, 1.0), 0.0, Vec2::new(frame as f32 * width, 0.0))
}
