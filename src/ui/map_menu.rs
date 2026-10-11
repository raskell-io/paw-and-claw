//! Advance Wars map menu. Esc (gamepad Start) opens it, as does confirming on a
//! tile with nothing to do. Like AW it leads with battle information (your
//! units, intel on both armies, the commanders), then the CO powers, options
//! and End Turn.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::hud::format_funds;
use super::input::MenuInput;
use super::setup::passive_bonuses;
use super::theme::{self, Tone};
use super::{BattleSetupState, PortraitTextures};
use crate::game::{
    AiState, BattleConfig, Commanders, EndTurnRequest, Faction, FactionFunds, FactionMember, FogOfWar,
    GameData, GameResult, GridCursor, GridPosition, InputMode, LoadGameEvent, MapMenu, PowerActivatedEvent,
    PowerLevel, SaveGameEvent, Tile, TurnState, Unit, Weather,
};
use crate::states::GameState;

#[derive(Clone, Copy, PartialEq, Default)]
enum Page {
    #[default]
    Main,
    Units,
    Intel,
    Commanders,
    Options,
}

#[derive(Clone, Copy, PartialEq)]
enum MainItem {
    Units,
    Intel,
    Commanders,
    Power(PowerLevel),
    Options,
    EndTurn,
}

#[derive(Clone, Copy, PartialEq)]
enum OptionItem {
    Fog,
    Controls,
    Save,
    Load,
    Title,
}

const OPTION_ITEMS: [OptionItem; 5] =
    [OptionItem::Fog, OptionItem::Controls, OptionItem::Save, OptionItem::Load, OptionItem::Title];

/// Where the player is in the menu
#[derive(Default)]
pub struct MenuNav {
    page: Page,
    selected: usize,
    /// Whether the menu was already open last frame (the key that opened it
    /// must not also pick an item)
    shown: bool,
    /// The selection moved by keys this frame, so lists scroll to it
    stepped: bool,
}

/// What the menu can change
#[derive(SystemParam)]
pub struct MenuActions<'w> {
    commanders: ResMut<'w, Commanders>,
    power_events: MessageWriter<'w, PowerActivatedEvent>,
    end_turn: MessageWriter<'w, EndTurnRequest>,
    next_state: ResMut<'w, NextState<GameState>>,
    fog: ResMut<'w, FogOfWar>,
    input_mode: ResMut<'w, InputMode>,
    save: MessageWriter<'w, SaveGameEvent>,
    load: MessageWriter<'w, LoadGameEvent>,
    cursor: ResMut<'w, GridCursor>,
}

/// What the menu shows
#[derive(SystemParam)]
pub struct MenuInfo<'w, 's> {
    game_data: Res<'w, GameData>,
    battle_config: Res<'w, BattleConfig>,
    turn_state: Res<'w, TurnState>,
    funds: Res<'w, FactionFunds>,
    weather: Res<'w, Weather>,
    setup: Res<'w, BattleSetupState>,
    game_result: Res<'w, GameResult>,
    ai_state: Res<'w, AiState>,
    portraits: Res<'w, PortraitTextures>,
    units: Query<'w, 's, (&'static Unit, &'static FactionMember, &'static GridPosition)>,
    tiles: Query<'w, 's, &'static Tile>,
}

impl MenuInfo<'_, '_> {
    fn player(&self) -> Faction {
        self.battle_config.player_faction
    }

    fn enemy(&self) -> Faction {
        self.battle_config.next_faction(self.player())
    }

    fn players_turn(&self) -> bool {
        !(self.ai_state.enabled && self.battle_config.is_ai_faction(self.turn_state.current_faction))
    }

    /// The player's units, ready ones first, as (unit, position)
    fn my_units(&self) -> Vec<(&Unit, (i32, i32))> {
        let mut list: Vec<_> = self.units.iter()
            .filter(|(_, f, _)| f.faction == self.player())
            .map(|(u, _, p)| (u, (p.x, p.y)))
            .collect();
        list.sort_by_key(|(u, (x, y))| (u.exhausted, *y, *x));
        list
    }
}

pub fn draw_map_menu(
    mut contexts: EguiContexts,
    mut menu: ResMut<MapMenu>,
    mut actions: MenuActions,
    info: MenuInfo,
    input: MenuInput,
    gamepads: Query<&Gamepad>,
    mut nav: Local<MenuNav>,
) {
    if info.setup.needs_setup || info.game_result.game_over {
        menu.open = false;
    } else if gamepads.iter().any(|pad| pad.just_pressed(GamepadButton::Start)) {
        menu.open = !menu.open;
    }
    if !menu.open {
        *nav = MenuNav::default();
        return;
    }
    let accept_input = std::mem::replace(&mut nav.shown, true);
    let game_data = &*info.game_data;
    let player = info.player();
    let co = game_data.commander(actions.commanders.get_active(player));

    // The entries on this page, for keyboard navigation
    let main_items: Vec<MainItem> = {
        let mut items = vec![MainItem::Units, MainItem::Intel, MainItem::Commanders];
        if info.players_turn() {
            for (level, has) in [(PowerLevel::Power, co.power.is_some()), (PowerLevel::Super, true)] {
                if has && actions.commanders.can_activate(player, level, game_data) {
                    items.push(MainItem::Power(level));
                }
            }
        }
        items.push(MainItem::Options);
        if info.players_turn() {
            items.push(MainItem::EndTurn);
        }
        items
    };
    let my_units = info.my_units();
    let count = match nav.page {
        Page::Main => main_items.len(),
        Page::Units => my_units.len(),
        Page::Options => OPTION_ITEMS.len(),
        Page::Intel | Page::Commanders => 0,
    };

    let mut confirmed = None;
    let mut back = false;
    nav.stepped = false;
    if accept_input {
        if nav.page == Page::Commanders {
            // Left/right flips between the two commanders
            nav.stepped = input.step_horizontal(&mut nav.selected, 2);
        } else if count > 0 {
            nav.stepped = input.step_vertical(&mut nav.selected, count);
        }
        if input.confirm() && count > 0 {
            confirmed = Some(nav.selected);
        }
        back = input.back();
    }
    nav.selected = nav.selected.min(count.max(if nav.page == Page::Commanders { 2 } else { 1 }) - 1);

    let Ok(ctx) = contexts.ctx_mut() else { return };
    theme::backdrop(ctx, "map_menu_backdrop", 110);
    egui::Area::new(egui::Id::new("map_menu"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            theme::card().show(ui, |ui| match nav.page {
                Page::Main => {
                    ui.set_width(300.0);
                    ui.vertical_centered(|ui| {
                        ui.label(theme::display(format!("Day {}", info.turn_state.turn_number), 28.0).color(theme::WOOD_DARK));
                        ui.label(theme::body(format!("{} · {} {}", info.setup.selected_map.name(),
                            info.weather.current.icon(), info.weather.current.name()), 14.0).color(theme::INK_SOFT));
                    });
                    ui.add_space(4.0);
                    for (i, item) in main_items.iter().enumerate() {
                        let ready_units = my_units.iter().filter(|(u, _)| !u.exhausted).count();
                        let (label, detail, tone) = match item {
                            MainItem::Units => ("Units".to_string(), Some(format!("{} ready of {}", ready_units, my_units.len())), Tone::Normal),
                            MainItem::Intel => ("Intel".to_string(), Some("Armies, funds, properties".to_string()), Tone::Normal),
                            MainItem::Commanders => ("Commanders".to_string(), Some("Abilities and powers".to_string()), Tone::Normal),
                            MainItem::Power(PowerLevel::Super) => (format!("★ {}", co.super_power.name), Some("Super CO Power".to_string()), Tone::Normal),
                            MainItem::Power(_) => (format!("★ {}", co.power.as_ref().map_or("", |p| p.name.as_str())), Some("CO Power".to_string()), Tone::Normal),
                            MainItem::Options => ("Options".to_string(), Some("Fog, controls, save, quit".to_string()), Tone::Normal),
                            MainItem::EndTurn => ("End turn".to_string(), None, Tone::Danger),
                        };
                        if matches!(item, MainItem::Options) {
                            ui.separator();
                        }
                        let response = theme::menu_item(ui, &label, detail.as_deref(), i == nav.selected, true, tone);
                        if theme::pointer_selects(ui, &response) {
                            nav.selected = i;
                        }
                        if response.clicked() {
                            confirmed = Some(i);
                        }
                    }
                    hints(ui, &[("↑↓", "Choose"), ("Enter", "Select"), ("Esc", "Close")]);
                }
                Page::Units => {
                    ui.set_width(380.0);
                    heading(ui, "Units", &format!("{} in the field", my_units.len()));
                    if my_units.is_empty() {
                        ui.label(theme::body("No units yet. Build some at a workshop.", 15.0).color(theme::INK_SOFT));
                    }
                    egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                        for (i, (unit, (x, y))) in my_units.iter().enumerate() {
                            let mut detail = format!("{} HP", unit.display_hp());
                            if unit.exhausted {
                                detail.push_str(" · done");
                            }
                            if unit.has_cargo() {
                                detail.push_str(" · carrying");
                            }
                            detail.push_str(&format!(" · {}", game_data.terrain_name(
                                info.tiles.iter().find(|t| t.position.x == *x && t.position.y == *y)
                                    .map_or(crate::game::Terrain::Grass, |t| t.terrain))));
                            let tone = if unit.exhausted { Tone::Muted } else { Tone::Normal };
                            let response = theme::menu_item(ui, game_data.unit_name(unit.unit_type), Some(&detail), i == nav.selected, true, tone);
                            if i == nav.selected && nav.stepped {
                                response.scroll_to_me(Some(egui::Align::Center));
                            }
                            if theme::pointer_selects(ui, &response) {
                                nav.selected = i;
                            }
                            if response.clicked() {
                                confirmed = Some(i);
                            }
                        }
                    });
                    hints(ui, &[("↑↓", "Choose"), ("Enter", "Go to unit"), ("Esc", "Back")]);
                }
                Page::Intel => {
                    ui.set_width(460.0);
                    heading(ui, "Intel", &format!("Day {} · {} {}", info.turn_state.turn_number,
                        info.weather.current.icon(), info.weather.current.name()));
                    intel(ui, &info, &actions.commanders);
                    hints(ui, &[("Esc", "Back")]);
                }
                Page::Commanders => {
                    ui.set_width(460.0);
                    let army = if nav.selected == 0 { player } else { info.enemy() };
                    commander_page(ui, &info, &actions.commanders, army);
                    hints(ui, &[("←→", "Other commander"), ("Esc", "Back")]);
                }
                Page::Options => {
                    ui.set_width(320.0);
                    heading(ui, "Options", "");
                    for (i, item) in OPTION_ITEMS.iter().enumerate() {
                        let (label, detail, tone) = match item {
                            OptionItem::Fog => (format!("Fog of war: {}", if actions.fog.enabled { "On" } else { "Off" }), None, Tone::Normal),
                            OptionItem::Controls => (format!("Controls: {}", actions.input_mode.name()), None, Tone::Normal),
                            OptionItem::Save => ("Save game".to_string(), Some("F5"), Tone::Normal),
                            OptionItem::Load => ("Load game".to_string(), Some("F9"), Tone::Normal),
                            OptionItem::Title => ("Retreat to title".to_string(), Some("Abandons this battle"), Tone::Danger),
                        };
                        if *item == OptionItem::Title {
                            ui.separator();
                        }
                        let response = theme::menu_item(ui, &label, detail, i == nav.selected, true, tone);
                        if theme::pointer_selects(ui, &response) {
                            nav.selected = i;
                        }
                        if response.clicked() {
                            confirmed = Some(i);
                        }
                    }
                    hints(ui, &[("↑↓", "Choose"), ("Enter", "Select"), ("Esc", "Back")]);
                }
            });
        });

    if back {
        if nav.page == Page::Main {
            menu.open = false;
        } else {
            // Back to the main page, on the entry that led here
            let from = match nav.page {
                Page::Units => MainItem::Units,
                Page::Intel => MainItem::Intel,
                Page::Commanders => MainItem::Commanders,
                _ => MainItem::Options,
            };
            nav.page = Page::Main;
            nav.selected = main_items.iter().position(|&i| i == from).unwrap_or(0);
        }
        return;
    }
    let Some(index) = confirmed else { return };
    let open_page = |nav: &mut MenuNav, page| {
        nav.page = page;
        nav.selected = 0;
    };
    match nav.page {
        Page::Main => match main_items[index] {
            MainItem::Units => open_page(&mut nav, Page::Units),
            MainItem::Intel => open_page(&mut nav, Page::Intel),
            MainItem::Commanders => open_page(&mut nav, Page::Commanders),
            MainItem::Options => open_page(&mut nav, Page::Options),
            MainItem::Power(level) => {
                if let Some(effects) = actions.commanders.activate_power(player, level, game_data) {
                    actions.power_events.write(PowerActivatedEvent { faction: player, level, effects });
                }
                menu.open = false;
            }
            MainItem::EndTurn => {
                actions.end_turn.write(EndTurnRequest);
                menu.open = false;
            }
        },
        Page::Units => {
            // Jump the cursor (and camera) to the unit
            let (_, (x, y)) = my_units[index];
            actions.cursor.x = x;
            actions.cursor.y = y;
            actions.cursor.follow = true;
            menu.open = false;
        }
        Page::Options => match OPTION_ITEMS[index] {
            OptionItem::Fog => actions.fog.enabled = !actions.fog.enabled,
            OptionItem::Controls => *actions.input_mode = actions.input_mode.cycle(),
            OptionItem::Save => {
                actions.save.write(SaveGameEvent { slot: 0 });
                menu.open = false;
            }
            OptionItem::Load => {
                actions.load.write(LoadGameEvent { slot: 0 });
                menu.open = false;
            }
            OptionItem::Title => {
                menu.open = false;
                actions.next_state.set(GameState::Menu);
            }
        },
        Page::Intel | Page::Commanders => {}
    }
}

fn heading(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.vertical_centered(|ui| {
        ui.label(theme::display(title, 28.0).color(theme::WOOD_DARK));
        if !detail.is_empty() {
            ui.label(theme::body(detail, 14.0).color(theme::INK_SOFT));
        }
    });
    ui.add_space(4.0);
}

fn hints(ui: &mut egui::Ui, keys: &[(&str, &str)]) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        for (key, action) in keys {
            theme::key_hint(ui, key, action, false);
        }
    });
}

/// Side by side numbers for both armies, like AW's Intel screen
fn intel(ui: &mut egui::Ui, info: &MenuInfo, commanders: &Commanders) {
    let game_data = &*info.game_data;
    let armies = [info.player(), info.enemy()];
    let stats = |army: Faction| {
        let units: Vec<&Unit> = info.units.iter().filter(|(_, f, _)| f.faction == army).map(|(u, _, _)| u).collect();
        let value: u32 = units.iter()
            .map(|u| game_data.unit_stats(u.unit_type).cost * u.display_hp() as u32 / 10)
            .sum();
        let owned: Vec<&Tile> = info.tiles.iter().filter(|t| t.owner == Some(army)).collect();
        let co = commanders.get_bonuses(army, game_data);
        let income: i32 = owned.iter()
            .map(|t| (game_data.terrain_income(t.terrain) as i32 + co.mods.income).max(0))
            .sum();
        (units.len(), value, owned.len(), income.max(0) as u32)
    };
    let columns: Vec<_> = armies.iter().map(|&a| (a, stats(a))).collect();
    theme::well().show(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("intel").num_columns(3).spacing([28.0, 6.0]).show(ui, |ui| {
            ui.label("");
            for (army, _) in &columns {
                ui.label(theme::bold(game_data.faction_name(*army), 16.0)
                    .color(theme::shade(theme::faction_color(game_data, *army), 0.75)));
            }
            ui.end_row();
            let row = |ui: &mut egui::Ui, name: &str, values: Vec<String>| {
                ui.label(theme::body(name, 15.0).color(theme::INK_SOFT));
                for v in values {
                    ui.label(theme::bold(v, 16.0));
                }
                ui.end_row();
            };
            row(ui, "Commander", armies.iter().map(|&a| game_data.commander(commanders.get_active(a)).name.clone()).collect());
            row(ui, "Funds", armies.iter().map(|&a| format_funds(info.funds.get(a))).collect());
            row(ui, "Income / day", columns.iter().map(|(_, s)| format_funds(s.3)).collect());
            row(ui, "Properties", columns.iter().map(|(_, s)| s.2.to_string()).collect());
            row(ui, "Units", columns.iter().map(|(_, s)| s.0.to_string()).collect());
            row(ui, "Army value", columns.iter().map(|(_, s)| format_funds(s.1)).collect());
            row(ui, "Power meter", armies.iter().map(|&a| {
                let full = commanders.power_cost(a, PowerLevel::Super, game_data).unwrap_or(1).max(1);
                format!("{}%", (commanders.get_charge(a) * 100 / full).min(100))
            }).collect());
        });
    });
}

/// One commander: portrait, abilities and both powers
fn commander_page(ui: &mut egui::Ui, info: &MenuInfo, commanders: &Commanders, army: Faction) {
    let game_data = &*info.game_data;
    let co_id = commanders.get_active(army);
    let co = game_data.commander(co_id);
    let color = theme::faction_color(game_data, army);
    ui.horizontal(|ui| {
        theme::portrait(ui, 84.0, color, &co.name, info.portraits.get(co_id));
        ui.vertical(|ui| {
            ui.label(theme::display(&co.name, 28.0).color(theme::WOOD_DARK));
            ui.label(theme::bold(game_data.faction_name(army), 15.0).color(theme::shade(color, 0.75)));
            ui.label(theme::body(format!("Plays like {}", co.based_on), 13.0).color(theme::INK_SOFT));
        });
    });
    ui.add_space(4.0);
    ui.label(theme::body(&co.description, 15.0).color(theme::INK));
    ui.add_space(2.0);
    ui.horizontal_wrapped(|ui| {
        let bonuses = passive_bonuses(co, game_data);
        if bonuses.is_empty() {
            theme::chip(ui, "Balanced", theme::INK_SOFT);
        }
        for (text, positive) in bonuses {
            theme::chip(ui, &text, if positive { theme::LEAF } else { theme::BERRY });
        }
    });
    let active = commanders.active_power(army);
    let powers = co.power.iter().map(|p| (PowerLevel::Power, p, "CO Power"))
        .chain(std::iter::once((PowerLevel::Super, &co.super_power, "Super CO Power")));
    for (level, power, kind) in powers {
        ui.add_space(6.0);
        let status = if active == level { " · active" } else { "" };
        ui.label(theme::bold(format!("★ {} · {kind}, {} stars{status}", power.name, power.stars), 15.0).color(theme::WOOD_DARK));
        ui.label(theme::body(&power.description, 14.0).color(theme::INK_SOFT));
    }
}
