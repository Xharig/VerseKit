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

//! The info card the game shows over a tile of the inventory or the freight manager, read.
//!
//! The card stands beside the pointer (freight manager) or at a fixed place left of the list (inventory). It is found
//! by its frame of bright thin lines ([`card_frames`], [`card_at`]), traced at a slant where the terminal stands in
//! the world. Its inside is read in several passes ([`CARD_PASSES`]): the name above the first label line, matched
//! against the names of the stock ([`stock_names`], [`match_name`]) or, where the game shows a text key in place of
//! the name, through the texts of the game ([`GameTexts`]); for goods the line of goods (`Titanium 295 0.404 SCU`),
//! read again alone and large ([`GOODS_PASSES`]). Items take their count from the box at the top right of the tile
//! under the pointer ([`count_boxes`], [`count_at`]); the inventory writes none on a single item. The count in its box
//! and the quality in its field, too small for the text recognition, are also read by the shape of their digits
//! ([`glyphs`], [`shaped_count`], [`shaped_quality`]). The tile under the
//! pointer is found by its faint frame ([`tile_at`]) and kept as a print ([`TilePrint`]), to know it again.
//!
//! ⚠ Nothing unsure is taken silently. The name is put together as for the blueprint scan
//! ([`crate::blueprint_scan::combine`]); quantity and quality only count by a clear majority of the readings
//! ([`combine`]); an amount above the capacity of the box, an amount of nothing, goods counted in pieces and goods
//! without a quality read are never [`Certainty::Sure`]. Nothing here writes into the stock: [`read_card`] says what
//! the card shows, the caller books it.

use std::collections::HashMap;

use crate::blueprint_scan::{Certainty, Entry, How, Names, Reading, TagCheck, check_tag, plain, split_tag};
use crate::refinery_scan::{Area, Job, Mode, Ocr, ScanError, Word, rows};
use crate::win_ocr::Bitmap;

// ------------------------------------------------------------ The frame of the card

/// The brightness of a pixel of a BGRA picture, 0 to 255.
fn brightness(raw: &[u8], width: i64, x: i64, y: i64) -> i32 {
    let at = ((y * width + x) * 4) as usize;
    let (b, g, r) = (i32::from(raw[at]), i32::from(raw[at + 1]), i32::from(raw[at + 2]));
    (2 * r + 5 * g + b) / 8
}

/// How much brighter a frame line is than the ground on both sides of it.
pub const LINE_CONTRAST: i32 = 25;

/// The same for the sides of a frame, which a slant spreads over two columns.
pub const SIDE_CONTRAST: i32 = 20;

/// How much brighter the frame of the tile under the pointer is, which the game draws faint.
pub const TILE_CONTRAST: i32 = 12;

/// One frame line running from left to right as traced: where it begins and where it ends, (x, y) each. A terminal in
/// the world shows it at a slant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Line {
    from: (i64, i64),
    to: (i64, i64),
}

impl Line {
    /// The row of the line at column `x`, on the straight way between its ends.
    fn y_at(&self, x: i64) -> i64 {
        self.from.1 + (self.to.1 - self.from.1) * (x - self.from.0) / (self.to.0 - self.from.0).max(1)
    }

    fn width(&self) -> i64 {
        self.to.0 - self.from.0
    }
}

/// The bright thin lines in `area` running from left to right, at least `least` long. A pixel is on a line where it
/// is `contrast` brighter than the pixels `gap` above and below it. A line is traced column by column, a row
/// up or down where it slants; gaps of up to `bridge` pixels are bridged.
fn lines_in(bitmap: &Bitmap, area: Area, (gap, bridge): (i64, i64), least: i64, contrast: i32) -> Vec<Line> {
    let (w, h) = (bitmap.width, bitmap.height);
    let (x0, y0) = (area.0.max(0), area.1.max(gap));
    let (x1, y1) = ((area.0 + area.2).min(w), (area.1 + area.3).min(h - gap));
    let mut out = Vec::new();
    if x1 <= x0 || y1 <= y0 {
        return out;
    }
    let width = x1 - x0;
    let at = |x: i64, y: i64| ((y - y0) * width + (x - x0)) as usize;
    let mut on = vec![false; ((y1 - y0) * width) as usize];
    for y in y0..y1 {
        for x in x0..x1 {
            let here = brightness(&bitmap.raw, w, x, y);
            let beside = brightness(&bitmap.raw, w, x, y - gap).max(brightness(&bitmap.raw, w, x, y + gap));
            on[at(x, y)] = here - beside >= contrast;
        }
    }
    let mut used = vec![false; on.len()];
    for y in y0..y1 {
        for x in x0..x1 {
            if !on[at(x, y)] || used[at(x, y)] {
                continue;
            }
            used[at(x, y)] = true;
            let (mut row, mut last, mut column) = (y, x, x + 1);
            while column < x1 && column - last <= bridge + 1 {
                let next = [row, row - 1, row + 1].into_iter().find(|&r| r >= y0 && r < y1 && on[at(column, r)]);
                if let Some(r) = next {
                    // The rows of a thick line are traced once.
                    for t in (r - 2).max(y0)..=(r + 2).min(y1 - 1) {
                        if on[at(column, t)] {
                            used[at(column, t)] = true;
                        }
                    }
                    (row, last) = (r, column);
                }
                column += 1;
            }
            if last - x + 1 >= least.max(1) {
                out.push(Line { from: (x, y), to: (last, row) });
            }
        }
    }
    out
}

/// The frames of cards in `area`: rectangles of bright thin lines, a top and a bottom line of the same ends and a
/// line on the left and on the right between them, at a slant where the terminal stands in the world. Sizes in
/// heights of the picture: at least [`CARD_LEAST`] wide and [`CARD_LOWEST`] high, at most [`CARD_HIGHEST`] high.
/// Each frame once, as (left, top, width, height) around its lines.
pub fn card_frames(bitmap: &Bitmap, area: Area) -> Vec<Area> {
    let unit = bitmap.height as f64;
    let (gap, tolerance) = line_steps(bitmap.height);
    let bridge = ((unit * 0.008).round() as i64).max(2 * gap);
    let lines = lines_in(bitmap, area, (gap, bridge), (CARD_LEAST * unit) as i64, LINE_CONTRAST);
    let (lowest, highest) = ((CARD_LOWEST * unit) as i64, (CARD_HIGHEST * unit) as i64);
    let near = |a: i64, b: i64| (a - b).abs() <= tolerance;
    // The ends of the top and the bottom line may stand apart by the slant of a terminal in the world.
    let slant = |a: i64, b: i64| (a - b).abs() <= 3 * tolerance;
    // The frame a top line makes with a bottom line. `hidden`: a line over something bright (a button, the ring of a
    // picture behind the card) loses its contrast there, the shorter of the two runs on as the longer one runs, and a
    // side may show only [`SIDE_SHARE_HIDDEN`].
    // A line with one end below (or above) an end of a longer one and at least a third of its width runs on as the
    // longer one runs.
    let run_on = |short: &mut Line, long: &Line| {
        let width = long.width().max(1);
        let rise = long.to.1 - long.from.1;
        if short.width() * 10 < width * 3 {
            return;
        }
        if slant(long.from.0, short.from.0) && short.to.0 < long.to.0 - 3 * tolerance {
            short.to = (long.to.0, short.from.1 + rise * (long.to.0 - short.from.0) / width);
        } else if slant(long.to.0, short.to.0) && short.from.0 > long.from.0 + 3 * tolerance {
            short.from = (long.from.0, short.to.1 - rise * (short.to.0 - long.from.0) / width);
        }
    };
    let frame_of = |given: &Line, found: &Line, hidden: bool| -> Option<Area> {
        let (mut top, mut bottom) = (*given, *found);
        if hidden {
            if bottom.width() <= top.width() {
                run_on(&mut bottom, &top);
            } else {
                run_on(&mut top, &bottom);
            }
        }
        let top = &top;
        let (left_height, right_height) = (bottom.from.1 - top.from.1, bottom.to.1 - top.to.1);
        if left_height.min(right_height) < lowest
            || left_height.max(right_height) > highest
            || !slant(top.from.0, bottom.from.0)
            || !slant(top.to.0, bottom.to.0)
        {
            return None;
        }
        let side =
            |a: (i64, i64), b: (i64, i64)| side_share(bitmap, (gap, tolerance, SIDE_CONTRAST), (a.0, b.0), (a.1, b.1));
        let (left_seen, right_seen) = (side(top.from, bottom.from), side(top.to, bottom.to));
        let (more, less) = (left_seen.max(right_seen), left_seen.min(right_seen));
        if more < SIDE_SHARE || less < if hidden { SIDE_SHARE_HIDDEN } else { SIDE_SHARE } {
            return None;
        }
        let (left, right) = (top.from.0.min(bottom.from.0), top.to.0.max(bottom.to.0));
        let (upper, lower) = (top.from.1.min(top.to.1), bottom.from.1.max(bottom.to.1));
        Some((left, upper, right - left + 1, lower - upper + 1))
    };
    // Each frame, and whether all of it was seen.
    let mut out: Vec<(Area, bool)> = Vec::new();
    for top in &lines {
        // A frame with a part hidden only for a top line without a whole one.
        let mut frames: Vec<Area> = lines.iter().filter_map(|found| frame_of(top, found, false)).collect();
        let whole = !frames.is_empty();
        if !whole {
            frames = lines.iter().filter_map(|found| frame_of(top, found, true)).collect();
        }
        for frame in frames {
            // A line two pixels thick gives the same frame from both of its rows: kept once, the outer one.
            if let Some((same, seen)) = out.iter_mut().find(|(f, _)| {
                near(f.0, frame.0)
                    && near(f.1, frame.1)
                    && near(f.0 + f.2, frame.0 + frame.2)
                    && near(f.1 + f.3, frame.1 + frame.3)
            }) {
                let (l, t) = (same.0.min(frame.0), same.1.min(frame.1));
                let (r, b) = ((same.0 + same.2).max(frame.0 + frame.2), (same.1 + same.3).max(frame.1 + frame.3));
                *same = (l, t, r - l, b - t);
                *seen |= whole;
                continue;
            }
            out.push((frame, whole));
        }
    }
    // A frame inside one seen whole is a part of a card (the line of goods), no card.
    let all = out.clone();
    let inside =
        |f: &Area, g: &Area| f != g && f.0 >= g.0 && f.1 >= g.1 && f.0 + f.2 <= g.0 + g.2 && f.1 + f.3 <= g.1 + g.3;
    out.into_iter().filter(|(f, _)| !all.iter().any(|(g, whole)| *whole && inside(f, g))).map(|(f, _)| f).collect()
}

/// Whether a side of a frame stands between two lines: [`SIDE_SHARE`] of the rows between them show it
/// ([`side_share`]).
fn side_holds(bitmap: &Bitmap, steps: (i64, i64, i32), x: (i64, i64), ends: (i64, i64)) -> bool {
    side_share(bitmap, steps, x, ends) >= SIDE_SHARE
}

/// The share of a side of a frame seen most rows need to show.
pub const SIDE_SHARE: f64 = 0.7;

/// The share a side may show where the other one shows [`SIDE_SHARE`]: a bright button under the card hides it.
pub const SIDE_SHARE_HIDDEN: f64 = 0.4;

/// The share of the rows between two lines that have a bright pixel of a vertical line near the straight way from
/// its end on the top line (`x.0`) to its end on the bottom line (`x.1`); a terminal in the world shows a frame at a
/// slant.
fn side_share(
    bitmap: &Bitmap,
    (gap, tolerance, contrast): (i64, i64, i32),
    x: (i64, i64),
    (top, bottom): (i64, i64),
) -> f64 {
    let inner = (top + tolerance, bottom - tolerance);
    if inner.1 <= inner.0 {
        return 1.0;
    }
    let hits = (inner.0..inner.1)
        .filter(|&y| {
            let at = x.0 + (x.1 - x.0) * (y - top) / (bottom - top).max(1);
            ((at - tolerance).max(gap)..=(at + tolerance).min(bitmap.width - 1 - gap)).any(|x| {
                let here = brightness(&bitmap.raw, bitmap.width, x, y);
                let beside = brightness(&bitmap.raw, bitmap.width, x - gap, y).max(brightness(
                    &bitmap.raw,
                    bitmap.width,
                    x + gap,
                    y,
                ));
                here - beside >= contrast
            })
        })
        .count();
    hits as f64 / (inner.1 - inner.0) as f64
}

/// The gap across a line and the tolerance of its ends for a picture `height` pixels high.
fn line_steps(height: i64) -> (i64, i64) {
    let unit = height as f64;
    (((unit / 480.0).round() as i64).max(2), ((unit * 0.008).round() as i64).max(4))
}

/// The frame of the tile under the pointer: the nearest line above the pointer that reaches over it, as long as a
/// tile is wide, and below the pointer a line that begins where it begins (the card may cover its other end), with
/// the left side between them. Lines of the card are none of the tile.
pub fn tile_at(bitmap: &Bitmap, pointer: (i64, i64), card: Option<Area>) -> Option<Area> {
    let unit = bitmap.height as f64;
    let (gap, tolerance) = line_steps(bitmap.height);
    let bridge = ((unit * 0.008).round() as i64).max(2 * gap);
    let reach = (TILE_LARGEST * unit) as i64;
    let area = (pointer.0 - reach, pointer.1 - reach, 2 * reach, 2 * reach);
    // A line of the card begins and ends within its frame.
    let of_card = |line: &Line| {
        card.is_some_and(|c| {
            let wider = (c.0 - tolerance, c.1 - tolerance, c.2 + 2 * tolerance, c.3 + 2 * tolerance);
            holds(wider, line.from) && holds(wider, line.to)
        })
    };
    let lines: Vec<Line> = lines_in(bitmap, area, (gap, bridge), (TILE_SMALLEST * unit * 0.2) as i64, TILE_CONTRAST)
        .into_iter()
        .filter(|l| !of_card(l))
        .collect();
    let top = lines
        .iter()
        .filter(|l| {
            l.y_at(pointer.0) < pointer.1
                && l.from.0 <= pointer.0
                && l.to.0 >= pointer.0
                && (TILE_SMALLEST * unit..=TILE_LARGEST * unit).contains(&(l.width() as f64))
        })
        .max_by_key(|l| l.y_at(pointer.0))?;
    let width = top.width();
    let bottom = lines
        .iter()
        .filter(|l| {
            let height = l.from.1 - top.from.1;
            l.y_at(pointer.0.clamp(l.from.0, l.to.0)) > pointer.1
                && (l.from.0 - top.from.0).abs() <= 2 * tolerance
                && l.to.0 <= top.to.0 + tolerance
                && height * 10 >= width * 5
                && height * 10 <= width * 16
                && side_holds(bitmap, (gap, tolerance, TILE_CONTRAST), (top.from.0, l.from.0), (top.from.1, l.from.1))
        })
        .min_by_key(|l| l.from.1)?;
    let left = top.from.0.min(bottom.from.0);
    let upper = top.from.1.min(top.to.1);
    // The bottom line may end under the card: its right end runs as the top line does.
    let lower = bottom.from.1.max(bottom.from.1 + top.to.1 - top.from.1);
    Some((left, upper, top.to.0 - left + 1, lower - upper + 1))
}

/// How much brighter the frame the game draws around the tile under the pointer in the inventory is than the ground
/// beside it.
pub const HOVER_CONTRAST: i32 = 60;

/// How long a side of that frame runs straight at least, in heights of the picture.
pub const HOVER_SIDE: f64 = 0.025;

/// The tile under the pointer where the game draws a bright frame around it (the inventory): from the pointer up,
/// down, left and right the first thin line [`HOVER_CONTRAST`] brighter than the ground on both sides that runs on
/// straight for [`HOVER_SIDE`] heights of the picture across the way; a tile [`TILE_SMALLEST`] to [`TILE_LARGEST`]
/// wide and high. `None` where a side is missing.
pub fn hovered_tile(bitmap: &Bitmap, pointer: (i64, i64)) -> Option<Area> {
    let unit = bitmap.height as f64;
    let (gap, _) = line_steps(bitmap.height);
    let reach = (TILE_LARGEST * unit) as i64;
    let run = ((HOVER_SIDE * unit) as i64).max(4);
    let inside = |x: i64, y: i64| x >= gap && y >= gap && x < bitmap.width - gap && y < bitmap.height - gap;
    // Whether a pixel stands out from the ones `gap` before and after it, across the line (`across` true: the line
    // runs up and down).
    let bright = |x: i64, y: i64, across: bool| {
        inside(x, y) && {
            let (a, b) = if across { ((x - gap, y), (x + gap, y)) } else { ((x, y - gap), (x, y + gap)) };
            lift(&bitmap.raw, bitmap.width, (x, y), a, b) >= HOVER_CONTRAST
        }
    };
    // From the pointer in the direction (dx, dy): the first line that runs on along the other direction, half of
    // `run` to each side, a pixel up or down allowed.
    let side = |(dx, dy): (i64, i64)| -> Option<i64> {
        let across = dx != 0;
        (1..reach).find_map(|step| {
            let (x, y) = (pointer.0 + dx * step, pointer.1 + dy * step);
            if !bright(x, y, across) {
                return None;
            }
            let seen = (-run / 2..=run / 2)
                .filter(|&along| {
                    (-1..=1).any(|off| {
                        let (px, py) = if across { (x + off, y + along) } else { (x + along, y + off) };
                        bright(px, py, across)
                    })
                })
                .count() as i64;
            (seen * 10 >= run * 9).then_some(if across { x } else { y })
        })
    };
    let (top, bottom, left, right) = (side((0, -1))?, side((0, 1))?, side((-1, 0))?, side((1, 0))?);
    let (width, height) = (right - left + 1, bottom - top + 1);
    let fits = |v: i64| (TILE_SMALLEST * unit..=TILE_LARGEST * unit).contains(&(v as f64));
    (fits(width) && fits(height)).then_some((left, top, width, height))
}

/// How much brighter the pixel `here` is than the brighter of the pixels `a` and `b`.
fn lift(raw: &[u8], width: i64, here: (i64, i64), a: (i64, i64), b: (i64, i64)) -> i32 {
    brightness(raw, width, here.0, here.1) - brightness(raw, width, a.0, a.1).max(brightness(raw, width, b.0, b.1))
}

// ------------------------------------------------------------ The print of a tile

/// How many cells a tile print has across and down.
pub const PRINT_CELLS: usize = 16;

/// How alike two prints must be ([`TilePrint::likeness`]) to show the same tile.
pub const PRINT_SAME: f64 = 0.85;

/// A small grey picture of a tile, to know it again: [`PRINT_CELLS`] × [`PRINT_CELLS`] cells, each the brightness
/// of its part of the tile less the mean of all, in steps of a sixteenth of their spread; a cell the card covers
/// (or that lies outside the screen) is left out. Spread and mean taken out, a tile is the same under a little more
/// or less light.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TilePrint {
    /// Row by row; `None` for a cell left out.
    pub cells: Vec<Option<i8>>,
}

impl TilePrint {
    /// The print of `tile` in `bitmap`, without its frame (a margin of a sixteenth of the tile on every side: the
    /// frame shows only on the tile under the pointer) and without what `cover` covers.
    pub fn of(bitmap: &Bitmap, tile: Area, cover: Option<Area>) -> Option<TilePrint> {
        let margin = (tile.2.min(tile.3) / 16).max(2);
        let inner = (tile.0 + margin, tile.1 + margin, tile.2 - 2 * margin, tile.3 - 2 * margin);
        if inner.2 < PRINT_CELLS as i64 || inner.3 < PRINT_CELLS as i64 {
            return None;
        }
        let mut means: Vec<Option<f64>> = Vec::with_capacity(PRINT_CELLS * PRINT_CELLS);
        for row in 0..PRINT_CELLS as i64 {
            for column in 0..PRINT_CELLS as i64 {
                let x0 = inner.0 + inner.2 * column / PRINT_CELLS as i64;
                let x1 = inner.0 + inner.2 * (column + 1) / PRINT_CELLS as i64;
                let y0 = inner.1 + inner.3 * row / PRINT_CELLS as i64;
                let y1 = inner.1 + inner.3 * (row + 1) / PRINT_CELLS as i64;
                let (mut sum, mut seen, mut all) = (0i64, 0i64, 0i64);
                for y in y0..y1 {
                    for x in x0..x1 {
                        all += 1;
                        if x < 0
                            || y < 0
                            || x >= bitmap.width
                            || y >= bitmap.height
                            || cover.is_some_and(|c| holds(c, (x, y)))
                        {
                            continue;
                        }
                        sum += i64::from(brightness(&bitmap.raw, bitmap.width, x, y));
                        seen += 1;
                    }
                }
                means.push((seen * 2 > all).then(|| sum as f64 / seen as f64));
            }
        }
        let known: Vec<f64> = means.iter().flatten().copied().collect();
        if known.len() < PRINT_CELLS * PRINT_CELLS / 4 {
            return None;
        }
        let mean = known.iter().sum::<f64>() / known.len() as f64;
        let spread = (known.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / known.len() as f64).sqrt().max(1.0);
        let cells = means
            .into_iter()
            .map(|v| v.map(|v| ((v - mean) / spread * 16.0).round().clamp(-127.0, 127.0) as i8))
            .collect();
        Some(TilePrint { cells })
    }

    /// How alike two prints are: the correlation of the cells both have, from -1 to 1; 0 where they share fewer
    /// than half of the cells.
    pub fn likeness(&self, other: &TilePrint) -> f64 {
        let pairs: Vec<(f64, f64)> = self
            .cells
            .iter()
            .zip(&other.cells)
            .filter_map(|(a, b)| Some((f64::from((*a)?), f64::from((*b)?))))
            .collect();
        if pairs.len() * 2 < self.cells.len().max(1) {
            return 0.0;
        }
        let n = pairs.len() as f64;
        let (ma, mb) = (pairs.iter().map(|p| p.0).sum::<f64>() / n, pairs.iter().map(|p| p.1).sum::<f64>() / n);
        let cov: f64 = pairs.iter().map(|(a, b)| (a - ma) * (b - mb)).sum();
        let va: f64 = pairs.iter().map(|(a, _)| (a - ma).powi(2)).sum();
        let vb: f64 = pairs.iter().map(|(_, b)| (b - mb).powi(2)).sum();
        if va <= 0.0 || vb <= 0.0 {
            return 0.0;
        }
        cov / (va * vb).sqrt()
    }

    /// Whether two prints show the same tile (the same item: two tiles of the same item are told apart by where
    /// they stand).
    pub fn same(&self, other: &TilePrint) -> bool {
        self.likeness(other) >= PRINT_SAME
    }

    /// The print as bytes to keep: one per cell, -128 for a cell left out.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.cells.iter().map(|c| c.unwrap_or(i8::MIN) as u8).collect()
    }

    /// A print kept by [`TilePrint::to_bytes`]; `None` for bytes of another length.
    pub fn from_bytes(bytes: &[u8]) -> Option<TilePrint> {
        (bytes.len() == PRINT_CELLS * PRINT_CELLS)
            .then(|| TilePrint { cells: bytes.iter().map(|&b| Some(b as i8).filter(|v| *v != i8::MIN)).collect() })
    }
}

/// The narrowest tile, in heights of the picture.
pub const TILE_SMALLEST: f64 = 0.06;
/// The widest tile, in heights of the picture.
pub const TILE_LARGEST: f64 = 0.25;

/// The card the pointer shows: of the frames around it, the nearest one that does not hold the pointer (the frame
/// of the tile under the pointer is no card). The inventory shows the card at a fixed place left of its list, so a
/// card may stand up to [`CARD_DISTANCE_FIXED`] heights of the picture off the pointer.
pub fn card_at(bitmap: &Bitmap, pointer: (i64, i64)) -> Option<Area> {
    cards_at(bitmap, pointer).into_iter().next()
}

/// The frames that may be the card at the pointer, nearest first, as [`card_at`] picks them: the inventory draws a
/// frame around its tiles too, so the nearest one may be no card.
pub fn cards_at(bitmap: &Bitmap, pointer: (i64, i64)) -> Vec<Area> {
    let frames = card_frames(bitmap, search_area(pointer, (bitmap.width, bitmap.height)));
    let unit = bitmap.height as f64;
    let mut near: Vec<(f64, Area)> = frames
        .into_iter()
        .filter(|f| !holds(*f, pointer))
        .map(|f| (gap_to(f, pointer), f))
        .filter(|(d, _)| *d <= CARD_DISTANCE_FIXED * unit)
        .collect();
    near.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| (a.1.2 * a.1.3).cmp(&(b.1.2 * b.1.3))));
    near.into_iter().map(|(_, f)| f).collect()
}

/// How many frames around the pointer are read at most to find the card ([`cards_at`]).
pub const CARD_TRIES: usize = 4;

/// The most a card at a fixed place stands off the pointer, in heights of the picture.
pub const CARD_DISTANCE_FIXED: f64 = 0.6;

/// Whether `area` holds the point.
fn holds(area: Area, (x, y): (i64, i64)) -> bool {
    x >= area.0 && x < area.0 + area.2 && y >= area.1 && y < area.1 + area.3
}

/// How far the point lies outside `area`, in pixels; 0 inside.
fn gap_to(area: Area, (x, y): (i64, i64)) -> f64 {
    let dx = (area.0 - x).max(x - (area.0 + area.2 - 1)).max(0);
    let dy = (area.1 - y).max(y - (area.1 + area.3 - 1)).max(0);
    (dx as f64).hypot(dy as f64)
}

/// The part of the screen searched for the card: the card stands right of and below the pointer, left of or above
/// it near the edge of the list, or left of the list in the inventory; cut to the screen.
pub fn search_area(pointer: (i64, i64), size: (i64, i64)) -> Area {
    let unit = size.1 as f64;
    let left = ((pointer.0 as f64 - (CARD_DISTANCE_FIXED + 0.3) * unit) as i64).max(0);
    let top = ((pointer.1 as f64 - (CARD_DISTANCE_FIXED + 0.1) * unit) as i64).max(0);
    let right = ((pointer.0 as f64 + 0.6 * unit) as i64).min(size.0);
    let bottom = ((pointer.1 as f64 + 0.45 * unit) as i64).min(size.1);
    (left, top, (right - left).max(0), (bottom - top).max(0))
}

// ------------------------------------------------------------ The card found by its writing

/// A line of writing as the recognition found it in a part of the screen: words side by side at the same height,
/// none further from the one before than [`WORD_GAP`] heights of the writing.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl TextLine {
    fn height(&self) -> f64 {
        (self.bottom - self.top).max(1.0)
    }
}

/// How far apart two words of one line stand at most, in heights of the writing.
pub const WORD_GAP: f64 = 1.8;

/// The lines of writing in `words`: words at the same height (their middles within half a height) and close side by
/// side make a line; words far apart at the same height (a card beside a list) make lines of their own. Top to
/// bottom, left to right.
pub fn text_lines(words: &[Word]) -> Vec<TextLine> {
    let mut ordered: Vec<&Word> = words.iter().filter(|w| !w.t.trim().is_empty() && w.h > 0.0).collect();
    ordered.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut lines: Vec<TextLine> = Vec::new();
    for word in ordered {
        let middle = word.y + word.h / 2.0;
        let joined = lines.iter_mut().rev().find(|line| {
            let height = line.height().max(word.h);
            (middle - (line.top + line.bottom) / 2.0).abs() <= height / 2.0
                && word.x - line.right <= WORD_GAP * height
                && word.x >= line.left
        });
        match joined {
            Some(line) => {
                line.text.push(' ');
                line.text.push_str(word.t.trim());
                line.right = line.right.max(word.x + word.w);
                line.top = line.top.min(word.y);
                line.bottom = line.bottom.max(word.y + word.h);
            }
            None => lines.push(TextLine {
                text: word.t.trim().to_string(),
                left: word.x,
                top: word.y,
                right: word.x + word.w,
                bottom: word.y + word.h,
            }),
        }
    }
    lines.sort_by(|a, b| a.top.total_cmp(&b.top).then(a.left.total_cmp(&b.left)));
    lines
}

/// Whether a line begins with a label of [`LABELS`], as the first line below the name of a card does.
fn begins_with_label(text: &str) -> bool {
    let low = plain(text);
    LABELS.iter().any(|label| low.starts_with(label))
}

/// How far the left ends of two lines of one card stand apart at most, in heights of the writing.
pub const ALIGNED: f64 = 0.8;

/// How far left of the label a line below it may begin and still belong to the card, in heights of the writing.
pub const INDENT: f64 = 3.0;

/// How far apart two fields of a line of the card stand at most, in heights of the writing.
pub const FIELD_GAP: f64 = 5.0;

/// How far below the line before it a line of the name stands at most, in heights of the writing.
pub const NAME_STEP: f64 = 1.2;

/// How far below the line before it a line of the card stands at most, in heights of the writing: the blank lines
/// and the dashed line between two parts of the card are bridged.
pub const CARD_STEP: f64 = 6.0;

/// The card at the pointer found by its writing, where its frame shows too faint to be found: a line beginning with
/// a label ([`LABELS`]) with up to [`NAME_LINES`] lines of the name above it, all beginning at the same place; with
/// the lines below beginning there too, the card reaches as far as they do. The places that may be the card, at most
/// [`CARD_DISTANCE_FIXED`] heights of the screen from the pointer, nearest first, each as (left, top, width,
/// height) with a margin of half a line around the writing.
pub fn cards_by_writing(lines: &[TextLine], pointer: (i64, i64), unit: f64) -> Vec<Area> {
    let aligned = |a: &TextLine, b: &TextLine| (a.left - b.left).abs() <= ALIGNED * a.height().max(b.height());
    let mut found: Vec<(f64, Area)> = Vec::new();
    // The lines that belong to a card found above: a label among them begins no card of its own.
    let mut taken = vec![false; lines.len()];
    for (at, label) in lines.iter().enumerate() {
        if taken[at] || !begins_with_label(&label.text) {
            continue;
        }
        let height = label.height();
        // The lines of the name, bottom up.
        let mut name: Vec<&TextLine> = Vec::new();
        let mut upper = label;
        for line in lines[..at].iter().rev() {
            if name.len() == NAME_LINES || upper.top - line.bottom > NAME_STEP * height {
                break;
            }
            if line.bottom <= upper.top + height / 2.0 && aligned(line, label) {
                if begins_with_label(&line.text) || is_label(&line.text) {
                    break;
                }
                name.push(line);
                upper = line;
            }
        }
        let Some(&first) = name.last() else { continue };
        let mut lower = label;
        let mut right = label.right;
        // The lines below begin where the label begins, or a little left of it (the capacity of a box of goods and
        // its line of goods); the fields of a line stand apart (`Aslarite`, `287`, `0.083 SCU`) and reach as far
        // right as the last one.
        let below_it = |line: &TextLine| {
            aligned(line, label) || (line.left < label.left && label.left - line.left <= INDENT * height)
        };
        let mut rows: Vec<usize> = Vec::new();
        let mut leftmost = first.left.min(label.left);
        for (below, line) in lines.iter().enumerate().skip(at + 1) {
            if line.top - lower.bottom > CARD_STEP * height {
                break;
            }
            if line.top >= lower.bottom - height / 2.0 && below_it(line) {
                right = right.max(line.right);
                leftmost = leftmost.min(line.left);
                lower = line;
                taken[below] = true;
                rows.push(below);
            }
        }
        for row in rows {
            let middle = (lines[row].top + lines[row].bottom) / 2.0;
            let mut end = lines[row].right;
            let mut beside: Vec<(usize, &TextLine)> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.left > lines[row].right && ((l.top + l.bottom) / 2.0 - middle).abs() <= height / 2.0)
                .collect();
            beside.sort_by(|a, b| a.1.left.total_cmp(&b.1.left));
            for (i, field) in beside {
                if field.left - end > FIELD_GAP * height {
                    break;
                }
                end = field.right;
                taken[i] = true;
            }
            right = right.max(end);
        }
        right = name.iter().map(|l| l.right).fold(right, f64::max);
        let margin = height / 2.0;
        let left = (leftmost - margin) as i64;
        let top = (first.top - margin) as i64;
        let card = (left, top, (right + margin) as i64 - left, (lower.bottom + margin) as i64 - top);
        let distance = gap_to(card, pointer);
        if distance <= CARD_DISTANCE_FIXED * unit {
            found.push((distance, card));
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found.into_iter().map(|(_, card)| card).collect()
}

/// The nearest place [`cards_by_writing`] finds.
pub fn card_by_writing(lines: &[TextLine], pointer: (i64, i64), unit: f64) -> Option<Area> {
    cards_by_writing(lines, pointer, unit).into_iter().next()
}

/// The enlargement (for a screen 1440 pixels high) the part around the pointer is read at first to find the card by
/// its writing ([`card_by_writing`]).
pub const WRITING_SCALE: f64 = 2.0;

/// The enlargements the part around the pointer is read at, one after the other, until the card is found by its
/// writing.
pub const WRITING_SCALES: [f64; 3] = [WRITING_SCALE, 1.5, 3.0];

/// The narrowest card, in heights of the picture.
pub const CARD_LEAST: f64 = 0.08;
/// The lowest card (one line of name), in heights of the picture.
pub const CARD_LOWEST: f64 = 0.018;
/// The highest card, in heights of the picture.
pub const CARD_HIGHEST: f64 = 0.4;

// ------------------------------------------------------------ The lines of the card

/// The beginnings of the lines below the name, in comparison form ([`plain`]): German, English and the other
/// languages of the game where known. A line with a colon counts as one too.
pub const LABELS: [&str; 22] = [
    "kapazitaet",
    "capacity",
    "volumen",
    "volume",
    "verbleibender inhalt",
    "remaining",
    "auf dich registriert",
    "registered to you",
    "munitionsanzahl",
    "ammo count",
    "feuerrate",
    "fire rate",
    "integritaet",
    "integrity",
    "haltbarkeit",
    "durability",
    "gestohlen",
    "stolen",
    "capacite",
    "capacidad",
    "capacita",
    "kapacita",
];

/// Whether a line of the card is one below the name: a label of [`LABELS`], a colon, a value with two decimals
/// (`46.00 - 46.00`), a volume or an amount in SCU.
pub fn is_label(line: &str) -> bool {
    let low = plain(line);
    let decimals = line.split_whitespace().any(|word| {
        let word = word.trim_end_matches(|c: char| !c.is_ascii_digit());
        word.rsplit_once(['.', ',']).is_some_and(|(whole, part)| {
            part.len() == 2 && !whole.is_empty() && (whole.chars().chain(part.chars())).all(|c| c.is_ascii_digit())
        })
    });
    line.contains(':')
        || decimals
        || LABELS.iter().any(|label| low.starts_with(label) || (label.contains(' ') && low.contains(label)))
        || low.split(' ').any(|word| {
            word == "scu"
                || word == "uscu"
                || word == "µscu"
                || word.strip_suffix("scu").is_some_and(|stem| !stem.is_empty())
        })
}

/// Whether a line names the ammunition of the item (`Munitionsanzahl: 80/0`).
pub fn is_ammo_line(line: &str) -> bool {
    let low = plain(line);
    low.starts_with("munitionsanzahl") || low.starts_with("ammo count")
}

/// Whether a line gives the capacity of a box of goods (`Kapazität: 1.00SCU`).
pub fn is_capacity_line(line: &str) -> bool {
    capacity_of(line).is_some()
}

/// The capacity of a box of goods from its line (`Kapazität: 1.00SCU`): behind a colon an amount ending in `SCU`.
/// `Some(None)` where the line is one but its number could not be read; `None` for any other line, also for a
/// volume in µSCU (which the recognition may read as `pSCU`).
pub fn capacity_of(line: &str) -> Option<Option<f64>> {
    let (_, value) = line.split_once(':')?;
    let value: String = value.chars().filter(|c| !c.is_whitespace()).collect();
    let upper = value.to_uppercase();
    let stem = upper.strip_suffix("SCU")?;
    // The capacity has two decimals (`1.00`); a volume in µSCU has none.
    if stem.is_empty() || stem.ends_with(['µ', 'Μ', 'U', 'P']) || !stem.contains(['.', ',']) {
        return None;
    }
    Some(value.get(..stem.len()).and_then(number_of))
}

/// The line of goods of a card (`Titanium 295 0.404 SCU`): the quality and the amount in SCU. `None` for a line
/// without an amount in SCU, for the capacity (it has a colon) and for a volume in µSCU.
pub fn goods_line(line: &str) -> Option<(Option<u32>, f64)> {
    if line.contains(':') {
        return None;
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    let at = words.iter().rposition(|w| w.to_uppercase().ends_with("SCU"))?;
    let upper = words[at].to_uppercase();
    let stem = &upper[..upper.len() - 3];
    if stem.ends_with('µ') || stem.ends_with('Μ') || stem == "U" {
        return None;
    }
    let (amount, before) = if stem.is_empty() { (*words.get(at.checked_sub(1)?)?, at - 1) } else { (stem, at) };
    // The game writes a part of an SCU with three decimals (`0.145 SCU`), a whole one without (`4 SCU`): other
    // decimals are digits lost.
    if amount.rsplit_once(['.', ',']).is_some_and(|(_, part)| part.chars().count() != 3) {
        return None;
    }
    // A box holds more than nothing: an amount of 0 was read wrong.
    let amount = number_of(amount).filter(|a| *a > 0.0)?;
    // A quality read mostly as letters taken for digits is guessed (`SBO` of `588` is no 580): it needs more digits
    // read as digits than letters taken for them. A lone round letter is the quality 0 of a good (`Gold O 16 SCU`).
    let quality = before
        .checked_sub(1)
        .and_then(|q| words.get(q))
        .filter(|q| {
            let digits = q.chars().filter(char::is_ascii_digit).count();
            digits > q.chars().count() - digits || matches!(**q, "O" | "o" | "D" | "Q")
        })
        .map(|q| digits_of(q))
        .filter(|q| (1..=4).contains(&q.len()) && q.chars().all(|c| c.is_ascii_digit()))
        .and_then(|q| q.parse::<u32>().ok())
        .filter(|q| *q <= MAX_QUALITY);
    // A whole number of SCU stands behind the quality (`Titanium 295 4 SCU`); alone behind the name it is the quality
    // with the amount lost (`Aslarite 602 SCU` of `Aslarite 602 0.166 SCU`).
    if quality.is_none() && !words[before].contains(['.', ',']) {
        return None;
    }
    Some((quality, amount))
}

/// A word of a number with the characters the recognition mistakes for digits read as them.
fn digits_of(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            'O' | 'o' | 'Q' | 'D' => '0',
            'l' | 'I' | '|' | 'i' => '1',
            'S' | 's' => '5',
            'B' => '8',
            'Z' | 'z' => '2',
            other => other,
        })
        .collect()
}

/// The highest quality of goods.
pub const MAX_QUALITY: u32 = 1000;

/// A number as the card writes it, with a dot or a comma; characters the recognition mistakes for digits read as
/// them.
fn number_of(text: &str) -> Option<f64> {
    let digits = digits_of(text);
    if !digits.chars().any(|c| c.is_ascii_digit())
        || !digits.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',')
    {
        return None;
    }
    crate::stock::parse_number(&digits).filter(|n| n.is_finite() && *n >= 0.0)
}

/// The count on a tile: the number behind the x in the freight manager (`x3`, `X1`, `×12`), before it in the
/// inventory (`2x`). `None` for any other word.
pub fn count_of(word: &str) -> Option<u32> {
    let word = word.trim();
    let is_x = |c: char| matches!(c, 'x' | 'X' | '×' | '*');
    let number = if word.starts_with(is_x) {
        word.trim_start_matches(is_x)
    } else if word.ends_with(is_x) && word.starts_with(|c: char| c.is_ascii_digit()) {
        word.trim_end_matches(is_x)
    } else {
        return None;
    };
    let visible = |text: &str| text.chars().filter(|c| !c.is_whitespace()).count();
    if visible(number) + 1 != visible(word) {
        return None;
    }
    let rest: String = number
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            'O' | 'o' | 'Q' | 'D' => '0',
            'l' | 'I' | 'i' | '|' | ']' | '[' => '1',
            other => other,
        })
        .collect();
    if rest.is_empty() || rest.len() > 4 || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok().filter(|n| *n > 0)
}

/// A text key the game writes where a name is missing in its language (`＠item_NameKsar_LMG_Energy_01_Mag`): the
/// key, without the sign in front.
pub fn broken_key(text: &str) -> Option<String> {
    let rest = match text.find(['@', '＠']) {
        Some(at) => &text[at + text[at..].chars().next().map_or(1, char::len_utf8)..],
        None => {
            let at = text.to_lowercase().find("item_")?;
            text.get(at..)?
        }
    };
    let key: String = rest.trim_start().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    (key.len() >= 6 && key.contains('_')).then_some(key)
}

// ------------------------------------------------------------ The texts of the game

/// The item texts of one `global.ini`: every key that begins with `item` (names of items, `items_commodities_…`)
/// with its text, and back from a text to its key.
#[derive(Clone, Debug, Default)]
pub struct GameTexts {
    /// Key in lower case → (key as written, text).
    by_key: HashMap<String, (String, String)>,
    /// Key folded ([`fold_key`]) → keys in lower case.
    by_folded: HashMap<String, Vec<String>>,
    /// Text in comparison form → key as written; a key with `name` in it wins over one without.
    by_text: HashMap<String, String>,
    /// Name of an item folded ([`fold_name`]) → keys as written.
    by_folded_name: HashMap<String, Vec<String>>,
}

/// A name in comparison form with the characters the recognition mistakes for one another folded together: those of
/// [`crate::blueprint_scan::fold_lookalikes`], and `6` and `g` with `s` (`SWISBR2`, `SWIGBR2` for `SW16BR2`).
fn fold_name(text: &str) -> String {
    crate::blueprint_scan::fold_lookalikes(&plain(text))
        .chars()
        .map(|c| match c {
            '6' | 'g' => 's',
            other => other,
        })
        .collect()
}

/// A key with the characters the recognition mistakes for one another folded together.
fn fold_key(key: &str) -> String {
    key.to_lowercase()
        .chars()
        .map(|c| match c {
            'l' | '1' | 'i' | '|' => 'i',
            '0' | 'o' => 'o',
            other => other,
        })
        .collect()
}

impl GameTexts {
    /// The item texts of the content of a `global.ini`.
    pub fn parse(ini: &str) -> GameTexts {
        let mut texts = GameTexts::default();
        for line in ini.trim_start_matches('\u{feff}').lines() {
            let Some((key, text)) = line.split_once('=') else { continue };
            let key = crate::text::py_strip(key.split(',').next().unwrap_or(""));
            let low = key.to_lowercase();
            if !low.starts_with("item") {
                continue;
            }
            let text = crate::text::py_strip(crate::text::py_strip(text).trim_start_matches('*')).to_string();
            if text.is_empty() || texts.by_key.contains_key(&low) {
                continue;
            }
            texts.by_folded.entry(fold_key(&low)).or_default().push(low.clone());
            let form = plain(&text);
            let named = low.contains("name");
            if low.starts_with("item_name") {
                texts.by_folded_name.entry(fold_name(&text)).or_default().push(key.to_string());
            }
            match texts.by_text.get(&form) {
                Some(old) if !named || old.to_lowercase().contains("name") => {}
                _ => {
                    texts.by_text.insert(form, key.to_string());
                }
            }
            texts.by_key.insert(low, (key.to_string(), text));
        }
        texts
    }

    /// How many texts there are.
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// The text of a key, whatever its case.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.by_key.get(&key.to_lowercase()).map(|(_, text)| text.as_str())
    }

    /// The key of a name.
    pub fn key_of(&self, name: &str) -> Option<&str> {
        self.by_text.get(&plain(name)).map(String::as_str)
    }

    /// The key a key read stands for, with its text: the same key, or the only one it is after folding the
    /// characters the recognition mistakes (`false` behind it).
    pub fn resolve(&self, read: &str) -> Option<(&str, &str, bool)> {
        if let Some((key, text)) = self.by_key.get(&read.to_lowercase()) {
            return Some((key, text, true));
        }
        match self.by_folded.get(&fold_key(read)).map(Vec::as_slice) {
            Some([only]) => self.by_key.get(only).map(|(key, text)| (key.as_str(), text.as_str(), false)),
            _ => None,
        }
    }
}

/// Pairs (translated name, English name) of the item texts two `global.ini` share by key, for
/// [`Names::add_translations`]; a translated name that equals the English one is left out.
pub fn translation_pairs(english: &GameTexts, game: &GameTexts) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = game
        .by_key
        .iter()
        .filter_map(|(low, (_, text))| {
            let (_, original) = english.by_key.get(low)?;
            (text != original).then(|| (text.clone(), original.clone()))
        })
        .collect();
    pairs.sort();
    pairs
}

/// The comparison set of the stock: every item and every good of `data`, under its name; an item the catalog knows
/// with class, size and grade, so a tag read behind its name is checked.
pub fn stock_names(data: &crate::stock_data::StockData, catalog: Option<&crate::catalog_cache::CatalogCache>) -> Names {
    let by_name: HashMap<&str, &crate::catalog_cache::Blueprint> =
        catalog.map(|c| c.blueprints.iter().map(|b| (b.name.as_str(), b)).collect()).unwrap_or_default();
    let goods: std::collections::HashSet<String> = data
        .choices()
        .values()
        .filter(|c| c.material.is_some())
        .map(|c| c.name.clone())
        .chain(data.trade_goods())
        .chain(data.material_names().iter().cloned())
        .collect();
    let mut all: Vec<String> = data.choices().values().map(|c| c.name.clone()).chain(goods.iter().cloned()).collect();
    all.sort();
    all.dedup();
    let mut names = Names::new();
    for name in all {
        let entry = match by_name.get(name.as_str()) {
            Some(blueprint) => crate::blueprint_scan::entry_of(blueprint),
            None if goods.contains(&name) => Entry { name, class: Some(GOODS_CLASS.to_string()), ..Entry::default() },
            None => Entry { name, ..Entry::default() },
        };
        names.add(entry);
    }
    // Every blueprint of the catalog is a name the scan knows, also where the stock does not hold it yet.
    // A name the stock holds in another spelling stays as the stock has it.
    let mut known: std::collections::HashSet<String> = names.all().map(crate::blueprint_scan::plain).collect();
    for blueprint in catalog.map(|c| c.blueprints.as_slice()).unwrap_or_default() {
        if known.insert(crate::blueprint_scan::plain(&blueprint.name)) {
            names.add(crate::blueprint_scan::entry_of(blueprint));
        }
    }
    names
}

/// The class a good of the stock carries in the comparison set ([`stock_names`]): goods are counted in SCU, never in
/// pieces.
pub const GOODS_CLASS: &str = "SCU";

/// Whether `name` is a good of the stock in `names` ([`GOODS_CLASS`]).
pub fn is_goods(names: &Names, name: &str) -> bool {
    names.entry(name).is_some_and(|e| e.class.as_deref() == Some(GOODS_CLASS))
}

// ------------------------------------------------------------ One reading of the card

/// A frame (left, top, right, bottom) in pixels of the picture.
pub type Frame = (f64, f64, f64, f64);

/// The lines of one reading, top to bottom, each the words of a row joined by blanks, with its frame. Lines with
/// fewer than two letters or digits (a frame line read as `|`) are left out.
pub fn card_lines(words: &[Word]) -> Vec<(String, Frame)> {
    rows(words)
        .into_iter()
        .filter_map(|row| {
            let text = row.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ");
            if text.chars().filter(|c| c.is_alphanumeric()).count() < 2 {
                return None;
            }
            let frame = row.iter().fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |f, w| {
                (f.0.min(w.x), f.1.min(w.y), f.2.max(w.x + w.w), f.3.max(w.y + w.h))
            });
            Some((text, frame))
        })
        .collect()
}

/// How wide a gap between two words of a line of the name may be at most, in heights of the words: a wider one is a
/// word the recognition lost (`TBF-4 Combat` of `TBF-4 "Balefire" Combat`).
pub const LOST_WORD_GAP: f64 = 1.5;

/// Whether a line of `words` within the rows of `frame` (left, top, right, bottom) has a gap between two words wider
/// than [`LOST_WORD_GAP`].
pub fn lost_word_in(words: &[Word], frame: Frame) -> bool {
    rows(words).iter().any(|row| {
        let middle = row.iter().map(|w| w.y + w.h / 2.0).sum::<f64>() / row.len().max(1) as f64;
        let height = row.iter().map(|w| w.h).fold(0.0, f64::max);
        middle >= frame.1
            && middle <= frame.3
            && row.windows(2).any(|pair| pair[1].x - (pair[0].x + pair[0].w) > LOST_WORD_GAP * height)
    })
}

/// [`read_lines`] of the words of one reading, the name in question where a word of it was lost ([`lost_word_in`]).
pub fn read_words(words: &[Word], names: &Names, texts: Option<&GameTexts>) -> CardReading {
    let mut reading = read_lines(&card_lines(words), names, texts);
    if reading.name_frame.is_some_and(|frame| lost_word_in(words, frame)) {
        reading.reading.certainty = reading.reading.certainty.min(Certainty::Likely);
    }
    reading
}

/// What one reading of the card gives.
#[derive(Clone, Debug, PartialEq)]
pub struct CardReading {
    /// The name matched against the stock.
    pub reading: Reading,
    /// A text key the card shows in place of a name, as read.
    pub broken: Option<String>,
    /// The key it stands for, where the texts of the game have it.
    pub key: Option<String>,
    /// Quality and amount in SCU of the line of goods.
    pub goods: Option<(Option<u32>, f64)>,
    /// A line with the capacity of a box of goods.
    pub capacity: bool,
    /// A line with the ammunition.
    pub ammo: bool,
    /// A label stands right below the lines of the name, as on every card: lines without one are a part of
    /// something else (a paragraph inside the card, a list beside it).
    pub labelled: bool,
    /// A tag behind the name, as of a component (`(Mil/1/B)`).
    pub tag: bool,
    /// Every line, as read.
    pub lines: Vec<String>,
    /// The frame of the lines of the name.
    pub name_frame: Option<Frame>,
    /// The frame of the line of goods.
    pub goods_frame: Option<Frame>,
    /// The capacity of a box of goods, where its number was read.
    pub capacity_value: Option<f64>,
}

/// How many lines a name may take at most.
pub const NAME_LINES: usize = 3;

/// Matches a name read against the stock. A tag behind it (`(Mil/1/B)`) is checked only against a name whose class,
/// size or grade the set knows: one that says otherwise makes the hit a question.
pub fn match_name(text: &str, names: &Names) -> Reading {
    let mut reading = match_name_as_read(text, names);
    if reading.certainty < Certainty::Sure
        && let Some(mended) = mend_tag(text)
        && mended != text
    {
        let again = match_name_as_read(&mended, names);
        if again.certainty > reading.certainty {
            reading = Reading { text: text.to_string(), ..again };
        }
    }
    // A short name found inside a much longer line (`Sawbuck` in `16BR2 “Sawbuck” Repeater`, a card whose left edge
    // was cut off: `SW16BR2 “Sawbuck” Repeater`) leaves most of the line unexplained: it is in question.
    if reading.certainty == Certainty::Sure
        && let Some(name) = reading.name.as_deref()
    {
        // The line without what stands in its last brackets: a tag, also one the recognition misread (`(Milf2/A)`).
        let base = text.rfind('(').map_or(text, |open| &text[..open]);
        let letters = |s: &str| plain(s).chars().filter(|c| c.is_alphanumeric()).count();
        // A nickname in quotes on the card (`TBF-4 "Balefire" Combat Knife`) that the name lacks (`TBF-4 Combat
        // Knife`) makes it another item.
        let name_plain = plain(name);
        let nickname_lost = quoted_words(base).iter().any(|word| !name_plain.contains(word.as_str()));
        if letters(name) * 10 < letters(base) * NAME_COVER || nickname_lost {
            reading.certainty = Certainty::Likely;
        }
    }
    reading
}

/// The words in quotes in a line (`"Balefire"`, `“Sawbuck”`), plain, each at least three letters long.
fn quoted_words(line: &str) -> Vec<String> {
    let quotes = ['"', '“', '”', '„', '«', '»'];
    line.split(|c| quotes.contains(&c))
        .skip(1)
        .step_by(2)
        .map(plain)
        .filter(|w| w.chars().filter(|c| c.is_alphanumeric()).count() >= 3)
        .collect()
}

/// How much of a line read (without its tag) a sure name must explain at least, in tenths of its letters.
const NAME_COVER: usize = 7;

/// The classes a tag behind a name begins with, as the recognition misreads them, with the class meant.
const TAG_CLASSES: [(&str, &str); 10] = [
    ("mil", "Mil"),
    ("mii", "Mil"),
    ("mi1", "Mil"),
    ("mi|", "Mil"),
    ("civ", "Civ"),
    ("ind", "Ind"),
    ("lnd", "Ind"),
    ("sth", "Sth"),
    ("stl", "Stl"),
    ("cmp", "Cmp"),
];

/// A line whose tag behind the name the recognition misread (`(Mi112/A)`, `(Milf2/A)`, `(MillilB)`) with the tag
/// written as meant (`(Mil/2/A)`, `(Mil/1/B)`): a class of [`TAG_CLASSES`], then strokes the recognition makes of the
/// slashes and at most one digit, then the grade. The size is that digit, or 1 where only strokes stand there.
/// `None` where the line ends in no such tag.
fn mend_tag(line: &str) -> Option<String> {
    let trimmed = line.trim_end();
    let inside = trimmed.strip_suffix(')')?;
    let open = inside.rfind('(')?;
    let tag: String = inside[open + 1..].chars().filter(|c| !c.is_whitespace()).collect();
    let low = tag.to_lowercase();
    let (_, class) = TAG_CLASSES.iter().find(|(read, _)| low.starts_with(read))?;
    let rest: Vec<char> = tag.chars().skip(3).collect();
    let (&grade, middle) = rest.split_last()?;
    let grade = grade.to_ascii_uppercase();
    if !('A'..='D').contains(&grade) {
        return None;
    }
    let stroke = |c: &char| matches!(c, '/' | '\\' | '1' | 'l' | 'i' | 'I' | '|' | '!' | 'f');
    let digits: Vec<char> = middle.iter().copied().filter(|c| ('2'..='9').contains(c)).collect();
    let size = match digits[..] {
        [digit] if middle.iter().all(|c| *c == digit || stroke(c)) => digit,
        [] if !middle.is_empty() && middle.iter().all(stroke) => '1',
        _ => return None,
    };
    Some(format!("{}({class}/{size}/{grade})", &inside[..open]))
}

/// [`match_name`] for the line as it was read.
fn match_name_as_read(text: &str, names: &Names) -> Reading {
    let (base, tag) = split_tag(text);
    let mut whole = crate::blueprint_scan::match_line(text, names);
    if whole.certainty < Certainty::Sure
        && let Some(name) = by_numeral(&base, &whole.candidates)
    {
        whole = Reading {
            certainty: Certainty::Sure,
            name: Some(name.clone()),
            candidates: vec![(name, 1.0)],
            ..whole
        };
    }
    // Brackets that belong to the name (`Titanium (Ore)`) are no tag.
    if tag.is_none() || whole.certainty == Certainty::Sure {
        return whole;
    }
    let Some(tag) = tag else { return whole };
    let mut reading = crate::blueprint_scan::match_line(&base, names);
    reading.text = text.to_string();
    let Some(entry) = reading.name.as_deref().and_then(|name| names.entry(name)) else { return reading };
    if entry.class.is_none() && entry.size.is_none() && entry.grade.is_none() {
        return reading;
    }
    reading.tag = check_tag(Some(&tag), entry);
    if reading.tag == TagCheck::Contradicts {
        reading.certainty = reading.certainty.min(Certainty::Likely);
    }
    reading
}

/// The roman numerals names of the game tell their sizes apart with (`Deadbolt V Cannon`, `Deadbolt VI Cannon`).
const NUMERALS: [&str; 10] = ["i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x"];

/// Of names in question that differ only in a roman numeral, the one whose numeral the line shows exactly, the other
/// words of the line at most one character off theirs (`Oeadbolt V Cannon` is `Deadbolt V Cannon`, not `Deadbolt IV
/// Cannon`). `None` where the names differ otherwise or the line shows no numeral of one of them.
fn by_numeral(line: &str, candidates: &[(String, f64)]) -> Option<String> {
    if candidates.len() < 2 {
        return None;
    }
    let read: Vec<String> = plain(line).split(' ').map(str::to_string).collect();
    let words: Vec<Vec<String>> =
        candidates.iter().map(|(name, _)| plain(name).split(' ').map(str::to_string).collect()).collect();
    if words.iter().any(|w| w.len() != read.len()) {
        return None;
    }
    // The one place where the names differ.
    let differ: Vec<usize> = (0..read.len()).filter(|&at| words.iter().any(|w| w[at] != words[0][at])).collect();
    let [at] = differ[..] else { return None };
    if !words.iter().all(|w| NUMERALS.contains(&w[at].as_str())) {
        return None;
    }
    let pick = words.iter().position(|w| w[at] == read[at])?;
    let near = (0..read.len()).all(|i| i == at || one_off(&read[i], &words[pick][i]));
    near.then(|| candidates[pick].0.clone())
}

/// Whether two words are the same but for at most one character changed, added or left out.
fn one_off(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    match long.len() - short.len() {
        0 => a.iter().zip(&b).filter(|(x, y)| x != y).count() == 1,
        1 => (0..long.len()).any(|skip| long.iter().enumerate().filter(|(i, _)| *i != skip).map(|(_, c)| c).eq(short.iter())),
        _ => false,
    }
}

/// Reads the lines of one reading of a card: the name from the lines above the first label ([`is_label`]), alone
/// and joined with the following ones, the surest match winning and of equal ones the longer; a text key in place
/// of the name through `texts`; the line of goods, the capacity and the ammunition.
pub fn read_lines(lines: &[(String, Frame)], names: &Names, texts: Option<&GameTexts>) -> CardReading {
    let name_lines: Vec<&(String, Frame)> =
        lines.iter().take_while(|(text, _)| !is_label(text)).take(NAME_LINES).collect();
    let mut best: Option<(Reading, usize)> = None;
    let mut joined = String::new();
    for (count, (text, _)) in name_lines.iter().enumerate() {
        joined = if count == 0 { text.clone() } else { crate::blueprint_scan::joined(&joined, text) };
        let reading = match_name(&joined, names);
        if best.as_ref().is_none_or(|(b, _)| reading.certainty >= b.certainty && reading.certainty > Certainty::None) {
            best = Some((reading, count + 1));
        }
    }
    let (mut reading, mut used) = best.unwrap_or_else(|| (none_reading(""), 0));
    // A name read whole that longer names begin with (`Titanium`, `Titanium (Ore)`) is the whole name where every
    // line above the first label went into it: the card ends the name there.
    if used > 0 && used == name_lines.len() && name_lines.len() < lines.len() && whole_but_shorter(&reading) {
        reading.certainty = Certainty::Sure;
    }
    // A name the stock does not know for sure, read exactly as the game names an item: the English name of the game,
    // as the stock journal names an item; the name of the stock where it knows that one for sure.
    if reading.certainty < Certainty::Sure
        && let Some(texts) = texts
    {
        let mut shown = String::new();
        for (count, (text, _)) in name_lines.iter().enumerate() {
            shown = if count == 0 { text.clone() } else { crate::blueprint_scan::joined(&shown, text) };
            let Some((_, english)) = texts.item_name(&shown) else { continue };
            let by_stock = match_name(english, names);
            reading = if by_stock.certainty == Certainty::Sure {
                by_stock
            } else {
                Reading {
                    certainty: Certainty::Sure,
                    how: How::Exact,
                    name: Some(english.to_string()),
                    candidates: vec![(english.to_string(), 1.0)],
                    ..none_reading(english)
                }
            };
            reading.text = shown.clone();
            used = count + 1;
        }
    }
    // A line above the first label left out of the name is a part of it read wrong (`Fresnel Energy LMG` sure, the
    // line below it `Battery cap)` of `… Battery (165 Cap)` lost): the name is in question.
    if used < name_lines.len() && reading.certainty == Certainty::Sure {
        reading.certainty = Certainty::Likely;
    }
    let together: String = name_lines.iter().map(|(text, _)| text.as_str()).collect::<Vec<_>>().join("");
    let broken = broken_key(&together);
    let mut key = None;
    if let Some(read) = &broken {
        match texts.and_then(|t| t.resolve(read)) {
            Some((found, text, exact)) => {
                key = Some(found.to_string());
                let mut by_text = match_name(text, names);
                if by_text.certainty == Certainty::None {
                    by_text = Reading {
                        certainty: Certainty::Likely,
                        how: How::Unknown,
                        name: Some(text.to_string()),
                        candidates: vec![(text.to_string(), 1.0)],
                        ..none_reading(text)
                    };
                }
                if !exact {
                    by_text.certainty = by_text.certainty.min(Certainty::Likely);
                }
                by_text.text = together.clone();
                reading = by_text;
            }
            None => reading = none_reading(&together),
        }
    }
    // Every line above the first label, also one left out of the name: read again alone, it must not lose it.
    let name_frame = name_lines
        .iter()
        .map(|(_, f)| *f)
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)));
    let rest = &lines[name_lines.len()..];
    let capacity = rest.iter().find_map(|(text, _)| capacity_of(text));
    // An amount above the capacity of the box was read wrong.
    let goods = rest
        .iter()
        .find_map(|(text, _)| goods_line(text))
        .filter(|(_, amount)| capacity.flatten().is_none_or(|most| *amount <= most + 0.0005));
    // The line of goods, or where it stands when it could not be read: below the capacity.
    let capacity_at = rest.iter().position(|(text, _)| capacity_of(text).is_some());
    let goods_frame = rest
        .iter()
        .find(|(text, _)| goods_line(text).is_some())
        .or_else(|| capacity_at.and_then(|at| rest.get(at + 1)))
        .map(|(_, frame)| *frame);
    CardReading {
        goods_frame,
        capacity_value: capacity.flatten(),
        tag: split_tag(&reading.text).1.is_some(),
        reading,
        broken,
        key,
        goods,
        capacity: capacity.is_some(),
        ammo: rest.iter().any(|(text, _)| is_ammo_line(text)),
        labelled: !name_lines.is_empty() && rest.first().is_some_and(|(text, _)| is_label(text)),
        lines: lines.iter().map(|(text, _)| text.clone()).collect(),
        name_frame,
    }
}

/// Whether a reading is a name read exactly (or with mistakable characters) and held back only because longer names
/// begin with it: no tag against it and no other name in question but the longer ones.
fn whole_but_shorter(reading: &Reading) -> bool {
    let Some(name) = reading.name.as_deref() else { return false };
    let start = format!("{} ", plain(name));
    reading.certainty == Certainty::Likely
        && matches!(reading.how, How::Exact | How::Lookalike)
        && reading.tag != TagCheck::Contradicts
        && reading.candidates.iter().all(|(other, _)| other == name || plain(other).starts_with(&start))
}

/// A reading of nothing.
fn none_reading(text: &str) -> Reading {
    Reading {
        text: text.to_string(),
        certainty: Certainty::None,
        how: How::Unknown,
        name: None,
        candidates: Vec::new(),
        tag: TagCheck::Unread,
    }
}

// ------------------------------------------------------------ The count on the tile

/// The part of the screen where the count of the tile under the pointer stands: the count is written at the top
/// right of the tile, so the part reaches a tile's height up and a tile's width right; cut to the screen.
pub fn count_area(pointer: (i64, i64), size: (i64, i64)) -> Area {
    let unit = size.1 as f64;
    let left = ((pointer.0 as f64 - 0.03 * unit) as i64).max(0);
    let top = ((pointer.1 as f64 - 0.2 * unit) as i64).max(0);
    let right = ((pointer.0 as f64 + 0.2 * unit) as i64).min(size.0);
    let bottom = ((pointer.1 as f64 + 0.03 * unit) as i64).min(size.1);
    (left, top, (right - left).max(0), (bottom - top).max(0))
}

/// The count of the tile under the pointer among the words of one reading: of the counts ([`count_of`]) above the
/// pointer and not ending left of it, the row nearest above it, and in that row the leftmost (the tile right of
/// it has its count further right). The row must lie within a tile's height above the pointer: the distance of two
/// counts in the row where it has two, else [`TILE_REACH`] heights of the screen; a tile without a count shows none
/// of the tile above it.
pub fn count_at(words: &[Word], pointer: (f64, f64), unit: f64) -> Option<u32> {
    let counts: Vec<(Frame, u32)> =
        words.iter().filter_map(|w| count_of(&w.t).map(|n| ((w.x, w.y, w.x + w.w, w.y + w.h), n))).collect();
    let places: Vec<Frame> = counts.iter().map(|c| c.0).collect();
    tile_count_place(&places, pointer, unit).map(|at| counts[at].1)
}

/// Of the places (left, top, right, bottom) of counts, the one of the tile under the pointer, as [`count_at`] picks
/// it, with the distance of two counts in its row where it has two.
fn tile_count_place(places: &[Frame], pointer: (f64, f64), unit: f64) -> Option<usize> {
    tile_count_row(places, pointer, unit, (TILE_REACH, 1.1)).map(|(at, _)| at)
}

/// [`tile_count_place`] with the distance of two counts in the row, where it has two; `reach` is how far above the
/// pointer a count may stand, in heights of the screen where the row shows no second count, else in distances of
/// two counts.
fn tile_count_row(places: &[Frame], pointer: (f64, f64), unit: f64, reach: (f64, f64)) -> Option<(usize, Option<f64>)> {
    let middle = |f: &Frame| (f.1 + f.3) / 2.0;
    let all: Vec<usize> = (0..places.len()).filter(|&i| middle(&places[i]) <= pointer.1 + 0.03 * unit).collect();
    let row = all.iter().filter(|&&i| places[i].2 >= pointer.0).map(|&i| middle(&places[i])).fold(f64::MIN, f64::max);
    let in_row: Vec<usize> = all.iter().copied().filter(|&i| (middle(&places[i]) - row).abs() <= 0.02 * unit).collect();
    let mut xs: Vec<f64> = in_row.iter().map(|&i| places[i].0).collect();
    xs.sort_by(f64::total_cmp);
    let pitch = xs.windows(2).map(|p| p[1] - p[0]).filter(|d| *d > 0.02 * unit).fold(f64::MAX, f64::min);
    let pitch = (pitch < f64::MAX).then_some(pitch);
    let reach = pitch.map_or(reach.0 * unit, |p| p * reach.1);
    if pointer.1 - row > reach {
        return None;
    }
    in_row
        .into_iter()
        .filter(|&i| places[i].2 >= pointer.0)
        .min_by(|&a, &b| places[a].0.total_cmp(&places[b].0))
        .map(|at| (at, pitch))
}

/// The box of the count of the tile under the pointer among `boxes` ([`count_boxes`]), picked as [`count_at`]
/// picks a count, with the distance of two boxes in its row where it has two.
pub fn count_box_at(boxes: &[Area], pointer: (f64, f64), unit: f64) -> Option<(Area, Option<f64>)> {
    let places: Vec<Frame> =
        boxes.iter().map(|b| (b.0 as f64, b.1 as f64, (b.0 + b.2) as f64, (b.1 + b.3) as f64)).collect();
    tile_count_row(&places, pointer, unit, (BOX_REACH, 0.97)).map(|(at, pitch)| (boxes[at], pitch))
}

/// The tile of a count box where its frame was not found: the box stands at its top right, the tile is as wide as
/// the distance of two boxes in the row ([`TILE_PITCH`] heights of the screen where the row shows one box) less the
/// gap between tiles, and a little less high than wide.
pub fn tile_of_box(b: Area, pitch: Option<f64>, height: i64) -> Area {
    let unit = height as f64;
    let pitch = pitch.unwrap_or(TILE_PITCH * unit);
    let (width, tall) = ((pitch * 0.94) as i64, (pitch * 0.86) as i64);
    let right = b.0 + b.2 + (0.004 * unit) as i64;
    let top = b.1 - (0.008 * unit) as i64;
    (right - width, top, width, tall)
}

/// The distance of two tiles of the freight manager, in heights of the screen.
pub const TILE_PITCH: f64 = 0.17;

/// How far above the pointer the box of its count may stand where the row shows no second box, in heights of the
/// screen: a tile of the freight manager high, so that the box of the tile above is never taken for it.
pub const BOX_REACH: f64 = 0.165;

/// Whether the writing of a count box reads `x1`: two marks, an x about as wide as high and right of it a narrow
/// stroke at least four fifths as high as the x (the game writes `X1`, its X as high as the 1). Any other writing is
/// left to the recognition.
pub fn box_reads_one(marks: &[Area], b: Area) -> bool {
    let [x, one] = marks else { return false };
    let x_ratio = x.2 as f64 / x.3.max(1) as f64;
    (0.6..=1.4).contains(&x_ratio)
        && one.3 * 5 >= x.3 * 4
        && one.3 * 100 >= b.3 * 35
        && one.2 * 100 <= one.3 * 46
        && one.0 > x.0 + x.2
}

/// The part of a count box (or a box of the ammunition) its writing stands in: the box less its edge, a seventh of
/// its height at the sides (the writing begins close to the edge) and a sixth at the top and bottom (the rounded
/// corners).
pub fn box_writing(b: Area) -> Area {
    let (ix, iy) = ((b.3 / 7).max(2), (b.3 / 6).max(1));
    (b.0 + ix, b.1 + iy, b.2 - 2 * ix, b.3 - 2 * iy)
}

/// The count in the box `b` read by the shape of its digits ([`glyphs`]): `x` and the number behind it, where every
/// digit is sure. `None` for anything else.
pub fn shaped_count(bitmap: &Bitmap, b: Area) -> Option<u32> {
    shaped_count_in(glyphs::font("box")?, bitmap, b)
}

/// [`shaped_count`] with the patterns of `font`.
pub fn shaped_count_in(font: &glyphs::Font, bitmap: &Bitmap, b: Area) -> Option<u32> {
    let text = glyphs::read(font, &glyphs::Ink::of(bitmap, box_writing(b), true)?)?;
    let digits = text.strip_prefix('x')?;
    if digits.is_empty() || digits.len() > 4 || !digits.chars().all(|c| c.is_ascii_digit()) || digits.starts_with('0') {
        return None;
    }
    digits.parse().ok()
}

/// How far around the field of the quality its writing is read, in pixels: a little of the ground of the card on
/// both sides, so that writing cut off at an edge of the field shows.
const FIELD_PAD: i64 = 2;

/// The quality in its field (`field`, [`quality_field`]) read by the shape of its digits ([`glyphs`]), where every
/// digit is sure and the writing stands whole in the field; `None` for anything else.
pub fn shaped_quality(bitmap: &Bitmap, field: Area) -> Option<u32> {
    shaped_quality_in(glyphs::font("quality")?, bitmap, field)
}

/// [`shaped_quality`] with the patterns of `font`.
pub fn shaped_quality_in(font: &glyphs::Font, bitmap: &Bitmap, field: Area) -> Option<u32> {
    let part = (field.0 - FIELD_PAD, field.1, field.2 + 2 * FIELD_PAD, field.3);
    quality_of(&glyphs::read_whole(font, &glyphs::Ink::of(bitmap, part, false)?)?)
}

/// The quality in its field by the shapes of its digits, every best pattern leading by a smaller lead than
/// [`shaped_quality`] needs ([`glyphs::read_whole_confirming`]), as a check of what the recognition of Windows read
/// (`read`): where it read anything, the shapes give their number, the same (it is confirmed) or another (the card is
/// in question: both readers mistake a `6` for an `8` now and then, alike every time). `None` where the recognition
/// read nothing (too weak a lead to stand alone) or the shapes give no number.
pub fn checked_quality(bitmap: &Bitmap, field: Area, read: &[u32]) -> Option<u32> {
    checked_quality_in(glyphs::font("quality")?, bitmap, field, read)
}

/// [`checked_quality`] with the patterns of `font`.
pub fn checked_quality_in(font: &glyphs::Font, bitmap: &Bitmap, field: Area, read: &[u32]) -> Option<u32> {
    if read.is_empty() {
        return None;
    }
    let part = (field.0 - FIELD_PAD, field.1, field.2 + 2 * FIELD_PAD, field.3);
    quality_of(&glyphs::read_whole_confirming(font, &glyphs::Ink::of(bitmap, part, false)?)?)
}

/// The amount in its field (`field`, [`amount_field`]) read by the shape of its characters ([`glyphs::read_number`],
/// the font `amount`): the number in front of its unit, where every character is sure; `None` for anything else.
pub fn shaped_amount(bitmap: &Bitmap, field: Area) -> Option<f64> {
    let font = glyphs::font("amount")?;
    let part = (field.0 - FIELD_PAD, field.1, field.2 + 2 * FIELD_PAD, field.3);
    amount_of(&glyphs::read_number(font, &glyphs::Ink::of(bitmap, part, false)?)?)
}

/// The amount in its field by the shapes of its characters with the smaller lead, as a check of what the
/// recognition of Windows read (`read`), as [`checked_quality`] does for the quality.
pub fn checked_amount(bitmap: &Bitmap, field: Area, read: &[f64]) -> Option<f64> {
    if read.is_empty() {
        return None;
    }
    let font = glyphs::font("amount")?;
    let part = (field.0 - FIELD_PAD, field.1, field.2 + 2 * FIELD_PAD, field.3);
    amount_of(&glyphs::read_number_confirming(font, &glyphs::Ink::of(bitmap, part, false)?)?)
}

/// The number of the amount and of the quality in their fields by the best pattern of every character, however
/// little it leads (`None` where a character fits no pattern well or there is no field): not a reading, only what
/// the shapes speak for ([`against_shapes`]).
pub fn best_goods(bitmap: &Bitmap, amount: Option<Area>, quality: Option<Area>) -> (Option<f64>, Option<u32>) {
    let pad = |f: Area| (f.0 - FIELD_PAD, f.1, f.2 + 2 * FIELD_PAD, f.3);
    let amount = amount.and_then(|f| {
        amount_of(&glyphs::read_number_best(glyphs::font("amount")?, &glyphs::Ink::of(bitmap, pad(f), false)?)?)
    });
    let quality = quality.and_then(|f| {
        quality_of(&glyphs::read_whole_best(glyphs::font("quality")?, &glyphs::Ink::of(bitmap, pad(f), false)?)?)
    });
    (amount, quality)
}

/// The outcome of a card of goods put in question where the shapes of the digits (`amount`, `quality`: the numbers
/// their best patterns give, [`best_goods`]) give another number than the readings agreed on: however often the
/// recognition read its number, it may have mistaken the same digit alike every time (a `6` for an `8`).
pub fn against_shapes(outcome: &mut CardOutcome, amount: Option<f64>, quality: Option<u32>) {
    if outcome.unit != Unit::Scu {
        return;
    }
    let other_amount = amount.is_some_and(|a| (a - outcome.quantity).abs() >= 0.0005);
    let other_quality = quality.is_some_and(|q| outcome.quality != Some(q));
    if other_amount || other_quality {
        outcome.certainty = outcome.certainty.min(Certainty::Likely);
    }
}

/// The most a box of goods holds, in SCU: an amount read above it is none.
pub const MOST_IN_A_BOX: f64 = 32.0;

/// The amount written with the digits `digits` (without its point, [`glyphs::read_number`]): the game writes an
/// amount below one with three places (`0.509`, `0.020`), a larger one whole (`16`). So `0` and three digits is a
/// part of one, any other `0` in front none; a whole amount above [`MOST_IN_A_BOX`] none (`1.500` read as `1500`).
fn amount_of(digits: &str) -> Option<f64> {
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let amount: f64 = match digits.strip_prefix('0') {
        Some(part) if part.len() == 3 => format!("0.{part}").parse().ok()?,
        Some(_) => return None,
        None => digits.parse().ok()?,
    };
    (amount > 0.0 && amount <= MOST_IN_A_BOX).then_some(amount)
}

/// The quality written as `text`: one to four digits, no `0` in front of others (a lone `0` is the quality of
/// gold that is none), at most [`MAX_QUALITY`].
fn quality_of(text: &str) -> Option<u32> {
    if text.is_empty()
        || text.len() > 4
        || !text.chars().all(|c| c.is_ascii_digit())
        || (text.starts_with('0') && text.len() > 1)
    {
        return None;
    }
    text.parse().ok().filter(|q| *q <= MAX_QUALITY)
}

/// The marks of the writing in a count box: its dark pixels (away from the rounded corners) joined into marks, left
/// to right, each as its frame. Marks of fewer than three pixels are left out.
pub fn count_marks(bitmap: &Bitmap, b: Area) -> Vec<Area> {
    let (mut sum, mut n) = (0i64, 0i64);
    for y in b.1..b.1 + b.3 {
        for x in b.0..b.0 + b.2 {
            sum += i64::from(brightness(&bitmap.raw, bitmap.width, x, y));
            n += 1;
        }
    }
    let mean = sum / n.max(1);
    let (ix, iy) = ((b.2 / 6).max(1), (b.3 / 6).max(1));
    let (x0, y0, w, h) = (b.0 + ix, b.1 + iy, (b.2 - 2 * ix).max(0), (b.3 - 2 * iy).max(0));
    let dark: Vec<bool> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| i64::from(brightness(&bitmap.raw, bitmap.width, x0 + x, y0 + y)) < mean - BOX_WRITING)
        .collect();
    let mut seen = vec![false; dark.len()];
    let mut marks = Vec::new();
    for start in 0..dark.len() {
        if !dark[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let (mut l, mut t, mut r, mut d, mut count) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN, 0);
        while let Some(at) = stack.pop() {
            count += 1;
            let (x, y) = (at as i64 % w, at as i64 / w);
            (l, t, r, d) = (l.min(x), t.min(y), r.max(x), d.max(y));
            for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                let next = (ny * w + nx) as usize;
                if dark[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if count >= 3 {
            marks.push((x0 + l, y0 + t, r - l + 1, d - t + 1));
        }
    }
    marks.sort_by_key(|m| m.0);
    marks
}

/// The boxes of counts in `area`: light filled boxes of the size of a count on a tile (the freight manager writes the
/// count dark into them). Each as its frame.
pub fn count_boxes(bitmap: &Bitmap, area: Area) -> Vec<Area> {
    light_boxes(bitmap, area, (0.04, 1.75))
}

/// Light filled boxes in `area` as [`count_boxes`] finds them, at most `widest.0` heights of the screen wide and
/// `widest.1` times as wide as high (the box of the ammunition of a tile, `369 / 369`, is wider than a count box).
pub fn light_boxes(bitmap: &Bitmap, area: Area, widest: (f64, f64)) -> Vec<Area> {
    let unit = bitmap.height as f64;
    let (x0, y0) = (area.0.max(0), area.1.max(0));
    let (x1, y1) = ((area.0 + area.2).min(bitmap.width), (area.1 + area.3).min(bitmap.height));
    if x1 <= x0 || y1 <= y0 {
        return Vec::new();
    }
    let width = (x1 - x0) as usize;
    // Light against the ground of the part: the terminal is lit more or less brightly.
    let mut sample: Vec<i32> = (y0..y1)
        .step_by(3)
        .flat_map(|y| (x0..x1).step_by(3).map(move |x| (x, y)))
        .map(|(x, y)| brightness(&bitmap.raw, bitmap.width, x, y))
        .collect();
    sample.sort_unstable();
    let ground = sample.get(sample.len() / 2).copied().unwrap_or(0);
    let threshold = (ground + BOX_LIFT).max(BOX_LIGHT);
    let bright: Vec<bool> = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .map(|(x, y)| brightness(&bitmap.raw, bitmap.width, x, y) >= threshold)
        .collect();
    // Only light areas, no thin lines: the box of the tile under the pointer touches the frame of the tile.
    let rows = (y1 - y0) as usize;
    let light: Vec<bool> = (0..bright.len())
        .map(|at| {
            if !bright[at] {
                return false;
            }
            let (x, y) = (at % width, at / width);
            let mut around = 0;
            for ny in y.saturating_sub(2)..(y + 3).min(rows) {
                for nx in x.saturating_sub(2)..(x + 3).min(width) {
                    around += usize::from(bright[ny * width + nx]);
                }
            }
            around >= 15
        })
        .collect();
    let mut seen = vec![false; light.len()];
    let mut out = Vec::new();
    for start in 0..light.len() {
        if !light[start] || seen[start] {
            continue;
        }
        // The light pixels joined to this one, and their frame.
        let (mut stack, mut count) = (vec![start], 0usize);
        seen[start] = true;
        let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
        while let Some(at) = stack.pop() {
            count += 1;
            let (x, y) = (at % width, at / width);
            (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));
            let mut push = |next: usize| {
                if light[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            };
            if x > 0 {
                push(at - 1);
            }
            if x + 1 < width {
                push(at + 1);
            }
            if at >= width {
                push(at - width);
            }
            if at + width < light.len() {
                push(at + width);
            }
        }
        let (w, h) = ((right - left + 1) as f64, (bottom - top + 1) as f64);
        // A filled box: most of its frame light (the writing is dark), as wide as a count.
        let fits = (0.016 * unit..=widest.0 * unit).contains(&w)
            && (0.013 * unit..=0.025 * unit).contains(&h)
            && w > h
            && w <= widest.1 * h
            && count as f64 >= 0.5 * w * h;
        if fits {
            // The edge pixels left out with the thin lines put back.
            out.push((x0 + left as i64 - 1, y0 + top as i64 - 1, w as i64 + 2, h as i64 + 2));
        }
    }
    out
}

/// How bright the box of a count is at least.
pub const BOX_LIGHT: i32 = 80;

/// How much brighter than the ground around it (the middle brightness of the part searched) the box of a count is.
pub const BOX_LIFT: i32 = 35;

/// The height of the writing of a box in [`count_strip`], in pixels.
pub const STRIP_HEIGHT: i64 = 64;

/// The boxes of counts drawn anew for the recognition: each box's dark writing black on white, enlarged to
/// [`STRIP_HEIGHT`], side by side with white between. With the strip, for each box drawn its index in `boxes` and
/// where it begins on the strip.
pub fn count_strip(bitmap: &Bitmap, boxes: &[Area]) -> Option<(Bitmap, Vec<(usize, i64)>)> {
    count_strip_of(bitmap, boxes, false)
}

/// [`count_strip`] for writing `light` on a dark ground (the count of a stack in the inventory) where `light` is
/// set: the brightness turned round, the writing comes out black on white all the same.
pub fn count_strip_of(bitmap: &Bitmap, boxes: &[Area], light: bool) -> Option<(Bitmap, Vec<(usize, i64)>)> {
    if boxes.is_empty() {
        return None;
    }
    let value = |x: i64, y: i64| {
        let v = brightness(&bitmap.raw, bitmap.width, x.clamp(0, bitmap.width - 1), y.clamp(0, bitmap.height - 1));
        if light { 255 - v } else { v }
    };
    // The writing of each box: its dark pixels away from the rounded corners, and their frame.
    let writing: Vec<(usize, Area, (i64, i64))> = boxes
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let (mut sum, mut n) = (0i64, 0i64);
            for y in b.1..b.1 + b.3 {
                for x in b.0..b.0 + b.2 {
                    sum += i64::from(value(x, y));
                    n += 1;
                }
            }
            let mean = sum / n.max(1);
            let (ix, iy) = ((b.2 / 6).max(1), (b.3 / 6).max(1));
            let (mut l, mut t, mut r, mut d) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
            let mut darkest = mean;
            for y in b.1 + iy..b.1 + b.3 - iy {
                for x in b.0 + ix..b.0 + b.2 - ix {
                    let v = i64::from(value(x, y));
                    if v < mean - BOX_WRITING {
                        (l, t, r, d) = (l.min(x), t.min(y), r.max(x), d.max(y));
                        darkest = darkest.min(v);
                    }
                }
            }
            (r >= l && d >= t).then_some((i, (l, t, r - l + 1, d - t + 1), (mean, darkest)))
        })
        .collect();
    if writing.is_empty() {
        return None;
    }
    let scale = |b: &Area| STRIP_HEIGHT as f64 / b.3.max(1) as f64;
    let margin = STRIP_HEIGHT;
    let width: i64 = writing
        .iter()
        .map(|(_, b, _)| ((b.2 as f64 * scale(b)) as i64 + STRIP_GAP) * STRIP_REPEAT + margin)
        .sum::<i64>()
        + margin;
    let height = STRIP_HEIGHT + 2 * margin;
    let mut raw = vec![255u8; (width * height * 4) as usize];
    let mut starts = Vec::new();
    let mut left = margin;
    for (i, b, (mean, darkest)) in &writing {
        let s = scale(b);
        let w = (b.2 as f64 * s) as i64;
        // Smoothly enlarged: how much darker than the box, from white (as the box) to black (as its darkest pixel).
        let at_point = |fx: f64, fy: f64| {
            let (x, y) = (fx.floor() as i64, fy.floor() as i64);
            let (tx, ty) = (fx - x as f64, fy - y as f64);
            let get = |x: i64, y: i64| f64::from(value(x, y));
            let top = get(x, y) * (1.0 - tx) + get(x + 1, y) * tx;
            let bottom = get(x, y + 1) * (1.0 - tx) + get(x + 1, y + 1) * tx;
            top * (1.0 - ty) + bottom * ty
        };
        let pad = 0.5;
        starts.push((*i, left));
        // The writing several times in a row: the recognition reads a short word alone badly, a row of them well.
        for _ in 0..STRIP_REPEAT {
            for dy in 0..STRIP_HEIGHT {
                for dx in 0..w {
                    let fx = b.0 as f64 - pad + dx as f64 / s;
                    let fy = b.1 as f64 - pad + dy as f64 / s;
                    let dark = (*mean as f64 - at_point(fx, fy)) / ((mean - darkest).max(1) as f64 * 0.8);
                    let v = (255.0 * (1.0 - dark.clamp(0.0, 1.0))) as u8;
                    let at = (((margin + dy) * width + left + dx) * 4) as usize;
                    raw[at..at + 3].copy_from_slice(&[v, v, v]);
                }
            }
            left += w + STRIP_GAP;
        }
        left += margin;
    }
    Some((Bitmap { width, height, raw }, starts))
}

/// How often the writing of each box stands on the strip.
const STRIP_REPEAT: i64 = 3;

/// The space between two copies of the writing on the strip, in pixels.
const STRIP_GAP: i64 = STRIP_HEIGHT / 2;

/// The count in what the recognition read of one box on the strip: the number behind an x that most of its copies
/// give, at least two of them.
pub fn strip_count(text: &str) -> Option<u32> {
    let chars: Vec<char> = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut found: Vec<u32> = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if matches!(chars[at], 'x' | 'X' | '×' | '*') {
            let digits: String = chars[at + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit() || matches!(c, 'O' | 'o' | 'l' | 'I' | '|' | ']'))
                .collect();
            if let Some(n) = count_of(&format!("x{digits}")) {
                found.push(n);
            }
            at += digits.chars().count();
        }
        at += 1;
    }
    let (n, times) = agreed(&found, |a, b| a == b)?;
    (times >= 2).then_some(n)
}

/// The count of a stack in what the recognition read of its writing on a strip ([`stacked_count`]): the number in
/// front of an x (`86x`) that most of its copies give, at least two of them. A number with anything else glued to
/// its front (`Æ6x`, a digit read wrong) counts for nothing.
pub fn stack_strip_count(text: &str) -> Option<u32> {
    let chars: Vec<char> = text.chars().collect();
    let mut found: Vec<u32> = Vec::new();
    // Where the digits in front of the next x began, where they began after a blank or an x.
    let mut start: Option<usize> = None;
    let mut clean = true;
    for (at, c) in chars.iter().enumerate() {
        if c.is_ascii_digit() {
            if start.is_none() && clean {
                start = Some(at);
            }
        } else if matches!(c, 'x' | 'X' | '×') {
            if let Some(from) = start {
                let digits: String = chars[from..at].iter().collect();
                if let Some(n) = count_of(&format!("{digits}x")) {
                    found.push(n);
                }
            }
            (start, clean) = (None, true);
        } else {
            (start, clean) = (None, c.is_whitespace());
        }
    }
    let (n, times) = agreed(&found, |a, b| a == b)?;
    (times >= 2).then_some(n)
}

/// How much darker than its box the writing of a count is.
const BOX_WRITING: i64 = 25;

/// Whether a word read stands where a tile writes its count: its middle within the width of the tile (and a little
/// right of it) and in its upper half (and a little above it).
pub fn count_on_tile(tile: Area, word: &Word, height: i64) -> bool {
    let near = 0.01 * height as f64;
    let (x, y) = (word.x + word.w / 2.0, word.y + word.h / 2.0);
    let (left, top) = (tile.0 as f64, tile.1 as f64);
    x >= left && x <= left + tile.2 as f64 + near && y >= top - near && y <= top + tile.3 as f64 / 2.0
}

/// The part of a tile where the inventory writes the count of a stack: the right half of its upper quarter, inside
/// its frame (a sixteenth of the tile); the picture of the item begins below it.
pub fn stack_corner(tile: Area) -> Area {
    let margin = (tile.2.min(tile.3) / 16).max(2);
    let left = tile.0 + tile.2 / 2;
    (left, tile.1 + margin, tile.0 + tile.2 - margin - left, tile.3 / 4 - margin)
}

/// How much brighter than the middle of the corner a pixel of writing is.
pub const CORNER_LIFT: i32 = 60;

/// How many pixels of writing make writing in the corner.
pub const CORNER_WRITING: usize = 12;

/// Whether `corner` ([`stack_corner`]) holds writing: at least [`CORNER_WRITING`] pixels [`CORNER_LIFT`] brighter
/// than its middle brightness. A corner outside the screen holds none.
pub fn corner_written(bitmap: &Bitmap, corner: Area) -> bool {
    let (x0, y0) = (corner.0.max(0), corner.1.max(0));
    let (x1, y1) = ((corner.0 + corner.2).min(bitmap.width), (corner.1 + corner.3).min(bitmap.height));
    if x1 <= x0 || y1 <= y0 {
        return false;
    }
    let mut values: Vec<i32> =
        (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).map(|(x, y)| brightness(&bitmap.raw, bitmap.width, x, y)).collect();
    values.sort_unstable();
    let middle = values[values.len() / 2];
    values.iter().filter(|v| **v >= middle + CORNER_LIFT).count() >= CORNER_WRITING
}

/// The readings of the corner of a tile where the inventory writes the count of a stack ([`stack_corner`]).
pub const STACK_PASSES: [(f64, Mode); 3] = [(3.0, Mode::Plain), (4.0, Mode::Plain), (3.0, Mode::Gray)];


/// How far above the pointer the count of its tile may stand where the row shows no second count, in heights of the
/// screen.
pub const TILE_REACH: f64 = 0.12;

// ------------------------------------------------------------ The outcome

/// How the stock counts what the card shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unit {
    /// Goods, in SCU.
    Scu,
    /// Items, in pieces.
    Pieces,
}

/// What kind of card it was, by its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardKind {
    /// Goods: a capacity and a line of goods in SCU.
    Goods,
    /// A component: a tag behind the name.
    Component,
    /// An item with ammunition (a magazine, a weapon).
    Ammo,
    /// Any other item.
    Item,
}

/// What a reading of the card at the pointer gives.
#[derive(Clone, Debug, PartialEq)]
pub struct CardOutcome {
    /// The name of the stock (English), for [`Certainty::Sure`] and [`Certainty::Likely`].
    pub name: Option<String>,
    /// The text key of the name, where the texts of the game have it.
    pub key: Option<String>,
    /// SCU for goods, pieces for items; 0 when it could not be read ([`CardOutcome::quantity_read`]).
    pub quantity: f64,
    pub unit: Unit,
    /// The quality of goods.
    pub quality: Option<u32>,
    pub certainty: Certainty,
    /// The names in question, best first.
    pub candidates: Vec<String>,
    /// The lines of the name as PNG, for the question to the player.
    pub picture: Option<Vec<u8>>,
    pub kind: CardKind,
    /// Whether the quantity was read.
    pub quantity_read: bool,
    /// The lines of the card as read, for a bug report.
    pub lines: Vec<String>,
    /// The frame of the card on the screen.
    pub frame: Option<Area>,
    /// The tile under the pointer on the screen, where its frame was found.
    pub tile_rect: Option<Area>,
    /// The print of that tile, to know it again after scrolling ([`TilePrint::same`]).
    pub tile_print: Option<TilePrint>,
}

impl CardOutcome {
    /// Nothing read.
    pub fn nothing() -> CardOutcome {
        CardOutcome {
            name: None,
            key: None,
            quantity: 0.0,
            unit: Unit::Pieces,
            quality: None,
            certainty: Certainty::None,
            candidates: Vec::new(),
            picture: None,
            kind: CardKind::Item,
            quantity_read: false,
            lines: Vec::new(),
            frame: None,
            tile_rect: None,
            tile_print: None,
        }
    }
}

/// The value most readings agree on, with how many gave it; numbers within a thousandth are the same.
fn agreed<T: Copy + PartialEq>(values: &[T], same: impl Fn(T, T) -> bool) -> Option<(T, usize)> {
    values.iter().map(|&v| (v, values.iter().filter(|&&o| same(v, o)).count())).max_by_key(|(_, n)| *n)
}

/// Whether `agree` of `total` readings giving the same value is enough to take it without asking: at least
/// [`crate::blueprint_scan::MIN_AGREE`] and all of them, or at least three and twice as many as all others.
fn enough(agree: usize, total: usize) -> bool {
    agree >= crate::blueprint_scan::MIN_AGREE && (agree == total || (agree >= 3 && agree >= 2 * (total - agree)))
}

/// Puts the readings of a card and the counts of the tile together. The name as [`crate::blueprint_scan::combine`]
/// does; the quantity is taken without asking only where at least [`crate::blueprint_scan::MIN_AGREE`] readings
/// give the same and none another, else the outcome is at most [`Certainty::Likely`]; nothing read leaves it 0.
/// With `none_is_one` an item without any count read is one piece (the inventory writes no count on a single item).
/// `more_goods` are readings of the line of goods alone, `more_qualities` readings of its field of the quality alone.
/// An item on a card where a line below the name looks like one of goods may be goods whose lines were not read
/// whole: it is asked about.
pub fn combine(
    readings: &[CardReading],
    counts: &[u32],
    none_is_one: bool,
    more_goods: &[(Option<u32>, f64)],
    more_qualities: &[u32],
) -> CardOutcome {
    let ones = [1, 1];
    let counts = if none_is_one && counts.is_empty() { &ones[..] } else { counts };
    // A name read only as the beginning of a longer one another reading read for sure (`SW16BR2` with the rest of
    // the line behind it, `SW16BR2 “Sawbuck” Repeater`) is that longer one, its last line lost.
    let sure: Vec<&str> = readings
        .iter()
        .filter(|r| r.reading.certainty == Certainty::Sure)
        .filter_map(|r| r.reading.name.as_deref())
        .collect();
    let names: Vec<Reading> = readings
        .iter()
        .map(|r| {
            let reading = r.reading.clone();
            let Some(short) = reading.name.as_deref().filter(|_| reading.certainty == Certainty::Likely) else {
                return reading;
            };
            let start = format!("{} ", plain(short));
            match sure.iter().find(|long| plain(long).starts_with(&start)) {
                Some(long) if reading.how == How::WithTag => {
                    Reading { name: Some(long.to_string()), candidates: vec![(long.to_string(), 1.0)], ..reading }
                }
                _ => reading,
            }
        })
        .collect();
    let outcome = crate::blueprint_scan::combine(&names);
    let goods = readings.iter().any(|r| r.goods.is_some() || r.capacity);
    let kind = if goods {
        CardKind::Goods
    } else if readings.iter().any(|r| r.ammo) {
        CardKind::Ammo
    } else if readings.iter().any(|r| r.tag && r.reading.certainty > Certainty::None) {
        CardKind::Component
    } else {
        CardKind::Item
    };
    let close = |a: f64, b: f64| (a - b).abs() < 0.0005;
    // The readings of the whole card mistake the small digits of the amount (`0.298` as `0.08`), the readings of the
    // line alone and large the quality on its grey ground (`588` as `50`): all of them count, a value is taken
    // without asking only by a clear majority ([`enough`]).
    let lines_of_goods: Vec<(Option<u32>, f64)> =
        readings.iter().filter_map(|r| r.goods).chain(more_goods.iter().copied()).collect();
    let (quantity, quantity_enough) = if goods {
        let amounts: Vec<f64> = lines_of_goods.iter().map(|g| g.1).collect();
        match agreed(&amounts, close) {
            Some((amount, n)) => (Some(amount), enough(n, amounts.len())),
            None => (None, false),
        }
    } else {
        match agreed(counts, |a, b| a == b) {
            Some((count, n)) => (Some(f64::from(count)), enough(n, counts.len())),
            None => (None, false),
        }
    };
    let qualities: Vec<u32> =
        lines_of_goods.iter().filter_map(|g| g.0).chain(more_qualities.iter().copied()).collect();
    let (quality, quality_enough) = match agreed(&qualities, |a, b| a == b) {
        Some((q, n)) => (Some(q), enough(n, qualities.len())),
        None => (None, false),
    };
    let mut certainty = outcome.certainty;
    // Goods always show their quality: one not read is a question.
    let goods_unread = !goods
        && readings.iter().flat_map(|r| r.lines.iter().skip(1)).any(|line| {
            let low = plain(line);
            goods_line(line).is_some()
                || capacity_of(line).is_some()
                || ["kapazit", "capacit", "kapacit"].iter().any(|label| low.starts_with(label))
        });
    if quantity.is_none() || !quantity_enough || (goods && !quality_enough) || goods_unread {
        certainty = certainty.min(Certainty::Likely);
    }
    let key = readings.iter().find_map(|r| r.key.clone());
    CardOutcome {
        name: outcome.name,
        key,
        quantity: quantity.unwrap_or(0.0),
        unit: if goods { Unit::Scu } else { Unit::Pieces },
        quality: if goods { quality } else { None },
        certainty,
        candidates: outcome.candidates,
        picture: None,
        kind,
        quantity_read: quantity.is_some(),
        lines: readings.first().map(|r| r.lines.clone()).unwrap_or_default(),
        frame: None,
        tile_rect: None,
        tile_print: None,
    }
}

// ------------------------------------------------------------ Reading at the pointer

/// The readings of the card: enlargement (for a screen 1440 pixels high) and preparation.
pub const CARD_PASSES: [(f64, Mode); 3] = [(2.5, Mode::Plain), (3.5, Mode::Plain), (3.0, Mode::Dark)];

/// The readings of the count on the tile.
pub const COUNT_PASSES: [(f64, Mode); 2] = [(3.0, Mode::Plain), (4.0, Mode::Plain)];

/// An enlargement for a screen of `height` pixels: the writing of the game grows with the height of the screen.
fn scaled_for(scale: f64, height: i64) -> f64 {
    (scale * 1440.0 / height.max(1) as f64).clamp(1.0, 6.0)
}

/// From how far off the pointer a card stands at a fixed place (the inventory) rather than beside the pointer, in
/// heights of the screen.
pub const FIXED_GAP: f64 = 0.08;

/// The margin around the lines of the name in the picture for the question, in heights of the screen.
pub const NAME_MARGIN: f64 = 0.012;

/// The widest picture kept with a question, in pixels.
pub const QUESTION_PICTURE_WIDTH: i64 = 640;

/// Whether the card of the inventory terminal is read ([`read_card_where`]): only in the test build until its
/// reading has been accepted in the game; the freight elevator is read in every build.
pub const READS_INVENTORY: bool = crate::edition::IS_TEST_BUILD;

/// Reads the card at the pointer as [`read_card_where`] does, the inventory terminal only where
/// [`READS_INVENTORY`].
pub fn read_card(
    bitmap: &Bitmap,
    pointer: (i64, i64),
    ocr: &mut dyn Ocr,
    names: &Names,
    texts: Option<&GameTexts>,
) -> Result<CardOutcome, ScanError> {
    read_card_where(bitmap, pointer, ocr, names, texts, READS_INVENTORY)
}

/// Reads the card at the pointer: the card found by its frame ([`card_at`]) read in every pass of [`CARD_PASSES`],
/// the count of the tile in every pass of [`COUNT_PASSES`] (for items), put together by [`combine`]; with the key of
/// the name from `texts` and the lines of the name as picture. Nothing when no card stands at the pointer, and
/// nothing at the inventory terminal (the card found by its writing, or the tile under the pointer in a bright
/// frame) without `inventory`.
pub fn read_card_where(
    bitmap: &Bitmap,
    pointer: (i64, i64),
    ocr: &mut dyn Ocr,
    names: &Names,
    texts: Option<&GameTexts>,
    inventory: bool,
) -> Result<CardOutcome, ScanError> {
    let size = (bitmap.width, bitmap.height);
    let candidates = cards_at(bitmap, pointer);
    let card_jobs = |frame: Area| -> Vec<Job> {
        let inset = ((size.1 as f64 / 480.0).round() as i64).max(2) + 1;
        let inner = (frame.0 + inset, frame.1 + inset, frame.2 - 2 * inset, frame.3 - 2 * inset);
        CARD_PASSES
            .iter()
            .map(|(scale, mode)| Job { area: inner, scale: scaled_for(*scale, size.1), mode: *mode })
            .collect()
    };
    let counter = count_area(pointer, size);
    let count_jobs: Vec<Job> = COUNT_PASSES
        .iter()
        .map(|(scale, mode)| Job { area: counter, scale: scaled_for(*scale, size.1), mode: *mode })
        .collect();
    let read = |passes: &[Vec<Word>]| -> Vec<CardReading> {
        passes.iter().map(|words| read_words(words, names, texts)).collect()
    };
    let named =
        |readings: &[CardReading]| readings.iter().any(|r| r.reading.certainty > Certainty::None || r.broken.is_some());
    // A name with a label below it, as every card shows.
    let carded = |readings: &[CardReading]| {
        readings.iter().any(|r| (r.reading.certainty > Certainty::None || r.broken.is_some()) && r.labelled)
    };
    // The card with its readings; else a frame that shows a name without a label; else the first frame read.
    let mut found: Option<(Area, Vec<CardReading>)> = None;
    // Whether the card was found by its writing, as only the card of the inventory terminal is.
    let mut by_writing = false;
    let mut named_only: Option<(Area, Vec<CardReading>)> = None;
    let mut first_read: Option<(Area, Vec<CardReading>)> = None;
    let mut count: Vec<Vec<Word>> = Vec::new();
    if let Some(&first) = candidates.first() {
        let mut jobs = card_jobs(first);
        jobs.extend(count_jobs.iter().cloned());
        let passes = ocr.read(&jobs)?;
        let (card, counted) = passes.split_at(passes.len().min(CARD_PASSES.len()));
        count = counted.to_vec();
        let mut readings = read(card);
        let mut frame = first;
        // A frame that shows no card may be another one than the card (a tile of the inventory, a part of the
        // card): the next ones are read.
        for &other in candidates.iter().skip(1).take(CARD_TRIES - 1) {
            if carded(&readings) {
                break;
            }
            if named(&readings) {
                named_only.get_or_insert((frame, readings.clone()));
            }
            first_read.get_or_insert((frame, readings));
            readings = read(&ocr.read(&card_jobs(other))?);
            frame = other;
        }
        if carded(&readings) {
            found = Some((frame, readings));
        } else {
            if named(&readings) {
                named_only.get_or_insert((frame, readings.clone()));
            }
            first_read.get_or_insert((frame, readings));
        }
    }
    // No frame shows a card: the card is looked for by its writing, read at one enlargement after the other (the
    // recognition misses a part of a large picture now and then).
    if found.is_none() {
        let area = search_area(pointer, size);
        let mut tried: Vec<Area> = Vec::new();
        'scales: for scale in WRITING_SCALES {
            let job = Job { area, scale: scaled_for(scale, size.1), mode: Mode::Plain };
            let words = ocr.read(&[job])?.into_iter().next().unwrap_or_default();
            for card in cards_by_writing(&text_lines(&words), pointer, size.1 as f64).into_iter().take(CARD_TRIES) {
                // A place read already at another enlargement is not read again.
                let (_, tolerance) = line_steps(size.1);
                let near = |a: i64, b: i64| (a - b).abs() <= tolerance;
                if tried.iter().any(|t| near(t.0, card.0) && near(t.1, card.1) && near(t.2, card.2) && near(t.3, card.3)) {
                    continue;
                }
                tried.push(card);
                let readings = read(&ocr.read(&card_jobs(card))?);
                if carded(&readings) {
                    found = Some((card, readings));
                    by_writing = true;
                    break 'scales;
                }
            }
        }
        if found.is_some() && candidates.is_empty() {
            count = ocr.read(&count_jobs)?;
        }
    }
    let Some((frame, readings)) = found.or(named_only).or(first_read) else { return Ok(CardOutcome::nothing()) };
    let hovered = hovered_tile(bitmap, pointer);
    let at = (pointer.0 as f64, pointer.1 as f64);
    let boxed = count_box_at(&count_boxes(bitmap, counter), at, size.1 as f64);
    // The freight elevator writes the count of every tile in a box: a card found by its writing beside one is its
    // card cut off at the edge of the screen.
    if !inventory && (hovered.is_some() || (by_writing && boxed.is_none())) {
        return Ok(CardOutcome::nothing());
    }
    // The box of the count of the tile under the pointer: a plain `x1` is read from its marks, any count from its
    // writing drawn anew for the recognition.
    // Where the game draws a bright frame around the tile under the pointer (the inventory), it writes the count
    // without a box: a light patch of the picture of an item is no box.
    let tile_box = boxed.filter(|_| hovered.is_none());
    // The tile from its count box where there is one (the same tile gives the same place in every picture), else from
    // the bright frame around it, else from its faint frame.
    let tile = tile_box
        .map(|(b, pitch)| tile_of_box(b, pitch, size.1))
        .or(hovered)
        .or_else(|| tile_at(bitmap, pointer, Some(frame)));
    // A count read counts only on the tile: the count of the tile beside it is none of its own.
    let on_tile = |w: &Word| tile.is_none_or(|t| count_on_tile(t, w, size.1));
    let mut counts: Vec<u32> = count
        .iter()
        .filter_map(|words| {
            let own: Vec<Word> = words.iter().filter(|w| on_tile(w)).cloned().collect();
            count_at(&own, at, size.1 as f64)
        })
        .collect();
    if let Some((b, _)) = tile_box {
        if box_reads_one(&count_marks(bitmap, b), b) {
            counts.extend([1, 1]);
        }
        counts.extend(boxed_count(bitmap, b));
        // Read by the shape of its digits, where every one is sure: as much as two readings, so that it stands alone
        // where the recognition reads nothing, and a reading of the recognition against it makes a question.
        if let Some(n) = shaped_count(bitmap, b) {
            counts.extend([n, n]);
        }
    }
    // A card at a fixed place away from the pointer is the one of the inventory, which writes a count only on a
    // stack, at the top right of its tile: a tile without writing there is a single piece; writing there not read
    // yet is read again alone and large.
    let fixed = gap_to(frame, pointer) > FIXED_GAP * size.1 as f64;
    let no_count_seen = tile_box.is_none() && count.iter().flatten().all(|w| count_of(&w.t).is_none() || !on_tile(w));
    let mut single = false;
    if let (true, true, Some(t)) = (fixed, no_count_seen, tile) {
        let corner = stack_corner(t);
        if corner_written(bitmap, corner) {
            // The recognition reads a short word alone badly: the corner is read with some of the tile around it.
            let pad = corner.3;
            let area = (corner.0 - pad, corner.1 - pad, corner.2 + 2 * pad, corner.3 + 2 * pad);
            let jobs: Vec<Job> = STACK_PASSES
                .iter()
                .map(|(scale, mode)| Job { area, scale: scaled_for(*scale, size.1), mode: *mode })
                .collect();
            counts.extend(
                ocr.read(&jobs)?
                    .iter()
                    .filter_map(|words| words.iter().filter(|w| on_tile(w)).find_map(|w| count_of(&w.t))),
            );
            counts.extend(stacked_count(bitmap, corner));
        } else {
            single = true;
        }
    }
    let mut more_goods = goods_again(bitmap, &readings, ocr, frame)?;
    // The amount read by the shape of its digits in its field, where every one is sure, else where the recognition
    // read an amount, by a smaller lead (it confirms that reading, or makes the card a question, [`against_shapes`]):
    // as much as two readings. Never more than the capacity read.
    // Goods where a reading of the card or of the line alone found a line of goods or the capacity.
    let goods_seen = readings.iter().any(|r| r.goods.is_some() || r.capacity) || !more_goods.is_empty();
    let goods_line = readings.iter().find_map(|r| r.goods_frame).filter(|_| goods_seen);
    let fields = (
        goods_line.and_then(|line| amount_field(bitmap, line, frame)),
        goods_line.and_then(|line| quality_field(bitmap, line, frame)),
    );
    if let Some(field) = fields.0 {
        let read: Vec<f64> = readings.iter().filter_map(|r| r.goods.map(|g| g.1)).chain(more_goods.iter().map(|g| g.1)).collect();
        let most = readings.iter().find_map(|r| r.capacity_value);
        if let Some(amount) = shaped_amount(bitmap, field)
            .or_else(|| checked_amount(bitmap, field, &read))
            .filter(|a| most.is_none_or(|most| *a <= most + 0.0005))
        {
            more_goods.extend([(None, amount), (None, amount)]);
        }
    }
    let mut all = readings.clone();
    all.extend(names_again(bitmap, &readings, ocr, names, texts)?);
    // An item in a box of the inventory shows the capacity of the box, but no line of goods: without one read, a name
    // that is no good of the stock makes the card one of an item.
    let goods_read = all.iter().any(|r| r.goods.is_some()) || !more_goods.is_empty();
    let an_item: Vec<String> = all
        .iter()
        .filter(|r| r.reading.certainty > Certainty::None)
        .filter_map(|r| r.reading.name.clone())
        .collect();
    if !goods_read && !an_item.is_empty() && an_item.iter().all(|name| !is_goods(names, name)) {
        for r in &mut all {
            r.capacity = false;
            r.capacity_value = None;
            r.lines.retain(|line| {
                let low = plain(line);
                capacity_of(line).is_none() && !["kapazit", "capacit", "kapacit"].iter().any(|l| low.starts_with(l))
            });
        }
    }
    // The quality read again alone from its field, for goods.
    let more_qualities = fields
        .1
        .map(|field| {
            let mut numbers = field_number(bitmap, field);
            // Read by the shape of its digits, where every one is sure: as much as two readings. Else, where the
            // recognition read a quality anywhere, the number the shapes give with a smaller lead, as much too: it
            // confirms that reading, or makes the card a question.
            let read: Vec<u32> = readings
                .iter()
                .filter_map(|r| r.goods.and_then(|g| g.0))
                .chain(more_goods.iter().filter_map(|g| g.0))
                .chain(numbers.iter().copied())
                .collect();
            if let Some(q) = shaped_quality(bitmap, field).or_else(|| checked_quality(bitmap, field, &read)) {
                numbers.extend([q, q]);
            }
            numbers
        })
        .unwrap_or_default();
    let mut outcome = combine(&all, &counts, single, &more_goods, &more_qualities);
    // Where the best patterns of the digits give another amount or quality than the one agreed on: a question.
    let (best_amount, best_quality) = best_goods(bitmap, fields.0, fields.1);
    against_shapes(&mut outcome, best_amount, best_quality);
    // A good of the stock counted in pieces had its lines of goods not read.
    if outcome.unit == Unit::Pieces && outcome.name.as_deref().is_some_and(|name| is_goods(names, name)) {
        outcome.certainty = outcome.certainty.min(Certainty::Likely);
    }
    if outcome.key.is_none() {
        outcome.key = outcome.name.as_deref().and_then(|name| texts?.key_of(name)).map(str::to_string);
    }
    outcome.frame = Some(frame);
    outcome.tile_rect = tile;
    let (_, tolerance) = line_steps(size.1);
    let cover = (frame.0 - tolerance, frame.1 - tolerance, frame.2 + 2 * tolerance, frame.3 + 2 * tolerance);
    outcome.tile_print = outcome.tile_rect.and_then(|tile| TilePrint::of(bitmap, tile, Some(cover)));
    let margin = NAME_MARGIN * size.1 as f64;
    let shown = readings.iter().find_map(|r| r.name_frame).map(|f| {
        let left = ((f.0 - margin) as i64).max(frame.0);
        let top = ((f.1 - margin) as i64).max(frame.1);
        let right = ((f.2 + margin) as i64).min(frame.0 + frame.2);
        let bottom = ((f.3 + margin) as i64).min(frame.1 + frame.3);
        (left, top, (right - left).max(1), (bottom - top).max(1))
    });
    outcome.picture = crate::win_ocr::png_of(bitmap, shown.unwrap_or(frame), Some(QUESTION_PICTURE_WIDTH)).ok();
    Ok(outcome)
}

/// The readings of the line of goods alone: enlargement (for a screen 1440 pixels high) and preparation.
pub const GOODS_PASSES: [(f64, Mode); 3] = [(6.0, Mode::Plain), (6.0, Mode::Gray), (8.0, Mode::Dark)];

/// The line of goods read again alone and larger in every pass of [`GOODS_PASSES`], where a reading of the card
/// found it: quality and amount of each pass that read it, an amount above the capacity left out. The line is read
/// to the right edge of the card (`card`): the amount stands far right of the rest and is missing from a reading
/// that lost it.
fn goods_again(
    bitmap: &Bitmap,
    readings: &[CardReading],
    ocr: &mut dyn Ocr,
    card: Area,
) -> Result<Vec<(Option<u32>, f64)>, ScanError> {
    let Some(frame) = readings.iter().find_map(|r| r.goods_frame) else { return Ok(Vec::new()) };
    let margin = 0.008 * bitmap.height as f64;
    let right = (frame.2 + margin).max((card.0 + card.2) as f64 - margin / 2.0);
    let area = (
        (frame.0 - margin) as i64,
        (frame.1 - margin) as i64,
        (right - frame.0 + margin) as i64,
        (frame.3 - frame.1 + 2.0 * margin) as i64,
    );
    let jobs: Vec<Job> = GOODS_PASSES
        .iter()
        .map(|(scale, mode)| Job { area, scale: scaled_for(*scale, bitmap.height), mode: *mode })
        .collect();
    let most = readings.iter().find_map(|r| r.capacity_value);
    Ok(ocr
        .read(&jobs)?
        .iter()
        .filter_map(|words| {
            let text = words.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ");
            goods_line(&text)
        })
        .filter(|(_, amount)| most.is_none_or(|most| *amount <= most + 0.0005))
        .collect())
}

/// The readings of the lines of the name alone: enlargement (for a screen 1440 pixels high) and preparation.
pub const NAME_PASSES: [(f64, Mode); 3] = [(4.0, Mode::Plain), (5.0, Mode::Gray), (4.0, Mode::Dark)];

/// The lines of the name read again alone and larger in every pass of [`NAME_PASSES`], where a reading of the card
/// found them: one reading of the name per pass.
fn names_again(
    bitmap: &Bitmap,
    readings: &[CardReading],
    ocr: &mut dyn Ocr,
    names: &Names,
    texts: Option<&GameTexts>,
) -> Result<Vec<CardReading>, ScanError> {
    let Some(frame) = readings.iter().find_map(|r| r.name_frame) else { return Ok(Vec::new()) };
    let margin = 0.006 * bitmap.height as f64;
    let area = (
        (frame.0 - margin) as i64,
        (frame.1 - margin) as i64,
        (frame.2 - frame.0 + 2.0 * margin) as i64,
        (frame.3 - frame.1 + 2.0 * margin) as i64,
    );
    let jobs: Vec<Job> = NAME_PASSES
        .iter()
        .map(|(scale, mode)| Job { area, scale: scaled_for(*scale, bitmap.height), mode: *mode })
        .collect();
    Ok(ocr.read(&jobs)?.iter().map(|words| read_words(words, names, texts)).collect())
}

/// The enlargements the strip of count boxes is read at.
const STRIP_SCALES: [f64; 3] = [0.5, 0.75, 1.0];

/// The field of the quality in the line of goods (`frame`, its band of rows) within `card`: the columns where the
/// ground is a grey teal (green a little above red and blue; the field of the name is purple, the one of the amount
/// olive, the ground of the card brown), at least [`FIELD_SHARE`] of the band, side by side with gaps of at most
/// two columns, as wide as a number of up to four digits. `None` where there is not exactly one such field.
pub fn quality_field(bitmap: &Bitmap, frame: Frame, card: Area) -> Option<Area> {
    // The ground of the field is a grey teal (green a little above red and blue); the ground of the card is brown
    // (red well above blue), the field of the amount olive (red and green well above blue).
    coloured_field(bitmap, frame, card, |r, g, b| g + 8 >= r && r - b <= 20 && g + 4 >= b && g > 40, (0.012, 0.06))
}

/// The field of the amount in the line of goods (`frame`) within `card`, as [`quality_field`] finds the one of the
/// quality: the columns where the ground is olive (red and green alike, well above blue; the ground of the card is
/// brown, its red well above its green), as wide as an amount with its unit (`0.509 SCU`, `8 SCU`): a part of a
/// field found narrower (its colour lost under bright writing) may have cut the number and is none.
pub fn amount_field(bitmap: &Bitmap, frame: Frame, card: Area) -> Option<Area> {
    coloured_field(bitmap, frame, card, |r, g, b| g - b >= 35 && r - g <= 18 && r + 8 >= g && g > 40, (0.021, 0.09))
}

/// The one field in the line of goods (`frame`, its band of rows) within `card` whose ground has the colour `ground`
/// (red, green, blue): its columns, where at least [`FIELD_SHARE`] of the band has the colour, side by side with
/// gaps of at most two columns, `widths` wide (least and most, in heights of the picture).
fn coloured_field(
    bitmap: &Bitmap,
    frame: Frame,
    card: Area,
    ground: impl Fn(i32, i32, i32) -> bool,
    widths: (f64, f64),
) -> Option<Area> {
    let unit = bitmap.height as f64;
    let (top, bottom) = ((frame.1 - 0.004 * unit) as i64, (frame.3 + 0.004 * unit) as i64);
    let (top, bottom) = (top.max(0), bottom.min(bitmap.height));
    let (left, right) = (card.0.max(0), (card.0 + card.2).min(bitmap.width));
    if bottom <= top || right <= left {
        return None;
    }
    let coloured = |x: i64| {
        let hits = (top..bottom)
            .filter(|&y| {
                let at = ((y * bitmap.width + x) * 4) as usize;
                let (b, g, r) = (i32::from(bitmap.raw[at]), i32::from(bitmap.raw[at + 1]), i32::from(bitmap.raw[at + 2]));
                ground(r, g, b)
            })
            .count();
        hits as f64 >= FIELD_SHARE * (bottom - top) as f64
    };
    let mut fields: Vec<(i64, i64)> = Vec::new();
    let mut run: Option<(i64, i64)> = None;
    for x in left..right {
        match (coloured(x), run) {
            (true, Some((from, to))) if x - to <= 3 => run = Some((from, x)),
            (true, _) => {
                if let Some(done) = run {
                    fields.push(done);
                }
                run = Some((x, x));
            }
            (false, _) => {}
        }
    }
    fields.extend(run);
    let fits = |(from, to): &(i64, i64)| (widths.0 * unit..=widths.1 * unit).contains(&((to - from + 1) as f64));
    let found: Vec<(i64, i64)> = fields.into_iter().filter(fits).collect();
    let [(from, to)] = found[..] else { return None };
    Some((from, top, to - from + 1, bottom - top))
}

/// The share of the band of the line of goods that must be teal in a column of the field of the quality.
pub const FIELD_SHARE: f64 = 0.3;

/// The number in what the recognition read of a field on a strip ([`field_number`]): the number that most of its
/// copies give, at least two of them; a number with anything but a blank glued to it counts for nothing.
pub fn strip_number(text: &str) -> Option<u32> {
    let found: Vec<u32> = text
        .split_whitespace()
        .filter(|word| word.chars().all(|c| c.is_ascii_digit()) && (1..=4).contains(&word.len()))
        .filter_map(|word| word.parse().ok())
        .collect();
    let (n, times) = agreed(&found, |a, b| a == b)?;
    (times >= 2).then_some(n)
}

/// The number in a field of the line of goods, its light writing drawn anew on a strip ([`count_strip_of`]) and read
/// by the text recognition of Windows: one number per reading of [`STRIP_SCALES`] that read one ([`strip_number`]).
/// Empty without a recognition.
pub fn field_number(bitmap: &Bitmap, field: Area) -> Vec<u32> {
    // The strip takes its writing from the part less a sixth on every side: it reaches that far around the field.
    let around = (field.0 - field.2 / 4, field.1 - field.3 / 4, field.2 * 3 / 2, field.3 * 3 / 2);
    let Some((strip, _)) = count_strip_of(bitmap, std::slice::from_ref(&around), true) else { return Vec::new() };
    let jobs: Vec<Job> = STRIP_SCALES
        .iter()
        .map(|&scale| Job { area: (0, 0, strip.width, strip.height), scale, mode: Mode::Plain })
        .collect();
    let Ok(read) = crate::win_ocr::Recognition::new(&strip).read(&jobs) else { return Vec::new() };
    read.iter()
        .filter_map(|words| strip_number(&words.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ")))
        .collect()
}

/// The count of a stack in the corner of a tile ([`stack_corner`]), its light writing read on a strip
/// ([`count_strip_of`]) by the text recognition of Windows: one count per reading of [`STRIP_SCALES`] that read
/// one ([`stack_strip_count`]). Empty without a recognition.
fn stacked_count(bitmap: &Bitmap, corner: Area) -> Vec<u32> {
    // The part the strip takes its writing from leaves out a sixth on every side: it reaches that far around the
    // corner, but not above the tile.
    let around = (corner.0 - corner.2 / 4, corner.1 - corner.3 / 3, corner.2 * 3 / 2, corner.3 * 5 / 3);
    let Some((strip, _)) = count_strip_of(bitmap, std::slice::from_ref(&around), true) else { return Vec::new() };
    let jobs: Vec<Job> = STRIP_SCALES
        .iter()
        .map(|&scale| Job { area: (0, 0, strip.width, strip.height), scale, mode: Mode::Plain })
        .collect();
    let Ok(read) = crate::win_ocr::Recognition::new(&strip).read(&jobs) else { return Vec::new() };
    read.iter()
        .filter_map(|words| stack_strip_count(&words.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ")))
        .collect()
}

/// The count in the box `b`, read on a strip of its writing ([`count_strip`]) by the text recognition of Windows:
/// one count per reading of [`STRIP_SCALES`] that read one. Empty without a recognition.
fn boxed_count(bitmap: &Bitmap, b: Area) -> Vec<u32> {
    let Some((strip, _)) = count_strip(bitmap, std::slice::from_ref(&b)) else { return Vec::new() };
    let jobs: Vec<Job> = STRIP_SCALES
        .iter()
        .map(|&scale| Job { area: (0, 0, strip.width, strip.height), scale, mode: Mode::Plain })
        .collect();
    let Ok(read) = crate::win_ocr::Recognition::new(&strip).read(&jobs) else { return Vec::new() };
    read.iter()
        .filter_map(|words| strip_count(&words.iter().map(|w| w.t.as_str()).collect::<Vec<_>>().join(" ")))
        .collect()
}

/// Reads the card at the mouse pointer (`pointer`, in points of the screen) in the window of the game while it is in
/// front. `Ok(None)` when the game is not in front or the pointer stands outside its window.
pub fn read_game(
    pointer: (i32, i32),
    names: &Names,
    texts: Option<&GameTexts>,
) -> Result<Option<CardOutcome>, ScanError> {
    use crate::refinery_scan::Reason;
    let Some(rect) = crate::screen_grab::game_rect() else { return Ok(None) };
    let at = (i64::from(pointer.0 - rect.0), i64::from(pointer.1 - rect.1));
    let size = (i64::from(rect.2), i64::from(rect.3));
    if at.0 < 0 || at.1 < 0 || at.0 >= size.0 || at.1 >= size.1 {
        return Ok(None);
    }
    let raw = crate::screen_grab::grab_raw(rect).map_err(|e| ScanError::new(Reason::Grab, e.key()))?;
    let bitmap = Bitmap { width: size.0, height: size.1, raw };
    let mut ocr = crate::win_ocr::Recognition::new(&bitmap);
    read_card(&bitmap, at, &mut ocr, names, texts).map(Some)
}

// ------------------------------------------------------------ Joined to the booking

impl CardOutcome {
    /// What the booking of the stock scan ([`crate::stock_scan_book`]) makes of this reading: sure only with a name
    /// and a quantity read, a question with the candidates and the picture of the name for anything else read,
    /// nothing where nothing was read. The tile goes along, its print as bytes ([`TilePrint::to_bytes`]).
    pub fn reading(&self) -> crate::stock_scan_book::Reading {
        use crate::stock_scan_book::{Measure, Reading, ScannedItem};
        if self.certainty == Certainty::None {
            return Reading::Nothing;
        }
        let name = self.name.clone().or_else(|| self.candidates.first().cloned()).unwrap_or_default();
        let measure = match self.unit {
            Unit::Scu => Measure::Scu,
            Unit::Pieces => Measure::Pieces,
        };
        let item = ScannedItem {
            tile_rect: self.tile_rect,
            tile_print: self.tile_print.as_ref().map(TilePrint::to_bytes),
            ..ScannedItem::new(&name, self.quantity, measure, self.quality)
        };
        if self.certainty == Certainty::Sure && self.name.is_some() && self.quantity_read {
            Reading::Sure(item)
        } else {
            Reading::Unsure { item, candidates: self.candidates.clone(), picture: self.picture.clone() }
        }
    }
}

/// What a reading needs besides the screen: the names of the stock and the texts of the game.
pub struct ScanContext {
    pub names: Names,
    /// The item texts of the English `global.ini`, with the keys only the `global.ini` the game reads has (names
    /// VerseKit fills in) added.
    pub texts: Option<GameTexts>,
}

/// How long a [`ScanContext`] is used before it is built anew, in seconds.
pub const CONTEXT_SECONDS: u64 = 300;

impl GameTexts {
    /// Takes the names of items of `other` (keys with `name` in them) as names this one knows the key of, where it
    /// has no key for the same name: the names in the language of the game lead to the keys, and the keys to the
    /// English names.
    pub fn add_names_of(&mut self, other: &GameTexts) {
        for (low, (key, text)) in &other.by_key {
            if low.contains("name") && self.by_key.contains_key(low) {
                self.by_text.entry(plain(text)).or_insert_with(|| key.clone());
                if low.starts_with("item_name") {
                    let keys = self.by_folded_name.entry(fold_name(text)).or_default();
                    if !keys.iter().any(|k| k.eq_ignore_ascii_case(key)) {
                        keys.push(key.clone());
                    }
                }
            }
        }
    }

    /// The English name of an item whose name the card shows, by the texts of the game: the key of the name, where
    /// it is one of an item (`item_Name…`), and its text; else the only name of an item that is the same once the
    /// characters the recognition mistakes are folded together ([`fold_name`]), where all its keys have the same
    /// text.
    pub fn item_name(&self, shown: &str) -> Option<(&str, &str)> {
        if let Some(key) = self.key_of(shown)
            && key.to_lowercase().starts_with("item_name")
        {
            return Some((key, self.text(key)?));
        }
        let keys = self.by_folded_name.get(&fold_name(shown))?;
        let first = keys.first()?;
        let text = self.text(first)?;
        keys.iter().all(|k| self.text(k) == Some(text)).then_some((first.as_str(), text))
    }

    /// Takes the keys of `other` this one lacks.
    pub fn add_missing(&mut self, other: &GameTexts) {
        for (low, (key, text)) in &other.by_key {
            if self.by_key.contains_key(low) {
                continue;
            }
            self.by_folded.entry(fold_key(low)).or_default().push(low.clone());
            self.by_text.entry(plain(text)).or_insert_with(|| key.clone());
            self.by_key.insert(low.clone(), (key.clone(), text.clone()));
        }
    }
}

/// The context of the data of `paths`: the stock (`StockData` of its files), the catalog the program works with,
/// the English `global.ini` of the game and the one the game reads (its names in the language of the game).
pub fn context_of(paths: &crate::paths::Paths) -> ScanContext {
    let data = crate::stock_data::StockData::load(|name| paths.app_file(name));
    let catalog = crate::catalog_cache::load_current(paths);
    let mut names = stock_names(&data, catalog.as_ref());
    let read = |path: &str| std::fs::read(path).ok().map(|bytes| GameTexts::parse(&String::from_utf8_lossy(&bytes)));
    let english =
        crate::game_ini::target_ini(paths, "english").filter(|p| crate::game_ini::is_file(p)).and_then(|p| read(&p));
    let game = crate::game_ini::ini_file(paths).filter(|p| crate::game_ini::is_file(p)).and_then(|p| read(&p));
    // The items of the kinds the scan needs by name (components, armour, weapons of a person), from the English
    // texts, before the names in the language of the game are tied to them.
    important::add_important_names(&mut names, &important::important_names(english.as_ref(), catalog.as_ref()));
    let texts = match (english, game) {
        (Some(mut english), Some(game)) => {
            names.add_translations(&translation_pairs(&english, &game));
            english.add_missing(&game);
            english.add_names_of(&game);
            Some(english)
        }
        (english, game) => english.or(game),
    };
    ScanContext { names, texts }
}

/// The context of `paths` ([`context_of`]), kept for [`CONTEXT_SECONDS`]: building it reads the stock, the catalog
/// and two `global.ini`.
pub fn scan_context(paths: &crate::paths::Paths) -> std::sync::Arc<ScanContext> {
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::{Duration, Instant};
    type Kept = Option<(String, Instant, Arc<ScanContext>)>;
    static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();
    let owner = paths.app_file("");
    let mut kept = KEPT.get_or_init(|| Mutex::new(None)).lock().unwrap_or_else(|e| e.into_inner());
    if let Some((of, at, context)) = kept.as_ref()
        && *of == owner
        && at.elapsed() < Duration::from_secs(CONTEXT_SECONDS)
    {
        return Arc::clone(context);
    }
    let context = Arc::new(context_of(paths));
    *kept = Some((owner, Instant::now(), Arc::clone(&context)));
    context
}

pub mod glyphs;
pub mod important;

#[cfg(test)]
mod tests;
