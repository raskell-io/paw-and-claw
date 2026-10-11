//! In-battle HUD: turn and commander cards, inspect card, unit tokens, control hints

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::icons::{self, Icon};
use super::theme;
use super::{BattleSetupState, HoveredUnit, PortraitTextures, SelectedTile};
use crate::game::{
    AiState, TurnBanner, DAY_BANNER_SECS, MATCH_INTRO_SECS, BattleConfig, Commanders, FactionFunds, FactionMember, FogOfWar, GameData, GameMap,
    GameResult, GridCursor, GridPosition, MovementHighlights, PowerActivatedEvent, PowerLevel, Terrain, Tile,
    TileVisibility, TurnState, Unit, UnitClass, Weather, IMPASSABLE,
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

pub fn draw_hud(
    mut contexts: EguiContexts,
    hud: HudContext,
    turn_state: Res<TurnState>,
    mut commanders: ResMut<Commanders>,
    mut power_events: MessageWriter<PowerActivatedEvent>,
    keyboard: Res<ButtonInput<KeyCode>>,
    modal: Res<crate::game::ModalOpen>,
    portraits: Res<PortraitTextures>,
    cursor: Res<GridCursor>,
    map: Res<GameMap>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut panel_left: Local<bool>,
) {
    if !hud.battle_active() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let game_data = &hud.game_data;
    let is_ai_turn = hud.is_ai_turn(&turn_state);
    let player = hud.battle_config.player_faction;

    // --- Army panel: Advance Wars' corner box for the army whose turn it is ---
    // Commander, star meter, funds, day and weather. It sits in the top corner
    // away from the cursor, flipping when the cursor crosses into its half.
    let current = turn_state.current_faction;
    if let Some(fraction) = cursor_screen_fraction(&cursor, &map, &cameras) {
        if *panel_left && fraction < 0.4 {
            *panel_left = false;
        } else if !*panel_left && fraction > 0.6 {
            *panel_left = true;
        }
    }
    let co_id = commanders.get_active(current);
    let co = game_data.commander(co_id);
    let charge = commanders.get_charge(current);
    let power_stars = co.power.as_ref().map_or(0, |p| p.stars);
    let super_cost = commanders.power_cost(current, PowerLevel::Super, game_data).unwrap_or(1).max(1);
    let per_star = super_cost as f32 / co.super_power.stars.max(1) as f32;
    let active = commanders.active_power(current);
    // C fires the CO Power, V the Super (also in the map menu)
    let ready = |level| commanders.can_activate(player, level, game_data) && !is_ai_turn && !modal.0;
    let mut activate = None;
    if ready(PowerLevel::Power) && keyboard.just_pressed(KeyCode::KeyC) {
        activate = Some(PowerLevel::Power);
    }
    if ready(PowerLevel::Super) && keyboard.just_pressed(KeyCode::KeyV) {
        activate = Some(PowerLevel::Super);
    }

    let (align, offset) = if *panel_left {
        (egui::Align2::LEFT_TOP, egui::vec2(16.0, 16.0))
    } else {
        (egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 16.0))
    };
    egui::Area::new(egui::Id::new("hud_army"))
        .anchor(align, offset)
        .order(egui::Order::Middle)
        .interactable(false)
        .show(ctx, |ui| {
            // The portrait fills the card's left side, edge to edge, with just a
            // divider between it and the numbers
            let card = theme::hud_card().inner_margin(egui::Margin { left: 0, right: 12, top: 0, bottom: 0 });
            card.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    let color = theme::faction_color(game_data, current);
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(104.0, 104.0), egui::Sense::hover());
                    let corners = egui::CornerRadius { nw: 7, sw: 7, ne: 0, se: 0 };
                    match portraits.get(co_id) {
                        Some(texture) => {
                            egui::Image::new((texture, rect.size())).corner_radius(corners).paint_at(ui, rect);
                        }
                        None => {
                            ui.painter().rect_filled(rect, corners, color);
                            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, co.name.chars().next().unwrap_or('?').to_string(),
                                egui::FontId::new(48.0, theme::title_family()), theme::PARCHMENT);
                        }
                    }
                    ui.painter().line_segment([rect.right_top(), rect.right_bottom()], theme::stroke(2.0, theme::WOOD));
                    ui.add_space(14.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.label(theme::display(&co.name, 22.0).color(theme::WOOD_DARK));
                            ui.add_space(12.0);
                            ui.label(theme::display(format!("Day {}", turn_state.turn_number), 18.0).color(theme::shade(color, 0.7)));
                        });
                        star_meter(ui, charge as f32 / per_star, power_stars, co.super_power.stars, active != PowerLevel::None);
                        ui.horizontal(|ui| {
                            icons::icon(ui, Icon::Coin, 22.0).on_hover_text("Funds");
                            ui.label(theme::bold(format_funds(hud.funds.get(current)), 18.0));
                            ui.add_space(10.0);
                            icons::icon(ui, icons::weather(hud.weather.current), 22.0);
                            ui.label(theme::bold(hud.weather.current.name(), 15.0).color(theme::SKY));
                            if hud.fog.enabled {
                                theme::chip(ui, "Fog", theme::INK_SOFT);
                            }
                        });
                        let powers = co.power.as_ref().map(|p| (PowerLevel::Power, p, "C"))
                            .into_iter()
                            .chain(std::iter::once((PowerLevel::Super, &co.super_power, "V")));
                        for (level, power, key) in powers {
                            if active == level {
                                ui.label(theme::bold(format!("★ {} active!", power.name), 15.0).color(theme::WOOD));
                            } else if current == player && ready(level) {
                                ui.horizontal(|ui| {
                                    ui.label(theme::bold(format!("★ {} ready", power.name), 14.0).color(theme::WOOD));
                                    theme::key_hint(ui, key, "", false);
                                });
                            }
                        }
                        if is_ai_turn {
                            let dots = ".".repeat((hud.time.elapsed_secs() * 2.0) as usize % 4);
                            ui.label(theme::body(format!("{} is plotting{dots}", game_data.faction_name(current)), 14.0).color(theme::INK_SOFT));
                        }
                    });
                });
            });
        });

    if let Some(level) = activate {
        if let Some(effects) = commanders.activate_power(player, level, game_data) {
            power_events.write(PowerActivatedEvent { faction: player, level, effects });
        }
    }
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
    unit_textures: Res<super::UnitTextures>,
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
            theme::glass_card().show(ui, |ui| {
                ui.set_width(270.0);
                if let Some((unit, faction)) = unit {
                    let stats = game_data.unit_stats(unit.unit_type);
                    let bonuses = commanders.get_bonuses(faction, game_data);
                    let color = theme::faction_color(game_data, faction);
                    ui.horizontal(|ui| {
                        // The unit's own sprite marching in place, on a disc of its army's colour
                        let (slot, _) = ui.allocate_exact_size(egui::vec2(52.0, 52.0), egui::Sense::hover());
                        match unit_textures.get(faction, unit.unit_type) {
                            Some((texture, frames)) => {
                                ui.painter().circle_filled(slot.center() + egui::vec2(0.0, 9.0), 19.0, color.gamma_multiply(0.3));
                                let frame = (hud.time.elapsed_secs() * 6.0) as u32 % frames.max(1);
                                let w = 1.0 / frames.max(1) as f32;
                                let uv = egui::Rect::from_min_max(egui::pos2(frame as f32 * w, 0.0), egui::pos2((frame + 1) as f32 * w, 1.0));
                                let tint = if unit.exhausted { egui::Color32::from_gray(170) } else { egui::Color32::WHITE };
                                ui.painter().image(texture, slot.expand(4.0), uv, tint);
                            }
                            None => unit_medallion(ui.painter(), slot.center(), 22.0, color, game_data.unit(unit.unit_type).symbol, unit.exhausted),
                        }
                        ui.vertical(|ui| {
                            ui.label(theme::display(game_data.unit_name(unit.unit_type), 21.0).color(theme::WOOD_DARK));
                            ui.label(theme::body(game_data.faction_name(faction), 14.0).color(theme::shade(color, 0.7)));
                        });
                    });
                    ui.add_space(2.0);
                    let hp_frac = unit.hp as f32 / stats.max_hp as f32;
                    // AW Sonja: the enemy can't read her units' HP
                    let hidden = faction != hud.battle_config.player_faction && bonuses.mods.hidden_hp;
                    if hidden {
                        labelled_meter(ui, Icon::Heart, "HP", "?/10", 1.0, theme::INK_SOFT);
                    } else {
                        labelled_meter(ui, Icon::Heart, "HP", &format!("{}/10", display_hp(unit.hp)), hp_frac, theme::health_color(hp_frac));
                    }
                    if stats.max_stamina > 0 {
                        let f = unit.stamina as f32 / stats.max_stamina as f32;
                        labelled_meter(ui, Icon::Flask, "Stamina", &format!("{}/{}", unit.stamina, stats.max_stamina), f, theme::SKY);
                    }
                    if stats.max_ammo > 0 {
                        let f = unit.ammo as f32 / stats.max_ammo as f32;
                        labelled_meter(ui, Icon::Arrow, "Ammo", &format!("{}/{}", unit.ammo, stats.max_ammo), f, theme::GOLD);
                    }
                    ui.horizontal_wrapped(|ui| {
                        let movement = bonuses.total_movement(unit.unit_type, &hud.weather, game_data);
                        let (min_r, mut max_r) = stats.attack_range;
                        if min_r > 1 {
                            max_r = (max_r as i32 + bonuses.range(unit.unit_type, game_data)).max(min_r as i32) as u32;
                        }
                        let range = if min_r == max_r { format!("{min_r}") } else { format!("{min_r}–{max_r}") };
                        icon_stat(ui, Icon::Boot, "Move", &movement.to_string());
                        icon_stat(ui, Icon::Target, "Range", &range);
                        icon_stat(ui, Icon::Eye, "Vision", &bonuses.total_vision(unit.unit_type, &hud.weather, game_data).to_string());
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
                        for cargo in &unit.cargo {
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
        icons::icon(ui, Icon::Shield, 18.0).on_hover_text(format!("Defense +{stars}"));
        ui.label(theme::body("★".repeat(stars as usize) + &"☆".repeat(4usize.saturating_sub(stars as usize)), 15.0)
            .color(theme::GOLD)).on_hover_text(format!("Defense +{stars}"));
    });
    ui.horizontal_wrapped(|ui| {
        let cost = game_data.movement_cost(terrain, class.unwrap_or(UnitClass::Foot));
        let label = if class.is_some() { "Move cost" } else { "Foot move" };
        icon_stat(ui, Icon::Boot, label, &if cost >= IMPASSABLE { "—".to_string() } else { cost.to_string() });
        if game_data.terrain_capturable(terrain) {
            icon_stat(ui, Icon::Coin, "Income per day", &format_funds(game_data.terrain_income(terrain)));
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
    commanders: Res<Commanders>,
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
                // A running power makes the army's units twinkle, as in Advance Wars
                if commanders.active_power(faction.faction) != PowerLevel::None {
                    power_sparkles(painter, center, now, entity.index() as f64);
                }
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

                // HP 1-9 bottom right, like Advance Wars; full health shows nothing.
                // AW Sonja's units show the enemy a "?" instead
                let hp = display_hp(unit.hp);
                let hidden = faction.faction != player
                    && commanders.get_bonuses(faction.faction, &hud.game_data).mods.hidden_hp;
                if (1..10).contains(&hp) {
                    let glyph = if hidden { &ICON_HIDDEN } else { &DIGITS[hp as usize] };
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

/// HP hidden from the enemy (AW Sonja)
const ICON_HIDDEN: Glyph = [".##.", "#..#", "..#.", "....", "..#."];

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
    if unit.has_cargo() {
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

/// AW power meter: `filled` stars of charge out of `total`; the first
/// `power_stars` are small (the CO Power), the rest big (the Super CO Power)
fn star_meter(ui: &mut egui::Ui, filled: f32, power_stars: u32, total: u32, glowing: bool) -> egui::Response {
    let (small, big) = (12.0, 17.0);
    let width = power_stars as f32 * (small + 2.0) + (total - power_stars.min(total)) as f32 * (big + 2.0);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width.max(20.0), big + 2.0), egui::Sense::hover());
    let painter = ui.painter();
    let mut x = rect.left();
    for i in 0..total {
        let size = if i < power_stars { small } else { big };
        let fill = (filled - i as f32).clamp(0.0, 1.0);
        let center = egui::pos2(x + size / 2.0, rect.center().y + (big - size) / 2.0);
        let empty = theme::shade(theme::PARCHMENT, 0.75);
        painter.text(center, egui::Align2::CENTER_CENTER, "★", egui::FontId::proportional(size), empty);
        if fill > 0.0 {
            // Fill from the left as the star charges
            let clip = egui::Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(x + size * fill, rect.bottom()));
            let color = if glowing || fill >= 1.0 { theme::GOLD } else { theme::SKY };
            painter.with_clip_rect(clip).text(center, egui::Align2::CENTER_CENTER, "★", egui::FontId::proportional(size), color);
        }
        x += size + 2.0;
    }
    response
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

fn labelled_meter(ui: &mut egui::Ui, icon: Icon, label: &str, value: &str, fraction: f32, color: egui::Color32) {
    ui.horizontal(|ui| {
        icons::icon(ui, icon, 20.0).on_hover_text(label);
        theme::meter(ui, 168.0, 10.0, fraction, color, 0);
        ui.label(theme::bold(value, 14.0));
    });
}

/// A stat as icon and value, named on hover
fn icon_stat(ui: &mut egui::Ui, icon: Icon, label: &str, value: &str) {
    icons::icon(ui, icon, 20.0).on_hover_text(label);
    ui.label(theme::bold(value, 15.0)).on_hover_text(label);
    ui.add_space(10.0);
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

/// Two small stars twinkling around a powered-up unit
fn power_sparkles(painter: &egui::Painter, center: egui::Pos2, now: f64, seed: f64) {
    for i in 0..2 {
        let t = now * 1.7 + seed * 0.37 + i as f64 * 1.9;
        let phase = t.fract();
        let angle = (t.floor() * 2.4 + seed) % std::f64::consts::TAU;
        let offset = egui::vec2(angle.cos() as f32 * 18.0, angle.sin() as f32 * 14.0 - 6.0);
        let alpha = (phase * std::f64::consts::PI).sin() as f32;
        let color = egui::Color32::from_rgba_unmultiplied(255, 236, 140, (alpha * 230.0) as u8);
        // A four-pointed glint: a thin diamond on each axis
        let (p, r, w) = (center + offset, 3.0 + alpha * 4.0, 1.2);
        for (dx, dy) in [(1.0, 0.0), (0.0, 1.0)] {
            let along = egui::vec2(dx, dy) * r;
            let across = egui::vec2(-dy, dx) * w;
            painter.add(egui::Shape::convex_polygon(vec![p - along, p + across, p + along, p - across], color, egui::Stroke::NONE));
        }
    }
}

/// Weather drawn over the board, under the HUD: falling snow, slanting rain,
/// drifting sand, or a pale haze for fog. Particles are placed from time alone,
/// so nothing is stored between frames.
pub fn draw_weather_overlay(
    mut contexts: EguiContexts,
    weather: Res<Weather>,
    setup: Res<BattleSetupState>,
    time: Res<Time>,
) {
    use crate::game::WeatherType;
    if setup.needs_setup || weather.current == WeatherType::Clear {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let screen = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    let t = time.elapsed_secs();
    // A cheap stable pseudo-random number per particle and channel
    let hash = |i: u32, k: u32| ((i.wrapping_mul(2654435761) ^ k.wrapping_mul(40503)).wrapping_mul(2246822519) >> 8) as f32 / (1u32 << 24) as f32;
    let px = 3.0; // chunky pixels, like the sprites
    let snap = |v: f32| (v / px).round() * px;
    let (w, h) = (screen.width(), screen.height());
    match weather.current {
        WeatherType::Snow => {
            painter.rect_filled(screen, 0.0, egui::Color32::from_rgba_unmultiplied(220, 230, 255, 18));
            for i in 0..140 {
                let speed = 30.0 + hash(i, 1) * 40.0;
                let y = (hash(i, 2) * h + t * speed) % (h + 20.0) - 10.0;
                let sway = (t * (0.8 + hash(i, 3)) + hash(i, 4) * 6.28).sin() * 10.0;
                let x = (hash(i, 5) * w + sway + t * 8.0) % w;
                let size = if hash(i, 6) > 0.7 { px * 2.0 } else { px };
                let pos = egui::pos2(screen.min.x + snap(x), screen.min.y + snap(y));
                painter.rect_filled(egui::Rect::from_min_size(pos, egui::vec2(size, size)), 0.0,
                    egui::Color32::from_rgba_unmultiplied(250, 252, 255, 220));
            }
        }
        WeatherType::Rain => {
            painter.rect_filled(screen, 0.0, egui::Color32::from_rgba_unmultiplied(30, 50, 90, 40));
            for i in 0..120 {
                let speed = 520.0 + hash(i, 1) * 200.0;
                let y = (hash(i, 2) * h + t * speed) % (h + 40.0) - 20.0;
                let x = (hash(i, 5) * w + t * speed * 0.25) % w;
                let len = 14.0 + hash(i, 6) * 10.0;
                let top = egui::pos2(screen.min.x + x, screen.min.y + y);
                painter.line_segment([top, top + egui::vec2(len * 0.25, len)],
                    egui::Stroke::new(2.0_f32, egui::Color32::from_rgba_unmultiplied(180, 205, 240, 150)));
            }
        }
        WeatherType::Sandstorm => {
            painter.rect_filled(screen, 0.0, egui::Color32::from_rgba_unmultiplied(200, 150, 80, 55));
            for i in 0..90 {
                let speed = 260.0 + hash(i, 1) * 180.0;
                let x = (hash(i, 2) * w + t * speed) % (w + 60.0) - 30.0;
                let y = (hash(i, 5) * h + (t * 2.0 + hash(i, 3) * 6.28).sin() * 12.0) % h;
                let len = 12.0 + hash(i, 6) * 24.0;
                let pos = egui::pos2(screen.min.x + snap(x), screen.min.y + snap(y));
                painter.rect_filled(egui::Rect::from_min_size(pos, egui::vec2(snap(len), px)), 0.0,
                    egui::Color32::from_rgba_unmultiplied(235, 195, 130, 140));
            }
        }
        WeatherType::Fog => {
            painter.rect_filled(screen, 0.0, egui::Color32::from_rgba_unmultiplied(225, 228, 235, 40));
            // Slow bands of mist drifting sideways
            for i in 0..6 {
                let y = screen.min.y + h * (0.1 + 0.16 * i as f32);
                let x = (hash(i, 1) * w + t * (10.0 + 6.0 * hash(i, 2))) % (w * 1.5) - w * 0.5;
                let band = egui::Rect::from_min_size(egui::pos2(screen.min.x + x, y), egui::vec2(w * 0.7, h * 0.08));
                painter.rect_filled(band, h * 0.04, egui::Color32::from_rgba_unmultiplied(235, 238, 242, 34));
            }
        }
        WeatherType::Clear => {}
    }
    ctx.request_repaint();
}

/// How far across the screen the grid cursor is, from 0 (left edge) to 1 (right)
fn cursor_screen_fraction(
    cursor: &GridCursor,
    map: &GameMap,
    cameras: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) -> Option<f32> {
    let (camera, transform) = cameras.single().ok()?;
    let size = camera.logical_viewport_size()?;
    let at = camera.world_to_viewport(transform, GridPosition::new(cursor.x, cursor.y).to_world(map)).ok()?;
    Some(at.x / size.x.max(1.0))
}

/// A commander's portrait in a framed square, or their initial when there's no
/// art yet, faded to `alpha` (0 = gone, 1 = solid)
fn portrait_square(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32, name: &str, texture: Option<egui::TextureId>, alpha: f32) {
    painter.rect_filled(rect.expand(5.0), 6, theme::shade(color, 0.4).gamma_multiply(alpha));
    match texture {
        Some(texture) => {
            painter.image(texture, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE.gamma_multiply(alpha));
        }
        None => {
            painter.rect_filled(rect, 4, theme::PARCHMENT_DEEP.gamma_multiply(alpha));
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, name.chars().next().unwrap_or('?').to_string(),
                egui::FontId::new(rect.height() * 0.47, theme::title_family()), theme::WOOD_DARK.gamma_multiply(alpha));
        }
    }
}

/// Text with a drop shadow, for banners over the board
fn shadowed(painter: &egui::Painter, pos: egui::Pos2, align: egui::Align2, text: &str, font: egui::FontId, color: egui::Color32) {
    let offset = font.size / 18.0 + 1.0;
    // The shadow fades along with the text
    let shadow = egui::Color32::from_black_alpha((140.0 * color.a() as f32 / 255.0) as u8);
    painter.text(pos + egui::vec2(offset, offset), align, text, font.clone(), shadow);
    painter.text(pos, align, text, font, color);
}

/// Advance Wars turn presentation. The battle opens with the map's name and
/// the two commanders facing off; then, and at the start of every turn, a band
/// in the army's colour sweeps across with its commander and "Day N".
pub fn draw_turn_banner(
    mut contexts: EguiContexts,
    banner: Res<TurnBanner>,
    commanders: Res<Commanders>,
    game_data: Res<GameData>,
    battle_config: Res<BattleConfig>,
    setup: Res<BattleSetupState>,
    portraits: Res<PortraitTextures>,
) {
    let Some(faction) = banner.faction else { return };
    if !banner.playing() || setup.needs_setup {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let screen = ctx.content_rect();
    let ease = |x: f32| { let x = x.clamp(0.0, 1.0); 1.0 - (1.0 - x).powi(3) };

    egui::Area::new(egui::Id::new("turn_banner"))
        .fixed_pos(screen.min)
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter();
            let t = banner.elapsed;

            if banner.intro && t < MATCH_INTRO_SECS {
                // Match intro: the map's name, then both commanders slide in from the sides
                let fade_out = 1.0 - ease((t - (MATCH_INTRO_SECS - 0.4)) / 0.4);
                let alpha = (ease(t / 0.3) * fade_out).clamp(0.0, 1.0);
                painter.rect_filled(screen, 0, egui::Color32::from_black_alpha((alpha * 200.0) as u8));
                let title = setup.selected_map.name();
                shadowed(painter, egui::pos2(screen.center().x, screen.top() + screen.height() * 0.11), egui::Align2::CENTER_CENTER,
                    title, egui::FontId::new(56.0, theme::title_family()), theme::PARCHMENT.gamma_multiply(alpha));

                let sides = [
                    (battle_config.player_faction, -1.0_f32),
                    (battle_config.next_faction(battle_config.player_faction), 1.0),
                ];
                for (i, (army, side)) in sides.into_iter().enumerate() {
                    let enter = ease((t - 0.3 - i as f32 * 0.2) / 0.45);
                    // Fade last: shading an already faded colour makes it opaque and dark
                    let color = theme::faction_color(&game_data, army);
                    let co_id = commanders.get_active(army);
                    let co = game_data.commander(co_id);
                    // The portraits are the stars: as large as the screen allows
                    let size = (screen.height() * 0.42).min(screen.width() * 0.25);
                    let center = egui::pos2(
                        screen.center().x + side * (size / 2.0 + 70.0 + (1.0 - enter) * screen.width() * 0.6),
                        screen.top() + screen.height() * 0.56,
                    );
                    let card = egui::Rect::from_center_size(center, egui::vec2(size + 40.0, size + 120.0));
                    painter.rect_filled(card, 10, theme::shade(color, 0.8).gamma_multiply(alpha));
                    painter.rect_stroke(card, 10, theme::stroke(4.0, theme::shade(color, 0.45).gamma_multiply(alpha)), egui::StrokeKind::Inside);
                    let portrait = egui::Rect::from_min_size(egui::pos2(card.left() + 20.0, card.top() + 20.0), egui::vec2(size, size));
                    portrait_square(painter, portrait, color, &co.name, portraits.get(co_id), alpha);
                    shadowed(painter, egui::pos2(card.center().x, portrait.bottom() + 36.0), egui::Align2::CENTER_CENTER,
                        &co.name, egui::FontId::new(34.0, theme::title_family()), theme::PARCHMENT.gamma_multiply(alpha));
                    painter.text(egui::pos2(card.center().x, portrait.bottom() + 74.0), egui::Align2::CENTER_CENTER,
                        game_data.faction_name(army), egui::FontId::new(17.0, theme::bold_family()),
                        theme::shade(theme::PARCHMENT, 0.85).gamma_multiply(alpha));
                }
                let vs = ease((t - 0.8) / 0.3);
                if vs > 0.0 {
                    shadowed(painter, egui::pos2(screen.center().x, screen.top() + screen.height() * 0.56), egui::Align2::CENTER_CENTER, "VS",
                        egui::FontId::new(40.0 + 30.0 * (1.0 - vs), theme::title_family()), theme::GOLD_LIGHT.gamma_multiply(vs * alpha));
                }
                return;
            }

            // Day banner: in, hold, out
            let t = if banner.intro { t - MATCH_INTRO_SECS } else { t };
            let enter = ease(t / 0.3);
            let leave = ease((t - (DAY_BANNER_SECS - 0.3)) / 0.3);
            let slide = (1.0 - enter) * screen.width() - leave * screen.width();
            let fade = (enter * (1.0 - leave)).clamp(0.0, 1.0);
            painter.rect_filled(screen, 0, egui::Color32::from_black_alpha((fade * 70.0) as u8));

            let color = theme::faction_color(&game_data, faction);
            let band = egui::Rect::from_center_size(screen.center() + egui::vec2(slide, 0.0), egui::vec2(screen.width() * 1.2, 150.0));
            painter.rect_filled(band, 0, theme::shade(color, 0.85));
            for edge in [band.top(), band.bottom()] {
                painter.line_segment([egui::pos2(band.left(), edge), egui::pos2(band.right(), edge)],
                    theme::stroke(5.0, theme::shade(color, 0.45)));
            }
            let stripe = egui::Rect::from_min_max(egui::pos2(band.left(), band.top() + 9.0), egui::pos2(band.right(), band.top() + 14.0));
            painter.rect_filled(stripe, 0, egui::Color32::from_white_alpha(60));

            let co_id = commanders.get_active(faction);
            let co = game_data.commander(co_id);
            // A big portrait breaking out of the band above and below, as in Advance Wars
            let size = (band.height() * 1.6).min(screen.height() * 0.4);
            let portrait = egui::Rect::from_center_size(egui::pos2(screen.center().x - 150.0 - size / 2.0 + slide, band.center().y), egui::vec2(size, size));
            painter.rect_filled(portrait.expand(9.0).translate(egui::vec2(6.0, 6.0)), 8, egui::Color32::from_black_alpha(90));
            portrait_square(painter, portrait, color, &co.name, portraits.get(co_id), 1.0);
            let text_x = portrait.right() + 36.0;
            shadowed(painter, egui::pos2(text_x, band.center().y - 10.0), egui::Align2::LEFT_CENTER, &format!("Day {}", banner.day),
                egui::FontId::new(72.0, theme::title_family()), theme::PARCHMENT);
            painter.text(egui::pos2(text_x, band.center().y + 46.0), egui::Align2::LEFT_CENTER,
                format!("{} · {}", game_data.faction_name(faction), co.name),
                egui::FontId::new(18.0, theme::bold_family()), theme::shade(theme::PARCHMENT, 0.85));
        });
    ctx.request_repaint();
}

/// How long the power cut-in plays, in seconds
const CUT_IN_SECS: f64 = 2.2;

/// Advance Wars style power cut-in: the screen darkens and a band in the army's
/// colour sweeps across with the commander's portrait and the power's name
pub fn draw_power_cut_in(
    mut contexts: EguiContexts,
    mut events: MessageReader<PowerActivatedEvent>,
    commanders: Res<Commanders>,
    game_data: Res<GameData>,
    portraits: Res<PortraitTextures>,
    time: Res<Time>,
    mut playing: Local<Option<(crate::game::Faction, PowerLevel, f64)>>,
) {
    let now = time.elapsed_secs_f64();
    for event in events.read() {
        *playing = Some((event.faction, event.level, now));
    }
    let Some((faction, level, started)) = *playing else { return };
    let t = now - started;
    if t > CUT_IN_SECS {
        *playing = None;
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let co_id = commanders.get_active(faction);
    let co = game_data.commander(co_id);
    let power = match level {
        PowerLevel::Super => Some(&co.super_power),
        _ => co.power.as_ref(),
    };
    let Some(power) = power else { return };
    let color = theme::faction_color(&game_data, faction);
    let screen = ctx.content_rect();

    // Ease in for 0.35s, hold, ease out over the last 0.35s
    let ease = |x: f64| { let x = x.clamp(0.0, 1.0); 1.0 - (1.0 - x).powi(3) };
    let enter = ease(t / 0.35) as f32;
    let leave = ease((t - (CUT_IN_SECS - 0.35)) / 0.35) as f32;
    let slide = (1.0 - enter) * -screen.width() + leave * screen.width();
    let fade = (enter * (1.0 - leave)).clamp(0.0, 1.0);

    egui::Area::new(egui::Id::new("power_cut_in"))
        .fixed_pos(screen.min)
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter();
            painter.rect_filled(screen, 0, egui::Color32::from_black_alpha((fade * 120.0) as u8));
            let band_h = 170.0;
            let band = egui::Rect::from_center_size(screen.center() + egui::vec2(slide, 0.0), egui::vec2(screen.width() * 1.2, band_h));
            // A slanted band with a pale leading stripe
            let slant = 40.0;
            let points = vec![
                egui::pos2(band.left() + slant, band.top()), egui::pos2(band.right(), band.top()),
                egui::pos2(band.right() - slant, band.bottom()), egui::pos2(band.left(), band.bottom()),
            ];
            painter.add(egui::Shape::convex_polygon(points, theme::shade(color, 0.85), theme::stroke(4.0, theme::shade(color, 0.45))));
            let stripe = egui::Rect::from_min_max(egui::pos2(band.left(), band.top() + 10.0), egui::pos2(band.right(), band.top() + 16.0));
            painter.rect_filled(stripe, 0, egui::Color32::from_white_alpha(70));

            // Portrait in a framed square on the left of the band
            let portrait = egui::Rect::from_center_size(egui::pos2(screen.center().x - 260.0 + slide, band.center().y), egui::vec2(136.0, 136.0));
            portrait_square(painter, portrait, color, &co.name, portraits.get(co_id), 1.0);

            // "SUPER CO POWER!" and the power's name
            let text_x = portrait.right() + 34.0;
            let kind = if level == PowerLevel::Super { "SUPER CO POWER!" } else { "CO POWER!" };
            painter.text(egui::pos2(text_x + 2.0, band.center().y - 26.0), egui::Align2::LEFT_CENTER, kind,
                egui::FontId::new(26.0, theme::title_family()), egui::Color32::from_black_alpha(120));
            painter.text(egui::pos2(text_x, band.center().y - 28.0), egui::Align2::LEFT_CENTER, kind,
                egui::FontId::new(26.0, theme::title_family()), theme::GOLD_LIGHT);
            painter.text(egui::pos2(text_x + 3.0, band.center().y + 23.0), egui::Align2::LEFT_CENTER, &power.name,
                egui::FontId::new(52.0, theme::title_family()), egui::Color32::from_black_alpha(140));
            painter.text(egui::pos2(text_x, band.center().y + 20.0), egui::Align2::LEFT_CENTER, &power.name,
                egui::FontId::new(52.0, theme::title_family()), theme::PARCHMENT);
            painter.text(egui::pos2(text_x, band.center().y + 60.0), egui::Align2::LEFT_CENTER, &co.name,
                egui::FontId::new(18.0, theme::bold_family()), theme::shade(theme::PARCHMENT, 0.85));
        });
    ctx.request_repaint();
}
