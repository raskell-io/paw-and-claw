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

/// Battle phase states (for future state machine implementation)
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
#[allow(dead_code)]
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
