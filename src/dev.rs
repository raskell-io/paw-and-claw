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
//! - `key NAME`   - press and release a key (Bevy `KeyCode` name, e.g. `Space`, `ArrowUp`, `KeyW`)
//! - `click X Y`  - left-click at logical window coordinates (reaches egui and cursor-based picking)
//! - `move X Y`   - move the cursor without clicking (hover)
//! - `shot NAME`  - save `NAME.png` into `PAW_SHOT_DIR` (default `screenshots/`)
//! - `gameover win|lose` - end the current battle immediately
//! - `quit`       - exit the app
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

pub struct DevScriptPlugin;

impl Plugin for DevScriptPlugin {
    fn build(&self, app: &mut App) {
        let Ok(script) = std::env::var("PAW_SCRIPT") else { return };
        let steps = parse(&script);
        let out_dir = PathBuf::from(std::env::var("PAW_SHOT_DIR").unwrap_or_else(|_| "screenshots".into()));
        std::fs::create_dir_all(&out_dir).expect("create screenshot dir");
        app.insert_resource(DevScript { steps, wait: 0, release: None, out_dir })
            .add_systems(First, run_script);
    }
}

#[derive(Debug)]
enum Step {
    Wait(u32),
    Key(KeyCode),
    Click(f32, f32),
    Move(f32, f32),
    Shot(String),
    GameOver(bool),
    Quit,
}

#[derive(Resource)]
struct DevScript {
    steps: VecDeque<Step>,
    wait: u32,
    /// Key or mouse button to release on the next frame
    release: Option<Release>,
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
                "key" => Step::Key(key_code(parts[1])),
                "click" => Step::Click(num(1), num(2)),
                "move" => Step::Move(num(1), num(2)),
                "shot" => Step::Shot(parts[1].to_string()),
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
        "ArrowUp" => KeyCode::ArrowUp,
        "ArrowDown" => KeyCode::ArrowDown,
        "ArrowLeft" => KeyCode::ArrowLeft,
        "ArrowRight" => KeyCode::ArrowRight,
        "KeyW" => KeyCode::KeyW,
        "KeyA" => KeyCode::KeyA,
        "KeyS" => KeyCode::KeyS,
        "KeyD" => KeyCode::KeyD,
        "KeyE" => KeyCode::KeyE,
        "KeyF" => KeyCode::KeyF,
        "KeyP" => KeyCode::KeyP,
        "KeyQ" => KeyCode::KeyQ,
        "KeyR" => KeyCode::KeyR,
        other => panic!("unsupported key '{other}' in dev script"),
    }
}

fn run_script(
    mut script: ResMut<DevScript>,
    mut commands: Commands,
    window: Query<Entity, With<PrimaryWindow>>,
    mut keys: MessageWriter<KeyboardInput>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut cursor: MessageWriter<CursorMoved>,
    mut exit: MessageWriter<AppExit>,
    mut game_result: ResMut<crate::game::GameResult>,
    battle_config: Res<crate::game::BattleConfig>,
) {
    let Ok(window) = window.single() else { return };

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

    let Some(step) = script.steps.pop_front() else { return };
    match step {
        Step::Wait(frames) => script.wait = frames,
        Step::Key(key_code) => {
            keys.write(key_message(key_code, ButtonState::Pressed, window));
            script.release = Some(Release::Key(key_code));
        }
        Step::Move(x, y) => {
            cursor.write(CursorMoved { window, position: Vec2::new(x, y), delta: None });
        }
        Step::Click(x, y) => {
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
        Step::GameOver(win) => {
            let player = battle_config.player_faction;
            game_result.game_over = true;
            game_result.winner = Some(if win { player } else { battle_config.next_faction(player) });
            game_result.victory_type = crate::game::VictoryType::Elimination;
        }
        Step::Quit => {
            exit.write(AppExit::Success);
        }
    }
}

fn key_message(key_code: KeyCode, state: ButtonState, window: Entity) -> KeyboardInput {
    KeyboardInput { key_code, logical_key: Key::Unidentified(bevy::input::keyboard::NativeKey::Unidentified), state, text: None, repeat: false, window }
}
