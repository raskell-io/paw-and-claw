use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Factions based on geographic animal kingdoms
/// Each represents animals native to different continents with distinct playstyles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Faction {
    /// Eastern Empire - Asian animals (tanuki, red panda, crane, kitsune)
    /// Playstyle: Industrial might, swarm tactics, quantity over quality
    Eastern,
    /// Northern Realm - European animals (badger, hedgehog, stoat, owl)
    /// Playstyle: Balanced forces, strong defense, disciplined armies
    Northern,
    /// Southern Frontier - American animals (raccoon, opossum, coyote, hawk)
    /// Playstyle: Guerrilla tactics, mobility, resourcefulness
    Southern,
    /// The Wanderer - Lone wolf mercenary
    /// Playstyle: Single powerful agent, diplomacy, versatility
    Wanderer,
}

impl Faction {
    pub fn name(&self) -> &'static str {
        match self {
            Faction::Eastern => "Eastern Empire",
            Faction::Northern => "Northern Realm",
            Faction::Southern => "Southern Frontier",
            Faction::Wanderer => "The Wanderer",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Faction::Eastern => "Disciplined armies of the rising sun. Strength in numbers.",
            Faction::Northern => "Stalwart defenders of the ancient forests. Unbreakable lines.",
            Faction::Southern => "Cunning survivalists of the wild frontier. Strike and fade.",
            Faction::Wanderer => "A lone warrior seeking fortune. One blade, many paths.",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Faction::Eastern => Color::srgb(0.9, 0.3, 0.3),   // Red (East Asian motif)
            Faction::Northern => Color::srgb(0.3, 0.5, 0.9),  // Blue (European motif)
            Faction::Southern => Color::srgb(0.4, 0.7, 0.3),  // Green (American wilderness)
            Faction::Wanderer => Color::srgb(0.6, 0.5, 0.4),  // Brown (neutral)
        }
    }

    pub fn unit_cost_modifier(&self) -> f32 {
        match self {
            Faction::Eastern => 0.85,   // Cheaper units, swarm tactics
            Faction::Northern => 1.0,   // Standard costs
            Faction::Southern => 0.95,  // Slightly cheaper, hit and run
            Faction::Wanderer => 2.0,   // Very expensive but powerful
        }
    }

    /// Representative animals for this faction
    pub fn animals(&self) -> &'static [&'static str] {
        match self {
            Faction::Eastern => &["Tanuki", "Red Panda", "Crane", "Kitsune", "Macaque"],
            Faction::Northern => &["Badger", "Hedgehog", "Stoat", "Owl", "Fox"],
            Faction::Southern => &["Raccoon", "Opossum", "Coyote", "Hawk", "Armadillo"],
            Faction::Wanderer => &["Wolf"],
        }
    }
}

/// Component marking an entity as belonging to a faction
#[derive(Component, Debug, Clone)]
pub struct FactionMember {
    pub faction: Faction,
}

/// Resource tracking the current player's faction
#[derive(Resource, Default)]
pub struct CurrentFaction(pub Option<Faction>);
