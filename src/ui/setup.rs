//! Skirmish setup: the armies (2 to 4, each played by a person or the
//! computer, optionally in two teams), each army's commander, and the
//! battlefield (built-in maps and maps made in the editor)

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::icons::Icon;
use super::input::MenuInput;
use super::theme::{self, Tone};
use super::{ArmySetup, BattleSetupState, PortraitTextures};
use crate::game::{
    get_builtin_map, map_problem, spawn_map_from_data, spawn_units_from_data, Army, BattleConfig, BattleMap, CommanderData,
    CommanderId, Commanders, CustomMaps, Faction, FactionFunds, GameData, GameMap, MapData, MapId, SpriteAssets,
    TilesetTheme, TurnPhase, TurnStartEvent, TurnState,
};
use crate::states::GameState;

/// Factions offered in setup (Nether is the antagonist, not playable)
pub const PLAYABLE: [Faction; 4] = [Faction::Western, Faction::Northern, Faction::Southern, Faction::Eastern];

/// Which column keyboard/gamepad input is driving
#[derive(Default, Clone, Copy, PartialEq)]
pub enum Column {
    #[default]
    Armies,
    Commander,
    Map,
}

impl Column {
    const ALL: [Column; 3] = [Column::Armies, Column::Commander, Column::Map];

    fn index(self) -> usize {
        Self::ALL.iter().position(|&c| c == self).unwrap()
    }
}

/// Rows in the Armies column: one per army, then the battle options
#[derive(Clone, Copy, PartialEq)]
enum ArmyRow {
    Army(usize),
    Count,
    Teams,
    DayLimit,
}

/// Day limits offered in setup
const DAY_LIMITS: [Option<u32>; 4] = [None, Some(20), Some(30), Some(40)];

/// Rows in the Commander column: a faction (random commander) or one commander
#[derive(Clone, Copy, PartialEq)]
enum CommanderRow {
    Random(Faction),
    Pick(CommanderId),
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
    images: ResMut<'w, Assets<Image>>,
    tileset_theme: Res<'w, TilesetTheme>,
    turn_start_events: MessageWriter<'w, TurnStartEvent>,
}

impl BattleSpawner<'_, '_> {
    /// Start a battle: each army gets its commander (random when none is
    /// picked) and its slot on the map; armies on the right face left
    fn start(&mut self, game_data: &GameData, armies: &[ArmySetup], teams: bool, day_limit: Option<u32>, map: &MapData, battle_map: BattleMap) {
        let factions: Vec<Faction> = armies.iter().map(|a| a.faction).collect();
        let map_data = map.with_armies(&factions);
        let mut config = Vec::new();
        for (i, army) in armies.iter().enumerate() {
            let co = army.co.unwrap_or_else(|| {
                let cos = CommanderId::for_faction(army.faction, game_data);
                cos.get(rand::random::<usize>() % cos.len().max(1)).copied().unwrap_or(CommanderId::Kira)
            });
            self.commanders.set_commander(army.faction, co);
            let facing_left = map_data.hq_of(army.faction).map_or(i % 2 == 1, |(x, _)| x as f32 >= map_data.width as f32 / 2.0);
            config.push(Army { faction: army.faction, ai: !army.human, team: if teams { (i % 2) as u8 } else { i as u8 },
                defeated: false, facing_left });
            info!("Army {}: {:?} led by {:?} ({})", i + 1, army.faction, co, if army.human { "person" } else { "computer" });
        }
        info!("Battle started on {}", battle_map.name);

        let first = armies[0].faction;
        let mut battle_config = BattleConfig::from_armies(config);
        battle_config.day_limit = day_limit;
        self.commands.insert_resource(battle_config);
        self.commands.insert_resource(battle_map);
        self.commands.insert_resource(TurnState { current_faction: first, turn_number: 1, phase: TurnPhase::Select });
        // Everyone starts broke; the first army collects Day 1 income from
        // the turn start event below
        self.commands.insert_resource(FactionFunds::default());

        spawn_map_from_data(&mut self.commands, &mut self.game_map, &mut self.meshes, &mut self.materials,
            &self.sprite_assets, &mut self.images, game_data, &map_data, *self.tileset_theme);
        spawn_units_from_data(&mut self.commands, &self.game_map, &mut self.meshes, &mut self.materials,
            &self.sprite_assets, &self.images, game_data, &map_data);

        self.turn_start_events.write(TurnStartEvent { faction: first });
    }
}

/// Give an army a faction; an army that had it takes this army's old one
fn assign_faction(armies: &mut [ArmySetup], index: usize, faction: Faction) {
    let old = armies[index].faction;
    if old == faction {
        return;
    }
    if let Some(other) = armies.iter().position(|a| a.faction == faction) {
        armies[other].faction = old;
        armies[other].co = None;
    }
    armies[index].faction = faction;
    armies[index].co = None;
}

/// "Player 2" for the second person, "Computer" for the AI
fn controller_name(armies: &[ArmySetup], index: usize) -> String {
    if !armies[index].human {
        return "Computer".to_string();
    }
    let number = armies[..=index].iter().filter(|a| a.human).count();
    format!("Player {number}")
}

pub fn draw_battle_setup(
    mut contexts: EguiContexts,
    mut setup: ResMut<BattleSetupState>,
    mut spawner: BattleSpawner,
    mut next_state: ResMut<NextState<GameState>>,
    game_data: Res<GameData>,
    input: MenuInput,
    time: Res<Time>,
    portraits: Res<PortraitTextures>,
    custom_maps: Res<CustomMaps>,
    mut column: Local<Column>,
    mut maps: Local<(usize, Vec<(MapId, MapData, Option<String>)>)>,
    mut shown: Local<bool>,
) {
    if !setup.needs_setup {
        *shown = false;
        return;
    }
    // The keypress that opened this screen must not also confirm it
    let accept_input = std::mem::replace(&mut *shown, true);
    // Built-in maps, then the player's own (rebuilt when the editor saved more)
    if maps.1.is_empty() || maps.0 != custom_maps.maps.len() {
        let builtin = MapId::all_builtin().into_iter().map(|id| (id, get_builtin_map(id), None));
        let custom = custom_maps.maps.iter().enumerate().map(|(i, (_, map))| (MapId::Custom(i), map.clone(), map_problem(map)));
        *maps = (custom_maps.maps.len(), builtin.chain(custom).collect());
    }
    let maps = &maps.1;
    let map_index = maps.iter().position(|(id, ..)| *id == setup.selected_map).unwrap_or(0);
    setup.selected_map = maps[map_index].0;
    let max_armies = maps[map_index].1.army_count().clamp(2, 4);
    setup.army_count = setup.army_count.clamp(2, max_armies);
    if setup.army_count < 4 {
        setup.teams = false;
    }
    setup.selected_army = setup.selected_army.min(setup.army_count - 1);
    let count = setup.army_count;

    let army_rows: Vec<ArmyRow> = (0..count).map(ArmyRow::Army)
        .chain((max_armies > 2).then_some(ArmyRow::Count))
        .chain((count == 4).then_some(ArmyRow::Teams))
        .chain(std::iter::once(ArmyRow::DayLimit))
        .collect();
    let commander_rows: Vec<CommanderRow> = PLAYABLE.iter()
        .flat_map(|&f| std::iter::once(CommanderRow::Random(f))
            .chain(CommanderId::for_faction(f, &game_data).into_iter().map(CommanderRow::Pick)))
        .collect();
    let selected = setup.selected_army;
    let current_commander_row = match setup.armies[selected].co {
        Some(co) => CommanderRow::Pick(co),
        None => CommanderRow::Random(setup.armies[selected].faction),
    };
    let mut army_row = *setup.armies_cursor.get_or_insert(0);
    army_row = army_row.min(army_rows.len() - 1);

    // --- Keyboard / gamepad ---
    let mut start = false;
    let mut army_action: Option<ArmyRow> = None;
    let mut pick: Option<CommanderRow> = None;
    let mut pick_map: Option<MapId> = None;
    if accept_input {
        let mut col = column.index();
        if input.step_horizontal(&mut col, Column::ALL.len()) || input.next_section() {
            if input.next_section() {
                col = (col + 1) % Column::ALL.len();
            }
        }
        *column = Column::ALL[col];
        match *column {
            Column::Armies => {
                input.step_vertical(&mut army_row, army_rows.len());
                if let ArmyRow::Army(i) = army_rows[army_row] {
                    setup.selected_army = i;
                }
                // Enter here changes the row (who plays, how many armies, teams)
                if input.confirm() {
                    army_action = Some(army_rows[army_row]);
                }
            }
            Column::Commander => {
                let mut i = commander_rows.iter().position(|&r| r == current_commander_row).unwrap_or(0);
                if input.step_vertical(&mut i, commander_rows.len()) {
                    pick = Some(commander_rows[i]);
                }
                start = input.confirm();
            }
            Column::Map => {
                let mut i = map_index;
                if input.step_vertical(&mut i, maps.len()) {
                    pick_map = Some(maps[i].0);
                }
                start = input.confirm();
            }
        }
        if input.back() {
            next_state.set(GameState::Menu);
            return;
        }
    }

    let mut back_to_title = false;
    let sprites_ready = spawner.sprite_assets.loading_complete;
    let map_ok = maps[map_index].2.is_none();
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let screen = ui.max_rect();
        theme::forest_backdrop(ui.painter(), screen, time.elapsed_secs());

        // Header, three columns and the footer always fit the window: the
        // columns take whatever height is left and scroll inside
        let content = screen.shrink2(egui::vec2((screen.width() - 1180.0).max(48.0) / 2.0, 20.0));
        let compact = content.height() < 640.0;
        let header_h = if compact { 50.0 } else { 92.0 };
        let footer_h = 104.0;
        let header = egui::Rect::from_min_size(content.min, egui::vec2(content.width(), header_h));
        let footer = egui::Rect::from_min_max(egui::pos2(content.left(), content.bottom() - footer_h), content.max);
        let columns = egui::Rect::from_min_max(egui::pos2(content.left(), header.bottom()), egui::pos2(content.right(), footer.top() - 12.0));

        ui.scope_builder(egui::UiBuilder::new().max_rect(header), |ui| {
            ui.vertical_centered(|ui| {
                ui.label(theme::display("Skirmish", if compact { 34.0 } else { 44.0 }).color(theme::PARCHMENT));
                // Short screens (phones) keep the title only
                if !compact {
                    ui.label(theme::body("Choose the armies, who leads them and the field of battle", 18.0).color(theme::GOLD_LIGHT));
                }
            });
        });

        let gap = 16.0;
        let col_w = (columns.width() - gap * 2.0) / 3.0;
        let col_h = columns.height();
        ui.scope_builder(egui::UiBuilder::new().max_rect(columns), |ui| {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                column_card(ui, col_w, col_h, "Armies", *column == Column::Armies, |ui| {
                    egui::ScrollArea::vertical().id_salt("setup_armies").auto_shrink([false, false]).show(ui, |ui| {
                        for (row_index, row) in army_rows.iter().enumerate() {
                            let focused = *column == Column::Armies && row_index == army_row;
                            let response = match *row {
                                ArmyRow::Army(i) => {
                                    let team = setup.teams.then_some((i % 2) as u8);
                                    army_card(ui, &game_data, &portraits, &setup.armies, i, team, i == setup.selected_army, focused)
                                }
                                ArmyRow::Count => theme::menu_item_icon(ui, Some(Icon::Banner), &format!("Armies: {count}"),
                                    Some(&format!("This map holds up to {max_armies}")), focused, true, Tone::Normal),
                                ArmyRow::DayLimit => theme::menu_item_icon(ui, Some(Icon::Hourglass),
                                    &match setup.day_limit { Some(d) => format!("Day limit: {d}"), None => "Day limit: off".to_string() },
                                    Some(if setup.day_limit.is_some() { "Most properties wins when time runs out" } else { "Play until an army falls" }),
                                    focused, true, Tone::Normal),
                                ArmyRow::Teams => theme::menu_item_icon(ui, Some(Icon::Swords),
                                    if setup.teams { "Teams: 2 vs 2" } else { "Teams: free for all" },
                                    Some(if setup.teams { "Armies 1 & 3 against 2 & 4" } else { "Every army for itself" }),
                                    focused, true, Tone::Normal),
                            };
                            if focused {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                *column = Column::Armies;
                                army_row = row_index;
                                // A tap on an army selects it; tapping it again changes who plays it
                                match *row {
                                    ArmyRow::Army(i) if i != setup.selected_army => setup.selected_army = i,
                                    other => army_action = Some(other),
                                }
                            }
                        }
                    });
                });
                column_card(ui, col_w, col_h, &format!("Army {} commander", selected + 1), *column == Column::Commander, |ui| {
                    egui::ScrollArea::vertical().id_salt("setup_commanders").auto_shrink([false, false]).show(ui, |ui| {
                        for row in &commander_rows {
                            let is_current = *row == current_commander_row;
                            let response = match *row {
                                CommanderRow::Random(faction) => {
                                    ui.add_space(4.0);
                                    let taken = setup.armies[..count].iter().position(|a| a.faction == faction).filter(|&i| i != selected);
                                    let detail = match taken {
                                        Some(i) => format!("Random commander · swaps with army {}", i + 1),
                                        None => "Random commander".to_string(),
                                    };
                                    faction_heading(ui, &game_data, faction, &detail, is_current)
                                }
                                CommanderRow::Pick(co_id) => {
                                    let co = game_data.commander(co_id);
                                    commander_row(ui, &game_data, co, theme::faction_color(&game_data, co.faction),
                                        is_current, portraits.get(co_id))
                                }
                            };
                            if is_current && *column == Column::Commander {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                pick = Some(*row);
                                *column = Column::Commander;
                            }
                        }
                    });
                });
                column_card(ui, col_w, col_h, "Battlefield", *column == Column::Map, |ui| {
                    let (_, map, problem) = &maps[map_index];
                    let factions: Vec<Faction> = setup.armies[..count].iter().map(|a| a.faction).collect();
                    minimap(ui, &game_data, &map.with_armies(&factions), egui::vec2(ui.available_width(), if compact { 80.0 } else { 120.0 }));
                    ui.add_space(2.0);
                    let note = problem.clone().unwrap_or_else(|| map.description.clone());
                    ui.label(theme::body(note, 14.0).color(if problem.is_some() { theme::BERRY } else { theme::INK_SOFT }).italics());
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().id_salt("setup_maps").auto_shrink([false, false]).show(ui, |ui| {
                        for (id, map, problem) in maps.iter() {
                            let armies = map.army_count();
                            let mut detail = format!("{} × {}", map.width, map.height);
                            if armies > 2 {
                                detail.push_str(&format!(" · up to {armies} armies"));
                            }
                            if matches!(id, MapId::Custom(_)) {
                                detail.push_str(if problem.is_some() { " · can't be played yet" } else { " · your map" });
                            }
                            let response = theme::menu_item(ui, &map.name, Some(&detail), setup.selected_map == *id, true,
                                if problem.is_some() { Tone::Muted } else { Tone::Normal });
                            if setup.selected_map == *id && *column == Column::Map {
                                response.scroll_to_me(None);
                            }
                            if response.clicked() {
                                pick_map = Some(*id);
                                *column = Column::Map;
                            }
                        }
                    });
                });
            });
        });

        ui.scope_builder(egui::UiBuilder::new().max_rect(footer), |ui| {
            // Matchup banner and start button
            theme::card().show(ui, |ui| {
                ui.set_width(footer.width() - 28.0);
                ui.horizontal(|ui| {
                    for i in 0..count {
                        if i > 0 {
                            let word = if setup.teams && i == 2 { "vs" } else if setup.teams { "&" } else { "vs" };
                            ui.label(theme::display(word, if count > 2 { 18.0 } else { 26.0 }).color(theme::INK_SOFT));
                        }
                        // Teams line up as 1 & 3 vs 2 & 4
                        let shown = if setup.teams { [0, 2, 1, 3][i] } else { i };
                        matchup_side(ui, &game_data, &portraits, &setup.armies, shown, count > 2);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if !sprites_ready { "Loading art…" } else if map_ok { "Begin Battle" } else { "Map not ready" };
                        if theme::primary_button(ui, label, sprites_ready && map_ok, true).clicked() {
                            start = true;
                        }
                    });
                });
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                theme::key_hint(ui, "←→", "Column", true);
                theme::key_hint(ui, "↑↓", "Choose", true);
                if *column == Column::Armies {
                    theme::key_hint(ui, "Enter", "Change", true);
                } else {
                    theme::key_hint(ui, "Enter", "Begin battle", true);
                }
                if theme::key_hint_button(ui, "Esc", "Back", true) {
                    back_to_title = true;
                }
            });
        });
    });

    setup.armies_cursor = Some(army_row);
    match army_action {
        Some(ArmyRow::Army(i)) => {
            setup.selected_army = i;
            // Who plays it: a person or the computer (the first army can be either too)
            setup.armies[i].human = !setup.armies[i].human;
        }
        Some(ArmyRow::Count) => {
            setup.army_count = if setup.army_count >= max_armies { 2 } else { setup.army_count + 1 };
        }
        Some(ArmyRow::Teams) => setup.teams = !setup.teams,
        Some(ArmyRow::DayLimit) => {
            let i = DAY_LIMITS.iter().position(|&d| d == setup.day_limit).unwrap_or(0);
            setup.day_limit = DAY_LIMITS[(i + 1) % DAY_LIMITS.len()];
        }
        None => {}
    }
    if let Some(row) = pick {
        let i = setup.selected_army;
        match row {
            CommanderRow::Random(faction) => {
                assign_faction(&mut setup.armies, i, faction);
                setup.armies[i].co = None;
            }
            CommanderRow::Pick(co) => {
                assign_faction(&mut setup.armies, i, game_data.commander(co).faction);
                setup.armies[i].co = Some(co);
            }
        }
    }
    if let Some(id) = pick_map {
        setup.selected_map = id;
    }

    if back_to_title {
        next_state.set(GameState::Menu);
        return;
    }

    // Wait for sprites so every unit and tile spawns with its art
    if start && sprites_ready && map_ok {
        let (id, map, _) = &maps[map_index];
        let battle_map = match id {
            MapId::Custom(_) => BattleMap { id: None, name: map.name.clone() },
            builtin => BattleMap { id: Some(*builtin), name: builtin.name().to_string() },
        };
        let armies = setup.armies[..count].to_vec();
        spawner.start(&game_data, &armies, setup.teams, setup.day_limit, map, battle_map);
        setup.needs_setup = false;
    }
}

/// One army in the Armies column: its banner, commander and who plays it
fn army_card(ui: &mut egui::Ui, game_data: &GameData, portraits: &PortraitTextures, armies: &[ArmySetup], index: usize,
             team: Option<u8>, selected: bool, focused: bool) -> egui::Response {
    let army = armies[index];
    let color = theme::faction_color(game_data, army.faction);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 70.0), egui::Sense::click());
    let painter = ui.painter().with_clip_rect(rect.shrink(1.0));
    let fill = if focused { theme::GOLD_LIGHT } else if selected || response.hovered() { theme::PARCHMENT_DEEP } else { egui::Color32::TRANSPARENT };
    painter.rect(rect, 8, fill, theme::stroke(if selected { 2.0 } else { 0.0 }, theme::WOOD), egui::StrokeKind::Inside);
    // Portrait (or banner when the commander is random)
    let face = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 8.0), egui::vec2(54.0, 54.0));
    painter.rect_filled(face, 4, theme::shade(color, 0.5));
    match army.co.and_then(|co| portraits.get(co)) {
        Some(texture) => {
            egui::Image::new((texture, face.size())).corner_radius(4).paint_at(ui, face.shrink(2.0));
        }
        None => {
            painter.text(face.center(), egui::Align2::CENTER_CENTER, "?", egui::FontId::new(30.0, theme::title_family()), theme::PARCHMENT);
        }
    }
    let painter = ui.painter().with_clip_rect(rect.shrink(1.0));
    painter.rect_stroke(face, 4, theme::stroke(2.0, color), egui::StrokeKind::Inside);
    let x = face.right() + 10.0;
    painter.text(egui::pos2(x, rect.top() + 17.0), egui::Align2::LEFT_CENTER,
        format!("{} · {}", index + 1, game_data.faction_name(army.faction)), egui::FontId::new(17.0, theme::bold_family()), theme::shade(color, 0.7));
    let co = army.co.map_or("Random commander".to_string(), |co| game_data.commander_name(co).to_string());
    painter.text(egui::pos2(x, rect.top() + 37.0), egui::Align2::LEFT_CENTER, co, egui::FontId::proportional(14.0), theme::INK);
    // Who plays it, and the team
    let who = controller_name(armies, index);
    let chip_color = if army.human { theme::LEAF } else { theme::INK_SOFT };
    let mut label = who;
    if let Some(team) = team {
        label.push_str(if team == 0 { " · Team A" } else { " · Team B" });
    }
    painter.text(egui::pos2(x, rect.top() + 56.0), egui::Align2::LEFT_CENTER, label,
        egui::FontId::new(13.0, theme::bold_family()), chip_color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A faction's heading in the Commander column (picks it with a random commander)
fn faction_heading(ui: &mut egui::Ui, game_data: &GameData, faction: Faction, detail: &str, selected: bool) -> egui::Response {
    let color = theme::faction_color(game_data, faction);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), egui::Sense::click());
    let painter = ui.painter();
    let fill = if selected { theme::GOLD_LIGHT } else if response.hovered() { theme::PARCHMENT_DEEP } else { egui::Color32::TRANSPARENT };
    painter.rect(rect, 6, fill, theme::stroke(if selected { 2.0 } else { 0.0 }, theme::WOOD), egui::StrokeKind::Inside);
    let banner = egui::Rect::from_min_size(rect.min + egui::vec2(10.0, 7.0), egui::vec2(16.0, rect.height() - 14.0));
    painter.add(egui::Shape::convex_polygon(
        vec![banner.left_top(), banner.right_top(), banner.right_bottom(),
             egui::pos2(banner.center().x, banner.bottom() - 6.0), banner.left_bottom()],
        color, theme::stroke(1.0, theme::shade(color, 0.6)),
    ));
    painter.text(egui::pos2(banner.right() + 10.0, rect.top() + 15.0), egui::Align2::LEFT_CENTER, game_data.faction_name(faction),
        egui::FontId::new(18.0, theme::display_family()), theme::shade(color, 0.7));
    painter.text(egui::pos2(banner.right() + 10.0, rect.top() + 33.0), egui::Align2::LEFT_CENTER, detail,
        egui::FontId::proportional(13.0), theme::INK_SOFT);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A titled card for one setup column; gold border when it has input focus
fn column_card(ui: &mut egui::Ui, width: f32, height: f32, title: &str, focused: bool, add: impl FnOnce(&mut egui::Ui)) {
    let icon = match title {
        "Armies" => Some(Icon::Banner),
        t if t.ends_with("commander") => Some(Icon::Crown),
        "Battlefield" => Some(Icon::Flag),
        _ => None,
    };
    let frame = theme::card().stroke(theme::stroke(if focused { 3.0 } else { 2.0 }, if focused { theme::GOLD } else { theme::WOOD }));
    frame.show(ui, |ui| {
        // Fixed size whatever the content: long text wraps and lists scroll
        ui.set_width(width - 32.0);
        ui.set_height(height - 32.0);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
        ui.vertical(|ui| {
            theme::heading_icon(ui, icon, title);
            add(ui);
        });
    });
}

fn commander_row(ui: &mut egui::Ui, game_data: &GameData, co: &CommanderData, color: egui::Color32, selected: bool, image: Option<egui::TextureId>) -> egui::Response {
    let frame = egui::Frame::new()
        .fill(if selected { theme::GOLD_LIGHT } else { theme::PARCHMENT_DEEP })
        .stroke(theme::stroke(if selected { 2.0 } else { 0.0 }, theme::WOOD))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(10));
    let inner = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_top(|ui| {
            theme::portrait(ui, 52.0, color, &co.name, image);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::display(&co.name, 20.0));
                    ui.label(theme::body(format!("{:?}", co.personality), 13.0).color(theme::INK_SOFT));
                });
                ui.horizontal_wrapped(|ui| {
                    let bonuses = passive_bonuses(co, game_data);
                    if bonuses.is_empty() {
                        theme::chip(ui, "Balanced", theme::INK_SOFT);
                    }
                    for (text, positive) in bonuses {
                        theme::chip(ui, &text, if positive { theme::LEAF } else { theme::BERRY });
                    }
                });
                let powers = co.power.iter().map(|p| (p, "★")).chain(std::iter::once((&co.super_power, "★★")));
                for (power, mark) in powers {
                    if selected {
                        ui.add_space(2.0);
                        ui.label(theme::bold(format!("{mark} {} · {} stars", power.name, power.stars), 15.0).color(theme::WOOD_DARK));
                        ui.label(theme::body(&power.description, 14.0).color(theme::INK_SOFT));
                    } else {
                        ui.label(theme::body(format!("{mark} {} ({}★)", power.name, power.stars), 14.0).color(theme::INK_SOFT));
                    }
                }
            });
        });
    });
    ui.add_space(4.0);
    inner.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A CO's always-on abilities as (label, is_positive) chips
pub fn passive_bonuses(co: &CommanderData, game_data: &GameData) -> Vec<(String, bool)> {
    use crate::game::UnitFilter;
    let who = |filter: &UnitFilter| -> String {
        match filter {
            UnitFilter::All => String::new(),
            UnitFilter::Direct | UnitFilter::DirectNonFoot => "direct ".into(),
            UnitFilter::Indirect => "indirect ".into(),
            UnitFilter::Foot => "foot ".into(),
            UnitFilter::NonFoot => "non-foot ".into(),
            UnitFilter::Vehicles => "vehicle ".into(),
            UnitFilter::Ground => "ground ".into(),
            UnitFilter::Air => "air ".into(),
            UnitFilter::Naval => "boat ".into(),
            UnitFilter::Transports => "transport ".into(),
            UnitFilter::Units(list) => list.iter().map(|&u| game_data.unit_name(u)).collect::<Vec<_>>().join("/") + " ",
        }
    };
    let m = &co.abilities;
    let mut out = Vec::new();
    for (filter, v) in &m.attack {
        out.push((format!("{v:+}% {}attack", who(filter)), *v > 0));
    }
    for (filter, v) in &m.defense {
        out.push((format!("{v:+}% {}defense", who(filter)), *v > 0));
    }
    for (filter, v) in &m.movement {
        out.push((format!("{v:+} {}move", who(filter)), *v > 0));
    }
    for (filter, v) in &m.range {
        out.push((format!("{v:+} {}range", who(filter)), *v > 0));
    }
    for (filter, v) in &m.vision {
        out.push((format!("{v:+} {}vision", who(filter)), *v > 0));
    }
    for (group, v) in &m.attack_on {
        out.push((format!("{v:+}% attack on {group:?}"), *v > 0));
    }
    for (filter, v) in &m.daily_stamina {
        out.push((format!("{v:+} {}stamina use", who(filter)), *v < 0));
    }
    if m.attack_per_terrain_star != 0 {
        out.push((format!("{:+}% attack per terrain ★", m.attack_per_terrain_star), true));
    }
    if m.capture != 0 {
        out.push((format!("{:+}% capture", m.capture), m.capture > 0));
    }
    if m.cost != 0 {
        out.push((format!("{:+}% unit cost", m.cost), m.cost < 0));
    }
    if m.luck != 0 {
        out.push((format!("Luck {:+}", m.luck), m.luck > 0));
    }
    if m.bad_luck != 0 {
        out.push((format!("Bad luck {}", m.bad_luck), false));
    }
    if m.income != 0 {
        out.push((format!("{:+} funds per property", m.income), m.income > 0));
    }
    if m.defense_vs_indirect != 0 {
        out.push((format!("{:+}% defense vs indirect", m.defense_vs_indirect), true));
    }
    if m.repair != 0 {
        out.push((format!("Repairs {:+} HP", m.repair), true));
    }
    for weather in &m.weather_proof {
        out.push((format!("Ignores {}", weather.name()), true));
    }
    if m.flat_terrain_cost {
        out.push(("All terrain costs 1".into(), true));
    }
    if m.hidden_hp {
        out.push(("HP hidden from enemy".into(), true));
    }
    out
}

fn matchup_side(ui: &mut egui::Ui, game_data: &GameData, portraits: &PortraitTextures, armies: &[ArmySetup], index: usize, small: bool) {
    let army = armies[index];
    let color = theme::faction_color(game_data, army.faction);
    let image = army.co.and_then(|co| portraits.get(co));
    theme::portrait(ui, if small { 36.0 } else { 44.0 }, color, game_data.faction_name(army.faction), image);
    ui.vertical(|ui| {
        let name = if small { army.co.map_or("Random", |co| game_data.commander_name(co)).to_string() } else { game_data.faction_name(army.faction).to_string() };
        ui.label(theme::display(name, if small { 16.0 } else { 20.0 }));
        let detail = if small {
            controller_name(armies, index)
        } else {
            format!("{} · {}", army.co.map_or("Random commander", |co| game_data.commander_name(co)), controller_name(armies, index))
        };
        ui.label(theme::body(detail, if small { 12.0 } else { 15.0 }).color(theme::INK_SOFT));
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
