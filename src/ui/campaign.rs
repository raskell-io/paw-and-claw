//! Campaign screens: the mission list (in place of skirmish setup) and the
//! story dialogue that plays over the board

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::icons::Icon;
use super::input::MenuInput;
use super::setup::{column_card, minimap, ArmyStart, BattleSpawner};
use super::theme::{self, Tone};
use super::{BattleSetupState, PortraitTextures};
use crate::game::{
    BattleConfig, BattleMap, CampaignProgress, Dialogue, Faction, FactionFunds, FogOfWar, GameData, Line, MapData,
    MissionData, Weather,
};
use crate::states::GameState;

/// The mission list's cursor, and a mission waiting to start
#[derive(Resource, Default)]
pub struct MissionSelect {
    pub selected: usize,
    /// Start this mission (by key) as soon as the last battle's board is gone:
    /// "Next mission" and "Try again" on the result screen
    pub launch: Option<String>,
}

/// Start a mission's battle and its briefing
fn start_mission(spawner: &mut BattleSpawner, game_data: &GameData, mission: &MissionData, dialogue: &mut Dialogue) {
    let Ok(map) = mission.map_data() else { return };
    let armies: Vec<ArmyStart> = mission.armies.iter()
        .map(|a| ArmyStart { faction: a.faction, co: a.commander, human: a.player, team: a.team })
        .collect();
    let holdout = mission.day_limit().map(|_| mission.player());
    let battle_map = BattleMap { id: None, name: mission.name.clone(), mission: Some(mission.key.clone()) };
    spawner.start(game_data, &armies, mission.day_limit(), holdout, &map, battle_map);

    let mut funds = FactionFunds::default();
    for army in &mission.armies {
        funds.add(army.faction, army.funds);
    }
    let mut fog = FogOfWar::default();
    fog.enabled = mission.fog;
    // A mission's weather holds for the whole battle
    let mut weather = mission.weather.map(Weather::new).unwrap_or_default();
    weather.dynamic_weather = mission.weather.is_none() && weather.dynamic_weather;
    spawner.insert_resource(funds);
    spawner.insert_resource(fog);
    spawner.insert_resource(weather);
    dialogue.start(&mission.briefing);
    info!("Mission started: {}", mission.name);
}

pub fn draw_campaign(
    mut contexts: EguiContexts,
    mut setup: ResMut<BattleSetupState>,
    mut select: ResMut<MissionSelect>,
    mut spawner: BattleSpawner,
    mut next_state: ResMut<NextState<GameState>>,
    game_data: Res<GameData>,
    progress: Res<CampaignProgress>,
    mut dialogue: ResMut<Dialogue>,
    input: MenuInput,
    time: Res<Time>,
    portraits: Res<PortraitTextures>,
    mut maps: Local<Vec<Option<MapData>>>,
    mut shown: Local<bool>,
) {
    if !setup.needs_setup || !setup.campaign {
        *shown = false;
        return;
    }
    let campaign = &game_data.campaign;
    let missions = &campaign.missions;
    if missions.is_empty() {
        next_state.set(GameState::Menu);
        return;
    }
    // The keypress that opened this screen must not also start a mission
    let accept_input = std::mem::replace(&mut *shown, true);
    let Ok(ctx) = contexts.ctx_mut() else { return };

    // A mission picked on the result screen starts once the old board is gone
    if let Some(key) = select.launch.clone() {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            theme::forest_backdrop(ui.painter(), ui.max_rect(), time.elapsed_secs());
        });
        if spawner.board_clear() && spawner.sprites_ready() {
            select.launch = None;
            if let Some(mission) = campaign.mission(&key) {
                start_mission(&mut spawner, &game_data, mission, &mut dialogue);
                setup.needs_setup = false;
            }
        }
        return;
    }

    if maps.len() != missions.len() {
        *maps = missions.iter().map(|mission| {
            let factions: Vec<Faction> = mission.armies.iter().map(|a| a.faction).collect();
            mission.map_data().ok().map(|map| map.with_armies(&factions))
        }).collect();
    }
    // Opening the list lands on the mission to play next
    if !accept_input {
        if let Some(next) = progress.next(campaign) {
            select.selected = next;
        }
    }
    let mut selected = select.selected.min(missions.len() - 1);
    let mut start = false;
    let mut back = false;
    if accept_input {
        input.step_vertical(&mut selected, missions.len());
        start = input.confirm();
        back = input.back();
    }
    let mission = &missions[selected];
    let unlocked = progress.unlocked(campaign, selected);
    let sprites_ready = spawner.sprites_ready();
    let ready = unlocked && sprites_ready && maps[selected].is_some();
    let mut pick = None;

    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let screen = ui.max_rect();
        theme::forest_backdrop(ui.painter(), screen, time.elapsed_secs());

        // Same frame as skirmish setup: header, columns that take the height
        // left over and scroll inside, footer
        let content = screen.shrink2(egui::vec2((screen.width() - 1180.0).max(48.0) / 2.0, 20.0));
        let compact = content.height() < 640.0;
        let header_h = if compact { 50.0 } else { 92.0 };
        let footer_h = 104.0;
        let header = egui::Rect::from_min_size(content.min, egui::vec2(content.width(), header_h));
        let footer = egui::Rect::from_min_max(egui::pos2(content.left(), content.bottom() - footer_h), content.max);
        let columns = egui::Rect::from_min_max(egui::pos2(content.left(), header.bottom()), egui::pos2(content.right(), footer.top() - 12.0));

        ui.scope_builder(egui::UiBuilder::new().max_rect(header), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(theme::display(&campaign.name, if compact { 34.0 } else { 44.0 }).color(theme::PARCHMENT));
                if !compact {
                    ui.label(theme::body(&campaign.tagline, 18.0).color(theme::GOLD_LIGHT));
                }
            });
        });

        let gap = 16.0;
        let list_w = ((columns.width() - gap) * 0.38).floor();
        let detail_w = columns.width() - gap - list_w;
        let col_h = columns.height();
        ui.scope_builder(egui::UiBuilder::new().max_rect(columns), |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                column_card(ui, list_w, col_h, "Missions", true, |ui| {
                    egui::ScrollArea::vertical().id_salt("campaign_missions").auto_shrink([false, false]).show(ui, |ui| {
                        for (i, row) in missions.iter().enumerate() {
                            let open = progress.unlocked(campaign, i);
                            let best = progress.best(&row.key);
                            let label = if open { format!("{}. {}", i + 1, row.name) } else { format!("{}. ???", i + 1) };
                            let detail = match (best, open) {
                                (Some(1), _) => "Cleared in 1 day".to_string(),
                                (Some(days), _) => format!("Cleared in {days} days"),
                                (None, true) => "Not yet cleared".to_string(),
                                (None, false) => "Locked".to_string(),
                            };
                            let icon = if best.is_some() { Icon::Star } else if open { Icon::Swords } else { Icon::Door };
                            let response = theme::menu_item_icon(ui, Some(icon), &label, Some(&detail), i == selected, true,
                                if open { Tone::Normal } else { Tone::Muted });
                            if i == selected {
                                response.scroll_to_me(None);
                            }
                            if theme::pointer_selects(ui, &response) || response.clicked() {
                                pick = Some(i);
                            }
                        }
                    });
                });
                column_card(ui, detail_w, col_h, &format!("Mission {}", selected + 1), false, |ui| {
                    if !unlocked {
                        ui.add_space(8.0);
                        ui.label(theme::display("Locked", 28.0).color(theme::INK_SOFT));
                        let before = &missions[selected - 1].name;
                        ui.label(theme::body(format!("Clear \"{before}\" to open this mission."), 16.0).color(theme::INK_SOFT).italics());
                        return;
                    }
                    ui.label(theme::display(&mission.name, if compact { 24.0 } else { 30.0 }).color(theme::WOOD_DARK));
                    ui.horizontal_wrapped(|ui| {
                        theme::chip(ui, &mission.objective.describe(), theme::WOOD);
                        if mission.fog {
                            theme::chip(ui, "Fog of war", theme::SKY);
                        }
                        if let Some(weather) = mission.weather {
                            theme::chip(ui, weather.name(), theme::SKY);
                        }
                        if let Some(days) = progress.best(&mission.key) {
                            theme::chip(ui, &format!("Best: {days} day{}", if days == 1 { "" } else { "s" }), theme::LEAF);
                        }
                    });
                    ui.add_space(6.0);
                    if let Some(map) = &maps[selected] {
                        let height = (ui.available_height() - if compact { 70.0 } else { 110.0 }).clamp(70.0, 230.0);
                        minimap(ui, &game_data, map, egui::vec2(ui.available_width(), height));
                    }
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().id_salt("campaign_summary").auto_shrink([false, false]).show(ui, |ui| {
                        ui.label(theme::body(&mission.summary, 16.0).color(theme::INK));
                    });
                });
            });
        });

        ui.scope_builder(egui::UiBuilder::new().max_rect(footer), |ui| {
            theme::card().show(ui, |ui| {
                ui.set_width(footer.width() - 28.0);
                ui.horizontal(|ui| {
                    if unlocked {
                        matchup(ui, &game_data, &portraits, mission);
                    } else {
                        ui.label(theme::body("The story goes on from here.", 16.0).color(theme::INK_SOFT).italics());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if !unlocked { "Locked" } else if !sprites_ready { "Loading art…" } else { "Begin Mission" };
                        if theme::primary_button(ui, label, ready, true).clicked() {
                            start = true;
                        }
                    });
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                theme::key_hint(ui, "↑↓", "Choose", true);
                theme::key_hint(ui, "Enter", "Begin mission", true);
                if theme::key_hint_button(ui, "Esc", "Back", true) {
                    back = true;
                }
            });
        });
    });

    select.selected = pick.unwrap_or(selected);
    if back {
        next_state.set(GameState::Menu);
        return;
    }
    // A click on another row and the button can't land in one frame, but keep
    // the mission that was on screen when it was confirmed
    if start && ready {
        select.selected = selected;
        start_mission(&mut spawner, &game_data, mission, &mut dialogue);
        setup.needs_setup = false;
    }
}

/// The mission's commanders: the player's side, then the enemy
fn matchup(ui: &mut egui::Ui, game_data: &GameData, portraits: &PortraitTextures, mission: &MissionData) {
    let player_team = mission.armies.iter().find(|a| a.player).map(|a| a.team);
    let small = mission.armies.len() > 2;
    let sides = [true, false].map(|friendly| {
        mission.armies.iter().filter(|a| (Some(a.team) == player_team) == friendly).collect::<Vec<_>>()
    });
    for (side, armies) in sides.iter().enumerate() {
        if side == 1 {
            ui.label(theme::display("vs", if small { 18.0 } else { 26.0 }).color(theme::INK_SOFT));
        }
        for army in armies {
            let co = game_data.commander(army.commander);
            let color = theme::faction_color(game_data, army.faction);
            theme::portrait_facing(ui, if small { 36.0 } else { 44.0 }, color, &co.name, portraits.get(army.commander), side == 1);
            ui.vertical(|ui| {
                ui.label(theme::display(&co.name, if small { 16.0 } else { 20.0 }));
                let role = if army.player { "You" } else if side == 0 { "Ally" } else { "Enemy" };
                ui.label(theme::body(format!("{role} · {}", game_data.faction_name(army.faction)), if small { 12.0 } else { 15.0 })
                    .color(theme::INK_SOFT));
            });
        }
    }
}

/// The story dialogue: a commander's portrait and their line along the bottom
/// of the screen, over the board. Confirm (or a tap) goes on, Esc skips it all.
pub fn draw_dialogue(
    mut contexts: EguiContexts,
    mut dialogue: ResMut<Dialogue>,
    game_data: Res<GameData>,
    battle_config: Res<BattleConfig>,
    portraits: Res<PortraitTextures>,
    input: MenuInput,
    mut shown: Local<bool>,
) {
    let Some(Line(speaker, text)) = dialogue.current().cloned() else {
        *shown = false;
        return;
    };
    // The press that brought the dialogue up must not also turn its first page
    let accept_input = std::mem::replace(&mut *shown, true);
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let co = game_data.commander(speaker);
    let color = theme::faction_color(&game_data, co.faction);
    // The player's side speaks from the left, everyone else from the right
    let player = battle_config.player_faction;
    let friend = battle_config.allied(co.faction, player);
    let typed = dialogue.typed();
    let whole = typed >= text.chars().count();

    let mut advance = accept_input && input.confirm();
    let mut skip = accept_input && input.back();
    egui::Area::new(egui::Id::new("dialogue"))
        .fixed_pos(egui::Pos2::ZERO)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            let screen = ui.ctx().content_rect();
            let tapped = ui.allocate_rect(screen, egui::Sense::click()).clicked();
            let painter = ui.painter().clone();
            painter.rect_filled(screen, 0, egui::Color32::from_black_alpha(70));

            let width = (screen.width() - 32.0).min(960.0);
            let face = (screen.height() * 0.24).clamp(96.0, 150.0);
            let pad = 14.0;
            let panel = egui::Rect::from_min_size(
                egui::pos2(screen.center().x - width / 2.0, screen.bottom() - face - pad * 2.0 - 20.0),
                egui::vec2(width, face + pad * 2.0),
            );
            painter.rect_filled(panel.translate(egui::vec2(0.0, 4.0)), 10, egui::Color32::from_black_alpha(90));
            painter.rect(panel, 10, theme::PARCHMENT, theme::stroke(3.0, theme::shade(color, 0.6)), egui::StrokeKind::Inside);

            let portrait = egui::Rect::from_min_size(
                egui::pos2(if friend { panel.left() + pad } else { panel.right() - pad - face }, panel.top() + pad),
                egui::vec2(face, face),
            );
            super::hud::portrait_square(&painter, portrait, color, &co.name, portraits.get(speaker), 1.0, !friend);

            let text_rect = if friend {
                egui::Rect::from_min_max(egui::pos2(portrait.right() + 18.0, panel.top() + pad), panel.max - egui::vec2(pad + 4.0, pad))
            } else {
                egui::Rect::from_min_max(panel.min + egui::vec2(pad + 4.0, pad), egui::pos2(portrait.left() - 18.0, panel.bottom() - pad))
            };
            painter.text(text_rect.left_top(), egui::Align2::LEFT_TOP, &co.name,
                egui::FontId::new(26.0, theme::title_family()), theme::shade(color, 0.7));
            painter.text(egui::pos2(text_rect.right(), text_rect.top() + 8.0), egui::Align2::RIGHT_TOP,
                game_data.faction_name(co.faction), egui::FontId::new(13.0, theme::bold_family()), theme::INK_SOFT);

            // The line is laid out whole, with the part still to come unseen,
            // so words don't jump to the next row as they're typed
            let split = text.char_indices().nth(typed).map_or(text.len(), |(at, _)| at);
            let font = egui::FontId::proportional(if face < 120.0 { 17.0 } else { 20.0 });
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = text_rect.width();
            job.append(&text[..split], 0.0, egui::TextFormat { font_id: font.clone(), color: theme::INK, ..Default::default() });
            job.append(&text[split..], 0.0, egui::TextFormat { font_id: font, color: egui::Color32::TRANSPARENT, ..Default::default() });
            let galley = painter.layout_job(job);
            painter.galley(text_rect.left_top() + egui::vec2(0.0, 36.0), galley, theme::INK);

            // Hints sit under the text, on the side away from the portrait
            let hints = egui::Rect::from_min_max(egui::pos2(text_rect.left(), panel.bottom() - 30.0), egui::pos2(text_rect.right(), panel.bottom() - 6.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(hints), |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::key_hint_button(ui, "Esc", "Skip", false) {
                        skip = true;
                    }
                    theme::key_hint(ui, "Enter", if whole { "Next" } else { "" }, false);
                });
            });
            if tapped && !skip {
                advance = true;
            }
        });

    if skip {
        dialogue.skip();
    } else if advance {
        dialogue.advance();
    }
}
