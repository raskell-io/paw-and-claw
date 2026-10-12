use bevy::prelude::*;

mod map;
mod maps;
mod unit;
mod faction;
mod combat;
mod movement;
mod turn;
mod ai;
mod fog;
mod commander;
mod weather;
mod sprites;
mod assets;
mod save;
mod custom_maps;
mod battle_scene;
mod modding;
mod effects;
mod audio;
mod touch;
mod diorama;

pub use map::*;
pub use maps::*;
pub use unit::*;
pub use faction::*;
pub use combat::*;
pub use movement::*;
pub use turn::*;
pub use ai::*;
pub use fog::*;
pub use commander::*;
pub use weather::*;
pub use sprites::*;
pub use assets::*;
pub use save::*;
pub use custom_maps::*;
pub use battle_scene::*;
pub use modding::*;
pub use effects::*;
pub use audio::*;
pub use touch::*;

// Future: use crate::states::GameState;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ModdingPlugin)  // Load game data first
            .add_plugins(AssetPlugin)
            .add_plugins(MapPlugin)
            .add_plugins(UnitPlugin)
            .add_plugins(TurnPlugin)
            .add_plugins(MovementPlugin)
            .add_plugins(CombatPlugin)
            .add_plugins(EffectsPlugin)
            .add_plugins(GameAudioPlugin)
            .add_plugins(TouchControlsPlugin)
            .add_plugins(CustomMapsPlugin)
            .add_plugins(BattleScenePlugin)
            .add_plugins(diorama::DioramaPlugin)
            .add_plugins(AiPlugin)
            .add_plugins(FogPlugin)
            .add_plugins(CommanderPlugin)
            .add_plugins(WeatherPlugin)
            .add_plugins(SpritePlugin)
            .add_plugins(SavePlugin);
    }
}
