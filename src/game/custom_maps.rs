//! Maps made in the map editor: kept with the saves (`saves/map_<name>.json`
//! on desktop, localStorage on the web) and offered in skirmish setup next to
//! the built-in maps.

use bevy::prelude::*;

use super::{list_settings, read_setting, remove_setting, write_setting, MapData, Terrain};

pub struct CustomMapsPlugin;

impl Plugin for CustomMapsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CustomMaps::read());
    }
}

/// Every stored custom map, as (storage name, map)
#[derive(Resource, Default)]
pub struct CustomMaps {
    pub maps: Vec<(String, MapData)>,
}

impl CustomMaps {
    fn read() -> Self {
        let maps = list_settings("map_").into_iter()
            .filter_map(|key| {
                let map: MapData = serde_json::from_str(&read_setting(&key)?).ok()?;
                Some((key, map))
            })
            .collect();
        CustomMaps { maps }
    }

    pub fn refresh(&mut self) {
        *self = Self::read();
    }

    /// Store a map under its name (a map with the same name is replaced)
    pub fn save(&mut self, map: &MapData) -> Result<(), String> {
        let json = serde_json::to_string(map).map_err(|e| e.to_string())?;
        write_setting(&storage_name(&map.name), &json)?;
        self.refresh();
        Ok(())
    }

    pub fn delete(&mut self, key: &str) {
        remove_setting(key);
        self.refresh();
    }
}

/// `map_` and the name in lowercase letters, digits and dashes
fn storage_name(name: &str) -> String {
    let slug: String = name.trim().to_lowercase().chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let slug = slug.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
    format!("map_{}", if slug.is_empty() { "untitled".to_string() } else { slug })
}

/// What keeps a map from being played, if anything: every army needs an HQ
pub fn map_problem(map: &MapData) -> Option<String> {
    let armies = map.army_count();
    let missing: Vec<usize> = (0..armies)
        .filter(|&i| map.hq_of(super::ARMY_SLOTS[i]).is_none())
        .map(|i| i + 1)
        .collect();
    if map.properties.iter().filter(|p| map.get_terrain(p.x, p.y) == Some(Terrain::Base)).count() < 2 {
        return Some("Needs an HQ for at least two armies".to_string());
    }
    (!missing.is_empty()).then(|| format!("Army {} has no HQ",
        missing.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_names_are_tidy() {
        assert_eq!(storage_name("  Twin Rivers! "), "map_twin-rivers");
        assert_eq!(storage_name("???"), "map_untitled");
    }
}
