use bevy::prelude::*;
use std::collections::{HashMap, HashSet};

use super::{
    Faction, FactionMember, Unit, UnitType, GridPosition, GameMap, Tile, Terrain,
    TurnState, TurnPhase, FactionFunds, AttackEvent, CaptureEvent, GameResult,
    calculate_movement_range, calculate_damage, spawn_unit, CoBonuses,
    Commanders, PowerActivatedEvent,
};

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

/// AI personality types - affects strategic decisions
/// Future: Used for CO (Commanding Officer) system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(dead_code)]
pub enum AiPersonality {
    #[default]
    Aggressive,  // Attack priority, risk-taking
    Defensive,   // Hold ground, protect bases
    Economic,    // Capture priority, income focus
    Swarm,       // Mass cheap units
}

/// AI commander configuration - for future CO system
#[derive(Resource)]
#[allow(dead_code)]
pub struct AiCommander {
    pub faction: Faction,
    pub personality: AiPersonality,
    pub name: String,
}

impl Default for AiCommander {
    fn default() -> Self {
        Self {
            faction: Faction::Northern,
            personality: AiPersonality::Aggressive,
            name: "Commander Blue".to_string(),
        }
    }
}

#[derive(Resource)]
pub struct AiState {
    pub enabled: bool,
    pub action_delay: Timer,
    pub phase: AiTurnPhase,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            enabled: true,
            action_delay: Timer::from_seconds(0.2, TimerMode::Once),
            phase: AiTurnPhase::Waiting,
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

        // Calculate all positions this enemy could attack from
        let moves = calculate_movement_range(
            &enemy.pos,
            stats.movement,
            map,
            &analysis.unit_positions,
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
    personality: AiPersonality,
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

    // Count damaged units
    let damaged_units = analysis.ai_units.iter()
        .filter(|u| u.hp_percent < 0.5)
        .count();

    let total_units = analysis.ai_units.len();

    // === ATTACK GOAL ===
    let mut attack_priority: f32 = 0.0;

    // Strong advantage -> attack
    if strength_ratio > 1.3 {
        attack_priority += 40.0;
    } else if strength_ratio > 1.0 {
        attack_priority += 20.0;
    }

    // Personality modifier
    attack_priority += match personality {
        AiPersonality::Aggressive => 30.0,
        AiPersonality::Defensive => -20.0,
        AiPersonality::Economic => 0.0,
        AiPersonality::Swarm => 15.0,
    };

    // If player is passive, be more aggressive
    if memory.player_aggression < 0.3 {
        attack_priority += 15.0;
    }

    goals.push(StrategicGoal::Attack { priority: attack_priority.max(0.0) });

    // === DEFEND GOAL ===
    let mut defend_priority = 0.0;

    // Weak position -> defend
    if strength_ratio < 0.8 {
        defend_priority += 40.0;
    }

    // Have valuable properties to defend
    defend_priority += our_income * 5.0;

    // Personality
    defend_priority += match personality {
        AiPersonality::Aggressive => -15.0,
        AiPersonality::Defensive => 35.0,
        AiPersonality::Economic => 10.0,
        AiPersonality::Swarm => 0.0,
    };

    // If player is aggressive, defend more
    if memory.player_aggression > 0.7 {
        defend_priority += 20.0;
    }

    goals.push(StrategicGoal::Defend { priority: defend_priority.max(0.0) });

    // === EXPAND GOAL ===
    let mut expand_priority = 0.0;

    // Neutral properties available
    expand_priority += neutral_properties * 10.0;

    // Economic disadvantage -> expand
    if enemy_income > our_income {
        expand_priority += (enemy_income - our_income) * 15.0;
    }

    // Early game -> expand
    if memory.turn_count < 5 {
        expand_priority += 25.0;
    }

    // Personality
    expand_priority += match personality {
        AiPersonality::Aggressive => -10.0,
        AiPersonality::Defensive => 5.0,
        AiPersonality::Economic => 40.0,
        AiPersonality::Swarm => 20.0,
    };

    goals.push(StrategicGoal::Expand { priority: expand_priority.max(0.0) });

    // === CONSOLIDATE GOAL ===
    let mut consolidate_priority: f32 = 0.0;

    // Many damaged units -> consolidate
    if total_units > 0 && damaged_units as f32 / total_units as f32 > 0.4 {
        consolidate_priority += 35.0;
    }

    // Significant disadvantage -> consolidate
    if strength_ratio < 0.6 {
        consolidate_priority += 30.0;
    }

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
            // Kill! Very high utility
            target_value as f32 * 2.5
        } else if damage_ratio > 0.7 {
            // Nearly kill - good but not as good as kill
            target_value as f32 * 1.5 * damage_ratio
        } else if damage_ratio > 0.4 {
            // Significant damage
            target_value as f32 * damage_ratio
        } else {
            // Minor damage - diminishing returns
            target_value as f32 * damage_ratio * 0.5
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
    fn position_safety(threat: f32, our_hp: i32) -> f32 {
        let danger_ratio = threat / our_hp as f32;

        if danger_ratio > 1.5 {
            -50.0  // Extremely dangerous
        } else if danger_ratio > 1.0 {
            -30.0  // Likely to die
        } else if danger_ratio > 0.5 {
            -15.0 * danger_ratio  // Risky
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
) -> Vec<PredictedAction> {
    let mut predictions = Vec::new();

    for enemy in &analysis.enemy_units {
        let stats = enemy.unit.unit_type.stats();

        // Calculate possible moves
        let moves = calculate_movement_range(
            &enemy.pos,
            stats.movement,
            map,
            &analysis.unit_positions,
        );

        // Find best predicted action for this enemy
        let mut best_score = f32::NEG_INFINITY;
        let mut best_action = PredictedAction {
            unit: enemy.entity,
            likely_position: (enemy.pos.x, enemy.pos.y),
            likely_target: None,
            confidence: 0.5,
        };

        for (mx, my) in &moves {
            let mut score = 0.0;

            // Check if they can attack one of our units from here
            if stats.attack > 0 {
                for ai_unit in &analysis.ai_units {
                    let dist = ((mx - ai_unit.pos.x).abs() + (my - ai_unit.pos.y).abs()) as u32;
                    let (min_r, max_r) = stats.attack_range;

                    if dist >= min_r && dist <= max_r {
                        // They can attack us! (use neutral bonuses for prediction)
                        let no_bonus = CoBonuses::none();
                        let damage = calculate_damage(&enemy.unit, &ai_unit.unit,
                            map.get(ai_unit.pos.x, ai_unit.pos.y).unwrap_or(Terrain::Grass),
                            &no_bonus, &no_bonus);

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
) -> f32 {
    let primary_goal = goals.first().cloned().unwrap_or(StrategicGoal::Attack { priority: 50.0 });

    match action {
        AiAction::Attack { move_to, target } => {
            score_attack_action(unit, *move_to, *target, analysis, influence, &primary_goal, predictions, map)
        }
        AiAction::Capture { move_to, tile } => {
            score_capture_action(unit, *move_to, *tile, analysis, influence, &primary_goal, predictions, map)
        }
        AiAction::Move { move_to } => {
            score_move_action(unit, *move_to, analysis, influence, &primary_goal, predictions, map)
        }
        AiAction::Wait => {
            score_wait_action(unit, influence)
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
) -> f32 {
    let target_unit = match analysis.enemy_units.iter().find(|u| u.entity == target) {
        Some(u) => u,
        None => return -1000.0,
    };

    // Use neutral bonuses for AI prediction (actual combat uses real CO bonuses)
    let no_bonus = CoBonuses::none();

    let defender_terrain = map.get(target_unit.pos.x, target_unit.pos.y).unwrap_or(Terrain::Grass);
    let damage = calculate_damage(&attacker.unit, &target_unit.unit, defender_terrain, &no_bonus, &no_bonus);

    // === BASE DAMAGE UTILITY ===
    let mut score = UtilityCurves::damage_utility(damage, target_unit.unit.hp, target_unit.value);

    // === COUNTER-ATTACK RISK ===
    let target_stats = target_unit.unit.unit_type.stats();
    if target_stats.attack > 0 && target_stats.attack_range.0 == 1 && target_unit.unit.hp > damage {
        let attacker_terrain = map.get(move_to.0, move_to.1).unwrap_or(Terrain::Grass);
        let mut temp_target = target_unit.unit.clone();
        temp_target.hp -= damage;
        let counter = calculate_damage(&temp_target, &attacker.unit, attacker_terrain, &no_bonus, &no_bonus);
        score += UtilityCurves::risk_utility(counter, attacker.unit.hp, attacker.value);
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
    let threat_after = influence.get_threat(move_to.0, move_to.1);
    score += UtilityCurves::position_safety(threat_after, attacker.unit.hp - if damage < target_unit.unit.hp {
        // Estimate we might take counter damage
        let target_stats = target_unit.unit.unit_type.stats();
        if target_stats.attack > 0 && target_stats.attack_range.0 == 1 {
            (target_stats.attack as f32 * 0.3) as i32
        } else { 0 }
    } else { 0 });

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

    // === ENEMY BASE = VICTORY CONDITION ===
    if is_base && tile.owner.is_some() && tile.owner != Some(Faction::Northern) {
        score += 150.0; // Capturing enemy base is huge
    }

    // === RISK ASSESSMENT ===
    let threat = influence.get_threat(move_to.0, move_to.1);
    score += UtilityCurves::position_safety(threat, unit.unit.hp);

    // If enemies are predicted to attack this position, risky
    for pred in predictions {
        if pred.likely_position == move_to {
            score -= 30.0 * pred.confidence;
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
    // Capturing behind enemy lines is risky
    if influence.get_territory(move_to.0, move_to.1) < -0.3 {
        score -= 20.0;
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
) -> f32 {
    let mut score = 0.0;

    let terrain = map.get(move_to.0, move_to.1).unwrap_or(Terrain::Grass);
    let defense_bonus = terrain.defense_bonus();

    // === TERRAIN DEFENSE ===
    score += defense_bonus as f32 * 4.0;

    // === THREAT AVOIDANCE ===
    let threat = influence.get_threat(move_to.0, move_to.1);
    score += UtilityCurves::position_safety(threat, unit.unit.hp);

    // === SUPPORT FROM ALLIES ===
    // Note: support map value available via influence.get_support() for future use
    let nearby_allies = analysis.ai_units.iter()
        .filter(|u| {
            let dist = (u.pos.x - move_to.0).abs() + (u.pos.y - move_to.1).abs();
            dist <= 3 && u.entity != unit.entity
        })
        .count();
    score += UtilityCurves::support_utility(nearby_allies);

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
            // Move toward safe positions
            let retreat = influence.retreat_value.get(&move_to).copied().unwrap_or(0.0);
            score += retreat * 0.5 * (priority / 100.0);

            // Move away from enemies
            let dist_to_enemy = analysis.enemy_units.iter()
                .map(|e| ((move_to.0 - e.pos.x).abs() + (move_to.1 - e.pos.y).abs()) as f32)
                .min_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(100.0);
            score += dist_to_enemy * 0.5 * (priority / 100.0);
        }
    }

    // === STRATEGIC VALUE OF POSITION ===
    score += influence.get_strategic(move_to.0, move_to.1) * 0.2;

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
        score += UtilityCurves::position_safety(threat, unit.unit.hp) * 0.5;
    }

    score
}

fn score_wait_action(unit: &UnitInfo, influence: &InfluenceMaps) -> f32 {
    // Waiting is usually bad, but okay if in a good defensive position
    let threat = influence.get_threat(unit.pos.x, unit.pos.y);
    let support = influence.get_support(unit.pos.x, unit.pos.y);

    if threat < 10.0 && support > 20.0 {
        0.0 // Okay to wait in safe supported position
    } else {
        -20.0 // Generally want to do something
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
        let moves = calculate_movement_range(
            &ai_unit.pos,
            stats.movement,
            map,
            &analysis.unit_positions,
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
                        let score = score_action(ai_unit, &action, analysis, influence, goals, predictions, map);
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
                        let score = score_action(ai_unit, &action, analysis, influence, goals, predictions, map);
                        all_possible.push((ai_unit.entity, action, score));
                    }
                }
            }

            // Evaluate moves
            let action = AiAction::Move { move_to: (*mx, *my) };
            let score = score_action(ai_unit, &action, analysis, influence, goals, predictions, map);
            all_possible.push((ai_unit.entity, action, score));
        }

        // Wait action
        let wait = AiAction::Wait;
        let score = score_action(ai_unit, &wait, analysis, influence, goals, predictions, map);
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
    personality: AiPersonality,
    funds: &mut ResMut<FactionFunds>,
    faction: Faction,
    analysis: &GameAnalysis,
    tiles: &[(Entity, Tile)],
    commands: &mut Commands,
    map: &GameMap,
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
    let our_combat = analysis.ai_units.iter().filter(|u| u.unit_type != UnitType::Scout).count();
    let enemy_tanks = analysis.enemy_units.iter().filter(|u| u.unit_type == UnitType::Ironclad).count();
    let enemy_artillery = analysis.enemy_units.iter().filter(|u| u.is_indirect).count();

    let primary_goal = goals.first();

    let mut build_list: Vec<(UnitType, u32, f32)> = Vec::new();

    // Need scouts for capturing
    if our_scouts < 2 {
        build_list.push((UnitType::Scout, 10, 85.0));
    }

    // Counter enemy composition
    if enemy_tanks > 0 {
        build_list.push((UnitType::Shocktrooper, 30, 70.0 + enemy_tanks as f32 * 15.0));
    }
    if enemy_artillery > 0 {
        build_list.push((UnitType::Recon, 40, 65.0 + enemy_artillery as f32 * 20.0));
    }

    // Goal-based production
    match primary_goal {
        Some(StrategicGoal::Attack { .. }) => {
            build_list.push((UnitType::Ironclad, 70, 60.0));
            build_list.push((UnitType::Shocktrooper, 30, 55.0));
        }
        Some(StrategicGoal::Defend { .. }) => {
            build_list.push((UnitType::Siege, 60, 65.0));
            build_list.push((UnitType::Shocktrooper, 30, 60.0));
        }
        Some(StrategicGoal::Expand { .. }) => {
            build_list.push((UnitType::Scout, 10, 90.0));
            build_list.push((UnitType::Recon, 40, 50.0));
        }
        Some(StrategicGoal::Consolidate { .. }) => {
            // Save money during consolidation
            if our_combat < 2 {
                build_list.push((UnitType::Shocktrooper, 30, 50.0));
            }
        }
        None => {
            build_list.push((UnitType::Shocktrooper, 30, 50.0));
        }
    }

    // Personality adjustments
    match personality {
        AiPersonality::Aggressive => {
            build_list.push((UnitType::Ironclad, 70, 55.0));
        }
        AiPersonality::Defensive => {
            build_list.push((UnitType::Siege, 60, 55.0));
        }
        AiPersonality::Economic => {
            build_list.push((UnitType::Scout, 10, 80.0));
        }
        AiPersonality::Swarm => {
            build_list.push((UnitType::Scout, 10, 75.0));
            build_list.push((UnitType::Shocktrooper, 30, 45.0));
        }
    }

    build_list.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    for (x, y) in empty_bases {
        for (unit_type, base_cost, _) in &build_list {
            // Apply CO cost modifier
            let adjusted_cost = (*base_cost as f32 * cost_modifier).round() as u32;
            if funds.spend(faction, adjusted_cost) {
                spawn_unit(commands, map, faction, *unit_type, x, y);
                info!("AI built {:?} at ({}, {})", unit_type, x, y);
                break;
            }
        }
    }
}

// ============================================================================
// AI TURN SYSTEM
// ============================================================================

fn ai_turn_system(
    mut ai_state: ResMut<AiState>,
    mut turn_plan: ResMut<AiTurnPlan>,
    mut memory: ResMut<AiMemory>,
    mut turn_state: ResMut<TurnState>,
    mut funds: ResMut<FactionFunds>,
    time: Res<Time>,
    mut commands: Commands,
    map: Res<GameMap>,
    mut units: Query<(Entity, &mut GridPosition, &mut Transform, &FactionMember, &mut Unit)>,
    tiles: Query<(Entity, &Tile)>,
    mut attack_events: EventWriter<AttackEvent>,
    mut capture_events: EventWriter<CaptureEvent>,
    game_result: Res<GameResult>,
    mut commanders: ResMut<Commanders>,
    mut power_events: EventWriter<PowerActivatedEvent>,
) {
    if game_result.game_over {
        return;
    }

    if !ai_state.enabled || turn_state.current_faction != Faction::Northern {
        ai_state.phase = AiTurnPhase::Waiting;
        return;
    }

    if turn_state.phase == TurnPhase::Action {
        return;
    }

    ai_state.action_delay.tick(time.delta());
    if !ai_state.action_delay.finished() {
        return;
    }

    let personality = AiPersonality::Aggressive;

    match ai_state.phase {
        AiTurnPhase::Waiting => {
            // Update memory with player positions from last turn
            update_memory(&mut memory, &units);
            memory.turn_count += 1;
            ai_state.phase = AiTurnPhase::Planning;
        }

        AiTurnPhase::Planning => {
            // Check if AI can activate CO power
            if commanders.can_activate(Faction::Northern) {
                // AI decision: activate power when it has multiple units
                let ai_unit_count = units.iter()
                    .filter(|(_, _, _, f, _)| f.faction == Faction::Northern)
                    .count();

                // Activate power if we have at least 2 units (power will benefit them)
                if ai_unit_count >= 2 {
                    if let Some(effect) = commanders.activate_power(Faction::Northern) {
                        let co = commanders.get_active(Faction::Northern).get_commander();
                        info!("AI activated CO Power: {}!", co.power.name);
                        power_events.send(PowerActivatedEvent {
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
            let influence = build_influence_maps(&analysis, &map, &all_tiles);
            let goals = determine_strategic_goals(&analysis, &influence, &memory, personality);
            let predictions = predict_enemy_actions(&analysis, &influence, &memory, &map);

            info!("AI Strategic Goals: {:?}", goals.iter().take(2).collect::<Vec<_>>());

            // Plan with all systems
            let actions = plan_turn_advanced(&analysis, &influence, &goals, &predictions, &map);

            turn_plan.actions = actions;
            turn_plan.current_index = 0;

            ai_state.phase = AiTurnPhase::ExecutingActions;
            ai_state.action_delay.reset();
        }

        AiTurnPhase::ExecutingActions => {
            if turn_plan.current_index >= turn_plan.actions.len() {
                ai_state.phase = AiTurnPhase::Production;
                ai_state.action_delay.reset();
                return;
            }

            let planned = &turn_plan.actions[turn_plan.current_index];
            execute_action(
                planned.action.clone(),
                planned.unit,
                &mut units,
                &map,
                &mut attack_events,
                &mut capture_events,
            );

            turn_plan.current_index += 1;
            ai_state.action_delay.reset();
        }

        AiTurnPhase::Production => {
            let all_units: Vec<_> = units.iter()
                .map(|(e, p, _, f, u)| (e, p.clone(), f.clone(), u.clone()))
                .collect();

            let all_tiles: Vec<_> = tiles.iter()
                .map(|(e, t)| (e, t.clone()))
                .collect();

            let analysis = analyze_game_state(&all_units, &all_tiles, Faction::Northern);
            let influence = build_influence_maps(&analysis, &map, &all_tiles);
            let goals = determine_strategic_goals(&analysis, &influence, &memory, personality);

            let co_bonuses = commanders.get_bonuses(Faction::Northern);
            smart_production(
                personality,
                &mut funds,
                Faction::Northern,
                &analysis,
                &all_tiles,
                &mut commands,
                &map,
                &goals,
                co_bonuses.cost,
            );

            ai_state.phase = AiTurnPhase::EndingTurn;
            ai_state.action_delay.reset();
        }

        AiTurnPhase::EndingTurn => {
            for (_, _, _, faction, mut unit) in units.iter_mut() {
                if faction.faction == Faction::Northern {
                    unit.moved = false;
                    unit.attacked = false;
                }
            }

            turn_state.current_faction = Faction::Eastern;
            turn_state.turn_number += 1;
            turn_state.phase = TurnPhase::Select;
            ai_state.phase = AiTurnPhase::Waiting;

            info!("AI ended turn {}", turn_state.turn_number - 1);
        }
    }
}

fn update_memory(
    memory: &mut AiMemory,
    units: &Query<(Entity, &mut GridPosition, &mut Transform, &FactionMember, &mut Unit)>,
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
    action: AiAction,
    entity: Entity,
    units: &mut Query<(Entity, &mut GridPosition, &mut Transform, &FactionMember, &mut Unit)>,
    map: &GameMap,
    attack_events: &mut EventWriter<AttackEvent>,
    capture_events: &mut EventWriter<CaptureEvent>,
) {
    match action {
        AiAction::Attack { move_to, target } => {
            if let Ok((_, mut pos, mut transform, _, mut unit)) = units.get_mut(entity) {
                info!("AI: {:?} ({},{}) -> ({},{}) attacks",
                    unit.unit_type, pos.x, pos.y, move_to.0, move_to.1);
                pos.x = move_to.0;
                pos.y = move_to.1;
                transform.translation = pos.to_world(map);
                unit.moved = true;
                unit.attacked = true;
            }
            attack_events.send(AttackEvent { attacker: entity, defender: target });
        }
        AiAction::Capture { move_to, tile } => {
            if let Ok((_, mut pos, mut transform, _, mut unit)) = units.get_mut(entity) {
                info!("AI: {:?} captures at ({},{})", unit.unit_type, move_to.0, move_to.1);
                pos.x = move_to.0;
                pos.y = move_to.1;
                transform.translation = pos.to_world(map);
                unit.moved = true;
                unit.attacked = true;
            }
            capture_events.send(CaptureEvent { unit: entity, tile });
        }
        AiAction::Move { move_to } => {
            if let Ok((_, mut pos, mut transform, _, mut unit)) = units.get_mut(entity) {
                info!("AI: {:?} ({},{}) -> ({},{})",
                    unit.unit_type, pos.x, pos.y, move_to.0, move_to.1);
                pos.x = move_to.0;
                pos.y = move_to.1;
                transform.translation = pos.to_world(map);
                unit.moved = true;
            }
        }
        AiAction::Wait => {
            if let Ok((_, _, _, _, mut unit)) = units.get_mut(entity) {
                info!("AI: {:?} waits", unit.unit_type);
                unit.moved = true;
            }
        }
    }
}
