use bevy::prelude::*;
use bevy::core_pipeline::core_3d::Camera3d;
use bevy_egui::{egui, EguiContexts, EguiPlugin};

use crate::game::{
    TurnState, TurnPhase, Unit, FactionMember, Faction, GridPosition,
    MovementHighlights, PendingAction, ProductionState, AttackEvent, CaptureEvent, JoinEvent, ResupplyEvent, LoadEvent, UnloadEvent,
    TurnStartEvent, FactionFunds, GameMap, Terrain, Tile, UnitType, spawn_unit,
    calculate_damage, AiState, GameResult, VictoryType, FogOfWar, Commanders,
    PowerActivatedEvent, CommanderId, MapId, get_builtin_map,
    spawn_map_from_data, spawn_units_from_data, MapData, UnitPlacement, PropertyOwnership,
    TILE_SIZE, Weather, WeatherType, SpriteAssets, screen_to_grid,
};
use crate::states::GameState;

/// Resource to track battle setup (CO + Map selection)
#[derive(Resource)]
pub struct BattleSetupState {
    pub needs_setup: bool,
    pub player_faction: Faction,
    pub player_co: Option<CommanderId>,
    pub selected_map: MapId,
}

impl Default for BattleSetupState {
    fn default() -> Self {
        Self {
            needs_setup: false,
            player_faction: Faction::Eastern,
            player_co: Some(CommanderId::Kira),  // Pre-select first CO
            selected_map: MapId::Woodland,
        }
    }
}

/// Edit mode for map editor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EditorMode {
    #[default]
    Terrain,
    Units,
    Properties,
}

/// Resource to track map editor state
#[derive(Resource)]
pub struct EditorState {
    /// The map being edited
    pub map: MapData,
    /// Current edit mode
    pub mode: EditorMode,
    /// Selected terrain brush
    pub selected_terrain: Terrain,
    /// Selected unit type for placement
    pub selected_unit: UnitType,
    /// Selected faction for units/properties
    pub selected_faction: Faction,
    /// Map name for saving
    pub map_name: String,
    /// Status message
    pub status: String,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            map: MapData::new("New Map", 12, 8),
            mode: EditorMode::Terrain,
            selected_terrain: Terrain::Grass,
            selected_unit: UnitType::Scout,
            selected_faction: Faction::Eastern,
            map_name: "custom_map".to_string(),
            status: String::new(),
        }
    }
}

/// Resource tracking hovered unit for tooltip display
#[derive(Resource, Default)]
pub struct HoveredUnit {
    pub entity: Option<Entity>,
    pub screen_pos: (f32, f32),
}

/// Resource to track selected tile for info panel (right-click or hover)
#[derive(Resource, Default)]
pub struct SelectedTile {
    pub position: Option<IVec2>,
    pub screen_pos: (f32, f32),
}

/// Resource to track in-game menu state
#[derive(Resource, Default)]
pub struct InGameMenuState {
    pub open: bool,
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin)
            .init_resource::<BattleSetupState>()
            .init_resource::<EditorState>()
            .init_resource::<HoveredUnit>()
            .init_resource::<SelectedTile>()
            .init_resource::<InGameMenuState>()
            .add_systems(Update, (
                draw_main_menu.run_if(in_state(GameState::Menu)),
                draw_battle_setup.run_if(in_state(GameState::Battle)),
                draw_battle_ui.run_if(in_state(GameState::Battle)),
                draw_action_menu.run_if(in_state(GameState::Battle)),
                draw_production_menu.run_if(in_state(GameState::Battle)),
                draw_victory_screen.run_if(in_state(GameState::Battle)),
                draw_ingame_menu.run_if(in_state(GameState::Battle)),
                handle_ingame_menu_input.run_if(in_state(GameState::Battle)),
                handle_fog_toggle.run_if(in_state(GameState::Battle)),
                track_hovered_unit.run_if(in_state(GameState::Battle)),
                track_hovered_tile.run_if(in_state(GameState::Battle)),
                draw_unit_tooltip.run_if(in_state(GameState::Battle)),
                draw_terrain_info_panel.run_if(in_state(GameState::Battle)),
                draw_unit_hp_numbers.run_if(in_state(GameState::Battle)),
                draw_editor.run_if(in_state(GameState::Editor)),
                editor_paint.run_if(in_state(GameState::Editor)),
            ))
            .add_systems(Startup, start_battle_for_testing)
            .add_systems(OnEnter(GameState::Battle), trigger_battle_setup)
            .add_systems(OnEnter(GameState::Editor), setup_editor)
            .add_systems(OnExit(GameState::Editor), cleanup_editor);
    }
}

/// Trigger battle setup when entering battle state
fn trigger_battle_setup(mut setup_state: ResMut<BattleSetupState>) {
    setup_state.needs_setup = true;
    setup_state.player_co = Some(CommanderId::Kira);  // Pre-select first CO
    setup_state.selected_map = MapId::Woodland;
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

/// Helper to format CO bonuses as string
fn format_co_bonuses(co: &crate::game::Commander) -> String {
    let mut bonuses = Vec::new();
    if co.attack_bonus > 1.0 {
        bonuses.push(format!("+{:.0}% ATK", (co.attack_bonus - 1.0) * 100.0));
    } else if co.attack_bonus < 1.0 {
        bonuses.push(format!("{:.0}% ATK", (co.attack_bonus - 1.0) * 100.0));
    }
    if co.defense_bonus > 1.0 {
        bonuses.push(format!("+{:.0}% DEF", (co.defense_bonus - 1.0) * 100.0));
    } else if co.defense_bonus < 1.0 {
        bonuses.push(format!("{:.0}% DEF", (co.defense_bonus - 1.0) * 100.0));
    }
    if co.movement_bonus > 0 {
        bonuses.push(format!("+{} MOV", co.movement_bonus));
    } else if co.movement_bonus < 0 {
        bonuses.push(format!("{} MOV", co.movement_bonus));
    }
    if co.income_bonus > 1.0 {
        bonuses.push(format!("+{:.0}% $", (co.income_bonus - 1.0) * 100.0));
    }
    if co.vision_bonus > 0 {
        bonuses.push(format!("+{} VIS", co.vision_bonus));
    }
    if co.cost_modifier < 1.0 {
        bonuses.push(format!("-{:.0}% Cost", (1.0 - co.cost_modifier) * 100.0));
    } else if co.cost_modifier > 1.0 {
        bonuses.push(format!("+{:.0}% Cost", (co.cost_modifier - 1.0) * 100.0));
    }
    bonuses.join(" | ")
}

/// Draw battle setup screen (CO + Map selection)
fn draw_battle_setup(
    mut contexts: EguiContexts,
    mut setup_state: ResMut<BattleSetupState>,
    mut commanders: ResMut<Commanders>,
    mut commands: Commands,
    mut game_map: ResMut<GameMap>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sprite_assets: Res<SpriteAssets>,
    images: Res<Assets<Image>>,
) {
    if !setup_state.needs_setup {
        return;
    }

    egui::Window::new("Battle Setup")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(contexts.ctx_mut(), |ui| {
            ui.set_min_width(700.0);

            // === FACTION SELECTION ===
            ui.label(egui::RichText::new("Choose Your Faction").size(18.0).strong());
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                let factions = [
                    (Faction::Eastern, "Eastern Empire", egui::Color32::from_rgb(200, 80, 80)),
                    (Faction::Northern, "Northern Realm", egui::Color32::from_rgb(80, 120, 200)),
                    (Faction::Western, "Western Frontier", egui::Color32::from_rgb(80, 160, 80)),
                    (Faction::Southern, "Southern Pride", egui::Color32::from_rgb(200, 160, 50)),
                ];

                for (faction, name, color) in factions {
                    let is_selected = setup_state.player_faction == faction;
                    let button_color = if is_selected { color } else { egui::Color32::from_rgb(60, 60, 60) };

                    if ui.add(egui::Button::new(egui::RichText::new(name).size(13.0))
                        .fill(button_color)
                        .min_size(egui::vec2(150.0, 30.0))).clicked()
                    {
                        if setup_state.player_faction != faction {
                            setup_state.player_faction = faction;
                            setup_state.player_co = None; // Reset CO when faction changes
                        }
                    }
                }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);

            // Two columns: Map selection on left, CO selection on right
            ui.horizontal(|ui| {
                // === MAP SELECTION ===
                ui.vertical(|ui| {
                    ui.set_min_width(280.0);
                    ui.label(egui::RichText::new("Select Map").size(16.0).strong());
                    ui.add_space(5.0);

                    egui::ScrollArea::vertical().max_height(250.0).show(ui, |ui| {
                        for map_id in MapId::all_builtin() {
                            let map_data = get_builtin_map(map_id);
                            let is_selected = setup_state.selected_map == map_id;

                            let button_color = if is_selected {
                                egui::Color32::from_rgb(80, 120, 180)
                            } else {
                                egui::Color32::from_rgb(60, 60, 60)
                            };

                            ui.horizontal(|ui| {
                                if ui.add(egui::Button::new(
                                    egui::RichText::new(if is_selected { "▶" } else { "  " })
                                ).fill(button_color).min_size(egui::vec2(24.0, 24.0))).clicked() {
                                    setup_state.selected_map = map_id;
                                }

                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(map_id.name()).size(12.0).strong());
                                    ui.label(egui::RichText::new(format!("{}x{}", map_data.width, map_data.height))
                                        .size(10.0).weak());
                                });
                            });

                            if is_selected {
                                ui.indent("map_desc", |ui| {
                                    ui.label(egui::RichText::new(&map_data.description)
                                        .size(10.0).weak().italics());
                                });
                            }
                            ui.add_space(3.0);
                        }
                    });
                });

                ui.separator();

                // === CO SELECTION ===
                ui.vertical(|ui| {
                    ui.set_min_width(350.0);
                    ui.label(egui::RichText::new(format!("{} Commanders", setup_state.player_faction.name()))
                        .size(16.0).strong());
                    ui.add_space(5.0);

                    let faction_cos = CommanderId::for_faction(setup_state.player_faction);

                    if faction_cos.is_empty() {
                        ui.label(egui::RichText::new("No commanders available for this faction")
                            .color(egui::Color32::GRAY));
                    } else {
                        for co_id in faction_cos {
                            let co = co_id.get_commander();
                            let is_selected = setup_state.player_co == Some(co_id);

                            let faction_color = setup_state.player_faction.color().to_srgba();
                            let button_color = if is_selected {
                                egui::Color32::from_rgb(
                                    (faction_color.red * 255.0) as u8,
                                    (faction_color.green * 255.0) as u8,
                                    (faction_color.blue * 255.0) as u8,
                                )
                            } else {
                                egui::Color32::from_rgb(60, 60, 60)
                            };

                            ui.horizontal(|ui| {
                                if ui.add(egui::Button::new(
                                    egui::RichText::new(if is_selected { "▶" } else { "  " })
                                ).fill(button_color).min_size(egui::vec2(24.0, 24.0))).clicked() {
                                    setup_state.player_co = Some(co_id);
                                }

                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new(co.name).size(13.0).strong());
                                        ui.label(egui::RichText::new(format!("- {}", co.power.name))
                                            .size(11.0).color(egui::Color32::from_rgb(255, 200, 50)));
                                    });

                                    let bonuses_str = format_co_bonuses(&co);
                                    if !bonuses_str.is_empty() {
                                        ui.label(egui::RichText::new(bonuses_str)
                                            .size(10.0).color(egui::Color32::from_rgb(150, 200, 150)));
                                    }
                                });
                            });
                            ui.add_space(4.0);
                        }
                    }
                });
            });

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(10.0);

            // Start button
            ui.horizontal(|ui| {
                let can_start = setup_state.player_co.is_some();

                ui.add_enabled_ui(can_start, |ui| {
                    if ui.add(egui::Button::new(egui::RichText::new("Start Battle!").size(18.0).strong())
                        .min_size(egui::vec2(200.0, 45.0))).clicked()
                    {
                        if let Some(player_co) = setup_state.player_co {
                            let player_faction = setup_state.player_faction;

                            // Set player CO
                            commanders.set_commander(player_faction, player_co);

                            // Determine AI faction (opposite of player)
                            let ai_faction = match player_faction {
                                Faction::Eastern => Faction::Northern,
                                Faction::Northern => Faction::Eastern,
                                Faction::Western => Faction::Southern,
                                Faction::Southern => Faction::Western,
                                Faction::Nether => Faction::Eastern,  // Nether vs everyone
                                Faction::Wanderer => Faction::Northern,
                            };

                            // Assign random CO to AI
                            let ai_cos = CommanderId::for_faction(ai_faction);
                            if !ai_cos.is_empty() {
                                let ai_co = ai_cos[rand::random::<usize>() % ai_cos.len()];
                                commanders.set_commander(ai_faction, ai_co);

                                info!("Battle started! Map: {}, Player ({:?}): {:?}, AI ({:?}): {:?}",
                                    setup_state.selected_map.name(),
                                    player_faction, player_co,
                                    ai_faction, ai_co);
                            }

                            // Load and spawn the selected map
                            let map_data = get_builtin_map(setup_state.selected_map);
                            spawn_map_from_data(&mut commands, &mut game_map, &mut meshes, &mut materials, &sprite_assets, &images, &map_data);
                            spawn_units_from_data(&mut commands, &game_map, &mut meshes, &mut materials, &sprite_assets, &images, &map_data);

                            setup_state.needs_setup = false;
                        }
                    }
                });

                if !can_start {
                    ui.label(egui::RichText::new("Select a commander to start")
                        .size(12.0).weak());
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
    selection_state: Res<BattleSetupState>,
    weather: Res<Weather>,
) {
    // Don't show battle UI controls if game is over (victory screen handles it)
    if game_result.game_over {
        return;
    }

    // Don't show if battle setup is pending
    if selection_state.needs_setup {
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

            ui.separator();

            // Weather display
            let weather_color = match weather.current {
                WeatherType::Clear => egui::Color32::from_rgb(255, 220, 100),     // Sunny yellow
                WeatherType::Rain => egui::Color32::from_rgb(100, 150, 200),      // Rainy blue
                WeatherType::Snow => egui::Color32::from_rgb(200, 220, 255),      // Icy white-blue
                WeatherType::Sandstorm => egui::Color32::from_rgb(200, 150, 80),  // Sandy brown
                WeatherType::Fog => egui::Color32::from_rgb(150, 150, 160),       // Misty gray
            };
            ui.label(egui::RichText::new(format!("{} {}", weather.current.icon(), weather.current.name()))
                .color(weather_color)
                .strong());

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
            ui.label("Click: Select/Move | WASD/Arrows: Pan camera | Space: Select/Confirm | ESC: Menu | F: Toggle Fog");
        });
    });
}

/// Handle ESC key to toggle in-game menu
fn handle_ingame_menu_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut menu_state: ResMut<InGameMenuState>,
    mut pending_action: ResMut<PendingAction>,
    mut highlights: ResMut<MovementHighlights>,
    setup_state: Res<BattleSetupState>,
    game_result: Res<GameResult>,
) {
    // Don't handle menu during setup or victory screen
    if setup_state.needs_setup || game_result.game_over {
        return;
    }

    if keyboard.just_pressed(KeyCode::Escape) {
        // If there's an active action/selection, cancel it first
        if pending_action.unit.is_some() || highlights.selected_unit.is_some() {
            pending_action.unit = None;
            pending_action.targets.clear();
            pending_action.can_capture = false;
            pending_action.capture_tile = None;
            pending_action.can_join = false;
            pending_action.join_target = None;
            highlights.selected_unit = None;
            highlights.tiles.clear();
            highlights.attack_targets.clear();
        } else {
            // Otherwise toggle the in-game menu
            menu_state.open = !menu_state.open;
        }
    }
}

/// Draw the in-game menu (Advance Wars style)
fn draw_ingame_menu(
    mut contexts: EguiContexts,
    mut menu_state: ResMut<InGameMenuState>,
    mut next_state: ResMut<NextState<GameState>>,
    mut fog: ResMut<FogOfWar>,
    setup_state: Res<BattleSetupState>,
    game_result: Res<GameResult>,
) {
    // Don't show during setup or victory screen
    if setup_state.needs_setup || game_result.game_over {
        return;
    }

    if !menu_state.open {
        return;
    }

    // Darken background
    egui::Area::new(egui::Id::new("menu_backdrop"))
        .fixed_pos(egui::pos2(0.0, 0.0))
        .order(egui::Order::Middle)
        .show(contexts.ctx_mut(), |ui| {
            let screen = ui.ctx().screen_rect();
            ui.painter().rect_filled(
                screen,
                0.0,
                egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180),
            );
        });

    // Center menu window
    egui::Window::new("Menu")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .min_width(250.0)
        .show(contexts.ctx_mut(), |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);

                let button_size = egui::vec2(200.0, 40.0);

                // Resume button
                if ui.add(egui::Button::new(egui::RichText::new("Resume").size(18.0)).min_size(button_size)).clicked() {
                    menu_state.open = false;
                }

                ui.add_space(8.0);

                // Toggle Fog of War
                let fog_text = if fog.enabled { "Fog of War: ON" } else { "Fog of War: OFF" };
                if ui.add(egui::Button::new(egui::RichText::new(fog_text).size(18.0)).min_size(button_size)).clicked() {
                    fog.enabled = !fog.enabled;
                }

                ui.add_space(8.0);

                // Unit Guide (placeholder)
                if ui.add(egui::Button::new(egui::RichText::new("Unit Guide").size(18.0)).min_size(button_size)).clicked() {
                    // TODO: Show unit guide
                }

                ui.add_space(8.0);

                // Options (placeholder)
                if ui.add(egui::Button::new(egui::RichText::new("Options").size(18.0)).min_size(button_size)).clicked() {
                    // TODO: Show options menu
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);

                // Surrender button - highlighted in red
                if ui.add(egui::Button::new(
                    egui::RichText::new("Surrender")
                        .size(18.0)
                        .color(egui::Color32::from_rgb(255, 100, 100))
                ).min_size(button_size)).clicked() {
                    menu_state.open = false;
                    next_state.set(GameState::Menu);
                }

                ui.add_space(10.0);
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
    mut join_events: EventWriter<JoinEvent>,
    mut resupply_events: EventWriter<ResupplyEvent>,
    mut load_events: EventWriter<LoadEvent>,
    mut unload_events: EventWriter<UnloadEvent>,
    map: Res<GameMap>,
    game_result: Res<GameResult>,
    commanders: Res<Commanders>,
    weather: Res<Weather>,
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

                // Calculate damage estimate (with CO bonuses and weather)
                let defender_terrain = map.get(pos_xy.0, pos_xy.1).unwrap_or(Terrain::Grass);
                let damage = calculate_damage(&attacker_unit, &unit, defender_terrain, &attacker_co, &defender_co, &weather);

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
                        Some(calculate_damage(&temp_defender, &attacker_unit, attacker_terrain, &defender_co, &attacker_co, &weather))
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
    let mut join_clicked = false;
    let mut resupply_clicked = false;
    let mut load_target: Option<(i32, i32)> = None;  // Position of transport to load into
    let mut unload_position: Option<(i32, i32)> = None;
    let mut wait_clicked = false;

    // Check if this is a Supplier unit that can resupply
    let is_supplier = attacker_unit.unit_type == UnitType::Supplier;

    // Check if this unit can be loaded into a transport
    let can_be_transported = attacker_unit.can_be_transported();

    // Check if this is a transport with cargo
    let is_transport_with_cargo = attacker_unit.is_transport() && attacker_unit.has_cargo();
    let cargo_name = attacker_unit.cargo.as_ref().map(|c| c.unit_type.name().to_string());

    // Find adjacent transports that can load this unit
    // We need to find them differently since we can't get Entity from iter()
    // For now, store position and transport name, and we'll find the entity when loading
    let adjacent_transport_info: Vec<((i32, i32), String)> = if can_be_transported {
        let adjacent = [
            (attacker_pos.0 - 1, attacker_pos.1),
            (attacker_pos.0 + 1, attacker_pos.1),
            (attacker_pos.0, attacker_pos.1 - 1),
            (attacker_pos.0, attacker_pos.1 + 1),
        ];

        let mut result = vec![];
        for (unit, faction, pos) in units.iter() {
            let Some(pos) = pos else { continue };
            let Some(faction) = faction else { continue };

            // Check same faction
            if faction.faction != turn_state.current_faction {
                continue;
            }

            // Check is empty transport
            if !unit.is_transport() || unit.has_cargo() {
                continue;
            }

            // Check if at one of the adjacent positions
            if adjacent.contains(&(pos.x, pos.y)) {
                result.push(((pos.x, pos.y), unit.unit_type.name().to_string()));
            }
        }
        result
    } else {
        vec![]
    };

    // Find valid unload positions for transports
    let unload_positions: Vec<(i32, i32)> = if is_transport_with_cargo {
        let adjacent = [
            (attacker_pos.0 - 1, attacker_pos.1),
            (attacker_pos.0 + 1, attacker_pos.1),
            (attacker_pos.0, attacker_pos.1 - 1),
            (attacker_pos.0, attacker_pos.1 + 1),
        ];

        // Get all unit positions
        let occupied: std::collections::HashSet<(i32, i32)> = units.iter()
            .filter_map(|(_, _, pos)| pos.map(|p| (p.x, p.y)))
            .collect();

        adjacent.iter()
            .filter(|(x, y)| {
                // Check on map
                if *x < 0 || *y < 0 || *x >= map.width as i32 || *y >= map.height as i32 {
                    return false;
                }
                // Check passable terrain
                let terrain = map.get(*x, *y).unwrap_or(Terrain::Grass);
                if terrain.movement_cost() >= 99 {
                    return false;
                }
                // Check not occupied
                !occupied.contains(&(*x, *y))
            })
            .copied()
            .collect()
    } else {
        vec![]
    };

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

            // Show join option if available
            if pending_action.can_join {
                ui.heading("Join");
                ui.separator();

                ui.label("Merge with allied unit");
                ui.label(egui::RichText::new("Combines HP, Stamina, Ammo")
                    .color(egui::Color32::from_rgb(150, 200, 150)));

                if ui.add(egui::Button::new(egui::RichText::new("Join").size(14.0))
                    .min_size(egui::vec2(180.0, 28.0))).clicked()
                {
                    join_clicked = true;
                }

                ui.separator();
            }

            // Show resupply option for Supplier units
            if is_supplier {
                ui.heading("Resupply");
                ui.separator();

                ui.label("Resupply adjacent friendly units");
                ui.label(egui::RichText::new("Restores Stamina & Ammo to max")
                    .color(egui::Color32::from_rgb(150, 200, 255)));

                if ui.add(egui::Button::new(egui::RichText::new("Resupply").size(14.0))
                    .min_size(egui::vec2(180.0, 28.0))).clicked()
                {
                    resupply_clicked = true;
                }

                ui.separator();
            }

            // Show load option for foot units near transports
            if !adjacent_transport_info.is_empty() {
                ui.heading("Load");
                ui.separator();

                for (transport_pos, transport_name) in &adjacent_transport_info {
                    ui.label(format!("Board {}", transport_name));
                    if ui.add(egui::Button::new(egui::RichText::new(format!("Load into {}", transport_name)).size(14.0))
                        .min_size(egui::vec2(180.0, 28.0))).clicked()
                    {
                        load_target = Some(*transport_pos);
                    }
                }

                ui.separator();
            }

            // Show unload option for transports with cargo
            if is_transport_with_cargo && !unload_positions.is_empty() {
                ui.heading("Unload");
                ui.separator();

                if let Some(cargo_name) = &cargo_name {
                    ui.label(format!("Carrying: {}", cargo_name));
                }

                for (ux, uy) in &unload_positions {
                    let terrain = map.get(*ux, *uy).unwrap_or(Terrain::Grass);
                    if ui.add(egui::Button::new(egui::RichText::new(format!("Unload to ({}, {}) - {}", ux, uy, terrain.name())).size(12.0))
                        .min_size(egui::vec2(180.0, 24.0))).clicked()
                    {
                        unload_position = Some((*ux, *uy));
                    }
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
        pending_action.can_join = false;
        pending_action.join_target = None;
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
            pending_action.can_join = false;
            pending_action.join_target = None;
            turn_state.phase = TurnPhase::Select;
        }
    } else if join_clicked {
        if let Some(join_target) = pending_action.join_target {
            join_events.send(JoinEvent {
                source: acting_entity,
                target: join_target,
            });

            // The source unit will be despawned by the join system
            // No need to mark as attacked since it will be gone

            pending_action.unit = None;
            pending_action.targets.clear();
            pending_action.can_capture = false;
            pending_action.capture_tile = None;
            pending_action.can_join = false;
            pending_action.join_target = None;
            turn_state.phase = TurnPhase::Select;
        }
    } else if resupply_clicked {
        // Send resupply event - the system will handle finding and resupplying adjacent units
        resupply_events.send(ResupplyEvent {
            supplier: acting_entity,
        });

        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        turn_state.phase = TurnPhase::Select;
    } else if let Some(transport_pos) = load_target {
        // Send load event with transport position - the handler will find the transport
        load_events.send(LoadEvent {
            transport_pos,
            passenger: acting_entity,
        });

        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        turn_state.phase = TurnPhase::Select;
    } else if let Some(pos) = unload_position {
        // Send unload event
        unload_events.send(UnloadEvent {
            transport: acting_entity,
            position: pos,
        });

        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
        turn_state.phase = TurnPhase::Select;
    } else if wait_clicked {
        pending_action.unit = None;
        pending_action.targets.clear();
        pending_action.can_capture = false;
        pending_action.capture_tile = None;
        pending_action.can_join = false;
        pending_action.join_target = None;
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
    sprite_assets: Res<SpriteAssets>,
    images: Res<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
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
                &mut meshes,
                &mut materials,
                &sprite_assets,
                &images,
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

/// Draw HP numbers on damaged units (Advance Wars style: 1-9)
fn draw_unit_hp_numbers(
    mut contexts: EguiContexts,
    units: Query<(Entity, &Unit, &GlobalTransform, &FactionMember)>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    setup_state: Res<BattleSetupState>,
    game_result: Res<GameResult>,
    fog: Res<FogOfWar>,
) {
    // Don't draw during setup or if game is over
    if setup_state.needs_setup || game_result.game_over {
        return;
    }

    let Ok((camera, camera_transform)) = camera.get_single() else { return };

    for (entity, unit, unit_transform, faction) in units.iter() {
        // Check fog visibility for enemy units
        if fog.enabled && faction.faction != Faction::Eastern {
            // Get grid position from world position
            let world_pos = unit_transform.translation();
            let grid_x = (world_pos.x / TILE_SIZE).round() as i32;
            let grid_y = (world_pos.z / TILE_SIZE).round() as i32;
            if !fog.is_visible(grid_x, grid_y) {
                continue;
            }
        }

        // Calculate HP display (1-9, don't show if 10 = full health)
        let hp_display = (unit.hp as f32 / 10.0).ceil() as i32;
        if hp_display >= 10 || hp_display <= 0 {
            continue; // Full health or dead, no display
        }

        // Convert world position to screen position
        let world_pos = unit_transform.translation();
        let Ok(screen_pos) = camera.world_to_viewport(camera_transform, world_pos) else {
            continue;
        };

        // Position at bottom-right of unit sprite
        let offset_x = 8.0;
        let offset_y = 8.0;

        // Draw HP number with dark background for visibility
        egui::Area::new(egui::Id::new(("hp_num", entity)))
            .fixed_pos(egui::pos2(screen_pos.x + offset_x, screen_pos.y + offset_y))
            .order(egui::Order::Foreground)
            .show(contexts.ctx_mut(), |ui| {
                // Small dark background
                let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgba_unmultiplied(0, 0, 0, 180));

                // HP number in white
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{}", hp_display),
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
            });
    }
}

// ============================================================================
// UNIT TOOLTIP
// ============================================================================

/// Track which unit is under the mouse cursor
fn track_hovered_unit(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    units: Query<(Entity, &GridPosition, &Unit, &FactionMember)>,
    map: Res<GameMap>,
    mut hovered: ResMut<HoveredUnit>,
    fog: Res<FogOfWar>,
) {
    let Ok(window) = windows.get_single() else { return };
    let Ok((camera, camera_transform)) = cameras.get_single() else { return };

    let Some(cursor_pos) = window.cursor_position() else {
        hovered.entity = None;
        return;
    };

    // Convert screen position to grid coordinates using ray-plane intersection
    let Some(grid_pos) = screen_to_grid(window, camera, camera_transform, &map) else {
        hovered.entity = None;
        return;
    };
    let grid_x = grid_pos.x;
    let grid_y = grid_pos.y;

    // Find unit at this position
    hovered.entity = None;
    for (entity, pos, _unit, faction) in units.iter() {
        if pos.x == grid_x && pos.y == grid_y {
            // Check fog of war - only show tooltip for visible units
            if fog.enabled && faction.faction != Faction::Eastern && !fog.is_visible(pos.x, pos.y) {
                continue;
            }
            hovered.entity = Some(entity);
            hovered.screen_pos = (cursor_pos.x, cursor_pos.y);
            break;
        }
    }
}

/// Track hovered tile for terrain info panel (only when not hovering a unit)
fn track_hovered_tile(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform)>,
    map: Res<GameMap>,
    hovered_unit: Res<HoveredUnit>,
    mut selected_tile: ResMut<SelectedTile>,
    fog: Res<FogOfWar>,
) {
    // Don't track tile if hovering a unit (unit tooltip takes priority)
    if hovered_unit.entity.is_some() {
        selected_tile.position = None;
        return;
    }

    let Ok(window) = windows.get_single() else { return };
    let Ok((camera, camera_transform)) = cameras.get_single() else { return };

    let Some(cursor_pos) = window.cursor_position() else {
        selected_tile.position = None;
        return;
    };

    // Convert screen position to grid coordinates
    let Some(grid_pos) = screen_to_grid(window, camera, camera_transform, &map) else {
        selected_tile.position = None;
        return;
    };

    // Check fog of war - only show for visible/explored tiles
    if fog.enabled {
        let visibility = fog.get_visibility(grid_pos.x, grid_pos.y);
        if visibility == crate::game::TileVisibility::Unexplored {
            selected_tile.position = None;
            return;
        }
    }

    selected_tile.position = Some(grid_pos);
    selected_tile.screen_pos = (cursor_pos.x, cursor_pos.y);
}

/// Draw terrain info panel when hovering terrain (no unit)
fn draw_terrain_info_panel(
    mut contexts: EguiContexts,
    selected_tile: Res<SelectedTile>,
    map: Res<GameMap>,
    tiles: Query<&Tile>,
    fog: Res<FogOfWar>,
) {
    let Some(pos) = selected_tile.position else { return };

    // Get terrain at position
    let Some(terrain) = map.get(pos.x, pos.y) else { return };

    // Find tile entity for owner info
    let mut tile_owner: Option<Faction> = None;
    for tile in tiles.iter() {
        if tile.position.x == pos.x && tile.position.y == pos.y {
            tile_owner = tile.owner;
            break;
        }
    }

    // Check if fogged (show limited info)
    let is_fogged = fog.enabled && fog.get_visibility(pos.x, pos.y) == crate::game::TileVisibility::Fogged;

    egui::Window::new("terrain_info")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::LEFT_BOTTOM, [10.0, -10.0])
        .frame(egui::Frame::popup(&contexts.ctx_mut().style()))
        .show(contexts.ctx_mut(), |ui| {
            ui.set_min_width(150.0);

            // Terrain name
            let terrain_color = terrain.color().to_srgba();
            let color = egui::Color32::from_rgb(
                (terrain_color.red * 255.0).min(255.0) as u8,
                (terrain_color.green * 255.0).min(255.0) as u8,
                (terrain_color.blue * 255.0).min(255.0) as u8,
            );
            ui.label(egui::RichText::new(terrain.name())
                .size(14.0)
                .strong()
                .color(color));

            ui.separator();

            // Stats
            egui::Grid::new("terrain_stats")
                .num_columns(2)
                .spacing([20.0, 2.0])
                .show(ui, |ui| {
                    // Defense bonus
                    let def_bonus = terrain.defense_bonus();
                    ui.label("Defense:");
                    if def_bonus > 0 {
                        ui.label(egui::RichText::new(format!("+{}★", def_bonus))
                            .color(egui::Color32::from_rgb(120, 180, 255)));
                    } else {
                        ui.label("--");
                    }
                    ui.end_row();

                    // Movement cost
                    ui.label("Move cost:");
                    let base_cost = terrain.movement_cost();
                    if base_cost >= 99 {
                        ui.label(egui::RichText::new("Impassable")
                            .color(egui::Color32::from_rgb(180, 80, 80)));
                    } else {
                        ui.label(format!("{}", base_cost));
                    }
                    ui.end_row();

                    // Owner (for capturable terrain)
                    if terrain.is_capturable() && !is_fogged {
                        ui.label("Owner:");
                        if let Some(owner) = tile_owner {
                            let owner_color = owner.color().to_srgba();
                            ui.label(egui::RichText::new(owner.name())
                                .color(egui::Color32::from_rgb(
                                    (owner_color.red * 255.0) as u8,
                                    (owner_color.green * 255.0) as u8,
                                    (owner_color.blue * 255.0) as u8,
                                )));
                        } else {
                            ui.label(egui::RichText::new("Neutral")
                                .color(egui::Color32::GRAY));
                        }
                        ui.end_row();
                    }

                    // Income (for property tiles)
                    if terrain.is_capturable() {
                        ui.label("Income:");
                        let income = terrain.income_value();
                        if income > 0 {
                            ui.label(egui::RichText::new(format!("+{}", income))
                                .color(egui::Color32::from_rgb(255, 200, 80)));
                        } else {
                            ui.label("--");
                        }
                        ui.end_row();
                    }

                    // Can produce units (Base)
                    if terrain == Terrain::Base {
                        ui.label("Production:");
                        ui.label(egui::RichText::new("Yes")
                            .color(egui::Color32::from_rgb(80, 200, 80)));
                        ui.end_row();
                    }
                });

            // Position
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("({}, {})", pos.x, pos.y))
                    .size(10.0)
                    .weak());
            });

            // Fogged indicator
            if is_fogged {
                ui.add_space(2.0);
                ui.label(egui::RichText::new("(Last seen)")
                    .size(10.0)
                    .weak()
                    .italics());
            }
        });
}

/// Draw tooltip for hovered unit
fn draw_unit_tooltip(
    mut contexts: EguiContexts,
    hovered: Res<HoveredUnit>,
    units: Query<(&Unit, &FactionMember, &GridPosition)>,
    map: Res<GameMap>,
    commanders: Res<Commanders>,
    weather: Res<Weather>,
) {
    let Some(entity) = hovered.entity else { return };
    let Ok((unit, faction, pos)) = units.get(entity) else { return };

    let stats = unit.unit_type.stats();
    let co_bonuses = commanders.get_bonuses(faction.faction);
    let terrain = map.get(pos.x, pos.y).unwrap_or(Terrain::Grass);

    // Calculate effective stats with CO and weather bonuses
    let base_movement = (stats.movement as i32 + co_bonuses.movement).max(1) as u32;
    let effective_movement = weather.apply_movement(base_movement);
    let base_vision = stats.vision + co_bonuses.vision;
    let effective_vision = weather.apply_vision(base_vision);

    // Attack with CO bonus
    let effective_attack = (stats.attack as f32 * co_bonuses.attack * weather.effects().attack_multiplier).round() as u32;
    // Defense with CO bonus + terrain
    let terrain_def = terrain.defense_bonus() as u32;
    let effective_defense = (stats.defense as f32 * co_bonuses.defense * weather.effects().defense_multiplier).round() as u32 + terrain_def * 10;

    // Faction color
    let faction_color = faction.faction.color().to_srgba();
    let color = egui::Color32::from_rgb(
        (faction_color.red * 255.0) as u8,
        (faction_color.green * 255.0) as u8,
        (faction_color.blue * 255.0) as u8,
    );

    egui::Window::new("unit_tooltip")
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::LEFT_BOTTOM, [10.0, -10.0])
        .frame(egui::Frame::popup(&contexts.ctx_mut().style()))
        .show(contexts.ctx_mut(), |ui| {
            ui.set_min_width(180.0);

            // Unit name and faction
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(unit.unit_type.name())
                    .size(14.0)
                    .strong()
                    .color(color));
                ui.label(egui::RichText::new(format!("({})", faction.faction.name()))
                    .size(10.0)
                    .weak());
            });

            ui.separator();

            // HP bar
            let hp_ratio = unit.hp as f32 / stats.max_hp as f32;
            let hp_color = if hp_ratio > 0.6 {
                egui::Color32::from_rgb(80, 200, 80)
            } else if hp_ratio > 0.3 {
                egui::Color32::from_rgb(220, 180, 50)
            } else {
                egui::Color32::from_rgb(220, 80, 80)
            };
            ui.horizontal(|ui| {
                ui.label("HP:");
                ui.add(egui::ProgressBar::new(hp_ratio)
                    .fill(hp_color)
                    .text(format!("{}/{}", unit.hp, stats.max_hp)));
            });

            // Stamina bar
            if stats.max_stamina > 0 {
                let stamina_ratio = unit.stamina as f32 / stats.max_stamina as f32;
                let stamina_color = if stamina_ratio > 0.5 {
                    egui::Color32::from_rgb(80, 180, 220)
                } else if stamina_ratio > 0.25 {
                    egui::Color32::from_rgb(220, 180, 50)
                } else {
                    egui::Color32::from_rgb(220, 80, 80)
                };
                ui.horizontal(|ui| {
                    ui.label("Stamina:");
                    ui.add(egui::ProgressBar::new(stamina_ratio)
                        .fill(stamina_color)
                        .text(format!("{}/{}", unit.stamina, stats.max_stamina)));
                });
            }

            // Ammo bar (only show if unit has ammo)
            if stats.max_ammo > 0 {
                let ammo_ratio = unit.ammo as f32 / stats.max_ammo as f32;
                let ammo_color = if ammo_ratio > 0.5 {
                    egui::Color32::from_rgb(200, 160, 80)
                } else if ammo_ratio > 0.25 {
                    egui::Color32::from_rgb(220, 120, 50)
                } else {
                    egui::Color32::from_rgb(220, 80, 80)
                };
                ui.horizontal(|ui| {
                    ui.label("Ammo:");
                    ui.add(egui::ProgressBar::new(ammo_ratio)
                        .fill(ammo_color)
                        .text(format!("{}/{}", unit.ammo, stats.max_ammo)));
                });
            }

            // Low resource warnings
            let low_stamina = stats.max_stamina > 0 && unit.stamina <= stats.max_stamina / 3;
            let low_ammo = stats.max_ammo > 0 && unit.ammo <= stats.max_ammo / 3;
            let no_ammo = stats.max_ammo > 0 && unit.ammo == 0;

            if low_stamina || low_ammo || no_ammo {
                ui.add_space(4.0);
                ui.separator();
                let warning_color = egui::Color32::from_rgb(255, 180, 60);
                let critical_color = egui::Color32::from_rgb(255, 80, 80);

                if no_ammo {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("! NO AMMO !").color(critical_color).strong());
                    });
                    ui.label(egui::RichText::new("Cannot attack").color(critical_color).small());
                } else if low_ammo {
                    ui.label(egui::RichText::new("! Low Ammo").color(warning_color).strong());
                }

                if low_stamina {
                    let stamina_warning = if unit.stamina == 0 {
                        ui.label(egui::RichText::new("! EXHAUSTED !").color(critical_color).strong());
                        "Cannot move"
                    } else {
                        ui.label(egui::RichText::new("! Low Stamina").color(warning_color).strong());
                        "Limited movement"
                    };
                    ui.label(egui::RichText::new(stamina_warning).color(warning_color).small());
                }

                // Suggest resupply
                if (low_stamina || low_ammo) && !no_ammo {
                    ui.add_space(2.0);
                    ui.label(egui::RichText::new("Move to Base/Storehouse to resupply")
                        .color(egui::Color32::from_rgb(150, 200, 255)).small().italics());
                }
            }

            ui.add_space(4.0);

            // Stats grid
            egui::Grid::new("stats_grid")
                .num_columns(2)
                .spacing([20.0, 2.0])
                .show(ui, |ui| {
                    // Attack
                    ui.label("Attack:");
                    let atk_text = if effective_attack != stats.attack as u32 {
                        format!("{} ({})", effective_attack, stats.attack)
                    } else {
                        format!("{}", stats.attack)
                    };
                    ui.label(egui::RichText::new(atk_text).color(egui::Color32::from_rgb(255, 120, 120)));
                    ui.end_row();

                    // Defense
                    ui.label("Defense:");
                    let def_bonus = if terrain_def > 0 { format!(" +{}", terrain_def * 10) } else { String::new() };
                    ui.label(egui::RichText::new(format!("{}{}", effective_defense, def_bonus))
                        .color(egui::Color32::from_rgb(120, 180, 255)));
                    ui.end_row();

                    // Movement
                    ui.label("Movement:");
                    let mov_text = if effective_movement != stats.movement {
                        format!("{} ({})", effective_movement, stats.movement)
                    } else {
                        format!("{}", stats.movement)
                    };
                    ui.label(egui::RichText::new(mov_text).color(egui::Color32::from_rgb(120, 255, 120)));
                    ui.end_row();

                    // Vision
                    ui.label("Vision:");
                    let vis_text = if effective_vision != stats.vision {
                        format!("{} ({})", effective_vision, stats.vision)
                    } else {
                        format!("{}", stats.vision)
                    };
                    ui.label(egui::RichText::new(vis_text).color(egui::Color32::from_rgb(255, 255, 120)));
                    ui.end_row();

                    // Range
                    ui.label("Range:");
                    let (min_r, max_r) = stats.attack_range;
                    let range_text = if min_r == max_r {
                        format!("{}", min_r)
                    } else {
                        format!("{}-{}", min_r, max_r)
                    };
                    ui.label(range_text);
                    ui.end_row();

                    // Cost
                    ui.label("Cost:");
                    let adjusted_cost = (stats.cost as f32 * co_bonuses.cost).round() as u32;
                    let cost_text = if adjusted_cost != stats.cost {
                        format!("{} ({})", adjusted_cost, stats.cost)
                    } else {
                        format!("{}", stats.cost)
                    };
                    ui.label(egui::RichText::new(cost_text).color(egui::Color32::from_rgb(255, 200, 80)));
                    ui.end_row();
                });

            // Terrain info
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Terrain:").weak());
                ui.label(format!("{}", terrain.name()));
                if terrain_def > 0 {
                    ui.label(egui::RichText::new(format!("(+{} def)", terrain_def * 10))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(120, 180, 255)));
                }
            });

            // Status indicators
            if unit.moved || unit.attacked {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if unit.moved {
                        ui.label(egui::RichText::new("Moved")
                            .size(10.0)
                            .color(egui::Color32::GRAY));
                    }
                    if unit.attacked {
                        ui.label(egui::RichText::new("Attacked")
                            .size(10.0)
                            .color(egui::Color32::GRAY));
                    }
                });
            }
        });
}

// ============================================================================
// MAP EDITOR
// ============================================================================

/// Marker component for editor tile sprites
#[derive(Component)]
struct EditorTile {
    x: i32,
    y: i32,
}

/// Marker component for editor unit sprites
#[derive(Component)]
struct EditorUnit;

/// Setup editor when entering editor state
fn setup_editor(
    mut commands: Commands,
    mut editor_state: ResMut<EditorState>,
) {
    // Reset to a fresh map
    *editor_state = EditorState::default();
    spawn_editor_tiles(&mut commands, &editor_state.map);
}

/// Cleanup editor entities when leaving
fn cleanup_editor(
    mut commands: Commands,
    tiles: Query<Entity, With<EditorTile>>,
    units: Query<Entity, With<EditorUnit>>,
) {
    for entity in tiles.iter() {
        commands.entity(entity).despawn();
    }
    for entity in units.iter() {
        commands.entity(entity).despawn();
    }
}

/// Spawn editor tile sprites
fn spawn_editor_tiles(commands: &mut Commands, map: &MapData) {
    let offset_x = -(map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_y = -(map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    for y in 0..map.height {
        for x in 0..map.width {
            let terrain = map.terrain[y as usize][x as usize];
            let world_x = x as f32 * TILE_SIZE + offset_x;
            let world_y = y as f32 * TILE_SIZE + offset_y;

            commands.spawn((
                Sprite {
                    color: terrain.color(),
                    custom_size: Some(Vec2::splat(TILE_SIZE - 2.0)),
                    ..default()
                },
                Transform::from_xyz(world_x, world_y, 0.0),
                EditorTile { x: x as i32, y: y as i32 },
            ));
        }
    }
}

/// Draw the editor UI
fn draw_editor(
    mut contexts: EguiContexts,
    mut editor_state: ResMut<EditorState>,
    mut next_state: ResMut<NextState<GameState>>,
    mut commands: Commands,
    tiles: Query<Entity, With<EditorTile>>,
    units: Query<Entity, With<EditorUnit>>,
) {
    let mut should_respawn = false;
    let mut should_save = false;

    // Left panel - Tools
    egui::SidePanel::left("editor_tools").min_width(200.0).show(contexts.ctx_mut(), |ui| {
        ui.heading("Map Editor");
        ui.separator();

        // Map name
        ui.label("Map Name:");
        ui.text_edit_singleline(&mut editor_state.map.name);
        ui.add_space(5.0);

        // Map dimensions
        ui.label("Dimensions:");
        ui.horizontal(|ui| {
            ui.label("W:");
            let mut width = editor_state.map.width;
            if ui.add(egui::DragValue::new(&mut width).range(8..=20)).changed() {
                if width != editor_state.map.width {
                    editor_state.map = MapData::new(&editor_state.map.name, width, editor_state.map.height);
                    should_respawn = true;
                }
            }
            ui.label("H:");
            let mut height = editor_state.map.height;
            if ui.add(egui::DragValue::new(&mut height).range(6..=16)).changed() {
                if height != editor_state.map.height {
                    editor_state.map = MapData::new(&editor_state.map.name, editor_state.map.width, height);
                    should_respawn = true;
                }
            }
        });
        ui.add_space(10.0);

        ui.separator();

        // Edit mode selection
        ui.label("Edit Mode:");
        ui.horizontal(|ui| {
            ui.selectable_value(&mut editor_state.mode, EditorMode::Terrain, "Terrain");
            ui.selectable_value(&mut editor_state.mode, EditorMode::Units, "Units");
            ui.selectable_value(&mut editor_state.mode, EditorMode::Properties, "Properties");
        });
        ui.add_space(10.0);

        ui.separator();

        // Mode-specific options
        match editor_state.mode {
            EditorMode::Terrain => {
                ui.label("Terrain Brush:");
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    let terrains = [
                        Terrain::Grass, Terrain::TallGrass, Terrain::Thicket,
                        Terrain::Brambles, Terrain::Log, Terrain::Boulder,
                        Terrain::Hollow, Terrain::Creek, Terrain::Pond,
                        Terrain::Shore, Terrain::Base, Terrain::Outpost,
                        Terrain::Storehouse,
                    ];
                    for terrain in terrains {
                        let color = terrain.color().to_srgba();
                        let egui_color = egui::Color32::from_rgb(
                            (color.red * 255.0) as u8,
                            (color.green * 255.0) as u8,
                            (color.blue * 255.0) as u8,
                        );
                        let selected = editor_state.selected_terrain == terrain;
                        ui.horizontal(|ui| {
                            ui.add(egui::Button::new("").fill(egui_color).min_size(egui::vec2(20.0, 20.0)));
                            if ui.selectable_label(selected, terrain.name()).clicked() {
                                editor_state.selected_terrain = terrain;
                            }
                        });
                    }
                });
            }
            EditorMode::Units => {
                ui.label("Faction:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut editor_state.selected_faction, Faction::Eastern, "Eastern");
                    ui.selectable_value(&mut editor_state.selected_faction, Faction::Northern, "Northern");
                });
                ui.add_space(5.0);
                ui.label("Unit Type:");
                let unit_types = [
                    UnitType::Scout, UnitType::Shocktrooper, UnitType::Recon,
                    UnitType::Ironclad, UnitType::Siege,
                ];
                for unit_type in unit_types {
                    let selected = editor_state.selected_unit == unit_type;
                    if ui.selectable_label(selected, unit_type.name()).clicked() {
                        editor_state.selected_unit = unit_type;
                    }
                }
                ui.add_space(5.0);
                ui.label(format!("Units placed: {}", editor_state.map.units.len()));
            }
            EditorMode::Properties => {
                ui.label("Property Owner:");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut editor_state.selected_faction, Faction::Eastern, "Eastern");
                    ui.selectable_value(&mut editor_state.selected_faction, Faction::Northern, "Northern");
                });
                ui.add_space(5.0);
                ui.label("Click on Base/Outpost/Storehouse to set owner");
                ui.add_space(5.0);
                ui.label(format!("Properties set: {}", editor_state.map.properties.len()));
            }
        }

        ui.add_space(20.0);
        ui.separator();

        // File operations
        ui.label("File Name:");
        ui.text_edit_singleline(&mut editor_state.map_name);

        ui.add_space(5.0);

        if ui.button("Save Map").clicked() {
            should_save = true;
        }

        ui.add_space(5.0);

        if ui.button("Clear Map").clicked() {
            let name = editor_state.map.name.clone();
            let width = editor_state.map.width;
            let height = editor_state.map.height;
            editor_state.map = MapData::new(&name, width, height);
            should_respawn = true;
        }

        ui.add_space(20.0);

        if ui.button("Back to Menu").clicked() {
            next_state.set(GameState::Menu);
        }

        // Status message
        if !editor_state.status.is_empty() {
            ui.add_space(10.0);
            ui.label(egui::RichText::new(&editor_state.status).color(egui::Color32::YELLOW));
        }
    });

    // Right panel - Info
    egui::SidePanel::right("editor_info").min_width(150.0).show(contexts.ctx_mut(), |ui| {
        ui.heading("Info");
        ui.separator();

        ui.label(format!("Size: {}x{}", editor_state.map.width, editor_state.map.height));
        ui.add_space(5.0);

        ui.label("Controls:");
        ui.label("- Left click: Paint");
        ui.label("- Right click: Erase");
        ui.add_space(10.0);

        ui.separator();
        ui.label("Units:");
        let eastern_units = editor_state.map.units.iter().filter(|u| u.faction == Faction::Eastern).count();
        let northern_units = editor_state.map.units.iter().filter(|u| u.faction == Faction::Northern).count();
        ui.label(format!("  Eastern: {}", eastern_units));
        ui.label(format!("  Northern: {}", northern_units));

        ui.add_space(10.0);
        ui.separator();
        ui.label("Properties:");
        let eastern_props = editor_state.map.properties.iter().filter(|p| p.owner == Faction::Eastern).count();
        let northern_props = editor_state.map.properties.iter().filter(|p| p.owner == Faction::Northern).count();
        ui.label(format!("  Eastern: {}", eastern_props));
        ui.label(format!("  Northern: {}", northern_props));
    });

    // Handle respawn
    if should_respawn {
        for entity in tiles.iter() {
            commands.entity(entity).despawn();
        }
        for entity in units.iter() {
            commands.entity(entity).despawn();
        }
        spawn_editor_tiles(&mut commands, &editor_state.map);
    }

    // Handle save
    if should_save {
        let path = std::path::Path::new(&editor_state.map_name).with_extension("json");
        match editor_state.map.save_to_file(&path) {
            Ok(()) => {
                editor_state.status = format!("Saved to {}", path.display());
                info!("Map saved to {}", path.display());
            }
            Err(e) => {
                editor_state.status = format!("Save failed: {}", e);
                error!("Failed to save map: {}", e);
            }
        }
    }
}

/// Handle painting in the editor
fn editor_paint(
    mut editor_state: ResMut<EditorState>,
    mut tiles: Query<(&EditorTile, &mut Sprite)>,
    mouse_button: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
) {
    // Get mouse position in world coordinates using ray-plane intersection
    let Ok(window) = windows.get_single() else { return };
    let Ok((camera, camera_transform)) = camera.get_single() else { return };

    let Some(cursor_pos) = window.cursor_position() else { return };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else { return };

    // Intersect with Y=0 ground plane
    if ray.direction.y.abs() < 0.0001 { return; }
    let t = -ray.origin.y / ray.direction.y;
    if t < 0.0 { return; }
    let hit = ray.origin + ray.direction * t;

    // Convert to tile coordinates
    let offset_x = -(editor_state.map.width as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;
    let offset_z = -(editor_state.map.height as f32 * TILE_SIZE) / 2.0 + TILE_SIZE / 2.0;

    let tile_x = ((hit.x - offset_x + TILE_SIZE / 2.0) / TILE_SIZE).floor() as i32;
    let tile_y = ((hit.z - offset_z + TILE_SIZE / 2.0) / TILE_SIZE).floor() as i32;  // World Z -> Grid Y

    // Check bounds
    if tile_x < 0 || tile_y < 0 || tile_x >= editor_state.map.width as i32 || tile_y >= editor_state.map.height as i32 {
        return;
    }

    let left_pressed = mouse_button.pressed(MouseButton::Left);
    let right_pressed = mouse_button.pressed(MouseButton::Right);

    if !left_pressed && !right_pressed {
        return;
    }

    match editor_state.mode {
        EditorMode::Terrain => {
            let terrain = if left_pressed {
                editor_state.selected_terrain
            } else {
                Terrain::Grass
            };

            // Update map data
            editor_state.map.set_terrain(tile_x, tile_y, terrain);

            // Update tile sprite color
            for (tile, mut sprite) in tiles.iter_mut() {
                if tile.x == tile_x && tile.y == tile_y {
                    sprite.color = terrain.color();
                }
            }
        }
        EditorMode::Units => {
            if left_pressed && mouse_button.just_pressed(MouseButton::Left) {
                let unit_type = editor_state.selected_unit;
                let faction = editor_state.selected_faction;
                // Remove existing unit at this position
                editor_state.map.units.retain(|u| u.x != tile_x || u.y != tile_y);
                // Add new unit
                editor_state.map.units.push(UnitPlacement {
                    unit_type,
                    faction,
                    x: tile_x,
                    y: tile_y,
                });
            } else if right_pressed && mouse_button.just_pressed(MouseButton::Right) {
                // Remove unit at position
                editor_state.map.units.retain(|u| u.x != tile_x || u.y != tile_y);
            }
        }
        EditorMode::Properties => {
            let terrain = editor_state.map.get_terrain(tile_x, tile_y);
            if let Some(t) = terrain {
                if t.is_capturable() {
                    if left_pressed && mouse_button.just_pressed(MouseButton::Left) {
                        let faction = editor_state.selected_faction;
                        // Remove existing property at this position
                        editor_state.map.properties.retain(|p| p.x != tile_x || p.y != tile_y);
                        // Add new property
                        editor_state.map.properties.push(PropertyOwnership {
                            x: tile_x,
                            y: tile_y,
                            owner: faction,
                        });
                    } else if right_pressed && mouse_button.just_pressed(MouseButton::Right) {
                        // Remove property at position
                        editor_state.map.properties.retain(|p| p.x != tile_x || p.y != tile_y);
                    }
                }
            }
        }
    }
}
