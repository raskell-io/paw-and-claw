//! In-battle HUD: turn and commander cards, inspect card, unit tokens, control hints

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::theme;
use super::{BattleSetupState, HoveredUnit, InGameMenuState, PortraitTextures, SelectedTile};
use crate::game::{
    advance_turn, AiState, BattleConfig, Commanders, FactionFunds, FactionMember, FogOfWar, GameData, GameMap,
    GameResult, GridCursor, GridPosition, MovementHighlights, PowerActivatedEvent, Terrain, Tile,
    TileVisibility, TurnPhase, TurnStartEvent, TurnState, Unit, UnitClass, Weather, IMPASSABLE,
};

/// Where each unit appeared on screen last frame, so menus can sit next to it
#[derive(Resource, Default)]
pub struct UnitScreenPositions(pub std::collections::HashMap<Entity, egui::Pos2>);

/// Read-only battle context shared by HUD systems
#[derive(SystemParam)]
pub struct HudContext<'w> {
    pub funds: Res<'w, FactionFunds>,
    pub ai_state: Res<'w, AiState>,
    pub game_result: Res<'w, GameResult>,
    pub setup: Res<'w, BattleSetupState>,
    pub weather: Res<'w, Weather>,
    pub game_data: Res<'w, GameData>,
    pub battle_config: Res<'w, BattleConfig>,
    pub fog: Res<'w, FogOfWar>,
    pub menu: Res<'w, InGameMenuState>,
    pub time: Res<'w, Time>,
}

impl HudContext<'_> {
    /// Whether the battlefield is live (not in setup, not over)
    pub fn battle_active(&self) -> bool {
        !self.setup.needs_setup && !self.game_result.game_over
    }

    pub fn is_ai_turn(&self, turn_state: &TurnState) -> bool {
        self.ai_state.enabled && self.battle_config.is_ai_faction(turn_state.current_faction)
    }

    /// Whether the player can currently see a tile
    pub fn visible_to_player(&self, x: i32, y: i32) -> bool {
        !self.fog.enabled || self.fog.is_visible(x, y)
    }
}

/// End the current faction's turn: refresh its units and hand over to the next faction
pub fn end_turn<'a>(
    turn_state: &mut TurnState,
    highlights: &mut MovementHighlights,
    battle_config: &BattleConfig,
    units: impl Iterator<Item = (Mut<'a, Unit>, &'a FactionMember)>,
    turn_start_events: &mut MessageWriter<TurnStartEvent>,
) {
    for (mut unit, faction) in units {
        if faction.faction == turn_state.current_faction {
            unit.moved = false;
            unit.attacked = false;
            unit.exhausted = false;
        }
    }
    advance_turn(turn_state, battle_config, highlights);
    turn_start_events.write(TurnStartEvent { faction: turn_state.current_faction });
}

pub fn draw_hud(
    mut contexts: EguiContexts,
    hud: HudContext,
    mut turn_state: ResMut<TurnState>,
    mut highlights: ResMut<MovementHighlights>,
    mut units: Query<(&mut Unit, &FactionMember)>,
    mut turn_start_events: MessageWriter<TurnStartEvent>,
    mut commanders: ResMut<Commanders>,
    mut power_events: MessageWriter<PowerActivatedEvent>,
    keyboard: Res<ButtonInput<KeyCode>>,
    modal: Res<crate::game::ModalOpen>,
    portraits: Res<PortraitTextures>,
) {
    if !hud.battle_active() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let game_data = &hud.game_data;
    let is_ai_turn = hud.is_ai_turn(&turn_state);
    let player = hud.battle_config.player_faction;

    // --- Turn card (top left) ---
    egui::Area::new(egui::Id::new("hud_turn"))
        .anchor(egui::Align2::LEFT_TOP, egui::vec2(16.0, 16.0))
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            theme::hud_card().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let current = turn_state.current_faction;
                    banner(ui, theme::faction_color(game_data, current), 44.0);
                    ui.vertical(|ui| {
                        ui.label(theme::display(format!("Day {}", turn_state.turn_number), 26.0).color(theme::WOOD_DARK));
                        let status = if is_ai_turn {
                            let dots = ".".repeat((hud.time.elapsed_secs() * 2.0) as usize % 4);
                            format!("{} is plotting{}", game_data.faction_name(current), dots)
                        } else {
                            format!("{} — your move", game_data.faction_name(current))
                        };
                        ui.label(theme::body(status, 15.0).color(theme::INK_SOFT));
                    });
                });
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    coin(ui);
                    ui.label(theme::bold(format_funds(hud.funds.get(player)), 19.0));
                    ui.add_space(10.0);
                    theme::chip(ui, &format!("{} {}", hud.weather.current.icon(), hud.weather.current.name()), theme::SKY);
                    if hud.fog.enabled {
                        theme::chip(ui, "Fog of war", theme::INK_SOFT);
                    }
                });
            });
        });

    // --- Commander card (top right) ---
    let co_id = commanders.get_active(player);
    let co = game_data.commander(co_id);
    let charge = commanders.get_charge(player);
    let power_cost = commanders.get_power_cost(player, game_data);
    let can_activate = commanders.can_activate(player, game_data) && !is_ai_turn;
    let power_active = commanders.is_power_active(player);
    let mut activate = can_activate && keyboard.just_pressed(KeyCode::KeyC) && !hud.menu.open;

    egui::Area::new(egui::Id::new("hud_commander"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
        .order(egui::Order::Middle)
        .show(ctx, |ui| {
            theme::hud_card().show(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::portrait(ui, 54.0, theme::faction_color(game_data, player), &co.name, portraits.get(co_id));
                    ui.vertical(|ui| {
                        ui.label(theme::display(&co.name, 22.0).color(theme::WOOD_DARK));
                        let fill = if power_active || can_activate { theme::GOLD } else { theme::SKY };
                        theme::meter(ui, 190.0, 12.0, charge as f32 / power_cost.max(1) as f32, fill, 5)
                            .on_hover_text(format!("{charge}/{power_cost} — charges as your units deal and take damage"));
                        if power_active {
                            ui.label(theme::bold(format!("★ {} active!", co.power.name), 15.0).color(theme::WOOD));
                        } else if can_activate {
                            ui.horizontal(|ui| {
                                if ui.button(theme::bold(format!("★ {}", co.power.name), 15.0))
                                    .on_hover_text(&co.power.description).clicked()
                                {
                                    activate = true;
                                }
                                theme::key_hint(ui, "C", "", false);
                            });
                        } else {
                            ui.label(theme::body(format!("★ {}", co.power.name), 14.0).color(theme::INK_SOFT))
                                .on_hover_text(&co.power.description);
                        }
                    });
                });
            });
        });

    if activate {
        if let Some(effect) = commanders.activate_power(player, game_data) {
            power_events.write(PowerActivatedEvent { faction: player, effect });
        }
    }

    // --- End turn (bottom right) ---
    if !is_ai_turn {
        egui::Area::new(egui::Id::new("hud_end_turn"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -52.0))
            .order(egui::Order::Middle)
            .show(ctx, |ui| {
                ui.set_width(190.0);
                let busy = turn_state.phase != TurnPhase::Select;
                if theme::primary_button(ui, "End Turn", !busy, false).clicked() {
                    end_turn(&mut turn_state, &mut highlights, &hud.battle_config,
                        units.iter_mut().map(|(u, f)| (u, f)), &mut turn_start_events);
                }
            });
    }

    // --- Control hints (bottom strip) ---
    let hints: &[(&str, &str)] = if modal.0 {
        &[]
    } else if is_ai_turn {
        &[("", "Enemy turn in progress")]
    } else {
        match turn_state.phase {
            TurnPhase::Select if highlights.selected_unit.is_some() => {
                &[("Arrows", "Plan route"), ("Space", "Move here"), ("Esc", "Cancel")]
            }
            TurnPhase::Select => &[
                ("Arrows", "Cursor"), ("Space", "Select / build"), ("Tab", "Next unit"),
                ("C", "CO power"), ("Q/E", "Zoom"), ("Esc", "Menu"),
            ],
            TurnPhase::Move => &[("Arrows", "Plan route"), ("Space", "Move here"), ("Esc", "Cancel")],
            TurnPhase::Action => &[("↑↓", "Choose action"), ("Enter", "Confirm"), ("Esc", "Wait")],
            TurnPhase::Animating => &[],
        }
    };
    egui::Area::new(egui::Id::new("hud_hints"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -10.0))
        .order(egui::Order::Middle)
        .interactable(false)
        .show(ctx, |ui| {
            if hints.is_empty() {
                return;
            }
            egui::Frame::new()
                .fill(egui::Color32::from_black_alpha(150))
                .corner_radius(14)
                .inner_margin(egui::Margin::symmetric(14, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (key, action) in hints {
                            if key.is_empty() {
                                ui.label(theme::body(*action, 15.0).color(theme::PARCHMENT));
                            } else {
                                theme::key_hint(ui, key, action, true);
                            }
                        }
                    });
                });
        });
}

/// Inspect card (bottom left) for whatever the mouse or the grid cursor points at
pub fn draw_inspect_card(
    mut contexts: EguiContexts,
    hud: HudContext,
    hovered: Res<HoveredUnit>,
    hovered_tile: Res<SelectedTile>,
    cursor: Res<GridCursor>,
    highlights: Res<MovementHighlights>,
    map: Res<GameMap>,
    units: Query<(Entity, &Unit, &FactionMember, &GridPosition)>,
    tiles: Query<&Tile>,
    commanders: Res<Commanders>,
) {
    if !hud.battle_active() {
        return;
    }
    // Mouse hover wins; otherwise follow the keyboard/gamepad cursor
    let target = hovered.entity.and_then(|e| units.get(e).ok()).map(|(_, _, _, p)| IVec2::new(p.x, p.y))
        .or(hovered_tile.position)
        .or(cursor.visible.then(|| IVec2::new(cursor.x, cursor.y)))
        .or_else(|| highlights.selected_unit.and_then(|e| units.get(e).ok()).map(|(_, _, _, p)| IVec2::new(p.x, p.y)));
    let Some(pos) = target else { return };
    let Some(terrain) = map.get(pos.x, pos.y) else { return };
    if hud.fog.enabled && hud.fog.get_visibility(pos.x, pos.y) == TileVisibility::Unexplored {
        return;
    }

    let player = hud.battle_config.player_faction;
    let unit = units.iter()
        .find(|(_, _, f, p)| p.x == pos.x && p.y == pos.y && (f.faction == player || hud.visible_to_player(pos.x, pos.y)))
        .map(|(_, u, f, _)| (u, f.faction));
    let owner = tiles.iter().find(|t| t.position == pos).and_then(|t| t.owner);
    let game_data = &hud.game_data;

    let Ok(ctx) = contexts.ctx_mut() else { return };
    egui::Area::new(egui::Id::new("hud_inspect"))
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(16.0, -52.0))
        .order(egui::Order::Middle)
        .interactable(false)
        .show(ctx, |ui| {
            theme::hud_card().show(ui, |ui| {
                ui.set_width(270.0);
                if let Some((unit, faction)) = unit {
                    let stats = game_data.unit_stats(unit.unit_type);
                    let bonuses = commanders.get_bonuses(faction, game_data);
                    let color = theme::faction_color(game_data, faction);
                    ui.horizontal(|ui| {
                        let (slot, _) = ui.allocate_exact_size(egui::vec2(46.0, 46.0), egui::Sense::hover());
                        unit_medallion(ui.painter(), slot.center(), 22.0, color, game_data.unit(unit.unit_type).symbol, unit.exhausted);
                        ui.vertical(|ui| {
                            ui.label(theme::display(game_data.unit_name(unit.unit_type), 21.0).color(theme::WOOD_DARK));
                            ui.label(theme::body(game_data.faction_name(faction), 14.0).color(theme::shade(color, 0.7)));
                        });
                    });
                    ui.add_space(2.0);
                    let hp_frac = unit.hp as f32 / stats.max_hp as f32;
                    labelled_meter(ui, "HP", &format!("{}/10", display_hp(unit.hp)), hp_frac, theme::health_color(hp_frac));
                    if stats.max_stamina > 0 {
                        let f = unit.stamina as f32 / stats.max_stamina as f32;
                        labelled_meter(ui, "Stamina", &format!("{}/{}", unit.stamina, stats.max_stamina), f, theme::SKY);
                    }
                    if stats.max_ammo > 0 {
                        let f = unit.ammo as f32 / stats.max_ammo as f32;
                        labelled_meter(ui, "Ammo", &format!("{}/{}", unit.ammo, stats.max_ammo), f, theme::GOLD);
                    }
                    ui.horizontal_wrapped(|ui| {
                        let movement = hud.weather.apply_movement((stats.movement as i32 + bonuses.movement).max(1) as u32);
                        let (min_r, max_r) = stats.attack_range;
                        let range = if min_r == max_r { format!("{min_r}") } else { format!("{min_r}–{max_r}") };
                        stat(ui, "Move", &movement.to_string());
                        stat(ui, "Range", &range);
                        stat(ui, "Vision", &hud.weather.apply_vision(stats.vision + bonuses.vision).to_string());
                    });
                    ui.horizontal_wrapped(|ui| {
                        if unit.exhausted {
                            theme::chip(ui, "Done", theme::INK_SOFT);
                        }
                        if stats.max_ammo > 0 && unit.ammo == 0 {
                            theme::chip(ui, "No ammo", theme::BERRY);
                        }
                        if stats.max_stamina > 0 && unit.stamina == 0 {
                            theme::chip(ui, "Exhausted", theme::BERRY);
                        } else if stats.max_stamina > 0 && unit.stamina <= stats.max_stamina / 3 {
                            theme::chip(ui, "Low stamina", theme::GOLD);
                        }
                        if let Some(cargo) = &unit.cargo {
                            theme::chip(ui, &format!("Carrying {}", game_data.unit_name(cargo.unit_type)), theme::SKY);
                        }
                    });
                    ui.separator();
                }
                terrain_section(ui, game_data, terrain, owner, unit.map(|(u, _)| game_data.unit_class(u.unit_type)));
            });
        });
}

fn terrain_section(ui: &mut egui::Ui, game_data: &GameData, terrain: Terrain, owner: Option<crate::game::Faction>, class: Option<UnitClass>) {
    ui.horizontal(|ui| {
        let (swatch, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
        ui.painter().rect(swatch, 3, theme::to_egui(game_data.terrain_color(terrain)),
            theme::stroke(1.0, theme::WOOD_DARK), egui::StrokeKind::Inside);
        ui.label(theme::bold(game_data.terrain_name(terrain), 17.0));
        let stars = game_data.terrain_defense(terrain);
        ui.label(theme::body("★".repeat(stars as usize) + &"☆".repeat(4usize.saturating_sub(stars as usize)), 15.0)
            .color(theme::GOLD)).on_hover_text(format!("Defense +{stars}"));
    });
    ui.horizontal_wrapped(|ui| {
        let cost = game_data.movement_cost(terrain, class.unwrap_or(UnitClass::Foot));
        let label = if class.is_some() { "Move cost" } else { "Foot move" };
        stat(ui, label, &if cost >= IMPASSABLE { "—".to_string() } else { cost.to_string() });
        if game_data.terrain_capturable(terrain) {
            stat(ui, "Income", &format_funds(game_data.terrain_income(terrain)));
            match owner {
                Some(f) => theme::chip(ui, game_data.faction_name(f), theme::faction_color(game_data, f)),
                None => theme::chip(ui, "Neutral", theme::INK_SOFT),
            }
            if game_data.terrain_produces(terrain) {
                theme::chip(ui, "Builds units", theme::LEAF);
            }
        }
    });
}

/// Faction tokens over every visible unit: type letter, HP pip, dimmed when done
pub fn draw_unit_tokens(
    mut contexts: EguiContexts,
    hud: HudContext,
    units: Query<(Entity, &Unit, &GlobalTransform, &FactionMember, &GridPosition, Has<crate::game::UnitSprite>)>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    tiles: Query<&crate::game::Tile>,
    mut screen_positions: ResMut<UnitScreenPositions>,
) {
    screen_positions.0.clear();
    if !hud.battle_active() {
        return;
    }
    let Ok((camera, camera_transform)) = camera.single() else { return };
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let player = hud.battle_config.player_faction;
    let game_data = &hud.game_data;

    egui::Area::new(egui::Id::new("unit_tokens"))
        .fixed_pos(egui::Pos2::ZERO)
        .order(egui::Order::Background)
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter();
            let now = ui.input(|i| i.time);
            let capturing: std::collections::HashMap<(i32, i32), crate::game::Faction> = tiles.iter()
                .filter(|t| t.capture_progress > 0)
                .filter_map(|t| t.capturing_faction.map(|f| ((t.position.x, t.position.y), f)))
                .collect();
            let right = camera_transform.right().as_vec3();
            let up = camera_transform.up().as_vec3();
            for (entity, unit, transform, faction, pos, has_sprite) in units.iter() {
                if faction.faction != player && !hud.visible_to_player(pos.x, pos.y) {
                    continue;
                }
                let Ok(screen) = camera.world_to_viewport(camera_transform, transform.translation()) else { continue };
                let center = egui::pos2(screen.x, screen.y);
                screen_positions.0.insert(entity, center);
                // Units with art need only their HP; cards get a type token
                if !has_sprite {
                    let color = theme::faction_color(game_data, faction.faction);
                    unit_medallion(painter, center, 15.0, color, game_data.unit(unit.unit_type).symbol, unit.exhausted);
                }

                // Badges hang off the sprite's bottom corners, sized to its pixels
                let half = crate::game::SPRITE_WORLD_SIZE / 2.0;
                let corner = |side: f32| camera.world_to_viewport(camera_transform,
                    transform.translation() + right * half * side - up * half).ok();
                let (Some(bottom_left), Some(bottom_right)) = (corner(-1.0), corner(1.0)) else { continue };
                let sprite_px = ((bottom_right.x - bottom_left.x) / 32.0).max(0.5);
                let cell = (sprite_px * 1.5).round().max(2.0);

                // HP 1-9 bottom right, like Advance Wars; full health shows nothing
                let hp = display_hp(unit.hp);
                if (1..10).contains(&hp) {
                    let glyph = &DIGITS[hp as usize];
                    pixel_badge(painter, egui::pos2(bottom_right.x, bottom_right.y), cell, glyph, egui::Color32::WHITE, true);
                }
                // Status bottom left, cycling when there's more than one
                let statuses = unit_statuses(unit, &hud.game_data, capturing.get(&(pos.x, pos.y)) == Some(&faction.faction));
                if !statuses.is_empty() {
                    let (glyph, color) = statuses[(now / STATUS_CYCLE_SECS) as usize % statuses.len()];
                    pixel_badge(painter, egui::pos2(bottom_left.x, bottom_left.y), cell, glyph, color, false);
                }
            }
        });
}

/// Seconds each status icon shows before the next one when a unit has several
const STATUS_CYCLE_SECS: f64 = 0.9;

/// A 5x5 pixel glyph, one string per row ('#' lit)
type Glyph = [&'static str; 5];

/// Pixel digits 0-9, 3 wide
const DIGITS: [Glyph; 10] = [
    ["###", "#.#", "#.#", "#.#", "###"],
    [".#.", "##.", ".#.", ".#.", "###"],
    ["###", "..#", "###", "#..", "###"],
    ["###", "..#", ".##", "..#", "###"],
    ["#.#", "#.#", "###", "..#", "..#"],
    ["###", "#..", "###", "..#", "###"],
    ["###", "#..", "###", "#.#", "###"],
    ["###", "..#", ".#.", ".#.", ".#."],
    ["###", "#.#", "###", "#.#", "###"],
    ["###", "#.#", "###", "..#", "###"],
];

/// Running low on arrows, stones and bolts
const ICON_AMMO: Glyph = ["..#..", ".###.", "..#..", "..#..", ".#.#."];
/// Running low on stamina (Advance Wars fuel): an acorn
const ICON_STAMINA: Glyph = [".###.", "#####", ".###.", ".###.", "..#.."];
/// Carrying another unit
const ICON_CARGO: Glyph = ["#####", "#.#.#", "#####", "#.#.#", "#####"];
/// Capturing the property it stands on
const ICON_CAPTURE: Glyph = ["##...", "####.", "##...", "#....", "#...."];

/// The status icons a unit shows, in order. Submerged, hidden and similar
/// states slot in here as the mechanics arrive.
fn unit_statuses(unit: &Unit, game_data: &GameData, capturing: bool) -> Vec<(&'static Glyph, egui::Color32)> {
    let stats = game_data.unit_stats(unit.unit_type);
    let mut statuses = Vec::new();
    if capturing {
        statuses.push((&ICON_CAPTURE, egui::Color32::from_rgb(255, 214, 90)));
    }
    if unit.cargo.is_some() {
        statuses.push((&ICON_CARGO, egui::Color32::from_rgb(232, 208, 160)));
    }
    if stats.max_ammo > 0 && unit.ammo * 3 <= stats.max_ammo {
        statuses.push((&ICON_AMMO, egui::Color32::from_rgb(255, 140, 90)));
    }
    if stats.max_stamina > 0 && unit.stamina * 4 <= stats.max_stamina {
        statuses.push((&ICON_STAMINA, egui::Color32::from_rgb(240, 196, 80)));
    }
    statuses
}

/// Advance Wars style badge: a black box with a chunky pixel glyph, anchored at
/// a sprite corner (`right` hangs it off the bottom-right, else bottom-left)
fn pixel_badge(painter: &egui::Painter, corner: egui::Pos2, cell: f32, glyph: &Glyph, color: egui::Color32, right: bool) {
    let cols = glyph[0].len() as f32;
    let size = egui::vec2((cols + 2.0) * cell, 7.0 * cell);
    let min = egui::pos2(if right { corner.x - size.x } else { corner.x }, corner.y - size.y).round();
    let rect = egui::Rect::from_min_size(min, size);
    painter.rect_filled(rect.expand(1.0), 1, egui::Color32::from_rgb(232, 224, 206));
    painter.rect_filled(rect, 0, egui::Color32::from_rgb(16, 14, 12));
    for (row, line) in glyph.iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            if ch == '#' {
                let p = min + egui::vec2((col as f32 + 1.0) * cell, (row as f32 + 1.0) * cell);
                painter.rect_filled(egui::Rect::from_min_size(p, egui::vec2(cell, cell)), 0, color);
            }
        }
    }
}

/// Round token with the unit's type letter
fn unit_medallion(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: egui::Color32, symbol: char, dimmed: bool) {
    let (fill, rim, text) = if dimmed {
        (theme::shade(color, 0.45), theme::shade(color, 0.3), egui::Color32::from_gray(170))
    } else {
        (color, theme::shade(color, 0.5), theme::PARCHMENT)
    };
    painter.circle(center + egui::vec2(0.0, 2.0), radius, egui::Color32::from_black_alpha(90), egui::Stroke::NONE);
    painter.circle(center, radius, fill, theme::stroke(2.0, rim));
    painter.circle_stroke(center, radius - 3.5, theme::stroke(1.0, theme::shade(fill, 1.3).gamma_multiply(0.7)));
    painter.text(center + egui::vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, symbol.to_string(),
        egui::FontId::new(radius * 1.15, theme::title_family()), text);
}

/// AW-style displayed HP (1-10)
fn display_hp(hp: i32) -> i32 {
    ((hp as f32) / 10.0).ceil().clamp(0.0, 10.0) as i32
}

fn labelled_meter(ui: &mut egui::Ui, label: &str, value: &str, fraction: f32, color: egui::Color32) {
    ui.horizontal(|ui| {
        ui.add_sized([56.0, 16.0], egui::Label::new(theme::body(label, 14.0).color(theme::INK_SOFT)));
        theme::meter(ui, 140.0, 10.0, fraction, color, 0);
        ui.label(theme::bold(value, 14.0));
    });
}

fn stat(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(theme::body(label, 14.0).color(theme::INK_SOFT));
    ui.label(theme::bold(value, 15.0));
    ui.add_space(6.0);
}

/// A small pennant in a faction's color
fn banner(ui: &mut egui::Ui, color: egui::Color32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, height), egui::Sense::hover());
    ui.painter().add(egui::Shape::convex_polygon(
        vec![rect.left_top(), rect.right_top(), rect.right_bottom(),
             egui::pos2(rect.center().x, rect.bottom() - 6.0), rect.left_bottom()],
        color, theme::stroke(1.0, theme::shade(color, 0.55)),
    ));
}

fn coin(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.circle(rect.center(), 8.0, theme::GOLD, theme::stroke(1.5, theme::WOOD));
    painter.circle_stroke(rect.center(), 4.5, theme::stroke(1.0, theme::WOOD.gamma_multiply(0.6)));
}

/// 12500 -> "12,500"
pub fn format_funds(amount: u32) -> String {
    let digits = amount.to_string();
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn funds_get_thousands_separators() {
        assert_eq!(format_funds(0), "0");
        assert_eq!(format_funds(950), "950");
        assert_eq!(format_funds(12500), "12,500");
        assert_eq!(format_funds(1234567), "1,234,567");
    }

    #[test]
    fn displayed_hp_rounds_up() {
        assert_eq!(display_hp(100), 10);
        assert_eq!(display_hp(91), 10);
        assert_eq!(display_hp(90), 9);
        assert_eq!(display_hp(1), 1);
        assert_eq!(display_hp(0), 0);
    }
}
