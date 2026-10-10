use bevy::prelude::*;

mod states;
mod game;
mod ui;
#[cfg(not(target_arch = "wasm32"))]
mod dev;

use states::GameState;
use game::GamePlugin;
use ui::UiPlugin;

fn main() {
    #[cfg(target_arch = "wasm32")]
    console_error_panic_hook::set_once();

    let mut app = App::new();
    app
        // Deep forest floor around the board
        .insert_resource(ClearColor(Color::srgb_u8(0x1F, 0x2B, 0x21)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Paw & Claw".into(),
                resolution: (1280, 720).into(),
                canvas: Some("#game-canvas".into()),
                prevent_default_event_handling: false,
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .add_plugins(GamePlugin)
        .add_plugins(UiPlugin)
        .add_systems(Startup, (setup_camera, setup_lighting));

    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(dev::DevScriptPlugin);

    app.run();
}

fn setup_camera(mut commands: Commands) {
    // 3D camera looking down at the game board at ~35 degree angle
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 400.0, 350.0)
            .looking_at(Vec3::ZERO, Vec3::Y),
        Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::PI / 6.0, // 30 degree FOV for tighter view
            ..default()
        }),
    ));
}

fn setup_lighting(mut commands: Commands) {
    // Ambient light for base illumination
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 0.6,
        affects_lightmapped_meshes: true,
    });

    // Directional light (sun-like)
    commands.spawn((
        DirectionalLight {
            illuminance: 8000.0,
            ..default()
        },
        Transform::from_rotation(Quat::from_euler(
            EulerRot::XYZ,
            -0.8, // Angle down
            0.4,  // Slight side angle
            0.0,
        )),
    ));
}
