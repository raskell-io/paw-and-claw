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
    #[cfg(not(target_arch = "wasm32"))]
    dev::export_maps_if_asked();
    #[cfg(not(target_arch = "wasm32"))]
    find_assets();

    let mut app = App::new();
    app
        // Deep forest floor around the board
        .insert_resource(ClearColor(Color::srgb_u8(0x1F, 0x2B, 0x21)))
        // Pixel-art sprites must stay crisp when scaled up
        .add_plugins(DefaultPlugins.set(ImagePlugin::default_nearest()).set(AssetPlugin {
            // Assets have no .meta files; on the web, asking for them would cost a
            // failed request per file
            meta_check: bevy::asset::AssetMetaCheck::Never,
            ..default()
        }).set(WindowPlugin {
            primary_window: Some(Window {
                title: "Paw & Claw".into(),
                resolution: window_size().into(),
                canvas: Some("#game-canvas".into()),
                // On the web, fill the page and keep the browser from scrolling,
                // zooming or acting on the game's keys and touches
                fit_canvas_to_parent: true,
                prevent_default_event_handling: true,
                ..default()
            }),
            ..default()
        }))
        .init_state::<GameState>()
        .insert_resource(states::BootSequence::at_launch())
        .add_plugins(GamePlugin)
        .add_plugins(UiPlugin)
        .add_systems(Startup, (setup_camera, setup_lighting));

    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(dev::DevScriptPlugin);

    app.run();
}

/// The window's size: 1280x720, or PAW_WINDOW=WIDTHxHEIGHT on desktop to try
/// other screens (844x390 is an iPhone in landscape)
fn window_size() -> (u32, u32) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some((w, h)) = std::env::var("PAW_WINDOW").ok().and_then(|v| {
        let (w, h) = v.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    }) {
        return (w, h);
    }
    (1280, 720)
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
            illuminance: 12000.0, // only lit pieces (the route arrow) use it; terrain and sprites are unlit
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

/// Point Bevy at the `assets` folder wherever the game is started from. Bevy
/// looks next to the executable unless run through `cargo run`, so starting
/// `target/debug/paw-and-claw` directly (or from another folder) found no art,
/// sound or fonts. Try the executable's folder, the current folder, then the
/// project the game was built from.
#[cfg(not(target_arch = "wasm32"))]
fn find_assets() {
    if std::env::var_os("BEVY_ASSET_ROOT").is_some() || std::env::var_os("CARGO_MANIFEST_DIR").is_some() {
        return;
    }
    let candidates = [
        std::env::current_exe().ok().and_then(|exe| exe.parent().map(|dir| dir.to_path_buf())),
        std::env::current_dir().ok(),
        Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))),
    ];
    if let Some(root) = candidates.into_iter().flatten().find(|dir| dir.join("assets").is_dir()) {
        // Set before any other thread starts
        std::env::set_var("BEVY_ASSET_ROOT", root);
    }
}
