//! Sound effects and music. Sounds are named in `assets/data/audio.ron`;
//! anything can ask for one with the `PlaySfx` message, and this plugin also
//! listens to the game's own events (moves, attacks, captures, powers, turns)
//! so game logic doesn't have to know about audio.

use std::collections::HashMap;

use bevy::audio::Volume;
use bevy::prelude::*;

use super::{
    AttackEvent, EffectKind, EndTurnRequest, GameData, GridCursor, JoinEvent, LoadEvent, MovementHighlights,
    PlayEffect, PowerActivatedEvent, TurnBanner, Unit, UnitAnimation, UnitClass, UnitDefeated, UnloadEvent,
    MATCH_INTRO_SECS,
};

pub struct GameAudioPlugin;

impl Plugin for GameAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudioSettings>()
            .init_resource::<DelayedSounds>()
            .add_message::<PlaySfx>()
            .add_systems(Startup, load_sounds)
            .add_systems(Update, (
                (board_sounds, unit_sounds, combat_sounds, turn_sounds, delayed_sounds),
                play_sounds,
            ).chain())
            .add_systems(Update, (choose_music, play_music).chain())
            .add_systems(Update, save_audio_settings);
    }
}

/// The player's volume settings, 0.0 to 1.0, kept between sessions
#[derive(Resource, Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct AudioSettings {
    pub master: f32,
    pub sfx: f32,
    pub music: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        super::read_setting("audio")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or(Self { master: 0.8, sfx: 0.8, music: 0.7 })
    }
}

/// Write the volume settings whenever the player changes them
fn save_audio_settings(settings: Res<AudioSettings>) {
    if settings.is_changed() && !settings.is_added() {
        if let Ok(json) = serde_json::to_string(&*settings) {
            if let Err(e) = super::write_setting("audio", &json) {
                warn!("Couldn't save audio settings: {e}");
            }
        }
    }
}

/// Play a sound effect by its id in audio.ron
#[derive(Message, Clone, Copy, Debug)]
pub struct PlaySfx(pub &'static str);

struct LoadedSound {
    handle: Handle<AudioSource>,
    volume: f32,
    pitch_variance: f32,
}

#[derive(Resource, Default)]
struct SoundLibrary {
    sfx: HashMap<String, LoadedSound>,
    /// Music is fetched the first time it's needed (on the web the game would
    /// otherwise download every theme before it starts), then kept
    music: HashMap<String, super::SoundData>,
    music_handles: HashMap<String, Handle<AudioSource>>,
}

impl SoundLibrary {
    /// Playback settings for a sound at the player's volume, with its pitch varied
    fn settings(&self, sound: &LoadedSound, audio: &AudioSettings, base: PlaybackSettings) -> Option<PlaybackSettings> {
        let volume = sound.volume * audio.master * audio.sfx;
        if volume <= 0.0 {
            return None;
        }
        let speed = 1.0 + (rand::random::<f32>() * 2.0 - 1.0) * sound.pitch_variance;
        Some(base.with_volume(Volume::Linear(volume)).with_speed(speed))
    }
}

fn load_sounds(mut commands: Commands, asset_server: Res<AssetServer>, game_data: Res<GameData>) {
    let load = |sounds: &HashMap<String, super::SoundData>| sounds.iter()
        .map(|(id, sound)| (id.clone(), LoadedSound {
            handle: asset_server.load(sound.file.clone()),
            volume: sound.volume,
            pitch_variance: sound.pitch_variance,
        }))
        .collect();
    commands.insert_resource(SoundLibrary {
        sfx: load(&game_data.audio.sfx),
        music: game_data.audio.music.clone(),
        music_handles: HashMap::new(),
    });
    commands.insert_resource(MusicCue::default());
}

/// Play this frame's requested sounds, each at most once
fn play_sounds(
    mut requests: MessageReader<PlaySfx>,
    mut commands: Commands,
    library: Option<Res<SoundLibrary>>,
    settings: Res<AudioSettings>,
) {
    let Some(library) = library else {
        requests.clear();
        return;
    };
    let mut played: Vec<&str> = Vec::new();
    for PlaySfx(id) in requests.read() {
        if played.contains(id) {
            continue;
        }
        played.push(id);
        let Some(sound) = library.sfx.get(*id) else {
            warn_once!("No sound effect '{id}' in audio.ron");
            continue;
        };
        if let Some(playback) = library.settings(sound, &settings, PlaybackSettings::DESPAWN) {
            commands.spawn((AudioPlayer(sound.handle.clone()), playback));
        }
    }
}

/// The cursor ticking from tile to tile, and picking a unit up
fn board_sounds(
    cursor: Res<GridCursor>,
    highlights: Res<MovementHighlights>,
    banner: Res<TurnBanner>,
    mut last_cursor: Local<Option<(i32, i32)>>,
    mut last_selected: Local<Option<Entity>>,
    mut play: MessageWriter<PlaySfx>,
) {
    let at = (cursor.x, cursor.y);
    // The turn banner moves the cursor to the HQ: that's not a step
    if last_cursor.is_some_and(|last| last != at) && !banner.playing() {
        play.write(PlaySfx("cursor"));
    }
    *last_cursor = Some(at);
    if highlights.selected_unit.is_some() && highlights.selected_unit != *last_selected {
        play.write(PlaySfx("select"));
    }
    *last_selected = highlights.selected_unit;
}

/// A looping movement sound, playing while its unit travels (as in Advance Wars)
#[derive(Component)]
#[require(super::BattleEntity)]
struct MoveLoop(Entity);

/// Start a unit's movement loop when it sets off and stop it on arrival; also
/// units joining, boarding or leaving transports
fn unit_sounds(
    mut commands: Commands,
    moving: Query<(Entity, &Unit), Added<UnitAnimation>>,
    still_moving: Query<(), With<UnitAnimation>>,
    loops: Query<(Entity, &MoveLoop)>,
    mut joins: MessageReader<JoinEvent>,
    mut loads: MessageReader<LoadEvent>,
    mut unloads: MessageReader<UnloadEvent>,
    game_data: Res<GameData>,
    library: Option<Res<SoundLibrary>>,
    settings: Res<AudioSettings>,
    mut play: MessageWriter<PlaySfx>,
) {
    for (sound, MoveLoop(unit)) in &loops {
        if still_moving.get(*unit).is_err() {
            commands.entity(sound).despawn();
        }
    }
    if let Some(library) = library {
        for (entity, unit) in &moving {
            let id = match game_data.unit_class(unit.unit_type) {
                UnitClass::Foot | UnitClass::Mech => "move_foot",
                UnitClass::Air | UnitClass::AirTransport => "move_air",
                UnitClass::Naval | UnitClass::NavalTransport => "move_boat",
                _ => "move_wheels",
            };
            let Some(sound) = library.sfx.get(id) else { continue };
            if let Some(playback) = library.settings(sound, &settings, PlaybackSettings::LOOP) {
                commands.spawn((AudioPlayer(sound.handle.clone()), playback, MoveLoop(entity)));
            }
        }
    }
    if joins.read().count() > 0 {
        play.write(PlaySfx("join"));
    }
    if loads.read().count() > 0 {
        play.write(PlaySfx("load"));
    }
    if unloads.read().count() > 0 {
        play.write(PlaySfx("unload"));
    }
}

/// Strikes (siege engines boom, other ranged units shoot, the rest hit),
/// units falling, and power effects in step with their animations
fn combat_sounds(
    mut attacks: MessageReader<AttackEvent>,
    mut reports: MessageReader<super::BattleReport>,
    display: Res<super::DisplaySettings>,
    mut defeated: MessageReader<UnitDefeated>,
    mut effects: MessageReader<PlayEffect>,
    mut powers: MessageReader<PowerActivatedEvent>,
    units: Query<&Unit>,
    game_data: Res<GameData>,
    mut queue: ResMut<DelayedSounds>,
    mut play: MessageWriter<PlaySfx>,
) {
    // With battle scenes on, the scene plays the strikes and losses in step
    // with its animation instead
    let scenes = display.battle != super::BattleAnimation::Off;
    let in_battle = reports.read().count() > 0;
    for attack in attacks.read() {
        let Ok(unit) = units.get(attack.attacker) else { continue };
        if !scenes {
            play.write(PlaySfx(super::strike_sound(&game_data, unit.unit_type)));
        }
    }
    for _ in defeated.read() {
        if !(scenes && in_battle) {
            queue.0.push((0.15, "defeat"));
        }
    }
    for _ in powers.read() {
        play.write(PlaySfx("power"));
    }
    for effect in effects.read() {
        let id = match effect.kind {
            EffectKind::Heal => "heal",
            EffectKind::Blast => "hit",
            EffectKind::Rockfall => "boom",
            EffectKind::Poof => continue,
        };
        queue.0.push((effect.delay, id));
    }
}

/// Sounds waiting for their moment, as (seconds left, id)
#[derive(Resource, Default)]
pub struct DelayedSounds(pub Vec<(f32, &'static str)>);

fn delayed_sounds(time: Res<Time>, mut queue: ResMut<DelayedSounds>, mut play: MessageWriter<PlaySfx>) {
    let dt = time.delta_secs();
    queue.0.retain_mut(|(left, id)| {
        *left -= dt;
        if *left <= 0.0 {
            play.write(PlaySfx(id));
            false
        } else {
            true
        }
    });
}

/// The "Day N" sting as its banner sweeps in, and a turn ending
fn turn_sounds(
    banner: Res<TurnBanner>,
    mut ends: MessageReader<EndTurnRequest>,
    mut last_elapsed: Local<f32>,
    mut play: MessageWriter<PlaySfx>,
) {
    if ends.read().count() > 0 {
        play.write(PlaySfx("turn_end"));
    }
    if banner.faction.is_some() {
        // The day banner starts after the match intro on a battle's first turn
        let start = if banner.intro { MATCH_INTRO_SECS } else { 0.0 };
        let now = banner.elapsed;
        if banner.playing() && (*last_elapsed > now || *last_elapsed <= start) && now > start {
            play.write(PlaySfx("day"));
        }
        *last_elapsed = now;
    }
}

// --- Music --------------------------------------------------------------------

/// The theme that should be playing (an id in audio.ron's `music`), if any
#[derive(Resource, Default, Clone, PartialEq, Debug)]
pub struct MusicCue(pub Option<String>);

/// How long a theme takes to fade in or out, in seconds
const MUSIC_FADE_SECS: f32 = 0.8;

/// A playing theme. It fades in to full, or out to silence and is removed
/// once it's no longer the cue.
#[derive(Component)]
struct MusicTrack {
    id: String,
    /// 0..1, multiplied with the theme's and the player's volume
    level: f32,
}

/// Pick the music for what's on screen: the title theme on the title and
/// setup screens; in battle the fanfare during the match intro, then the
/// theme of the commander whose turn it is (`co_<commander key>` in
/// audio.ron), as in Advance Wars
fn choose_music(
    state: Res<State<crate::states::GameState>>,
    map: Res<super::GameMap>,
    banner: Res<TurnBanner>,
    turn_state: Res<super::TurnState>,
    commanders: Res<super::Commanders>,
    game_result: Res<super::GameResult>,
    game_data: Res<GameData>,
    boot: Res<crate::states::BootSequence>,
    battle_config: Res<super::BattleConfig>,
    mut cue: ResMut<MusicCue>,
) {
    use crate::states::GameState;
    use crate::states::BootStage;
    let wanted = match state.get() {
        GameState::Menu => match boot.stage {
            BootStage::Disclaimer | BootStage::Studio => None,
            BootStage::Intro => Some("intro".to_string()),
            // Let the intro song's last hit ring before the title theme
            BootStage::Title if boot.elapsed < 2.5 && !boot.intro_skipped() => Some("intro".to_string()),
            _ => Some("title".to_string()),
        },
        // No map yet means the skirmish setup screen
        GameState::Battle if map.width == 0 => Some("title".to_string()),
        // Victory when a person is on the winning side (or only the computer played)
        GameState::Battle if game_result.game_over => game_result.winner.map(|winner| {
            let anyone = battle_config.armies.iter().any(|a| !a.ai);
            let won = !anyone || battle_config.armies.iter()
                .any(|a| !a.ai && battle_config.allied(a.faction, winner));
            if won { "victory" } else { "defeat" }.to_string()
        }),
        GameState::Battle if banner.intro && banner.elapsed < MATCH_INTRO_SECS => Some("battle_start".to_string()),
        GameState::Battle => {
            let faction = turn_state.current_faction;
            let has = |theme: &String| game_data.audio.music.contains_key(theme);
            // A CO Power is running: its army's power theme
            let power = format!("power_{}", game_data.faction(faction).asset_folder);
            let powered = commanders.power_active.get(&faction).is_some_and(|&level| level != super::PowerLevel::None);
            if powered && has(&power) {
                Some(power)
            } else {
                // The commander's own theme, else another theme from the same army
                let own = format!("co_{}", super::commander_to_key(commanders.get_active(faction)));
                let army = super::CommanderId::for_faction(faction, &game_data).into_iter()
                    .map(|co| format!("co_{}", super::commander_to_key(co)))
                    .find(|theme| has(theme));
                if has(&own) { Some(own) } else { army }
            }
        }
        _ => None,
    };
    if cue.0 != wanted {
        cue.0 = wanted;
    }
}

/// Start the cued theme, fade the old one out, and keep every playing theme
/// at the player's music volume
fn play_music(
    mut commands: Commands,
    cue: Res<MusicCue>,
    library: Option<ResMut<SoundLibrary>>,
    asset_server: Res<AssetServer>,
    settings: Res<AudioSettings>,
    time: Res<Time>,
    mut tracks: Query<(Entity, &mut MusicTrack, Option<&mut AudioSink>)>,
) {
    let Some(mut library) = library else { return };
    let step = time.delta_secs() / MUSIC_FADE_SECS;
    let mut cued_playing = false;
    for (entity, mut track, sink) in &mut tracks {
        let current = cue.0.as_deref() == Some(track.id.as_str());
        cued_playing |= current;
        track.level = (track.level + if current { step } else { -step }).clamp(0.0, 1.0);
        if !current && track.level <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        if let (Some(mut sink), Some(sound)) = (sink, library.music.get(&track.id)) {
            sink.set_volume(Volume::Linear(sound.volume * settings.master * settings.music * track.level));
        }
    }
    if let Some(id) = &cue.0 {
        if !cued_playing {
            let Some(sound) = library.music.get(id).cloned() else {
                warn_once!("No music '{id}' in audio.ron");
                return;
            };
            let handle = library.music_handles.entry(id.clone())
                .or_insert_with(|| asset_server.load(sound.file.clone()))
                .clone();
            // Themes fade in; a jingle starts at full volume and plays once
            let (playback, level) = if sound.looped { (PlaybackSettings::LOOP, 0.0) } else { (PlaybackSettings::ONCE, 1.0) };
            let volume = sound.volume * settings.master * settings.music * level;
            commands.spawn((
                AudioPlayer(handle),
                playback.with_volume(Volume::Linear(volume)),
                MusicTrack { id: id.clone(), level },
            ));
        }
    }
}
