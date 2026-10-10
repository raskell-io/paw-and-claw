//! Title screen

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::input::MenuInput;
use super::theme::{self, Tone};
use crate::states::GameState;

struct MenuEntry {
    label: &'static str,
    detail: &'static str,
    target: Option<MenuTarget>,
}

#[derive(Clone)]
enum MenuTarget {
    State(GameState),
    Quit,
}

fn entries() -> Vec<MenuEntry> {
    let mut entries = vec![
        MenuEntry { label: "Skirmish", detail: "Pick a faction, a commander and a map", target: Some(MenuTarget::State(GameState::Battle)) },
        MenuEntry { label: "Map Editor", detail: "Paint your own battlefield", target: Some(MenuTarget::State(GameState::Editor)) },
        MenuEntry { label: "Campaign", detail: "Coming soon", target: None },
        MenuEntry { label: "Roguelike", detail: "Coming soon", target: None },
    ];
    // Browsers own the tab lifecycle, so only offer Quit on native
    if cfg!(not(target_arch = "wasm32")) {
        entries.push(MenuEntry { label: "Quit", detail: "Back to the burrow", target: Some(MenuTarget::Quit) });
    }
    entries
}

pub fn draw_main_menu(
    mut contexts: EguiContexts,
    mut next_state: ResMut<NextState<GameState>>,
    mut exit: MessageWriter<AppExit>,
    input: MenuInput,
    time: Res<Time>,
    mut selected: Local<usize>,
) {
    let entries = entries();
    let enabled: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].target.is_some()).collect();
    // Keyboard/gamepad navigation skips disabled entries
    let mut pos = enabled.iter().position(|&i| i == *selected).unwrap_or(0);
    input.step_vertical(&mut pos, enabled.len());
    *selected = enabled[pos];

    let mut activate = input.confirm().then_some(*selected);

    let Ok(ctx) = contexts.ctx_mut() else { return };
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let screen = ui.max_rect();
        theme::forest_backdrop(ui.painter(), screen, time.elapsed_secs());

        // Title with a soft drop shadow
        let title_pos = egui::pos2(screen.center().x, screen.top() + screen.height() * 0.2);
        let title_font = egui::FontId::new(84.0, theme::title_family());
        ui.painter().text(title_pos + egui::vec2(0.0, 4.0), egui::Align2::CENTER_CENTER, "Paw & Claw",
            title_font.clone(), egui::Color32::from_black_alpha(140));
        ui.painter().text(title_pos, egui::Align2::CENTER_CENTER, "Paw & Claw", title_font, theme::PARCHMENT);
        ui.painter().text(title_pos + egui::vec2(0.0, 58.0), egui::Align2::CENTER_CENTER,
            "Tactics of the Woodland Realms", egui::FontId::new(22.0, theme::display_family()), theme::GOLD_LIGHT);

        let card_width = 380.0;
        let card_top = title_pos.y + 110.0;
        let card_rect = egui::Rect::from_min_max(
            egui::pos2(screen.center().x - card_width / 2.0, card_top),
            egui::pos2(screen.center().x + card_width / 2.0, screen.bottom()),
        );
        let mut card_ui = ui.new_child(egui::UiBuilder::new().max_rect(card_rect));
        card_ui.vertical_centered(|ui| {
            theme::card().show(ui, |ui| {
                ui.set_width(card_width - 28.0);
                for (i, entry) in entries.iter().enumerate() {
                    let response = theme::menu_item(ui, entry.label, Some(entry.detail), i == *selected,
                        entry.target.is_some(), Tone::Normal);
                    if theme::pointer_selects(ui, &response) && entry.target.is_some() {
                        *selected = i;
                    }
                    if response.clicked() {
                        activate = Some(i);
                    }
                }
            });
        });

        // Controls hint along the bottom
        let hint_rect = egui::Rect::from_min_size(egui::pos2(screen.left() + 24.0, screen.bottom() - 40.0), egui::vec2(screen.width(), 30.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(hint_rect), |ui| {
            ui.horizontal(|ui| {
                theme::key_hint(ui, "↑↓", "Choose", true);
                theme::key_hint(ui, "Enter", "Select", true);
            });
        });
        ui.painter().text(egui::pos2(screen.right() - 24.0, screen.bottom() - 26.0), egui::Align2::RIGHT_CENTER,
            concat!("v", env!("CARGO_PKG_VERSION")), egui::FontId::proportional(14.0), theme::PARCHMENT.gamma_multiply(0.6));
    });

    if let Some(i) = activate {
        match entries[i].target.clone() {
            Some(MenuTarget::State(state)) => next_state.set(state),
            Some(MenuTarget::Quit) => {
                exit.write(AppExit::Success);
            }
            None => {}
        }
    }
}
