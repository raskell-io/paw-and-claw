use bevy::prelude::*;

mod states;
mod game;
mod ui;

use states::GameState;
use game::GamePlugin;
use ui::UiPlugin;

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Paw & Claw".into(),
                resolution: (1280.0, 720.0).into(),
                canvas: Some("#game-canvas".into()),
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .add_plugins(GamePlugin)
        .add_plugins(UiPlugin)
        .add_systems(Startup, setup_camera)
        .run();
}

fn setup_camera(mut commands: Commands) {
    commands.spawn(Camera2d::default());
}
