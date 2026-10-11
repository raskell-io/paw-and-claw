//! Pixel-art UI icons, drawn in code like the HP badges so they match the
//! sprites (the UI font has few symbol glyphs). Each icon is a 9x9 mask:
//! `#` takes the icon's colour, `+` a light tint of it, and every filled pixel
//! gets a 1px dark outline so icons read on parchment and over the board.

use bevy_egui::egui;

use super::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Heart,
    Shield,
    Sword,
    Swords,
    Boot,
    Eye,
    Target,
    Arrow,
    Flask,
    Flag,
    Coin,
    Star,
    Hourglass,
    Undo,
    Quill,
    Banner,
    Die,
    Door,
    Paw,
    Scroll,
    Crown,
    Gear,
    Book,
    OpenBook,
    Gamepad,
    Cloud,
    Plus,
    LoadIn,
    UnloadOut,
    Crate,
    Sun,
    Rain,
    Snow,
    Wind,
    Bell,
    Note,
}

impl Icon {
    fn mask(self) -> [&'static str; 9] {
        match self {
            Icon::Heart => [".........", ".##...##.", "#+##.####", "#########", "#########", ".#######.", "..#####..", "...###...", "....#...."],
            Icon::Shield => [".#######.", "#########", "##+######", "##+######", "##+######", ".#######.", ".#######.", "..#####..", "...###..."],
            Icon::Sword => ["....#....", "...#+#...", "...#+#...", "...#+#...", "...#+#...", ".#######.", "....#....", "....#....", "...###..."],
            Icon::Swords => ["#.......#", ".#.....#.", "..#...#..", "...#.#...", "....#....", "...#.#...", "#.#...#.#", ".#.....#.", "#.#...#.#"],
            Icon::Boot => ["...####..", "...####..", "...####..", "...####..", "...#####.", "..#######", "..#######", "..++++++.", "........."],
            Icon::Eye => [".........", "..#####..", ".#+++++#.", "#++###++#", "#++#.#++#", "#++###++#", ".#+++++#.", "..#####..", "........."],
            Icon::Target => ["....#....", "..#####..", ".#..#..#.", ".#..#..#.", "#########", ".#..#..#.", ".#..#..#.", "..#####..", "....#...."],
            Icon::Arrow => [".....####", "......###", ".....#.##", "....#...#", "...#.....", "+.#......", ".#.......", "#+.......", "++......."],
            Icon::Flask => ["...###...", "....#....", "...###...", "..#####..", ".#######.", ".#+#####.", ".#+#####.", ".#######.", "..#####.."],
            Icon::Flag => [".#.......", ".####....", ".######..", ".#+######", ".######..", ".####....", ".#.......", ".#.......", "###......"],
            Icon::Coin => ["..#####..", ".#+++++#.", "#++###++#", "#+#+++++#", "#+#+++++#", "#+#+++++#", "#++###++#", ".#+++++#.", "..#####.."],
            Icon::Star => ["....#....", "....#....", "...###...", "#########", ".#######.", "..#####..", "..##.##..", ".##...##.", ".#.....#."],
            Icon::Hourglass => ["#########", ".#+++++#.", "..#+++#..", "...#+#...", "....#....", "...#.#...", "..#.+.#..", ".#+++++#.", "#########"],
            Icon::Undo => ["..#......", ".##......", "#######..", ".##....#.", "..#.....#", "........#", ".......#.", "....###..", "........."],
            Icon::Quill => [".......##", "......#+#", ".....#+#.", "....#+#..", "...#+#...", "..###....", "..##.....", ".#.......", "#........"],
            Icon::Banner => ["#########", "#########", "#+#####+#", "#########", "#########", "####.####", "###...###", "##.....##", "#.......#"],
            Icon::Die => ["#########", "#+++++++#", "#+#+++#+#", "#+++++++#", "#+++#+++#", "#+++++++#", "#+#+++#+#", "#+++++++#", "#########"],
            Icon::Door => ["..#####..", ".#+++++#.", "#+++++++#", "#+++++++#", "#++++#++#", "#+++++++#", "#+++++++#", "#+++++++#", "#########"],
            Icon::Paw => ["..##.##..", "..##.##..", "##.....##", "##.###.##", "..#####..", ".#######.", ".#######.", "..#####..", "........."],
            Icon::Scroll => [".#######.", "#+#####+#", ".#+++++#.", ".#+###+#.", ".#+++++#.", ".#+###+#.", ".#+++++#.", "#+#####+#", ".#######."],
            Icon::Crown => [".........", "#...#...#", "##.###.##", "#########", "#+##+##+#", "#########", "#########", ".........", "........."],
            Icon::Gear => ["...#.#...", ".#.###.#.", "..#####..", "####.####", "###...###", "####.####", "..#####..", ".#.###.#.", "...#.#..."],
            Icon::Book => [".........", "########.", "#+++++##.", "#+####+#.", "#+++++##.", "#+####+#.", "#+++++##.", "########.", "........."],
            Icon::OpenBook => [".........", "###...###", "#++#.#++#", "#++#.#++#", "#++#.#++#", "#++#.#++#", "####.####", "...###...", "........."],
            Icon::Gamepad => [".........", ".#######.", "##.###+##", "#...#+.+#", "##.###+##", "#########", "###...###", "##.....##", "........."],
            Icon::Cloud => [".........", "...###...", "..#####..", ".#+#####.", "#########", "#########", ".........", ".........", "........."],
            Icon::Plus => ["...###...", "...###...", "...###...", "#########", "####+####", "#########", "...###...", "...###...", "...###..."],
            Icon::LoadIn => ["...###...", "...###...", ".#######.", "..#####..", "...###...", "#...#...#", "#.......#", "#########", "........."],
            Icon::UnloadOut => ["....#....", "...###...", "..#####..", "...###...", "...###...", "#.......#", "#.......#", "#########", "........."],
            Icon::Crate => ["#########", "#+#####+#", "##+###+##", "###+#+###", "####+####", "###+#+###", "##+###+##", "#+#####+#", "#########"],
            Icon::Sun => ["....#....", ".#.....#.", "...###...", "..##+##..", "#.#+###.#", "..#####..", "...###...", ".#.....#.", "....#...."],
            Icon::Rain => ["..####...", ".##+###..", "#########", "#########", ".........", ".#..#..#.", ".#..#..#.", "#..#..#..", "........."],
            Icon::Snow => ["....#....", ".#..#..#.", "..#.#.#..", "...###...", "####+####", "...###...", "..#.#.#..", ".#..#..#.", "....#...."],
            Icon::Bell => ["....#....", "...###...", "..##+##..", "..#+###..", "..#####..", ".#######.", "#########", ".........", "...###..."],
            Icon::Note => ["....####.", "....#++#.", "....#..#.", "....#..#.", "....#..#.", ".###...#.", "#+##..###", "####.#+##", ".##..####"],
            Icon::Wind => [".........", "######...", ".......#.", "..######.", ".........", "#####....", "......#..", ".######..", "........."],
        }
    }

    /// The colour an icon usually takes
    pub fn color(self) -> egui::Color32 {
        match self {
            Icon::Heart => theme::BERRY,
            Icon::Coin | Icon::Star | Icon::Crown | Icon::Sun => theme::GOLD,
            Icon::Eye | Icon::Rain | Icon::Snow | Icon::Cloud => theme::SKY,
            Icon::Flag | Icon::Banner => theme::LEAF,
            Icon::Sword | Icon::Swords | Icon::Shield | Icon::Gear | Icon::Target => egui::Color32::from_rgb(0x8C, 0x94, 0x9E),
            _ => theme::WOOD,
        }
    }
}

/// Paint an icon into `rect` (squared to fit) in `color`
pub fn paint(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    let mask = icon.mask();
    let cell = (rect.width().min(rect.height()) / 11.0).floor().max(1.0);
    let size = cell * 11.0;
    let origin = rect.center() - egui::vec2(size, size) / 2.0 + egui::vec2(cell, cell);
    let filled = |x: i32, y: i32| (0..9).contains(&x) && (0..9).contains(&y) && mask[y as usize].as_bytes()[x as usize] != b'.';
    let at = |x: i32, y: i32| egui::Rect::from_min_size(origin + egui::vec2(x as f32 * cell, y as f32 * cell), egui::vec2(cell, cell));
    let outline = theme::shade(color, 0.35);
    // Outline first: every empty pixel next to a filled one
    for y in -1..10 {
        for x in -1..10 {
            if !filled(x, y) && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| filled(x + dx, y + dy)) {
                painter.rect_filled(at(x, y), 0.0, outline);
            }
        }
    }
    let light = color.lerp_to_gamma(egui::Color32::WHITE, 0.55);
    for (y, row) in mask.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            let fill = match c {
                b'#' => color,
                b'+' => light,
                _ => continue,
            };
            painter.rect_filled(at(x as i32, y as i32), 0.0, fill);
        }
    }
}

/// An icon as a widget of `size` points, in its usual colour
pub fn icon(ui: &mut egui::Ui, icon: Icon, size: f32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint(ui.painter(), rect, icon, icon.color());
    response
}

/// The icon for a kind of weather
pub fn weather(kind: crate::game::WeatherType) -> Icon {
    use crate::game::WeatherType;
    match kind {
        WeatherType::Clear => Icon::Sun,
        WeatherType::Rain => Icon::Rain,
        WeatherType::Snow => Icon::Snow,
        WeatherType::Sandstorm => Icon::Wind,
        WeatherType::Fog => Icon::Cloud,
    }
}
