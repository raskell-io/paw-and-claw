use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};

use crate::game::{
    TurnState, TurnPhase, Unit, FactionMember, Faction, GridPosition,
    MovementHighlights, PendingAction, ProductionState, AttackEvent, CaptureEvent,
    TurnStartEvent, FactionFunds, GameMap, Terrain, Tile, UnitType, spawn_unit,
    calculate_damage, AiState, GameResult, VictoryType, FogOfWar, Commanders,
    PowerActivatedEvent, CommanderId,
};
use crate::states::GameState;

/// Resource to track if CO selection is pending
#[derive(Resource, Default)]
pub struct CoSelectionState {
    pub needs_selection: bool,
    pub player_selected: Option<CommanderId>,
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin)
            .init_resource::<CoSelectionState>()
            .add_systems(Update, (
                draw_main_menu.run_if(in_state(GameState::Menu)),
                draw_co_selection.run_if(in_state(GameState::Battle)),
                draw_battle_ui.run_if(in_state(GameState::Battle)),
                draw_action_menu.run_if(in_state(GameState::Battle)),
                draw_production_menu.run_if(in_state(GameState::Battle)),
                draw_victory_screen.run_if(in_state(GameState::Battle)),
                handle_fog_toggle.run_if(in_state(GameState::Battle)),
            ))
            .add_systems(Startup, start_battle_for_testing)
            .add_systems(OnEnter(GameState::Battle), trigger_co_selection);
    }
}

/// Trigger CO selection when entering battle
fn trigger_co_selection(mut selection_state: ResMut<CoSelectionState>) {
    selection_state.needs_selection = true;
    selection_state.player_selected = None;
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

            // Large title
            ui.label(egui::RichText::new("Paw & Claw").size(48.0).strong());
            ui.add_space(40.0);

            let button_size = egui::vec2(200.0, 50.0);

            if ui.add(egui::Button::new(egui::RichText::new("Battle Mode").size(20.0)).min_size(button_size)).clicked() {
                next_state.set(GameState::Battle);
            }
            ui.add_space(10.0);

            if ui.add(egui::Button::new(egui::RichText::new("Map Editor").size(20.0)).min_size(button_size)).clicked() {
                next_state.set(GameState::Editor);
            }
            ui.add_space(10.0);

            if ui.add(egui::Button::new(egui::RichText::new("Roguelike").size(20.0)).min_size(button_size)).clicked() {
                next_state.set(GameState::Roguelike);
            }
            ui.add_space(10.0);

            if ui.add(egui::Button::new(egui::RichText::new("Campaign").size(20.0)).min_size(button_size)).clicked() {
                next_state.set(GameState::Campaign);
            }
        });
    });
}

/// Draw CO selection screen at battle start
fn draw_co_selection(
    mut contexts: EguiContexts,
    mut selection_state: ResMut<CoSelectionState>,
    mut commanders: ResMut<Commanders>,
) {
    if !selection_state.needs_selection {
        return;
    }

    // Dim background
    egui::Area::new(egui::Id::new("co_select_bg"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .show(contexts.ctx_mut(), |ui| {
            ui.add(egui::Label::new("").sense(egui::Sense::click()));
        });

    egui::Window::new("Select Your Commander")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(contexts.ctx_mut(), |ui| {
            ui.set_min_width(500.0);

            ui.label(egui::RichText::new("Choose your Commanding Officer for this battle")
                .size(14.0));
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Eastern Empire COs
            ui.label(egui::RichText::new("Eastern Empire").size(16.0).strong()
                .color(egui::Color32::from_rgb(100, 150, 255)));
            ui.add_space(5.0);

            let eastern_cos = CommanderId::for_faction(Faction::Eastern);
            for co_id in eastern_cos {
                let co = co_id.get_commander();
                let is_selected = selection_state.player_selected == Some(co_id);

                ui.horizontal(|ui| {
                    // Selection button
                    let button_text = if is_selected { "Selected" } else { "Select" };
                    let button_color = if is_selected {
                        egui::Color32::from_rgb(100, 200, 100)
                    } else {
                        egui::Color32::from_rgb(80, 80, 80)
                    };

                    if ui.add(egui::Button::new(egui::RichText::new(button_text).color(egui::Color32::WHITE))
                        .fill(button_color)
                        .min_size(egui::vec2(80.0, 30.0))).clicked()
                    {
                        selection_state.player_selected = Some(co_id);
                    }

                    ui.add_space(10.0);

                    // CO info
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(co.name).size(14.0).strong());
                            ui.label(egui::RichText::new(format!(" - {}", co.power.name))
                                .size(12.0)
                                .color(egui::Color32::from_rgb(255, 200, 50)));
                        });

                        // Show passive bonuses
                        let mut bonuses = Vec::new();
                        if co.attack_bonus > 1.0 {
                            bonuses.push(format!("+{:.0}% ATK", (co.attack_bonus - 1.0) * 100.0));
                        }
                        if co.defense_bonus > 1.0 {
                            bonuses.push(format!("+{:.0}% DEF", (co.defense_bonus - 1.0) * 100.0));
                        }
                        if co.movement_bonus > 0 {
                            bonuses.push(format!("+{} MOV", co.movement_bonus));
                        }
                        if co.income_bonus > 1.0 {
                            bonuses.push(format!("+{:.0}% Income", (co.income_bonus - 1.0) * 100.0));
                        }
                        if co.vision_bonus > 0 {
                            bonuses.push(format!("+{} Vision", co.vision_bonus));
                        }
                        if co.cost_modifier < 1.0 {
                            bonuses.push(format!("-{:.0}% Cost", (1.0 - co.cost_modifier) * 100.0));
                        }

                        ui.label(egui::RichText::new(bonuses.join(" | "))
                            .size(11.0)
                            .color(egui::Color32::from_rgb(150, 200, 150)));

                        ui.label(egui::RichText::new(co.description)
                            .size(10.0)
                            .weak());
                    });
                });

                ui.add_space(8.0);
            }

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Start battle button
            ui.horizontal(|ui| {
                let can_start = selection_state.player_selected.is_some();

                ui.add_enabled_ui(can_start, |ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("Start Battle!").size(16.0).strong())
                        .min_size(egui::vec2(200.0, 40.0))).clicked()
                    {
                        if let Some(player_co) = selection_state.player_selected {
                            // Set player CO
                            commanders.set_commander(Faction::Eastern, player_co);

                            // Assign random CO to AI
                            let ai_cos = CommanderId::for_faction(Faction::Northern);
                            let ai_co = ai_cos[rand::random::<usize>() % ai_cos.len()];
                            commanders.set_commander(Faction::Northern, ai_co);

                            info!("Player selected: {:?}, AI assigned: {:?}", player_co, ai_co);

                            selection_state.needs_selection = false;
                        }
                    }
                });

                if !can_start {
                    ui.label(egui::RichText::new("Select a commander to continue")
                        .size(12.0)
                        .weak());
                }
            });
        });
}

fn draw_battle_ui(
    mut contexts: EguiContexts,
    mut turn_state: ResMut<TurnState>,
    mut highlights: ResMut<MovementHighlights>,
    mut units: Query<(&mut Unit, &FactionMember, &GridPosition)>,
    mut next_state: ResMut<NextState<GameState>>,
    funds: Res<FactionFunds>,
    tiles: Query<&Tile>,
    mut turn_start_events: EventWriter<TurnStartEvent>,
    ai_state: Res<AiState>,
    game_result: Res<GameResult>,
    mut fog: ResMut<FogOfWar>,
    mut commanders: ResMut<Commanders>,
    mut power_events: EventWriter<PowerActivatedEvent>,
    selection_state: Res<CoSelectionState>,
) {
    // Don't show battle UI controls if game is over (victory screen handles it)
    if game_result.game_over {
        return;
    }

    // Don't show if CO selection is pending
    if selection_state.needs_selection {
        return;
    }

    let is_ai_turn = ai_state.enabled && turn_state.current_faction == Faction::Northern;

    // Top panel - turn info
    egui::TopBottomPanel::top("turn_info").show(contexts.ctx_mut(), |ui| {
        ui.horizontal(|ui| {
            if is_ai_turn {
                ui.label(egui::RichText::new(format!(
                    "Turn {} - AI Thinking...",
                    turn_state.turn_number
                )).color(egui::Color32::from_rgb(100, 150, 255)));
            } else {
                ui.label(format!(
                    "Turn {} - {}'s Turn",
                    turn_state.turn_number,
                    turn_state.current_faction.name()
                ));
            }

            ui.separator();

            // Show current funds
            ui.label(egui::RichText::new(format!(
                "Funds: {}",
                funds.get(turn_state.current_faction)
            )).strong());

            ui.separator();

            // CO Power meter
            let player_faction = Faction::Eastern;
            let co_id = commanders.get_active(player_faction);
            let co = co_id.get_commander();
            let charge = commanders.get_charge(player_faction);
            let power_cost = commanders.get_power_cost(player_faction);
            let can_activate = commanders.can_activate(player_faction);
            let power_active = commanders.is_power_active(player_faction);

            ui.label(egui::RichText::new(format!("CO: {}", co.name)).strong());

            // Power meter bar
            let progress = charge as f32 / power_cost as f32;
            let bar_color = if can_activate {
                egui::Color32::from_rgb(255, 200, 50)  // Gold when full
            } else {
                egui::Color32::from_rgb(100, 150, 255)  // Blue while charging
            };

            ui.add(egui::ProgressBar::new(progress)
                .fill(bar_color)
                .text(format!("{}/{}", charge, power_cost)));

            // Activate Power button
            if power_active {
                ui.label(egui::RichText::new("POWER ACTIVE!")
                    .color(egui::Color32::from_rgb(255, 200, 50))
                    .strong());
            } else {
                ui.add_enabled_ui(can_activate && !is_ai_turn, |ui| {
                    if ui.button(egui::RichText::new(format!("{}", co.power.name)).strong()).clicked() {
                        if let Some(effect) = commanders.activate_power(player_faction) {
                            power_events.send(PowerActivatedEvent {
                                faction: player_faction,
                                effect,
                            });
                            info!("Activated CO Power: {}!", co.power.name);
                        }
                    }
                });
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Fog of war toggle
                let fog_text = if fog.enabled { "Fog: ON" } else { "Fog: OFF" };
                if ui.button(fog_text).clicked() {
                    fog.enabled = !fog.enabled;
                }

                ui.separator();

                // Disable End Turn during AI turn
                ui.add_enabled_ui(!is_ai_turn, |ui| {
                    if ui.button("End Turn").clicked() {
                        // Reset all units of current faction
                        for (mut unit, faction, _) in units.iter_mut() {
                            if faction.faction == turn_state.current_faction {
                                unit.moved = false;
                                unit.attacked = false;
                            }
                        }

                        // Switch to next faction
                        let old_faction = turn_state.current_faction;
                        turn_state.current_faction = match turn_state.current_faction {
                            Faction::Eastern => Faction::Northern,
                            Faction::Northern => {
                                turn_state.turn_number += 1;
                                Faction::Eastern
                            }
                            _ => Faction::Eastern,
                        };
                        turn_state.phase = TurnPhase::Select;

                        // Clear selection
                        highlights.selected_unit = None;
                        highlights.tiles.clear();
                        highlights.attack_targets.clear();

                        // Calculate income for the new faction and fire event
                        let income: u32 = tiles.iter()
                            .filter(|t| t.owner == Some(turn_state.current_faction))
                            .map(|t| t.terrain.income_value())
                            .sum();

                        turn_start_events.send(TurnStartEvent {
                            faction: turn_state.current_faction,
                            income,
                        });

                        info!("Turn ended. {} -> {} (+{} income)",
                            old_faction.name(), turn_state.current_faction.name(), income);
                    }
                });

                if ui.button("Menu").clicked() {
                    next_state.set(GameState::Menu);
                }
            });
        });
    });

    // Side panel - unit info when selected
    if let Some(_selected_entity) = highlights.selected_unit {
        if let Some((unit, faction, pos)) = units
            .iter()
            .find(|(_, _, _)| true) // TODO: Match by entity
        {
            egui::SidePanel::right("unit_info")
                .min_width(220.0)
                .show(contexts.ctx_mut(), |ui| {
                    ui.heading(unit.unit_type.name());
                    ui.label(egui::RichText::new(unit.unit_type.description()).weak().size(11.0));
                    ui.add_space(4.0);
                    ui.label(format!("Faction: {}", faction.faction.name()));
                    ui.label(format!("Position: ({}, {})", pos.x, pos.y));
                    ui.separator();

                    let stats = unit.unit_type.stats();

                    // HP bar
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
                    ui.heading("Combat");
                    ui.horizontal(|ui| {
                        ui.label("Attack:");
                        ui.label(egui::RichText::new(format!("{}", stats.attack)).strong());
                    });
                    ui.horizontal(|ui| {
                        ui.label("Defense:");
                        ui.label(egui::RichText::new(format!("{}", stats.defense)).strong());
                    });
                    ui.horizontal(|ui| {
                        ui.label("Range:");
                        if stats.attack_range.0 == stats.attack_range.1 {
                            ui.label(egui::RichText::new(format!("{}", stats.attack_range.0)).strong());
                        } else {
                            ui.label(egui::RichText::new(format!("{}-{}", stats.attack_range.0, stats.attack_range.1)).strong());
                        }
                        if stats.attack_range.0 > 1 {
                            ui.label(egui::RichText::new("(indirect)").weak().size(10.0));
                        }
                    });

                    ui.separator();
                    ui.heading("Movement");
                    ui.horizontal(|ui| {
                        ui.label("Move:");
                        ui.label(egui::RichText::new(format!("{}", stats.movement)).strong());
                    });
                    ui.horizontal(|ui| {
                        ui.label("Vision:");
                        ui.label(egui::RichText::new(format!("{}", stats.vision)).strong());
                    });
                    ui.horizontal(|ui| {
                        ui.label("Class:");
                        ui.label(egui::RichText::new(format!("{:?}", stats.class)).size(11.0));
                    });

                    if stats.can_capture {
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("Can capture buildings").color(egui::Color32::from_rgb(100, 200, 100)));
                    }

                    ui.separator();
                    ui.heading("Status");
                    if unit.moved && unit.attacked {
                        ui.label(egui::RichText::new("Turn complete").color(egui::Color32::GRAY));
                    } else if unit.moved {
                        ui.label(egui::RichText::new("Moved (can act)").color(egui::Color32::YELLOW));
                    } else if unit.attacked {
                        ui.label(egui::RichText::new("Attacked").color(egui::Color32::GRAY));
                    } else {
                        ui.label(egui::RichText::new("Ready").color(egui::Color32::GREEN));
                    }

                    ui.separator();
                    ui.label(egui::RichText::new(format!("Cost: {}", stats.cost)).weak().size(11.0));
                });
        }
    }

    // Bottom panel - controls hint
    egui::TopBottomPanel::bottom("controls").show(contexts.ctx_mut(), |ui| {
        ui.horizontal(|ui| {
            ui.label("Click: Select/Move | WASD/Arrows: Pan camera | Space: Select/Confirm | ESC: Cancel | F: Toggle Fog");
        });
    });
}

/// Draw the action menu when a unit has moved and can attack
fn draw_action_menu(
    mut contexts: EguiContexts,
    mut pending_action: ResMut<PendingAction>,
    mut turn_state: ResMut<TurnState>,
    mut units: Query<(&mut Unit, Option<&FactionMember>, Option<&GridPosition>)>,
    tiles: Query<&Tile>,
    mut attack_events: EventWriter<AttackEvent>,
    mut capture_events: EventWriter<CaptureEvent>,
    map: Res<GameMap>,
    game_result: Res<GameResult>,
    commanders: Res<Commanders>,
) {
    // Don't show if game is over
    if game_result.game_over {
        return;
    }

    // Only show during Action phase
    if turn_state.phase != TurnPhase::Action {
        return;
    }

    let Some(acting_entity) = pending_action.unit else {
        return;
    };

    // Get attacker info for damage calculation
    let attacker_info = units.get(acting_entity).ok().map(|(unit, _, pos)| {
        (unit.clone(), pos.map(|p| (p.x, p.y)).unwrap_or((0, 0)))
    });

    let Some((attacker_unit, attacker_pos)) = attacker_info else {
        return;
    };

    // Get CO bonuses for damage calculation
    let attacker_co = commanders.get_bonuses(turn_state.current_faction);
    // Defender faction - opposite of current
    let defender_faction = match turn_state.current_faction {
        Faction::Eastern => Faction::Northern,
        Faction::Northern => Faction::Eastern,
        _ => Faction::Northern,
    };
    let defender_co = commanders.get_bonuses(defender_faction);

    // Collect target info with damage estimates
    let target_info: Vec<_> = pending_action.targets.iter()
        .filter_map(|&entity| {
            units.get(entity).ok().map(|(unit, _faction, pos)| {
                let pos_xy = pos.map(|p| (p.x, p.y)).unwrap_or((0, 0));

                // Calculate damage estimate (with CO bonuses)
                let defender_terrain = map.get(pos_xy.0, pos_xy.1).unwrap_or(Terrain::Grass);
                let damage = calculate_damage(&attacker_unit, &unit, defender_terrain, &attacker_co, &defender_co);

                // Calculate counter-attack damage (if defender can counter)
                let defender_stats = unit.unit_type.stats();
                let counter_damage = if defender_stats.attack > 0 && defender_stats.attack_range.0 == 1 {
                    // Estimate defender HP after our attack
                    let defender_hp_after = (unit.hp - damage).max(0);
                    if defender_hp_after > 0 {
                        let attacker_terrain = map.get(attacker_pos.0, attacker_pos.1).unwrap_or(Terrain::Grass);
                        // Create a temporary unit with reduced HP for counter calculation
                        let mut temp_defender = unit.clone();
                        temp_defender.hp = defender_hp_after;
                        Some(calculate_damage(&temp_defender, &attacker_unit, attacker_terrain, &defender_co, &attacker_co))
                    } else {
                        None // Defender will be destroyed, no counter
                    }
                } else {
                    None // Defender can't counter (indirect or no attack)
                };

                (
                    entity,
                    unit.unit_type.name().to_string(),
                    unit.hp,
                    damage,
                    counter_damage,
                )
            })
        })
        .collect();

    // Track which action was taken
    let mut attack_target: Option<Entity> = None;
    let mut capture_clicked = false;
    let mut wait_clicked = false;

    // Get capture info if applicable
    let capture_info = if pending_action.can_capture {
        pending_action.capture_tile.and_then(|tile_entity| {
            tiles.get(tile_entity).ok().map(|tile| {
                let required = tile.terrain.capture_points();
                let progress = tile.capture_progress;
                (tile.terrain.name(), progress, required)
            })
        })
    } else {
        None
    };

    // Show action menu in center of screen
    egui::Window::new("Action")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(contexts.ctx_mut(), |ui| {
            ui.set_min_width(200.0);

            // Show available targets
            if !target_info.is_empty() {
                ui.heading("Attack");
                ui.separator();

                for (entity, name, hp, damage, counter_damage) in &target_info {
                    // Show target name and damage estimate
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(name).strong().size(14.0));
                        ui.label(format!("(HP: {})", hp));
                    });

                    // Damage preview
                    let damage_text = if *damage >= *hp {
                        format!("Deal {} dmg (DESTROY)", damage)
                    } else {
                        format!("Deal {} dmg", damage)
                    };
                    ui.label(egui::RichText::new(&damage_text).color(egui::Color32::from_rgb(100, 200, 100)));

                    // Counter-attack warning
                    if let Some(counter) = counter_damage {
                        ui.label(egui::RichText::new(format!("Take {} counter", counter))
                            .color(egui::Color32::from_rgb(255, 150, 100)));
                    }

                    if ui.add(egui::Button::new(egui::RichText::new("Attack").size(14.0))
                        .min_size(egui::vec2(180.0, 28.0))).clicked()
                    {
                        attack_target = Some(*entity);
                    }

                    ui.add_space(5.0);
                }

                ui.separator();
            }

            // Show capture option if available
            if let Some((terrain_name, progress, required)) = &capture_info {
                ui.heading("Capture");
                ui.separator();

                ui.label(egui::RichText::new(*terrain_name).strong().size(14.0));
                ui.label(format!("Progress: {}/{}", progress, required));

                // Show progress bar
                let progress_pct = *progress as f32 / *required as f32;
                ui.add(egui::ProgressBar::new(progress_pct)
                    .fill(egui::Color32::from_rgb(255, 200, 50)));

                if ui.add(egui::Button::new(egui::RichText::new("Capture").size(14.0))
                    .min_size(egui::vec2(180.0, 28.0))).clicked()
                {
                    capture_clicked = true;
                }

                ui.separator();
            }

            // Wait button
            if ui.add(egui::Button::new(egui::RichText::new("Wait").size(16.0))
                .min_size(egui::vec2(180.0, 35.0))).clicked()
            {
                wait_clicked = true;
            }
        });

    // Process action outside the UI closure
    if let Some(target) = attack_target {
        attack_events.send(AttackEvent {
            attacker: acting_entity,
            defender: target,
        });

        if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
            unit.attacked = true;
        }

        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        turn_state.phase = TurnPhase::Select;
    } else if capture_clicked {
        if let Some(tile_entity) = pending_action.capture_tile {
            capture_events.send(CaptureEvent {
                unit: acting_entity,
                tile: tile_entity,
            });

            if let Ok((mut unit, _, _)) = units.get_mut(acting_entity) {
                unit.attacked = true; // Capturing ends the unit's turn
            }

            pending_action.unit = None;
            pending_action.targets.clear();
            pending_action.can_capture = false;
            pending_action.capture_tile = None;
            turn_state.phase = TurnPhase::Select;
        }
    } else if wait_clicked {
        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        turn_state.phase = TurnPhase::Select;
    }
}

/// Draw production menu when clicking on owned base
fn draw_production_menu(
    mut contexts: EguiContexts,
    mut production_state: ResMut<ProductionState>,
    mut funds: ResMut<FactionFunds>,
    turn_state: Res<TurnState>,
    mut commands: Commands,
    map: Res<GameMap>,
    game_result: Res<GameResult>,
    commanders: Res<Commanders>,
) {
    // Don't show if game is over
    if game_result.game_over {
        return;
    }

    if !production_state.active {
        return;
    }

    // Get CO cost modifier
    let co_bonuses = commanders.get_bonuses(turn_state.current_faction);

    // Units available for production at bases (base costs)
    let base_units = [
        (UnitType::Scout, "Scout", 10u32, "Infantry, can capture"),
        (UnitType::Shocktrooper, "Shocktrooper", 30u32, "Heavy infantry"),
        (UnitType::Recon, "Recon", 40u32, "Fast scout vehicle"),
        (UnitType::Siege, "Siege", 60u32, "Artillery (2-3 range)"),
        (UnitType::Ironclad, "Ironclad", 70u32, "Main battle tank"),
    ];

    // Apply CO cost modifier
    let buildable_units: Vec<_> = base_units.iter()
        .map(|(unit_type, name, base_cost, desc)| {
            let adjusted_cost = (*base_cost as f32 * co_bonuses.cost).round() as u32;
            (*unit_type, *name, adjusted_cost, *desc)
        })
        .collect();

    let current_funds = funds.get(turn_state.current_faction);
    let mut close_menu = false;
    let mut spawn_unit_type: Option<UnitType> = None;
    let mut spawn_cost: u32 = 0;

    egui::Window::new("Build Unit")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(contexts.ctx_mut(), |ui| {
            ui.set_min_width(250.0);

            ui.label(egui::RichText::new(format!("Funds: {}", current_funds)).strong());
            if co_bonuses.cost < 1.0 {
                ui.label(egui::RichText::new(format!("CO Discount: -{:.0}%", (1.0 - co_bonuses.cost) * 100.0))
                    .color(egui::Color32::from_rgb(100, 200, 100))
                    .size(11.0));
            }
            ui.separator();

            for (unit_type, name, cost, description) in &buildable_units {
                let can_afford = current_funds >= *cost;

                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(*name).strong().size(14.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let cost_color = if can_afford {
                            egui::Color32::from_rgb(100, 200, 100)
                        } else {
                            egui::Color32::from_rgb(200, 100, 100)
                        };
                        ui.label(egui::RichText::new(format!("{}", cost)).color(cost_color));
                    });
                });

                ui.label(egui::RichText::new(*description).weak().size(12.0));

                ui.add_enabled_ui(can_afford, |ui| {
                    if ui.add(egui::Button::new("Build").min_size(egui::vec2(230.0, 24.0))).clicked() {
                        spawn_unit_type = Some(*unit_type);
                        spawn_cost = *cost;
                    }
                });

                ui.add_space(5.0);
            }

            ui.separator();

            if ui.add(egui::Button::new(egui::RichText::new("Cancel").size(14.0))
                .min_size(egui::vec2(230.0, 30.0))).clicked()
            {
                close_menu = true;
            }
        });

    // Handle spawn outside the UI closure
    if let Some(unit_type) = spawn_unit_type {
        if funds.spend(turn_state.current_faction, spawn_cost) {
            let (x, y) = production_state.base_position;
            spawn_unit(
                &mut commands,
                &map,
                turn_state.current_faction,
                unit_type,
                x,
                y,
            );
            info!("Built {:?} at ({}, {}) for {} funds",
                unit_type, x, y, spawn_cost);
            close_menu = true;
        }
    }

    if close_menu {
        production_state.active = false;
        production_state.base_entity = None;
    }
}

/// Draw victory/defeat screen when game is over
fn draw_victory_screen(
    mut contexts: EguiContexts,
    game_result: Res<GameResult>,
    turn_state: Res<TurnState>,
    mut next_state: ResMut<NextState<GameState>>,
    mut commands: Commands,
    units: Query<Entity, With<Unit>>,
    tiles: Query<Entity, With<Tile>>,
) {
    if !game_result.game_over {
        return;
    }

    let Some(winner) = game_result.winner else {
        return;
    };

    // Determine if player won or lost (player is Eastern)
    let player_won = winner == Faction::Eastern;

    let title = if player_won { "Victory!" } else { "Defeat" };
    let title_color = if player_won {
        egui::Color32::from_rgb(100, 255, 100)
    } else {
        egui::Color32::from_rgb(255, 100, 100)
    };

    let victory_message = match game_result.victory_type {
        VictoryType::Elimination => {
            if player_won {
                "All enemy forces have been eliminated!"
            } else {
                "Your forces have been eliminated..."
            }
        }
        VictoryType::HQCapture => {
            if player_won {
                "You captured the enemy headquarters!"
            } else {
                "The enemy has captured your headquarters..."
            }
        }
        VictoryType::None => "",
    };

    let mut restart_clicked = false;
    let mut menu_clicked = false;

    // Dark overlay
    egui::Area::new(egui::Id::new("victory_overlay"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .order(egui::Order::Background)
        .show(contexts.ctx_mut(), |ui| {
            let screen_rect = ui.ctx().screen_rect();
            ui.painter().rect_filled(
                screen_rect,
                0.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
            );
        });

    // Victory/Defeat window
    egui::Window::new("Game Over")
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(contexts.ctx_mut(), |ui| {
            ui.set_min_width(300.0);

            ui.vertical_centered(|ui| {
                ui.add_space(20.0);

                // Title
                ui.label(egui::RichText::new(title)
                    .size(48.0)
                    .strong()
                    .color(title_color));

                ui.add_space(10.0);

                // Winner faction
                ui.label(egui::RichText::new(format!("{} wins!", winner.name()))
                    .size(24.0));

                ui.add_space(10.0);

                // Victory type message
                ui.label(egui::RichText::new(victory_message).size(16.0));

                ui.add_space(10.0);

                // Stats
                ui.label(format!("Turns: {}", turn_state.turn_number));

                ui.add_space(30.0);

                // Buttons
                if ui.add(egui::Button::new(egui::RichText::new("Play Again").size(18.0))
                    .min_size(egui::vec2(200.0, 40.0))).clicked()
                {
                    restart_clicked = true;
                }

                ui.add_space(10.0);

                if ui.add(egui::Button::new(egui::RichText::new("Main Menu").size(18.0))
                    .min_size(egui::vec2(200.0, 40.0))).clicked()
                {
                    menu_clicked = true;
                }

                ui.add_space(20.0);
            });
        });

    // Handle button clicks outside UI closure
    if restart_clicked || menu_clicked {
        // Despawn all game entities to reset
        for entity in units.iter() {
            commands.entity(entity).despawn();
        }
        for entity in tiles.iter() {
            commands.entity(entity).despawn();
        }

        // Reset game result
        commands.insert_resource(GameResult::default());
        commands.insert_resource(TurnState::default());
        commands.insert_resource(FactionFunds::default());

        if menu_clicked {
            next_state.set(GameState::Menu);
        }
        // If restart_clicked, stay in Battle state - the map will regenerate
    }
}

/// Handle keyboard shortcut for fog toggle
fn handle_fog_toggle(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut fog: ResMut<FogOfWar>,
) {
    if keyboard.just_pressed(KeyCode::KeyF) {
        fog.enabled = !fog.enabled;
        info!("Fog of war: {}", if fog.enabled { "ON" } else { "OFF" });
    }
}
