use bevy::prelude::*;

use super::{Faction, FactionMember, Unit};

pub struct TurnPlugin;

impl Plugin for TurnPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TurnState>()
            .add_systems(Update, check_victory_condition);
    }
}

/// Tracks turn state
#[derive(Resource)]
pub struct TurnState {
    pub current_faction: Faction,
    pub turn_number: u32,
    pub phase: TurnPhase,
}

impl Default for TurnState {
    fn default() -> Self {
        Self {
            current_faction: Faction::Eastern,
            turn_number: 1,
            phase: TurnPhase::Select,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnPhase {
    Select,     // Selecting a unit
    Move,       // Unit selected, choosing where to move
    Action,     // Unit moved, choosing action (attack/wait)
    Animating,  // Playing animation
}

impl TurnState {
    pub fn end_turn<'a>(&mut self, units: impl Iterator<Item = &'a mut Unit>) {
        // Reset all units of current faction
        for unit in units {
            unit.reset_turn();
        }

        // Switch to other faction (simple 2-player for now)
        self.current_faction = match self.current_faction {
            Faction::Eastern => Faction::Northern,
            Faction::Northern => {
                self.turn_number += 1;
                Faction::Eastern
            }
            _ => Faction::Eastern,
        };

        self.phase = TurnPhase::Select;
    }
}

fn check_victory_condition(
    units: Query<(&Unit, &FactionMember)>,
    _turn_state: Res<TurnState>,
) {
    let mut eastern_units = 0;
    let mut northern_units = 0;

    for (_unit, faction) in units.iter() {
        match faction.faction {
            Faction::Eastern => eastern_units += 1,
            Faction::Northern => northern_units += 1,
            _ => {}
        }
    }

    if eastern_units == 0 {
        info!("Northern Realm wins!");
    } else if northern_units == 0 {
        info!("Eastern Empire wins!");
    }
}
