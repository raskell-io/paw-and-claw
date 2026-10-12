//! Advance Wars battle scenes: each attack is replayed as a short side-view
//! clash after it resolves. Combat (`combat::process_attacks`) does the maths
//! at once and sends a `BattleReport`: both units, the ground they stand on,
//! their starting HP and every strike in order. The reports queue here and
//! play one at a time; while one plays the board and the AI wait. The UI
//! draws the scene from `BattleScene::current` (`ui/battle_scene.rs`).
//!
//! The player picks Full, Quick or Off (map menu Options); any button skips.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use super::{Faction, PlaySfx, Terrain, UnitAnimation, UnitType};

pub struct BattleScenePlugin;

impl Plugin for BattleScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<BattleReport>()
            .init_resource::<BattleScene>()
            .init_resource::<DisplaySettings>()
            .add_systems(Update, (queue_reports, tick_battle_scene).chain())
            .add_systems(Update, save_display_settings);
    }
}

/// One side of a battle as it started
#[derive(Clone, Debug)]
pub struct Combatant {
    pub faction: Faction,
    pub unit_type: UnitType,
    pub terrain: Terrain,
    /// Internal HP (0-100) before the battle
    pub hp: i32,
}

/// One blow: who struck, the target's HP afterwards, and how it was dealt
#[derive(Clone, Debug)]
pub struct Strike {
    pub by_attacker: bool,
    pub target_hp_after: i32,
    /// Fired from range (volley or siege shot) rather than in melee
    pub ranged: bool,
}

/// A resolved attack, for the battle scene to replay
#[derive(Message, Clone, Debug)]
pub struct BattleReport {
    pub attacker: Combatant,
    pub defender: Combatant,
    pub strikes: Vec<Strike>,
}

/// How battles are shown
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BattleAnimation {
    #[default]
    Full,
    /// Twice as fast
    Quick,
    Off,
}

impl BattleAnimation {
    pub fn name(self) -> &'static str {
        match self {
            BattleAnimation::Full => "Full",
            BattleAnimation::Quick => "Quick",
            BattleAnimation::Off => "Off",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            BattleAnimation::Full => BattleAnimation::Quick,
            BattleAnimation::Quick => BattleAnimation::Off,
            BattleAnimation::Off => BattleAnimation::Full,
        }
    }

    fn speed(self) -> f32 {
        if self == BattleAnimation::Quick { 2.0 } else { 1.0 }
    }
}

/// Display options, saved with the other settings
#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub battle: BattleAnimation,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        super::read_setting("display")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or(Self { battle: BattleAnimation::Full })
    }
}

fn save_display_settings(settings: Res<DisplaySettings>) {
    if settings.is_changed() && !settings.is_added() {
        if let Ok(json) = serde_json::to_string(&*settings) {
            let _ = super::write_setting("display", &json);
        }
    }
}

// ============================================================================
// TIMING (seconds at Full speed)
// ============================================================================

/// The two sides slide in
pub const INTRO: f32 = 0.5;
/// One strike: wind-up, impact (HP counts down), a beat after
pub const WINDUP: f32 = 0.4;
pub const IMPACT: f32 = 0.6;
pub const BEAT: f32 = 0.2;
pub const STRIKE: f32 = WINDUP + IMPACT + BEAT;
/// The scene fades out
pub const OUTRO: f32 = 0.5;

impl BattleReport {
    pub fn duration(&self) -> f32 {
        INTRO + self.strikes.len() as f32 * STRIKE + OUTRO
    }

    /// Each side's HP at `t` seconds into the scene, counting down during impacts
    pub fn hp_at(&self, t: f32) -> (f32, f32) {
        let mut hp = (self.attacker.hp as f32, self.defender.hp as f32);
        for (i, strike) in self.strikes.iter().enumerate() {
            let start = INTRO + i as f32 * STRIKE + WINDUP;
            let k = ((t - start) / IMPACT).clamp(0.0, 1.0);
            let target = if strike.by_attacker { &mut hp.1 } else { &mut hp.0 };
            *target += (strike.target_hp_after as f32 - *target) * k;
        }
        hp
    }

    /// The strike in progress at `t` and how far through it is (0-1), if any
    pub fn strike_at(&self, t: f32) -> Option<(&Strike, f32)> {
        let i = ((t - INTRO) / STRIKE).floor();
        if i < 0.0 {
            return None;
        }
        self.strikes.get(i as usize).map(|s| (s, (t - INTRO - i * STRIKE) / STRIKE))
    }
}

/// Battles waiting to be shown, and the one on screen
#[derive(Resource, Default)]
pub struct BattleScene {
    queue: VecDeque<BattleReport>,
    /// The scene playing and its time in seconds (at Full speed)
    pub current: Option<(BattleReport, f32)>,
    /// Ask to end the current scene (any button)
    pub skip: bool,
}

impl BattleScene {
    /// A battle is on screen or about to be: the board and the AI wait
    pub fn busy(&self) -> bool {
        self.current.is_some() || !self.queue.is_empty()
    }
}

fn queue_reports(
    mut reports: MessageReader<BattleReport>,
    mut scene: ResMut<BattleScene>,
    settings: Res<DisplaySettings>,
) {
    for report in reports.read() {
        debug!("Battle report queued ({} strikes, scenes {:?})", report.strikes.len(), settings.battle);
        if settings.battle != BattleAnimation::Off {
            scene.queue.push_back(report.clone());
        }
    }
}

/// The sound a strike makes: siege engines boom, other ranged units shoot,
/// the rest hit
pub fn strike_sound(game_data: &super::GameData, unit_type: UnitType) -> &'static str {
    let (min_range, _) = game_data.unit_stats(unit_type).attack_range;
    match (min_range > 1, game_data.unit_class(unit_type)) {
        (true, super::UnitClass::Treads | super::UnitClass::Wheels) => "boom",
        (true, _) => "shot",
        _ => "hit",
    }
}

fn tick_battle_scene(
    time: Res<Time>,
    mut scene: ResMut<BattleScene>,
    settings: Res<DisplaySettings>,
    walking: Query<(), With<UnitAnimation>>,
    game_data: Res<super::GameData>,
    mut play: MessageWriter<PlaySfx>,
) {
    if settings.battle == BattleAnimation::Off {
        scene.queue.clear();
        scene.current = None;
        return;
    }
    // A scene starts once the attacker has walked to its spot
    if scene.current.is_none() {
        if !walking.is_empty() {
            return;
        }
        let Some(report) = scene.queue.pop_front() else { return };
        debug!("Battle scene: {:?} attacks {:?}", report.attacker.unit_type, report.defender.unit_type);
        scene.current = Some((report, 0.0));
        scene.skip = false;
        play.write(PlaySfx("select"));
    }
    let skip = std::mem::take(&mut scene.skip);
    let Some((report, t)) = scene.current.as_mut() else { return };
    let before = *t;
    *t += time.delta_secs() * settings.battle.speed();
    let now = *t;

    // Sounds at the moment each strike lands, and when a side is wiped out
    for (i, strike) in report.strikes.iter().enumerate() {
        let start = INTRO + i as f32 * STRIKE;
        let shooter = if strike.by_attacker { report.attacker.unit_type } else { report.defender.unit_type };
        if before < start + WINDUP * 0.5 && now >= start + WINDUP * 0.5 {
            play.write(PlaySfx(strike_sound(&game_data, shooter)));
        }
        if before < start + WINDUP + IMPACT && now >= start + WINDUP + IMPACT && strike.target_hp_after <= 0 {
            play.write(PlaySfx("defeat"));
        }
    }
    if skip || now >= report.duration() {
        scene.current = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> BattleReport {
        let side = |hp| Combatant { faction: Faction::Western, unit_type: UnitType::Scout, terrain: Terrain::Grass, hp };
        BattleReport {
            attacker: side(100),
            defender: side(80),
            strikes: vec![
                Strike { by_attacker: true, target_hp_after: 30, ranged: false },
                Strike { by_attacker: false, target_hp_after: 85, ranged: false },
            ],
        }
    }

    #[test]
    fn hp_counts_down_during_each_impact() {
        let r = report();
        assert_eq!(r.hp_at(0.0), (100.0, 80.0));
        // Halfway through the first impact the defender is halfway down
        let mid = INTRO + WINDUP + IMPACT / 2.0;
        assert!((r.hp_at(mid).1 - 55.0).abs() < 0.01);
        // After both strikes
        assert_eq!(r.hp_at(r.duration()), (85.0, 30.0));
        assert!(r.strike_at(INTRO + STRIKE + 0.1).is_some_and(|(s, _)| !s.by_attacker));
    }
}
