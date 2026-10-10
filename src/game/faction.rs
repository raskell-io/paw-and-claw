use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Factions based on geographic animal kingdoms
/// Each represents animals native to different continents with distinct playstyles
/// Art style inspired by Root: A Game of Woodland Might and Right
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Faction {
    /// Eastern Empire - Asian animals (tanuki, red panda, crane, kitsune)
    /// Playstyle: Industrial might, swarm tactics, quantity over quality
    Eastern,
    /// Northern Realm - European animals (badger, hedgehog, stoat, owl)
    /// Playstyle: Balanced forces, strong defense, disciplined armies
    Northern,
    /// Western Frontier - American animals (raccoon, opossum, coyote, hawk)
    /// Playstyle: Guerrilla tactics, mobility, resourcefulness
    Western,
    /// Southern Pride - African animals (lion, elephant, rhino, hyena)
    /// Playstyle: Raw power, territorial control, endurance
    Southern,
    /// Nether Dominion - Subterranean creatures (moles, naked mole rats, beetles)
    /// Playstyle: Ambush tactics, terrain manipulation, relentless expansion
    /// Antagonist faction (like Black Hole in Advance Wars)
    Nether,
}

impl Faction {

    /// Get all faction variants
    pub fn all() -> &'static [Faction] {
        &[
            Faction::Eastern,
            Faction::Northern,
            Faction::Western,
            Faction::Southern,
            Faction::Nether,
        ]
    }
}

/// Component marking an entity as belonging to a faction
#[derive(Component, Debug, Clone)]
pub struct FactionMember {
    pub faction: Faction,
}

