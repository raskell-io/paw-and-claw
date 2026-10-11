use bevy::prelude::*;

/// Main game states
#[derive(States, Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum GameState {
    #[default]
    Menu,
    Battle,
    Editor,
}

/// The opening sequence on the title screen: disclaimers, the studio stamp,
/// the animated intro, the title with "Press any button", then the main menu.
/// Any button skips ahead a stage; once it reaches `Menu` it stays there for
/// the session (retreating from a battle goes straight back to the menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BootStage {
    #[default]
    Disclaimer,
    Studio,
    Intro,
    Title,
    Menu,
}

/// Where the opening sequence is, and how long the current stage has run
#[derive(Resource, Debug, Default)]
pub struct BootSequence {
    pub stage: BootStage,
    pub elapsed: f32,
    /// The intro was cut short by a button, so its song shouldn't keep
    /// ringing under the title
    pub skipped_intro: bool,
}

impl BootSequence {
    pub fn intro_skipped(&self) -> bool {
        self.skipped_intro
    }

    pub fn go(&mut self, stage: BootStage) {
        self.stage = stage;
        self.elapsed = 0.0;
    }
}
