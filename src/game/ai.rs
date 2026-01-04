use bevy::prelude::*;
use bevy::ecs::system::SystemParam;
use std::collections::{HashMap, HashSet};

use super::{
    Faction, FactionMember, Unit, UnitType, GridPosition, GameMap, Tile, Terrain,
    TurnState, TurnPhase, FactionFunds, AttackEvent, CaptureEvent, GameResult,
    calculate_movement_range, calculate_damage, spawn_unit, CoBonuses,
    Commanders, PowerActivatedEvent, Weather, WeatherType, SpriteAssetsParam,
    UnitAnimation, effective_movement, GameData,
};

/// Bundled AI-related resources to stay under Bevy's system parameter limit
#[derive(SystemParam)]
struct AiResources<'w> {
    ai_state: ResMut<'w, AiState>,
    turn_plan: ResMut<'w, AiTurnPlan>,
    memory: ResMut<'w, AiMemory>,
    game_data: Res<'w, GameData>,
}

pub struct AiPlugin;

impl Plugin for AiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AiState>()
            .init_resource::<AiTurnPlan>()
            .init_resource::<AiMemory>()
            .add_systems(Update, ai_turn_system);
    }
}

// ============================================================================
// AI CONFIGURATION & TYPES
// ============================================================================

/// AI personality types - affects HOW the AI plays (risk tolerance, aggression)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AiPersonality {
    #[default]
    Aggressive,  // High risk tolerance, always pushes forward
    Cautious,    // Calculates trades carefully, avoids bad engagements
    Reckless,    // Will sacrifice units for objectives
    Methodical,  // Slow, deliberate, prefers good positions
}

/// AI strategy types - affects WHAT the AI prioritizes (objectives, goals)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiStrategy {
    #[default]
    Balanced,      // Adapt to situation, no strong preference
    Annihilation,  // Kill all enemy units, ignore economy - "leave no survivors"
    Domination,    // Capture all properties, starve enemy of income
    Blitz,         // Rush enemy HQ, ignore everything else
    Attrition,     // Trade efficiently, wear down enemy over time
    Swarm,         // Mass cheap units, overwhelm with numbers
    Fortress,      // Defend key positions, counter-attack only
}

/// AI configuration combining personality and strategy
#[derive(Resource, Clone)]
pub struct AiConfig {
    pub personality: AiPersonality,
    pub strategy: AiStrategy,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            personality: AiPersonality::Aggressive,
            strategy: AiStrategy::Balanced,
        }
    }
}

impl AiConfig {
    pub fn new(personality: AiPersonality, strategy: AiStrategy) -> Self {
        Self { personality, strategy }
    }

    /// Get risk tolerance multiplier (how much the AI discounts danger)
    pub fn risk_tolerance(&self) -> f32 {
        match self.personality {
            AiPersonality::Aggressive => 0.5,  // Ignores half the risk
            AiPersonality::Cautious => 1.5,    // Amplifies risk perception
            AiPersonality::Reckless => 0.2,    // Nearly ignores risk
            AiPersonality::Methodical => 1.0,  // Normal risk assessment
        }
    }

    /// Get attack preference multiplier
    pub fn attack_preference(&self) -> f32 {
        match self.personality {
            AiPersonality::Aggressive => 1.5,
            AiPersonality::Cautious => 0.8,
            AiPersonality::Reckless => 2.0,
            AiPersonality::Methodical => 1.0,
        }
    }

    /// Get position value multiplier (how much AI values good terrain)
    pub fn position_value(&self) -> f32 {
        match self.personality {
            AiPersonality::Aggressive => 0.5,   // Doesn't care much about position
            AiPersonality::Cautious => 1.5,     // Values good positions highly
            AiPersonality::Reckless => 0.3,     // Ignores terrain advantages
            AiPersonality::Methodical => 2.0,   // Very focused on positioning
        }
    }
}

#[derive(Resource)]
pub struct AiState {
    pub enabled: bool,
    pub action_delay: Timer,
    pub phase: AiTurnPhase,
    pub config: AiConfig,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            enabled: true,
            action_delay: Timer::from_seconds(0.2, TimerMode::Once),
            phase: AiTurnPhase::Waiting,
            config: AiConfig::default(),
        }
    }
}

impl AiState {
    pub fn with_config(personality: AiPersonality, strategy: AiStrategy) -> Self {
        Self {
            config: AiConfig::new(personality, strategy),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiTurnPhase {
    #[default]
    Waiting,
    Planning,
    ExecutingActions,
    Production,
    EndingTurn,
}

/// Memory of past turns for opponent modeling
#[derive(Resource, Default)]
pub struct AiMemory {
    /// Track where player units were last turn
    player_last_positions: HashMap<Entity, (i32, i32)>,
    /// Track player's aggressive tendency (how often they attack vs defend)
    player_aggression: f32,
    /// Track which of our units the player tends to target
    targeted_unit_types: HashMap<UnitType, u32>,
    /// Turns since game start
    turn_count: u32,
}

#[derive(Resource, Default)]
struct AiTurnPlan {
    actions: Vec<PlannedAction>,
    current_index: usize,
}

#[derive(Debug, Clone)]
struct PlannedAction {
    unit: Entity,
    action: AiAction,
    priority: f32,
}

#[derive(Debug, Clone)]
enum AiAction {
    Attack { move_to: (i32, i32), target: Entity },
    Capture { move_to: (i32, i32), tile: Entity },
    Move { move_to: (i32, i32) },
    Wait,
}

// ============================================================================
// INFLUENCE MAPS - Core of spatial reasoning
// ============================================================================

/// Influence map containing multiple layers of spatial information
#[allow(dead_code)]
struct InfluenceMaps {
    width: i32,
    height: i32,
    /// Our military control (positive = we control, negative = enemy controls)
    territory: HashMap<(i32, i32), f32>,
    /// Threat level from enemy units
    enemy_threat: HashMap<(i32, i32), f32>,
    /// Our defensive coverage
    friendly_support: HashMap<(i32, i32), f32>,
    /// Strategic value of positions (bases, chokepoints, high ground)
    strategic_value: HashMap<(i32, i32), f32>,
    /// Tension zones (where our influence meets enemy influence)
    frontline: HashMap<(i32, i32), f32>,
    /// Safe retreat paths
    retreat_value: HashMap<(i32, i32), f32>,
}

impl InfluenceMaps {
    fn new(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            territory: HashMap::new(),
            enemy_threat: HashMap::new(),
            friendly_support: HashMap::new(),
            strategic_value: HashMap::new(),
            frontline: HashMap::new(),
            retreat_value: HashMap::new(),
        }
    }

    fn get_territory(&self, x: i32, y: i32) -> f32 {
        *self.territory.get(&(x, y)).unwrap_or(&0.0)
    }

    fn get_threat(&self, x: i32, y: i32) -> f32 {
        *self.enemy_threat.get(&(x, y)).unwrap_or(&0.0)
    }

    fn get_support(&self, x: i32, y: i32) -> f32 {
        *self.friendly_support.get(&(x, y)).unwrap_or(&0.0)
    }

    fn get_strategic(&self, x: i32, y: i32) -> f32 {
        *self.strategic_value.get(&(x, y)).unwrap_or(&0.0)
    }

    fn get_frontline(&self, x: i32, y: i32) -> f32 {
        *self.frontline.get(&(x, y)).unwrap_or(&0.0)
    }

    fn is_behind_lines(&self, x: i32, y: i32) -> bool {
        self.get_territory(x, y) > 0.3 && self.get_threat(x, y) < 20.0
    }
}

fn build_influence_maps(
    analysis: &GameAnalysis,
    map: &GameMap,
    tiles: &[(Entity, Tile)],
    game_data: &GameData,
) -> InfluenceMaps {
    let mut maps = InfluenceMaps::new(map.width as i32, map.height as i32);

    // === TERRITORY INFLUENCE ===
    // Each unit projects influence that decays with distance
    for unit in &analysis.ai_units {
        let strength = get_unit_strength(unit) * unit.hp_percent;
        project_influence(&mut maps.territory, unit.pos.x, unit.pos.y, strength, 5, map);
    }
    for unit in &analysis.enemy_units {
        let strength = get_unit_strength(unit) * unit.hp_percent;
        project_influence(&mut maps.territory, unit.pos.x, unit.pos.y, -strength, 5, map);
    }

    // === ENEMY THREAT MAP ===
    // Project threat based on attack range and damage potential
    for enemy in &analysis.enemy_units {
        let stats = enemy.unit.unit_type.stats();
        if stats.attack == 0 {
            continue;
        }

        // Skip enemies with no ammo if they need it
        if stats.max_ammo > 0 && enemy.unit.ammo == 0 {
            continue; // No threat without ammo
        }

        // Calculate all positions this enemy could attack from (limited by stamina)
        let actual_movement = effective_movement(stats.movement, enemy.unit.stamina);
        let moves = calculate_movement_range(
            &enemy.pos,
            actual_movement,
            map,
            &analysis.unit_positions,
            stats.class,
            game_data,
        );

        for (mx, my) in moves {
            let (min_r, max_r) = stats.attack_range;
            for dx in -(max_r as i32)..=(max_r as i32) {
                for dy in -(max_r as i32)..=(max_r as i32) {
                    let dist = (dx.abs() + dy.abs()) as u32;
                    if dist >= min_r && dist <= max_r {
                        let tx = mx + dx;
                        let ty = my + dy;
                        if tx >= 0 && tx < map.width as i32 && ty >= 0 && ty < map.height as i32 {
                            let threat = stats.attack as f32 * enemy.hp_percent;
                            *maps.enemy_threat.entry((tx, ty)).or_insert(0.0) += threat;
                        }
                    }
                }
            }
        }
    }

    // === FRIENDLY SUPPORT MAP ===
    // How much backup each position has
    for unit in &analysis.ai_units {
        let stats = unit.unit.unit_type.stats();
        if stats.attack > 0 {
            let support = stats.attack as f32 * unit.hp_percent * 0.5;
            project_influence(&mut maps.friendly_support, unit.pos.x, unit.pos.y, support, 3, map);
        }
    }

    // === STRATEGIC VALUE MAP ===
    // Bases, outposts, terrain chokepoints
    for (_entity, tile) in tiles {
        let x = tile.position.x;
        let y = tile.position.y;

        // Capturable properties are valuable
        if tile.terrain.is_capturable() {
            let base_value = tile.terrain.income_value() as f32 * 2.0;
            let ownership_bonus = match tile.owner {
                Some(Faction::Northern) => 5.0,  // Defend our stuff
                Some(_) => 15.0,                  // Enemy property = high value target
                None => 10.0,                     // Neutral = capture opportunity
            };
            *maps.strategic_value.entry((x, y)).or_insert(0.0) += base_value + ownership_bonus;

            // Bases are extra valuable (production!)
            if tile.terrain == Terrain::Base {
                *maps.strategic_value.entry((x, y)).or_insert(0.0) += 30.0;
            }
        }

        // Defensive terrain is strategically valuable
        let defense = map.get(x, y).map(|t| t.defense_bonus()).unwrap_or(0);
        if defense >= 2 {
            *maps.strategic_value.entry((x, y)).or_insert(0.0) += defense as f32 * 3.0;
        }
    }

    // === FRONTLINE DETECTION ===
    // Where territory control is contested
    for x in 0..map.width as i32 {
        for y in 0..map.height as i32 {
            let territory = maps.get_territory(x, y);
            // Frontline is where control is close to zero (contested)
            let frontline_intensity = 1.0 - territory.abs().min(1.0);
            if frontline_intensity > 0.3 {
                maps.frontline.insert((x, y), frontline_intensity);
            }
        }
    }

    // === RETREAT VALUE ===
    // Paths back to our bases
    for (_entity, tile) in tiles {
        if tile.terrain == Terrain::Base && tile.owner == Some(Faction::Northern) {
            project_influence(
                &mut maps.retreat_value,
                tile.position.x,
                tile.position.y,
                50.0,
                10,
                map,
            );
        }
    }

    maps
}

/// Project influence from a point with distance decay
fn project_influence(
    map_layer: &mut HashMap<(i32, i32), f32>,
    cx: i32,
    cy: i32,
    strength: f32,
    radius: i32,
    game_map: &GameMap,
) {
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            let x = cx + dx;
            let y = cy + dy;
            if x >= 0 && x < game_map.width as i32 && y >= 0 && y < game_map.height as i32 {
                let dist = (dx.abs() + dy.abs()) as f32;
                if dist <= radius as f32 {
                    // Exponential decay with distance
                    let decay = (-dist * 0.5).exp();
                    *map_layer.entry((x, y)).or_insert(0.0) += strength * decay;
                }
            }
        }
    }
}

fn get_unit_strength(unit: &UnitInfo) -> f32 {
    let stats = unit.unit.unit_type.stats();
    (stats.attack as f32 + stats.defense as f32 * 0.5) * (unit.value as f32 / 50.0)
}

// ============================================================================
// STRATEGIC LAYER - High-level goal selection
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
enum StrategicGoal {
    /// Aggressive push - attack enemy units and territory
    Attack { priority: f32 },
    /// Defend key positions
    Defend { priority: f32 },
    /// Expand economy by capturing properties
    Expand { priority: f32 },
    /// Consolidate - pull back and regroup
    Consolidate { priority: f32 },
}

fn determine_strategic_goals(
    analysis: &GameAnalysis,
    _influence: &InfluenceMaps,
    memory: &AiMemory,
    config: &AiConfig,
) -> Vec<StrategicGoal> {
    let mut goals = Vec::new();

    // Calculate strategic situation
    let our_strength: f32 = analysis.ai_units.iter()
        .map(|u| get_unit_strength(u) * u.hp_percent)
        .sum();
    let enemy_strength: f32 = analysis.enemy_units.iter()
        .map(|u| get_unit_strength(u) * u.hp_percent)
        .sum();

    let strength_ratio = if enemy_strength > 0.0 {
        our_strength / enemy_strength
    } else {
        2.0 // We're winning hard
    };

    let our_income = analysis.our_properties.len() as f32;
    let enemy_income = analysis.enemy_properties.len() as f32;
    let neutral_properties = analysis.capturable_tiles.iter()
        .filter(|t| t.owner.is_none())
        .count() as f32;

    // Count damaged units and unit counts
    let damaged_units = analysis.ai_units.iter()
        .filter(|u| u.hp_percent < 0.5)
        .count();
    let total_units = analysis.ai_units.len();
    let enemy_units = analysis.enemy_units.len();

    // Check for enemy HQ
    let enemy_hq = analysis.enemy_properties.iter()
        .find(|t| t.terrain == Terrain::Base);

    // =========================================================================
    // STRATEGY-BASED GOAL PRIORITIES
    // Strategy determines the BASE priorities, personality modifies them
    // =========================================================================

    let (mut attack_priority, mut defend_priority, mut expand_priority, mut consolidate_priority): (f32, f32, f32, f32) =
        match config.strategy {
            AiStrategy::Balanced => {
                // Adapt to situation
                let attack = if strength_ratio > 1.0 { 50.0 } else { 30.0 };
                let defend = if strength_ratio < 0.8 { 40.0 } else { 20.0 };
                let expand = if neutral_properties > 0.0 { 35.0 } else { 10.0 };
                let consolidate = 0.0;
                (attack, defend, expand, consolidate)
            }
            AiStrategy::Annihilation => {
                // Kill everything - attacks are always top priority
                // "The only good enemy is a dead enemy"
                let attack = 80.0 + (enemy_units as f32 * 5.0); // More enemies = more to kill
                let defend = 10.0; // Minimal defense
                let expand = 5.0;  // Ignore economy
                let consolidate = 0.0; // Never retreat
                (attack, defend, expand, consolidate)
            }
            AiStrategy::Domination => {
                // Capture everything, strangle enemy economy
                let expand = 70.0 + (neutral_properties * 10.0);
                let attack = 30.0; // Attack to clear capturers
                let defend = 40.0; // Defend captured properties
                let consolidate = 0.0;
                (attack, defend, expand, consolidate)
            }
            AiStrategy::Blitz => {
                // Rush enemy HQ, ignore everything else
                let attack = if enemy_hq.is_some() { 90.0 } else { 50.0 };
                let expand = 60.0; // Capture HQ specifically
                let defend = 5.0;  // Don't defend, keep pushing
                let consolidate = 0.0;
                (attack, defend, expand, consolidate)
            }
            AiStrategy::Attrition => {
                // Trade efficiently, wear them down
                // Only attack when we have advantage
                let attack = if strength_ratio > 1.2 { 60.0 } else { 20.0 };
                let defend = 50.0; // Strong defense
                let expand = 40.0; // Build economy
                let consolidate = if strength_ratio < 0.8 { 30.0 } else { 0.0 };
                (attack, defend, expand, consolidate)
            }
            AiStrategy::Swarm => {
                // Overwhelm with numbers
                let attack = 50.0 + (total_units as f32 * 3.0); // More units = more aggressive
                let expand = 60.0; // Need income for units
                let defend = 20.0;
                let consolidate = 0.0;
                (attack, defend, expand, consolidate)
            }
            AiStrategy::Fortress => {
                // Defend, only counter-attack
                let defend = 70.0;
                let attack = if strength_ratio > 1.5 { 40.0 } else { 15.0 }; // Counter-attack when strong
                let expand = 30.0;
                let consolidate = if damaged_units > 0 { 20.0 } else { 0.0 };
                (attack, defend, expand, consolidate)
            }
        };

    // =========================================================================
    // PERSONALITY MODIFIERS
    // Personality adjusts the strategy-based priorities
    // =========================================================================

    match config.personality {
        AiPersonality::Aggressive => {
            attack_priority *= 1.3;
            defend_priority *= 0.7;
            consolidate_priority *= 0.3;
        }
        AiPersonality::Cautious => {
            attack_priority *= 0.8;
            defend_priority *= 1.2;
            // More likely to consolidate when hurt
            if total_units > 0 && damaged_units as f32 / total_units as f32 > 0.4 {
                consolidate_priority += 15.0;
            }
        }
        AiPersonality::Reckless => {
            attack_priority *= 1.5;
            defend_priority *= 0.5;
            consolidate_priority = 0.0; // Never retreat
        }
        AiPersonality::Methodical => {
            // Values position, doesn't rush
            defend_priority *= 1.1;
            expand_priority *= 1.2;
        }
    }

    // =========================================================================
    // SITUATIONAL ADJUSTMENTS
    // =========================================================================

    // Strength advantage encourages attacking
    if strength_ratio > 1.3 {
        attack_priority += 20.0;
    }

    // Strength disadvantage (except for reckless/annihilation)
    if strength_ratio < 0.6 && config.strategy != AiStrategy::Annihilation {
        defend_priority += 20.0;
        if config.personality != AiPersonality::Reckless {
            consolidate_priority += 15.0;
        }
    }

    // React to player aggression
    if memory.player_aggression > 0.7 {
        defend_priority += 15.0;
    } else if memory.player_aggression < 0.3 {
        attack_priority += 10.0;
    }

    // Early game expansion bonus
    if memory.turn_count < 5 && config.strategy != AiStrategy::Annihilation {
        expand_priority += 15.0;
    }

    // Economic disadvantage
    if enemy_income > our_income + 1.0 && config.strategy != AiStrategy::Annihilation {
        expand_priority += (enemy_income - our_income) * 8.0;
    }

    goals.push(StrategicGoal::Attack { priority: attack_priority.max(0.0) });
    goals.push(StrategicGoal::Defend { priority: defend_priority.max(0.0) });
    goals.push(StrategicGoal::Expand { priority: expand_priority.max(0.0) });
    goals.push(StrategicGoal::Consolidate { priority: consolidate_priority.max(0.0) });

    // Sort by priority
    goals.sort_by(|a, b| {
        let pa = match a {
            StrategicGoal::Attack { priority } => *priority,
            StrategicGoal::Defend { priority } => *priority,
            StrategicGoal::Expand { priority } => *priority,
            StrategicGoal::Consolidate { priority } => *priority,
        };
        let pb = match b {
            StrategicGoal::Attack { priority } => *priority,
            StrategicGoal::Defend { priority } => *priority,
            StrategicGoal::Expand { priority } => *priority,
            StrategicGoal::Consolidate { priority } => *priority,
        };
        pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
    });

    goals
}

// ============================================================================
// UTILITY CURVES - Non-linear scoring functions
// ============================================================================

/// Attempt to model diminishing returns and thresholds
struct UtilityCurves;

impl UtilityCurves {
    /// Damage utility - killing is much better than wounding
    fn damage_utility(damage: i32, target_hp: i32, target_value: u32) -> f32 {
        let damage_ratio = (damage as f32 / target_hp as f32).min(1.0);

        if damage >= target_hp {
            // Kill! Very high utility - should almost always be worth it
            target_value as f32 * 5.0 + 50.0
        } else if damage_ratio > 0.7 {
            // Nearly kill - good but not as good as kill
            target_value as f32 * 3.0 * damage_ratio
        } else if damage_ratio > 0.4 {
            // Significant damage
            target_value as f32 * 2.0 * damage_ratio
        } else {
            // Minor damage - still worthwhile
            target_value as f32 * damage_ratio
        }
    }

    /// Risk utility - exponential penalty for high risk
    fn risk_utility(potential_damage: i32, our_hp: i32, our_value: u32) -> f32 {
        let damage_ratio = potential_damage as f32 / our_hp as f32;

        if potential_damage >= our_hp {
            // We die - very bad
            -(our_value as f32 * 2.0)
        } else if damage_ratio > 0.7 {
            // Critically wounded
            -(our_value as f32 * damage_ratio * 1.5)
        } else if damage_ratio > 0.3 {
            // Moderate damage
            -(our_value as f32 * damage_ratio * 0.8)
        } else {
            // Light damage - acceptable
            -(our_value as f32 * damage_ratio * 0.3)
        }
    }

    /// Position safety utility with threshold
    /// Note: These penalties are intentionally moderate - combat is inherently risky
    fn position_safety(threat: f32, our_hp: i32) -> f32 {
        let danger_ratio = threat / our_hp as f32;

        if danger_ratio > 1.5 {
            -25.0  // Dangerous but sometimes worth it
        } else if danger_ratio > 1.0 {
            -15.0  // Risky
        } else if danger_ratio > 0.5 {
            -5.0 * danger_ratio  // Slightly risky
        } else {
            0.0  // Acceptable risk
        }
    }

    /// Capture utility - almost capturing is much better than starting
    fn capture_utility(progress: i32, required: i32, income: u32, is_base: bool) -> f32 {
        let completion_ratio = progress as f32 / required as f32;

        let base_value = income as f32 * 3.0 + if is_base { 50.0 } else { 0.0 };

        if completion_ratio >= 1.0 {
            base_value * 2.0  // Complete capture this turn
        } else if completion_ratio > 0.5 {
            base_value * (1.0 + completion_ratio)  // Significant progress
        } else if completion_ratio > 0.0 {
            base_value * (0.5 + completion_ratio)  // Some progress
        } else {
            base_value * 0.5  // Starting fresh
        }
    }

    /// Support utility - being near allies is good, but diminishing returns
    fn support_utility(nearby_allies: usize) -> f32 {
        match nearby_allies {
            0 => -10.0,  // Isolated - bad
            1 => 5.0,
            2 => 12.0,
            3 => 16.0,
            _ => 18.0,   // Diminishing returns after 3
        }
    }
}

// ============================================================================
// OPPONENT MODELING - Predict player behavior
// ============================================================================

fn predict_enemy_actions(
    analysis: &GameAnalysis,
    _influence: &InfluenceMaps,
    memory: &AiMemory,
    map: &GameMap,
    game_data: &GameData,
) -> Vec<PredictedAction> {
    let mut predictions = Vec::new();

    for enemy in &analysis.enemy_units {
        let stats = enemy.unit.unit_type.stats();

        // Calculate possible moves (limited by stamina)
        let actual_movement = effective_movement(stats.movement, enemy.unit.stamina);
        let moves = calculate_movement_range(
            &enemy.pos,
            actual_movement,
            map,
            &analysis.unit_positions,
            stats.class,
            game_data,
        );

        // Find best predicted action for this enemy
        let mut best_score = f32::NEG_INFINITY;
        let mut best_action = PredictedAction {
            unit: enemy.entity,
            likely_position: (enemy.pos.x, enemy.pos.y),
            likely_target: None,
            confidence: 0.5,
        };

        // Check if enemy has ammo (if they need it)
        let can_attack = stats.attack > 0 && (stats.max_ammo == 0 || enemy.unit.ammo > 0);

        for (mx, my) in &moves {
            let mut score = 0.0;

            // Check if they can attack one of our units from here
            if can_attack {
                for ai_unit in &analysis.ai_units {
                    let dist = ((mx - ai_unit.pos.x).abs() + (my - ai_unit.pos.y).abs()) as u32;
                    let (min_r, max_r) = stats.attack_range;

                    if dist >= min_r && dist <= max_r {
                        // They can attack us! (use neutral bonuses for prediction)
                        let no_bonus = CoBonuses::none();
                        let clear_weather = Weather::new(WeatherType::Clear);
                        let damage = calculate_damage(&enemy.unit, &ai_unit.unit,
                            map.get(ai_unit.pos.x, ai_unit.pos.y).unwrap_or(Terrain::Grass),
                            &no_bonus, &no_bonus, &clear_weather, game_data);

                        // Players tend to go for kills
                        if damage >= ai_unit.unit.hp {
                            score += 100.0;
                        } else {
                            score += damage as f32;
                        }

                        // Players target high-value units
                        score += ai_unit.value as f32 * 0.5;

                        // Check player's targeting history
                        if let Some(&count) = memory.targeted_unit_types.get(&ai_unit.unit_type) {
                            score += count as f32 * 5.0;
                        }

                        if score > best_score {
                            best_score = score;
                            best_action = PredictedAction {
                                unit: enemy.entity,
                                likely_position: (*mx, *my),
                                likely_target: Some(ai_unit.entity),
                                confidence: 0.7,
                            };
                        }
                    }
                }
            }

            // Check if they might capture
            if stats.can_capture {
                for tile in &analysis.our_properties {
                    if tile.pos.x == *mx && tile.pos.y == *my {
                        score = 80.0; // High priority to predict captures
                        if score > best_score {
                            best_score = score;
                            best_action = PredictedAction {
                                unit: enemy.entity,
                                likely_position: (*mx, *my),
                                likely_target: None,
                                confidence: 0.6,
                            };
                        }
                    }
                }
            }

            // Movement toward our base
            let dist_to_our_base = analysis.our_properties.iter()
                .filter(|t| t.terrain == Terrain::Base)
                .map(|t| ((mx - t.pos.x).abs() + (my - t.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);

            let advance_score = 30.0 - dist_to_our_base;
            if advance_score > best_score && best_action.likely_target.is_none() {
                best_score = advance_score;
                best_action = PredictedAction {
                    unit: enemy.entity,
                    likely_position: (*mx, *my),
                    likely_target: None,
                    confidence: 0.4,
                };
            }
        }

        predictions.push(best_action);
    }

    predictions
}

#[derive(Clone)]
struct PredictedAction {
    unit: Entity,
    likely_position: (i32, i32),
    likely_target: Option<Entity>,
    confidence: f32,
}

// ============================================================================
// GAME STATE ANALYSIS
// ============================================================================

struct GameAnalysis {
    ai_units: Vec<UnitInfo>,
    enemy_units: Vec<UnitInfo>,
    capturable_tiles: Vec<TileInfo>,
    our_properties: Vec<TileInfo>,
    enemy_properties: Vec<TileInfo>,
    unit_positions: HashMap<(i32, i32), Entity>,
}

#[derive(Clone)]
struct UnitInfo {
    entity: Entity,
    pos: GridPosition,
    unit: Unit,
    unit_type: UnitType,
    hp_percent: f32,
    value: u32,
    is_indirect: bool,
}

#[derive(Clone)]
struct TileInfo {
    entity: Entity,
    pos: GridPosition,
    terrain: Terrain,
    owner: Option<Faction>,
    capture_progress: i32,
    capturing_faction: Option<Faction>,
}

fn analyze_game_state(
    units: &[(Entity, GridPosition, FactionMember, Unit)],
    tiles: &[(Entity, Tile)],
    ai_faction: Faction,
) -> GameAnalysis {
    let mut ai_units = Vec::new();
    let mut enemy_units = Vec::new();
    let mut unit_positions = HashMap::new();

    for (entity, pos, faction, unit) in units {
        unit_positions.insert((pos.x, pos.y), *entity);

        let stats = unit.unit_type.stats();
        let info = UnitInfo {
            entity: *entity,
            pos: pos.clone(),
            unit: unit.clone(),
            unit_type: unit.unit_type,
            hp_percent: unit.hp_percentage(),
            value: get_unit_value_by_type(unit.unit_type),
            is_indirect: stats.attack_range.0 > 1,
        };

        if faction.faction == ai_faction {
            ai_units.push(info);
        } else {
            enemy_units.push(info);
        }
    }

    let mut capturable_tiles = Vec::new();
    let mut our_properties = Vec::new();
    let mut enemy_properties = Vec::new();

    for (entity, tile) in tiles {
        let info = TileInfo {
            entity: *entity,
            pos: GridPosition::new(tile.position.x, tile.position.y),
            terrain: tile.terrain,
            owner: tile.owner,
            capture_progress: tile.capture_progress,
            capturing_faction: tile.capturing_faction,
        };

        if tile.terrain.is_capturable() {
            if tile.owner == Some(ai_faction) {
                our_properties.push(info.clone());
            } else if tile.owner.is_some() && tile.owner != Some(ai_faction) {
                enemy_properties.push(info.clone());
                capturable_tiles.push(info);
            } else {
                capturable_tiles.push(info);
            }
        }
    }

    GameAnalysis {
        ai_units,
        enemy_units,
        capturable_tiles,
        our_properties,
        enemy_properties,
        unit_positions,
    }
}

fn get_unit_value_by_type(unit_type: UnitType) -> u32 {
    match unit_type {
        UnitType::Scout => 10,
        UnitType::Shocktrooper => 30,
        UnitType::Recon => 40,
        UnitType::Siege => 60,
        UnitType::Ironclad => 70,
        UnitType::Juggernaut => 100,
        UnitType::Behemoth => 150,
        UnitType::Flak => 50,
        UnitType::Barrage => 80,
        UnitType::Stinger => 60,
        UnitType::Carrier => 40,
        UnitType::Supplier => 40,
        _ => 50,
    }
}

// ============================================================================
// ADVANCED SCORING WITH ALL SYSTEMS
// ============================================================================

fn score_action(
    unit: &UnitInfo,
    action: &AiAction,
    analysis: &GameAnalysis,
    influence: &InfluenceMaps,
    goals: &[StrategicGoal],
    predictions: &[PredictedAction],
    map: &GameMap,
    config: &AiConfig,
    game_data: &GameData,
) -> f32 {
    let primary_goal = goals.first().cloned().unwrap_or(StrategicGoal::Attack { priority: 50.0 });

    match action {
        AiAction::Attack { move_to, target } => {
            score_attack_action(unit, *move_to, *target, analysis, influence, &primary_goal, predictions, map, config, game_data)
        }
        AiAction::Capture { move_to, tile } => {
            score_capture_action(unit, *move_to, *tile, analysis, influence, &primary_goal, predictions, map, config)
        }
        AiAction::Move { move_to } => {
            score_move_action(unit, *move_to, analysis, influence, &primary_goal, predictions, map, config)
        }
        AiAction::Wait => {
            score_wait_action(unit, influence, config)
        }
    }
}

fn score_attack_action(
    attacker: &UnitInfo,
    move_to: (i32, i32),
    target: Entity,
    analysis: &GameAnalysis,
    influence: &InfluenceMaps,
    goal: &StrategicGoal,
    predictions: &[PredictedAction],
    map: &GameMap,
    config: &AiConfig,
    game_data: &GameData,
) -> f32 {
    let target_unit = match analysis.enemy_units.iter().find(|u| u.entity == target) {
        Some(u) => u,
        None => return -1000.0,
    };

    // Check ammo - can't attack without ammo if unit uses ammo
    let attacker_stats = attacker.unit.unit_type.stats();
    if attacker_stats.max_ammo > 0 && attacker.unit.ammo == 0 {
        return -1000.0; // No ammo, can't attack
    }

    // Use neutral bonuses for AI prediction (actual combat uses real CO bonuses)
    let no_bonus = CoBonuses::none();
    let clear_weather = Weather::new(WeatherType::Clear);

    let defender_terrain = map.get(target_unit.pos.x, target_unit.pos.y).unwrap_or(Terrain::Grass);
    let damage = calculate_damage(&attacker.unit, &target_unit.unit, defender_terrain, &no_bonus, &no_bonus, &clear_weather, game_data);

    // === BASE DAMAGE UTILITY ===
    // Apply personality attack preference
    let mut score = UtilityCurves::damage_utility(damage, target_unit.unit.hp, target_unit.value)
        * config.attack_preference();

    // Base attack bonus - attacking is generally good in a tactics game
    score += 20.0 * config.attack_preference();

    // === COUNTER-ATTACK RISK ===
    // Apply personality risk tolerance (lower = ignores risk more)
    let target_stats = target_unit.unit.unit_type.stats();
    if target_stats.attack > 0 && target_stats.attack_range.0 == 1 && target_unit.unit.hp > damage {
        let attacker_terrain = map.get(move_to.0, move_to.1).unwrap_or(Terrain::Grass);
        let mut temp_target = target_unit.unit.clone();
        temp_target.hp -= damage;
        let counter = calculate_damage(&temp_target, &attacker.unit, attacker_terrain, &no_bonus, &no_bonus, &clear_weather, game_data);
        score += UtilityCurves::risk_utility(counter, attacker.unit.hp, attacker.value)
            * config.risk_tolerance();
    }

    // === FOCUS FIRE BONUS ===
    // If another unit is also attacking this target, big bonus (coordinated attack)
    if target_unit.hp_percent < 0.6 {
        score += 35.0; // Target is already damaged, focus fire!
    }

    // === PREEMPTIVE STRIKE ===
    // If this enemy is predicted to attack us, bonus for hitting them first
    for pred in predictions {
        if pred.unit == target && pred.likely_target.is_some() && pred.confidence > 0.5 {
            score += 25.0; // Preemptive strike!
        }
    }

    // === STRATEGY-SPECIFIC BONUSES ===
    match config.strategy {
        AiStrategy::Annihilation => {
            // Massive bonus for any attack, extra for kills
            score += 30.0;
            if damage >= target_unit.unit.hp {
                score += 50.0; // Kill bonus
            }
        }
        AiStrategy::Attrition => {
            // Bonus for favorable trades
            let our_loss_estimate = if target_stats.attack > 0 && target_stats.attack_range.0 == 1 {
                (target_stats.attack as f32 * 0.3) as i32
            } else { 0 };
            if damage > our_loss_estimate * 2 {
                score += 25.0; // Good trade
            }
        }
        AiStrategy::Blitz => {
            // Bonus for attacking units blocking path to HQ
            let dist_to_enemy_hq = analysis.enemy_properties.iter()
                .filter(|t| t.terrain == Terrain::Base)
                .map(|t| ((target_unit.pos.x - t.pos.x).abs() + (target_unit.pos.y - t.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            if dist_to_enemy_hq < 5.0 {
                score += 30.0; // Clearing path to HQ
            }
        }
        _ => {}
    }

    // === STRATEGIC GOAL ALIGNMENT ===
    match goal {
        StrategicGoal::Attack { priority } => {
            score += priority * 0.5;
        }
        StrategicGoal::Defend { .. } => {
            // Defensive attacks on threats to our territory
            if influence.get_territory(target_unit.pos.x, target_unit.pos.y) > 0.0 {
                score += 20.0; // Attacking invader
            }
        }
        _ => {}
    }

    // === POSITION AFTER ATTACK ===
    // Apply risk tolerance to position safety
    let threat_after = influence.get_threat(move_to.0, move_to.1);
    let estimated_hp_after = attacker.unit.hp - if damage < target_unit.unit.hp {
        if target_stats.attack > 0 && target_stats.attack_range.0 == 1 {
            (target_stats.attack as f32 * 0.3) as i32
        } else { 0 }
    } else { 0 };
    score += UtilityCurves::position_safety(threat_after, estimated_hp_after)
        * config.risk_tolerance();

    // === INFLUENCE MAP BONUSES ===
    // Attacking in contested territory is good
    let frontline = influence.get_frontline(target_unit.pos.x, target_unit.pos.y);
    score += frontline * 15.0;

    // Attacking high-strategic-value positions
    score += influence.get_strategic(target_unit.pos.x, target_unit.pos.y) * 0.3;

    score
}

fn score_capture_action(
    unit: &UnitInfo,
    move_to: (i32, i32),
    tile_entity: Entity,
    analysis: &GameAnalysis,
    influence: &InfluenceMaps,
    goal: &StrategicGoal,
    predictions: &[PredictedAction],
    _map: &GameMap,
    config: &AiConfig,
) -> f32 {
    let tile = match analysis.capturable_tiles.iter().find(|t| t.entity == tile_entity) {
        Some(t) => t,
        None => return -1000.0,
    };

    // Progress toward capture (unit HP = capture power)
    let progress = if tile.capturing_faction == Some(Faction::Northern) {
        tile.capture_progress
    } else {
        0
    };
    let total_progress = progress + unit.unit.hp;
    let required = tile.terrain.capture_points();

    let is_base = tile.terrain == Terrain::Base;
    let income = tile.terrain.income_value();

    // === BASE CAPTURE UTILITY ===
    let mut score = UtilityCurves::capture_utility(total_progress, required, income, is_base);

    // === STRATEGY-SPECIFIC BONUSES ===
    match config.strategy {
        AiStrategy::Domination => {
            // Huge bonus for capturing
            score *= 1.5;
            if tile.owner.is_none() {
                score += 30.0; // Extra for neutral properties
            }
        }
        AiStrategy::Blitz => {
            // Only care about enemy HQ
            if is_base && tile.owner == Some(Faction::Eastern) {
                score += 200.0; // This is the win condition!
            } else {
                score *= 0.3; // Ignore other captures
            }
        }
        AiStrategy::Annihilation => {
            // Don't really care about capturing
            score *= 0.3;
        }
        AiStrategy::Swarm => {
            // Need income for swarm
            score *= 1.3;
        }
        _ => {}
    }

    // === ENEMY BASE = VICTORY CONDITION ===
    if is_base && tile.owner.is_some() && tile.owner != Some(Faction::Northern) {
        score += 150.0; // Capturing enemy base is huge
    }

    // === RISK ASSESSMENT ===
    let threat = influence.get_threat(move_to.0, move_to.1);
    score += UtilityCurves::position_safety(threat, unit.unit.hp) * config.risk_tolerance();

    // If enemies are predicted to attack this position, risky
    for pred in predictions {
        if pred.likely_position == move_to {
            score -= 30.0 * pred.confidence * config.risk_tolerance();
        }
    }

    // === STRATEGIC GOAL ALIGNMENT ===
    match goal {
        StrategicGoal::Expand { priority } => {
            score += priority * 0.8;
        }
        StrategicGoal::Defend { priority } => {
            // Capturing to deny enemy
            if tile.capturing_faction.is_some() && tile.capturing_faction != Some(Faction::Northern) {
                score += priority * 0.5;
            }
        }
        _ => {}
    }

    // === INFLUENCE MAP ===
    // Capturing behind enemy lines is risky (but personality affects this)
    if influence.get_territory(move_to.0, move_to.1) < -0.3 {
        score -= 20.0 * config.risk_tolerance();
    }

    // Capturing in safe territory is better
    if influence.is_behind_lines(move_to.0, move_to.1) {
        score += 15.0;
    }

    score
}

fn score_move_action(
    unit: &UnitInfo,
    move_to: (i32, i32),
    analysis: &GameAnalysis,
    influence: &InfluenceMaps,
    goal: &StrategicGoal,
    predictions: &[PredictedAction],
    map: &GameMap,
    config: &AiConfig,
) -> f32 {
    let mut score = 0.0;

    let terrain = map.get(move_to.0, move_to.1).unwrap_or(Terrain::Grass);
    let defense_bonus = terrain.defense_bonus();

    // === TERRAIN DEFENSE ===
    // Personality affects how much we value defensive terrain
    score += defense_bonus as f32 * 4.0 * config.position_value();

    // === THREAT AVOIDANCE ===
    let threat = influence.get_threat(move_to.0, move_to.1);
    score += UtilityCurves::position_safety(threat, unit.unit.hp) * config.risk_tolerance();

    // === SUPPORT FROM ALLIES ===
    let nearby_allies = analysis.ai_units.iter()
        .filter(|u| {
            let dist = (u.pos.x - move_to.0).abs() + (u.pos.y - move_to.1).abs();
            dist <= 3 && u.entity != unit.entity
        })
        .count();
    score += UtilityCurves::support_utility(nearby_allies);

    // === STRATEGY-SPECIFIC MOVEMENT ===
    match config.strategy {
        AiStrategy::Blitz => {
            // Move toward enemy HQ aggressively
            let dist_to_enemy_hq = analysis.enemy_properties.iter()
                .filter(|t| t.terrain == Terrain::Base)
                .map(|t| ((move_to.0 - t.pos.x).abs() + (move_to.1 - t.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            score += 50.0 - dist_to_enemy_hq * 3.0; // Strong pull toward HQ
        }
        AiStrategy::Fortress => {
            // Stay near our properties
            let dist_to_our_base = analysis.our_properties.iter()
                .filter(|t| t.terrain == Terrain::Base)
                .map(|t| ((move_to.0 - t.pos.x).abs() + (move_to.1 - t.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            if dist_to_our_base < 4.0 {
                score += 20.0; // Bonus for staying close to base
            }
        }
        AiStrategy::Annihilation => {
            // Move toward enemies always
            let dist_to_enemy = analysis.enemy_units.iter()
                .map(|e| ((move_to.0 - e.pos.x).abs() + (move_to.1 - e.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            score += 30.0 - dist_to_enemy * 2.0;
        }
        _ => {}
    }

    // === STRATEGIC GOAL ALIGNMENT ===
    match goal {
        StrategicGoal::Attack { priority } => {
            // Move toward enemies
            let dist_to_enemy = analysis.enemy_units.iter()
                .map(|e| ((move_to.0 - e.pos.x).abs() + (move_to.1 - e.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            score += (20.0 - dist_to_enemy * 1.5) * (priority / 100.0);

            // Move toward frontline
            let frontline = influence.get_frontline(move_to.0, move_to.1);
            score += frontline * priority * 0.3;
        }
        StrategicGoal::Defend { priority } => {
            // Move toward our properties
            let dist_to_property = analysis.our_properties.iter()
                .map(|p| ((move_to.0 - p.pos.x).abs() + (move_to.1 - p.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            score += (15.0 - dist_to_property) * (priority / 100.0);

            // Interpose between enemies and our base
            for pred in predictions {
                if pred.likely_target.is_some() {
                    let dist_to_threat = ((move_to.0 - pred.likely_position.0).abs() +
                        (move_to.1 - pred.likely_position.1).abs()) as f32;
                    if dist_to_threat <= 2.0 {
                        score += 20.0 * pred.confidence;
                    }
                }
            }
        }
        StrategicGoal::Expand { priority } => {
            // Move toward capturable tiles
            let stats = unit.unit.unit_type.stats();
            if stats.can_capture {
                let dist_to_capture = analysis.capturable_tiles.iter()
                    .map(|t| ((move_to.0 - t.pos.x).abs() + (move_to.1 - t.pos.y).abs()) as f32)
                    .min_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(100.0);
                score += (25.0 - dist_to_capture * 2.0) * (priority / 100.0);
            }
        }
        StrategicGoal::Consolidate { priority } => {
            // Move toward safe positions but don't flee too aggressively
            let retreat = influence.retreat_value.get(&move_to).copied().unwrap_or(0.0);
            score += retreat * 0.2 * (priority / 100.0);

            // Slight preference for distance from enemies, but don't run away
            let dist_to_enemy = analysis.enemy_units.iter()
                .map(|e| ((move_to.0 - e.pos.x).abs() + (move_to.1 - e.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            score += dist_to_enemy * 0.2 * (priority / 100.0);
        }
    }

    // === STRATEGIC VALUE OF POSITION ===
    score += influence.get_strategic(move_to.0, move_to.1) * 0.2 * config.position_value();

    // === ARTILLERY POSITIONING ===
    if unit.is_indirect {
        // Stay back but in range
        let can_hit_enemy = analysis.enemy_units.iter().any(|e| {
            let dist = ((move_to.0 - e.pos.x).abs() + (move_to.1 - e.pos.y).abs()) as u32;
            let stats = unit.unit.unit_type.stats();
            dist >= stats.attack_range.0 && dist <= stats.attack_range.1
        });
        if can_hit_enemy {
            score += 25.0;
        }
        // Extra safety for artillery
        score += UtilityCurves::position_safety(threat, unit.unit.hp) * 0.5 * config.risk_tolerance();
    }

    // === RESUPPLY SEEKING ===
    // If low on stamina or ammo, seek friendly supply buildings
    let stats = unit.unit.unit_type.stats();
    let low_stamina = unit.unit.stamina <= stats.max_stamina / 3;
    let low_ammo = stats.max_ammo > 0 && unit.unit.ammo <= stats.max_ammo / 3;

    if low_stamina || low_ammo {
        // Find distance to nearest friendly resupply building (Base or Storehouse)
        let dist_to_supply = analysis.our_properties.iter()
            .filter(|t| matches!(t.terrain, Terrain::Base | Terrain::Storehouse))
            .map(|t| ((move_to.0 - t.pos.x).abs() + (move_to.1 - t.pos.y).abs()) as f32)
            .min_by(|a, b| a.partial_cmp(b).unwrap());

        if let Some(dist) = dist_to_supply {
            // Strong bonus for moving toward supply when resources are low
            let urgency = if low_stamina && low_ammo { 2.0 } else { 1.0 };
            score += (40.0 - dist * 4.0) * urgency;

            // Huge bonus for actually being on a supply building
            if dist == 0.0 {
                score += 50.0 * urgency;
            }
        }
    }

    score
}

fn score_wait_action(unit: &UnitInfo, influence: &InfluenceMaps, config: &AiConfig) -> f32 {
    // Waiting is usually bad, but okay if in a good defensive position
    let threat = influence.get_threat(unit.pos.x, unit.pos.y);
    let support = influence.get_support(unit.pos.x, unit.pos.y);

    // Aggressive personalities hate waiting
    let wait_penalty = match config.personality {
        AiPersonality::Aggressive => -30.0,
        AiPersonality::Reckless => -50.0,
        AiPersonality::Cautious => -10.0,
        AiPersonality::Methodical => -15.0,
    };

    if threat < 10.0 && support > 20.0 {
        wait_penalty + 10.0 // Okay to wait in safe supported position
    } else {
        wait_penalty // Generally want to do something
    }
}

// ============================================================================
// TURN PLANNING
// ============================================================================

fn plan_turn_advanced(
    analysis: &GameAnalysis,
    influence: &InfluenceMaps,
    goals: &[StrategicGoal],
    predictions: &[PredictedAction],
    map: &GameMap,
    config: &AiConfig,
    game_data: &GameData,
) -> Vec<PlannedAction> {
    let mut actions: Vec<PlannedAction> = Vec::new();
    let mut planned_positions: HashSet<(i32, i32)> = HashSet::new();
    let mut assigned_units: HashSet<Entity> = HashSet::new();

    // Collect all possible actions with scores
    let mut all_possible: Vec<(Entity, AiAction, f32)> = Vec::new();

    for ai_unit in &analysis.ai_units {
        if ai_unit.unit.moved {
            continue;
        }

        let stats = ai_unit.unit.unit_type.stats();
        // Use effective movement (limited by stamina)
        let actual_movement = effective_movement(stats.movement, ai_unit.unit.stamina);
        let moves = calculate_movement_range(
            &ai_unit.pos,
            actual_movement,
            map,
            &analysis.unit_positions,
            stats.class,
            game_data,
        );

        for (mx, my) in &moves {
            if analysis.unit_positions.contains_key(&(*mx, *my))
                && (*mx, *my) != (ai_unit.pos.x, ai_unit.pos.y) {
                continue;
            }

            let move_pos = GridPosition::new(*mx, *my);

            // Evaluate attacks
            if stats.attack > 0 {
                for enemy in &analysis.enemy_units {
                    let dist = move_pos.distance_to(&enemy.pos);
                    let (min_r, max_r) = stats.attack_range;

                    if dist >= min_r && dist <= max_r {
                        let action = AiAction::Attack {
                            move_to: (*mx, *my),
                            target: enemy.entity,
                        };
                        let score = score_action(ai_unit, &action, analysis, influence, goals, predictions, map, config, game_data);
                        all_possible.push((ai_unit.entity, action, score));
                    }
                }
            }

            // Evaluate captures
            if stats.can_capture {
                for tile in &analysis.capturable_tiles {
                    if tile.pos.x == *mx && tile.pos.y == *my {
                        let action = AiAction::Capture {
                            move_to: (*mx, *my),
                            tile: tile.entity,
                        };
                        let score = score_action(ai_unit, &action, analysis, influence, goals, predictions, map, config, game_data);
                        all_possible.push((ai_unit.entity, action, score));
                    }
                }
            }

            // Evaluate moves
            let action = AiAction::Move { move_to: (*mx, *my) };
            let score = score_action(ai_unit, &action, analysis, influence, goals, predictions, map, config, game_data);
            all_possible.push((ai_unit.entity, action, score));
        }

        // Wait action
        let wait = AiAction::Wait;
        let score = score_action(ai_unit, &wait, analysis, influence, goals, predictions, map, config, game_data);
        all_possible.push((ai_unit.entity, wait, score));
    }

    // Sort by score
    all_possible.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Assign greedily
    for (unit_entity, action, priority) in all_possible {
        if assigned_units.contains(&unit_entity) {
            continue;
        }

        let move_pos = match &action {
            AiAction::Attack { move_to, .. } => Some(*move_to),
            AiAction::Capture { move_to, .. } => Some(*move_to),
            AiAction::Move { move_to } => Some(*move_to),
            AiAction::Wait => None,
        };

        if let Some(pos) = move_pos {
            if planned_positions.contains(&pos) {
                continue;
            }
            planned_positions.insert(pos);
        }

        assigned_units.insert(unit_entity);
        actions.push(PlannedAction {
            unit: unit_entity,
            action,
            priority,
        });
    }

    // Sort by priority for execution
    actions.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));

    actions
}

// ============================================================================
// SMART PRODUCTION
// ============================================================================

fn smart_production(
    config: &AiConfig,
    funds: &mut ResMut<FactionFunds>,
    faction: Faction,
    analysis: &GameAnalysis,
    tiles: &[(Entity, Tile)],
    commands: &mut Commands,
    map: &GameMap,
    sprite_param: &mut SpriteAssetsParam,
    goals: &[StrategicGoal],
    cost_modifier: f32,
) {
    let unit_positions: HashSet<(i32, i32)> = analysis.ai_units.iter()
        .map(|u| (u.pos.x, u.pos.y))
        .chain(analysis.enemy_units.iter().map(|u| (u.pos.x, u.pos.y)))
        .collect();

    let empty_bases: Vec<_> = tiles.iter()
        .filter(|(_, t)| {
            t.terrain == Terrain::Base
                && t.owner == Some(faction)
                && !unit_positions.contains(&(t.position.x, t.position.y))
        })
        .map(|(_, t)| (t.position.x, t.position.y))
        .collect();

    if empty_bases.is_empty() {
        return;
    }

    // Analyze needs
    let our_scouts = analysis.ai_units.iter().filter(|u| u.unit_type == UnitType::Scout).count();
    let _our_combat = analysis.ai_units.iter().filter(|u| u.unit_type != UnitType::Scout).count();
    let enemy_tanks = analysis.enemy_units.iter().filter(|u| u.unit_type == UnitType::Ironclad).count();
    let enemy_artillery = analysis.enemy_units.iter().filter(|u| u.is_indirect).count();

    let primary_goal = goals.first();

    let mut build_list: Vec<(UnitType, u32, f32)> = Vec::new();

    // === STRATEGY-BASED PRODUCTION ===
    match config.strategy {
        AiStrategy::Annihilation => {
            // Build heavy combat units
            build_list.push((UnitType::Ironclad, 70, 90.0));
            build_list.push((UnitType::Shocktrooper, 30, 80.0));
            build_list.push((UnitType::Siege, 60, 70.0));
            // Minimal scouts
            if our_scouts < 1 {
                build_list.push((UnitType::Scout, 10, 50.0));
            }
        }
        AiStrategy::Domination => {
            // Lots of scouts for capturing
            build_list.push((UnitType::Scout, 10, 95.0));
            build_list.push((UnitType::Scout, 10, 90.0)); // Want multiple scouts
            build_list.push((UnitType::Recon, 40, 60.0)); // Fast capture support
            build_list.push((UnitType::Shocktrooper, 30, 50.0));
        }
        AiStrategy::Blitz => {
            // Fast, mobile units
            build_list.push((UnitType::Recon, 40, 85.0));
            build_list.push((UnitType::Scout, 10, 80.0)); // For capturing HQ
            build_list.push((UnitType::Ironclad, 70, 70.0));
        }
        AiStrategy::Attrition => {
            // Balanced, cost-effective units
            build_list.push((UnitType::Shocktrooper, 30, 80.0));
            build_list.push((UnitType::Siege, 60, 75.0)); // Artillery for safe damage
            if our_scouts < 2 {
                build_list.push((UnitType::Scout, 10, 70.0));
            }
        }
        AiStrategy::Swarm => {
            // Cheap units, lots of them
            build_list.push((UnitType::Scout, 10, 95.0));
            build_list.push((UnitType::Scout, 10, 90.0));
            build_list.push((UnitType::Shocktrooper, 30, 85.0));
            build_list.push((UnitType::Shocktrooper, 30, 80.0));
        }
        AiStrategy::Fortress => {
            // Defensive units
            build_list.push((UnitType::Siege, 60, 85.0)); // Artillery
            build_list.push((UnitType::Ironclad, 70, 80.0)); // Tanks to hold ground
            build_list.push((UnitType::Shocktrooper, 30, 70.0));
            if our_scouts < 1 {
                build_list.push((UnitType::Scout, 10, 60.0));
            }
        }
        AiStrategy::Balanced => {
            // Standard production based on goals
            if our_scouts < 2 {
                build_list.push((UnitType::Scout, 10, 85.0));
            }

            match primary_goal {
                Some(StrategicGoal::Attack { .. }) => {
                    build_list.push((UnitType::Ironclad, 70, 70.0));
                    build_list.push((UnitType::Shocktrooper, 30, 65.0));
                }
                Some(StrategicGoal::Defend { .. }) => {
                    build_list.push((UnitType::Siege, 60, 70.0));
                    build_list.push((UnitType::Shocktrooper, 30, 65.0));
                }
                Some(StrategicGoal::Expand { .. }) => {
                    build_list.push((UnitType::Scout, 10, 90.0));
                    build_list.push((UnitType::Recon, 40, 60.0));
                }
                _ => {
                    build_list.push((UnitType::Shocktrooper, 30, 60.0));
                }
            }
        }
    }

    // Counter enemy composition (applies to all strategies)
    if enemy_tanks > 1 {
        build_list.push((UnitType::Shocktrooper, 30, 75.0 + enemy_tanks as f32 * 10.0));
    }
    if enemy_artillery > 1 {
        build_list.push((UnitType::Recon, 40, 70.0 + enemy_artillery as f32 * 15.0));
    }

    // Personality adjustments
    match config.personality {
        AiPersonality::Aggressive | AiPersonality::Reckless => {
            // Prefer offensive units
            for (unit_type, _, priority) in build_list.iter_mut() {
                if *unit_type == UnitType::Ironclad || *unit_type == UnitType::Shocktrooper {
                    *priority += 10.0;
                }
            }
        }
        AiPersonality::Cautious | AiPersonality::Methodical => {
            // Prefer defensive units
            for (unit_type, _, priority) in build_list.iter_mut() {
                if *unit_type == UnitType::Siege {
                    *priority += 10.0;
                }
            }
        }
    }

    build_list.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    for (x, y) in empty_bases {
        for (unit_type, base_cost, _) in &build_list {
            // Apply CO cost modifier
            let adjusted_cost = (*base_cost as f32 * cost_modifier).round() as u32;
            if funds.spend(faction, adjusted_cost) {
                spawn_unit(commands, map, &mut sprite_param.meshes, &mut sprite_param.materials, &sprite_param.assets, &sprite_param.images, faction, *unit_type, x, y);
                info!("AI ({:?}/{:?}) built {:?} at ({}, {})",
                    config.strategy, config.personality, unit_type, x, y);
                break;
            }
        }
    }
}

// ============================================================================
// AI CO POWER DECISION
// ============================================================================

/// Decide whether the AI should activate its CO power
fn should_ai_activate_power(
    commanders: &Commanders,
    units: &Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    tiles: &Query<(Entity, &Tile)>,
    faction: Faction,
) -> bool {
    use super::PowerEffect;

    let Some(co) = commanders.get_commander(faction) else {
        return false;
    };

    let ai_units: Vec<_> = units.iter()
        .filter(|(_, _, _, f, _)| f.faction == faction)
        .collect();

    let enemy_units: Vec<_> = units.iter()
        .filter(|(_, _, _, f, _)| f.faction != faction)
        .collect();

    let ai_unit_count = ai_units.len();
    let enemy_unit_count = enemy_units.len();

    // Don't waste power if we have no units
    if ai_unit_count == 0 {
        return false;
    }

    match &co.power.effect {
        PowerEffect::StatBoost { attack, defense, movement } => {
            // Use stat boost when we have units to benefit
            // Better when we have more units and enemies nearby
            let has_good_army = ai_unit_count >= 2;
            let has_enemies = enemy_unit_count > 0;
            let boost_significant = *attack > 1.1 || *defense > 1.1 || *movement > 0;
            has_good_army && has_enemies && boost_significant
        }

        PowerEffect::BonusFunds { multiplier: _ } => {
            // Use Gold Rush when we have decent funds to multiply
            // Don't use when nearly broke (waste) or when swimming in cash (overkill)
            // Best used mid-game when economy is established
            let owned_bases = tiles.iter()
                .filter(|(_, t)| t.terrain == Terrain::Base && t.owner == Some(faction))
                .count();
            owned_bases >= 1 // Use if we have at least one base
        }

        PowerEffect::RevealAndBoost { attack_boost: _ } => {
            // Fog Piercer - always useful if there are enemies
            enemy_unit_count > 0
        }

        PowerEffect::DefenseAndHeal { defense: _, heal } => {
            // Iron Wall - use when units are damaged
            let damaged_units = ai_units.iter()
                .filter(|(_, _, _, _, u)| {
                    let max_hp = u.unit_type.stats().max_hp;
                    u.hp < max_hp - *heal // Would benefit from heal
                })
                .count();
            damaged_units >= 2 || (damaged_units >= 1 && ai_unit_count <= 2)
        }

        PowerEffect::FreeUnits { unit_type: _ } => {
            // Endless Horde - use when we have empty bases
            let empty_bases = tiles.iter()
                .filter(|(_, t)| t.terrain == Terrain::Base && t.owner == Some(faction))
                .filter(|(_, t)| {
                    !units.iter().any(|(_, p, _, _, _)| p.x == t.position.x && p.y == t.position.y)
                })
                .count();
            empty_bases >= 1
        }

        PowerEffect::ExtraMove => {
            // Charge! - use when we have unmoved units that can attack
            // Best used when enemies are in range
            let can_attack_count = ai_units.iter()
                .filter(|(_, _, _, _, u)| !u.moved && u.unit_type.stats().attack > 0)
                .count();
            can_attack_count >= 2 && enemy_unit_count > 0
        }

        PowerEffect::StealFunds { steal_percent: _, attack_boost: _ } => {
            // Heist - use when enemies have units (implies they have funds)
            // and we have units to benefit from attack boost
            ai_unit_count >= 1 && enemy_unit_count > 0
        }

        PowerEffect::IgnoreTerrain => {
            // Undermine - use when we have units on difficult terrain
            // or when we need to cross difficult terrain to reach enemies
            ai_unit_count >= 2 && enemy_unit_count > 0
        }
    }
}

// ============================================================================
// AI TURN SYSTEM
// ============================================================================

fn ai_turn_system(
    mut ai_res: AiResources,
    mut turn_state: ResMut<TurnState>,
    mut funds: ResMut<FactionFunds>,
    time: Res<Time>,
    mut commands: Commands,
    map: Res<GameMap>,
    mut units: Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    tiles: Query<(Entity, &Tile)>,
    mut attack_events: MessageWriter<AttackEvent>,
    mut capture_events: MessageWriter<CaptureEvent>,
    game_result: Res<GameResult>,
    mut commanders: ResMut<Commanders>,
    mut power_events: MessageWriter<PowerActivatedEvent>,
    mut sprite_param: SpriteAssetsParam,
) {
    if game_result.game_over {
        return;
    }

    if !ai_res.ai_state.enabled || turn_state.current_faction != Faction::Northern {
        ai_res.ai_state.phase = AiTurnPhase::Waiting;
        return;
    }

    if turn_state.phase == TurnPhase::Action {
        return;
    }

    ai_res.ai_state.action_delay.tick(time.delta());
    if !ai_res.ai_state.action_delay.finished() {
        return;
    }

    // Get AI config from CO personality
    let co = commanders.get_active(Faction::Northern).get_commander();
    let config = AiConfig {
        personality: co.personality,
        strategy: ai_res.ai_state.config.strategy, // Keep strategy from state
    };

    match ai_res.ai_state.phase {
        AiTurnPhase::Waiting => {
            // Update memory with player positions from last turn
            update_memory(&mut ai_res.memory, &units);
            ai_res.memory.turn_count += 1;
            ai_res.ai_state.phase = AiTurnPhase::Planning;

            // Log AI configuration at start of turn
            info!("AI Turn ({}) - Strategy: {:?}, Personality: {:?}",
                co.name, config.strategy, config.personality);
        }

        AiTurnPhase::Planning => {
            // Check if AI should activate CO power
            if commanders.can_activate(Faction::Northern) {
                let should_activate = should_ai_activate_power(
                    &commanders,
                    &units,
                    &tiles,
                    Faction::Northern,
                );

                if should_activate {
                    if let Some(effect) = commanders.activate_power(Faction::Northern) {
                        info!("AI activated CO Power: {}!", co.power.name);
                        power_events.write(PowerActivatedEvent {
                            faction: Faction::Northern,
                            effect,
                        });
                    }
                }
            }

            let all_units: Vec<_> = units.iter()
                .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.clone()))
                .collect();

            let all_tiles: Vec<_> = tiles.iter()
                .map(|(e, t)| (e, t.clone()))
                .collect();

            // Full analysis pipeline
            let analysis = analyze_game_state(&all_units, &all_tiles, Faction::Northern);
            let influence = build_influence_maps(&analysis, &map, &all_tiles, &ai_res.game_data);
            let goals = determine_strategic_goals(&analysis, &influence, &ai_res.memory, &config);
            let predictions = predict_enemy_actions(&analysis, &influence, &ai_res.memory, &map, &ai_res.game_data);

            info!("AI Strategic Goals: {:?}", goals.iter().take(2).collect::<Vec<_>>());

            // Plan with all systems
            let actions = plan_turn_advanced(&analysis, &influence, &goals, &predictions, &map, &config, &ai_res.game_data);

            ai_res.turn_plan.actions = actions;
            ai_res.turn_plan.current_index = 0;

            ai_res.ai_state.phase = AiTurnPhase::ExecutingActions;
            ai_res.ai_state.action_delay.reset();
        }

        AiTurnPhase::ExecutingActions => {
            if ai_res.turn_plan.current_index >= ai_res.turn_plan.actions.len() {
                ai_res.ai_state.phase = AiTurnPhase::Production;
                ai_res.ai_state.action_delay.reset();
                return;
            }

            let planned = &ai_res.turn_plan.actions[ai_res.turn_plan.current_index];
            execute_action(
                &mut commands,
                planned.action.clone(),
                planned.unit,
                &mut units,
                &map,
                &mut attack_events,
                &mut capture_events,
            );

            ai_res.turn_plan.current_index += 1;
            ai_res.ai_state.action_delay.reset();
        }

        AiTurnPhase::Production => {
            let all_units: Vec<_> = units.iter()
                .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.clone()))
                .collect();

            let all_tiles: Vec<_> = tiles.iter()
                .map(|(e, t)| (e, t.clone()))
                .collect();

            let analysis = analyze_game_state(&all_units, &all_tiles, Faction::Northern);
            let influence = build_influence_maps(&analysis, &map, &all_tiles, &ai_res.game_data);
            let goals = determine_strategic_goals(&analysis, &influence, &ai_res.memory, &config);

            let co_bonuses = commanders.get_bonuses(Faction::Northern);
            smart_production(
                &config,
                &mut funds,
                Faction::Northern,
                &analysis,
                &all_tiles,
                &mut commands,
                &map,
                &mut sprite_param,
                &goals,
                co_bonuses.cost,
            );

            ai_res.ai_state.phase = AiTurnPhase::EndingTurn;
            ai_res.ai_state.action_delay.reset();
        }

        AiTurnPhase::EndingTurn => {
            for (_, _, _, faction, mut unit) in units.iter_mut() {
                if faction.faction == Faction::Northern {
                    unit.moved = false;
                    unit.attacked = false;
                    unit.exhausted = false;
                }
            }

            turn_state.current_faction = Faction::Eastern;
            turn_state.turn_number += 1;
            turn_state.phase = TurnPhase::Select;
            ai_res.ai_state.phase = AiTurnPhase::Waiting;

            info!("AI ended turn {}", turn_state.turn_number - 1);
        }
    }
}

fn update_memory(
    memory: &mut AiMemory,
    units: &Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
) {
    // Track player unit movements for aggression calculation
    let attacks_detected = 0; // TODO: Track actual attacks in future
    let mut total_moves = 0;

    for (entity, pos, _, faction, _) in units.iter() {
        if faction.faction == Faction::Eastern {
            if let Some(&(old_x, old_y)) = memory.player_last_positions.get(&entity) {
                let moved = (pos.x != old_x) || (pos.y != old_y);
                if moved {
                    total_moves += 1;
                    // If they moved closer to our units, that's aggressive
                    // (simplified - just count moves for now)
                }
            }
            memory.player_last_positions.insert(entity, (pos.x, pos.y));
        }
    }

    // Update aggression estimate (exponential moving average)
    if total_moves > 0 {
        let new_aggression = attacks_detected as f32 / total_moves as f32;
        memory.player_aggression = memory.player_aggression * 0.7 + new_aggression * 0.3;
    }
}

fn execute_action(
    commands: &mut Commands,
    action: AiAction,
    entity: Entity,
    units: &mut Query<(Entity, &mut GridPosition, &Transform, &FactionMember, &mut Unit)>,
    map: &GameMap,
    attack_events: &mut MessageWriter<AttackEvent>,
    capture_events: &mut MessageWriter<CaptureEvent>,
) {
    match action {
        AiAction::Attack { move_to, target } => {
            if let Ok((_, mut pos, transform, _, mut unit)) = units.get_mut(entity) {
                info!("AI: {:?} ({},{}) -> ({},{}) attacks",
                    unit.unit_type, pos.x, pos.y, move_to.0, move_to.1);
                let start_pos = transform.translation;
                pos.x = move_to.0;
                pos.y = move_to.1;
                // Calculate end position (preserve Y height)
                let new_world_pos = pos.to_world(map);
                let end_pos = Vec3::new(new_world_pos.x, start_pos.y, new_world_pos.z);
                // Add animation component for smooth movement
                commands.entity(entity).insert(UnitAnimation::new(start_pos, end_pos));
                unit.moved = true;
                unit.attacked = true;
                unit.exhausted = true;
            }
            attack_events.write(AttackEvent { attacker: entity, defender: target });
        }
        AiAction::Capture { move_to, tile } => {
            if let Ok((_, mut pos, transform, _, mut unit)) = units.get_mut(entity) {
                info!("AI: {:?} captures at ({},{})", unit.unit_type, move_to.0, move_to.1);
                let start_pos = transform.translation;
                pos.x = move_to.0;
                pos.y = move_to.1;
                // Calculate end position (preserve Y height)
                let new_world_pos = pos.to_world(map);
                let end_pos = Vec3::new(new_world_pos.x, start_pos.y, new_world_pos.z);
                // Add animation component for smooth movement
                commands.entity(entity).insert(UnitAnimation::new(start_pos, end_pos));
                unit.moved = true;
                unit.attacked = true;
                unit.exhausted = true;
            }
            capture_events.write(CaptureEvent { unit: entity, tile });
        }
        AiAction::Move { move_to } => {
            if let Ok((_, mut pos, transform, _, mut unit)) = units.get_mut(entity) {
                info!("AI: {:?} ({},{}) -> ({},{})",
                    unit.unit_type, pos.x, pos.y, move_to.0, move_to.1);
                let start_pos = transform.translation;
                pos.x = move_to.0;
                pos.y = move_to.1;
                // Calculate end position (preserve Y height)
                let new_world_pos = pos.to_world(map);
                let end_pos = Vec3::new(new_world_pos.x, start_pos.y, new_world_pos.z);
                // Add animation component for smooth movement
                commands.entity(entity).insert(UnitAnimation::new(start_pos, end_pos));
                unit.moved = true;
                unit.exhausted = true;
            }
        }
        AiAction::Wait => {
            if let Ok((_, _, _, _, mut unit)) = units.get_mut(entity) {
                info!("AI: {:?} waits", unit.unit_type);
                unit.exhausted = true;
            }
        }
    }
}
