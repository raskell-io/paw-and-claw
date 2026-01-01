use bevy::prelude::*;

mod map;
mod unit;
mod faction;
mod combat;
mod movement;
mod turn;

pub use map::*;
pub use unit::*;
pub use faction::*;
pub use combat::*;
pub use movement::*;
pub use turn::*;

// Future: use crate::states::GameState;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MapPlugin)
            .add_plugins(UnitPlugin)
            .add_plugins(TurnPlugin)
            .add_plugins(MovementPlugin)
            .add_plugins(CombatPlugin);
    }
}
