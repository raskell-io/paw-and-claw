use bevy::prelude::*;
use std::collections::HashSet;

use super::{Faction, FactionMember, Unit, GridPosition, GameMap, Tile, Terrain, TurnState, Commanders};

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogOfWar>()
            .add_systems(Update, (
                update_fog_of_war,
                apply_fog_to_tiles,
                apply_fog_to_units,
            ).chain());
    }
}

/// Visibility state for a tile
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TileVisibility {
    /// Never seen - completely hidden
    #[default]
    Unexplored,
    /// Previously seen but not currently visible - terrain visible, units hidden
    Fogged,
    /// Currently visible - everything visible
    Visible,
}

/// Resource tracking fog of war state
#[derive(Resource)]
pub struct FogOfWar {
    /// Whether fog of war is enabled
    pub enabled: bool,
    /// Visibility state for each tile (x, y) for the player (Eastern faction)
    visibility: HashSet<(i32, i32)>,
    /// Tiles that have been explored (seen at least once)
    explored: HashSet<(i32, i32)>,
    /// Map dimensions for bounds checking
    width: u32,
    height: u32,
}

impl Default for FogOfWar {
    fn default() -> Self {
        Self {
            enabled: true,
            visibility: HashSet::new(),
            explored: HashSet::new(),
            width: 0,
            height: 0,
        }
    }
}

impl FogOfWar {
    /// Get visibility state for a tile
    pub fn get_visibility(&self, x: i32, y: i32) -> TileVisibility {
        if !self.enabled {
            return TileVisibility::Visible;
        }

        if self.visibility.contains(&(x, y)) {
            TileVisibility::Visible
        } else if self.explored.contains(&(x, y)) {
            TileVisibility::Fogged
        } else {
            TileVisibility::Unexplored
        }
    }

    /// Check if a position is currently visible
    #[allow(dead_code)]
    pub fn is_visible(&self, x: i32, y: i32) -> bool {
        !self.enabled || self.visibility.contains(&(x, y))
    }

    /// Mark a tile as explored (for CO powers that reveal the map)
    pub fn mark_explored(&mut self, x: i32, y: i32) {
        self.explored.insert((x, y));
        self.visibility.insert((x, y));  // Also make currently visible
    }

    /// Clear current visibility (called at start of turn)
    fn clear_visibility(&mut self) {
        self.visibility.clear();
    }

    /// Add visibility around a position with given range
    fn add_vision(&mut self, x: i32, y: i32, range: u32, map: &GameMap) {
        let range = range as i32;
        for dx in -range..=range {
            for dy in -range..=range {
                let dist = dx.abs() + dy.abs();
                if dist <= range {
                    let tx = x + dx;
                    let ty = y + dy;
                    if tx >= 0 && tx < map.width as i32 && ty >= 0 && ty < map.height as i32 {
                        // Check if vision is blocked by terrain (forests reduce vision)
                        let can_see = self.check_line_of_sight(x, y, tx, ty, map, range as u32);
                        if can_see {
                            self.visibility.insert((tx, ty));
                            self.explored.insert((tx, ty));
                        }
                    }
                }
            }
        }
    }

    /// Simple line of sight check - forests block vision beyond them
    fn check_line_of_sight(&self, _x1: i32, _y1: i32, x2: i32, y2: i32, map: &GameMap, _range: u32) -> bool {
        // Simplified: just check if the target tile is within range
        // In a full implementation, you'd trace a line and check for blocking terrain

        // For now, forests reduce vision by 1 (you can see into a forest but not through it easily)
        if let Some(_terrain) = map.get(x2, y2) {
            // Can always see into any terrain directly
            return true;
        }
        true
    }

    /// Update map dimensions
    fn set_dimensions(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}

/// Component to track visibility for rendering (for future fog overlay sprites)
#[derive(Component)]
#[allow(dead_code)]
pub struct FogOverlay;

/// System to update fog of war based on player unit positions
fn update_fog_of_war(
    mut fog: ResMut<FogOfWar>,
    map: Res<GameMap>,
    _turn_state: Res<TurnState>,
    units: Query<(&GridPosition, &Unit, &FactionMember)>,
    tiles: Query<&Tile>,
    commanders: Res<Commanders>,
) {
    if !fog.enabled {
        return;
    }

    // Update dimensions if needed
    if fog.width != map.width || fog.height != map.height {
        fog.set_dimensions(map.width, map.height);
    }

    // Clear current visibility
    fog.clear_visibility();

    // Get CO vision bonus for player faction
    let co_bonuses = commanders.get_bonuses(Faction::Eastern);

    // Add vision from all player (Eastern) units
    for (pos, unit, faction) in units.iter() {
        if faction.faction == Faction::Eastern {
            let stats = unit.unit_type.stats();
            let mut vision = stats.vision + co_bonuses.vision;

            // Terrain affects vision - boulders provide height advantage
            if let Some(terrain) = map.get(pos.x, pos.y) {
                match terrain {
                    Terrain::Boulder => vision += 1, // Height advantage
                    Terrain::Hollow => vision += 1, // Elevated position
                    _ => {}
                }
            }

            fog.add_vision(pos.x, pos.y, vision, &map);
        }
    }

    // Properties we own also provide vision (like in Advance Wars)
    for tile in tiles.iter() {
        if tile.owner == Some(Faction::Eastern) && tile.terrain.is_capturable() {
            // Bases and outposts provide 2 vision
            let vision = match tile.terrain {
                Terrain::Base => 3,
                Terrain::Outpost => 2,
                Terrain::Storehouse => 1,
                _ => 1,
            };
            fog.add_vision(tile.position.x, tile.position.y, vision, &map);
        }
    }
}

/// Apply fog visual effect to tiles
fn apply_fog_to_tiles(
    fog: Res<FogOfWar>,
    mut tiles: Query<(&Tile, &mut Sprite)>,
) {
    if !fog.enabled {
        // Restore full brightness if fog disabled
        for (_, mut sprite) in tiles.iter_mut() {
            sprite.color = sprite.color.with_alpha(1.0);
        }
        return;
    }

    for (tile, mut sprite) in tiles.iter_mut() {
        let visibility = fog.get_visibility(tile.position.x, tile.position.y);

        match visibility {
            TileVisibility::Visible => {
                // Full brightness
                let base_color = tile.terrain.color();
                sprite.color = base_color;
            }
            TileVisibility::Fogged => {
                // Darkened - 40% brightness
                let base_color = tile.terrain.color();
                let darkened = Color::srgba(
                    base_color.to_srgba().red * 0.4,
                    base_color.to_srgba().green * 0.4,
                    base_color.to_srgba().blue * 0.4,
                    1.0,
                );
                sprite.color = darkened;
            }
            TileVisibility::Unexplored => {
                // Very dark - 20% brightness
                let base_color = tile.terrain.color();
                let very_dark = Color::srgba(
                    base_color.to_srgba().red * 0.2,
                    base_color.to_srgba().green * 0.2,
                    base_color.to_srgba().blue * 0.2,
                    1.0,
                );
                sprite.color = very_dark;
            }
        }
    }
}

/// Apply fog to units - hide enemy units in fog
fn apply_fog_to_units(
    fog: Res<FogOfWar>,
    mut units: Query<(&GridPosition, &FactionMember, &mut Visibility, &Children)>,
    mut child_visibility: Query<&mut Visibility, Without<FactionMember>>,
) {
    if !fog.enabled {
        // Show all units if fog disabled
        for (_, _, mut vis, children) in units.iter_mut() {
            *vis = Visibility::Visible;
            for child in children.iter() {
                if let Ok(mut child_vis) = child_visibility.get_mut(*child) {
                    *child_vis = Visibility::Visible;
                }
            }
        }
        return;
    }

    for (pos, faction, mut vis, children) in units.iter_mut() {
        // Player units are always visible to the player
        if faction.faction == Faction::Eastern {
            *vis = Visibility::Visible;
            for child in children.iter() {
                if let Ok(mut child_vis) = child_visibility.get_mut(*child) {
                    *child_vis = Visibility::Visible;
                }
            }
            continue;
        }

        // Enemy units are only visible if in visible tile
        let visibility = fog.get_visibility(pos.x, pos.y);
        match visibility {
            TileVisibility::Visible => {
                *vis = Visibility::Visible;
                for child in children.iter() {
                    if let Ok(mut child_vis) = child_visibility.get_mut(*child) {
                        *child_vis = Visibility::Visible;
                    }
                }
            }
            TileVisibility::Fogged | TileVisibility::Unexplored => {
                // Hide enemy units in fog
                *vis = Visibility::Hidden;
                for child in children.iter() {
                    if let Ok(mut child_vis) = child_visibility.get_mut(*child) {
                        *child_vis = Visibility::Hidden;
                    }
                }
            }
        }
    }
}
