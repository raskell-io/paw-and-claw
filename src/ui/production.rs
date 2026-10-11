//! Unit production at owned bases

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::hud::format_funds;
use super::input::MenuInput;
use super::theme;
use crate::game::{
    spawn_unit_with_state, AiState, BattleConfig, Commanders, FactionFunds, GameData, GameMap, GameResult,
    ProductionState, SpriteAssets, TurnState, Unit, UnitType,
};

/// Read-only context for the production menu (separate from the HUD's, which
/// reads funds immutably)
#[derive(SystemParam)]
pub struct ProductionContext<'w> {
    game_data: Res<'w, GameData>,
    game_result: Res<'w, GameResult>,
    ai_state: Res<'w, AiState>,
    battle_config: Res<'w, BattleConfig>,
}

#[derive(SystemParam)]
pub struct UnitSpawner<'w, 's> {
    commands: Commands<'w, 's>,
    map: Res<'w, GameMap>,
    sprite_assets: Res<'w, SpriteAssets>,
    images: Res<'w, Assets<Image>>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

pub fn draw_production_menu(
    mut contexts: EguiContexts,
    ctx_res: ProductionContext,
    mut production: ResMut<ProductionState>,
    mut funds: ResMut<FactionFunds>,
    turn_state: Res<TurnState>,
    commanders: Res<Commanders>,
    unit_textures: Res<super::UnitTextures>,
    time: Res<Time>,
    mut spawner: UnitSpawner,
    input: MenuInput,
    mut selected: Local<usize>,
    mut shown: Local<bool>,
    mut shortfall: Local<Option<u32>>,
) {
    let ai_turn = ctx_res.ai_state.enabled && ctx_res.battle_config.is_ai_faction(turn_state.current_faction);
    if !production.active || ctx_res.game_result.game_over || ai_turn {
        production.active = false;
        *shown = false;
        return;
    }
    // Ignore the keypress that opened the menu
    let accept_input = std::mem::replace(&mut *shown, true);
    if !accept_input {
        *selected = 0;
        *shortfall = None;
    }

    let game_data: &GameData = &ctx_res.game_data;
    let faction = turn_state.current_faction;
    let co_cost = commanders.get_bonuses(faction, game_data).cost;
    let available = funds.get(faction);
    // Everything this building makes (`builds` in terrain.ron), cheapest first
    let (bx, by) = production.base_position;
    let Some(building) = spawner.map.get(bx, by) else {
        production.active = false;
        return;
    };
    let roster: Vec<(UnitType, u32)> = game_data.buildable_units(building).into_iter()
        .map(|unit_type| (unit_type, game_data.production_cost(unit_type, co_cost)))
        .collect();
    if roster.is_empty() {
        production.active = false;
        return;
    }
    *selected = (*selected).min(roster.len() - 1);
    let mut moved_selection = false;

    let mut build: Option<usize> = None;
    let mut close = false;
    if accept_input {
        moved_selection = input.step_vertical(&mut selected, roster.len());
        if input.confirm() {
            build = Some(*selected);
        }
        close |= input.back();
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    theme::backdrop(ctx, "production_backdrop", 110);
    egui::Area::new(egui::Id::new("production_menu"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            theme::card().show(ui, |ui| {
                ui.set_width(440.0);
                ui.horizontal(|ui| {
                    ui.label(theme::display("Muster Troops", 26.0).color(theme::WOOD_DARK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(theme::bold(format_funds(available), 19.0));
                        ui.label(theme::body("Funds", 15.0).color(theme::INK_SOFT));
                    });
                });
                if let Some(missing) = *shortfall {
                    theme::chip(ui, &format!("Need {} more funds", format_funds(missing)), theme::BERRY);
                }
                if co_cost < 1.0 {
                    theme::chip(ui, &format!("Commander discount {:.0}%", (1.0 - co_cost) * 100.0), theme::LEAF);
                }
                ui.add_space(6.0);

                let color = theme::faction_color(game_data, faction);
                // The highlighted unit marches in place, at the board's selected-unit pace
                let tick = (time.elapsed_secs() * 8.0) as u32;
                // Long rosters scroll, keeping the highlighted row in view
                egui::ScrollArea::vertical().max_height(5.5 * 58.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (i, (unit_type, cost)) in roster.iter().enumerate() {
                        let affordable = available >= *cost;
                        let sprite = unit_textures.get(faction, *unit_type)
                            .map(|(texture, frames)| (texture, if i == *selected { tick % frames } else { 0 }, frames));
                        let response = roster_row(ui, game_data, *unit_type, *cost, color, sprite, affordable, i == *selected);
                        if i == *selected && moved_selection {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }
                        if theme::pointer_selects(ui, &response) {
                            *selected = i;
                        }
                        if response.clicked() {
                            build = Some(i);
                        }
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    theme::key_hint(ui, "↑↓", "Choose", false);
                    theme::key_hint(ui, "Enter", "Build", false);
                    theme::key_hint(ui, "Esc", "Close", false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    });
                });
            });
        });

    if let Some(i) = build {
        let (unit_type, cost) = roster[i];
        *shortfall = cost.checked_sub(available).filter(|&m| m > 0);
        if funds.spend(faction, cost) {
            let (x, y) = production.base_position;
            // Fresh units can't act on the turn they're built
            let mut unit = Unit::new(unit_type, game_data);
            unit.exhausted = true;
            let s = &mut spawner;
            spawn_unit_with_state(&mut s.commands, &s.map, &mut s.meshes, &mut s.materials, &s.sprite_assets,
                &s.images, game_data, faction, unit, x, y);
            info!("Built {:?} at ({}, {}) for {} funds", unit_type, x, y, cost);
            close = true;
        }
    }
    if close {
        production.active = false;
        production.base_entity = None;
    }
}

fn roster_row(
    ui: &mut egui::Ui,
    game_data: &GameData,
    unit_type: UnitType,
    cost: u32,
    faction_color: egui::Color32,
    sprite: Option<(egui::TextureId, u32, u32)>,
    affordable: bool,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 58.0), egui::Sense::click());
    let painter = ui.painter();
    let hot = selected || response.hovered();
    painter.rect(rect, 6, if hot { theme::GOLD_LIGHT } else { egui::Color32::TRANSPARENT },
        theme::stroke(if selected { 1.5 } else { 0.0 }, theme::WOOD), egui::StrokeKind::Inside);

    let data = game_data.unit(unit_type);
    let stats = &data.stats;
    let token = egui::pos2(rect.left() + 28.0, rect.center().y);
    let token_color = if affordable { faction_color } else { theme::shade(faction_color, 0.5) };
    if let Some((texture, frame, frames)) = sprite {
        // The unit's own map sprite (one frame of its strip) on a soft faction-colored disc
        painter.circle_filled(token + egui::vec2(0.0, 8.0), 16.0, token_color.gamma_multiply(0.25));
        let size = 48.0; // 1.5x the 32px sprite
        let uv_width = 1.0 / frames.max(1) as f32;
        let uv = egui::Rect::from_min_max(egui::pos2(frame as f32 * uv_width, 0.0), egui::pos2((frame + 1) as f32 * uv_width, 1.0));
        let tint = if affordable { egui::Color32::WHITE } else { egui::Color32::from_gray(120) };
        painter.image(texture, egui::Rect::from_center_size(token, egui::vec2(size, size)), uv, tint);
    } else {
        painter.circle(token, 18.0, token_color, theme::stroke(2.0, theme::shade(token_color, 0.5)));
        painter.text(token + egui::vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, data.symbol.to_string(),
            egui::FontId::new(20.0, theme::title_family()), theme::PARCHMENT);
    }

    let ink = if affordable { theme::INK } else { theme::INK_SOFT };
    let x = rect.left() + 56.0;
    painter.text(egui::pos2(x, rect.top() + 18.0), egui::Align2::LEFT_CENTER, &data.name,
        egui::FontId::new(18.0, theme::bold_family()), ink);
    let (min_r, max_r) = stats.attack_range;
    let range = if min_r == max_r { format!("{min_r}") } else { format!("{min_r}–{max_r}") };
    // Keep the description clear of the price column
    let text_clip = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right() - 84.0, rect.bottom()));
    painter.with_clip_rect(text_clip).text(egui::pos2(x, rect.top() + 39.0), egui::Align2::LEFT_CENTER,
        format!("Move {} · Range {} · {}", stats.movement, range, data.description),
        egui::FontId::proportional(14.0), theme::INK_SOFT);

    painter.text(egui::pos2(rect.right() - 12.0, rect.center().y), egui::Align2::RIGHT_CENTER, format_funds(cost),
        egui::FontId::new(18.0, theme::bold_family()), if affordable { theme::WOOD_DARK } else { theme::BERRY });

    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
