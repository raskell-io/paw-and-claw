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

// Future: use crate::states::GameState;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MapPlugin)
            .add_plugins(UnitPlugin)
            .add_plugins(TurnPlugin)
            .add_plugins(MovementPlugin)
            .add_plugins(CombatPlugin)
            .add_plugins(AiPlugin)
            .add_plugins(FogPlugin)
            .add_plugins(CommanderPlugin);
    }
}
