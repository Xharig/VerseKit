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

//! Small numbers read by the shape of their digits, where the text recognition of Windows fails on them: the count
//! in the box of a tile of the freight elevator (`x3`, dark on light) and the quality in its field of the line of
//! goods (`588`, light on teal), both about ten pixels high.
//!
//! The writing of a part is measured as ink ([`Ink::of`]), cut into its characters along the columns ([`cells`]),
//! each drawn anew on a small grid ([`CELL_W`] × [`CELL_H`]) and compared with the pattern of every character of its
//! [`Font`] (taken from pictures of the game, [`FONTS`]). ⚠ A character is only taken where its pattern fits by at
//! least [`MIN_LIKENESS`] and no other pattern comes within [`MIN_LEAD`] of it; one character not taken leaves the
//! whole number unread, and the scan asks.

use crate::refinery_scan::Area;
use crate::win_ocr::Bitmap;

/// The columns of the grid a character is drawn on.
pub const CELL_W: usize = 8;
/// The rows of that grid.
pub const CELL_H: usize = 12;

/// How well the pattern of a character must fit at least (correlation, -1 to 1).
pub const MIN_LIKENESS: f64 = 0.80;
/// How much better the best pattern must fit than the next best one of another character.
pub const MIN_LEAD: f64 = 0.12;

/// How much the writing must stand out from its ground at least, in steps of brightness.
pub const MIN_CONTRAST: i32 = 18;

/// The writing of a part as ink: per pixel, row by row, how far it lies from the ground (0) to the writing (1).
#[derive(Clone, Debug, PartialEq)]
pub struct Ink {
    pub width: usize,
    pub height: usize,
    pub ink: Vec<f64>,
}

/// The brightness of a pixel, 0 to 255, as the rest of the scan weighs it.
fn brightness(bitmap: &Bitmap, x: i64, y: i64) -> i32 {
    let at = ((y * bitmap.width + x) * 4) as usize;
    let (b, g, r) = (i32::from(bitmap.raw[at]), i32::from(bitmap.raw[at + 1]), i32::from(bitmap.raw[at + 2]));
    (2 * r + 5 * g + b) / 8
}

impl Ink {
    /// The ink of `area` (cut to the picture): writing `dark` on a light ground (the count box) or light on a dark
    /// one. The ground is the middle brightness of the part, the writing the brightness of its strongest pixels;
    /// `None` where they lie less than [`MIN_CONTRAST`] apart.
    pub fn of(bitmap: &Bitmap, area: Area, dark: bool) -> Option<Ink> {
        let (x0, y0) = (area.0.max(0), area.1.max(0));
        let (x1, y1) = ((area.0 + area.2).min(bitmap.width), (area.1 + area.3).min(bitmap.height));
        if x1 - x0 < 3 || y1 - y0 < 3 {
            return None;
        }
        let value = |x: i64, y: i64| {
            let v = brightness(bitmap, x, y);
            if dark { 255 - v } else { v }
        };
        let mut values: Vec<i32> = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| value(x, y)).collect();
        let raw = values.clone();
        values.sort_unstable();
        // Writing is light after the turn: the ground lies at the middle (darker ground around a field below it), the
        // writing at the top.
        let ground = values[values.len() / 2];
        let writing = values[values.len() - 1 - values.len() / 40];
        if writing - ground < MIN_CONTRAST {
            return None;
        }
        let span = f64::from(writing - ground);
        Some(Ink {
            width: (x1 - x0) as usize,
            height: (y1 - y0) as usize,
            ink: raw.into_iter().map(|v| (f64::from(v - ground) / span).clamp(0.0, 1.0)).collect(),
        })
    }

    fn at(&self, x: usize, y: usize) -> f64 {
        self.ink[y * self.width + x]
    }

    /// The ink at a point between pixels, smoothly between the four around it; none outside.
    fn between(&self, fx: f64, fy: f64) -> f64 {
        let (x, y) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x, fy - y);
        let get = |x: f64, y: f64| {
            if x < 0.0 || y < 0.0 || x >= self.width as f64 || y >= self.height as f64 {
                0.0
            } else {
                self.at(x as usize, y as usize)
            }
        };
        let top = get(x, y) * (1.0 - tx) + get(x + 1.0, y) * tx;
        let bottom = get(x, y + 1.0) * (1.0 - tx) + get(x + 1.0, y + 1.0) * tx;
        top * (1.0 - ty) + bottom * ty
    }
}

/// A font of the game as far as the scan knows it: the patterns of its characters, how much each cell of the grid
/// tells the characters apart (its weight, 0 to 1: the outline all digits share tells little, the gap at the top
/// right of a `6` much), and how wide a character stands beside the next one (its advance), in heights of a digit.
#[derive(Clone, Debug)]
pub struct Font {
    pub name: String,
    pub advance: f64,
    pub weights: [f64; CELL_W * CELL_H],
    pub patterns: Vec<(char, [f64; CELL_W * CELL_H])>,
}

/// One character of the writing: where its cell lies (left, top, width, height, in pixels of the ink) and the cell
/// drawn on the grid.
#[derive(Clone, Debug)]
pub struct Cell {
    pub place: (f64, f64, f64, f64),
    pub grid: [f64; CELL_W * CELL_H],
}

/// The share of a row's most ink a row needs to belong to the band of the writing.
const ROW_SHARE: f64 = 0.2;

/// How much ink a column needs to belong to a character.
const COLUMN_INK: f64 = 0.6;

/// The least ink of a character, in squares of the height of the band: the narrow `1` has about a fifth.
const RUN_INK: f64 = 0.08;

/// The characters of the writing in `ink`: the band of rows the writing fills (the longest run of rows with ink),
/// in it the runs of columns with ink, a run wider than one and a half characters cut into as many as fit
/// (`advance` in heights of the band), each drawn on the grid as a cell `advance` wide around its middle.
pub fn cells(ink: &Ink, advance: f64) -> Vec<Cell> {
    marks(ink, advance)
        .map(|(marks, _)| {
            marks
                .into_iter()
                .filter_map(|m| match m {
                    Mark::Char(cell, _) => Some(*cell),
                    Mark::Speck { .. } => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A mark of the writing, with the columns of the ink it covers: a character, or a speck too small to be one (a
/// decimal point, a pixel of the ground) and whether it sits low in the band of the writing, as a point does.
#[derive(Clone, Debug)]
pub enum Mark {
    Char(Box<Cell>, (usize, usize)),
    Speck { columns: (usize, usize), low: bool },
}

/// Where a speck sits low in the band: the middle of its ink below this share of the band; a decimal point begins
/// below it.
const LOW: f64 = 0.6;

/// The most rows a decimal point covers, in heights of the band.
const POINT_HEIGHT: f64 = 0.35;

/// The ink a row of a mark needs to count as covered by it.
const POINT_ROW: f64 = 0.5;

/// The marks of the writing in `ink` left to right as [`cells`] finds them, with the height of the band of the
/// writing; `None` without writing.
pub fn marks(ink: &Ink, advance: f64) -> Option<(Vec<Mark>, f64)> {
    let rows: Vec<f64> = (0..ink.height).map(|y| (0..ink.width).map(|x| ink.at(x, y)).sum()).collect();
    let most = rows.iter().copied().fold(0.0, f64::max);
    if most <= 0.0 {
        return None;
    }
    let (top, bottom) = longest_run(&rows.iter().map(|r| *r >= ROW_SHARE * most).collect::<Vec<_>>())?;
    let height = (bottom - top + 1) as f64;
    let step = advance * height;
    let columns: Vec<f64> = (0..ink.width).map(|x| (top..=bottom).map(|y| ink.at(x, y)).sum()).collect();
    // A speck of the ground (the edge of the box, a pixel of the picture behind) is no character, nor a decimal
    // point: ink in a few rows at the foot of the band only.
    let flat = |(from, to): (usize, usize)| {
        let inked: Vec<usize> =
            (top..=bottom).filter(|&y| (from..to).map(|c| ink.at(c, y)).sum::<f64>() >= POINT_ROW).collect();
        match (inked.first(), inked.last()) {
            (Some(&a), Some(&b)) => ((b - a + 1) as f64) <= POINT_HEIGHT * height && (a - top) as f64 >= LOW * height,
            _ => true,
        }
    };
    let speck =
        |(from, to): (usize, usize)| columns[from..to].iter().sum::<f64>() < RUN_INK * height * height || flat((from, to));
    let on: Vec<bool> = columns.iter().map(|c| *c >= COLUMN_INK).collect();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut x = 0;
    while x < on.len() {
        if !on[x] {
            x += 1;
            continue;
        }
        let from = x;
        while x < on.len() && on[x] {
            x += 1;
        }
        // A character whose middle columns hold little ink (the thin `0` of the field of the quality) falls into two
        // runs: runs that stand together within one character are one; a speck beside a character (its point) stays
        // apart.
        match runs.last_mut() {
            Some(last) if (x - last.0) as f64 <= JOINED * step && !speck(*last) && !speck((from, x)) => last.1 = x,
            _ => runs.push((from, x)),
        }
    }
    let speck_mark = |(from, to): (usize, usize)| {
        let (mut sum, mut weight) = (0.0, 0.0);
        for y in top..=bottom {
            for c in from..to {
                sum += (y - top) as f64 * ink.at(c, y);
                weight += ink.at(c, y);
            }
        }
        Mark::Speck { columns: (from, to), low: weight > 0.0 && sum / weight >= LOW * (height - 1.0) }
    };
    let mut out = Vec::new();
    for (from, x) in runs {
        if speck((from, x)) {
            out.push(speck_mark((from, x)));
            continue;
        }
        for (l, r) in rejoined(split(&columns, (from, x), step), step, speck) {
            // A point blurred into the character beside it is cut off with a part of its own.
            if speck((l, r)) {
                out.push(speck_mark((l, r)));
                continue;
            }
            // The middle of the ink of the part.
            let (mut sum, mut weight) = (0.0, 0.0);
            for (c, ink) in columns.iter().enumerate().take(r).skip(l) {
                sum += (c as f64 + 0.5) * ink;
                weight += ink;
            }
            let middle = if weight > 0.0 { sum / weight } else { (l + r) as f64 / 2.0 };
            let place = (middle - step / 2.0, top as f64, step, height);
            out.push(Mark::Char(Box::new(Cell { place, grid: grid_of(ink, place) }), (l, r)));
        }
    }
    Some((out, height))
}

/// The unit behind the amount in its field, as the patterns of its font know it.
pub const UNIT: &str = "SCU";

/// The digits in front of the unit in `ink` (`0509` of `0.509 SCU`; the point, blurred into the digits beside it,
/// is left out). `None` where a character is not sure, the writing does not end in the unit read sure ([`UNIT`]), or
/// it does not stand whole in the part.
pub fn read_number(font: &Font, ink: &Ink) -> Option<String> {
    read_number_leading(font, ink, MIN_LEAD)
}

/// [`read_number`] with every digit's best pattern leading only by [`MIN_LEAD_CONFIRMING`]. ⚠ Only to confirm a
/// number another reader read: alone it is not sure.
pub fn read_number_confirming(font: &Font, ink: &Ink) -> Option<String> {
    read_number_leading(font, ink, MIN_LEAD_CONFIRMING)
}

/// The number as [`read_number`] reads it, but every character only the best pattern by any lead. ⚠ Never a
/// reading: only to see whether the shapes speak against what was read ([`crate::stock_scan::against_shapes`]).
pub fn read_number_best(font: &Font, ink: &Ink) -> Option<String> {
    read_number_leading(font, ink, 0.0)
}

/// The writing as [`read_whole`] reads it, but every character only the best pattern by any lead. ⚠ Never a
/// reading, as [`read_number_best`].
pub fn read_whole_best(font: &Font, ink: &Ink) -> Option<String> {
    if !whole(ink) {
        return None;
    }
    read_leading(font, ink, 0.0)
}

fn read_number_leading(font: &Font, ink: &Ink, lead: f64) -> Option<String> {
    if !whole(ink) {
        return None;
    }
    let (marks, _) = marks(ink, font.advance)?;
    let mut text = String::new();
    for mark in &marks {
        if let Mark::Char(cell, _) = mark {
            text.push(character_leading(font, &cell.grid, lead)?);
        }
    }
    text.strip_suffix(UNIT).filter(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).map(str::to_string)
}

/// How wide two runs of columns may stand together at most, in characters, to be one character: two narrow `1`
/// side by side stand wider.
const JOINED: f64 = 1.15;

/// How much wider than a character a run of columns may be and still be one: characters that touch are cut apart.
const TOUCHING: f64 = 1.2;

/// The narrowest part a cut may leave, in characters: a `1` is narrow.
const LEAST_PART: f64 = 0.35;

/// Below how wide a part, in characters, it is narrow: two narrow parts side by side ([`rejoined`]) are the halves
/// of one character a cut went through (the thin middle of a `U` or `0`).
const NARROW: f64 = 0.6;

/// The parts of a run ([`split`]) with neighbouring narrow ones ([`NARROW`]) that stand within one character
/// ([`JOINED`]) joined again; a part that is a speck (`speck`: a decimal point cut off) stays apart.
fn rejoined(parts: Vec<(usize, usize)>, step: f64, speck: impl Fn((usize, usize)) -> bool) -> Vec<(usize, usize)> {
    let narrow = |(l, r): (usize, usize)| ((r - l) as f64) < NARROW * step && !speck((l, r));
    let mut out: Vec<(usize, usize)> = Vec::new();
    for part in parts {
        match out.last_mut() {
            Some(last) if narrow(*last) && narrow(part) && ((part.1 - last.0) as f64) <= JOINED * step => last.1 = part.1,
            _ => out.push(part),
        }
    }
    out
}

/// The run of columns `from..to` cut into characters: a run wider than [`TOUCHING`] characters (`step` wide) is cut
/// at its column of least ink that leaves no part narrower than [`LEAST_PART`], and both parts again.
fn split(columns: &[f64], (from, to): (usize, usize), step: f64) -> Vec<(usize, usize)> {
    let width = (to - from) as f64;
    if width <= TOUCHING * step {
        return vec![(from, to)];
    }
    let margin = (LEAST_PART * step).ceil() as usize;
    let (lo, hi) = (from + margin, to.saturating_sub(margin));
    let Some(cut) = (lo..hi).min_by(|&a, &b| columns[a].total_cmp(&columns[b])) else { return vec![(from, to)] };
    let mut parts = split(columns, (from, cut), step);
    parts.extend(split(columns, (cut + 1, to), step));
    parts
}

/// The longest run of `true`, as its first and last index.
fn longest_run(flags: &[bool]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut start = None;
    for (i, &f) in flags.iter().chain([false].iter()).enumerate() {
        match (f, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if best.is_none_or(|(a, b)| i - s > b - a + 1) {
                    best = Some((s, i - 1));
                }
                start = None;
            }
            _ => {}
        }
    }
    best
}

/// The part `place` of the ink drawn on the grid: each cell of the grid the mean of nine points spread over it.
fn grid_of(ink: &Ink, place: (f64, f64, f64, f64)) -> [f64; CELL_W * CELL_H] {
    let mut grid = [0.0; CELL_W * CELL_H];
    let (cw, ch) = (place.2 / CELL_W as f64, place.3 / CELL_H as f64);
    for gy in 0..CELL_H {
        for gx in 0..CELL_W {
            let mut sum = 0.0;
            for sy in 0..3 {
                for sx in 0..3 {
                    let fx = place.0 + (gx as f64 + (sx as f64 + 0.5) / 3.0) * cw - 0.5;
                    let fy = place.1 + (gy as f64 + (sy as f64 + 0.5) / 3.0) * ch - 0.5;
                    sum += ink.between(fx, fy);
                }
            }
            grid[gy * CELL_W + gx] = sum / 9.0;
        }
    }
    grid
}

/// How alike two grids are: their correlation, from -1 to 1; 0 where one of them is even.
pub fn likeness(a: &[f64], b: &[f64]) -> f64 {
    weighted_likeness(a, b, None)
}

/// [`likeness`] with each cell counted by its weight (`None`: all alike).
pub fn weighted_likeness(a: &[f64], b: &[f64], weights: Option<&[f64]>) -> f64 {
    let w = |i: usize| weights.map_or(1.0, |w| w[i]);
    let cells = || a.iter().zip(b).enumerate().map(|(i, (x, y))| (w(i), *x, *y));
    let total: f64 = cells().map(|c| c.0).sum();
    if total <= 1e-9 {
        return 0.0;
    }
    let ma = cells().map(|(w, x, _)| w * x).sum::<f64>() / total;
    let mb = cells().map(|(w, _, y)| w * y).sum::<f64>() / total;
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (w, x, y) in cells() {
        cov += w * (x - ma) * (y - mb);
        va += w * (x - ma).powi(2);
        vb += w * (y - mb).powi(2);
    }
    if va <= 1e-9 || vb <= 1e-9 { 0.0 } else { cov / (va * vb).sqrt() }
}

/// What the patterns of `font` make of a cell: the best character, how well it fits and by how much it leads the
/// best other character, each cell of the grid counted by its weight in the font.
pub fn judge(font: &Font, grid: &[f64]) -> Option<(char, f64, f64)> {
    let mut scores: Vec<(char, f64)> =
        font.patterns.iter().map(|(c, p)| (*c, weighted_likeness(grid, p, Some(&font.weights)))).collect();
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (best, score) = *scores.first()?;
    let other = scores.iter().find(|(c, _)| *c != best).map_or(-1.0, |s| s.1);
    Some((best, score, score - other))
}

/// How well the pattern of a mark that is no digit (the `x` in front of a count) must fit at least: it carries no
/// value, a count still needs every digit sure.
pub const MIN_LIKENESS_MARK: f64 = 0.6;

/// How much the best pattern must lead at least where the writing only confirms what the text recognition of Windows
/// read ([`read_whole_confirming`]): two readers that work apart then have to mistake the same digit alike.
pub const MIN_LEAD_CONFIRMING: f64 = 0.06;

/// The character a cell shows where its pattern is sure ([`MIN_LIKENESS`], [`MIN_LEAD`]; a mark that is no digit
/// [`MIN_LIKENESS_MARK`]).
pub fn character(font: &Font, grid: &[f64]) -> Option<char> {
    character_leading(font, grid, MIN_LEAD)
}

/// [`character`] with the best pattern leading by at least `lead`.
fn character_leading(font: &Font, grid: &[f64], lead: f64) -> Option<char> {
    let (c, score, ahead) = judge(font, grid)?;
    let least = if c.is_ascii_digit() { MIN_LIKENESS } else { MIN_LIKENESS_MARK };
    (score >= least && ahead >= lead).then_some(c)
}

/// Whether the writing of `ink` stands whole in it: not at its left or right edge, as writing of a part cut too
/// narrow does (`602` cut to `02`).
fn whole(ink: &Ink) -> bool {
    let edge = |x: usize| (0..ink.height).map(|y| ink.at(x, y)).sum::<f64>() >= COLUMN_INK;
    ink.width >= 2 && !edge(0) && !edge(ink.width - 1)
}

/// [`read`] for a part that may have been cut too narrow (a field found by its colour): `None` where the writing
/// does not stand whole in it.
pub fn read_whole(font: &Font, ink: &Ink) -> Option<String> {
    if !whole(ink) {
        return None;
    }
    read(font, ink)
}

/// The writing of a part as [`read_whole`] reads it, but every character with its best pattern leading only by
/// [`MIN_LEAD_CONFIRMING`]. ⚠ Only to confirm a number another reader read: alone it is not sure.
pub fn read_whole_confirming(font: &Font, ink: &Ink) -> Option<String> {
    if !whole(ink) {
        return None;
    }
    read_leading(font, ink, MIN_LEAD_CONFIRMING)
}

/// The writing of `ink` read character by character with `font`; `None` where one character is not sure.
pub fn read(font: &Font, ink: &Ink) -> Option<String> {
    read_leading(font, ink, MIN_LEAD)
}

/// [`read`] with every best pattern leading by at least `lead`.
fn read_leading(font: &Font, ink: &Ink, lead: f64) -> Option<String> {
    let cells = cells(ink, font.advance);
    if cells.is_empty() {
        return None;
    }
    cells.iter().map(|cell| character_leading(font, &cell.grid, lead)).collect()
}

/// A pattern kept as text: [`CELL_H`] rows of [`CELL_W`] characters, ink in steps of a ninth from `0` to `9`.
pub fn pattern_text(grid: &[f64]) -> String {
    let mut out = String::new();
    for row in grid.chunks(CELL_W) {
        for v in row {
            out.push(char::from(b'0' + (v * 9.0).round().clamp(0.0, 9.0) as u8));
        }
        out.push('\n');
    }
    out
}

/// The fonts kept in `text`: per font a line `font <name> <advance>`, then optionally a line `weights` and
/// [`CELL_H`] rows of [`pattern_text`] (all cells weigh alike without), then per character a line `char <c>` and
/// [`CELL_H`] rows of [`pattern_text`]. Lines beginning with `#` are notes.
pub fn parse_fonts(text: &str) -> Vec<Font> {
    let mut fonts: Vec<Font> = Vec::new();
    let mut lines = text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#'));
    fn grid_of_lines<'a>(lines: &mut impl Iterator<Item = &'a str>) -> [f64; CELL_W * CELL_H] {
        let mut grid = [0.0; CELL_W * CELL_H];
        for row in 0..CELL_H {
            let Some(cells) = lines.next() else { break };
            for (x, d) in cells.bytes().take(CELL_W).enumerate() {
                grid[row * CELL_W + x] = f64::from(d.saturating_sub(b'0').min(9)) / 9.0;
            }
        }
        grid
    }
    while let Some(line) = lines.next() {
        let mut words = line.split_whitespace();
        match (words.next(), words.next(), words.next()) {
            (Some("font"), Some(name), Some(advance)) => fonts.push(Font {
                name: name.to_string(),
                advance: advance.parse().unwrap_or(0.75),
                weights: [1.0; CELL_W * CELL_H],
                patterns: Vec::new(),
            }),
            (Some("weights"), _, _) => {
                let grid = grid_of_lines(&mut lines);
                if let Some(font) = fonts.last_mut() {
                    font.weights = grid;
                }
            }
            (Some("char"), Some(c), _) => {
                let grid = grid_of_lines(&mut lines);
                if let (Some(font), Some(c)) = (fonts.last_mut(), c.chars().next()) {
                    font.patterns.push((c, grid));
                }
            }
            _ => {}
        }
    }
    fonts
}

/// The fonts of the game the scan reads by shape, taken from pictures of the game (`glyphs.txt`).
pub static FONTS: std::sync::LazyLock<Vec<Font>> = std::sync::LazyLock::new(|| parse_fonts(include_str!("glyphs.txt")));

/// The font of the given name among [`FONTS`].
pub fn font(name: &str) -> Option<&'static Font> {
    FONTS.iter().find(|f| f.name == name)
}
