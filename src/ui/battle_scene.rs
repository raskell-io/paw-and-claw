//! Draws the Advance Wars battle scene (`game::battle_scene` runs the clock):
//! a widescreen panel split down the middle, each side on a backdrop of the
//! ground its unit stands on, with a squad of figures sized to its HP. The
//! army facing right stands on the left. Strikes lunge (melee) or send a
//! volley across (ranged); the side that's hit flashes and shakes while its
//! HP counts down and figures fall. Any button or tap skips.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::input::MenuInput;
use super::theme;
use super::UnitTextures;
use crate::game::{
    BattleConfig, BattleReport, BattleScene, Combatant, GameData, Terrain, UnitClass, UnitType, INTRO, IMPACT, OUTRO,
    STRIKE, WINDUP,
};

pub fn draw_battle_scene(
    mut contexts: EguiContexts,
    mut scene: ResMut<BattleScene>,
    game_data: Res<GameData>,
    battle_config: Res<BattleConfig>,
    unit_textures: Res<UnitTextures>,
    input: MenuInput,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
) {
    let Some((report, t)) = scene.current.clone() else { return };
    // Any button or tap skips (not in the first moment, so a double press doesn't)
    if t > 0.25 && (input.confirm() || input.back() || mouse.just_pressed(MouseButton::Left) || touches.any_just_released()) {
        scene.skip = true;
    }
    let Ok(ctx) = contexts.ctx_mut() else { return };
    ctx.request_repaint();
    let screen = ctx.content_rect();
    let ease = |x: f32| { let x = x.clamp(0.0, 1.0); 1.0 - (1.0 - x).powi(3) };
    let fade = ease(t / 0.25) * (1.0 - ((t - (report.duration() - OUTRO)) / OUTRO).clamp(0.0, 1.0));

    // The army facing right stands on the left (attacker first when both face the same way)
    let attacker_left = !battle_config.facing_left(report.attacker.faction) || battle_config.facing_left(report.defender.faction);
    let (hp_attacker, hp_defender) = report.hp_at(t);

    egui::Area::new(egui::Id::new("battle_scene"))
        .fixed_pos(egui::Pos2::ZERO)
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            let painter = ui.painter().clone();
            painter.rect_filled(screen, 0, egui::Color32::from_black_alpha((170.0 * fade) as u8));
            let width = (screen.width() - 32.0).min(1000.0);
            let height = (screen.height() * 0.66).min(440.0);
            let panel = egui::Rect::from_center_size(screen.center(), egui::vec2(width, height));
            let slide = (1.0 - ease(t / INTRO)) * screen.width() * 0.6;

            for left in [true, false] {
                let is_attacker = left == attacker_left;
                let (side, hp) = if is_attacker { (&report.attacker, hp_attacker) } else { (&report.defender, hp_defender) };
                let half = if left {
                    egui::Rect::from_min_max(panel.min, egui::pos2(panel.center().x, panel.max.y)).translate(egui::vec2(-slide, 0.0))
                } else {
                    egui::Rect::from_min_max(egui::pos2(panel.center().x, panel.min.y), panel.max).translate(egui::vec2(slide, 0.0))
                };
                draw_side(ui, &painter, &game_data, &unit_textures, &report, side, hp, is_attacker, left, half, t, fade);
            }
            // The split down the middle
            let mid = panel.center().x;
            painter.line_segment([egui::pos2(mid, panel.top()), egui::pos2(mid, panel.bottom())],
                theme::stroke(4.0, theme::WOOD_DARK.gamma_multiply(fade)));
            painter.rect_stroke(panel, 6, theme::stroke(3.0, theme::WOOD_DARK.gamma_multiply(fade)), egui::StrokeKind::Outside);

            // Volleys fly across the split
            if let Some((strike, f)) = report.strike_at(t) {
                if strike.ranged {
                    let shooter_left = strike.by_attacker == attacker_left;
                    let k = ((f * STRIKE - WINDUP * 0.5) / (WINDUP * 0.5 + IMPACT * 0.25)).clamp(0.0, 1.0);
                    if k > 0.0 && k < 1.0 {
                        let ground = panel.top() + panel.height() * 0.62;
                        let (from, to) = if shooter_left {
                            (egui::pos2(panel.left() + width * 0.3, ground), egui::pos2(panel.right() - width * 0.25, ground))
                        } else {
                            (egui::pos2(panel.right() - width * 0.3, ground), egui::pos2(panel.left() + width * 0.25, ground))
                        };
                        for n in 0..4 {
                            let kk = (k - n as f32 * 0.06).clamp(0.0, 1.0);
                            let p = from + (to - from) * kk - egui::vec2(0.0, (kk * std::f32::consts::PI).sin() * height * 0.45);
                            painter.circle_filled(p + egui::vec2(0.0, n as f32 * 9.0), 4.5, theme::INK.gamma_multiply(fade));
                        }
                    }
                }
            }
            painter.text(egui::pos2(panel.center().x, panel.bottom() + 22.0), egui::Align2::CENTER_CENTER,
                "Tap or press any button to skip", egui::FontId::proportional(14.0), theme::PARCHMENT.gamma_multiply(0.7 * fade));
        });
}

/// One side: backdrop, squad, strike motion and the HP bar
#[allow(clippy::too_many_arguments)]
fn draw_side(
    ui: &egui::Ui,
    painter: &egui::Painter,
    game_data: &GameData,
    unit_textures: &UnitTextures,
    report: &BattleReport,
    side: &Combatant,
    hp: f32,
    is_attacker: bool,
    left: bool,
    half: egui::Rect,
    t: f32,
    fade: f32,
) {
    let painter = painter.with_clip_rect(half);
    let ground_top = half.top() + half.height() * 0.58;
    backdrop(&painter, half, ground_top, side.terrain, fade);

    // Motion: the striking side lunges (melee) or recoils (ranged); the side
    // that's hit flashes and shakes as the blow lands
    let mut offset = egui::Vec2::ZERO;
    let mut flash = 0.0;
    if let Some((strike, f)) = report.strike_at(t) {
        let local = f * STRIKE;
        let toward = if left { 1.0 } else { -1.0 };
        if strike.by_attacker == is_attacker {
            let wind = (local / WINDUP).clamp(0.0, 1.0);
            let push = (wind * std::f32::consts::PI).sin();
            offset.x += toward * if strike.ranged { -10.0 * push } else { 60.0 * push };
        } else if local > WINDUP && local < WINDUP + IMPACT {
            let k = (local - WINDUP) / IMPACT;
            flash = (1.0 - k).powi(2);
            offset.x += (t * 70.0).sin() * 6.0 * (1.0 - k);
        }
    }

    // The squad: one figure per two HP (five at most), big units fewer
    let (max_figures, per) = if big(game_data, side.unit_type) { (2, 5.0) } else { (5, 2.0) };
    let shown_hp = (hp / 10.0).ceil();
    let figures = ((shown_hp / per).ceil() as usize).min(max_figures);
    let bar_top = half.bottom() - 46.0;
    let size = (half.height() * if big(game_data, side.unit_type) { 0.4 } else { 0.3 }).min(160.0);
    let flier = matches!(game_data.unit_class(side.unit_type), UnitClass::Air | UnitClass::AirTransport);
    let front_y = if flier { half.top() + half.height() * 0.5 } else { ground_top + (bar_top - ground_top) * 0.78 };
    // Front row of three toward the split, a staggered back row of two behind
    let slots: [(f32, f32); 5] = [(0.0, 0.0), (1.0, 0.0), (0.5, 1.0), (2.0, 0.0), (1.5, 1.0)];
    let texture = unit_textures.battle(side.faction, side.unit_type);
    let frame = ((t * 4.0) as u32) % texture.map_or(1, |(_, f)| f.max(1));
    let mut order: Vec<usize> = (0..figures).collect();
    // Back row drawn first, so the front row stands in front of it
    order.sort_by(|a, b| slots[*b].1.total_cmp(&slots[*a].1));
    for i in order {
        let (col, row) = slots[i];
        let x_from_split = 30.0 + size * 0.5 + col * size * 0.72;
        let x = if left { half.right() - x_from_split } else { half.left() + x_from_split };
        let y = front_y - row * size * 0.3;
        let scale = 1.0 - row * 0.1;
        let rect = egui::Rect::from_center_size(egui::pos2(x, y - size * 0.45), egui::vec2(size, size) * scale).translate(offset);
        match texture {
            Some((texture, frames)) => {
                let w = 1.0 / frames.max(1) as f32;
                let (u0, u1) = (frame as f32 * w, (frame + 1) as f32 * w);
                // Art faces right; the right-hand side faces left
                let uv = if left {
                    egui::Rect::from_min_max(egui::pos2(u0, 0.0), egui::pos2(u1, 1.0))
                } else {
                    egui::Rect::from_min_max(egui::pos2(u1, 0.0), egui::pos2(u0, 1.0))
                };
                painter.image(texture, rect, uv, egui::Color32::WHITE.gamma_multiply(fade));
                if flash > 0.0 {
                    painter.image(texture, rect, uv, egui::Color32::from_white_alpha((flash * 200.0 * fade) as u8));
                }
            }
            None => {
                painter.circle_filled(rect.center(), size * 0.3, theme::faction_color(game_data, side.faction).gamma_multiply(fade));
            }
        }
    }

    // Name and HP along the bottom
    let bar = egui::Rect::from_min_max(egui::pos2(half.left(), half.bottom() - 46.0), half.max);
    let color = theme::faction_color(game_data, side.faction);
    painter.rect_filled(bar, 0, theme::shade(color, 0.55).gamma_multiply(0.92 * fade));
    let (name_x, align) = if left { (bar.left() + 16.0, egui::Align2::LEFT_CENTER) } else { (bar.right() - 16.0, egui::Align2::RIGHT_CENTER) };
    painter.text(egui::pos2(name_x, bar.center().y), align, game_data.unit_name(side.unit_type),
        egui::FontId::new(20.0, theme::display_family()), theme::PARCHMENT.gamma_multiply(fade));
    let hp_x = if left { bar.right() - 16.0 } else { bar.left() + 16.0 };
    let hp_align = if left { egui::Align2::RIGHT_CENTER } else { egui::Align2::LEFT_CENTER };
    painter.text(egui::pos2(hp_x, bar.center().y), hp_align, format!("{}", shown_hp as i32),
        egui::FontId::new(30.0, theme::title_family()), theme::GOLD_LIGHT.gamma_multiply(fade));
    // Ten HP pips beside the number
    let pip_w = 9.0;
    for k in 0..10 {
        let on = (k as f32) < shown_hp;
        let x = if left { hp_x - 40.0 - (9 - k) as f32 * (pip_w + 3.0) } else { hp_x + 40.0 + k as f32 * (pip_w + 3.0) };
        let pip = egui::Rect::from_min_size(egui::pos2(x - pip_w / 2.0, bar.center().y - 7.0), egui::vec2(pip_w, 14.0));
        painter.rect_filled(pip, 2, if on { theme::GOLD_LIGHT.gamma_multiply(fade) } else { egui::Color32::from_black_alpha((90.0 * fade) as u8) });
    }
    let _ = ui;
}

/// Big units (the Colossus, boats, transports) show at most two figures
fn big(game_data: &GameData, unit_type: UnitType) -> bool {
    unit_type == UnitType::Behemoth
        || matches!(game_data.unit_class(unit_type), UnitClass::Naval | UnitClass::NavalTransport | UnitClass::Transport | UnitClass::AirTransport)
}

/// Sky and the ground the unit stands on
fn backdrop(painter: &egui::Painter, half: egui::Rect, ground_top: f32, terrain: Terrain, fade: f32) {
    let c = |r: u8, g: u8, b: u8| egui::Color32::from_rgb(r, g, b).gamma_multiply(fade);
    // Sky in bands, lighter toward the horizon
    let bands = [c(0x9C, 0xCF, 0xE8), c(0xAF, 0xDA, 0xEC), c(0xC4, 0xE4, 0xEE), c(0xD9, 0xEE, 0xEE)];
    let band_h = (ground_top - half.top()) / bands.len() as f32;
    for (i, color) in bands.iter().enumerate() {
        let r = egui::Rect::from_min_max(egui::pos2(half.left(), half.top() + i as f32 * band_h),
            egui::pos2(half.right(), half.top() + (i + 1) as f32 * band_h + 1.0));
        painter.rect_filled(r, 0, *color);
    }
    // Far treeline
    let mut x = half.left() - 10.0;
    let mut k = 0;
    while x < half.right() + 10.0 {
        let h = 26.0 + ((k * 37) % 17) as f32;
        painter.add(egui::Shape::convex_polygon(
            vec![egui::pos2(x, ground_top), egui::pos2(x + 14.0, ground_top - h), egui::pos2(x + 28.0, ground_top)],
            c(0x5E, 0x8C, 0x5A), egui::Stroke::NONE));
        x += 22.0;
        k += 1;
    }
    let ground = egui::Rect::from_min_max(egui::pos2(half.left(), ground_top), half.max);
    let (main, dark) = match terrain {
        Terrain::Pond | Terrain::Creek | Terrain::LilyPads => (c(0x4F, 0x8F, 0xC9), c(0x3D, 0x74, 0xA8)),
        Terrain::Shore => (c(0xE6, 0xD3, 0x9A), c(0xC9, 0xB4, 0x78)),
        Terrain::Boulder | Terrain::Brambles | Terrain::Hollow => (c(0x8E, 0x8A, 0x6E), c(0x6E, 0x6A, 0x52)),
        Terrain::Log => (c(0xC9, 0xA4, 0x6B), c(0xA8, 0x84, 0x52)),
        Terrain::Base | Terrain::Workshop | Terrain::Dock | Terrain::Roost | Terrain::Outpost | Terrain::Storehouse =>
            (c(0xB7, 0xB2, 0xA4), c(0x93, 0x8E, 0x80)),
        _ => (c(0x7C, 0xC0, 0x5A), c(0x62, 0xA2, 0x46)),
    };
    painter.rect_filled(ground, 0, main);
    // A few strokes of texture: ripples, tufts or flagstone seams
    for i in 0..14 {
        let gx = half.left() + ((i * 71) % 100) as f32 / 100.0 * half.width();
        let gy = ground_top + 8.0 + ((i * 43) % 100) as f32 / 100.0 * (ground.height() - 60.0);
        painter.line_segment([egui::pos2(gx, gy), egui::pos2(gx + 18.0, gy)], theme::stroke(3.0, dark));
    }
    // Bushes stand around units in thickets (AW forest)
    if matches!(terrain, Terrain::Thicket | Terrain::TallGrass) {
        for i in 0..5 {
            let bx = half.left() + (i as f32 + 0.3) * half.width() / 5.0;
            painter.circle_filled(egui::pos2(bx, ground_top + 6.0), 22.0, c(0x3F, 0x7A, 0x38));
            painter.circle_filled(egui::pos2(bx + 14.0, ground_top - 2.0), 16.0, c(0x4C, 0x8E, 0x42));
        }
    }
}
