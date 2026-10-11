//! The opening sequence: disclaimers, the raskell.io studio stamp, the
//! animated intro showing every army and commander, and the title slamming in
//! with "Press any button". Any button skips ahead; the main menu
//! (`menu::draw_main_menu`) takes over at `BootStage::Menu`.

use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::theme;
use super::PortraitTextures;
use crate::game::{CommanderId, Faction, GameData};
use crate::states::{BootSequence, BootStage};

/// The two armies the intro features, like Advance Wars' Orange Star vs Blue
/// Moon: the first faces right, the second (mirrored) faces left
const ARMIES: [Faction; 2] = [Faction::Western, Faction::Northern];

const DISCLAIMER_SECS: f32 = 4.2;
const STUDIO_SECS: f32 = 4.8;
/// The intro song's final hit (tools/audio/music.py `intro_song`): the title slams in on it
const INTRO_SECS: f32 = 30.0;
/// Beats of the intro song (168 BPM)
const BEAT: f32 = 60.0 / 168.0;

/// The brand art handed to egui (tools/sprites/brand.py)
#[derive(Resource, Default)]
pub struct BrandTextures {
    handles: Vec<Handle<Image>>,
    ink: Option<egui::TextureId>,
    hand: Option<egui::TextureId>,
    washi: Option<egui::TextureId>,
}

pub fn load_brand_textures(mut commands: Commands, asset_server: Res<AssetServer>) {
    let handles = ["sprites/brand/stamp_ink.png", "sprites/brand/stamp_hand.png", "sprites/brand/washi.png"]
        .iter()
        .map(|path| asset_server.load(*path))
        .collect();
    commands.insert_resource(BrandTextures { handles, ..default() });
}

pub fn register_brand_textures(mut contexts: EguiContexts, mut brand: ResMut<BrandTextures>) {
    if brand.ink.is_some() || brand.handles.len() < 3 {
        return;
    }
    let ids: Vec<_> = brand.handles.iter()
        .map(|h| contexts.add_image(bevy_egui::EguiTextureHandle::Strong(h.clone())))
        .collect();
    brand.ink = Some(ids[0]);
    brand.hand = Some(ids[1]);
    brand.washi = Some(ids[2]);
}

/// Any key, mouse button, touch or gamepad button pressed this frame
fn any_button(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>, touches: &Touches, gamepads: &Query<&Gamepad>) -> bool {
    keys.get_just_pressed().next().is_some()
        || mouse.get_just_pressed().next().is_some()
        || touches.any_just_pressed()
        || gamepads.iter().any(|g| g.get_just_pressed().next().is_some())
}

/// The sequence's input and clock
#[derive(bevy::ecs::system::SystemParam)]
pub struct BootInput<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    mouse: Res<'w, ButtonInput<MouseButton>>,
    touches: Res<'w, Touches>,
    gamepads: Query<'w, 's, &'static Gamepad>,
    time: Res<'w, Time>,
}

pub fn draw_boot(
    mut contexts: EguiContexts,
    mut boot: ResMut<BootSequence>,
    input: BootInput,
    brand: Res<BrandTextures>,
    portraits: Res<PortraitTextures>,
    game_data: Res<GameData>,
) {
    if boot.stage == BootStage::Menu {
        return;
    }
    let before = boot.elapsed;
    boot.elapsed += input.time.delta_secs();
    let (t, prev) = (boot.elapsed, before);
    let pressed = any_button(&input.keys, &input.mouse, &input.touches, &input.gamepads);
    let Ok(ctx) = contexts.ctx_mut() else { return };
    ctx.request_repaint();

    // Advance on time, or skip ahead on any button
    match boot.stage {
        BootStage::Disclaimer if t > DISCLAIMER_SECS || (pressed && t > 0.3) => boot.go(BootStage::Studio),
        BootStage::Studio if t > STUDIO_SECS || (pressed && t > 0.3) => boot.go(BootStage::Intro),
        BootStage::Intro if t > INTRO_SECS || (pressed && t > 0.3) => {
            boot.skipped_intro = t <= INTRO_SECS;
            boot.go(BootStage::Title);
        }
        BootStage::Title if pressed && t > 0.8 => {
            theme::queue_sound(ctx, "menu_confirm");
            boot.go(BootStage::Menu);
            return;
        }
        _ => {}
    }
    let stage_changed = boot.elapsed == 0.0;
    let (t, prev) = if stage_changed { (0.0, -1.0) } else { (t, prev) };

    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
        let screen = ui.max_rect();
        let painter = ui.painter().clone();
        match boot.stage {
            BootStage::Disclaimer => disclaimer(&painter, screen, t),
            BootStage::Studio => studio(ui.ctx(), &painter, screen, t, prev, &brand),
            BootStage::Intro => intro(ui.ctx(), &painter, screen, t, prev, &portraits, &game_data),
            BootStage::Title => title(ui.ctx(), &painter, screen, t, prev),
            BootStage::Menu => {}
        }
    });
}

/// Whether the clock passed `at` this frame
fn crossed(prev: f32, now: f32, at: f32) -> bool {
    prev < at && now >= at
}

fn fade(t: f32, total: f32, edge: f32) -> f32 {
    (t / edge).min((total - t) / edge).clamp(0.0, 1.0)
}

fn ease_out(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

/// Overshoot then settle, for bouncy pops
fn ease_back(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let c = 1.70158;
    1.0 + (c + 1.0) * (x - 1.0).powi(3) + c * (x - 1.0).powi(2)
}

fn text(painter: &egui::Painter, pos: egui::Pos2, s: &str, size: f32, family: egui::FontFamily, color: egui::Color32) {
    painter.text(pos, egui::Align2::CENTER_CENTER, s, egui::FontId::new(size, family), color);
}

fn shadowed(painter: &egui::Painter, pos: egui::Pos2, s: &str, size: f32, family: egui::FontFamily, color: egui::Color32) {
    let off = size / 20.0 + 1.0;
    text(painter, pos + egui::vec2(off, off), s, size, family.clone(), egui::Color32::from_black_alpha((color.a() as f32 * 0.6) as u8));
    text(painter, pos, s, size, family, color);
}

// --- Disclaimers ---------------------------------------------------------------

fn disclaimer(painter: &egui::Painter, screen: egui::Rect, t: f32) {
    painter.rect_filled(screen, 0.0, egui::Color32::from_rgb(0x14, 0x1A, 0x15));
    let a = fade(t, DISCLAIMER_SECS, 0.45);
    let ink = theme::PARCHMENT.gamma_multiply(a);
    let soft = theme::PARCHMENT.gamma_multiply(a * 0.7);
    let c = screen.center();
    text(painter, c + egui::vec2(0.0, -96.0), "PAW & CLAW", 30.0, theme::title_family(), theme::GOLD_LIGHT.gamma_multiply(a));
    let lines = [
        "An unofficial fan project, made with love for Advance Wars.",
        "Not affiliated with or endorsed by Nintendo or Intelligent Systems.",
        "All characters, art and music are original.",
    ];
    for (i, line) in lines.iter().enumerate() {
        text(painter, c + egui::vec2(0.0, -30.0 + i as f32 * 30.0), line, 19.0, egui::FontFamily::Proportional, ink);
    }
    text(painter, c + egui::vec2(0.0, 92.0), "Best enjoyed with sound on.", 16.0, egui::FontFamily::Proportional, soft);
}

// --- Studio: the raskell.io stamp -------------------------------------------------

/// Timing of the stamp, in seconds into the studio stage
const HAND_IN: f32 = 0.35;
const WIND_UP: f32 = 0.95;
const SLAM: f32 = 1.35;
const IMPACT: f32 = 1.47;
const LIFT: f32 = 1.95;
const LETTERS: f32 = 2.5;

fn studio(ctx: &egui::Context, painter: &egui::Painter, screen: egui::Rect, t: f32, prev: f32, brand: &BrandTextures) {
    if crossed(prev, t, HAND_IN) {
        theme::queue_sound(ctx, "whoosh");
    }
    if crossed(prev, t, SLAM) {
        theme::queue_sound(ctx, "whoosh");
    }
    if crossed(prev, t, IMPACT) {
        theme::queue_sound(ctx, "stamp");
    }
    // Screen shake after the impact
    let shake = if t > IMPACT && t < IMPACT + 0.3 {
        let k = 1.0 - (t - IMPACT) / 0.3;
        egui::vec2((t * 90.0).sin() * 9.0 * k, (t * 71.0).cos() * 7.0 * k)
    } else {
        egui::Vec2::ZERO
    };

    // Washi paper, tiled
    let a = fade(t, STUDIO_SECS, 0.35);
    painter.rect_filled(screen, 0.0, egui::Color32::BLACK);
    let paper = egui::Color32::WHITE.gamma_multiply(a);
    match brand.washi {
        Some(tex) => {
            let tile = 256.0;
            let mut y = screen.top() - tile + shake.y;
            while y < screen.bottom() + tile {
                let mut x = screen.left() - tile + shake.x;
                while x < screen.right() + tile {
                    painter.image(tex, egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(tile, tile)),
                        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), paper);
                    x += tile;
                }
                y += tile;
            }
        }
        None => {
            painter.rect_filled(screen, 0.0, egui::Color32::from_rgb(246, 242, 232).gamma_multiply(a));
        }
    }

    // Everything scales with the screen height so the glove stays in view and
    // the text fits on a phone. The ink matches the stamp's rubber face (116 px of art).
    let art_scale = screen.height() * 0.0022;
    let seal = 116.0 * art_scale;
    let center = egui::pos2(screen.center().x, screen.top() + screen.height() * 0.55) + shake;

    // The ink, left on the paper from the impact on, with a few splats
    if t >= IMPACT {
        let pop = ease_back((t - IMPACT) / 0.18).max(0.0);
        let size = seal * (0.92 + 0.08 * pop);
        let rect = egui::Rect::from_center_size(center, egui::vec2(size, size));
        if let Some(ink) = brand.ink {
            painter.image(ink, rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE.gamma_multiply(a));
        }
        let splat = egui::Color32::from_rgb(196, 32, 38).gamma_multiply(a);
        for k in 0..9 {
            let ang = k as f32 * 2.39 + 0.4;
            let dist = seal * (0.56 + 0.08 * ((k * 7) % 5) as f32 / 4.0) * ease_out((t - IMPACT) / 0.12);
            let r = 3.0 + ((k * 13) % 5) as f32;
            painter.circle_filled(center + egui::vec2(ang.cos(), ang.sin()) * dist, r, splat);
        }
    }

    // The hand: swoops in, winds up (squash), slams, holds, lifts away
    if (HAND_IN..LIFT + 0.5).contains(&t) {
        let hover = center + egui::vec2(0.0, -screen.height() * 0.15);
        let (pos, sx, sy, angle) = if t < WIND_UP {
            let k = ease_out((t - HAND_IN) / (WIND_UP - HAND_IN));
            let from = egui::pos2(screen.right() + 160.0, screen.top() - 260.0);
            (from.lerp(hover, k), 1.0, 1.0, (1.0 - k) * 0.6)
        } else if t < SLAM {
            // Anticipation: rise and stretch up
            let k = ((t - WIND_UP) / (SLAM - WIND_UP)).clamp(0.0, 1.0);
            (hover + egui::vec2(0.0, -40.0 * ease_out(k)), 1.0 - 0.08 * k, 1.0 + 0.12 * k, -0.08 * k)
        } else if t < IMPACT {
            let k = ((t - SLAM) / (IMPACT - SLAM)).clamp(0.0, 1.0);
            let top = hover + egui::vec2(0.0, -40.0);
            let down = center;
            (top.lerp(down, k * k), 0.92, 1.12, -0.08 * (1.0 - k))
        } else if t < LIFT {
            // Squashed onto the paper, wobbling
            let k = (t - IMPACT) / (LIFT - IMPACT);
            let wobble = (k * 18.0).sin() * (1.0 - k) * 0.06;
            (center, 1.14 - 0.14 * k + wobble, 0.86 + 0.14 * k - wobble, 0.0)
        } else {
            let k = ease_out((t - LIFT) / 0.5);
            let from = center;
            (from + egui::vec2(120.0 * k, -(screen.height()) * k), 1.0, 1.0, 0.5 * k)
        };
        if let Some(hand) = brand.hand {
            // Sized so the stamp block covers the seal; `pos` is the stamp face's
            // centre, near the bottom of the art (y 0.9), and squash pivots there
            let size = egui::vec2(180.0 * art_scale * sx, 300.0 * art_scale * sy);
            let rect = egui::Rect::from_min_size(pos + egui::vec2(-size.x / 2.0, -size.y * 0.9), size);
            egui::Image::new((hand, size))
                .rotate(angle, egui::Vec2::splat(0.5))
                .tint(egui::Color32::WHITE.gamma_multiply(a))
                .paint_at(&egui::Ui::new(ctx.clone(), egui::Id::new("boot_hand"), egui::UiBuilder::new().max_rect(screen)), rect);
        }
    }

    // "produced by" then "raskell.io" popping in letter by letter
    if t >= LETTERS - 0.3 {
        let fa = (((t - (LETTERS - 0.3)) / 0.3).clamp(0.0, 1.0)) * a;
        text(painter, center + egui::vec2(0.0, seal * 0.62), "produced by", 18.0, egui::FontFamily::Proportional,
            egui::Color32::from_rgb(90, 74, 64).gamma_multiply(fa));
        let word = "raskell.io";
        let size = 46.0;
        let spacing = size * 0.52;
        let start_x = center.x - spacing * (word.len() as f32 - 1.0) / 2.0;
        for (i, ch) in word.chars().enumerate() {
            let at = LETTERS + i as f32 * 0.07;
            if crossed(prev, t, at) {
                theme::queue_sound(ctx, "letter");
            }
            if t < at {
                continue;
            }
            let k = ease_back((t - at) / 0.22);
            let pos = egui::pos2(start_x + i as f32 * spacing, center.y + seal * 0.62 + 44.0 - 18.0 * (1.0 - k));
            text(painter, pos, &ch.to_string(), size * (0.4 + 0.6 * k), theme::title_family(),
                egui::Color32::from_rgb(40, 30, 34).gamma_multiply(a));
        }
    }
}

// --- Intro: the armies and their commanders ----------------------------------------

fn intro(ctx: &egui::Context, painter: &egui::Painter, screen: egui::Rect, t: f32, _prev: f32,
         portraits: &PortraitTextures, game_data: &GameData) {
    painter.rect_filled(screen, 0.0, egui::Color32::from_rgb(10, 12, 14));
    let count_in = 4.0 * BEAT;
    let riff_end = count_in + 16.0 * BEAT;
    let verse_end = riff_end + 32.0 * BEAT;

    speed_lines(painter, screen, t, if t < count_in { t / count_in } else { 1.0 });

    if t < count_in {
        // Count-in: speed lines build to a white flash
        let flash = ((t - (count_in - 0.12)) / 0.12).clamp(0.0, 1.0);
        painter.rect_filled(screen, 0.0, egui::Color32::from_white_alpha((flash * 230.0) as u8));
        return;
    }
    if t < riff_end {
        // Each army's banner slams across on its colour
        let per = (riff_end - count_in) / ARMIES.len() as f32;
        let i = (((t - count_in) / per) as usize).min(ARMIES.len() - 1);
        let local = t - count_in - i as f32 * per;
        army_banner(painter, screen, ARMIES[i], local, per, game_data);
        flash(painter, screen, local);
        return;
    }
    if t < verse_end {
        // A bar per army: its commanders rush across a slanted band
        let per = (verse_end - riff_end) / ARMIES.len() as f32;
        let i = (((t - riff_end) / per) as usize).min(ARMIES.len() - 1);
        let local = t - riff_end - i as f32 * per;
        commander_parade(painter, screen, ARMIES[i], local, per, portraits, game_data);
        flash(painter, screen, local);
        return;
    }
    // Chorus: quick cuts of every commander, alternating sides, then the face-off
    let chorus_t = t - verse_end;
    let hero_secs = 7.0;
    if chorus_t < hero_secs {
        let order = hero_order(game_data);
        let per = hero_secs / order.len().max(1) as f32;
        let i = ((chorus_t / per) as usize).min(order.len() - 1);
        let local = chorus_t - i as f32 * per;
        let (army, id) = order[i];
        hero_shot(painter, screen, army, id, local, per, portraits, game_data);
        flash(painter, screen, local);
    } else {
        face_off(painter, screen, chorus_t - hero_secs, portraits, game_data);
    }
    let _ = ctx;
}

fn speed_lines(painter: &egui::Painter, screen: egui::Rect, t: f32, strength: f32) {
    let c = screen.center();
    for k in 0..40 {
        let ang = k as f32 * 0.157 + (k * 37 % 11) as f32 * 0.05;
        let phase = (t * (1.5 + (k % 5) as f32 * 0.4) + k as f32 * 0.31).fract();
        let r0 = screen.width() * (0.25 + phase * 0.6);
        let r1 = r0 + screen.width() * 0.12 * strength;
        let dir = egui::vec2(ang.cos(), ang.sin());
        let alpha = (strength * 70.0 * (1.0 - phase)) as u8;
        painter.line_segment([c + dir * r0, c + dir * r1], egui::Stroke::new(2.0_f32, egui::Color32::from_white_alpha(alpha)));
    }
}

/// A short white flash at the start of each cut
fn flash(painter: &egui::Painter, screen: egui::Rect, local: f32) {
    let a = (1.0 - local / 0.1).clamp(0.0, 1.0);
    if a > 0.0 {
        painter.rect_filled(screen, 0.0, egui::Color32::from_white_alpha((a * 160.0) as u8));
    }
}

fn slanted_band(painter: &egui::Painter, center: egui::Pos2, width: f32, height: f32, slant: f32, color: egui::Color32) {
    let h = height / 2.0;
    let w = width / 2.0;
    painter.add(egui::Shape::convex_polygon(vec![
        center + egui::vec2(-w + slant, -h), center + egui::vec2(w + slant, -h),
        center + egui::vec2(w - slant, h), center + egui::vec2(-w - slant, h),
    ], color, egui::Stroke::NONE));
}

fn army_banner(painter: &egui::Painter, screen: egui::Rect, army: Faction, t: f32, per: f32, game_data: &GameData) {
    let color = theme::faction_color(game_data, army);
    let slide = (1.0 - ease_out(t / 0.25)) * screen.width() - ease_out((t - (per - 0.2)) / 0.2) * screen.width();
    let c = screen.center() + egui::vec2(slide, 0.0);
    slanted_band(painter, c, screen.width() * 1.4, 190.0, 60.0, theme::shade(color, 0.85));
    slanted_band(painter, c + egui::vec2(0.0, -82.0), screen.width() * 1.4, 10.0, 60.0, egui::Color32::from_white_alpha(70));
    let data = game_data.faction(army);
    shadowed(painter, c + egui::vec2(0.0, -14.0), &data.name.to_uppercase(), 64.0, theme::title_family(), theme::PARCHMENT);
    text(painter, c + egui::vec2(0.0, 48.0), &data.animals.join("  ·  "), 20.0, theme::bold_family(), theme::shade(theme::PARCHMENT, 0.85));
}

fn portrait(painter: &egui::Painter, rect: egui::Rect, id: CommanderId, color: egui::Color32, portraits: &PortraitTextures,
            game_data: &GameData, alpha: f32, facing_left: bool) {
    painter.rect_filled(rect.expand(4.0), 6.0, theme::shade(color, 0.4).gamma_multiply(alpha));
    match portraits.get(id) {
        Some(tex) => {
            painter.image(tex, rect, theme::portrait_uv(facing_left), egui::Color32::WHITE.gamma_multiply(alpha));
        }
        None => {
            painter.rect_filled(rect, 4.0, theme::PARCHMENT_DEEP.gamma_multiply(alpha));
            let name = &game_data.commander(id).name;
            text(painter, rect.center(), &name.chars().next().unwrap_or('?').to_string(), rect.height() * 0.5,
                theme::title_family(), theme::WOOD_DARK.gamma_multiply(alpha));
        }
    }
}

/// A commander's full-body art standing with its feet at `feet`, `height`
/// points tall (art is 80x144, facing right; `facing_left` mirrors it), with a
/// soft shadow. Returns false when there's no full-body art for them yet.
fn figure(painter: &egui::Painter, feet: egui::Pos2, height: f32, id: CommanderId, portraits: &PortraitTextures,
          facing_left: bool, alpha: f32) -> bool {
    let Some(tex) = portraits.figure(id) else { return false };
    let size = egui::vec2(height * 80.0 / 144.0, height);
    let rect = egui::Rect::from_min_size(feet - egui::vec2(size.x / 2.0, size.y), size);
    // Shadow: a dark copy nudged down-right, so the figure pops off the band
    painter.image(tex, rect.translate(egui::vec2(height * 0.025, height * 0.02)), theme::portrait_uv(facing_left),
        egui::Color32::from_black_alpha((110.0 * alpha) as u8));
    painter.image(tex, rect, theme::portrait_uv(facing_left), egui::Color32::WHITE.gamma_multiply(alpha));
    true
}

fn commander_parade(painter: &egui::Painter, screen: egui::Rect, army: Faction, t: f32, per: f32,
                    portraits: &PortraitTextures, game_data: &GameData) {
    let color = theme::faction_color(game_data, army);
    let c = screen.center();
    slanted_band(painter, c, screen.width() * 1.4, screen.height() * 0.52, 90.0, theme::shade(color, 0.8));
    let cos = CommanderId::for_faction(army, game_data);
    let size = (screen.height() * 0.34).min(screen.width() / (cos.len() as f32 + 1.0) * 1.15);
    // The row drifts across while each portrait pops in a beat after the last
    let drift = (t / per - 0.5) * -80.0;
    let total = cos.len() as f32 * size * 1.08;
    for (k, &id) in cos.iter().enumerate() {
        let at = k as f32 * (per * 0.6 / cos.len() as f32);
        if t < at {
            continue;
        }
        let pop = ease_back((t - at) / 0.18);
        let x = c.x - total / 2.0 + (k as f32 + 0.5) * size * 1.08 + drift;
        let y = c.y - 10.0 + if k % 2 == 0 { -10.0 } else { 10.0 };
        // Full-body art stands on the band's lower edge, popping up from it
        let feet = egui::pos2(x, c.y + screen.height() * 0.26 - 4.0);
        if !figure(painter, feet, screen.height() * 0.5 * pop.max(0.05), id, portraits, army == ARMIES[1], 1.0) {
            let rect = egui::Rect::from_center_size(egui::pos2(x, y), egui::vec2(size, size) * pop.max(0.05));
            portrait(painter, rect, id, color, portraits, game_data, 1.0, army == ARMIES[1]);
        }
    }
    shadowed(painter, egui::pos2(c.x, c.y + screen.height() * 0.26 + 26.0), &game_data.faction_name(army).to_uppercase(),
        34.0, theme::title_family(), theme::PARCHMENT);
}

/// One commander filling the screen: the first army's on the left facing
/// right with its colour sweeping in from the left, the second army's mirrored
fn hero_shot(painter: &egui::Painter, screen: egui::Rect, army: Faction, id: CommanderId, t: f32, per: f32,
             portraits: &PortraitTextures, game_data: &GameData) {
    let color = theme::faction_color(game_data, army);
    let co = game_data.commander(id);
    let second = army == ARMIES[1];
    let side = if second { 1.0 } else { -1.0 };
    let sweep = ease_out(t / 0.15);
    let c = screen.center();
    slanted_band(painter, c + egui::vec2(side * (1.0 - sweep) * -screen.width(), 0.0), screen.width() * 1.5, screen.height() * 1.2,
        side * 260.0, theme::shade(color, 0.75));
    let size = screen.height() * 0.62;
    let push = (t / per) * 30.0;
    let grow = 0.9 + 0.1 * ease_back(t / 0.2);
    let x = c.x + side * (screen.width() * 0.18 + push);
    // The full figure towers over the frame, as in Advance Wars' CO art
    if !figure(painter, egui::pos2(x, screen.bottom() + screen.height() * 0.04), screen.height() * 0.95 * grow, id, portraits, second, 1.0) {
        let rect = egui::Rect::from_center_size(egui::pos2(x, c.y), egui::vec2(size, size) * grow);
        portrait(painter, rect, id, color, portraits, game_data, 1.0, second);
    }
    let name_x = c.x - side * (screen.width() * 0.2 + push);
    shadowed(painter, egui::pos2(name_x, c.y - 30.0), &co.name.to_uppercase(), 76.0, theme::title_family(), theme::PARCHMENT);
    text(painter, egui::pos2(name_x, c.y + 34.0), game_data.faction_name(army), 22.0, theme::bold_family(), theme::shade(theme::PARCHMENT, 0.85));
}

/// Every commander of both armies, alternating sides, as quick cuts
fn hero_order(game_data: &GameData) -> Vec<(Faction, CommanderId)> {
    let first = CommanderId::for_faction(ARMIES[0], game_data);
    let second = CommanderId::for_faction(ARMIES[1], game_data);
    let mut order = Vec::new();
    for k in 0..first.len().max(second.len()) {
        if let Some(&id) = first.get(k) {
            order.push((ARMIES[0], id));
        }
        if let Some(&id) = second.get(k) {
            order.push((ARMIES[1], id));
        }
    }
    order
}

/// The finale: both armies' lead commanders face off across a diagonal split
/// with "VS" slamming in between, then a white-out into the title
fn face_off(painter: &egui::Painter, screen: egui::Rect, t: f32, portraits: &PortraitTextures, game_data: &GameData) {
    let c = screen.center();
    let w = screen.width();
    for (k, &army) in ARMIES.iter().enumerate() {
        let color = theme::faction_color(game_data, army);
        let second = k == 1;
        let side = if second { 1.0 } else { -1.0 };
        let enter = ease_out((t - 0.15 * k as f32) / 0.3);
        let off = side * (1.0 - enter) * w;
        // One diagonal through the middle, leaning like a slash
        let (top_mid, bottom_mid) = (c.x + 70.0 + off, c.x - 70.0 + off);
        let poly = if second {
            vec![egui::pos2(top_mid, screen.top()), egui::pos2(screen.right() + off, screen.top()),
                 egui::pos2(screen.right() + off, screen.bottom()), egui::pos2(bottom_mid, screen.bottom())]
        } else {
            vec![egui::pos2(screen.left() + off, screen.top()), egui::pos2(top_mid, screen.top()),
                 egui::pos2(bottom_mid, screen.bottom()), egui::pos2(screen.left() + off, screen.bottom())]
        };
        painter.add(egui::Shape::convex_polygon(poly, theme::shade(color, 0.7), egui::Stroke::NONE));
        let Some(&id) = CommanderId::for_faction(army, game_data).first() else { continue };
        let size = screen.height() * 0.6;
        let x = c.x + side * w * 0.24 + off;
        let rect = egui::Rect::from_center_size(egui::pos2(x, c.y - 10.0), egui::vec2(size, size));
        let name_y = if figure(painter, egui::pos2(x, screen.bottom() - screen.height() * 0.1), screen.height() * 0.8, id, portraits, second, 1.0) {
            screen.bottom() - screen.height() * 0.05
        } else {
            portrait(painter, rect, id, color, portraits, game_data, 1.0, second);
            rect.bottom() + 34.0
        };
        shadowed(painter, egui::pos2(x, name_y), &game_data.commander(id).name.to_uppercase(), 40.0, theme::title_family(), theme::PARCHMENT);
    }
    let vs = ease_back((t - 0.55) / 0.2);
    if t > 0.55 {
        shadowed(painter, c, "VS", 70.0 + 50.0 * (1.0 - vs).max(0.0), theme::title_family(), theme::GOLD_LIGHT);
    }
    // White out into the title
    let white = ((t - 3.6) / 0.6).clamp(0.0, 1.0);
    painter.rect_filled(screen, 0.0, egui::Color32::from_white_alpha((white * 255.0) as u8));
}

// --- Title -------------------------------------------------------------------

/// The title slams in with a snap and a flash, then "Press any button" blinks.
/// It sits exactly where the main menu draws it, so the menu takes over seamlessly.
fn title(ctx: &egui::Context, painter: &egui::Painter, screen: egui::Rect, t: f32, prev: f32) {
    if crossed(prev, t, 0.0) || (prev < 0.0 && t >= 0.0) {
        theme::queue_sound(ctx, "snap");
    }
    let shake = if t < 0.3 {
        let k = 1.0 - t / 0.3;
        egui::vec2((t * 80.0).sin() * 8.0 * k, (t * 63.0).cos() * 6.0 * k)
    } else {
        egui::Vec2::ZERO
    };
    theme::forest_frame(painter, screen.translate(shake), ctx.input(|i| i.time) as f32);
    let pos = egui::pos2(screen.center().x, screen.top() + screen.height() * 0.2) + shake;
    let slam = ease_back(t / 0.18);
    let size = 84.0 * (2.2 - 1.2 * slam).max(0.6);
    shadowed(painter, pos, "Paw & Claw", size, theme::title_family(), theme::PARCHMENT);
    let sub = ((t - 0.4) / 0.4).clamp(0.0, 1.0);
    text(painter, pos + egui::vec2(0.0, 58.0), "Tactics of the Woodland Realms", 22.0, theme::display_family(),
        theme::GOLD_LIGHT.gamma_multiply(sub));
    if t > 1.0 {
        let blink = 0.55 + 0.45 * ((t - 1.0) * 3.2).cos().abs();
        shadowed(painter, egui::pos2(screen.center().x, screen.top() + screen.height() * 0.68), "Press any button", 28.0,
            theme::display_family(), theme::PARCHMENT.gamma_multiply(blink));
    }
    // The white from the intro fades out under the title
    let white = (1.0 - t / 0.35).clamp(0.0, 1.0);
    painter.rect_filled(screen, 0.0, egui::Color32::from_white_alpha((white * 255.0) as u8));
}
