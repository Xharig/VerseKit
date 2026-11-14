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

//! The own text recognition of the refinery terminal, the same code on every system and without a library: the
//! terminal writes in one font, so a part of the picture is cut into lines, words and letters, and every letter
//! is compared with the pictures of the letters of that font ([`Font`], shipped in `schrift.bin`, learned from
//! kept pictures of the terminal with known lines by `tests/refinery_font.rs`). Under Linux it is to read in place
//! of the recognition of Windows, once it reads the same pictures at least as well (`tests/refinery_test_set.rs`
//! measures both); until then it is built and measured but not used ([`READY`]).
//!
//! The steps of one reading ([`segment`]):
//! 1. The part enlarged as the job says ([`crate::picture_scale`]), one brightness per point: the brightest
//!    channel for [`Mode::Plain`] (coloured writing on the dark ground stays bright), the luminance otherwise.
//!    The readings of a part differ so (enlargement and brightness), and the voting of
//!    [`super::merge_passes`] keeps working.
//! 2. Ink is every point clearly brighter than the mean round it ([`RADIUS`], [`CONTRAST`]): bright writing on
//!    a dark ground, also on a ground that grows brighter across the screen.
//! 3. Connected points make pieces; pieces one above the other make one letter (the dot of an i, a colon).
//! 4. Letters at the same height make a line; a gap wider than [`WORD_GAP`] of the height of the line begins a
//!    new word. A letter far wider than any letter of the font is two touching ones and is cut where the least
//!    ink is.
//! 5. Every letter is reduced to [`FW`] × [`FH`] points of brightness plus its width and its place in the
//!    height of the line ([`Shape`]), and named by the nearest learned picture ([`Font::classify`]).
//!
//! ⚠ A digit that lies almost as near to another digit as to its own is written as `?`: the reading of the cell
//! then fails in this pass and the other passes decide, instead of a wrong number standing as sure. 0 and 8 are
//! then decided by the learned digits of the player ([`super::digits`]) as with the recognition of Windows.

use std::sync::OnceLock;

use crate::win_ocr::{Bitmap, fitted, word_in_picture};

use super::{Job, Mode, Ocr, ScanError, Word};

/// Size of the picture of a letter.
pub const FW: usize = 12;
pub const FH: usize = 16;
/// The radius of the mean a point is compared with, in points of the enlarged part.
pub const RADIUS: usize = 30;
/// How much brighter than that mean a point of ink is (0–255), and how bright at least.
pub const CONTRAST: i32 = 40;
pub const INK_MIN: u8 = 70;
/// A gap wider than this share of the height of the line separates two words.
/// ⚠ Not smaller: the digits of the terminal are all equally wide, and beside a 1 stands almost a third of the
/// height of ground; a number cut at its 1 is lost. Words of the heads run together then, the columns of the
/// table stay far apart.
pub const WORD_GAP: f32 = 0.5;
/// The margin of ground taken round a letter for its shape, in points of the enlarged part.
pub const MARGIN: usize = 2;
/// A letter wider than this share of the height of its line is two touching letters.
pub const SPLIT_ASPECT: f32 = 1.25;
/// A digit whose nearest other digit lies less than this factor farther away is not sure: written as `?`.
pub const DIGIT_MARGIN: f32 = 1.2;
/// A letter farther than this from every learned picture is none of the font (measured at the other half of the
/// set: 99 % of the letters lie within 0.1, the farthest at 0.3).
pub const NO_LETTER: f32 = 0.15;
/// The core font a learned font is cleaned with ([`Font::build`]): pictures per character, and the company a
/// picture needs to be taken into it (more than this many of its character nearer than this).
const CORE: usize = 15;
const DENSE: f32 = 0.03;
const DENSE_COUNT: usize = 3;
/// How far the width of a learned picture may lie from the usual one of its character (in heights of the line).
const WIDTH_SPREAD: f32 = 0.25;
/// The largest side of an enlarged part.
pub const LIMIT: u32 = 12000;
/// Pieces smaller than this many points are dust; letters higher than this many points are no writing.
const DUST: usize = 4;
const TALLEST: usize = 320;

/// A letter reduced for comparing: its brightness in [`FW`] × [`FH`] points (spread to 0–255), its width, and
/// where its top and bottom stand, all in heights of its line (top 0 at the line's top, bottom 0 at its base).
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub pixels: Vec<u8>,
    pub aspect: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Shape {
    /// The distance of two letters: the mean square difference of the points (0–1) and of the measures.
    pub fn distance(&self, other: &Shape) -> f32 {
        let points: u32 = self
            .pixels
            .iter()
            .zip(&other.pixels)
            .map(|(a, b)| {
                let d = i32::from(*a) - i32::from(*b);
                (d * d) as u32
            })
            .sum();
        let points = points as f32 / (FW * FH) as f32 / (255.0 * 255.0);
        let (da, dt, db) = (self.aspect - other.aspect, self.top - other.top, self.bottom - other.bottom);
        points + 2.0 * da * da + dt * dt + db * db
    }
}

/// One word as cut from the part: its frame in points of the enlarged part and its letters.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub frame: (f32, f32, f32, f32),
    pub shapes: Vec<Shape>,
}

/// A part cut into words: the part of the picture, the size it was enlarged to, the words.
#[derive(Clone, Debug, PartialEq)]
pub struct Segmented {
    pub area: super::Area,
    pub size: (usize, usize),
    pub words: Vec<Piece>,
}

impl Segmented {
    /// The frame of a word in pixels of the picture, as the recognition of Windows gives it.
    pub fn word(&self, text: &str, piece: &Piece) -> Word {
        word_in_picture(text, piece.frame, self.area, self.size)
    }
}

/// A box of points: left, top, right, bottom (inclusive).
type Box4 = [usize; 4];

fn width(b: &Box4) -> usize {
    b[2] - b[0] + 1
}

fn height(b: &Box4) -> usize {
    b[3] - b[1] + 1
}

fn union(a: &Box4, b: &Box4) -> Box4 {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

/// The brightness of the enlarged part, one byte a point.
fn brightness(bgra: &[u8], mode: Mode) -> Vec<u8> {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .map(|p| match mode {
            Mode::Plain => p[0].max(p[1]).max(p[2]),
            Mode::Gray | Mode::Dark => {
                ((u32::from(p[2]) * 77 + u32::from(p[1]) * 150 + u32::from(p[0]) * 29) >> 8) as u8
            }
        })
        .collect()
}

/// Ink: every point clearly brighter than the mean of the square of [`RADIUS`] round it.
pub fn ink(gray: &[u8], w: usize, h: usize) -> Vec<bool> {
    // Sums over the rectangle from the corner, one row and one column more.
    let mut sums = vec![0u32; (w + 1) * (h + 1)];
    for y in 0..h {
        let mut row = 0u32;
        for x in 0..w {
            row += u32::from(gray[y * w + x]);
            sums[(y + 1) * (w + 1) + x + 1] = sums[y * (w + 1) + x + 1] + row;
        }
    }
    let mut out = vec![false; w * h];
    for y in 0..h {
        let (y0, y1) = (y.saturating_sub(RADIUS), (y + RADIUS + 1).min(h));
        for x in 0..w {
            let v = gray[y * w + x];
            if v < INK_MIN {
                continue;
            }
            let (x0, x1) = (x.saturating_sub(RADIUS), (x + RADIUS + 1).min(w));
            let total =
                sums[y1 * (w + 1) + x1] + sums[y0 * (w + 1) + x0] - sums[y0 * (w + 1) + x1] - sums[y1 * (w + 1) + x0];
            let mean = total / ((y1 - y0) * (x1 - x0)) as u32;
            out[y * w + x] = i32::from(v) > mean as i32 + CONTRAST;
        }
    }
    out
}

/// A piece of ink: its box and how many points of ink it holds.
type Blob = (Box4, usize);

/// The connected pieces of ink (eight neighbours); dust and pieces too large for writing left out.
pub fn pieces(mask: &[bool], w: usize, h: usize) -> Vec<Blob> {
    let mut seen = vec![false; w * h];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if !mask[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let (mut b, mut count) = ([start % w, start / w, start % w, start / w], 0usize);
        while let Some(at) = stack.pop() {
            count += 1;
            let (x, y) = (at % w, at / w);
            b = union(&b, &[x, y, x, y]);
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let next = ny * w + nx;
                    if mask[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
        }
        // A thin line of the frame of a panel is no writing, nor an outline (a box with little ink in it).
        let (bw, bh) = (width(&b), height(&b));
        let outline = bh > 12 && count * 12 < bw * bh;
        if count >= DUST && bh <= TALLEST && bw <= TALLEST * 2 && bw <= bh * 10 && !outline {
            out.push((b, count));
        }
    }
    out
}

/// Pieces one above the other made one letter: the dot of an i, the points of an umlaut. The smaller one is less
/// than half as high as the other, they overlap in at least half the width of the narrower, and the gap between
/// them is at most half the height of the larger: two letters of lines one above the other stay apart.
fn letters(mut blobs: Vec<Blob>) -> Vec<Blob> {
    blobs.sort_by_key(|(b, _)| b[0]);
    let mut out: Vec<Blob> = Vec::new();
    for (b, count) in blobs {
        let joined = out.iter_mut().rev().take(6).find(|(o, _)| {
            let overlap = (o[2].min(b[2]) + 1).saturating_sub(o[0].max(b[0]));
            let gap = b[1].saturating_sub(o[3]).max(o[1].saturating_sub(b[3]));
            let (small, large) = (height(o).min(height(&b)), height(o).max(height(&b)));
            overlap * 2 >= width(o).min(width(&b)) && small * 2 < large && gap * 2 <= large
        });
        match joined {
            Some((o, n)) => {
                *o = union(o, &b);
                *n += count;
            }
            None => out.push((b, count)),
        }
    }
    out
}

/// Whether a letter looks like writing (not a point, a dash or a block): high enough, not too wide, not too
/// empty. Only such letters begin a line.
fn writing(blob: &Blob) -> bool {
    let (b, count) = blob;
    let (bw, bh) = (width(b), height(b));
    bh >= 6 && bw * 10 <= bh * 16 && count * 10 >= bw * bh
}

/// A line of letters, left to right, with the band (top and base) of the letter that began it, and the median
/// height of its writing.
struct Line {
    top: usize,
    base: usize,
    letters: Vec<Box4>,
    height: f32,
}

/// The letters in lines: letters of writing, tallest first, begin a line or join the one their middle lies in;
/// then points and dashes join the line their middle lies in. A point without a line is left out.
fn lines(letters: Vec<Blob>) -> Vec<Line> {
    let (mut tall, small): (Vec<Blob>, Vec<Blob>) = letters.into_iter().partition(writing);
    tall.sort_by_key(|(b, _)| std::cmp::Reverse(height(b)));
    let mut out: Vec<Line> = Vec::new();
    // A letter joins the line its middle lies in; a letter of writing only one of about its own height (the band
    // at most 1.7 times as high): a bar of a panel as high as several lines must not swallow them.
    let find = |out: &mut Vec<Line>, b: &Box4, writing: bool| -> Option<usize> {
        let middle = (b[1] + b[3]) / 2;
        // Of several lines the lowest band: a point between the lines of a bar and of writing belongs to writing.
        out.iter()
            .enumerate()
            .filter(|(_, l)| {
                let band = l.base + 1 - l.top;
                let slack = band / 5;
                let fits = !writing || band * 10 <= height(b) * 17;
                fits && l.top.saturating_sub(slack) <= middle && middle <= l.base + slack
            })
            .min_by_key(|(_, l)| l.base - l.top)
            .map(|(i, _)| i)
    };
    for (b, _) in tall {
        match find(&mut out, &b, true) {
            Some(i) => out[i].letters.push(b),
            None => out.push(Line { top: b[1], base: b[3], letters: vec![b], height: 0.0 }),
        }
    }
    for line in &mut out {
        let mut heights: Vec<usize> = line.letters.iter().map(height).collect();
        heights.sort_unstable();
        line.height = heights[heights.len() / 2] as f32;
    }
    for (b, _) in small {
        if let Some(i) = find(&mut out, &b, false) {
            out[i].letters.push(b);
        }
    }
    for line in &mut out {
        line.letters.sort_by_key(|b| b[0]);
    }
    out
}

/// The band of a word: the median top and base of its letters at least 70 % as high as its tallest.
fn band(letters: &[Box4]) -> (usize, usize) {
    let tallest = letters.iter().map(height).max().unwrap_or(1);
    let tall: Vec<&Box4> = letters.iter().filter(|b| height(b) * 10 >= tallest * 7).collect();
    let mut tops: Vec<usize> = tall.iter().map(|b| b[1]).collect();
    let mut bases: Vec<usize> = tall.iter().map(|b| b[3]).collect();
    tops.sort_unstable();
    bases.sort_unstable();
    let (top, base) = (tops[tops.len() / 2], bases[bases.len() / 2]);
    (top, base.max(top))
}

/// Cuts a letter too wide for one at the column with the least ink in its middle part; again for each half.
fn split(b: Box4, mask: &[bool], w: usize, line_h: f32, depth: usize) -> Vec<Box4> {
    if depth == 0 || (width(&b) as f32) <= line_h * SPLIT_ASPECT {
        return vec![b];
    }
    let margin = ((line_h * 0.3) as usize).max(1);
    let (from, to) = (b[0] + margin, b[2].saturating_sub(margin));
    if from >= to {
        return vec![b];
    }
    let column = |x: usize| (b[1]..=b[3]).filter(|&y| mask[y * w + x]).count();
    let cut = (from..=to).min_by_key(|&x| (column(x), (x as i64 - ((b[0] + b[2]) / 2) as i64).unsigned_abs()));
    let Some(cut) = cut else { return vec![b] };
    let shrink = |mut part: Box4| -> Option<Box4> {
        let rows: Vec<usize> = (part[1]..=part[3]).filter(|&y| (part[0]..=part[2]).any(|x| mask[y * w + x])).collect();
        part[1] = *rows.first()?;
        part[3] = *rows.last()?;
        Some(part)
    };
    let mut out = Vec::new();
    for part in [[b[0], b[1], cut.saturating_sub(1).max(b[0]), b[3]], [cut, b[1], b[2], b[3]]] {
        if let Some(part) = shrink(part) {
            out.extend(split(part, mask, w, line_h, depth - 1));
        }
    }
    out
}

/// The shape of the letter `b` of a line with the band `top`..`base`, in a picture `w` × `h`. The points are taken
/// with a margin of [`MARGIN`] round the letter: the ground beside it gives the spread its dark end, else a letter
/// that fills its box (an I, a dash) would be spread from noise alone.
fn shape(gray: &[u8], w: usize, h: usize, b: &Box4, top: usize, base: usize) -> Shape {
    let line_h = (base + 1 - top) as f32;
    let measures =
        (width(b) as f32 / line_h, (b[1] as f32 - top as f32) / line_h, (b[3] as f32 - base as f32) / line_h);
    let b = &[
        b[0].saturating_sub(MARGIN),
        b[1].saturating_sub(MARGIN),
        (b[2] + MARGIN).min(w - 1),
        (b[3] + MARGIN).min(h - 1),
    ];
    let (bw, bh) = (width(b), height(b));
    let mut pixels = vec![0u8; FW * FH];
    // Each cell is the mean of the points it covers, at least one: a letter narrower than the grid repeats its
    // columns.
    let span = |i: usize, size: usize, cells: usize| {
        let from = i * size / cells;
        from..((i + 1) * size).div_ceil(cells).max(from + 1).min(size)
    };
    let mut means = [0u32; FW * FH];
    for cy in 0..FH {
        let ys = span(cy, bh, FH);
        for cx in 0..FW {
            let xs = span(cx, bw, FW);
            let mut sum = 0u32;
            for y in ys.clone() {
                for x in xs.clone() {
                    sum += u32::from(gray[(b[1] + y) * w + b[0] + x]);
                }
            }
            means[cy * FW + cx] = sum / (ys.len() * xs.len()) as u32;
        }
    }
    let (low, high) = (means.iter().min().copied().unwrap_or(0), means.iter().max().copied().unwrap_or(0));
    let spread = (high - low).max(1);
    for (pixel, mean) in pixels.iter_mut().zip(means) {
        *pixel = ((mean - low) * 255 / spread) as u8;
    }
    Shape { pixels, aspect: measures.0, top: measures.1, bottom: measures.2 }
}

/// One job cut into words; `None` when its part lies outside the picture.
pub fn segment(bitmap: &Bitmap, job: &Job) -> Option<Segmented> {
    let (area, scale) = fitted(job, bitmap.width, bitmap.height, LIMIT)?;
    let (w, h, bgra) = crate::win_ocr::scaled_own(bitmap, area, scale, Mode::Plain).ok()?;
    let gray = brightness(&bgra, job.mode);
    let mask = ink(&gray, w, h);
    let mut words = Vec::new();
    for line in lines(letters(pieces(&mask, w, h))) {
        let line_h = line.height;
        let mut current: Vec<Box4> = Vec::new();
        let mut flush = |current: &mut Vec<Box4>| {
            if current.is_empty() {
                return;
            }
            let frame = current.iter().fold(current[0], |all, b| union(&all, b));
            // A word of points and dashes alone (`--`) takes the band of its line: its own would make a dash
            // as high as a letter.
            let tallest = current.iter().map(height).max().unwrap_or(0) as f32;
            let (top, base) = if tallest < line_h * 0.5 { (line.top, line.base) } else { band(current) };
            let shapes = current.iter().map(|b| shape(&gray, w, h, b, top, base)).collect();
            words.push(Piece {
                frame: (frame[0] as f32, frame[1] as f32, width(&frame) as f32, height(&frame) as f32),
                shapes,
            });
            current.clear();
        };
        for b in &line.letters {
            if let Some(last) = current.last()
                && (b[0] as f32 - last[2] as f32) > line_h * WORD_GAP
            {
                flush(&mut current);
            }
            current.extend(split(*b, &mask, w, line_h, 3));
        }
        flush(&mut current);
    }
    Some(Segmented { area, size: (w, h), words })
}

/// What [`Font::classify`] names: the nearest character with its distance, and the nearest other one.
pub type Named = ((char, f32), Option<(char, f32)>);

/// The learned pictures of the letters of the terminal's font.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Font {
    pub glyphs: Vec<(char, Shape)>,
}

/// The head of the file of a font.
const MAGIC: &[u8; 4] = b"VKSF";
const VERSION: u8 = 1;

impl Font {
    /// The font shipped with the program (`schrift.bin`); empty when the file holds none.
    pub fn shipped() -> &'static Font {
        static FONT: OnceLock<Font> = OnceLock::new();
        FONT.get_or_init(|| Font::from_bytes(include_bytes!("schrift.bin")).unwrap_or_default())
    }

    /// The font as bytes: the head, then per letter its character, its measures and its points, packed by zlib.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut raw = MAGIC.to_vec();
        raw.extend_from_slice(&[VERSION, FW as u8, FH as u8]);
        raw.extend_from_slice(&(self.glyphs.len() as u32).to_le_bytes());
        let milli =
            |v: f32| ((v * 1000.0).round().clamp(f32::from(i16::MIN), f32::from(i16::MAX)) as i16).to_le_bytes();
        for (c, s) in &self.glyphs {
            raw.extend_from_slice(&u32::from(*c).to_le_bytes());
            raw.extend_from_slice(&milli(s.aspect));
            raw.extend_from_slice(&milli(s.top));
            raw.extend_from_slice(&milli(s.bottom));
            raw.extend_from_slice(&s.pixels);
        }
        miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9)
    }

    /// A font from [`Font::to_bytes`]; `None` for anything else.
    pub fn from_bytes(bytes: &[u8]) -> Option<Font> {
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(bytes).ok()?;
        if raw.get(..4)? != MAGIC || raw.get(4..7)? != [VERSION, FW as u8, FH as u8] {
            return None;
        }
        let count = u32::from_le_bytes(raw.get(7..11)?.try_into().ok()?) as usize;
        let size = 4 + 6 + FW * FH;
        let body = raw.get(11..)?;
        if body.len() != count * size {
            return None;
        }
        let mut glyphs = Vec::with_capacity(count);
        for entry in body.chunks_exact(size) {
            let c = char::from_u32(u32::from_le_bytes(entry[..4].try_into().ok()?))?;
            let milli = |at: usize| f32::from(i16::from_le_bytes([entry[at], entry[at + 1]])) / 1000.0;
            glyphs.push((c, Shape { aspect: milli(4), top: milli(6), bottom: milli(8), pixels: entry[10..].to_vec() }));
        }
        Some(Font { glyphs })
    }

    /// A font from learned letters: per character at most `keep` pictures, the ones farthest from those taken
    /// already (the first is the one nearest to all others of its character).
    ///
    /// ⚠ The farthest pictures are also the wrongly named ones (a word cut otherwise in two places has as many
    /// letters but shifted names). So first a core font of the [`CORE`] pictures nearest to the middle of each
    /// character; only pictures that core names as their own character are chosen from.
    pub fn build(samples: &[(char, Shape)], keep: usize) -> Font {
        let mut chars: Vec<char> = samples.iter().map(|(c, _)| *c).collect();
        chars.sort_unstable();
        chars.dedup();
        // A picture far wider or narrower than its character is mostly is a neighbour named wrongly.
        let usual: Vec<(char, f32)> = chars
            .iter()
            .map(|&c| {
                let mut widths: Vec<f32> = samples.iter().filter(|(d, _)| *d == c).map(|(_, s)| s.aspect).collect();
                widths.sort_by(f32::total_cmp);
                (c, widths[widths.len() / 2])
            })
            .collect();
        let width_fits =
            |c: char, s: &Shape| usual.iter().any(|(u, w)| *u == c && (s.aspect - w).abs() <= WIDTH_SPREAD);
        let samples: Vec<(char, Shape)> = samples.iter().filter(|(c, s)| width_fits(*c, s)).cloned().collect();
        let core = Font::spread(&samples, &chars, CORE, true);
        let clean: Vec<(char, Shape)> = samples
            .iter()
            .filter(|(c, s)| core.classify(s).is_some_and(|((named, _), _)| named == *c))
            .cloned()
            .collect();
        Font::spread(&clean, &chars, keep, false)
    }

    /// Per character at most `keep` pictures: the one nearest to the others first, then the farthest from those
    /// taken; for the core (`central`) only among pictures with company.
    fn spread(samples: &[(char, Shape)], chars: &[char], keep: usize, central: bool) -> Font {
        let mut glyphs = Vec::new();
        for &c in chars {
            let own: Vec<&Shape> = samples.iter().filter(|(d, _)| *d == c).map(|(_, s)| s).collect();
            if own.is_empty() {
                continue;
            }
            // The core takes only pictures with company: at least [`DENSE_COUNT`] others of their character within
            // [`DENSE`] (among the first 300); a wrongly named one stands alone.
            // A character without such pictures (one seldom seen) keeps all of its own.
            let own: Vec<&Shape> = if central && own.len() > DENSE_COUNT {
                let looked = &own[..own.len().min(300)];
                let dense: Vec<&Shape> = own
                    .iter()
                    .copied()
                    .filter(|s| looked.iter().filter(|o| o.distance(s) < DENSE).count() > DENSE_COUNT)
                    .collect();
                if dense.is_empty() { own } else { dense }
            } else {
                own
            };
            // At most 400 looked at for the first: the sum over all pairs grows fast.
            let looked = &own[..own.len().min(400)];
            let first = (0..looked.len())
                .min_by(|&a, &b| {
                    let sum = |i: usize| looked.iter().map(|s| s.distance(looked[i])).sum::<f32>();
                    sum(a).total_cmp(&sum(b))
                })
                .unwrap_or(0);
            let mut taken = vec![first];
            let mut nearest: Vec<f32> = own.iter().map(|s| s.distance(own[first])).collect();
            while taken.len() < keep.min(own.len()) {
                let (next, far) = nearest
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(i, d)| (i, *d))
                    .unwrap_or((0, 0.0));
                if far < 0.004 {
                    break;
                }
                taken.push(next);
                for (i, s) in own.iter().enumerate() {
                    nearest[i] = nearest[i].min(s.distance(own[next]));
                }
            }
            glyphs.extend(taken.into_iter().map(|i| (c, own[i].clone())));
        }
        Font { glyphs }
    }

    /// The nearest character and its distance, and the nearest other character with its distance.
    pub fn classify(&self, shape: &Shape) -> Option<Named> {
        let mut best: Option<(char, f32)> = None;
        let mut by_char: Vec<(char, f32)> = Vec::new();
        for (c, s) in &self.glyphs {
            // Letters of a very different width are never the nearest: skipped without comparing the points.
            if (s.aspect - shape.aspect).abs() > 0.6 {
                continue;
            }
            let d = s.distance(shape);
            match by_char.iter_mut().find(|(k, _)| k == c) {
                Some(entry) => entry.1 = entry.1.min(d),
                None => by_char.push((*c, d)),
            }
            if best.is_none_or(|(_, b)| d < b) {
                best = Some((*c, d));
            }
        }
        let best = best?;
        let second = by_char.into_iter().filter(|(c, _)| *c != best.0).min_by(|a, b| a.1.total_cmp(&b.1));
        Some((best, second))
    }

    /// The text of a word: its letters named one by one; an unsure digit as `?`. A flat letter is a dash in the
    /// middle of the line and a point at its base, by its place alone: the terminal writes `--` for a yield not
    /// known yet, and as a learned picture a dash lies nearer to a 4 than to anything else.
    ///
    /// A letter farther than [`NO_LETTER`] from every picture is no letter of the font (the symbol in front of a
    /// material): it is left out, or written as `?` when the nearest picture is a digit, so that a number never
    /// loses a digit unseen.
    pub fn read(&self, piece: &Piece) -> String {
        piece
            .shapes
            .iter()
            .filter_map(|shape| match self.classify(shape) {
                _ if flat(shape) => Some(if shape.bottom > -0.2 { '.' } else { '-' }),
                Some(((c, d), _)) if d > NO_LETTER => c.is_ascii_digit().then_some('?'),
                Some(((c, d), Some((other, o))))
                    if c.is_ascii_digit() && other.is_ascii_digit() && o < d * DIGIT_MARGIN =>
                {
                    Some('?')
                }
                Some(((c, _), _)) => Some(c),
                None => None,
            })
            .collect()
    }
}

/// A letter less than a third as high as its line and wider than half its height: a dash or a point.
fn flat(shape: &Shape) -> bool {
    let high = 1.0 + shape.bottom - shape.top;
    high < 0.34 && shape.aspect > high * 0.5
}

/// Whether the reading of the screen may use the own recognition. Not yet: at the set of refinery pictures it
/// reads 34 % of the lines whole against 71 % of the recognition of Windows (`tests/refinery_test_set.rs`, 14.11.2026),
/// and a wrong number in the stock is worse than none. Switched on when it reads at least as well.
pub const READY: bool = false;

/// Can the own recognition read (switched on, a font shipped)?
pub fn supported() -> bool {
    READY && !Font::shipped().glyphs.is_empty()
}

/// The own recognition over one picture, for [`super::read_image`].
pub struct Recognition<'a> {
    pub bitmap: &'a Bitmap,
    pub font: &'a Font,
}

impl<'a> Recognition<'a> {
    pub fn new(bitmap: &'a Bitmap) -> Recognition<'a> {
        Recognition { bitmap, font: Font::shipped() }
    }
}

impl Ocr for Recognition<'_> {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        Ok(jobs
            .iter()
            .filter_map(|job| segment(self.bitmap, job))
            .map(|cut| cut.words.iter().map(|piece| cut.word(&self.font.read(piece), piece)).collect())
            .collect())
    }
}

#[cfg(test)]
mod tests;
