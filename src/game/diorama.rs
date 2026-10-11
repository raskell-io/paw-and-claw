//! The title screen's backdrop: a live skirmish on a real map, seen from a low,
//! slowly circling camera behind the menu. Two armies start face to face in the
//! middle of the island; every moment one unit steps toward the nearest enemy
//! or strikes it, and a unit worn down poofs and rejoins from the back.
//!
//! It uses the battle map, unit and effect code, so it lives in the shared
//! battle resources (`GameMap`) while the title screen is up and is torn down
//! when the menu closes.

use bevy::prelude::*;

use super::{
    get_builtin_map, spawn_map_from_data, spawn_unit, BattleConfig, BattleEntity, EffectKind, Faction, GameData,
    GameMap, GameResult, GridPosition, MapId, PlayEffect, SpriteAssets, TilesetTheme, Unit, UnitAnimation, UnitDefeated,
    UnitType, IMPASSABLE, TILE_SIZE,
};
use crate::states::GameState;

pub struct DioramaPlugin;

impl Plugin for DioramaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Diorama>()
            .add_systems(OnEnter(GameState::Menu), |mut diorama: ResMut<Diorama>| diorama.spawned = false)
            .add_systems(OnExit(GameState::Menu), tear_down)
            .add_systems(Update, (spawn_diorama, orbit_camera, skirmish).run_if(in_state(GameState::Menu)));
    }
}

const ARMIES: [Faction; 2] = [Faction::Western, Faction::Northern];
const ROSTER: [UnitType; 6] = [UnitType::Scout, UnitType::Shocktrooper, UnitType::Recon, UnitType::Ironclad,
    UnitType::Siege, UnitType::Scout];
/// Seconds between moves in the skirmish
const BEAT: f32 = 0.7;

#[derive(Resource, Default)]
struct Diorama {
    spawned: bool,
    clock: f32,
    next_move: f32,
    turn: usize,
    /// Where each army's units start (and rejoin)
    starts: [Vec<(i32, i32)>; 2],
}

fn spawn_diorama(
    mut commands: Commands,
    mut diorama: ResMut<Diorama>,
    mut game_map: ResMut<GameMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sprites: Res<SpriteAssets>,
    mut images: ResMut<Assets<Image>>,
    game_data: Res<GameData>,
) {
    if diorama.spawned || !sprites.loading_complete {
        return;
    }
    diorama.spawned = true;
    let map_data = get_builtin_map(MapId::SpannIsland).with_factions(ARMIES[0], ARMIES[1]);
    spawn_map_from_data(&mut commands, &mut game_map, &mut meshes, &mut materials, &sprites, &mut images,
        &game_data, &map_data, TilesetTheme::default());
    // The first army faces right and the second left, like in battle
    commands.insert_resource(BattleConfig::new(ARMIES[0], ARMIES[1]));

    // Each army takes the open land nearest the island's middle on its side
    let (w, h) = (map_data.width as i32, map_data.height as i32);
    let mut open: Vec<(i32, i32)> = (0..h).flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let terrain = map_data.terrain[y as usize][x as usize];
            game_data.movement_cost(terrain, super::UnitClass::Treads) < IMPASSABLE && !game_data.terrain_capturable(terrain)
        })
        .collect();
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    open.sort_by(|a, b| {
        let d = |p: &(i32, i32)| (p.0 as f32 + 0.5 - cx).abs() + (p.1 as f32 + 0.5 - cy).abs();
        d(a).total_cmp(&d(b))
    });
    for (side, &army) in ARMIES.iter().enumerate() {
        let starts: Vec<_> = open.iter().copied()
            .filter(|&(x, _)| if side == 0 { (x as f32) < cx - 1.0 } else { (x as f32) >= cx + 1.0 })
            .take(ROSTER.len())
            .collect();
        for (&(x, y), &unit_type) in starts.iter().zip(ROSTER.iter()) {
            spawn_unit(&mut commands, &game_map, &mut meshes, &mut materials, &sprites, &images, &game_data, army, unit_type, x, y);
        }
        diorama.starts[side] = starts;
    }
}

/// A low camera circling the island slowly
fn orbit_camera(
    time: Res<Time>,
    map: Res<GameMap>,
    mut diorama: ResMut<Diorama>,
    mut camera: Query<&mut Transform, With<Camera3d>>,
) {
    if map.width == 0 {
        return;
    }
    diorama.clock += time.delta_secs();
    let Ok(mut transform) = camera.single_mut() else { return };
    let size = map.width.max(map.height) as f32 * TILE_SIZE;
    let angle = diorama.clock * 0.045 + 0.6;
    let radius = size * 0.74;
    let eye = Vec3::new(angle.sin() * radius, size * 0.3, angle.cos() * radius);
    *transform = Transform::from_translation(eye).looking_at(Vec3::new(0.0, 0.0, 0.0), Vec3::Y);
}

/// Every beat one unit acts: strikes an adjacent enemy, or steps toward the nearest one
fn skirmish(
    time: Res<Time>,
    mut diorama: ResMut<Diorama>,
    map: Res<GameMap>,
    game_data: Res<GameData>,
    mut units: Query<(Entity, &mut Unit, &super::FactionMember, &mut GridPosition, &Transform), Without<UnitAnimation>>,
    moving: Query<(), With<UnitAnimation>>,
    mut commands: Commands,
    mut effects: MessageWriter<PlayEffect>,
    mut defeats: MessageWriter<UnitDefeated>,
) {
    if !diorama.spawned || map.width == 0 {
        return;
    }
    diorama.next_move -= time.delta_secs();
    if diorama.next_move > 0.0 || !moving.is_empty() {
        return;
    }
    diorama.next_move = BEAT;
    diorama.turn += 1;
    let side = diorama.turn % 2;
    let army = ARMIES[side];

    let snapshot: Vec<(Entity, Faction, (i32, i32))> = units.iter().map(|(e, _, f, p, _)| (e, f.faction, (p.x, p.y))).collect();
    let mine: Vec<_> = snapshot.iter().filter(|(_, f, _)| *f == army).collect();
    if mine.is_empty() {
        return;
    }
    let (actor, _, from) = *mine[diorama.turn / 2 % mine.len()];
    let Some(&(target, _, at)) = snapshot.iter().filter(|(_, f, _)| *f != army)
        .min_by_key(|(_, _, p)| (p.0 - from.0).abs() + (p.1 - from.1).abs()) else { return };

    if (at.0 - from.0).abs() + (at.1 - from.1).abs() == 1 {
        // Strike: a burst on the enemy, who loses some HP; worn down, it poofs
        // and rejoins from its army's start
        effects.write(PlayEffect { kind: EffectKind::Blast, position: GridPosition::new(at.0, at.1), delay: 0.0 });
        if let Ok((_, mut unit, faction, mut pos, mut _t)) = units.get_mut(target) {
            unit.hp -= 25 + (diorama.turn as i32 * 7) % 20;
            if unit.hp <= 10 {
                defeats.write(UnitDefeated { position: *pos });
                unit.hp = game_data.unit_stats(unit.unit_type).max_hp;
                let back = if faction.faction == ARMIES[0] { 0 } else { 1 };
                let free = diorama.starts[back].iter().copied()
                    .find(|s| !snapshot.iter().any(|(_, _, p)| p == s));
                if let Some((x, y)) = free {
                    *pos = GridPosition::new(x, y);
                    let world = pos.to_world(&map);
                    commands.entity(target).insert(Transform::from_xyz(world.x, _t.translation.y, world.z));
                }
            }
        }
        return;
    }

    // Step toward the enemy onto open ground
    let class = units.get(actor).map(|(_, u, ..)| game_data.unit_class(u.unit_type)).unwrap_or(super::UnitClass::Foot);
    let mut best = None;
    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let next = (from.0 + dx, from.1 + dy);
        let Some(terrain) = map.get(next.0, next.1) else { continue };
        if game_data.movement_cost(terrain, class) >= IMPASSABLE || snapshot.iter().any(|(_, _, p)| *p == next) {
            continue;
        }
        let dist = (at.0 - next.0).abs() + (at.1 - next.1).abs();
        if best.is_none_or(|(_, d)| dist < d) {
            best = Some((next, dist));
        }
    }
    if let Some((next, _)) = best {
        if let Ok((_, _, _, mut pos, transform)) = units.get_mut(actor) {
            *pos = GridPosition::new(next.0, next.1);
            let world = pos.to_world(&map);
            let start = transform.translation;
            commands.entity(actor).insert(UnitAnimation::from_path(vec![start, Vec3::new(world.x, start.y, world.z)]));
        }
    }
}

/// Leaving the title screen clears the backdrop and its battle state
fn tear_down(mut commands: Commands, entities: Query<Entity, With<BattleEntity>>) {
    for entity in &entities {
        commands.entity(entity).despawn();
    }
    commands.insert_resource(GameMap::default());
    commands.insert_resource(GameResult::default());
}
