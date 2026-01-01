use bevy::prelude::*;

/// Main game states
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    Menu,
    Battle,
    Editor,
    Campaign,
    Roguelike,
}

/// Battle phase states
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum BattlePhase {
    #[default]
    Waiting,
    SelectingUnit,
    SelectingMove,
    SelectingTarget,
    Animating,
    EnemyTurn,
    Victory,
    Defeat,
}
