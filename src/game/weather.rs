use bevy::prelude::*;
use rand::Rng;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Weather>()
            .add_event::<WeatherChangedEvent>()
            .add_systems(Update, weather_change_system);
    }
}

// ============================================================================
// WEATHER TYPES & EFFECTS
// ============================================================================

/// Weather conditions that affect gameplay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum WeatherType {
    #[default]
    Clear,      // No effects - baseline weather
    Rain,       // Reduced vision, forests lose defense bonus
    Snow,       // Reduced movement, slight defense boost
    Sandstorm,  // Reduced vision and attack
    Fog,        // Heavily reduced vision
}

impl WeatherType {
    /// Display name for UI
    pub fn name(&self) -> &'static str {
        match self {
            WeatherType::Clear => "Clear",
            WeatherType::Rain => "Rain",
            WeatherType::Snow => "Snow",
            WeatherType::Sandstorm => "Sandstorm",
            WeatherType::Fog => "Dense Fog",
        }
    }

    /// Description of effects
    pub fn description(&self) -> &'static str {
        match self {
            WeatherType::Clear => "Perfect conditions. No combat modifiers.",
            WeatherType::Rain => "Vision -1. Forests provide no defense bonus.",
            WeatherType::Snow => "Movement -1 for all units. Defense +10%.",
            WeatherType::Sandstorm => "Vision -2. Attack -15% for all units.",
            WeatherType::Fog => "Vision reduced to 1. Perfect for ambushes.",
        }
    }

    /// Icon/emoji for weather display
    pub fn icon(&self) -> &'static str {
        match self {
            WeatherType::Clear => "☀",
            WeatherType::Rain => "🌧",
            WeatherType::Snow => "❄",
            WeatherType::Sandstorm => "🌪",
            WeatherType::Fog => "🌫",
        }
    }

    /// Get all weather types for random selection
    pub fn all() -> &'static [WeatherType] {
        &[
            WeatherType::Clear,
            WeatherType::Rain,
            WeatherType::Snow,
            WeatherType::Sandstorm,
            WeatherType::Fog,
        ]
    }

    /// Weight for random selection (Clear is more common)
    pub fn weight(&self) -> u32 {
        match self {
            WeatherType::Clear => 50,     // 50% base chance
            WeatherType::Rain => 15,      // 15%
            WeatherType::Snow => 10,      // 10%
            WeatherType::Sandstorm => 10, // 10%
            WeatherType::Fog => 15,       // 15%
        }
    }
}

/// Computed weather effects that modify gameplay
#[derive(Debug, Clone, Copy, Default)]
pub struct WeatherEffects {
    /// Vision modifier (added to base vision, can be negative)
    pub vision_modifier: i32,
    /// Movement modifier (added to base movement, can be negative)
    pub movement_modifier: i32,
    /// Attack multiplier (1.0 = no change)
    pub attack_multiplier: f32,
    /// Defense multiplier (1.0 = no change)
    pub defense_multiplier: f32,
    /// Whether forests provide their defense bonus
    pub forests_provide_cover: bool,
    /// Maximum vision (hard cap, 0 = no cap)
    pub vision_cap: u32,
}

impl WeatherEffects {
    pub fn from_weather(weather: WeatherType) -> Self {
        match weather {
            WeatherType::Clear => Self {
                vision_modifier: 0,
                movement_modifier: 0,
                attack_multiplier: 1.0,
                defense_multiplier: 1.0,
                forests_provide_cover: true,
                vision_cap: 0,
            },
            WeatherType::Rain => Self {
                vision_modifier: -1,
                movement_modifier: 0,
                attack_multiplier: 1.0,
                defense_multiplier: 1.0,
                forests_provide_cover: false, // Rain negates forest cover
                vision_cap: 0,
            },
            WeatherType::Snow => Self {
                vision_modifier: 0,
                movement_modifier: -1,
                attack_multiplier: 1.0,
                defense_multiplier: 1.1, // +10% defense
                forests_provide_cover: true,
                vision_cap: 0,
            },
            WeatherType::Sandstorm => Self {
                vision_modifier: -2,
                movement_modifier: 0,
                attack_multiplier: 0.85, // -15% attack
                defense_multiplier: 1.0,
                forests_provide_cover: true,
                vision_cap: 0,
            },
            WeatherType::Fog => Self {
                vision_modifier: 0,
                movement_modifier: 0,
                attack_multiplier: 1.0,
                defense_multiplier: 1.0,
                forests_provide_cover: true,
                vision_cap: 1, // Vision capped at 1
            },
        }
    }
}

// ============================================================================
// WEATHER RESOURCE & EVENTS
// ============================================================================

/// Resource tracking current weather state
#[derive(Resource)]
pub struct Weather {
    /// Current weather condition
    pub current: WeatherType,
    /// Turns remaining for current weather (0 = permanent until changed)
    pub turns_remaining: u32,
    /// Whether weather changes randomly each turn
    pub dynamic_weather: bool,
    /// Chance (0-100) of weather changing each turn when dynamic
    pub change_chance: u32,
    /// Cached effects for current weather
    effects: WeatherEffects,
}

impl Default for Weather {
    fn default() -> Self {
        Self {
            current: WeatherType::Clear,
            turns_remaining: 0,
            dynamic_weather: true,
            change_chance: 20, // 20% chance per turn
            effects: WeatherEffects::from_weather(WeatherType::Clear),
        }
    }
}

impl Weather {
    /// Create weather with specific type
    pub fn new(weather_type: WeatherType) -> Self {
        Self {
            current: weather_type,
            turns_remaining: 0,
            dynamic_weather: true,
            change_chance: 20,
            effects: WeatherEffects::from_weather(weather_type),
        }
    }

    /// Set weather to a specific type
    pub fn set(&mut self, weather_type: WeatherType) {
        self.current = weather_type;
        self.effects = WeatherEffects::from_weather(weather_type);
    }

    /// Set weather with duration
    pub fn set_for_turns(&mut self, weather_type: WeatherType, turns: u32) {
        self.set(weather_type);
        self.turns_remaining = turns;
    }

    /// Get current weather effects
    pub fn effects(&self) -> &WeatherEffects {
        &self.effects
    }

    /// Apply vision modifier (returns modified vision, minimum 1)
    pub fn apply_vision(&self, base_vision: u32) -> u32 {
        let modified = (base_vision as i32 + self.effects.vision_modifier).max(1) as u32;
        if self.effects.vision_cap > 0 {
            modified.min(self.effects.vision_cap)
        } else {
            modified
        }
    }

    /// Apply movement modifier (returns modified movement, minimum 1)
    pub fn apply_movement(&self, base_movement: u32) -> u32 {
        (base_movement as i32 + self.effects.movement_modifier).max(1) as u32
    }

    /// Apply attack modifier
    pub fn apply_attack(&self, base_attack: f32) -> f32 {
        base_attack * self.effects.attack_multiplier
    }

    /// Apply defense modifier
    pub fn apply_defense(&self, base_defense: f32) -> f32 {
        base_defense * self.effects.defense_multiplier
    }

    /// Check if forests provide cover in current weather
    pub fn forests_provide_cover(&self) -> bool {
        self.effects.forests_provide_cover
    }

    /// Randomly select a new weather type
    pub fn random_weather() -> WeatherType {
        let mut rng = rand::thread_rng();
        let total_weight: u32 = WeatherType::all().iter().map(|w| w.weight()).sum();
        let mut roll = rng.gen_range(0..total_weight);

        for weather in WeatherType::all() {
            if roll < weather.weight() {
                return *weather;
            }
            roll -= weather.weight();
        }

        WeatherType::Clear
    }

    /// Try to change weather randomly based on change_chance
    pub fn try_random_change(&mut self) -> Option<WeatherType> {
        if !self.dynamic_weather {
            return None;
        }

        // If weather has duration, decrement it
        if self.turns_remaining > 0 {
            self.turns_remaining -= 1;
            if self.turns_remaining > 0 {
                return None;
            }
        }

        let mut rng = rand::thread_rng();
        if rng.gen_range(0..100) < self.change_chance {
            let new_weather = Self::random_weather();
            if new_weather != self.current {
                self.set(new_weather);
                return Some(new_weather);
            }
        }

        None
    }
}

/// Event fired when weather changes
#[derive(Event)]
pub struct WeatherChangedEvent {
    pub old_weather: WeatherType,
    pub new_weather: WeatherType,
}

// ============================================================================
// SYSTEMS
// ============================================================================

/// System that handles weather changes at turn start
fn weather_change_system(
    mut weather: ResMut<Weather>,
    turn_state: Res<crate::game::TurnState>,
    mut weather_events: EventWriter<WeatherChangedEvent>,
    mut last_turn: Local<u32>,
) {
    // Only check at start of new turns
    if turn_state.turn_number == *last_turn {
        return;
    }
    *last_turn = turn_state.turn_number;

    // Skip turn 1 (game start)
    if turn_state.turn_number <= 1 {
        return;
    }

    let old_weather = weather.current;
    if let Some(new_weather) = weather.try_random_change() {
        info!("Weather changed from {:?} to {:?}!", old_weather, new_weather);
        weather_events.send(WeatherChangedEvent {
            old_weather,
            new_weather,
        });
    }
}
