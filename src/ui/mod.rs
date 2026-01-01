use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};

use crate::game::{TurnState, Unit, FactionMember, GridPosition, MovementHighlights};
use crate::states::GameState;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin)
            .add_systems(Update, (
                draw_main_menu.run_if(in_state(GameState::Menu)),
                draw_battle_ui.run_if(in_state(GameState::Battle)),
            ))
            .add_systems(Startup, start_battle_for_testing);
    }
}

// Temporary: skip menu and go straight to battle for testing
fn start_battle_for_testing(mut next_state: ResMut<NextState<GameState>>) {
    next_state.set(GameState::Battle);
}

fn draw_main_menu(
    mut contexts: EguiContexts,
    mut next_state: ResMut<NextState<GameState>>,
) {
    egui::CentralPanel::default().show(contexts.ctx_mut(), |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading("Paw & Claw");
            ui.add_space(20.0);

            if ui.button("Battle Mode").clicked() {
                next_state.set(GameState::Battle);
            }

            if ui.button("Map Editor").clicked() {
                next_state.set(GameState::Editor);
            }

            if ui.button("Roguelike").clicked() {
                next_state.set(GameState::Roguelike);
            }

            if ui.button("Campaign").clicked() {
                next_state.set(GameState::Campaign);
            }
        });
    });
}

fn draw_battle_ui(
    mut contexts: EguiContexts,
    turn_state: Res<TurnState>,
    highlights: Res<MovementHighlights>,
    units: Query<(&Unit, &FactionMember, &GridPosition)>,
    mut next_state: ResMut<NextState<GameState>>,
) {
    // Top panel - turn info
    egui::TopBottomPanel::top("turn_info").show(contexts.ctx_mut(), |ui| {
        ui.horizontal(|ui| {
            ui.label(format!(
                "Turn {} - {}'s Turn",
                turn_state.turn_number,
                turn_state.current_faction.name()
            ));

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("End Turn").clicked() {
                    // TODO: Actually end turn
                    info!("End turn clicked");
                }

                if ui.button("Menu").clicked() {
                    next_state.set(GameState::Menu);
                }
            });
        });
    });

    // Side panel - unit info when selected
    if let Some(_selected_entity) = highlights.selected_unit {
        if let Some((unit, faction, _pos)) = units
            .iter()
            .find(|(_, _, _)| true) // TODO: Match by entity
        {
            egui::SidePanel::right("unit_info")
                .min_width(200.0)
                .show(contexts.ctx_mut(), |ui| {
                    ui.heading(unit.unit_type.name());
                    ui.label(format!("Faction: {}", faction.faction.name()));
                    ui.separator();

                    let stats = unit.unit_type.stats();
                    ui.label(format!("HP: {}/{}", unit.hp, stats.max_hp));

                    let hp_pct = unit.hp_percentage();
                    let bar_color = if hp_pct > 0.5 {
                        egui::Color32::GREEN
                    } else if hp_pct > 0.25 {
                        egui::Color32::YELLOW
                    } else {
                        egui::Color32::RED
                    };

                    ui.add(
                        egui::ProgressBar::new(hp_pct)
                            .fill(bar_color)
                            .show_percentage()
                    );

                    ui.separator();
                    ui.label(format!("Attack: {}", stats.attack));
                    ui.label(format!("Defense: {}", stats.defense));
                    ui.label(format!("Movement: {}", stats.movement));
                    ui.label(format!("Range: {}-{}", stats.attack_range.0, stats.attack_range.1));

                    ui.separator();
                    if unit.moved {
                        ui.label("Already moved");
                    }
                    if unit.attacked {
                        ui.label("Already attacked");
                    }
                });
        }
    }

    // Bottom panel - controls hint
    egui::TopBottomPanel::bottom("controls").show(contexts.ctx_mut(), |ui| {
        ui.horizontal(|ui| {
            ui.label("Left-click: Select unit | Right-click: Move/Attack | ESC: Deselect");
        });
    });
}
