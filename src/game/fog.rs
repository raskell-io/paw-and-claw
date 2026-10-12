use bevy::prelude::*;
use std::collections::HashSet;

use super::{Faction, FactionMember, Unit, GridPosition, GameMap, Tile, Terrain, TurnState, Commanders, Weather, TerrainFeature, GameData, BattleConfig};

pub struct FogPlugin;

impl Plugin for FogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FogOfWar>()
            .add_systems(Update, (
                update_fog_of_war,
                apply_fog_to_tiles,
                apply_fog_to_features,
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
    /// Tiles in sight whose unit is still hidden: submerged divers with no
    /// enemy next to them (AW Sub)
    concealed: HashSet<(i32, i32)>,
    /// Map dimensions for bounds checking
    width: u32,
    height: u32,
}

impl Default for FogOfWar {
    fn default() -> Self {
        Self {
            enabled: false,  // Disabled by default for easier testing
            visibility: HashSet::new(),
            explored: HashSet::new(),
            concealed: HashSet::new(),
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

    /// Check if a position, and any unit on it, is currently visible
    pub fn is_visible(&self, x: i32, y: i32) -> bool {
        !self.enabled || (self.visibility.contains(&(x, y)) && !self.concealed.contains(&(x, y)))
    }

    /// Clear current visibility (called at start of turn)
    fn clear_visibility(&mut self) {
        self.visibility.clear();
        self.concealed.clear();
    }

    /// Add visibility in a diamond around a position. As in Advance Wars,
    /// forests and reefs (thickets and lily pads) are only seen into from an
    /// adjacent tile, unless `see_hidden` (AW Sonja's powers).
    fn add_vision_seeing(&mut self, x: i32, y: i32, range: u32, map: &GameMap, see_hidden: bool) {
        let range = range as i32;
        for dx in -range..=range {
            for dy in -range..=range {
                let dist = dx.abs() + dy.abs();
                let (tx, ty) = (x + dx, y + dy);
                let Some(terrain) = map.get(tx, ty) else { continue };
                if dist > range || (dist > 1 && hides_units(terrain) && !see_hidden) {
                    continue;
                }
                self.visibility.insert((tx, ty));
                self.explored.insert((tx, ty));
            }
        }
    }

    /// Update map dimensions
    fn set_dimensions(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }
}

/// Terrain that hides units from anyone not right next to them (AW forest and reef)
pub fn hides_units(terrain: Terrain) -> bool {
    matches!(terrain, Terrain::Thicket | Terrain::LilyPads)
}

/// System to update fog of war based on player unit positions
fn update_fog_of_war(
    mut fog: ResMut<FogOfWar>,
    map: Res<GameMap>,
    turn_state: Res<TurnState>,
    units: Query<(&GridPosition, &Unit, &FactionMember)>,
    tiles: Query<&Tile>,
    commanders: Res<Commanders>,
    weather: Res<Weather>,
    game_data: Res<GameData>,
    battle_config: Res<BattleConfig>,
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

    // The board is seen through the eyes of the person playing (or playing
    // next); allies share what they see
    let viewer = battle_config.viewer(turn_state.current_faction);
    let friendly = |f: Faction| battle_config.allied(f, viewer);

    // Add vision from all friendly units
    for (pos, unit, faction) in units.iter() {
        if friendly(faction.faction) {
            let co_bonuses = commanders.get_bonuses(faction.faction, &game_data);
            let mut vision = co_bonuses.total_vision(unit.unit_type, &weather, &game_data);

            // AW: foot soldiers on a mountain see 3 further
            let on_foot = matches!(game_data.unit_class(unit.unit_type), super::UnitClass::Foot | super::UnitClass::Mech);
            if on_foot && matches!(map.get(pos.x, pos.y), Some(Terrain::Boulder | Terrain::Brambles)) {
                vision += 3;
            }
            fog.add_vision_seeing(pos.x, pos.y, vision, &map, co_bonuses.mods.see_hidden);
        }
    }

    // AW: a submerged diver stays hidden unless one of our units is next to it
    let ours: Vec<(i32, i32)> = units.iter().filter(|(_, _, f)| friendly(f.faction)).map(|(p, _, _)| (p.x, p.y)).collect();
    for (pos, unit, faction) in units.iter() {
        if !friendly(faction.faction) && unit.submerged
            && !ours.iter().any(|&(x, y)| (x - pos.x).abs() + (y - pos.y).abs() <= 1)
        {
            fog.concealed.insert((pos.x, pos.y));
        }
    }

    // AW: properties we own reveal their own tile only
    for tile in tiles.iter() {
        if tile.owner.is_some_and(friendly) {
            fog.visibility.insert((tile.position.x, tile.position.y));
            fog.explored.insert((tile.position.x, tile.position.y));
        }
    }
}

/// Brightness of terrain by visibility: what's in sight, what was seen, what never was
fn fog_shade(visibility: TileVisibility) -> f32 {
    match visibility {
        TileVisibility::Visible => 1.0,
        TileVisibility::Fogged => 0.55,
        TileVisibility::Unexplored => 0.4,
    }
}

/// Darken tiles out of sight (each tile has its own material)
fn apply_fog_to_tiles(
    fog: Res<FogOfWar>,
    tiles: Query<(&Tile, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    game_data: Res<GameData>,
) {
    for (tile, material) in tiles.iter() {
        let shade = fog_shade(fog.get_visibility(tile.position.x, tile.position.y));
        let Some(material) = materials.get_mut(&material.0) else { continue };
        let textured = material.base_color_texture.is_some();
        let base = super::tile_tint(&game_data, tile.terrain, tile.owner, textured).to_srgba();
        let color = Color::srgb(base.red * shade, base.green * shade, base.blue * shade);
        if material.base_color != color {
            material.base_color = color;
        }
    }
}

/// Darken terrain features and buildings out of sight
fn apply_fog_to_features(
    fog: Res<FogOfWar>,
    features: Query<(&TerrainFeature, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (feature, material) in features.iter() {
        let shade = fog_shade(fog.get_visibility(feature.grid_position.x, feature.grid_position.y));
        let Some(material) = materials.get_mut(&material.0) else { continue };
        let color = Color::srgb(shade, shade, shade);
        if material.base_color != color {
            material.base_color = color;
        }
    }
}

/// Apply fog to units - hide enemy units in fog
fn apply_fog_to_units(
    fog: Res<FogOfWar>,
    mut units: Query<(&GridPosition, &FactionMember, &mut Visibility, &Children)>,
    mut child_visibility: Query<&mut Visibility, Without<FactionMember>>,
    battle_config: Res<BattleConfig>,
    turn_state: Res<TurnState>,
) {
    if !fog.enabled {
        // Show all units if fog disabled
        for (_, _, mut vis, children) in units.iter_mut() {
            *vis = Visibility::Visible;
            for child in children.iter() {
                if let Ok(mut child_vis) = child_visibility.get_mut(child) {
                    *child_vis = Visibility::Visible;
                }
            }
        }
        return;
    }

    for (pos, faction, mut vis, children) in units.iter_mut() {
        // The viewer's and allies' units are always visible
        if battle_config.allied(faction.faction, battle_config.viewer(turn_state.current_faction)) {
            *vis = Visibility::Visible;
            for child in children.iter() {
                if let Ok(mut child_vis) = child_visibility.get_mut(child) {
                    *child_vis = Visibility::Visible;
                }
            }
            continue;
        }

        // Enemy units are only visible if in visible tile (and not hidden underwater)
        let visibility = fog.get_visibility(pos.x, pos.y);
        let visibility = if fog.is_visible(pos.x, pos.y) { visibility } else { TileVisibility::Fogged };
        match visibility {
            TileVisibility::Visible => {
                *vis = Visibility::Visible;
                for child in children.iter() {
                    if let Ok(mut child_vis) = child_visibility.get_mut(child) {
                        *child_vis = Visibility::Visible;
                    }
                }
            }
            TileVisibility::Fogged | TileVisibility::Unexplored => {
                // Hide enemy units in fog
                *vis = Visibility::Hidden;
                for child in children.iter() {
                    if let Ok(mut child_vis) = child_visibility.get_mut(child) {
                        *child_vis = Visibility::Hidden;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forests_hide_units_from_all_but_neighbours() {
        let map = GameMap {
            width: 5,
            height: 1,
            tiles: vec![vec![Terrain::Grass, Terrain::Thicket, Terrain::Grass, Terrain::Thicket, Terrain::Grass]],
        };
        let mut fog = FogOfWar { enabled: true, ..Default::default() };
        fog.add_vision_seeing(0, 0, 4, &map, false);
        assert!(fog.is_visible(1, 0), "a forest next to you is seen");
        assert!(fog.is_visible(2, 0) && fog.is_visible(4, 0));
        assert!(!fog.is_visible(3, 0), "a forest further away isn't");
    }
}
