//! Pause menu, game-over screen and battle teardown

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::input::MenuInput;
use super::theme::{self, Tone};
use super::{BattleSetupState, InGameMenuState};
use crate::game::{
    reset_ai, AiState, BattleEntity, FactionFunds, FactionMember, FogOfWar, GameData, GameMap,
    GameResult, GridCursor, InputMode, LoadGameEvent, MovementHighlights, MovementPath, PendingAction,
    ProductionState, SaveGameEvent, TurnState, Unit, VictoryType, Weather,
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
    commands.insert_resource(InGameMenuState::default());
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

/// Pause menu entries
#[derive(Clone, Copy, PartialEq)]
enum PauseItem {
    Resume,
    Fog,
    Controls,
    Save,
    Load,
    Title,
}

const PAUSE_ITEMS: [PauseItem; 6] =
    [PauseItem::Resume, PauseItem::Fog, PauseItem::Controls, PauseItem::Save, PauseItem::Load, PauseItem::Title];

#[derive(SystemParam)]
pub struct PauseActions<'w> {
    next_state: ResMut<'w, NextState<GameState>>,
    fog: ResMut<'w, FogOfWar>,
    input_mode: ResMut<'w, InputMode>,
    save: MessageWriter<'w, SaveGameEvent>,
    load: MessageWriter<'w, LoadGameEvent>,
}

pub fn draw_pause_menu(
    mut contexts: EguiContexts,
    mut menu: ResMut<InGameMenuState>,
    mut actions: PauseActions,
    setup: Res<BattleSetupState>,
    game_result: Res<GameResult>,
    input: MenuInput,
    gamepads: Query<&Gamepad>,
    mut selected: Local<usize>,
    mut shown: Local<bool>,
) {
    if setup.needs_setup || game_result.game_over {
        menu.open = false;
    }
    // Gamepad Start toggles the menu
    if !setup.needs_setup && !game_result.game_over
        && gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::Start))
    {
        menu.open = !menu.open;
    }
    if !menu.open {
        *shown = false;
        return;
    }
    // The Esc that opened the menu must not immediately close it
    let accept_input = std::mem::replace(&mut *shown, true);
    if !accept_input {
        *selected = 0;
    }

    let mut activate = None;
    if accept_input {
        input.step_vertical(&mut selected, PAUSE_ITEMS.len());
        if input.confirm() {
            activate = Some(PAUSE_ITEMS[*selected]);
        }
        if input.back() {
            menu.open = false;
            return;
        }
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    theme::backdrop(ctx, "pause_backdrop", 150);
    egui::Area::new(egui::Id::new("pause_menu"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            theme::card().show(ui, |ui| {
                ui.set_width(340.0);
                ui.vertical_centered(|ui| ui.label(theme::display("Paused", 30.0).color(theme::WOOD_DARK)));
                ui.add_space(4.0);
                for (i, item) in PAUSE_ITEMS.iter().enumerate() {
                    let (label, detail, tone) = match item {
                        PauseItem::Resume => ("Resume".to_string(), None, Tone::Normal),
                        PauseItem::Fog => (format!("Fog of war: {}", if actions.fog.enabled { "On" } else { "Off" }), None, Tone::Normal),
                        PauseItem::Controls => (format!("Controls: {}", actions.input_mode.name()), None, Tone::Normal),
                        PauseItem::Save => ("Save game".to_string(), Some("F5"), Tone::Normal),
                        PauseItem::Load => ("Load game".to_string(), Some("F9"), Tone::Normal),
                        PauseItem::Title => ("Retreat to title".to_string(), Some("Abandons this battle"), Tone::Danger),
                    };
                    if *item == PauseItem::Title {
                        ui.separator();
                    }
                    let response = theme::menu_item(ui, &label, detail, i == *selected, true, tone);
                    if theme::pointer_selects(ui, &response) {
                        *selected = i;
                    }
                    if response.clicked() {
                        activate = Some(*item);
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    theme::key_hint(ui, "↑↓", "Choose", false);
                    theme::key_hint(ui, "Enter", "Select", false);
                    theme::key_hint(ui, "Esc", "Resume", false);
                });
            });
        });

    match activate {
        Some(PauseItem::Resume) => menu.open = false,
        Some(PauseItem::Fog) => actions.fog.enabled = !actions.fog.enabled,
        Some(PauseItem::Controls) => *actions.input_mode = actions.input_mode.cycle(),
        Some(PauseItem::Save) => {
            actions.save.write(SaveGameEvent { slot: 0 });
            menu.open = false;
        }
        Some(PauseItem::Load) => {
            actions.load.write(LoadGameEvent { slot: 0 });
            menu.open = false;
        }
        Some(PauseItem::Title) => {
            menu.open = false;
            actions.next_state.set(GameState::Menu);
        }
        None => {}
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
    fog: Res<FogOfWar>,
    ai: Res<AiState>,
    input: MenuInput,
    time: Res<Time>,
    mut selected: Local<usize>,
) {
    let (true, Some(winner)) = (game_result.game_over, game_result.winner) else {
        *selected = 0;
        return;
    };
    let player = battle_config.player_faction;
    let won = winner == player;
    let message = match (game_result.victory_type, won) {
        (VictoryType::Elimination, true) => "Every enemy unit has been routed.",
        (VictoryType::Elimination, false) => "Your forces have been scattered to the winds.",
        (VictoryType::HQCapture, true) => "Your banner flies over the enemy headquarters.",
        (VictoryType::HQCapture, false) => "The enemy has taken your headquarters.",
        (VictoryType::None, _) => "",
    };
    let survivors = |faction| units.iter().filter(|(_, f)| f.faction == faction).count();
    let enemy = battle_config.next_faction(player);

    input.step_vertical(&mut selected, 2);
    let mut choice = input.confirm().then_some(*selected);

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
                    ui.label(theme::display(format!("{} prevails", game_data.faction_name(winner)), 22.0)
                        .color(theme::shade(theme::faction_color(&game_data, winner), 0.75)));
                    ui.add_space(4.0);
                    ui.label(theme::body(message, 17.0).color(theme::INK_SOFT).italics());
                });
                ui.add_space(10.0);
                theme::well().show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::Grid::new("battle_stats").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
                        ui.label(theme::body("Days fought", 16.0).color(theme::INK_SOFT));
                        ui.label(theme::bold(turn_state.turn_number.to_string(), 16.0));
                        ui.end_row();
                        ui.label(theme::body(format!("{} units left", game_data.faction_name(player)), 16.0).color(theme::INK_SOFT));
                        ui.label(theme::bold(survivors(player).to_string(), 16.0));
                        ui.end_row();
                        ui.label(theme::body(format!("{} units left", game_data.faction_name(enemy)), 16.0).color(theme::INK_SOFT));
                        ui.label(theme::bold(survivors(enemy).to_string(), 16.0));
                        ui.end_row();
                    });
                });
                ui.add_space(8.0);
                for (i, label) in ["Play again", "Return to title"].iter().enumerate() {
                    let response = theme::menu_item(ui, label, None, i == *selected, true, Tone::Normal);
                    if theme::pointer_selects(ui, &response) {
                        *selected = i;
                    }
                    if response.clicked() {
                        choice = Some(i);
                    }
                }
            });
        });

    match choice {
        Some(0) => {
            // Same state, fresh board: reopen setup with the last choices kept
            reset_battle(&mut commands, &entities, fog.enabled, ai.enabled);
            setup.needs_setup = true;
        }
        Some(_) => next_state.set(GameState::Menu),
        None => {}
    }
}
