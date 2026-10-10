//! Skirmish setup: faction, commander and battlefield selection

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::input::MenuInput;
use super::theme::{self, Tone};
use super::BattleSetupState;
use crate::game::{
    get_builtin_map, spawn_map_from_data, spawn_units_from_data, BattleConfig, CommanderData, CommanderId,
    Commanders, Faction, FactionFunds, GameData, GameMap, MapData, MapId, SpriteAssets, TilesetTheme,
    TurnPhase, TurnStartEvent, TurnState,
};
use crate::states::GameState;

/// Factions offered to the player (Nether is the antagonist, not playable)
const PLAYABLE: [Faction; 4] = [Faction::Eastern, Faction::Northern, Faction::Western, Faction::Southern];

/// Which column keyboard/gamepad input is driving
#[derive(Default, Clone, Copy, PartialEq)]
pub enum Column {
    #[default]
    Faction,
    Commander,
    Map,
}

impl Column {
    const ALL: [Column; 3] = [Column::Faction, Column::Commander, Column::Map];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&c| c == self).unwrap()
    }
}

/// The faction the AI fields against the player's choice
pub fn opponent_for(player: Faction) -> Faction {
    match player {
        Faction::Eastern => Faction::Northern,
        Faction::Northern => Faction::Eastern,
        Faction::Western => Faction::Southern,
        Faction::Southern => Faction::Western,
        Faction::Nether => Faction::Eastern,
    }
}

/// Everything needed to spawn a battle
#[derive(SystemParam)]
pub struct BattleSpawner<'w, 's> {
    commands: Commands<'w, 's>,
    commanders: ResMut<'w, Commanders>,
    game_map: ResMut<'w, GameMap>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    sprite_assets: Res<'w, SpriteAssets>,
    images: Res<'w, Assets<Image>>,
    tileset_theme: Res<'w, TilesetTheme>,
    turn_start_events: MessageWriter<'w, TurnStartEvent>,
}

impl BattleSpawner<'_, '_> {
    fn start(&mut self, game_data: &GameData, player_faction: Faction, player_co: CommanderId, map_id: MapId) {
        let ai_faction = opponent_for(player_faction);
        self.commanders.set_commander(player_faction, player_co);

        let ai_cos = CommanderId::for_faction(ai_faction, game_data);
        if let Some(&ai_co) = ai_cos.get(rand::random::<usize>() % ai_cos.len().max(1)) {
            self.commanders.set_commander(ai_faction, ai_co);
            info!("Battle started! Map: {}, Player ({:?}): {:?}, AI ({:?}): {:?}",
                map_id.name(), player_faction, player_co, ai_faction, ai_co);
        }

        self.commands.insert_resource(BattleConfig::new(player_faction, ai_faction));
        self.commands.insert_resource(TurnState {
            current_faction: player_faction,
            turn_number: 1,
            phase: TurnPhase::Select,
        });
        // Everyone starts broke; the first player collects Day 1 income from
        // the turn start event below
        self.commands.insert_resource(FactionFunds::default());

        let map_data = get_builtin_map(map_id).with_factions(player_faction, ai_faction);
        spawn_map_from_data(&mut self.commands, &mut self.game_map, &mut self.meshes, &mut self.materials,
            &self.sprite_assets, &self.images, game_data, &map_data, *self.tileset_theme);
        spawn_units_from_data(&mut self.commands, &self.game_map, &mut self.meshes, &mut self.materials,
            &self.sprite_assets, &self.images, game_data, &map_data);

        self.turn_start_events.write(TurnStartEvent { faction: player_faction });
    }
}

pub fn draw_battle_setup(
    mut contexts: EguiContexts,
    mut setup: ResMut<BattleSetupState>,
    mut spawner: BattleSpawner,
    mut next_state: ResMut<NextState<GameState>>,
    game_data: Res<GameData>,
    input: MenuInput,
    time: Res<Time>,
    mut column: Local<Column>,
    mut maps: Local<Vec<(MapId, MapData)>>,
    mut shown: Local<bool>,
) {
    if !setup.needs_setup {
        *shown = false;
        return;
    }
    // The keypress that opened this screen must not also confirm it
    let accept_input = std::mem::replace(&mut *shown, true);
    if maps.is_empty() {
        *maps = MapId::all_builtin().into_iter().map(|id| (id, get_builtin_map(id))).collect();
    }

    let commanders = CommanderId::for_faction(setup.player_faction, &game_data);
    if setup.player_co.map_or(true, |co| !commanders.contains(&co)) {
        setup.player_co = commanders.first().copied();
    }

    // --- Keyboard / gamepad ---
    let mut start = false;
    if accept_input {
    let mut col = column.index();
    if input.step_horizontal(&mut col, Column::ALL.len()) || input.next_section() {
        if input.next_section() {
            col = (col + 1) % Column::ALL.len();
        }
    }
    *column = Column::ALL[col];
    match *column {
        Column::Faction => {
            let mut i = PLAYABLE.iter().position(|&f| f == setup.player_faction).unwrap_or(0);
            if input.step_vertical(&mut i, PLAYABLE.len()) {
                setup.player_faction = PLAYABLE[i];
                setup.player_co = None;
            }
        }
        Column::Commander => {
            let mut i = setup.player_co.and_then(|co| commanders.iter().position(|&c| c == co)).unwrap_or(0);
            if input.step_vertical(&mut i, commanders.len()) {
                setup.player_co = Some(commanders[i]);
            }
        }
        Column::Map => {
            let mut i = maps.iter().position(|(id, _)| *id == setup.selected_map).unwrap_or(0);
            if input.step_vertical(&mut i, maps.len()) {
                setup.selected_map = maps[i].0;
            }
        }
    }
    start = input.confirm();
    if input.back() {
        next_state.set(GameState::Menu);
        return;
    }
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    let opponent = opponent_for(setup.player_faction);

    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let screen = ui.max_rect();
        theme::forest_backdrop(ui.painter(), screen, time.elapsed_secs());

        let content = screen.shrink2(egui::vec2((screen.width() - 1180.0).max(48.0) / 2.0, 28.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(content), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(theme::display("Skirmish", 44.0).color(theme::PARCHMENT));
                ui.label(theme::body("Choose your army, your commander and the field of battle", 18.0).color(theme::GOLD_LIGHT));
            });
            ui.add_space(18.0);

            let gap = 16.0;
            let col_w = (content.width() - gap * 2.0) / 3.0;
            let col_h = (content.height() - 300.0).max(300.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                column_card(ui, col_w, col_h, "Faction", *column == Column::Faction, |ui| {
                    for faction in PLAYABLE {
                        if faction_row(ui, &game_data, faction, setup.player_faction == faction).clicked() {
                            setup.player_faction = faction;
                            setup.player_co = None;
                            *column = Column::Faction;
                        }
                    }
                });
                column_card(ui, col_w, col_h, "Commander", *column == Column::Commander, |ui| {
                    for &co_id in &commanders {
                        let co = game_data.commander(co_id);
                        if commander_row(ui, co, theme::faction_color(&game_data, setup.player_faction),
                            setup.player_co == Some(co_id)).clicked()
                        {
                            setup.player_co = Some(co_id);
                            *column = Column::Commander;
                        }
                    }
                });
                column_card(ui, col_w, col_h, "Battlefield", *column == Column::Map, |ui| {
                    let selected = maps.iter().find(|(id, _)| *id == setup.selected_map).map(|(_, m)| m.clone());
                    if let Some(map) = &selected {
                        let preview = map.with_factions(setup.player_faction, opponent);
                        minimap(ui, &game_data, &preview, egui::vec2(ui.available_width(), 120.0));
                        ui.add_space(2.0);
                        ui.label(theme::body(&map.description, 14.0).color(theme::INK_SOFT).italics());
                        ui.add_space(6.0);
                    }
                    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                        for (id, map) in maps.iter() {
                            let detail = format!("{} × {}", map.width, map.height);
                            let response = theme::menu_item(ui, id.name(), Some(&detail), setup.selected_map == *id, true, Tone::Normal);
                            if setup.selected_map == *id && *column == Column::Map {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                setup.selected_map = *id;
                                *column = Column::Map;
                            }
                        }
                    });
                });
            });

            ui.add_space(14.0);
            // Matchup banner and start button
            theme::card().show(ui, |ui| {
                ui.set_width(content.width() - 28.0);
                ui.horizontal(|ui| {
                    matchup_side(ui, &game_data, setup.player_faction,
                        setup.player_co.map(|co| game_data.commander_name(co).to_string()).unwrap_or_else(|| "—".into()));
                    ui.add_space(12.0);
                    ui.label(theme::display("vs", 26.0).color(theme::INK_SOFT));
                    ui.add_space(12.0);
                    matchup_side(ui, &game_data, opponent, "Random commander".into());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if theme::primary_button(ui, "Begin Battle", setup.player_co.is_some(), true).clicked() {
                            start = true;
                        }
                    });
                });
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                theme::key_hint(ui, "←→", "Column", true);
                theme::key_hint(ui, "↑↓", "Choose", true);
                theme::key_hint(ui, "Enter", "Begin battle", true);
                theme::key_hint(ui, "Esc", "Back", true);
            });
        });
    });

    if start {
        if let Some(co) = setup.player_co {
            spawner.start(&game_data, setup.player_faction, co, setup.selected_map);
            setup.needs_setup = false;
        }
    }
}

/// A titled card for one setup column; gold border when it has input focus
fn column_card(ui: &mut egui::Ui, width: f32, height: f32, title: &str, focused: bool, add: impl FnOnce(&mut egui::Ui)) {
    let frame = theme::card().stroke(theme::stroke(if focused { 3.0 } else { 2.0 }, if focused { theme::GOLD } else { theme::WOOD }));
    frame.show(ui, |ui| {
        ui.set_width(width - 28.0);
        ui.set_height(height);
        ui.vertical(|ui| {
            theme::heading(ui, title);
            add(ui);
        });
    });
}

fn faction_row(ui: &mut egui::Ui, game_data: &GameData, faction: Faction, selected: bool) -> egui::Response {
    let data = game_data.faction(faction);
    let color = theme::faction_color(game_data, faction);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 74.0), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect.shrink(2.0));
    let hot = selected || response.hovered();
    painter.rect(rect, 8, if hot { theme::GOLD_LIGHT } else { theme::PARCHMENT_DEEP },
        theme::stroke(if selected { 2.0 } else { 0.0 }, theme::WOOD), egui::StrokeKind::Inside);
    // Faction banner swatch
    let banner = egui::Rect::from_min_size(rect.min + egui::vec2(10.0, 10.0), egui::vec2(18.0, rect.height() - 20.0));
    painter.add(egui::Shape::convex_polygon(
        vec![banner.left_top(), banner.right_top(), banner.right_bottom(),
             egui::pos2(banner.center().x, banner.bottom() - 7.0), banner.left_bottom()],
        color, theme::stroke(1.0, theme::shade(color, 0.6)),
    ));
    let x = banner.right() + 12.0;
    painter.text(egui::pos2(x, rect.top() + 18.0), egui::Align2::LEFT_CENTER, &data.name,
        egui::FontId::new(19.0, theme::display_family()), theme::INK);
    painter.text(egui::pos2(x, rect.top() + 38.0), egui::Align2::LEFT_CENTER, &data.description,
        egui::FontId::proportional(14.0), theme::INK_SOFT);
    painter.text(egui::pos2(x, rect.top() + 57.0), egui::Align2::LEFT_CENTER, data.animals.join(" · "),
        egui::FontId::new(13.0, theme::bold_family()), theme::shade(color, 0.7));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn commander_row(ui: &mut egui::Ui, co: &CommanderData, color: egui::Color32, selected: bool) -> egui::Response {
    let frame = egui::Frame::new()
        .fill(if selected { theme::GOLD_LIGHT } else { theme::PARCHMENT_DEEP })
        .stroke(theme::stroke(if selected { 2.0 } else { 0.0 }, theme::WOOD))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(10));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            theme::portrait(ui, 52.0, color, &co.name);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::display(&co.name, 20.0));
                    ui.label(theme::body(format!("{:?}", co.personality), 13.0).color(theme::INK_SOFT));
                });
                ui.horizontal_wrapped(|ui| {
                    let bonuses = passive_bonuses(co);
                    if bonuses.is_empty() {
                        theme::chip(ui, "Balanced", theme::INK_SOFT);
                    }
                    for (text, positive) in bonuses {
                        theme::chip(ui, &text, if positive { theme::LEAF } else { theme::BERRY });
                    }
                });
                if selected {
                    ui.add_space(2.0);
                    ui.label(theme::bold(format!("★ {}", co.power.name), 15.0).color(theme::WOOD_DARK));
                    ui.label(theme::body(&co.power.description, 14.0).color(theme::INK_SOFT));
                } else {
                    ui.label(theme::body(format!("★ {}", co.power.name), 14.0).color(theme::INK_SOFT));
                }
            });
        });
    });
    ui.add_space(4.0);
    inner.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Passive CO modifiers as (label, is_positive)
pub fn passive_bonuses(co: &CommanderData) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let pct = |v: f32| ((v - 1.0) * 100.0).round() as i32;
    if co.attack_bonus != 1.0 {
        out.push((format!("{:+}% Attack", pct(co.attack_bonus)), co.attack_bonus > 1.0));
    }
    if co.defense_bonus != 1.0 {
        out.push((format!("{:+}% Defense", pct(co.defense_bonus)), co.defense_bonus > 1.0));
    }
    if co.movement_bonus != 0 {
        out.push((format!("{:+} Move", co.movement_bonus), co.movement_bonus > 0));
    }
    if co.income_bonus != 1.0 {
        out.push((format!("{:+}% Income", pct(co.income_bonus)), co.income_bonus > 1.0));
    }
    if co.vision_bonus != 0 {
        out.push((format!("+{} Vision", co.vision_bonus), true));
    }
    if co.cost_modifier != 1.0 {
        out.push((format!("{:+}% Unit cost", pct(co.cost_modifier)), co.cost_modifier < 1.0));
    }
    if co.terrain_cost_reduction > 0 {
        out.push(("Eases rough terrain".into(), true));
    }
    out
}

fn matchup_side(ui: &mut egui::Ui, game_data: &GameData, faction: Faction, commander: String) {
    let color = theme::faction_color(game_data, faction);
    theme::portrait(ui, 44.0, color, game_data.faction_name(faction));
    ui.vertical(|ui| {
        ui.label(theme::display(game_data.faction_name(faction), 20.0));
        ui.label(theme::body(commander, 15.0).color(theme::INK_SOFT));
    });
}

/// Draw a tiny overview of a map: terrain colors, owned properties and units
pub fn minimap(ui: &mut egui::Ui, game_data: &GameData, map: &MapData, max: egui::Vec2) {
    let cell = (max.x / map.width as f32).min(max.y / map.height as f32).floor().max(2.0);
    let size = egui::vec2(cell * map.width as f32, cell * map.height as f32);
    let (outer, _) = ui.allocate_exact_size(egui::vec2(max.x, size.y), egui::Sense::hover());
    let rect = egui::Rect::from_center_size(outer.center(), size);
    let painter = ui.painter();
    for y in 0..map.height {
        for x in 0..map.width {
            let terrain = map.terrain[y as usize][x as usize];
            let r = egui::Rect::from_min_size(rect.min + egui::vec2(x as f32 * cell, y as f32 * cell), egui::vec2(cell, cell));
            painter.rect_filled(r, 0.0, theme::to_egui(game_data.terrain_color(terrain)));
        }
    }
    for p in &map.properties {
        let r = egui::Rect::from_min_size(rect.min + egui::vec2(p.x as f32 * cell, p.y as f32 * cell), egui::vec2(cell, cell));
        painter.rect_stroke(r.shrink(1.0), 0.0, theme::stroke(2.0, theme::faction_color(game_data, p.owner)), egui::StrokeKind::Inside);
    }
    for u in &map.units {
        let c = rect.min + egui::vec2((u.x as f32 + 0.5) * cell, (u.y as f32 + 0.5) * cell);
        let color = theme::faction_color(game_data, u.faction);
        painter.circle(c, cell * 0.3, color, theme::stroke(1.0, theme::shade(color, 0.5)));
    }
    painter.rect_stroke(rect, 2, theme::stroke(1.5, theme::WOOD_DARK), egui::StrokeKind::Outside);
}
