use bevy::prelude::*;

/// Main game states
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    Menu,
    Battle,
    Editor,
}
