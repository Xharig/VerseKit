// VerseKit — companion for Star Citizen.
// Copyright (C) 2026 Xharig
//
// SPDX-License-Identifier: GPL-3.0-only
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! The pictures of the keys in the house style of the VerseKit profiles, as PNG: the dark card with its two corner
//! brackets, the Lucide symbol of 52 pixels, the name in Lato Bold of 18 pixels in one or two lines. The measures
//! and colours come from `streamdeck-plugin/hausstil.json`, the one source the profile pictures are drawn from too.
//! A state stands as the second line (a word, never the colour alone).

use std::sync::OnceLock;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use resvg::tiny_skia::{self, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};
use serde_json::Value;
use versekit_core::streamdeck::Action;
use versekit_core::theme::Color;

use crate::render::{Look, Tone, icon_body};

const HAUSSTIL: &str = include_str!("../../../streamdeck-plugin/hausstil.json");
const LATO: &[u8] = include_bytes!("../../versekit-ui/fonts/Lato-Bold.ttf");

/// The measures and colours of the house style.
#[derive(Debug)]
pub struct Style {
    pub size: u32,
    pub ground: Color,
    pub card: Color,
    pub card_inset: f32,
    pub card_round: f32,
    pub bracket_width: f32,
    pub bracket_radius: f32,
    pub bracket_leg: f32,
    pub bracket_inset: f32,
    pub symbol: f32,
    pub symbol_middle: f32,
    pub font: f32,
    pub font_min: f32,
    /// The name of a game action key, where it fits.
    pub font_game: f32,
    /// The widest a single line may be, and each of two lines (beside the bottom right bracket).
    pub text_width: f32,
    pub text_width_two: f32,
    pub one_line: f32,
    pub two_lines: [f32; 2],
    pub dimmed: f64,
    /// The name of a game action, as the key pictures of the profiles write it ([`game_lines`]).
    pub game: GameWriting,
    source: Value,
}

/// How the name of a game action is written: one line as large as it fits, else two lines split at a space.
#[derive(Clone, Debug)]
pub struct GameWriting {
    pub one: Vec<f32>,
    pub two: Vec<f32>,
    pub width_one: f32,
    /// The widest the first and the second of two lines may be (the second beside the bottom right bracket).
    pub width_two: [f32; 2],
    pub one_line: f32,
    pub two_lines: [f32; 2],
}

impl Style {
    /// A colour of the style by its name (`gruen`, `tuerkis`, `seite` …); green for an unknown one.
    pub fn colour(&self, name: &str) -> Color {
        self.source["farben"][name].as_str().and_then(Color::parse).unwrap_or(Color { r: 0x9c, g: 0xe4, b: 0x30 })
    }

    /// The colour a key may name in its settings (`farbe`), as the name the house style knows; `None` for another
    /// word. The colours of states (`warnung`, `fehler`, `gedaempft`) and white are no colours of a key.
    pub fn key_colour(&self, name: &str) -> Option<&'static str> {
        KEY_COLOURS.iter().copied().find(|c| *c == name && self.source["farben"][*c].is_string())
    }

    /// The colour of an action: frame and symbol.
    pub fn colour_of(&self, action: Action) -> Color {
        self.colour(self.source["farbe_je_aktion"][action.id()].as_str().unwrap_or("gruen"))
    }

    /// The colour of the writing on a key of `action`: green on a green key, white on any other.
    pub fn writing_of(&self, action: Action) -> Color {
        let colour = self.colour_of(action);
        if colour == self.colour("gruen") { colour } else { self.colour("weiss") }
    }
}

/// The colours of the categories of the profiles a key may take.
pub const KEY_COLOURS: [&str; 10] = ["gruen", "tuerkis", "seite", "rot", "orange", "blau", "violett", "gelb", "pink", "charakter"];

fn number(value: &Value, name: &str) -> f32 {
    value[name].as_f64().unwrap_or_else(|| panic!("hausstil.json: {name}")) as f32
}

/// The house style, read once.
pub fn style() -> &'static Style {
    static STYLE: OnceLock<Style> = OnceLock::new();
    STYLE.get_or_init(|| {
        let v: Value = serde_json::from_str(HAUSSTIL).expect("hausstil.json");
        let colour = |name: &str| v[name].as_str().and_then(Color::parse).unwrap_or_else(|| panic!("hausstil.json: {name}"));
        Style {
            size: v["groesse"].as_u64().unwrap_or(144) as u32,
            ground: colour("grund"),
            card: colour("karte"),
            card_inset: number(&v, "karte_einzug"),
            card_round: number(&v, "karte_rundung"),
            bracket_width: number(&v, "klammer_strich"),
            bracket_radius: number(&v, "klammer_radius"),
            bracket_leg: number(&v, "klammer_schenkel"),
            bracket_inset: number(&v, "klammer_einzug"),
            symbol: number(&v, "symbol_px"),
            symbol_middle: number(&v, "symbol_mitte_y"),
            font: number(&v, "schrift_px"),
            font_min: number(&v, "schrift_px_min"),
            font_game: number(&v, "schrift_px_spielaktion"),
            text_width: number(&v, "schrift_breite_eine"),
            text_width_two: number(&v, "schrift_breite_zwei"),
            one_line: number(&v, "zeile_mitte_eine"),
            two_lines: [v["zeile_mitte_zwei"][0].as_f64().unwrap_or(88.0) as f32, v["zeile_mitte_zwei"][1].as_f64().unwrap_or(109.0) as f32],
            dimmed: v["gedimmt_anteil"].as_f64().unwrap_or(0.45),
            game: {
                let g = &v["spielaktion_schrift"];
                let list = |name: &str| -> Vec<f32> {
                    g[name].as_array().map(|l| l.iter().filter_map(Value::as_f64).map(|n| n as f32).collect()).unwrap_or_default()
                };
                let pair = |name: &str| -> [f32; 2] {
                    let l = list(name);
                    [l.first().copied().unwrap_or_else(|| panic!("hausstil.json: {name}")), l.get(1).copied().unwrap_or(0.0)]
                };
                GameWriting {
                    one: list("eine_px"),
                    two: list("zwei_px"),
                    width_one: number(g, "breite_eine"),
                    width_two: pair("breite_zwei"),
                    one_line: number(g, "zeile_mitte_eine"),
                    two_lines: pair("zeile_mitte_zwei"),
                }
            },
            source: v,
        }
    })
}

fn lato() -> &'static FontRef<'static> {
    static FONT: OnceLock<FontRef<'static>> = OnceLock::new();
    FONT.get_or_init(|| FontRef::try_from_slice(LATO).expect("Lato Bold"))
}

/// The scale of letters of `size` pixels as GDI and CSS mean it (the em square). `ab_glyph` scales by the height
/// from ascent to descent, which in Lato is larger than the em.
fn em(size: f32) -> PxScale {
    let font = lato();
    let per_em = font.units_per_em().unwrap_or(1000.0);
    PxScale::from(size * font.height_unscaled() / per_em)
}

/// The width of a line in Lato Bold at `size` pixels.
pub fn line_width(text: &str, size: f32) -> f32 {
    let font = lato().as_scaled(em(size));
    let mut width = 0.0;
    let mut last = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = last {
            width += font.kern(prev, id);
        }
        width += font.h_advance(id);
        last = Some(id);
    }
    width
}

/// The name in one or two lines and the size that lets it fit: 18 pixels down to 15, one line first, then the
/// split with the most even lines; `None` when it does not fit even so.
pub fn lines_of(label: &str, two_allowed: bool) -> Option<(Vec<String>, f32)> {
    let s = style();
    let words: Vec<&str> = label.split_whitespace().collect();
    let mut size = s.font;
    while size >= s.font_min {
        if line_width(label, size) <= s.text_width {
            return Some((vec![label.to_string()], size));
        }
        let fits = |line: &str| line_width(line, size) <= s.text_width_two;
        if two_allowed {
            let even = (1..words.len())
                .map(|at| (words[..at].join(" "), words[at..].join(" ")))
                .filter(|(a, b)| fits(a) && fits(b))
                .min_by(|x, y| {
                    let gap = |(a, b): &(String, String)| (line_width(a, size) - line_width(b, size)).abs();
                    gap(x).total_cmp(&gap(y))
                });
            if let Some((a, b)) = even {
                return Some((vec![a, b], size));
            }
            if let Some(at) = label.find('-')
                && fits(&label[..=at])
                && fits(&label[at + 1..])
            {
                return Some((vec![label[..=at].to_string(), label[at + 1..].to_string()], size));
            }
        }
        size -= 0.5;
    }
    None
}

/// The name of a game action as the key pictures of the profiles write it: on one line at the largest size of
/// [`GameWriting::one`] it fits, else on two lines split at a space (never inside a word) at the largest size of
/// [`GameWriting::two`] where both fit, the split with the most even number of letters; `None` when it fits neither.
pub fn game_lines(label: &str) -> Option<(Vec<String>, f32)> {
    let g = &style().game;
    if let Some(size) = g.one.iter().copied().find(|&size| line_width(label, size) <= g.width_one) {
        return Some((vec![label.to_string()], size));
    }
    let splits: Vec<(String, String)> =
        label.match_indices(' ').map(|(at, _)| (label[..at].trim().to_string(), label[at + 1..].trim().to_string())).collect();
    for &size in &g.two {
        let best = splits
            .iter()
            .filter(|(a, b)| !a.is_empty() && !b.is_empty())
            .filter(|(a, b)| line_width(a, size) <= g.width_two[0] && line_width(b, size) <= g.width_two[1])
            .min_by_key(|(a, b)| a.chars().count().abs_diff(b.chars().count()));
        if let Some((a, b)) = best {
            return Some((vec![a.clone(), b.clone()], size));
        }
    }
    None
}

/// The size, 18 pixels down to 15, at which `text` fits on one line `width` pixels wide; `None` when it does not.
pub fn size_within(text: &str, width: f32) -> Option<f32> {
    size_from(text, width, style().font)
}

/// As [`size_within`], beginning at `start` pixels.
pub fn size_from(text: &str, width: f32, start: f32) -> Option<f32> {
    let s = style();
    let mut size = start;
    while size >= s.font_min {
        if line_width(text, size) <= width {
            return Some(size);
        }
        size -= 0.5;
    }
    None
}

fn paint(colour: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color_rgba8(colour.r, colour.g, colour.b, 255);
    paint.anti_alias = true;
    paint
}

fn rounded(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    // A quarter circle as a cubic curve: the handles at 0.5523 of the radius.
    let k = 0.552_284_8 * r;
    let mut p = PathBuilder::new();
    p.move_to(x + r, y);
    p.line_to(x + w - r, y);
    p.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    p.line_to(x + w, y + h - r);
    p.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    p.line_to(x + r, y + h);
    p.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    p.line_to(x, y + r);
    p.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    p.close();
    p.finish()
}

/// The two corner brackets: top left and bottom right. Since 26.11.2026 only the small quarter circle without
/// legs (`klammer_schenkel` = `klammer_radius`), with flat ends, as the key pictures of the profiles draw it.
fn brackets(pixmap: &mut Pixmap, colour: Color) {
    let s = style();
    let (r, leg) = (s.bracket_radius, s.bracket_leg);
    let k = 0.552_284_8 * r;
    let low = s.card_inset + s.bracket_inset;
    let high = s.size as f32 - low;
    // Without legs the ends stay flat: round caps would make the small bracket longer than in the profile pictures.
    let cap = if leg > r { LineCap::Round } else { LineCap::Butt };
    let stroke = Stroke { width: s.bracket_width, line_cap: cap, line_join: LineJoin::Round, ..Stroke::default() };
    let mut top = PathBuilder::new();
    top.move_to(low, low + leg);
    top.line_to(low, low + r);
    top.cubic_to(low, low + r - k, low + r - k, low, low + r, low);
    top.line_to(low + leg, low);
    let mut bottom = PathBuilder::new();
    bottom.move_to(high, high - leg);
    bottom.line_to(high, high - r);
    bottom.cubic_to(high, high - r + k, high - r + k, high, high - r, high);
    bottom.line_to(high - leg, high);
    for path in [top.finish(), bottom.finish()].into_iter().flatten() {
        pixmap.stroke_path(&path, &paint(colour), &stroke, Transform::identity(), None);
    }
}

/// The Lucide symbol, `size` pixels wide, its middle at `middle` from the top.
fn symbol(pixmap: &mut Pixmap, icon: &str, colour: Color, size: f32, middle: f32) {
    symbol_at(pixmap, icon, colour, size, style().size as f32 / 2.0, middle);
}

/// The Lucide symbol, `size` pixels wide, its middle at `x`, `middle`.
fn symbol_at(pixmap: &mut Pixmap, icon: &str, colour: Color, size: f32, x: f32, middle: f32) {
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="{}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
        colour.to_hex(),
        icon_body(icon)
    );
    let Ok(tree) = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()) else { return };
    let scale = size / 24.0;
    let left = x - size / 2.0;
    let transform = Transform::from_row(scale, 0.0, 0.0, scale, left, middle - size / 2.0);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
}

/// How much each pixel of a `width` × `height` picture is covered by one line of writing centred on `centre`,
/// `middle` (0 to 1).
fn coverage(text: &str, size: f32, centre: f32, middle: f32, width: u32, height: u32) -> Vec<f32> {
    let font = lato().as_scaled(em(size));
    let mut x = centre - line_width(text, size) / 2.0;
    let baseline = middle + (font.ascent() + font.descent()) / 2.0;
    let mut out = vec![0.0f32; (width * height) as usize];
    let mut last = None;
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = last {
            x += font.kern(prev, id);
        }
        if let Some(outline) = lato().outline_glyph(id.with_scale_and_position(em(size), ab_glyph::point(x, baseline))) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, cover| {
                let (px, py) = (bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32);
                if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                    let at = (py as u32 * width + px as u32) as usize;
                    out[at] = out[at].max(cover);
                }
            });
        }
        x += font.h_advance(id);
        last = Some(id);
    }
    out
}

/// A box blur of `radius` over a field of `width` × `height`, along the rows and then the columns.
fn box_blur(values: &[f32], width: usize, height: usize, radius: usize) -> Vec<f32> {
    let pass = |input: &[f32], along_rows: bool| -> Vec<f32> {
        let mut out = vec![0.0f32; input.len()];
        let (lines, length) = if along_rows { (height, width) } else { (width, height) };
        for line in 0..lines {
            let at = |i: usize| if along_rows { line * width + i } else { i * width + line };
            for i in 0..length {
                let (from, to) = (i.saturating_sub(radius), (i + radius).min(length - 1));
                // At the edges fewer values lie in the box: the mean of those there.
                let sum: f32 = (from..=to).map(|j| input[at(j)]).sum();
                out[at(i)] = sum / (to - from + 1) as f32;
            }
        }
        out
    };
    pass(&pass(values, true), false)
}

/// A soft glow of `colour` under one line of writing: its letters blurred, at most `strength` (0 to 1) strong.
fn glow(pixmap: &mut Pixmap, text: &str, size: f32, centre: f32, middle: f32, colour: Color, strength: f32) {
    let (width, height) = (pixmap.width(), pixmap.height());
    let mut soft = coverage(text, size, centre, middle, width, height);
    // Three box blurs come close to the gaussian blur of radius 4 of the model.
    for _ in 0..3 {
        soft = box_blur(&soft, width as usize, height as usize, 3);
    }
    for (pixel, cover) in pixmap.pixels_mut().iter_mut().zip(soft) {
        let a = (cover * strength).clamp(0.0, 1.0);
        if a <= 0.0 {
            continue;
        }
        let under = pixel.demultiply();
        let mix = |u: u8, c: u8| (f32::from(u) * (1.0 - a) + f32::from(c) * a).round() as u8;
        if let Some(new) =
            tiny_skia::PremultipliedColorU8::from_rgba(mix(under.red(), colour.r), mix(under.green(), colour.g), mix(under.blue(), colour.b), 255)
        {
            *pixel = new;
        }
    }
}

/// One line of writing, centred on `middle`, blended onto the picture.
fn line(pixmap: &mut Pixmap, text: &str, size: f32, middle: f32, colour: Color) {
    line_at(pixmap, text, size, style().size as f32 / 2.0, middle, colour);
}

/// One line of writing, centred on `centre`, `middle`, blended onto the picture.
fn line_at(pixmap: &mut Pixmap, text: &str, size: f32, centre: f32, middle: f32, colour: Color) {
    let font = lato().as_scaled(em(size));
    let width = line_width(text, size);
    let mut x = centre - width / 2.0;
    // The line box (ascent to descent) centred on `middle`, as GDI centres it in design.ps1.
    let baseline = middle + (font.ascent() + font.descent()) / 2.0;
    let mut last = None;
    let full = pixmap.width();
    let height = pixmap.height();
    let data = pixmap.pixels_mut();
    for c in text.chars() {
        let id = font.glyph_id(c);
        if let Some(prev) = last {
            x += font.kern(prev, id);
        }
        let glyph = id.with_scale_and_position(em(size), ab_glyph::point(x, baseline));
        if let Some(outline) = lato().outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, cover| {
                let px = bounds.min.x as i32 + gx as i32;
                let py = bounds.min.y as i32 + gy as i32;
                if px < 0 || py < 0 || px as u32 >= full || py as u32 >= height {
                    return;
                }
                let at = (py as u32 * full + px as u32) as usize;
                let under = data[at].demultiply();
                let mix = |a: u8, b: u8| (f32::from(a) * (1.0 - cover) + f32::from(b) * cover).round() as u8;
                let (r, g, b) = (mix(under.red(), colour.r), mix(under.green(), colour.g), mix(under.blue(), colour.b));
                if let Some(new) = tiny_skia::PremultipliedColorU8::from_rgba(r, g, b, 255) {
                    data[at] = new;
                }
            });
        }
        x += font.h_advance(id);
        last = Some(id);
    }
}

fn mixed(colour: Color, toward: Color, share: f64) -> Color {
    let m = |a: u8, b: u8| (f64::from(a) * share + f64::from(b) * (1.0 - share)).round() as u8;
    Color { r: m(colour.r, toward.r), g: m(colour.g, toward.g), b: m(colour.b, toward.b) }
}

/// The picture of a key of `action` as PNG.
pub fn png(look: &Look, action: Action) -> Vec<u8> {
    let s = style();
    let Some(mut pixmap) = Pixmap::new(s.size, s.size) else { return Vec::new() };
    pixmap.fill(tiny_skia::Color::from_rgba8(s.ground.r, s.ground.g, s.ground.b, 255));
    let side = s.size as f32 - 2.0 * s.card_inset;
    if let Some(card) = rounded(s.card_inset, s.card_inset, side, side, s.card_round) {
        pixmap.fill_path(&card, &paint(s.card), FillRule::Winding, Transform::identity(), None);
    }
    // A key that names its colour (a game action of the profiles) takes it: frame and symbol in it, the writing
    // green on green and white on any other.
    let base = look.colour.map_or_else(|| s.colour_of(action), |c| s.colour(c));
    let writing = if look.colour.is_some() {
        if base == s.colour("gruen") { base } else { s.colour("weiss") }
    } else {
        s.writing_of(action)
    };
    // The name of a game action stands larger where it fits: the short names of the profiles ([`game_lines`] first).
    let name_start = if matches!(action, Action::GameAction | Action::Emote) { s.font_game } else { s.font };
    let muted = s.colour("gedaempft");
    // A key that does nothing yet (the balance) stands dimmed: its colour mixed into the card.
    let dimmed = action == Action::Wallet;
    let frame = if dimmed { mixed(base, s.card, s.dimmed) } else { base };
    brackets(&mut pixmap, frame);
    let icon_colour = match look.icon_tone {
        Tone::Accent => frame,
        Tone::Muted => muted,
        Tone::Warning => s.colour("warnung"),
        Tone::Error => s.colour("fehler"),
    };
    symbol(&mut pixmap, look.icon, icon_colour, s.symbol, s.symbol_middle);
    // A dangerous game action held down: a ring round the symbol fills in the colour of warnings, from the top.
    if let Some(held) = look.hold {
        let middle = (s.size as f32 / 2.0, s.symbol_middle);
        let radius = s.symbol / 2.0 + 8.0;
        let warning = s.colour("warnung");
        arc_around(&mut pixmap, middle, radius, 0.0, 360.0, 5.0, mixed(warning, s.card, 0.25));
        arc_around(&mut pixmap, middle, radius, 270.0, 270.0 + 360.0 * held.clamp(0.0, 1.0), 5.0, warning);
    }
    let name_colour = if dimmed { mixed(writing, s.card, s.dimmed) } else { writing };
    let game = matches!(action, Action::GameAction | Action::Emote);
    let g = &s.game;
    match &look.badge {
        // A game action as the key pictures of the profiles: large, on one line or split at a space.
        None if game && let Some((lines, size)) = game_lines(&look.label) => {
            if lines.len() == 1 {
                line(&mut pixmap, &lines[0], size, g.one_line, name_colour);
            } else {
                line(&mut pixmap, &lines[0], size, g.two_lines[0], name_colour);
                line(&mut pixmap, &lines[1], size, g.two_lines[1], name_colour);
            }
        }
        None => {
            let one = (name_start > s.font && line_width(&look.label, name_start) <= s.text_width)
                .then(|| (vec![look.label.clone()], name_start));
            let (lines, size) =
                one.or_else(|| lines_of(&look.label, true)).unwrap_or_else(|| (vec![look.label.clone()], s.font_min));
            if lines.len() == 1 {
                line(&mut pixmap, &lines[0], size, s.one_line, name_colour);
            } else {
                line(&mut pixmap, &lines[0], size, s.two_lines[0], name_colour);
                line(&mut pixmap, &lines[1], size, s.two_lines[1], name_colour);
            }
        }
        Some(badge) => {
            // The state as the second line: the word in the colour of the key when on, grey when off. Both lines as
            // wide as two lines may be (26.11.: "Hauptfenster" at 18 pixels ran to the edge of the key on the device).
            // The name above a state stands as large as the short names of the game keys, on every key.
            // A game action: the name on one line as large as two lines of the profiles may be, above the state.
            let game_size = game.then(|| g.two.iter().copied().find(|&px| line_width(&look.label, px) <= g.width_two[0])).flatten();
            let size = game_size.or_else(|| size_from(&look.label, s.text_width_two, s.font_game)).unwrap_or(s.font_min);
            // A long name of the game that does not fit on the key even at the smallest size is cut with „…“, not
            // drawn past its edge (the line of the name stands above the band of the lower brackets).
            let label = shortened(&look.label, size, s.size as f32 - 8.0);
            let name_middle = if game_size.is_some() { g.two_lines[0] } else { s.two_lines[0] };
            line(&mut pixmap, &label, size, name_middle, name_colour);
            let state_colour = match badge.tone {
                Tone::Warning => s.colour("warnung"),
                Tone::Error => s.colour("fehler"),
                // On, open, done: green on every key.
                _ if badge.filled => s.colour("gruen"),
                _ => muted,
            };
            let word_size = size_within(&badge.word, s.text_width_two).unwrap_or(s.font_min);
            line(&mut pixmap, &badge.word, word_size, s.two_lines[1], state_colour);
        }
    }
    pixmap.encode_png().unwrap_or_default()
}

/// The field of a dial on the touch strip.
pub const DIAL_WIDTH: u32 = 200;
pub const DIAL_HEIGHT: u32 = 100;
/// The ring: its middle below the field, only the upper half shows; 14 pixels wide (outer radius 76, inner 62).
const RING: (f32, f32, f32, f32) = (100.0, 136.0, 76.0, 62.0);
/// The middles of − and +: the same height, the same distance from the edges.
const MINUS_X: f32 = 26.0;
const PLUS_X: f32 = 174.0;
/// The height of − and + and of the line under the name: with the name, and in the compact form.
const SIGN_Y: [f32; 2] = [40.0, 30.0];
/// The small corner brackets of a dial: a quarter circle of this radius, its ends this far from the edge.
const DIAL_BRACKET: (f32, f32) = (14.0, 5.0);

/// An arc of the ring between two angles (degrees, 180 left, 270 top, 360 right), `width` wide at `radius`.
fn arc(pixmap: &mut Pixmap, radius: f32, from: f32, to: f32, width: f32, colour: Color) {
    arc_around(pixmap, (RING.0, RING.1), radius, from, to, width, colour);
}

/// An arc around `middle` between two angles (degrees, 270 at the top, going round with the clock).
fn arc_around(pixmap: &mut Pixmap, middle: (f32, f32), radius: f32, from: f32, to: f32, width: f32, colour: Color) {
    if to <= from {
        return;
    }
    let (x, y) = middle;
    let mut path = PathBuilder::new();
    let steps = ((to - from) / 2.0).ceil().max(1.0) as u32;
    for i in 0..=steps {
        let a = (from + (to - from) * i as f32 / steps as f32).to_radians();
        let (px, py) = (x + radius * a.cos(), y + radius * a.sin());
        if i == 0 { path.move_to(px, py) } else { path.line_to(px, py) }
    }
    let stroke = Stroke { width, line_cap: LineCap::Butt, ..Stroke::default() };
    if let Some(path) = path.finish() {
        pixmap.stroke_path(&path, &paint(colour), &stroke, Transform::identity(), None);
    }
}

/// A straight stroke from one point to another.
fn stroke_line(pixmap: &mut Pixmap, from: (f32, f32), to: (f32, f32), width: f32, colour: Color) {
    let mut path = PathBuilder::new();
    path.move_to(from.0, from.1);
    path.line_to(to.0, to.1);
    let stroke = Stroke { width, line_cap: LineCap::Butt, ..Stroke::default() };
    if let Some(path) = path.finish() {
        pixmap.stroke_path(&path, &paint(colour), &stroke, Transform::identity(), None);
    }
}

/// The small corner brackets of the house style on the field of a dial: top left and bottom right.
fn dial_brackets(pixmap: &mut Pixmap, colour: Color) {
    let s = style();
    let (r, low) = DIAL_BRACKET;
    let k = 0.552_284_8 * r;
    let (right, bottom) = (DIAL_WIDTH as f32 - low, DIAL_HEIGHT as f32 - low);
    let stroke = Stroke { width: s.bracket_width, line_cap: LineCap::Butt, ..Stroke::default() };
    let mut top = PathBuilder::new();
    top.move_to(low, low + r);
    top.cubic_to(low, low + r - k, low + r - k, low, low + r, low);
    let mut end = PathBuilder::new();
    end.move_to(right, bottom - r);
    end.cubic_to(right, bottom - r + k, right - r + k, bottom, right - r, bottom);
    for path in [top.finish(), end.finish()].into_iter().flatten() {
        pixmap.stroke_path(&path, &paint(colour), &stroke, Transform::identity(), None);
    }
}

/// The size from `start` down to `smallest` at which `text` fits into `width`; `None` when it does not.
fn fitting(text: &str, width: f32, start: f32, smallest: f32) -> Option<f32> {
    let mut size = start;
    while size >= smallest {
        if line_width(text, size) <= width {
            return Some(size);
        }
        size -= 0.5;
    }
    None
}

/// The field of a dial as PNG: with its name (the standard) or compact; − left and + right on one height, the ring
/// below, the value or the symbol between them, nothing made up where no value comes.
pub fn dial_png(look: &crate::render::DialLook) -> Vec<u8> {
    let s = style();
    let Some(mut pixmap) = Pixmap::new(DIAL_WIDTH, DIAL_HEIGHT) else { return Vec::new() };
    pixmap.fill(tiny_skia::Color::from_rgba8(s.ground.r, s.ground.g, s.ground.b, 255));
    let k = s.colour(look.colour);
    let white = s.colour("weiss");
    let motion = look.motion;
    // Nothing darkens or covers the field: a turn lights only its quarter of the ring and its sign.
    if let Some(shape) = rounded(3.0, 3.0, DIAL_WIDTH as f32 - 6.0, DIAL_HEIGHT as f32 - 6.0, 12.0) {
        pixmap.fill_path(&shape, &paint(s.card), FillRule::Winding, Transform::identity(), None);
    }
    // The ring: dimmed without a value or when off, full when on.
    let (_, ring_y, outer, inner) = RING;
    let (middle, width) = ((outer + inner) / 2.0, outer - inner);
    let dark = mixed(k, s.card, 0.30);
    let base = if look.on == Some(true) { k } else { dark };
    arc(&mut pixmap, middle, 180.0, 360.0, width, base);
    // The light colour of the dial: its colour with some white.
    let light = mixed(k, white, 0.65);
    if motion.turn != 0 {
        let lit = mixed(light, base, f64::from(motion.glow));
        if motion.turn > 0 {
            arc(&mut pixmap, middle, 270.0, 270.0 + motion.reach, width, lit);
        } else {
            arc(&mut pixmap, middle, 270.0 - motion.reach, 270.0, width, lit);
        }
    }
    // A dangerous press held down: the ring fills from the left in the colour of warnings.
    if let Some(held) = look.hold {
        arc(&mut pixmap, middle, 180.0, 180.0 + 180.0 * held.clamp(0.0, 1.0), width, s.colour("warnung"));
    }
    let rim = mixed(k, white, 0.45);
    arc(&mut pixmap, outer, 180.0, 360.0, 2.0, rim);
    arc(&mut pixmap, inner, 180.0, 360.0, 2.0, rim);
    // The needle at the top.
    let angle = 270.0_f32.to_radians();
    let from = (RING.0 + (inner - 4.0) * angle.cos(), ring_y + (inner - 4.0) * angle.sin());
    let to = (RING.0 + (outer + 1.0) * angle.cos(), ring_y + (outer + 1.0) * angle.sin());
    stroke_line(&mut pixmap, from, to, 7.0, s.ground);
    stroke_line(&mut pixmap, from, to, 3.0, white);
    // The press: a dot in the hole of the ring.
    if motion.pulse > 0.0 {
        let r = 6.0 + 6.0 * motion.pulse;
        if let Some(dot) = PathBuilder::from_circle(RING.0, 91.0, r) {
            pixmap.fill_path(&dot, &paint(mixed(k, s.card, f64::from(motion.pulse))), FillRule::Winding, Transform::identity(), None);
        }
    }
    // A dial of the power of the ship: the power symbol in the hole of the ring; green when the game says on, white
    // when off, in the colour of the dial while nothing is known.
    if look.energy {
        let colour = match look.on {
            Some(true) => s.colour("gruen"),
            Some(false) => white,
            None => k,
        };
        symbol_at(&mut pixmap, "power", colour, 18.0 + 6.0 * motion.pulse, RING.0, 91.0);
    }
    // Name, then value (with its symbol), warning or symbol; − and + on the height of that line.
    let sign_y = SIGN_Y[usize::from(look.compact)];
    if !look.compact {
        let name = look.label.to_uppercase();
        let size = fitting(&name, 150.0, 24.0, 16.0).unwrap_or(16.0);
        // The name with a soft glow in the colour of the dial below it, as the model.
        glow(&mut pixmap, &name, size, 100.0, 17.0, k, 200.0 / 255.0);
        line_at(&mut pixmap, &name, size, 100.0, 17.0, mixed(white, k, 0.88));
    }
    if let Some(warning) = &look.warning {
        let (start, smallest) = if look.compact { (22.0, 16.0) } else { (20.0, 15.0) };
        let size = fitting(warning, 118.0, start, smallest).unwrap_or(smallest);
        line_at(&mut pixmap, warning, size, 100.0, sign_y, s.colour("warnung"));
    } else if let Some(value) = &look.value {
        // Symbol and value as one group in the middle, at most 104 pixels wide (between − and +).
        let (icon_size, start, smallest, y) = if look.compact { (26.0, 24.0, 14.0, 30.0) } else { (18.0, 20.0, 14.0, 41.0) };
        let size = fitting(value, 104.0 - icon_size - 6.0, start, smallest).unwrap_or(smallest);
        let group = icon_size + 6.0 + line_width(value, size);
        let left = 100.0 - group / 2.0;
        symbol_at(&mut pixmap, look.icon, k, icon_size, left + icon_size / 2.0, y);
        line_at(&mut pixmap, value, size, left + icon_size + 6.0 + line_width(value, size) / 2.0, y + 1.0, white);
    } else {
        let icon_size = if look.compact { 34.0 } else { 20.0 };
        symbol_at(&mut pixmap, look.icon, k, icon_size, 100.0, sign_y);
    }
    dial_brackets(&mut pixmap, k);
    let lit = |side: i8| if motion.turn == side { mixed(light, k, f64::from(motion.glow)) } else { k };
    let half = 10.0;
    stroke_line(&mut pixmap, (MINUS_X - half, sign_y), (MINUS_X + half, sign_y), 5.0, lit(-1));
    stroke_line(&mut pixmap, (PLUS_X - half, sign_y), (PLUS_X + half, sign_y), 5.0, lit(1));
    stroke_line(&mut pixmap, (PLUS_X, sign_y - half), (PLUS_X, sign_y + half), 5.0, lit(1));
    pixmap.encode_png().unwrap_or_default()
}

/// An info dial as PNG: the name above in capitals with its glow, the value large in white, the line below in the
/// colour of its tone; the small corner brackets in the colour of the dial.
pub fn info_png(look: &crate::render::InfoLook) -> Vec<u8> {
    let s = style();
    let Some(mut pixmap) = Pixmap::new(DIAL_WIDTH, DIAL_HEIGHT) else { return Vec::new() };
    pixmap.fill(tiny_skia::Color::from_rgba8(s.ground.r, s.ground.g, s.ground.b, 255));
    if let Some(shape) = rounded(3.0, 3.0, DIAL_WIDTH as f32 - 6.0, DIAL_HEIGHT as f32 - 6.0, 12.0) {
        pixmap.fill_path(&shape, &paint(s.card), FillRule::Winding, Transform::identity(), None);
    }
    let k = s.colour(look.colour);
    let white = s.colour("weiss");
    let name = look.label.to_uppercase();
    let size = fitting(&name, 150.0, 24.0, 16.0).unwrap_or(16.0);
    glow(&mut pixmap, &name, size, 100.0, 17.0, k, 200.0 / 255.0);
    line_at(&mut pixmap, &name, size, 100.0, 17.0, mixed(white, k, 0.88));
    let value_size = fitting(&look.value, 170.0, 32.0, 18.0).unwrap_or(18.0);
    line_at(&mut pixmap, &look.value, value_size, 100.0, 51.0, white);
    let line_colour = match look.tone {
        crate::render::Tone::Accent => k,
        crate::render::Tone::Warning => s.colour("warnung"),
        crate::render::Tone::Error => s.colour("fehler"),
        crate::render::Tone::Muted => s.colour("gedaempft"),
    };
    let line_size = fitting(&look.line, 176.0, 16.0, 12.0).unwrap_or(12.0);
    let text = shortened(&look.line, line_size, 176.0);
    line_at(&mut pixmap, &text, line_size, 100.0, 82.0, line_colour);
    dial_brackets(&mut pixmap, k);
    pixmap.encode_png().unwrap_or_default()
}

/// `text` cut at the end so it fits into `width` at `size`, with „…“ where something was cut.
fn shortened(text: &str, size: f32, width: f32) -> String {
    if line_width(text, size) <= width {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() && line_width(&format!("{}…", chars.iter().collect::<String>()), size) > width {
        chars.pop();
    }
    format!("{}…", chars.iter().collect::<String>().trim_end())
}

/// `text` in at most two lines of `width` at `size`: as many words as fit on the first, the rest on the second,
/// cut with „…“ where it does not fit.
pub fn two_lines(text: &str, size: f32, width: f32) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut first = String::new();
    let mut used = 0;
    for word in &words {
        let next = if first.is_empty() { (*word).to_string() } else { format!("{first} {word}") };
        if line_width(&next, size) > width {
            break;
        }
        first = next;
        used += 1;
    }
    if used == 0 {
        return vec![shortened(text, size, width)];
    }
    let rest = words[used..].join(" ");
    if rest.is_empty() { vec![first] } else { vec![first, shortened(&rest, size, width)] }
}

/// The objective field of a dial as PNG: the head with its symbol, a line, the objective in two lines at most, and
/// below how far the contract is; all in the green of VerseKit.
pub fn objective_png(look: &crate::render::ObjectiveLook) -> Vec<u8> {
    let s = style();
    let Some(mut pixmap) = Pixmap::new(DIAL_WIDTH, DIAL_HEIGHT) else { return Vec::new() };
    pixmap.fill(tiny_skia::Color::from_rgba8(s.ground.r, s.ground.g, s.ground.b, 255));
    if let Some(shape) = rounded(3.0, 3.0, DIAL_WIDTH as f32 - 6.0, DIAL_HEIGHT as f32 - 6.0, 12.0) {
        pixmap.fill_path(&shape, &paint(s.card), FillRule::Winding, Transform::identity(), None);
    }
    let k = s.colour("gruen");
    let white = s.colour("weiss");
    symbol_at(&mut pixmap, "clipboard-list", k, 20.0, 46.0, 17.0);
    let head_size = fitting(&look.head, 120.0, 22.0, 16.0).unwrap_or(16.0);
    line_at(&mut pixmap, &look.head, head_size, 60.0 + line_width(&look.head, head_size) / 2.0, 18.0, k);
    stroke_line(&mut pixmap, (16.0, 31.0), (184.0, 31.0), 2.0, mixed(k, s.card, 0.6));
    let lines = two_lines(&look.text, 16.0, 170.0);
    let tops: &[f32] = if look.stand.is_none() && lines.len() == 1 { &[56.0] } else { &[47.0, 64.0] };
    for (text, middle) in lines.iter().zip(tops) {
        line_at(&mut pixmap, text, 16.0, 100.0, *middle, white);
    }
    if let Some(stand) = &look.stand {
        line_at(&mut pixmap, stand, 18.0, 100.0, 84.0, k);
    }
    dial_brackets(&mut pixmap, k);
    pixmap.encode_png().unwrap_or_default()
}

/// The picture as the data address `setImage` takes.
pub fn data_url(png: &[u8]) -> String {
    use base64::Engine;
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

/// The picture as greys of the same luminance: what someone sees who cannot tell colours.
#[cfg(test)]
pub fn grey(png: &[u8]) -> Vec<u8> {
    let picture = image::load_from_memory(png).expect("png").to_rgba8();
    picture
        .pixels()
        .map(|p| (0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2])).round() as u8)
        .collect()
}

/// The box of the drawn pixels inside `x` × `y` of a key picture scaled to 144: (width, height). A pixel is drawn
/// when its luminance stands well apart from that of the card, whatever its colour, dimmed or not.
#[cfg(test)]
pub fn drawn_box(png: &[u8], x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> (u32, u32) {
    let (w, h, _) = drawn_box_at(png, x, y);
    (w, h)
}

/// Inside the card, clear of both corner brackets: the place of the symbol, and that of the writing.
#[cfg(test)]
pub const SYMBOL_PLACE: (std::ops::Range<u32>, std::ops::Range<u32>) = (40..104, 16..76);
#[cfg(test)]
pub const WRITING_PLACE: (std::ops::Range<u32>, std::ops::Range<u32>) = (14..106, 77..124);

/// As [`drawn_box`], with the top of the box.
#[cfg(test)]
pub fn drawn_box_at(png: &[u8], x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> (u32, u32, u32) {
        let picture = image::load_from_memory(png).expect("png").to_rgba8();
        let picture = image::imageops::resize(&picture, 144, 144, image::imageops::FilterType::Triangle);
        let lum = |p: &image::Rgba<u8>| 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
        let card = lum(picture.get_pixel(72, 125));
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for py in y.clone() {
            for px in x.clone() {
                if (lum(picture.get_pixel(px, py)) - card).abs() > 60.0 {
                    (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px), y1.max(py));
                }
            }
        }
        if x1 < x0 { (0, 0, 0) } else { (x1 - x0 + 1, y1 - y0 + 1, y0) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Badge;

    /// A model picture of the profiles (`streamdeck-plugin/vorlagen`, drawn by the profile repository): what the
    /// plugin must look like.
    fn template(name: &str) -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin/vorlagen").join(name))
            .expect("model picture")
    }

    /// The Anti-AFK key of the profiles, the model of the keys with a state.
    fn model() -> Vec<u8> {
        template("taste-anti-afk.png")
    }

    fn hangar() -> Vec<u8> {
        let look = Look { icon: crate::plugin::page_icon("hf_hangar"), icon_tone: Tone::Accent, label: "Mein Hangar".into(), badge: None, colour: None, hold: None };
        png(&look, Action::Page)
    }

    #[test]
    fn a_page_key_is_drawn_as_the_page_key_of_the_profiles_in_its_own_colour() {
        let (ours, theirs) = (hangar(), template("start-seite-hangar-de.png"));
        let (sw, sh, st) = drawn_box_at(&ours, SYMBOL_X, SYMBOL_Y);
        let (mw, mh, mt) = drawn_box_at(&theirs, SYMBOL_X, SYMBOL_Y);
        assert!(sw.abs_diff(mw) <= 6 && sh.abs_diff(mh) <= 6 && st.abs_diff(mt) <= 3, "symbol {sw}×{sh} at {st}, profile {mw}×{mh} at {mt}");
        let (_, th, tt) = drawn_box_at(&ours, LINE_X, 80..106);
        let (_, mth, mtt) = drawn_box_at(&theirs, LINE_X, 80..106);
        assert!(th.abs_diff(mth) <= 3 && tt.abs_diff(mtt) <= 3, "name {th} high at {tt}, profile {mth} at {mtt}");
        // The bracket in magenta, as the profile; the colour of the page keys, not the green.
        let a = image::load_from_memory(&ours).expect("ours").to_rgba8();
        let b = image::load_from_memory(&theirs).expect("theirs").to_rgba8();
        let (p, q) = (a.get_pixel(BRACKET.0, BRACKET.1), b.get_pixel(BRACKET.0, BRACKET.1));
        assert!((0..3).all(|i| p[i].abs_diff(q[i]) <= 24), "bracket {:?}, profile {:?}", p.0, q.0);
        assert_eq!(style().colour_of(Action::Page).to_hex(), "#e040fb");
        assert_eq!(style().writing_of(Action::Page).to_hex(), "#ffffff", "the name white on a page key");
    }

    fn anti_afk() -> Vec<u8> {
        let look = Look { icon: Action::AntiAfk.icon(), icon_tone: Tone::Accent, label: "Anti-AFK an/aus".into(), badge: None, colour: None, hold: None };
        png(&look, Action::AntiAfk)
    }

    /// The middle of the top left bracket, on the quarter circle (old and new brackets share it).
    const BRACKET: (u32, u32) = (17, 17);

    #[test]
    fn the_brackets_are_the_small_quarter_circles_without_legs() {
        let a = image::load_from_memory(&anti_afk()).expect("ours").to_rgba8();
        let card = style().card;
        let is_card = |x: u32, y: u32| {
            let p = a.get_pixel(x, y);
            p[0].abs_diff(card.r) <= 6 && p[1].abs_diff(card.g) <= 6 && p[2].abs_diff(card.b) <= 6
        };
        assert!(!is_card(BRACKET.0, BRACKET.1), "the quarter circle is drawn");
        // Where the legs of the old brackets ran (x 12, y 34; x 34, y 12; and the same at the bottom right): card.
        for (x, y) in [(12, 34), (34, 12), (132, 110), (110, 132)] {
            assert!(is_card(x, y), "no leg at {x},{y}");
        }
    }

    // Inside the card, clear of both corner brackets.
    const SYMBOL_X: std::ops::Range<u32> = 40..104;
    const SYMBOL_Y: std::ops::Range<u32> = 20..80;
    const LINE_X: std::ops::Range<u32> = 14..130;
    const LINE_Y: std::ops::Range<u32> = 79..98;

    #[test]
    fn the_symbol_and_the_writing_are_as_large_as_on_the_keys_of_the_profile() {
        let (ours, theirs) = (anti_afk(), model());
        let (sw, sh, st) = drawn_box_at(&ours, SYMBOL_X, SYMBOL_Y);
        let (mw, mh, mt) = drawn_box_at(&theirs, SYMBOL_X, SYMBOL_Y);
        assert!(sw.abs_diff(mw) <= 6 && sh.abs_diff(mh) <= 6, "symbol {sw}×{sh}, profile {mw}×{mh}");
        assert!(st.abs_diff(mt) <= 3, "symbol top at {st}, profile at {mt}");
        let (_, _, lt) = drawn_box_at(&ours, LINE_X, LINE_Y);
        let (_, _, mlt) = drawn_box_at(&theirs, LINE_X, LINE_Y);
        assert!(lt.abs_diff(mlt) <= 3, "first line from {lt}, profile from {mlt}");
        assert!(sh >= 44, "a symbol of about 52 pixels: {sh}");
        let (_, th) = drawn_box(&ours, LINE_X, LINE_Y);
        let (_, mth) = drawn_box(&theirs, LINE_X, LINE_Y);
        assert!(th.abs_diff(mth) <= 3, "first line {th} high, profile {mth}");
        assert!(th >= 12, "letters of about 18 pixels: {th}");
        assert_eq!((style().symbol, style().font), (52.0, 18.0), "the measures of the house style");
    }

    #[test]
    fn the_card_brackets_and_colours_are_those_of_the_profile() {
        let (ours, theirs) = (anti_afk(), model());
        let a = image::load_from_memory(&ours).expect("ours").to_rgba8();
        let b = image::load_from_memory(&theirs).expect("theirs").to_rgba8();
        let b = image::imageops::resize(&b, 144, 144, image::imageops::FilterType::Triangle);
        // The ground, the card and the middle of the bracket's quarter circle.
        for (x, y) in [(2, 2), (72, 125), BRACKET] {
            let (p, q) = (a.get_pixel(x, y), b.get_pixel(x, y));
            let gap = (0..3).map(|i| p[i].abs_diff(q[i])).max().unwrap_or(0);
            assert!(gap <= 24, "at {x},{y}: {:?} against the profile {:?}", p.0, q.0);
        }
    }

    #[test]
    fn a_state_stands_as_a_word_on_the_second_line() {
        let off = Look {
            icon: "footprints",
            icon_tone: Tone::Accent,
            label: "Anti-AFK".into(),
            badge: Some(Badge { word: "AUS".into(), filled: false, tone: Tone::Muted }),
            colour: None,
            hold: None,
        };
        let on = Look { badge: Some(Badge { word: "AN".into(), filled: true, tone: Tone::Accent }), ..off.clone() };
        assert_ne!(grey(&png(&off, Action::AntiAfk)), grey(&png(&on, Action::AntiAfk)));
        let (_, second) = drawn_box(&png(&on, Action::AntiAfk), 14..106, 100..120);
        assert!(second >= 10, "the word stands on the second line: {second}");
    }

    /// A game action key of the profiles: short name, own symbol, colour of its category, the binding below.
    fn short_key(label: &str, colour: Option<&'static str>) -> Vec<u8> {
        let look = Look {
            icon: "crosshair",
            icon_tone: Tone::Accent,
            label: label.into(),
            badge: Some(Badge { word: "Alt rechts + T".into(), filled: false, tone: Tone::Muted }),
            colour,
            hold: None,
        };
        png(&look, Action::GameAction)
    }

    #[test]
    fn a_game_action_key_takes_the_colour_it_names_with_white_writing_and_a_larger_name() {
        let s = style();
        let ours = image::load_from_memory(&short_key("Fadenkreuz", Some("orange"))).expect("png").to_rgba8();
        let p = ours.get_pixel(BRACKET.0, BRACKET.1);
        let orange = s.colour("orange");
        assert!(p[0].abs_diff(orange.r) <= 24 && p[1].abs_diff(orange.g) <= 24 && p[2].abs_diff(orange.b) <= 24, "bracket {:?}", p.0);
        assert_ne!(grey(&short_key("Fadenkreuz", Some("orange"))), grey(&short_key("Fadenkreuz", None)), "the colour shows");
        // A short name stands in 20 pixels: wider than it would be in the 18 of the house style.
        let (wide, _) = drawn_box(&short_key("Fadenkreuz", Some("orange")), LINE_X, 76..98);
        assert!(wide as f32 > line_width("Fadenkreuz", s.font) + 2.0, "name {wide} wide");
        // Without a state below it, too.
        let alone =
            Look { icon: "crosshair", icon_tone: Tone::Accent, label: "Fadenkreuz".into(), badge: None, colour: Some("orange"), hold: None };
        let (alone_wide, _) = drawn_box(&png(&alone, Action::GameAction), LINE_X, 80..106);
        assert!(alone_wide as f32 > line_width("Fadenkreuz", s.font) + 2.0, "name alone {alone_wide} wide");
        assert_eq!(s.font_game, 20.0);
        // The state below stays readable: never smaller than the smallest writing of the house style.
        assert!(size_within("Alt rechts + T", s.text_width_two).is_some());
        // Only the colours of the categories are colours of a key.
        assert_eq!(s.key_colour("orange"), Some("orange"));
        for word in ["warnung", "fehler", "weiss", "gedaempft", "lila", ""] {
            assert_eq!(s.key_colour(word), None, "{word}");
        }
        for colour in KEY_COLOURS {
            assert!(s.key_colour(colour).is_some(), "{colour} missing in hausstil.json");
        }
    }

    /// The names of the game action and emote keys of every shipped profile (German and English), and the short
    /// names the plugin shows on a key without a name of its own.
    fn every_short_name() -> Vec<String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../streamdeck-plugin");
        let wanted = [Action::GameAction.uuid(), Action::Emote.uuid()];
        let mut names: Vec<String> = Vec::new();
        let mut add = |name: &str| {
            if !name.trim().is_empty() && !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        };
        for entry in std::fs::read_dir(root.join("profiles")).expect("profiles").flatten() {
            let file = std::fs::read(entry.path()).expect("profile");
            for zipped in versekit_core::zip::entries(&file).expect("zip") {
                if !zipped.name.to_ascii_lowercase().ends_with("/manifest.json") {
                    continue;
                }
                let Ok(page) = serde_json::from_slice::<Value>(&versekit_core::zip::read(&file, &zipped).expect("read")) else { continue };
                for controller in page["Controllers"].as_array().into_iter().flatten() {
                    for action in controller["Actions"].as_object().into_iter().flat_map(|m| m.values()) {
                        if action["UUID"].as_str().is_some_and(|u| wanted.iter().any(|w| w == u))
                            && let Some(name) = action["Settings"]["name"].as_str()
                        {
                            add(name);
                        }
                    }
                }
            }
        }
        let short: Value = serde_json::from_str(&std::fs::read_to_string(root.join("kurznamen.json")).expect("kurznamen")).expect("json");
        // As the plugin shows them: written as the profiles write them.
        for pair in short["aktionen"].as_object().expect("aktionen").values() {
            let langs = [versekit_core::language::Lang::De, versekit_core::language::Lang::En];
            for (name, lang) in pair.as_array().expect("pair").iter().zip(langs) {
                add(&versekit_core::streamdeck::written(name.as_str().expect("name"), lang));
            }
        }
        names
    }

    #[test]
    fn every_short_name_stands_large_as_on_the_profiles_without_a_split_word_or_a_cut() {
        let (s, g) = (style(), &style().game);
        assert!(s.symbol >= 52.0, "symbol {} px", s.symbol);
        let names = every_short_name();
        assert!(names.len() > 300, "{} names", names.len());
        let mut wrong = Vec::new();
        for name in &names {
            let Some((lines, size)) = game_lines(name) else {
                wrong.push(format!("{name}: does not fit (shorten it)"));
                continue;
            };
            if lines.len() > 2 || lines.join(" ") != *name || lines.iter().any(|l| l.ends_with('-')) {
                wrong.push(format!("{name}: split as {lines:?}"));
            }
            let least = if lines.len() == 1 { 22.0 } else { 20.0 };
            if size < least {
                wrong.push(format!("{name}: {size} px, less than {least}"));
            }
            let widths: &[f32] = if lines.len() == 1 { &[g.width_one] } else { &g.width_two };
            for (l, w) in lines.iter().zip(widths) {
                if line_width(l, size) > *w {
                    wrong.push(format!("{name}: line {l:?} wider than {w}"));
                }
            }
            // Drawn in white: no writing outside the card, none in the field of the bottom right bracket.
            let look = Look { icon: "crosshair", icon_tone: Tone::Accent, label: name.clone(), badge: None, colour: Some("rot"), hold: None };
            let picture = image::load_from_memory(&png(&look, Action::GameAction)).expect("png").to_rgba8();
            let picture = image::imageops::resize(&picture, 144, 144, image::imageops::FilterType::Triangle);
            'rows: for y in 60..144 {
                for x in 0..144 {
                    let p = picture.get_pixel(x, y);
                    if p[0] > 200 && p[1] > 200 && p[2] > 200 && (!(12..=132).contains(&x) || (x >= 113 && y >= 113)) {
                        wrong.push(format!("{name}: writing at {x},{y}"));
                        break 'rows;
                    }
                }
            }
        }
        assert!(wrong.is_empty(), "{} of {} names: {wrong:#?}", wrong.len(), names.len());
        // One line as large as it fits, two only at a space.
        assert_eq!(game_lines("Licht"), Some((vec!["Licht".to_string()], 28.0)));
        assert_eq!(game_lines("Munition bündeln").map(|(l, _)| l.len()), Some(2));
        assert_eq!(game_lines("Hauptfensterbankleiste"), None, "one long word is not split");
    }

    /// Writes keys of game actions as the plugin draws them, scaled to the keys of the devices (72 and 96 pixels),
    /// German and English, into the folder `VERSEKIT_RENDER_OUT`, to look at by eye.
    #[test]
    #[ignore = "writes pictures to look at"]
    fn pictures_of_game_keys_to_look_at() {
        use versekit_core::language::Lang;
        let Some(out) = std::env::var_os("VERSEKIT_RENDER_OUT") else { return };
        let out = std::path::Path::new(&out);
        std::fs::create_dir_all(out).expect("folder");
        let actions = [
            ("select_primary_pit", None),
            ("ammoRepool", None),
            ("toggle_lowered", None),
            ("toggle_flashlight", None),
            ("reloadSecondary", None),
            ("v_atc_request", None),
            ("notification_accept", None),
            ("v_lights", Some("sd_ga_nicht_belegt")),
            ("v_toggle_quantum_mode", Some("sd_ga_unsicher")),
        ];
        for lang in [Lang::De, Lang::En] {
            for (action, state) in actions {
                let short = versekit_core::streamdeck::short_name(action, lang).expect("a short name");
                let label = versekit_core::streamdeck::written(short, lang);
                let style = crate::game_style::of(action, "");
                let look = Look {
                    icon: if state.is_some() { "keyboard-off" } else { style.as_ref().map_or("crosshair", |s| s.icon) },
                    icon_tone: if state.is_some() { Tone::Warning } else { Tone::Accent },
                    label,
                    badge: state.map(|key| Badge { word: versekit_core::language::t_in(lang, key), filled: false, tone: Tone::Warning }),
                    colour: style.as_ref().map(|s| s.colour),
                    hold: None,
                };
                let picture = image::load_from_memory(&png(&look, Action::GameAction)).expect("png").to_rgba8();
                for size in [72u32, 96, 144] {
                    let scaled = image::imageops::resize(&picture, size, size, image::imageops::FilterType::Lanczos3);
                    scaled.save(out.join(format!("{}-{action}-{size}.png", lang.code()))).expect("save");
                }
            }
        }
    }

    fn dial(compact: bool) -> crate::render::DialLook {
        crate::render::DialLook {
            label: "Schilde".into(),
            icon: "shield-half",
            colour: "blau",
            value: None,
            on: None,
            warning: None,
            compact,
            energy: false,
            motion: crate::render::Motion::default(),
            hold: None,
        }
    }

    /// The box of the pixels of a dial field that stand apart from its card, inside `x` × `y`: (left, top, width,
    /// height).
    fn dial_box(png: &[u8], x: std::ops::Range<u32>, y: std::ops::Range<u32>) -> (u32, u32, u32, u32) {
        let picture = image::load_from_memory(png).expect("png").to_rgba8();
        assert_eq!(picture.dimensions(), (DIAL_WIDTH, DIAL_HEIGHT));
        let lum = |p: &image::Rgba<u8>| 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
        let card = lum(picture.get_pixel(150, 8));
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for py in y {
            for px in x.clone() {
                if (lum(picture.get_pixel(px, py)) - card).abs() > 40.0 {
                    (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px), y1.max(py));
                }
            }
        }
        if x1 < x0 { (0, 0, 0, 0) } else { (x0, y0, x1 - x0 + 1, y1 - y0 + 1) }
    }

    #[test]
    fn the_minus_and_the_plus_of_a_dial_stand_on_one_height_in_both_forms() {
        for compact in [false, true] {
            let png = dial_png(&dial(compact));
            let (_, mt, mw, mh) = dial_box(&png, 10..46, 22..56);
            let (_, pt, pw, ph) = dial_box(&png, 154..192, 14..60);
            let (minus_mid, plus_mid) = (mt as f32 + mh as f32 / 2.0, pt as f32 + ph as f32 / 2.0);
            assert!((minus_mid - plus_mid).abs() <= 1.0, "compact {compact}: − at {minus_mid}, + at {plus_mid}");
            assert!(mw.abs_diff(pw) <= 1 && mw >= 18, "compact {compact}: − {mw} wide, + {pw}");
            assert!(ph >= 18 && mh <= 7, "compact {compact}: a plus {ph} high, a minus {mh}");
            assert!((plus_mid - SIGN_Y[usize::from(compact)]).abs() <= 1.0, "compact {compact}: at {plus_mid}");
        }
    }

    #[test]
    fn the_standard_dial_has_its_name_above_and_the_compact_one_none_but_a_larger_symbol() {
        let named = dial_png(&dial(false));
        let compact = dial_png(&dial(true));
        let (_, top, width, high) = dial_box(&named, 40..160, 4..26);
        assert!(width >= 60 && high >= 12 && top >= 5, "name {width}×{high} from {top}");
        assert_eq!(dial_box(&compact, 40..160, 4..12).2, 0, "nothing above in the compact form");
        let (_, _, small, _) = dial_box(&named, 80..120, 28..50);
        let (_, _, large, _) = dial_box(&compact, 70..130, 12..50);
        assert!(large > small + 6, "symbol {small} with the name, {large} compact");
        // The name fits at 16 pixels or more: never smaller.
        assert!(fitting("VERTEILUNG", 150.0, 22.0, 16.0).is_some());
    }

    #[test]
    fn a_dial_shows_a_value_or_a_warning_only_where_one_is_given() {
        let plain = dial_png(&dial(false));
        let with_value = dial_png(&crate::render::DialLook { value: Some("216 m/s".into()), ..dial(false) });
        assert_ne!(grey(&plain), grey(&with_value), "a value stands in place of the symbol");
        let (_, _, value_wide, _) = dial_box(&with_value, 50..150, 28..50);
        let (_, _, symbol_wide, _) = dial_box(&plain, 50..150, 28..50);
        assert!(value_wide > symbol_wide, "value {value_wide}, symbol {symbol_wide}");
        let warned = dial_png(&crate::render::DialLook { warning: Some("Nicht belegt".into()), ..dial(false) });
        let picture = image::load_from_memory(&warned).expect("png").to_rgba8();
        let warning = style().colour("warnung");
        let near = |p: &image::Rgba<u8>| p[0].abs_diff(warning.r) <= 40 && p[1].abs_diff(warning.g) <= 40 && p[2].abs_diff(warning.b) <= 60;
        assert!((40..160).any(|x| (28..50).any(|y| near(picture.get_pixel(x, y)))), "the word in the colour of warnings");
        // On lights the whole ring; without a value it stays dimmed.
        let on = dial_png(&crate::render::DialLook { on: Some(true), ..dial(false) });
        assert_ne!(grey(&plain), grey(&on));
    }

    #[test]
    fn a_turn_lights_the_half_it_turned_to() {
        let turned = |turn: i8| {
            let motion = crate::render::turn_frames(turn)[2];
            image::load_from_memory(&dial_png(&crate::render::DialLook { motion, ..dial(false) })).expect("png").to_rgba8()
        };
        let lum = |p: &image::Rgba<u8>| 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
        // On the middle of the ring, 45 degrees right and left of the top.
        let at = |deg: f32| {
            let a = deg.to_radians();
            ((RING.0 + 69.0 * a.cos()) as u32, (RING.1 + 69.0 * a.sin()) as u32)
        };
        let (right, left) = (at(315.0), at(225.0));
        let (r, l) = (turned(1), turned(-1));
        assert!(lum(r.get_pixel(right.0, right.1)) > lum(l.get_pixel(right.0, right.1)) + 20.0, "right lit by a right turn");
        assert!(lum(l.get_pixel(left.0, left.1)) > lum(r.get_pixel(left.0, left.1)) + 20.0, "left lit by a left turn");
        // Nothing else changes: the other quarter, the card, the name and the symbol stand as at rest; no darkening.
        let rest = image::load_from_memory(&dial_png(&dial(false))).expect("png").to_rgba8();
        assert_eq!(r.get_pixel(left.0, left.1), rest.get_pixel(left.0, left.1), "the other quarter as at rest");
        for (x, y) in [(150, 8), (60, 60), (100, 17), (100, 40), (8, 90)] {
            assert_eq!(r.get_pixel(x, y), rest.get_pixel(x, y), "at {x},{y}");
        }
        // The sign of the turned side lights up with it, the other stays.
        let (_, _, minus_wide, _) = dial_box(&dial_png(&dial(false)), 10..46, 30..50);
        assert!(minus_wide >= 18);
        let sign = |img: &image::RgbaImage, x: u32| lum(img.get_pixel(x, SIGN_Y[0] as u32));
        assert!(sign(&r, PLUS_X as u32 - 6) > sign(&rest, PLUS_X as u32 - 6) + 20.0, "+ lit by a right turn");
        assert_eq!(sign(&r, MINUS_X as u32), sign(&rest, MINUS_X as u32), "− stays");
        // 300 ms lit, then at rest.
        assert_eq!(crate::render::turn_frames(1).len(), 3);
        assert!(crate::render::turn_frames(-1).iter().all(|m| m.turn == -1 && m.glow == 1.0 && m.reach == 90.0));
        assert_eq!(crate::render::press_frames().len(), 4);
    }

    #[test]
    fn the_name_of_a_dial_has_a_soft_glow_in_its_colour() {
        let picture = image::load_from_memory(&dial_png(&dial(false))).expect("png").to_rgba8();
        let card = style().card;
        let blue = style().colour("blau");
        // Above and beside the letters of SCHILDE, on the card: tinted toward the blue of the dial.
        for (x, y) in [(100, 5), (60, 17), (140, 17)] {
            let p = picture.get_pixel(x, y);
            let toward = |c: u8, k: u8, own: u8| (i32::from(own) - i32::from(c)) * (i32::from(k) - i32::from(c)) > 0;
            assert!(
                toward(card.b, blue.b, p[2]) && toward(card.g, blue.g, p[1]),
                "at {x},{y}: {:?}, card {:?}",
                p.0,
                (card.r, card.g, card.b)
            );
        }
        // Far from the name the card stays plain.
        let far = picture.get_pixel(60, 60);
        assert_eq!((far[0], far[1], far[2]), (card.r, card.g, card.b));
        let mut blurred = box_blur(&[0.0, 0.0, 9.0, 0.0, 0.0], 5, 1, 1);
        assert_eq!(blurred[2], 3.0, "a box of three");
        blurred = box_blur(&blurred, 5, 1, 1);
        assert!(blurred[0] > 0.0, "it spreads further with each pass");
    }

    #[test]
    fn an_energy_dial_has_the_power_symbol_in_the_hole_of_its_ring() {
        let plain = dial_png(&dial(false));
        let energy = dial_png(&crate::render::DialLook { energy: true, ..dial(false) });
        let (_, _, w, h) = dial_box(&energy, 86..114, 80..100);
        assert!(w >= 12 && h >= 12, "a symbol of about 18 pixels: {w}×{h}");
        assert_eq!(dial_box(&plain, 90..110, 84..98).2, 0, "nothing there without");
        let on = dial_png(&crate::render::DialLook { energy: true, on: Some(true), ..dial(false) });
        let green = style().colour("gruen");
        let picture = image::load_from_memory(&on).expect("png").to_rgba8();
        let near = |p: &image::Rgba<u8>| p[0].abs_diff(green.r) <= 30 && p[1].abs_diff(green.g) <= 30 && p[2].abs_diff(green.b) <= 30;
        assert!((88..112).any(|x| (80..100).any(|y| near(picture.get_pixel(x, y)))), "green when the game says on");
    }

    #[test]
    fn the_objective_field_shows_head_objective_and_how_far() {
        let look = crate::render::ObjectiveLook {
            head: "AUFTRAG".into(),
            text: "Fracht abholen bei Everus Harbour".into(),
            stand: Some("2 von 3".into()),
        };
        let png = objective_png(&look);
        let (left, _, head_wide, _) = dial_box(&png, 30..196, 6..28);
        assert!(left <= 40 && head_wide >= 100, "symbol and head from {left}, {head_wide} wide");
        let (_, _, rule_wide, _) = dial_box(&png, 10..190, 30..33);
        assert!(rule_wide >= 160, "the line under the head: {rule_wide}");
        let (_, top, _, high) = dial_box(&png, 14..186, 38..74);
        assert!(top <= 42 && high >= 26, "two lines of the objective: {high} high from {top}");
        let (_, _, stand_wide, _) = dial_box(&png, 40..160, 76..94);
        assert!(stand_wide >= 40, "how far: {stand_wide}");
        let fits = two_lines("Fracht abholen bei Everus Harbour", 16.0, 170.0);
        assert_eq!(fits.len(), 2);
        assert_eq!(fits.join(" "), "Fracht abholen bei Everus Harbour", "nothing cut where it fits");
        let long = two_lines("Collect the cargo from the drop point at Everus Harbour and bring it home", 16.0, 170.0);
        assert_eq!(long.len(), 2);
        assert!(long[1].ends_with('…') && long.iter().all(|l| line_width(l, 16.0) <= 170.0), "{long:?}");
        let empty = crate::render::ObjectiveLook { head: "AUFTRAG".into(), text: "Kein laufender Auftrag".into(), stand: None };
        assert_eq!(dial_box(&objective_png(&empty), 40..160, 76..94).2, 0, "nothing made up below");
    }

    #[test]
    fn an_info_dial_has_its_name_above_a_large_value_and_a_line_in_its_tone() {
        let look = crate::render::InfoLook {
            label: "Contested".into(),
            colour: "seite",
            value: "12:05".into(),
            line: "OFFEN".into(),
            tone: Tone::Accent,
        };
        let png = info_png(&look);
        let (_, _, name_wide, _) = dial_box(&png, 20..180, 6..28);
        assert!(name_wide >= 80, "the name: {name_wide}");
        let (_, top, value_wide, high) = dial_box(&png, 20..180, 32..70);
        assert!(value_wide >= 60 && high >= 20 && top <= 42, "the value: {value_wide} wide, {high} high from {top}");
        let (_, _, line_wide, _) = dial_box(&png, 20..180, 74..92);
        assert!(line_wide >= 30, "the line: {line_wide}");
        let empty = info_png(&crate::render::InfoLook { value: String::new(), line: String::new(), ..look.clone() });
        assert_eq!(dial_box(&empty, 20..180, 32..92).2, 0, "nothing drawn without value and line");
        let tone_of = |tone: Tone| image::load_from_memory(&info_png(&crate::render::InfoLook { tone, ..look.clone() })).expect("png").to_rgba8();
        assert_ne!(tone_of(Tone::Accent), tone_of(Tone::Warning), "the line takes the colour of its tone");
        let long = "Log fehlt · 3 Tasten fehlen";
        assert!(fitting(long, 176.0, 16.0, 12.0).is_some(), "the longest line fits without cutting");
    }

    #[test]
    fn the_name_of_a_game_key_with_a_state_is_at_least_as_large_as_that_of_any_key() {
        let with_state = |action: Action, word: &str| {
            let look = Look {
                icon: "layers",
                icon_tone: Tone::Accent,
                label: "Overlay".into(),
                badge: Some(Badge { word: word.into(), filled: false, tone: Tone::Muted }),
                colour: None,
                hold: None,
            };
            png(&look, action)
        };
        let (_, state_high, _) = drawn_box_at(&with_state(Action::Overlay, "AUS"), LINE_X, 76..98);
        let (_, game_high, _) = drawn_box_at(&with_state(Action::GameAction, "AUS"), LINE_X, 76..98);
        // A game key writes its name as the profiles do, larger than the other keys of the plugin.
        assert!(game_high > state_high, "Overlay above AUS {state_high} high, on a game key {game_high}");
        let (w, _) = drawn_box(&with_state(Action::Overlay, "AUS"), LINE_X, 76..98);
        assert!(w as f32 > line_width("Overlay", style().font) + 2.0, "wider than at 18 pixels: {w}");
    }

    #[test]
    fn a_name_above_a_state_stays_inside_the_width_of_two_lines() {
        let s = style();
        let size = size_within("Hauptfenster", s.text_width_two).expect("fits");
        assert!(line_width("Hauptfenster", size) <= s.text_width_two, "at {size}");
        assert!(size < s.font, "Hauptfenster needs less than 18 pixels to stay inside 104: {size}");
        assert_eq!(size_within("Overlay", s.text_width_two), Some(s.font));
        let look = Look {
            icon: "layout-dashboard",
            icon_tone: Tone::Accent,
            label: "Hauptfenster".into(),
            badge: Some(Badge { word: "HINTEN".into(), filled: false, tone: Tone::Warning }),
            colour: None,
            hold: None,
        };
        // Nothing of the name left of x 20 or right of x 124 (the width of two lines, centred).
        let (w, _) = drawn_box(&png(&look, Action::Main), 14..130, 80..98);
        assert!(w <= 106, "the name is {w} pixels wide");
        // A long name of the game on a key without a short name: cut with „…“, inside the key (x 4 to 140).
        let long = Look { label: "Bergbaulaser wechseln zwischen den Modi".into(), ..look.clone() };
        let (w, _) = drawn_box(&png(&long, Action::GameAction), 0..144, 80..98);
        assert!(w <= 136, "the long name is {w} pixels wide");
        assert!(shortened("Bergbaulaser wechseln zwischen den Modi", s.font_min, 136.0).ends_with('…'));
        // A name that fits on the key at the smallest size stays whole.
        assert_eq!(shortened("Schilde umschalten", s.font_min, 136.0), "Schilde umschalten");
    }

    #[test]
    fn a_dangerous_key_held_down_shows_its_ring_filling_in_the_colour_of_warnings() {
        let key = |hold: Option<f32>| {
            let look = Look {
                icon: "door-open",
                icon_tone: Tone::Accent,
                label: "Aussteigen".into(),
                badge: Some(Badge { word: "HALTEN".into(), filled: false, tone: Tone::Muted }),
                colour: Some("gelb"),
                hold,
            };
            image::load_from_memory(&png(&look, Action::GameAction)).expect("png").to_rgba8()
        };
        let s = style();
        let warning = s.colour("warnung");
        let near = |p: &image::Rgba<u8>| p[0].abs_diff(warning.r) <= 30 && p[1].abs_diff(warning.g) <= 30 && p[2].abs_diff(warning.b) <= 40;
        let radius = s.symbol / 2.0 + 8.0;
        let at = |deg: f32| {
            let a = deg.to_radians();
            ((72.0 + radius * a.cos()) as u32, (s.symbol_middle + radius * a.sin()) as u32)
        };
        // A quarter held: the ring is filled right of the top, not yet below.
        let quarter = key(Some(0.25));
        let (fx, fy) = at(315.0);
        let (ex, ey) = at(90.0);
        assert!(near(quarter.get_pixel(fx, fy)), "filled at the top right");
        assert!(!near(quarter.get_pixel(ex, ey)), "not yet at the bottom");
        assert!(near(key(Some(1.0)).get_pixel(ex, ey)), "full after the second");
        assert!(!near(key(None).get_pixel(fx, fy)), "no ring while nothing is held");
        // The dial: its ring fills from the left.
        let dial_held = |hold| dial_png(&crate::render::DialLook { hold, ..dial(false) });
        assert_ne!(grey(&dial_held(Some(0.5))), grey(&dial_held(None)));
    }

    #[test]
    fn a_name_too_long_for_two_lines_is_said_not_cut() {
        assert!(lines_of(&"W".repeat(60), true).is_none());
        let (one, size) = lines_of("Overlay", true).expect("fits");
        assert_eq!((one.len(), size), (1, 18.0));
    }
}
