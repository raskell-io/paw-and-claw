//! Dev-only scripted driver for capturing screenshots of the real game.
//!
//! Set `PAW_SCRIPT` to a `;`-separated list of steps and the game replays them
//! as real input, saving screenshots and exiting when done:
//!
//! ```text
//! PAW_SCRIPT="wait 60; shot menu; key Enter; wait 20; shot setup; click 640 400; quit" cargo run
//! ```
//!
//! Steps:
//! - `wait N`     - let N frames pass
//! - `until menu|setup|board` - wait for the title screen, the skirmish setup or
//!   a loaded battle board (then a few frames more, past first-frame input guards).
//!   Prefer this over long `wait`s at startup: frame timing varies with load time
//! - `key NAME`   - press and release a key (`Space`, `Enter`, `Escape`, `Tab`, arrows, `KeyA`..`KeyZ`)
//! - `hold NAME N` - hold a key down for N frames (zoom, key repeat)
//! - `click X Y`  - left-click at logical window coordinates (reaches egui and cursor-based picking)
//! - `move X Y`   - move the cursor without clicking (hover)
//! - `touch down|move|up X Y` - a finger on the screen (tap = down then up)
//! - `boot STAGE SECS` - jump the opening sequence (disclaimer|studio|intro|title|menu)
//! - `shot NAME`  - save `NAME.png` into `PAW_SHOT_DIR` (default `screenshots/`)
//! - `unit X Y FIELD VALUE` - set the unit on tile (X, Y)'s `hp` (0-100), `ammo` or `stamina`
//! - `charge`     - fill the player's CO power meter
//! - `gameover win|lose` - end the current battle immediately
//! - `quit`       - exit the app
//!
//! `PAW_BARE=1` hides the title screen's UI so `shot` captures the bare diorama.
//!
//! `PAW_EXPORT_MAPS=DIR` writes every built-in map to `DIR/<id>.json` (`MapData` as
//! JSON, as the game sets it up) and exits without opening a window. The website
//! (pawandclaw.raskell.io) builds its map pages from these.
//!
//! Native builds only; inert unless `PAW_SCRIPT` is set.

use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};
use bevy::window::{CursorMoved, PrimaryWindow};
use std::collections::VecDeque;
use std::path::PathBuf;

/// `PAW_EXPORT_MAPS=DIR`: write each built-in map as JSON, then exit
pub fn export_maps_if_asked() {
    let Some(dir) = std::env::var_os("PAW_EXPORT_MAPS") else { return };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("create map export dir");
    for id in crate::game::MapId::all_builtin() {
        let path = dir.join(format!("{id:?}.json"));
        let json = serde_json::to_string_pretty(&crate::game::get_builtin_map(id)).expect("serialize map");
        std::fs::write(&path, json).expect("write map");
        println!("{}", path.display());
    }
    std::process::exit(0);
}

pub struct DevScriptPlugin;

impl Plugin for DevScriptPlugin {
    fn build(&self, app: &mut App) {
        let Ok(script) = std::env::var("PAW_SCRIPT") else { return };
        let steps = parse(&script);
        let out_dir = PathBuf::from(std::env::var("PAW_SHOT_DIR").unwrap_or_else(|_| "screenshots".into()));
        std::fs::create_dir_all(&out_dir).expect("create screenshot dir");
        // Scripts start at the main menu; PAW_BOOT=1 plays the whole opening
        // sequence, even when it has been seen before; PAW_BOOT=launch starts
        // the way a real launch does
        match std::env::var("PAW_BOOT").as_deref() {
            Err(_) => { app.insert_resource(crate::states::BootSequence { stage: crate::states::BootStage::Menu, ..default() }); }
            Ok("launch") => {}
            Ok(_) => { app.insert_resource(crate::states::BootSequence::default()); }
        }
        app.insert_resource(DevScript { steps, wait: 0, release: None, held: None, out_dir })
            .add_systems(First, run_script);
    }
}

#[derive(Debug)]
enum Step {
    Wait(u32),
    Until(String),
    Key(KeyCode),
    Hold(KeyCode, u32),
    Click(f32, f32),
    Move(f32, f32),
    Shot(String),
    SetUnit(i32, i32, String, i32),
    Charge,
    GameOver(bool),
    /// Give the player's army this commander (by data key, e.g. `dusty`)
    Commander(String),
    /// Jump the opening sequence to a stage and time: `boot intro 19.5`
    Boot(crate::states::BootStage, f32),
    /// A finger: `touch down|move|up X Y`
    Touch(bevy::input::touch::TouchPhase, f32, f32),
    Quit,
}

#[derive(Resource)]
struct DevScript {
    steps: VecDeque<Step>,
    wait: u32,
    /// Key or mouse button to release on the next frame
    release: Option<Release>,
    /// Key held down by `hold`, released once its wait runs out
    held: Option<KeyCode>,
    out_dir: PathBuf,
}

enum Release {
    Key(KeyCode),
    Mouse,
}

fn parse(script: &str) -> VecDeque<Step> {
    script
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|step| {
            let parts: Vec<&str> = step.split_whitespace().collect();
            let num = |i: usize| -> f32 {
                parts.get(i).and_then(|p| p.parse().ok()).unwrap_or_else(|| panic!("bad step: {step}"))
            };
            match parts[0] {
                "wait" => Step::Wait(num(1) as u32),
                "until" => Step::Until(parts[1].to_string()),
                "key" => Step::Key(key_code(parts[1])),
                "hold" => Step::Hold(key_code(parts[1]), num(2) as u32),
                "click" => Step::Click(num(1), num(2)),
                "move" => Step::Move(num(1), num(2)),
                "boot" => {
                    use crate::states::BootStage;
                    let stage = match parts.get(1).copied() {
                        Some("disclaimer") => BootStage::Disclaimer,
                        Some("studio") => BootStage::Studio,
                        Some("intro") => BootStage::Intro,
                        Some("title") => BootStage::Title,
                        _ => BootStage::Menu,
                    };
                    Step::Boot(stage, parts.get(2).and_then(|v| v.parse().ok()).unwrap_or(0.0))
                }
                "touch" => {
                    use bevy::input::touch::TouchPhase;
                    let phase = match parts.get(1).copied() {
                        Some("down") => TouchPhase::Started,
                        Some("move") => TouchPhase::Moved,
                        Some("up") => TouchPhase::Ended,
                        _ => panic!("bad step: {step} (touch down|move|up X Y)"),
                    };
                    Step::Touch(phase, num(2), num(3))
                }
                "shot" => Step::Shot(parts[1].to_string()),
                "unit" => Step::SetUnit(num(1) as i32, num(2) as i32, parts[3].to_string(), num(4) as i32),
                "charge" => Step::Charge,
                "co" => Step::Commander(parts[1].to_string()),
                "gameover" => Step::GameOver(parts.get(1) == Some(&"win")),
                "quit" => Step::Quit,
                other => panic!("unknown dev script step '{other}'"),
            }
        })
        .collect()
}

fn key_code(name: &str) -> KeyCode {
    match name {
        "Space" => KeyCode::Space,
        "Enter" => KeyCode::Enter,
        "Escape" => KeyCode::Escape,
        "Tab" => KeyCode::Tab,
        "F5" => KeyCode::F5,
        "F9" => KeyCode::F9,
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        letter if letter.len() == 4 && letter.starts_with("Key") => letter_key(letter.as_bytes()[3]),
        other => panic!("unsupported key '{other}' in dev script"),
    }
}

fn letter_key(letter: u8) -> KeyCode {
    const KEYS: [KeyCode; 26] = [
        KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE, KeyCode::KeyF, KeyCode::KeyG,
        KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ, KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN,
        KeyCode::KeyO, KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT, KeyCode::KeyU,
        KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY, KeyCode::KeyZ,
    ];
    match letter {
        b'A'..=b'Z' => KEYS[(letter - b'A') as usize],
        _ => panic!("unsupported key 'Key{}' in dev script", letter as char),
    }
}

fn run_script(
    mut script: ResMut<DevScript>,
    mut commands: Commands,
    mut window: Query<(Entity, &mut Window), With<PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut cursor: MessageWriter<CursorMoved>,
    mut touches: MessageWriter<bevy::input::touch::TouchInput>,
    mut game_result: ResMut<crate::game::GameResult>,
    battle_config: Res<crate::game::BattleConfig>,
    mut units: Query<(&crate::game::GridPosition, &mut crate::game::Unit)>,
    state: Res<State<crate::states::GameState>>,
    setup: Res<crate::ui::BattleSetupState>,
    banner: Res<crate::game::TurnBanner>,
    map: Res<crate::game::GameMap>,
    sprites: Res<crate::game::SpriteAssets>,
    mut commanders: ResMut<crate::game::Commanders>,
) {
    let Ok((window, mut window_state)) = window.single_mut() else { return };

    if let Some(release) = script.release.take() {
        match release {
            Release::Key(key_code) => {
                keys.write(key_message(key_code, ButtonState::Released, window));
            }
            Release::Mouse => {
                buttons.write(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Released, window });
            }
        }
        return;
    }

    if script.wait > 0 {
        script.wait -= 1;
        return;
    }

    if let Some(key_code) = script.held.take() {
        keys.write(key_message(key_code, ButtonState::Released, window));
        return;
    }

    // `until` holds the script until the screen it names is up
    if let Some(Step::Until(screen)) = script.steps.front() {
        use crate::states::GameState;
        let ready = match screen.as_str() {
            "menu" => *state.get() == GameState::Menu && sprites.loading_complete,
            "setup" => *state.get() == GameState::Battle && setup.needs_setup,
            "board" => *state.get() == GameState::Battle && !setup.needs_setup && map.width > 0,
            // The board, once the match intro or "Day N" banner has played and input is live
            "turn" => *state.get() == GameState::Battle && !setup.needs_setup && map.width > 0 && banner.faction.is_some() && !banner.playing(),
            other => panic!("unknown screen '{other}' for `until` (menu, setup, board, turn)"),
        };
        if ready {
            script.steps.pop_front();
            script.wait = 10;
        }
        return;
    }

    let Some(step) = script.steps.pop_front() else { return };
    match step {
        Step::Wait(frames) => script.wait = frames,
        Step::Until(_) => unreachable!("handled above"),
        Step::Key(key_code) => {
            keys.write(key_message(key_code, ButtonState::Pressed, window));
            script.release = Some(Release::Key(key_code));
        }
        Step::Hold(key_code, frames) => {
            keys.write(key_message(key_code, ButtonState::Pressed, window));
            script.held = Some(key_code);
            script.wait = frames;
        }
        // Board picking reads the window's cursor position, so set that too
        Step::Boot(stage, secs) => {
            commands.insert_resource(crate::states::BootSequence { stage, elapsed: secs, skipped_intro: false });
        }
        Step::Touch(phase, x, y) => {
            touches.write(bevy::input::touch::TouchInput { phase, position: Vec2::new(x, y), window, force: None, id: 1 });
        }
        Step::Move(x, y) => {
            window_state.set_cursor_position(Some(Vec2::new(x, y)));
            cursor.write(CursorMoved { window, position: Vec2::new(x, y), delta: None });
        }
        Step::Click(x, y) => {
            window_state.set_cursor_position(Some(Vec2::new(x, y)));
            cursor.write(CursorMoved { window, position: Vec2::new(x, y), delta: None });
            buttons.write(MouseButtonInput { button: MouseButton::Left, state: ButtonState::Pressed, window });
            script.release = Some(Release::Mouse);
        }
        Step::Shot(name) => {
            let path = script.out_dir.join(format!("{name}.png"));
            info!("Dev script: screenshot -> {}", path.display());
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
            // Give the capture time to be written before the next step
            script.wait = 5;
        }
        Step::Charge => {
            commanders.power_meter.insert(battle_config.player_faction, 1_000_000);
        }
        Step::Commander(key) => {
            let id = *crate::game::CommanderId::all().iter()
                .find(|&&id| crate::game::commander_to_key(id) == key)
                .unwrap_or_else(|| panic!("unknown commander '{key}' in dev script"));
            commanders.set_commander(battle_config.player_faction, id);
        }
        Step::SetUnit(x, y, field, value) => {
            for (pos, mut unit) in units.iter_mut().filter(|(p, _)| p.x == x && p.y == y) {
                match field.as_str() {
                    "hp" => unit.hp = value,
                    "ammo" => unit.ammo = value.max(0) as u32,
                    "stamina" => unit.stamina = value.max(0) as u32,
                    other => panic!("unknown unit field '{other}' in dev script"),
                }
                info!("Dev script: unit at ({}, {}) {} = {}", pos.x, pos.y, field, value);
            }
        }
        Step::GameOver(win) => {
            let player = battle_config.player_faction;
            game_result.game_over = true;
            game_result.winner = Some(if win { player } else { battle_config.next_faction(player) });
            game_result.victory_type = crate::game::VictoryType::Elimination;
        }
        Step::Quit => {
            commands.write_message(AppExit::Success);
        }
    }
}

fn key_message(key_code: KeyCode, state: ButtonState, window: Entity) -> KeyboardInput {
    KeyboardInput { key_code, logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified), state, text: None, repeat: false, window }
}
