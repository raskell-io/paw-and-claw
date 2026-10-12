//! The save and load screen: the autosave and three slots, each showing its
//! map, day, commanders and age, plus backup export/import on the web.
//! Opened from the map menu's Options (Save, Load), Continue on the title
//! screen, or F5 (save) and F9 (load).

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::icons::{self, Icon};
use super::input::MenuInput;
use super::theme::{self, Tone};
use super::{BattleSetupState, PortraitTextures};
use crate::game::{
    export_backup, request_import, time_ago, GameData, GameLoadedEvent, ImportPending, LoadGameEvent, MapMenu,
    ModalOpen, SaveGameEvent, SaveIndex, SaveNotice, SaveSlot, SaveSummary, BACKUPS, MANUAL_SLOTS,
};
use crate::states::GameState;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SaveMode {
    Save,
    Load,
}

/// The save/load screen's state
#[derive(Resource, Default)]
pub struct SaveMenu {
    mode: Option<SaveMode>,
    selected: usize,
    /// The selected row asked "are you sure?" (overwrite, or load over a battle)
    armed: bool,
    /// Open since last frame (the input that opened it must not also pick a row)
    shown: bool,
    /// Esc goes back to the map menu
    from_map_menu: bool,
    /// Closed this frame: other menus ignore input until the next
    just_closed: bool,
    /// Row to start on
    prefer: Option<SaveSlot>,
}

impl SaveMenu {
    pub fn open(&mut self, mode: SaveMode, from_map_menu: bool, prefer: Option<SaveSlot>) {
        *self = SaveMenu { mode: Some(mode), from_map_menu, prefer, ..default() };
    }

    pub fn is_open(&self) -> bool {
        self.mode.is_some()
    }

    /// Whether other menus should leave input alone this frame
    pub fn blocks_input(&self) -> bool {
        self.mode.is_some() || self.just_closed
    }

    fn close(&mut self) {
        *self = SaveMenu { just_closed: true, ..default() };
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Row {
    Slot(SaveSlot),
    Export,
    Import,
}

fn rows(mode: SaveMode) -> Vec<Row> {
    let mut rows: Vec<Row> = match mode {
        // The autosave is the game's own; players save into their slots
        SaveMode::Save => (1..=MANUAL_SLOTS).map(|n| Row::Slot(SaveSlot::Manual(n))).collect(),
        SaveMode::Load => SaveSlot::all().map(Row::Slot).collect(),
    };
    if BACKUPS {
        rows.extend([Row::Export, Row::Import]);
    }
    rows
}

pub fn draw_save_menu(
    mut contexts: EguiContexts,
    mut menu: ResMut<SaveMenu>,
    index: Res<SaveIndex>,
    portraits: Res<PortraitTextures>,
    game_data: Res<GameData>,
    input: MenuInput,
    state: Res<State<GameState>>,
    mut saves: MessageWriter<SaveGameEvent>,
    mut loads: MessageWriter<LoadGameEvent>,
    mut map_menu: ResMut<MapMenu>,
    mut notice: ResMut<SaveNotice>,
    mut import: ResMut<ImportPending>,
) {
    menu.just_closed = false;
    let Some(mode) = menu.mode else { return };
    let accept_input = std::mem::replace(&mut menu.shown, true);
    let rows = rows(mode);
    let in_battle = *state.get() == GameState::Battle;
    let enabled = |row: &Row| match row {
        Row::Slot(slot) => mode == SaveMode::Save || index.get(*slot).is_some(),
        Row::Export => index.newest().is_some(),
        Row::Import => true,
    };
    let usable: Vec<usize> = (0..rows.len()).filter(|&i| enabled(&rows[i])).collect();
    if !accept_input {
        // Start on the preferred slot, else the newest save (load) or first empty slot (save)
        let start = menu.prefer
            .or_else(|| match mode {
                SaveMode::Load => index.newest().map(|(slot, _)| slot),
                SaveMode::Save => (1..=MANUAL_SLOTS).map(SaveSlot::Manual).find(|&s| index.get(s).is_none()),
            })
            .and_then(|slot| rows.iter().position(|r| *r == Row::Slot(slot)));
        menu.selected = start.or(usable.first().copied()).unwrap_or(0);
    }

    let mut confirmed = None;
    let mut back = false;
    if accept_input {
        let mut pos = usable.iter().position(|&i| i == menu.selected).unwrap_or(0);
        if !usable.is_empty() && input.step_vertical(&mut pos, usable.len()) {
            menu.armed = false;
        }
        if let Some(&i) = usable.get(pos) {
            menu.selected = i;
        }
        if input.confirm() && usable.contains(&menu.selected) {
            confirmed = Some(menu.selected);
        }
        back = input.back();
    }

    let Ok(ctx) = contexts.ctx_mut() else { return };
    theme::backdrop(ctx, "save_menu_backdrop", 130);
    let mut hint = None;
    egui::Area::new(egui::Id::new("save_menu"))
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            theme::card().show(ui, |ui| {
                ui.set_width(470.0);
                let (icon, title) = match mode {
                    SaveMode::Save => (Icon::Book, "Save game"),
                    SaveMode::Load => (Icon::OpenBook, "Load game"),
                };
                theme::heading_icon(ui, Some(icon), title);
                for (i, row) in rows.iter().enumerate() {
                    if *row == Row::Export {
                        ui.separator();
                    }
                    let selected = i == menu.selected;
                    let response = match row {
                        Row::Slot(slot) => {
                            let summary = index.get(*slot);
                            let warning = (selected && menu.armed).then(|| match mode {
                                SaveMode::Save => "Overwrite this save? Press again",
                                SaveMode::Load => "Leave this battle unsaved? Press again",
                            });
                            slot_row(ui, *slot, summary, selected, enabled(row), warning, &portraits, &game_data)
                        }
                        Row::Export => theme::menu_item_icon(ui, Some(Icon::UnloadOut), "Back up saves",
                            Some("Save every slot to a file"), selected, enabled(row), Tone::Normal),
                        Row::Import => theme::menu_item_icon(ui, Some(Icon::LoadIn), "Restore backup",
                            Some("Load slots from a backup file"), selected, true, Tone::Normal),
                    };
                    if theme::pointer_selects(ui, &response) && enabled(row) && menu.selected != i {
                        menu.selected = i;
                        menu.armed = false;
                    }
                    if response.clicked() {
                        if menu.selected != i {
                            menu.armed = false;
                        }
                        menu.selected = i;
                        confirmed = Some(i);
                    }
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    theme::key_hint(ui, "↑↓", "Choose", false);
                    if theme::key_hint_button(ui, "Enter", if mode == SaveMode::Save { "Save" } else { "Load" }, false) {
                        hint = Some("Enter");
                    }
                    if theme::key_hint_button(ui, "Esc", "Back", false) {
                        hint = Some("Esc");
                    }
                });
            });
        });
    match hint {
        Some("Esc") => back = true,
        Some("Enter") if usable.contains(&menu.selected) => confirmed = Some(menu.selected),
        _ => {}
    }

    if back {
        let to_map_menu = menu.from_map_menu;
        menu.close();
        if to_map_menu {
            map_menu.open = true;
        }
        return;
    }
    let Some(i) = confirmed else { return };
    match rows[i] {
        Row::Slot(slot) => {
            // Overwriting a save, or loading over a battle in progress, asks twice
            let risky = match mode {
                SaveMode::Save => index.get(slot).is_some(),
                SaveMode::Load => in_battle,
            };
            if risky && !menu.armed {
                menu.armed = true;
                theme::queue_sound(ctx, "menu_move");
                return;
            }
            match mode {
                SaveMode::Save => {
                    saves.write(SaveGameEvent { slot });
                }
                SaveMode::Load => {
                    loads.write(LoadGameEvent { slot });
                }
            }
            menu.close();
        }
        Row::Export => export_backup(&mut notice),
        Row::Import => request_import(&mut import, &mut notice),
    }
}

/// One slot: name and age, map and day, and both commanders' portraits
#[allow(clippy::too_many_arguments)]
fn slot_row(
    ui: &mut egui::Ui,
    slot: SaveSlot,
    summary: Option<&SaveSummary>,
    selected: bool,
    enabled: bool,
    warning: Option<&str>,
    portraits: &PortraitTextures,
    game_data: &GameData,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 66.0),
        if enabled { egui::Sense::click() } else { egui::Sense::hover() });
    if enabled {
        let was_selected = ui.ctx().data(|d| d.get_temp::<bool>(response.id)).unwrap_or(false);
        if selected && !was_selected {
            theme::queue_sound(ui.ctx(), "menu_move");
        }
        ui.ctx().data_mut(|d| d.insert_temp(response.id, selected));
        if response.clicked() {
            theme::queue_sound(ui.ctx(), "menu_confirm");
        }
    }
    let painter = ui.painter();
    let fill = match (enabled, selected, response.hovered()) {
        (true, true, _) => theme::GOLD_LIGHT,
        (true, false, true) => theme::PARCHMENT_DEEP,
        _ => egui::Color32::TRANSPARENT,
    };
    painter.rect(rect, 6, fill, if selected && enabled { theme::stroke(1.5, theme::WOOD) } else { egui::Stroke::NONE },
        egui::StrokeKind::Inside);
    if selected && enabled {
        let t = ui.input(|i| i.time) as f32;
        let bob = ((t * 5.0).sin() * 0.5 + 0.5) * 3.0;
        ui.ctx().request_repaint();
        let c = egui::pos2(rect.left() + 12.0 + bob, rect.center().y);
        painter.add(egui::Shape::convex_polygon(
            vec![c + egui::vec2(-4.0, -6.0), c + egui::vec2(5.0, 0.0), c + egui::vec2(-4.0, 6.0)],
            theme::WOOD, egui::Stroke::NONE,
        ));
    }
    let faded = egui::Color32::from_rgb(0xA8, 0x95, 0x7C);
    let icon = if slot == SaveSlot::Auto { Icon::Hourglass } else { Icon::Book };
    let icon_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 43.0, rect.center().y), egui::vec2(30.0, 30.0));
    icons::paint(painter, icon_rect, icon, if summary.is_some() { icon.color() } else { egui::Color32::from_rgb(0xB8, 0xA8, 0x90) });

    let left = icon_rect.right() + 12.0;
    let title_color = if enabled { theme::INK } else { faded };
    painter.text(egui::pos2(left, rect.top() + 16.0), egui::Align2::LEFT_CENTER, slot.name(),
        egui::FontId::new(18.0, theme::bold_family()), title_color);

    // Portraits on the right, the opponent mirrored to face the player
    if let Some(summary) = summary {
        let size = 50.0;
        for (k, &(faction, co)) in summary.armies.iter().take(2).enumerate().rev() {
            let frame = egui::Rect::from_min_size(
                egui::pos2(rect.right() - 8.0 - size * (2 - k) as f32 - 4.0 * (1 - k) as f32, rect.center().y - size / 2.0),
                egui::vec2(size, size));
            let color = theme::faction_color(game_data, faction);
            painter.rect_filled(frame, 4, theme::shade(color, 0.5));
            if let Some(texture) = portraits.get(co) {
                egui::Image::new((texture, frame.size())).uv(theme::portrait_uv(k == 1))
                    .corner_radius(4).paint_at(ui, frame.shrink(2.0));
            }
            ui.painter().rect_stroke(frame, 4, theme::stroke(2.0, color), egui::StrokeKind::Inside);
        }
        let text_right = rect.right() - 8.0 - size * 2.0 - 16.0;
        let painter = ui.painter();
        painter.text(egui::pos2(text_right, rect.top() + 16.0), egui::Align2::RIGHT_CENTER, time_ago(summary.saved_at),
            egui::FontId::proportional(13.0), theme::INK_SOFT);
        let line = match warning {
            Some(text) => text.to_string(),
            None => format!("Day {} · {}", summary.day, summary.map),
        };
        painter.text(egui::pos2(left, rect.top() + 36.0), egui::Align2::LEFT_CENTER, line,
            egui::FontId::new(15.0, theme::bold_family()), if warning.is_some() { theme::BERRY } else { theme::WOOD_DARK });
        let names: Vec<&str> = summary.armies.iter().map(|&(_, co)| game_data.commander(co).name.as_str()).collect();
        painter.text(egui::pos2(left, rect.top() + 53.0), egui::Align2::LEFT_CENTER, names.join(" vs "),
            egui::FontId::proportional(13.0), theme::INK_SOFT);
    } else {
        let text = match (slot, warning) {
            (_, Some(text)) => text,
            (SaveSlot::Auto, _) => "Saved every turn and move",
            _ => "Empty",
        };
        ui.painter().text(egui::pos2(left, rect.top() + 40.0), egui::Align2::LEFT_CENTER, text,
            egui::FontId::proportional(14.0), faded);
    }
    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// "Saved to Slot 1" and friends, low in the middle of the screen
pub fn draw_save_notice(mut contexts: EguiContexts, notice: Res<SaveNotice>) {
    if !notice.visible() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let alpha = ((SaveNotice::SECS - notice.age) / 0.4).clamp(0.0, 1.0) * (notice.age / 0.15).clamp(0.0, 1.0);
    ctx.request_repaint();
    egui::Area::new(egui::Id::new("save_notice"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -56.0))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(alpha);
            theme::hud_card().show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::icon(ui, if notice.error { Icon::Scroll } else { Icon::Book }, 22.0);
                    ui.label(theme::bold(&notice.text, 16.0).color(if notice.error { theme::BERRY } else { theme::INK }));
                });
            });
        });
}

/// F5 saves, F9 loads, wherever that makes sense
pub fn save_hotkeys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<SaveMenu>,
    state: Res<State<GameState>>,
    modal: Res<ModalOpen>,
    boot: Res<crate::states::BootSequence>,
    index: Res<SaveIndex>,
) {
    if menu.blocks_input() {
        return;
    }
    let free = match state.get() {
        GameState::Battle => !modal.0,
        GameState::Menu => boot.stage == crate::states::BootStage::Menu,
        _ => false,
    };
    if !free {
        return;
    }
    if keyboard.just_pressed(KeyCode::F5) && *state.get() == GameState::Battle {
        menu.open(SaveMode::Save, false, None);
    } else if keyboard.just_pressed(KeyCode::F9) && index.newest().is_some() {
        menu.open(SaveMode::Load, false, None);
    }
}

/// A loaded battle skips the setup screen and remembers its choices
pub fn on_game_loaded(mut events: MessageReader<GameLoadedEvent>, mut setup: ResMut<BattleSetupState>) {
    for event in events.read() {
        setup.needs_setup = false;
        if let Some(map) = event.map_id {
            setup.selected_map = map;
        }
    }
}
