//! Short battle effects drawn over the board, like the dust poof of a defeated unit

use bevy::prelude::*;

use super::{Billboard, GameData, GameMap, GridPosition, SPRITE_WORLD_SIZE, TILE_SIZE};

pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<UnitDefeated>()
            .add_systems(Startup, load_effect_sprites)
            .add_systems(Update, (spawn_defeat_effects, animate_effects));
    }
}

/// A unit's HP reached 0 on this tile
#[derive(Message)]
pub struct UnitDefeated {
    pub position: GridPosition,
}

#[derive(Resource)]
struct EffectSprites {
    /// `effects/poof.png`: a strip of square frames (tools/sprites/effects.py)
    poof: Handle<Image>,
}

fn load_effect_sprites(mut commands: Commands, asset_server: Res<AssetServer>) {
    commands.insert_resource(EffectSprites { poof: asset_server.load("sprites/effects/poof.png") });
}

/// A one-shot sprite animation that removes itself when done
#[derive(Component)]
#[require(super::BattleEntity)]
struct Effect {
    frames: u32,
    shown: u32,
    timer: Timer,
    material: Handle<StandardMaterial>,
}

const POOF_FRAMES: u32 = 6;
const POOF_FPS: f32 = 12.0;

fn spawn_defeat_effects(
    mut events: MessageReader<UnitDefeated>,
    mut commands: Commands,
    sprites: Option<Res<EffectSprites>>,
    map: Res<GameMap>,
    game_data: Res<GameData>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(sprites) = sprites else { return };
    for event in events.read() {
        let world = event.position.to_world(&map);
        let ground = map.get(event.position.x, event.position.y).map_or(4.0, |t| game_data.terrain(t).tile_height);
        let material = materials.add(StandardMaterial {
            base_color_texture: Some(sprites.poof.clone()),
            unlit: true,
            alpha_mode: AlphaMode::Mask(0.5),
            uv_transform: frame_uv(0, POOF_FRAMES),
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            Mesh3d(meshes.add(Rectangle::new(SPRITE_WORLD_SIZE, SPRITE_WORLD_SIZE))),
            MeshMaterial3d(material.clone()),
            // Where the unit stood, in front of the terrain like units are
            Transform::from_xyz(world.x, ground + SPRITE_WORLD_SIZE / 2.0 + 1.0, world.z + TILE_SIZE * 0.5),
            Billboard,
            Effect {
                frames: POOF_FRAMES,
                shown: 0,
                timer: Timer::from_seconds(1.0 / POOF_FPS, TimerMode::Repeating),
                material,
            },
        ));
    }
}

fn animate_effects(
    time: Res<Time>,
    mut commands: Commands,
    mut effects: Query<(Entity, &mut Effect)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, mut effect) in effects.iter_mut() {
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
