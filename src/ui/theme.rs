//! Visual theme: storybook parchment-and-wood look for all egui UI.
//!
//! Palette follows `assets/sprites/prompts/palette.md` and the art direction
//! ("charming woodland warfare ... storybook presentation"). Fonts are
//! Alegreya (OFL, see `assets/fonts/`), embedded so WASM builds stay one file.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, TextStyle};
use std::sync::Arc;

use crate::game::{Faction, GameData};

// ============================================================================
// PALETTE
// ============================================================================

/// Panel paper
pub const PARCHMENT: Color32 = Color32::from_rgb(0xF3, 0xE6, 0xC8);
/// Recessed areas on parchment (rows, wells)
pub const PARCHMENT_DEEP: Color32 = Color32::from_rgb(0xE7, 0xD5, 0xAD);
/// Main text
pub const INK: Color32 = Color32::from_rgb(0x3B, 0x2A, 0x1E);
/// Secondary text
pub const INK_SOFT: Color32 = Color32::from_rgb(0x7A, 0x63, 0x4C);
/// Frames, borders, primary buttons
pub const WOOD: Color32 = Color32::from_rgb(0x7A, 0x4F, 0x2C);
pub const WOOD_DARK: Color32 = Color32::from_rgb(0x4E, 0x32, 0x1C);
/// Selection and highlights
pub const GOLD: Color32 = Color32::from_rgb(0xD9, 0xA4, 0x41);
pub const GOLD_LIGHT: Color32 = Color32::from_rgb(0xF2, 0xCF, 0x73);
/// Positive values (healing, damage dealt, bonuses)
pub const LEAF: Color32 = Color32::from_rgb(0x4E, 0x7F, 0x2E);
/// Danger (damage taken, destroy, surrender)
pub const BERRY: Color32 = Color32::from_rgb(0xB0, 0x3E, 0x2A);
/// Informational accents
pub const SKY: Color32 = Color32::from_rgb(0x3F, 0x72, 0x9B);
/// Full-screen menu backdrop (deep forest)
pub const FOREST_NIGHT: Color32 = Color32::from_rgb(0x1B, 0x26, 0x1D);

pub const HEALTH_HIGH: Color32 = Color32::from_rgb(0x50, 0xA8, 0x5E);
pub const HEALTH_MID: Color32 = Color32::from_rgb(0xD9, 0xB0, 0x3C);
pub const HEALTH_LOW: Color32 = Color32::from_rgb(0xC8, 0x48, 0x3A);

/// Stroke with a concrete f32 width (egui's `Stroke::new` is generic over the width)
pub fn stroke(width: f32, color: Color32) -> Stroke {
    Stroke::new(width, color)
}

/// Convert a Bevy color (e.g. faction color from GameData) to egui
pub fn to_egui(color: Color) -> Color32 {
    let c = color.to_srgba();
    Color32::from_rgb((c.red * 255.0) as u8, (c.green * 255.0) as u8, (c.blue * 255.0) as u8)
}

pub fn faction_color(game_data: &GameData, faction: Faction) -> Color32 {
    to_egui(game_data.faction_color(faction))
}

/// Darker shade of a color, for borders and text on faction banners
pub fn shade(color: Color32, factor: f32) -> Color32 {
    let f = |c: u8| (c as f32 * factor).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(color.r()), f(color.g()), f(color.b()))
}

pub fn health_color(fraction: f32) -> Color32 {
    if fraction > 0.6 {
        HEALTH_HIGH
    } else if fraction > 0.3 {
        HEALTH_MID
    } else {
        HEALTH_LOW
    }
}

// ============================================================================
// FONTS & STYLE
// ============================================================================

const FONT_BODY: &str = "alegreya-sans";
const FONT_BOLD: &str = "alegreya-sans-bold";
const FONT_DISPLAY: &str = "alegreya-sc-bold";
const FONT_TITLE: &str = "alegreya-sc-black";

/// Bold body text family
pub fn bold_family() -> FontFamily {
    FontFamily::Name(FONT_BOLD.into())
}

/// Small-caps display family for headings
pub fn display_family() -> FontFamily {
    FontFamily::Name(FONT_DISPLAY.into())
}

/// Heaviest small-caps family for the game title
pub fn title_family() -> FontFamily {
    FontFamily::Name(FONT_TITLE.into())
}

pub fn display(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, display_family()))
}

pub fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, bold_family()))
}

pub fn body(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::proportional(size))
}

/// Install fonts and visuals once egui is ready
pub fn apply_theme(mut contexts: EguiContexts, mut applied: Local<bool>) {
    if *applied {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    ctx.set_fonts(fonts());
    ctx.all_styles_mut(style);
    *applied = true;
}

fn fonts() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    let mut add = |name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add(FONT_BODY, include_bytes!("../../assets/fonts/AlegreyaSans-Regular.ttf"));
    add(FONT_BOLD, include_bytes!("../../assets/fonts/AlegreyaSans-Bold.ttf"));
    add(FONT_DISPLAY, include_bytes!("../../assets/fonts/AlegreyaSC-Bold.ttf"));
    add(FONT_TITLE, include_bytes!("../../assets/fonts/AlegreyaSC-Black.ttf"));

    // egui's default fonts stay as fallbacks for symbols and emoji
    let fallbacks = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |primary: &str| {
        let mut list = vec![primary.to_owned()];
        list.extend(fallbacks.iter().cloned());
        list
    };
    fonts.families.insert(FontFamily::Proportional, family(FONT_BODY));
    fonts.families.insert(bold_family(), family(FONT_BOLD));
    fonts.families.insert(display_family(), family(FONT_DISPLAY));
    fonts.families.insert(title_family(), family(FONT_TITLE));
    fonts
}

fn style(style: &mut egui::Style) {
    style.text_styles = [
        (TextStyle::Small, FontId::proportional(13.0)),
        (TextStyle::Body, FontId::proportional(17.0)),
        (TextStyle::Button, FontId::new(17.0, bold_family())),
        (TextStyle::Heading, FontId::new(26.0, display_family())),
        (TextStyle::Monospace, FontId::monospace(14.0)),
    ]
    .into();

    style.spacing.item_spacing = egui::vec2(8.0, 6.0);
    style.spacing.button_padding = egui::vec2(14.0, 6.0);
    style.spacing.window_margin = Margin::same(16);
    style.interaction.selectable_labels = false;

    let v = &mut style.visuals;
    v.dark_mode = false;
    v.override_text_color = Some(INK);
    v.panel_fill = PARCHMENT;
    v.window_fill = PARCHMENT;
    v.window_stroke = stroke(2.0, WOOD);
    v.window_corner_radius = CornerRadius::same(10);
    v.window_shadow = egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(90) };
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(70) };
    v.menu_corner_radius = CornerRadius::same(8);
    v.faint_bg_color = PARCHMENT_DEEP;
    v.extreme_bg_color = PARCHMENT_DEEP;
    v.selection.bg_fill = GOLD_LIGHT;
    v.selection.stroke = stroke(1.0, WOOD_DARK);

    let radius = CornerRadius::same(6);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PARCHMENT;
    w.noninteractive.weak_bg_fill = PARCHMENT;
    w.noninteractive.bg_stroke = stroke(1.0, Color32::from_rgb(0xC9, 0xAE, 0x7E));
    w.noninteractive.fg_stroke = stroke(1.0, INK);
    w.noninteractive.corner_radius = radius;

    w.inactive.bg_fill = PARCHMENT_DEEP;
    w.inactive.weak_bg_fill = PARCHMENT_DEEP;
    w.inactive.bg_stroke = stroke(1.0, Color32::from_rgb(0xB8, 0x98, 0x64));
    w.inactive.fg_stroke = stroke(1.0, INK);
    w.inactive.corner_radius = radius;

    w.hovered.bg_fill = GOLD_LIGHT;
    w.hovered.weak_bg_fill = GOLD_LIGHT;
    w.hovered.bg_stroke = stroke(1.5, WOOD);
    w.hovered.fg_stroke = stroke(1.5, INK);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 1.0;

    w.active.bg_fill = GOLD;
    w.active.weak_bg_fill = GOLD;
    w.active.bg_stroke = stroke(1.5, WOOD_DARK);
    w.active.fg_stroke = stroke(1.5, INK);
    w.active.corner_radius = radius;

    w.open = w.active;
}

// ============================================================================
// FRAMES
// ============================================================================

/// Standard parchment card with a wooden border
pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(PARCHMENT)
        .stroke(stroke(2.0, WOOD))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(14))
        .shadow(egui::Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(80) })
}

/// Compact card for HUD elements floating over the board
pub fn hud_card() -> egui::Frame {
    card().inner_margin(Margin::symmetric(12, 8)).corner_radius(CornerRadius::same(8))
}

/// See-through HUD card for info that sits over the board (the tile and unit
/// under the cursor), so the map stays visible beneath it
pub fn glass_card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(PARCHMENT.r(), PARCHMENT.g(), PARCHMENT.b(), 175))
        .stroke(stroke(1.5, Color32::from_rgba_unmultiplied(WOOD.r(), WOOD.g(), WOOD.b(), 110)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .shadow(egui::Shadow { offset: [0, 2], blur: 18, spread: 2, color: Color32::from_black_alpha(35) })
}

/// Recessed well inside a card
pub fn well() -> egui::Frame {
    egui::Frame::new()
        .fill(PARCHMENT_DEEP)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10))
}

// ============================================================================
// WIDGETS
// ============================================================================

/// Card heading in small caps with a thin rule underneath
pub fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(display(text, 22.0).color(WOOD_DARK));
    let rect = ui.available_rect_before_wrap();
    let y = ui.cursor().top() - 2.0;
    ui.painter().hline(rect.left()..=rect.right(), y, stroke(1.0, Color32::from_rgb(0xC9, 0xAE, 0x7E)));
    ui.add_space(4.0);
}

/// Visual tone of a menu row
#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Normal,
    Danger,
    /// Done for now, like a unit that has already moved
    Muted,
}

/// A full-width menu row that shows keyboard/gamepad selection and mouse hover
/// the same way. Returns the response so callers can sync hover with selection.
pub fn menu_item(ui: &mut egui::Ui, label: &str, detail: Option<&str>, selected: bool, enabled: bool, tone: Tone) -> egui::Response {
    let height = if detail.is_some() { 48.0 } else { 38.0 };
    let width = ui.available_width();
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), sense);
    let painter = ui.painter();
    // Selection (keyboard/gamepad focus) is gold; plain hover is a softer tint
    let fill = match (enabled, selected, response.hovered()) {
        (true, true, _) => GOLD_LIGHT,
        (true, false, true) => PARCHMENT_DEEP,
        _ => Color32::TRANSPARENT,
    };
    painter.rect(rect, CornerRadius::same(6), fill,
        if selected && enabled { stroke(1.5, WOOD) } else { Stroke::NONE }, egui::StrokeKind::Inside);

    let text_color = match (enabled, tone) {
        (false, _) => Color32::from_rgb(0xA8, 0x95, 0x7C),
        (true, Tone::Danger) => BERRY,
        (true, Tone::Normal) => INK,
        (true, Tone::Muted) => INK_SOFT,
    };
    let left = rect.left() + 28.0;
    if selected && enabled {
        // Selection pip
        let c = egui::pos2(rect.left() + 14.0, rect.center().y);
        painter.add(egui::Shape::convex_polygon(
            vec![c + egui::vec2(-4.0, -6.0), c + egui::vec2(5.0, 0.0), c + egui::vec2(-4.0, 6.0)],
            WOOD, Stroke::NONE,
        ));
    }
    let title_y = if detail.is_some() { rect.top() + 15.0 } else { rect.center().y };
    painter.text(egui::pos2(left, title_y), egui::Align2::LEFT_CENTER, label,
        FontId::new(18.0, bold_family()), text_color);
    if let Some(detail) = detail {
        painter.text(egui::pos2(left, rect.top() + 34.0), egui::Align2::LEFT_CENTER, detail,
            FontId::proportional(14.0), if enabled { INK_SOFT } else { text_color });
    }

    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// Whether the mouse is actively pointing at this item. A pointer resting
/// over a menu must not fight keyboard/gamepad selection, so only movement counts.
pub fn pointer_selects(ui: &egui::Ui, response: &egui::Response) -> bool {
    response.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)
}

/// Big primary action button (wood with parchment text)
pub fn primary_button(ui: &mut egui::Ui, label: &str, enabled: bool, focused: bool) -> egui::Response {
    let size = egui::vec2(ui.available_width().min(260.0), 48.0);
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hot = enabled && (focused || response.hovered());
    let fill = match (enabled, hot) {
        (false, _) => Color32::from_rgb(0xB8, 0xA6, 0x8A),
        (true, true) => Color32::from_rgb(0x8F, 0x5E, 0x35),
        (true, false) => WOOD,
    };
    let painter = ui.painter();
    painter.rect(rect, CornerRadius::same(8), fill, stroke(2.0, if hot { GOLD } else { WOOD_DARK }), egui::StrokeKind::Inside);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, label, FontId::new(22.0, display_family()),
        if enabled { PARCHMENT } else { Color32::from_rgb(0xEC, 0xE2, 0xCC) });
    if enabled { response.on_hover_cursor(egui::CursorIcon::PointingHand) } else { response }
}

/// Keyboard/gamepad hint: a key cap followed by what it does
pub fn key_hint(ui: &mut egui::Ui, key: &str, action: &str, on_dark: bool) {
    let cap = egui::Frame::new()
        .fill(if on_dark { Color32::from_rgb(0xE9, 0xDB, 0xBB) } else { PARCHMENT_DEEP })
        .stroke(stroke(1.0, if on_dark { Color32::from_rgb(0x9C, 0x83, 0x5E) } else { Color32::from_rgb(0xB8, 0x98, 0x64) }))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 1));
    cap.show(ui, |ui| ui.add(egui::Label::new(bold(key, 13.0).color(INK)).extend()));
    if !action.is_empty() {
        ui.add(egui::Label::new(body(action, 14.0).color(if on_dark { PARCHMENT } else { INK_SOFT })).extend());
    }
    ui.add_space(8.0);
}

/// Horizontal meter (HP, power, capture progress). `segments` > 1 draws notches.
pub fn meter(ui: &mut egui::Ui, width: f32, height: f32, fraction: f32, fill: Color32, segments: u32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter();
    let r = CornerRadius::same((height / 2.0).min(6.0) as u8);
    painter.rect_filled(rect, r, Color32::from_rgb(0xD8, 0xC3, 0x96));
    let mut filled = rect;
    filled.set_width(rect.width() * fraction.clamp(0.0, 1.0));
    if filled.width() > 0.5 {
        painter.rect_filled(filled, r, fill);
    }
    for i in 1..segments {
        let x = rect.left() + rect.width() * i as f32 / segments as f32;
        painter.vline(x, rect.y_range(), stroke(1.0, Color32::from_black_alpha(50)));
    }
    painter.rect_stroke(rect, r, stroke(1.0, WOOD_DARK), egui::StrokeKind::Inside);
    response
}

/// Round portrait in a faction-colored frame. Uses the commander's portrait
/// sprite when there is one, otherwise their initial on a medallion.
pub fn portrait(ui: &mut egui::Ui, size: f32, color: Color32, name: &str, image: Option<egui::TextureId>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let c = rect.center();
    if let Some(texture) = image {
        let inner = rect.shrink(3.0);
        egui::Image::new((texture, inner.size()))
            .corner_radius(CornerRadius::same((inner.width() / 2.0) as u8))
            .paint_at(ui, inner);
        let painter = ui.painter();
        painter.circle_stroke(c, size / 2.0 - 1.5, stroke(3.0, shade(color, 0.55)));
        painter.circle_stroke(c, size / 2.0 - 3.5, stroke(1.5, color));
        return response;
    }
    let painter = ui.painter();
    painter.circle(c, size / 2.0, shade(color, 0.55), Stroke::NONE);
    painter.circle(c, size / 2.0 - 3.0, color, stroke(1.5, shade(color, 1.25)));
    let initial: String = name.chars().next().map(|ch| ch.to_uppercase().collect()).unwrap_or_default();
    painter.text(c + egui::vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, initial,
        FontId::new(size * 0.5, title_family()), PARCHMENT);
    response
}

/// Small pill showing a stat change, e.g. "+10% ATK"
pub fn chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.15))
        .stroke(stroke(1.0, color.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(8, 1))
        .show(ui, |ui| ui.add(egui::Label::new(bold(text, 13.0).color(shade(color, 0.8))).extend()));
}

/// Dim the whole screen behind a modal
pub fn backdrop(ctx: &egui::Context, id: &str, alpha: u8) {
    egui::Area::new(egui::Id::new(id))
        .fixed_pos(egui::Pos2::ZERO)
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            let screen = ui.ctx().content_rect();
            ui.allocate_rect(screen, egui::Sense::click());
            ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(alpha));
        });
}

/// Paint the full-screen forest backdrop used by the title and setup screens
pub fn forest_backdrop(painter: &egui::Painter, rect: egui::Rect, time: f32) {
    // Sky-to-forest vertical gradient
    let steps = 24;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let band = egui::Rect::from_min_max(
            egui::pos2(rect.left(), egui::lerp(rect.top()..=rect.bottom(), t0)),
            egui::pos2(rect.right(), egui::lerp(rect.top()..=rect.bottom(), t1) + 1.0),
        );
        painter.rect_filled(band, 0.0, lerp_color(Color32::from_rgb(0x3E, 0x52, 0x3C), FOREST_NIGHT, t0));
    }

    // Three layers of pine silhouettes, nearer layers darker and larger
    let layers = [
        (0.62, 70.0, Color32::from_rgb(0x2F, 0x42, 0x31), 0.0),
        (0.74, 95.0, Color32::from_rgb(0x24, 0x34, 0x26), 37.0),
        (0.88, 130.0, Color32::from_rgb(0x17, 0x21, 0x18), 71.0),
    ];
    for (horizon, tree_h, color, seed) in layers {
        let base_y = egui::lerp(rect.top()..=rect.bottom(), horizon);
        painter.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.left(), base_y), rect.max), 0.0, color);
        let spacing = tree_h * 0.55;
        let mut x = rect.left() - spacing;
        let mut i = 0.0_f32;
        while x < rect.right() + spacing {
            let h = tree_h * (0.7 + 0.3 * ((i * 1.7 + seed).sin() * 0.5 + 0.5));
            let w = h * 0.42;
            let sway = (time * 0.6 + i + seed).sin() * 1.5;
            for tier in 0..3 {
                let tier_f = tier as f32;
                let top = base_y - h + tier_f * h * 0.22;
                let half = w * (0.45 + tier_f * 0.25);
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(x + sway, top),
                        egui::pos2(x + half, base_y - h * 0.18 * (2.0 - tier_f)),
                        egui::pos2(x - half, base_y - h * 0.18 * (2.0 - tier_f)),
                    ],
                    color,
                    Stroke::NONE,
                ));
            }
            x += spacing * (0.8 + 0.4 * ((i * 2.3 + seed).cos() * 0.5 + 0.5));
            i += 1.0;
        }
    }

    // Fireflies
    for i in 0..18 {
        let fi = i as f32;
        let px = rect.left() + rect.width() * ((fi * 0.618).fract());
        let py = egui::lerp(rect.top()..=rect.bottom(), 0.55 + 0.35 * ((fi * 0.377).fract()))
            + (time * 0.8 + fi).sin() * 6.0;
        let glow = ((time * 1.3 + fi * 1.9).sin() * 0.5 + 0.5) * 0.8;
        painter.circle_filled(egui::pos2(px, py), 5.0, GOLD_LIGHT.gamma_multiply(glow * 0.25));
        painter.circle_filled(egui::pos2(px, py), 1.8, GOLD_LIGHT.gamma_multiply(glow));
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| egui::lerp(x as f32..=y as f32, t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}
