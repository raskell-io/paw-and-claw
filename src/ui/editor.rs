//! Map editor: paint terrain, place units and assign property owners

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::theme::{self, Tone};
use super::{EditorMode, EditorState};
use crate::game::{Faction, GameData, MapData, PropertyOwnership, Terrain, UnitPlacement, UnitType};
use crate::states::GameState;

/// Maps are authored for two army slots; battles assign real factions to them
/// (see `MapData::with_factions`)
const ARMIES: [(Faction, &str); 2] = [(Faction::Eastern, "First army"), (Faction::Northern, "Second army")];

const PALETTE: [Terrain; 13] = [
    Terrain::Grass, Terrain::TallGrass, Terrain::Thicket, Terrain::Brambles, Terrain::Log,
    Terrain::Boulder, Terrain::Hollow, Terrain::Creek, Terrain::Pond, Terrain::Shore,
    Terrain::Base, Terrain::Outpost, Terrain::Storehouse,
];

const PLACEABLE_UNITS: [UnitType; 5] =
    [UnitType::Scout, UnitType::Shocktrooper, UnitType::Recon, UnitType::Ironclad, UnitType::Siege];

/// Start every editor session from a blank map
pub fn setup_editor(mut editor_state: ResMut<EditorState>) {
    *editor_state = EditorState::default();
}

pub fn draw_editor(
    mut contexts: EguiContexts,
    mut editor: ResMut<EditorState>,
    mut next_state: ResMut<NextState<GameState>>,
    game_data: Res<GameData>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };

    egui::SidePanel::left("editor_tools")
        .exact_width(300.0)
        .resizable(false)
        .show_separator_line(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 10)).show(ui, |ui| {
                    tools_panel(ui, &mut editor, &mut next_state, &game_data)
                });
            });
        });

    egui::CentralPanel::default().frame(egui::Frame::new().fill(egui::Color32::from_rgb(0x1F, 0x2B, 0x21)))
        .show(ctx, |ui| canvas(ui, &mut editor, &game_data));
}

fn tools_panel(ui: &mut egui::Ui, editor: &mut EditorState, next_state: &mut NextState<GameState>, game_data: &GameData) {
    ui.label(theme::display("Map Editor", 28.0).color(theme::WOOD_DARK));
    ui.add_space(6.0);

    theme::heading(ui, "Map");
    ui.label(theme::body("Name", 14.0).color(theme::INK_SOFT));
    ui.text_edit_singleline(&mut editor.map.name);
    ui.horizontal(|ui| {
        let (mut width, mut height) = (editor.map.width, editor.map.height);
        ui.label(theme::body("Size", 14.0).color(theme::INK_SOFT));
        let w = ui.add(egui::DragValue::new(&mut width).range(8..=20).suffix(" wide"));
        let h = ui.add(egui::DragValue::new(&mut height).range(6..=16).suffix(" tall"));
        if (w.changed() || h.changed()) && (width, height) != (editor.map.width, editor.map.height) {
            editor.map = resized(&editor.map, width, height);
        }
    });
    ui.add_space(8.0);

    theme::heading(ui, "Brush");
    ui.horizontal(|ui| {
        for (mode, label) in [(EditorMode::Terrain, "Terrain"), (EditorMode::Units, "Units"), (EditorMode::Properties, "Owners")] {
            if ui.selectable_label(editor.mode == mode, theme::bold(label, 15.0)).clicked() {
                editor.mode = mode;
            }
        }
    });
    ui.add_space(4.0);

    match editor.mode {
        EditorMode::Terrain => {
            egui::Grid::new("palette").num_columns(2).spacing([6.0, 4.0]).show(ui, |ui| {
                for (i, terrain) in PALETTE.iter().enumerate() {
                    let selected = editor.selected_terrain == *terrain;
                    if swatch_button(ui, game_data, *terrain, selected).clicked() {
                        editor.selected_terrain = *terrain;
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        }
        EditorMode::Units => {
            army_picker(ui, editor, game_data);
            ui.add_space(4.0);
            for unit_type in PLACEABLE_UNITS {
                let detail = format!("Symbol {}", game_data.unit(unit_type).symbol);
                if theme::menu_item(ui, game_data.unit_name(unit_type), Some(&detail), editor.selected_unit == unit_type, true, Tone::Normal).clicked() {
                    editor.selected_unit = unit_type;
                }
            }
        }
        EditorMode::Properties => {
            army_picker(ui, editor, game_data);
            ui.label(theme::body("Click a base, outpost or storehouse to give it to this army.", 14.0).color(theme::INK_SOFT));
        }
    }
    ui.add_space(8.0);

    theme::heading(ui, "Overview");
    for (faction, label) in ARMIES {
        let units = editor.map.units.iter().filter(|u| u.faction == faction).count();
        let props = editor.map.properties.iter().filter(|p| p.owner == faction).count();
        let has_base = editor.map.properties.iter()
            .any(|p| p.owner == faction && editor.map.get_terrain(p.x, p.y) == Some(Terrain::Base));
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 6.0, theme::faction_color(game_data, faction));
            ui.label(theme::bold(label, 15.0));
            ui.label(theme::body(format!("{units} units · {props} properties"), 14.0).color(theme::INK_SOFT));
        });
        if !has_base {
            theme::chip(ui, &format!("{label} has no base"), theme::BERRY);
        }
    }
    ui.add_space(8.0);

    theme::heading(ui, "File");
    ui.horizontal(|ui| {
        ui.text_edit_singleline(&mut editor.map_name);
        ui.label(theme::body(".json", 14.0).color(theme::INK_SOFT));
    });
    ui.horizontal(|ui| {
        if ui.button("Save map").clicked() {
            let path = std::path::Path::new(&editor.map_name).with_extension("json");
            editor.status = match editor.map.save_to_file(&path) {
                Ok(()) => format!("Saved to {}", path.display()),
                Err(e) => format!("Save failed: {e}"),
            };
        }
        if ui.button("Clear").clicked() {
            let (name, w, h) = (editor.map.name.clone(), editor.map.width, editor.map.height);
            editor.map = MapData::new(&name, w, h);
            editor.status = "Map cleared".into();
        }
    });
    if !editor.status.is_empty() {
        ui.label(theme::body(&editor.status, 14.0).color(theme::WOOD));
    }
    ui.add_space(12.0);
    if theme::menu_item(ui, "Back to title", None, false, true, Tone::Normal).clicked() {
        next_state.set(GameState::Menu);
    }
    ui.add_space(8.0);
    ui.label(theme::body("Left-click paints · right-click erases · drag to paint many tiles", 13.0).color(theme::INK_SOFT));
}

fn army_picker(ui: &mut egui::Ui, editor: &mut EditorState, game_data: &GameData) {
    ui.horizontal(|ui| {
        for (faction, label) in ARMIES {
            let color = theme::faction_color(game_data, faction);
            let selected = editor.selected_faction == faction;
            let button = egui::Button::new(theme::bold(label, 15.0).color(if selected { theme::PARCHMENT } else { theme::INK }))
                .fill(if selected { color } else { theme::PARCHMENT_DEEP })
                .stroke(theme::stroke(1.5, theme::shade(color, 0.6)));
            if ui.add(button).clicked() {
                editor.selected_faction = faction;
            }
        }
    });
}

fn swatch_button(ui: &mut egui::Ui, game_data: &GameData, terrain: Terrain, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(118.0, 30.0), egui::Sense::click());
    let painter = ui.painter();
    let hot = selected || response.hovered();
    painter.rect(rect, 6, if hot { theme::GOLD_LIGHT } else { theme::PARCHMENT_DEEP },
        theme::stroke(if selected { 1.5 } else { 0.0 }, theme::WOOD), egui::StrokeKind::Inside);
    let sw = egui::Rect::from_min_size(rect.min + egui::vec2(6.0, 6.0), egui::vec2(18.0, 18.0));
    painter.rect(sw, 3, theme::to_egui(game_data.terrain_color(terrain)), theme::stroke(1.0, theme::WOOD_DARK), egui::StrokeKind::Inside);
    painter.text(egui::pos2(sw.right() + 8.0, rect.center().y), egui::Align2::LEFT_CENTER, game_data.terrain_name(terrain),
        egui::FontId::new(14.0, theme::bold_family()), theme::INK);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Draw the map and apply brush strokes from the mouse
fn canvas(ui: &mut egui::Ui, editor: &mut EditorState, game_data: &GameData) {
    let area = ui.available_rect_before_wrap().shrink(24.0);
    let (w, h) = (editor.map.width as f32, editor.map.height as f32);
    let cell = (area.width() / w).min(area.height() / h).floor().max(8.0);
    let board = egui::Rect::from_center_size(area.center(), egui::vec2(cell * w, cell * h));
    let response = ui.allocate_rect(board, egui::Sense::click_and_drag());

    // Brush strokes: primary paints, secondary erases; drags paint terrain continuously
    let hovered_tile = response.hover_pos().map(|p| {
        let local = (p - board.min) / cell;
        (local.x.floor() as i32, local.y.floor() as i32)
    }).filter(|&(x, y)| x >= 0 && y >= 0 && x < editor.map.width as i32 && y < editor.map.height as i32);
    if let Some((x, y)) = hovered_tile {
        let (paint, erase, fresh) = ui.input(|i| (
            i.pointer.primary_down(),
            i.pointer.secondary_down(),
            i.pointer.any_pressed(),
        ));
        if paint || erase {
            apply_brush(editor, game_data, x, y, erase, fresh);
        }
    }

    let painter = ui.painter_at(board.expand(4.0));
    painter.rect_filled(board.expand(4.0), 6, theme::WOOD_DARK);
    for y in 0..editor.map.height {
        for x in 0..editor.map.width {
            let terrain = editor.map.terrain[y as usize][x as usize];
            let r = egui::Rect::from_min_size(board.min + egui::vec2(x as f32 * cell, y as f32 * cell), egui::vec2(cell, cell));
            painter.rect_filled(r.shrink(0.5), 2, theme::to_egui(game_data.terrain_color(terrain)));
            if game_data.terrain_has_feature(terrain) && cell >= 24.0 {
                // Small glyph so features read at a glance
                let glyph = match terrain {
                    Terrain::Base => "⌂", Terrain::Outpost => "▲", Terrain::Storehouse => "■",
                    Terrain::Thicket | Terrain::Brambles => "♣", Terrain::Boulder => "●", Terrain::Log => "═",
                    _ => "◦",
                };
                painter.text(r.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, glyph,
                    egui::FontId::proportional(cell * 0.28), egui::Color32::from_black_alpha(110));
            }
        }
    }
    for p in &editor.map.properties {
        let r = egui::Rect::from_min_size(board.min + egui::vec2(p.x as f32 * cell, p.y as f32 * cell), egui::vec2(cell, cell));
        painter.rect_stroke(r.shrink(2.0), 3, theme::stroke((cell * 0.08).max(2.0), theme::faction_color(game_data, p.owner)), egui::StrokeKind::Inside);
    }
    for u in &editor.map.units {
        let c = board.min + egui::vec2((u.x as f32 + 0.5) * cell, (u.y as f32 + 0.5) * cell);
        let color = theme::faction_color(game_data, u.faction);
        painter.circle(c, cell * 0.3, color, theme::stroke(2.0, theme::shade(color, 0.5)));
        painter.text(c + egui::vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, game_data.unit(u.unit_type).symbol.to_string(),
            egui::FontId::new(cell * 0.32, theme::title_family()), theme::PARCHMENT);
    }
    if let Some((x, y)) = hovered_tile {
        let r = egui::Rect::from_min_size(board.min + egui::vec2(x as f32 * cell, y as f32 * cell), egui::vec2(cell, cell));
        painter.rect_stroke(r.shrink(1.0), 3, theme::stroke(2.5, theme::GOLD_LIGHT), egui::StrokeKind::Inside);
        let terrain = editor.map.terrain[y as usize][x as usize];
        response.on_hover_text_at_pointer(format!("{} ({x}, {y})", game_data.terrain_name(terrain)));
    }
}

fn apply_brush(editor: &mut EditorState, game_data: &GameData, x: i32, y: i32, erase: bool, fresh_press: bool) {
    match editor.mode {
        EditorMode::Terrain => {
            let terrain = if erase { Terrain::Grass } else { editor.selected_terrain };
            editor.map.set_terrain(x, y, terrain);
            // Properties only make sense on capturable terrain
            if !game_data.terrain_capturable(terrain) {
                editor.map.properties.retain(|p| p.x != x || p.y != y);
            }
        }
        // Placing units and owners is per click, not per drag
        EditorMode::Units if fresh_press => {
            editor.map.units.retain(|u| u.x != x || u.y != y);
            if !erase {
                editor.map.units.push(UnitPlacement { unit_type: editor.selected_unit, faction: editor.selected_faction, x, y });
            }
        }
        EditorMode::Properties if fresh_press => {
            editor.map.properties.retain(|p| p.x != x || p.y != y);
            let capturable = editor.map.get_terrain(x, y).is_some_and(|t| game_data.terrain_capturable(t));
            if !erase && capturable {
                editor.map.properties.push(PropertyOwnership { x, y, owner: editor.selected_faction });
            }
        }
        _ => {}
    }
}

/// Resize while keeping whatever fits of the existing map
fn resized(map: &MapData, width: u32, height: u32) -> MapData {
    let mut new = MapData::new(&map.name, width, height);
    new.description = map.description.clone();
    for y in 0..height.min(map.height) {
        for x in 0..width.min(map.width) {
            new.terrain[y as usize][x as usize] = map.terrain[y as usize][x as usize];
        }
    }
    let inside = |x: i32, y: i32| x < width as i32 && y < height as i32;
    new.units = map.units.iter().filter(|u| inside(u.x, u.y)).cloned().collect();
    new.properties = map.properties.iter().filter(|p| inside(p.x, p.y)).cloned().collect();
    new
}
