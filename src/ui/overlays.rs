//! Game-over screen and battle teardown

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::input::MenuInput;
use super::theme::{self, Tone};
use super::BattleSetupState;
use crate::game::{
    reset_ai, AiState, BattleEntity, FactionFunds, FactionMember, FogOfWar, GameData, GameMap,
    GameResult, GridCursor, MovementHighlights, MovementPath, PendingAction,
    ProductionState, TurnState, Unit, VictoryType, Weather,
};
use crate::states::GameState;

/// Despawn everything from the current battle and reset per-battle state
pub fn reset_battle(
    commands: &mut Commands,
    entities: &Query<Entity, With<BattleEntity>>,
    fog_enabled: bool,
    ai_enabled: bool,
) {
    for entity in entities.iter() {
        commands.entity(entity).despawn();
    }
    commands.insert_resource(GameMap::default());
    commands.insert_resource(GameResult::default());
    commands.insert_resource(TurnState::default());
    commands.insert_resource(FactionFunds::default());
    commands.insert_resource(PendingAction::default());
    commands.insert_resource(MovementHighlights::default());
    commands.insert_resource(MovementPath::default());
    commands.insert_resource(ProductionState::default());
    commands.insert_resource(GridCursor::default());
    commands.insert_resource(Weather::default());
    let mut fog = FogOfWar::default();
    fog.enabled = fog_enabled;
    commands.insert_resource(fog);
    reset_ai(commands, ai_enabled);
    commands.insert_resource(crate::game::MapMenu::default());
}

/// Leaving the battle state (to the title screen) tears the battle down
pub fn cleanup_battle_on_exit(
    mut commands: Commands,
    entities: Query<Entity, With<BattleEntity>>,
    fog: Res<FogOfWar>,
    ai: Res<AiState>,
) {
    reset_battle(&mut commands, &entities, fog.enabled, ai.enabled);
}

/// What the result screen needs beyond the battle itself: the campaign
/// (which mission this was, what comes next) and what a fresh board keeps
#[derive(SystemParam)]
pub struct ResultContext<'w> {
    battle_map: Res<'w, crate::game::BattleMap>,
    dialogue: Res<'w, crate::game::Dialogue>,
    progress: Res<'w, crate::game::CampaignProgress>,
    missions: ResMut<'w, super::campaign::MissionSelect>,
    fog: Res<'w, FogOfWar>,
    ai: Res<'w, AiState>,
}

/// Where the result screen can lead
#[derive(Clone, Copy, PartialEq)]
enum After {
    PlayAgain,
    NextMission,
    TryAgain,
    Missions,
    Title,
}

impl After {
    fn label(self) -> &'static str {
        match self {
            After::PlayAgain => "Play again",
            After::NextMission => "Next mission",
            After::TryAgain => "Try again",
            After::Missions => "Mission list",
            After::Title => "Return to title",
        }
    }
}

pub fn draw_game_over(
    mut contexts: EguiContexts,
    game_result: Res<GameResult>,
    turn_state: Res<TurnState>,
    game_data: Res<GameData>,
    battle_config: Res<crate::game::BattleConfig>,
    units: Query<(&Unit, &FactionMember)>,
    mut setup: ResMut<BattleSetupState>,
    mut next_state: ResMut<NextState<GameState>>,
    mut commands: Commands,
    entities: Query<Entity, With<BattleEntity>>,
    mut context: ResultContext,
    input: MenuInput,
    time: Res<Time>,
    mut selected: Local<usize>,
    mut shown: Local<bool>,
) {
    let (true, Some(winner)) = (game_result.game_over, game_result.winner) else {
        *selected = 0;
        *shown = false;
        return;
    };
    // A mission's closing lines come first
    if context.dialogue.active() {
        *shown = false;
        return;
    }
    // The press that ended the dialogue must not also pick an option
    let accept_input = std::mem::replace(&mut *shown, true);
    // The winning side (allies win together); it's a victory when a person
    // is on it, or when only the computer was playing
    let winners: Vec<_> = battle_config.armies.iter().map(|a| a.faction).filter(|&f| battle_config.allied(f, winner)).collect();
    let anyone_playing = battle_config.armies.iter().any(|a| !a.ai);
    let won = !anyone_playing || winners.iter().any(|&f| !battle_config.is_ai_faction(f));
    let message = match (game_result.victory_type, won) {
        (VictoryType::Elimination, true) => "Every enemy unit has been routed.",
        (VictoryType::Elimination, false) => "Your forces have been scattered to the winds.",
        (VictoryType::HQCapture, true) => "Your banner flies over the enemy headquarters.",
        (VictoryType::HQCapture, false) => "The enemy has taken your headquarters.",
        (VictoryType::DayLimit, true) => "Time is up, and the woods are yours: you hold the most ground.",
        (VictoryType::DayLimit, false) => "Time is up. The enemy holds more of the woods.",
        (VictoryType::Holdout, true) => "The last day has passed, and the line still stands.",
        (VictoryType::Holdout, false) => "The enemy held out to the last day.",
        (VictoryType::None, _) => "",
    };
    let survivors = |faction| units.iter().filter(|(_, f)| f.faction == faction).count();

    // A campaign mission leads on to the next one (or back for another try)
    let campaign = &game_data.campaign;
    let mission = context.battle_map.mission.as_deref().and_then(|key| campaign.index_of(key));
    let options: Vec<After> = match mission {
        None => vec![After::PlayAgain, After::Title],
        Some(i) if won && i + 1 < campaign.missions.len() => vec![After::NextMission, After::Missions, After::Title],
        Some(_) if won => vec![After::Missions, After::Title],
        Some(_) => vec![After::TryAgain, After::Missions, After::Title],
    };
    let mut choice = None;
    if accept_input {
        input.step_vertical(&mut selected, options.len());
        choice = input.confirm().then_some(*selected);
    }
    *selected = (*selected).min(options.len() - 1);

    let Ok(ctx) = contexts.ctx_mut() else { return };
    theme::backdrop(ctx, "game_over_backdrop", 170);
    egui::Area::new(egui::Id::new("game_over"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            theme::card().show(ui, |ui| {
                ui.set_width(420.0);
                ui.vertical_centered(|ui| {
                    // Gentle pulse on the title
                    let pulse = 1.0 + 0.03 * (time.elapsed_secs() * 2.5).sin();
                    let (title, color) = if won { ("Victory!", theme::LEAF) } else { ("Defeat", theme::BERRY) };
                    ui.label(egui::RichText::new(title).font(egui::FontId::new(56.0 * pulse, theme::title_family())).color(color));
                    if let Some(i) = mission {
                        ui.label(theme::bold(format!("Mission {} · {}", i + 1, campaign.missions[i].name), 16.0).color(theme::WOOD_DARK));
                    }
                    let names: Vec<&str> = winners.iter().map(|&f| game_data.faction_name(f)).collect();
                    let verb = if names.len() > 1 { "prevail" } else { "prevails" };
                    ui.label(theme::display(format!("{} {verb}", names.join(" & ")), 22.0)
                        .color(theme::shade(theme::faction_color(&game_data, winner), 0.75)));
                    ui.add_space(4.0);
                    ui.label(theme::body(message, 17.0).color(theme::INK_SOFT).italics());
                    if won && mission.is_some_and(|i| i + 1 == campaign.missions.len()) {
                        ui.label(theme::body("That was the last mission, for now.", 15.0).color(theme::INK_SOFT));
                    }
                });
                ui.add_space(10.0);
                theme::well().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::Grid::new("battle_stats").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            super::icons::icon(ui, super::icons::Icon::Sun, 20.0);
                            ui.label(theme::body("Days fought", 16.0).color(theme::INK_SOFT));
                        });
                        // A holdout ends as the day after its last one begins
                        let days = battle_config.day_limit.map_or(turn_state.turn_number, |limit| turn_state.turn_number.min(limit));
                        ui.label(theme::bold(days.to_string(), 16.0));
                        ui.end_row();
                        if let Some(best) = mission.filter(|_| won).and_then(|i| context.progress.best(&campaign.missions[i].key)) {
                            ui.horizontal(|ui| {
                                super::icons::icon(ui, super::icons::Icon::Star, 20.0);
                                ui.label(theme::body("Best", 16.0).color(theme::INK_SOFT));
                            });
                            ui.label(theme::bold(format!("{best} day{}", if best == 1 { "" } else { "s" }), 16.0));
                            ui.end_row();
                        }
                        for army in &battle_config.armies {
                            ui.horizontal(|ui| {
                                let icon = if winners.contains(&army.faction) { super::icons::Icon::Paw } else { super::icons::Icon::Swords };
                                super::icons::icon(ui, icon, 20.0);
                                ui.label(theme::body(format!("{} units left", game_data.faction_name(army.faction)), 16.0).color(theme::INK_SOFT));
                            });
                            ui.label(theme::bold(survivors(army.faction).to_string(), 16.0));
                            ui.end_row();
                        }
                    });
                });
                ui.add_space(8.0);
                for (i, option) in options.iter().enumerate() {
                    let response = theme::menu_item(ui, option.label(), None, i == *selected, true, Tone::Normal);
                    if theme::pointer_selects(ui, &response) {
                        *selected = i;
                    }
                    if response.clicked() {
                        choice = Some(i);
                    }
                }
            });
        });

    let Some(after) = choice.and_then(|i| options.get(i).copied()) else { return };
    if after == After::Title {
        next_state.set(GameState::Menu);
        return;
    }
    // Same state, fresh board: back to setup (skirmish keeps the last
    // choices) or to the mission list, which starts a waiting mission itself
    reset_battle(&mut commands, &entities, context.fog.enabled, context.ai.enabled);
    setup.needs_setup = true;
    setup.campaign = after != After::PlayAgain;
    let key = |i: usize| campaign.missions.get(i).map(|m| m.key.clone());
    context.missions.launch = match (after, mission) {
        (After::NextMission, Some(i)) => key(i + 1),
        (After::TryAgain, Some(i)) => key(i),
        _ => None,
    };
}
