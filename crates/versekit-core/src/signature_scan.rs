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
//
// ---------------------------------------------------------------------------
// The approach (snapping the signature to the possible values, voting over
// several images) follows a work by ryze, published under the MIT license:
//
// MIT License
//
// Copyright (c) 2026 ryze
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
// ---------------------------------------------------------------------------

//! Reads the scan signature of the mining scanner from a grey image.
//!
//! The display always shows a base signature times a chunk count. The digits are the bright areas of the
//! crop: each is brought to a fixed grid and compared with templates. The value read is a POSSIBLE value, the
//! one whose digits fit best, not a digit-for-digit guess. Results carry key words, no sentences.
//!
//! ⚠ There are two grey images of the screen ([`VARIANTS`]), both are read:
//!
//! | Grey image | Value | Carries when |
//! |---|---|---|
//! | [`Variant::Green`] | green channel | the HUD digits are white with an orange-red outline: in the brightest channel the outline fills the dark insides of small digits (6, 8, 0) |
//! | [`Variant::Bright`] | brightest of the three channels | the digits themselves carry little green: in the green channel they vanish into the ground |
//!
//! A value read from the green image always counts; the bright image is only read when the green one reads
//! nothing sure ([`read_variants`]). The templates taught are kept per grey image ([`OwnFiles`]).
//!
//! ⚠ Every limit below was measured on images with a known answer; a change to one of them is measured in
//! three runs (taught old / read old, taught old / read new, taught new / read new).

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::jsonio::{self, Newline, Value};
use crate::mining_data::{self, MiningData};

pub const NORM_W: usize = 16;
pub const NORM_H: usize = 24;
const CELLS: usize = NORM_W * NORM_H;

pub const SAMPLE_FOLDER: &str = "signatur-bilder";
pub const SAMPLE_LIMIT: usize = 200;
/// Shipped with the program, folder `daten`.
pub const TEMPLATE_FILE: &str = "signatur-ziffern.json";
/// The text of the shipped file.
pub const SHIPPED_TEMPLATES: &str = include_str!("../../../daten/signatur-ziffern.json");
/// Taught by the player, without saying from which grey image: valid for both.
pub const OWN_TEMPLATE_FILE: &str = "signatur-ziffern-eigene.json";
/// Taught by the player from the green grey image.
pub const OWN_GREEN_FILE: &str = "signatur-ziffern-eigene-gruen.json";
/// Taught by the player from the bright grey image.
pub const OWN_BRIGHT_FILE: &str = "signatur-ziffern-eigene-hell.json";
/// Unix time from which digits taught can come from the green channel. A shared file of the player written
/// last before holds digits from the brightest channel only and is valid for the bright grey image alone.
pub const GREEN_LEARNED_SINCE: u64 = 1_792_890_000;

/// Added to the distance when the holes do not fit; no hard exclusion, a noisy digit can lose a hole.
const HOLE_PENALTY: f64 = 0.25;
const HOLE_POSITION_TOLERANCE: f64 = 0.12;
/// How many points a closed area needs at least to count as a hole.
const MIN_HOLE: usize = 8;
/// Worst mean distance that still counts as read, and the lead over the second best value.
const MAX_DISTANCE: f64 = 0.34;
const VALUE_MARGIN: f64 = 0.02;
/// Highest distance of ONE digit.
const MAX_DIGIT_DISTANCE: f64 = 0.26;
/// So many taught images per digit; the youngest stay.
const MAX_OWN_PER_DIGIT: usize = 24;
/// Other thresholds each taught image is cut at for derived templates.
const DERIVED_THRESHOLDS: usize = 2;
/// Most derived templates kept per digit in a file of taught digits, apart from the taught ones.
const MAX_DERIVED_PER_DIGIT: usize = 24;
/// At least so many characters in a row.
const MIN_CHARS: usize = 3;
const LEARN_HOLE_TOLERANCE: f64 = 0.07;
/// How far (share of the grid width) the lower of two holes must lie right of the upper one for a slashed zero.
const SLASH_SHIFT: f64 = 0.15;
/// Rows with more merged areas than this are not tried with the fallback cuts.
const VARIANT_MAX_MERGED: usize = 2;
/// How far (share of the digit height) the first character's bottom must lie above the others' base line to
/// count as the head of the location symbol.
const HEAD_RAISE: f64 = 0.2;
/// The last character of a row lower than this share of the digit height is the end of a unit, not a digit.
const UNIT_RATIO: f64 = 0.85;
/// The digits of the signature are at least so many points high; the small print beside the pill is lower.
const MIN_DIGIT_HEIGHT: i32 = 7;

/// Switches the two rules about units and small print off: the rows are then found by the plain rule alone,
/// which the comparison with the Python version needs.
static LEGACY_ROWS: AtomicBool = AtomicBool::new(false);

pub fn legacy_rows(on: bool) {
    LEGACY_ROWS.store(on, Ordering::Relaxed);
}
/// Show the number read when the sure digits say something else than the snapped value.
const FREE_WINS: bool = true;
const FREE_MARGIN: f64 = 0.06;

/// The search area for the pill in the game window: left, top, width, height as shares.
pub const SEARCH_AREA: (f64, f64, f64, f64) = (0.2, 0.1, 0.6, 0.75);
const CELL: i32 = 8;

/// A box around an area: left, top, right, bottom (both edges included).
pub type Box4 = [i32; 4];
pub type Pattern = [u8; CELLS];
/// Closed areas of a character: count and the highest position from the top (0..1).
pub type Holes = (usize, Option<f64>);

/// The holes a digit must have to be taught: (counts allowed, position from the top). A slashed zero
/// ([`slashed_zero`]) fits the zero and nothing else.
fn required_holes(digit: char) -> Option<(&'static [usize], Option<f64>)> {
    Some(match digit {
        '0' => (&[1], Some(0.5)),
        '1' | '2' | '3' | '5' | '7' => (&[0], None),
        '4' => (&[0, 1], None),
        '6' => (&[1], Some(0.68)),
        '8' => (&[2], None),
        '9' => (&[1], Some(0.35)),
        _ => return None,
    })
}

/// A grey image, row by row.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

impl Gray {
    pub fn new(w: usize, h: usize, data: Vec<u8>) -> Gray {
        debug_assert_eq!(data.len(), w * h);
        Gray { w, h, data }
    }
    pub fn at(&self, x: i32, y: i32) -> u8 {
        self.data[y as usize * self.w + x as usize]
    }
    fn row(&self, y: usize) -> &[u8] {
        &self.data[y * self.w..(y + 1) * self.w]
    }
}

// ------------------------------------------------------------ the two grey images

/// Which grey image of the screen a crop is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Variant {
    /// The green channel.
    Green,
    /// The brightest of blue, green and red.
    Bright,
}

/// The grey images that are read, the first one in front.
pub const VARIANTS: [Variant; 2] = [Variant::Green, Variant::Bright];

impl Variant {
    /// The key word the pages and the report know.
    pub fn key(self) -> &'static str {
        match self {
            Variant::Green => "gruen",
            Variant::Bright => "hell",
        }
    }

    pub fn from_key(key: &str) -> Option<Variant> {
        VARIANTS.into_iter().find(|v| v.key() == key)
    }

    /// The file name of the templates taught from this grey image.
    pub fn own_file(self) -> &'static str {
        match self {
            Variant::Green => OWN_GREEN_FILE,
            Variant::Bright => OWN_BRIGHT_FILE,
        }
    }
}

/// One value for each grey image.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ByVariant<T> {
    pub green: T,
    pub bright: T,
}

impl<T> ByVariant<T> {
    pub fn get(&self, variant: Variant) -> &T {
        match variant {
            Variant::Green => &self.green,
            Variant::Bright => &self.bright,
        }
    }

    pub fn get_mut(&mut self, variant: Variant) -> &mut T {
        match variant {
            Variant::Green => &mut self.green,
            Variant::Bright => &mut self.bright,
        }
    }

    pub fn map<U>(&self, mut each: impl FnMut(Variant, &T) -> U) -> ByVariant<U> {
        ByVariant { green: each(Variant::Green, &self.green), bright: each(Variant::Bright, &self.bright) }
    }
}

impl<T: Clone> ByVariant<T> {
    /// The same value for both grey images.
    pub fn both(value: T) -> ByVariant<T> {
        ByVariant { green: value.clone(), bright: value }
    }
}

/// The same place of the screen as each grey image.
pub type Images = ByVariant<Option<Gray>>;

/// The files of the templates the player taught: the shared one of the versions before the two grey images
/// (read only) and one for each grey image.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OwnFiles {
    pub shared: String,
    pub by_variant: ByVariant<String>,
}

impl OwnFiles {
    /// The files in the folder `app_file` gives the full path of a name in.
    pub fn in_folder(app_file: impl Fn(&str) -> String) -> OwnFiles {
        OwnFiles {
            shared: app_file(OWN_TEMPLATE_FILE),
            by_variant: ByVariant { green: app_file(OWN_GREEN_FILE), bright: app_file(OWN_BRIGHT_FILE) },
        }
    }

    pub fn of(&self, variant: Variant) -> &str {
        self.by_variant.get(variant)
    }

    /// Every file with its name in the archive: the shared one first.
    pub fn all(&self) -> [(&'static str, &str); 3] {
        [
            (OWN_TEMPLATE_FILE, self.shared.as_str()),
            (OWN_GREEN_FILE, self.by_variant.green.as_str()),
            (OWN_BRIGHT_FILE, self.by_variant.bright.as_str()),
        ]
    }
}

// ------------------------------------------------------------ taking the image apart

/// The line between text and ground, determined by the image itself (Otsu).
pub fn otsu(counts: &[u64; 256]) -> u8 {
    let total: u64 = counts.iter().sum();
    if total == 0 {
        return 128;
    }
    let weighted: f64 = counts.iter().enumerate().map(|(i, c)| (i as u64 * c) as f64).sum();
    let (mut sum_a, mut weight_a, mut best, mut threshold) = (0.0f64, 0u64, -1.0f64, 128u8);
    for (i, &count) in counts.iter().enumerate() {
        weight_a += count;
        if weight_a == 0 {
            continue;
        }
        let weight_b = total - weight_a;
        if weight_b == 0 {
            break;
        }
        sum_a += (i as u64 * count) as f64;
        let mean_a = sum_a / weight_a as f64;
        let mean_b = (weighted - sum_a) / weight_b as f64;
        let quality = (weight_a * weight_b) as f64 * (mean_a - mean_b).powi(2);
        if quality > best {
            best = quality;
            threshold = i as u8;
        }
    }
    threshold
}

fn histogram(raster: &Gray) -> [u64; 256] {
    let mut counts = [0u64; 256];
    for &v in &raster.data {
        counts[v as usize] += 1;
    }
    counts
}

/// Several lines: Otsu, the brightest percentiles and steps in between.
///
/// ⚠ Otsu alone does not do: in front of a bright asteroid the line slips and the digits merge with the rubble.
/// With small HUD digits all percentiles lie on the bright location symbol and Otsu on the ground of the pill;
/// the digits stand clean only in between.
pub fn thresholds(raster: &Gray) -> Vec<u8> {
    let counts = histogram(raster);
    let total: u64 = counts.iter().sum();
    let base = otsu(&counts) as i32;
    let mut found = vec![base];
    let mut top = base;
    if total > 0 {
        for share in [0.97f64, 0.99, 0.995] {
            let limit = total as f64 * share;
            let mut running = 0u64;
            for (value, count) in counts.iter().enumerate() {
                running += count;
                if running as f64 >= limit {
                    found.push(value as i32);
                    top = top.max(value as i32);
                    break;
                }
            }
        }
        for share in [0.25f64, 0.45, 0.65] {
            found.push((base as f64 + (top - base) as f64 * share) as i32);
        }
    }
    let set: BTreeSet<i32> = found.into_iter().filter(|v| (8..=245).contains(v)).collect();
    set.into_iter().map(|v| v as u8).collect()
}

/// Connected bright areas as (left, top, right, bottom), joined over the corners too; single bright points are
/// noise and fall out.
pub fn components(raster: &Gray, threshold: u8) -> Vec<Box4> {
    if raster.h == 0 {
        return Vec::new();
    }
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = Vec::new();
    let mut spans: Vec<[i32; 4]> = Vec::new(); // left, right, top, bottom
    let mut previous: Vec<(i32, i32, usize)> = Vec::new();
    for y in 0..raster.h {
        let line = raster.row(y);
        let mut current: Vec<(i32, i32, usize)> = Vec::new();
        let mut j = 0usize;
        let mut x = 0usize;
        while x < line.len() {
            if line[x] <= threshold {
                x += 1;
                continue;
            }
            let left = x as i32;
            while x < line.len() && line[x] > threshold {
                x += 1;
            }
            let right = x as i32 - 1;
            let index = parent.len();
            parent.push(index);
            spans.push([left, right, y as i32, y as i32]);
            while j < previous.len() && previous[j].1 < left - 1 {
                j += 1;
            }
            let mut k = j;
            while k < previous.len() && previous[k].0 <= right + 1 {
                let a = root(&mut parent, index);
                let b = root(&mut parent, previous[k].2);
                if a != b {
                    parent[a] = b;
                }
                k += 1;
            }
            current.push((left, right, index));
        }
        previous = current;
    }
    let mut merged: HashMap<usize, [i32; 4]> = HashMap::new();
    for (index, &[left, right, top, bottom]) in spans.iter().enumerate() {
        let r = root(&mut parent, index);
        merged
            .entry(r)
            .and_modify(|b| {
                b[0] = b[0].min(left);
                b[1] = b[1].max(right);
                b[2] = b[2].min(top);
                b[3] = b[3].max(bottom);
            })
            .or_insert([left, right, top, bottom]);
    }
    let mut boxes: Vec<Box4> =
        merged.values().filter(|b| b[1] - b[0] >= 1 && b[3] - b[2] >= 3).map(|b| [b[0], b[2], b[1], b[3]]).collect();
    boxes.sort();
    boxes
}

fn median(values: impl IntoIterator<Item = i32>) -> i32 {
    let mut ordered: Vec<i32> = values.into_iter().collect();
    ordered.sort();
    ordered.get(ordered.len() / 2).copied().unwrap_or(0)
}

/// Count of bright points per column of a box.
fn columns_of(boxr: Box4, raster: &Gray, threshold: u8) -> Vec<usize> {
    (boxr[0]..=boxr[2]).map(|x| (boxr[1]..=boxr[3]).filter(|&y| raster.at(x, y) > threshold).count()).collect()
}

/// Cuts a box into `parts` pieces: with the picture at the darkest column near the expected place, without it
/// evenly.
fn cut_box(boxr: Box4, parts: i32, reach: i32, image: Option<(&Gray, u8)>) -> Vec<Box4> {
    let wide = boxr[2] - boxr[0] + 1;
    let mut cuts: Vec<i32> = Vec::new();
    match image {
        Some((raster, threshold)) => {
            let columns = columns_of(boxr, raster, threshold);
            for i in 1..parts {
                let expected = i * wide / parts;
                let (from, to) = ((expected - reach).max(1), (wide - 1).min(expected + reach + 1));
                let mut chosen: Option<(usize, i32, i32)> = None;
                for c in from..to {
                    let key = (columns[c as usize], (c - expected).abs());
                    if chosen.is_none_or(|(a, b, _)| key < (a, b)) {
                        chosen = Some((key.0, key.1, c));
                    }
                }
                if let Some((_, _, c)) = chosen {
                    cuts.push(c);
                }
            }
        }
        None => cuts = (1..parts).map(|i| i * wide / parts).collect(),
    }
    let mut edges = vec![0];
    let set: BTreeSet<i32> = cuts.into_iter().collect();
    edges.extend(set);
    edges.push(wide);
    let mut out = Vec::new();
    for pair in edges.windows(2) {
        let (left, right) = (boxr[0] + pair[0], boxr[0] + pair[1] - 1);
        if right >= left {
            out.push([left, boxr[1], right, boxr[3]]);
        }
    }
    out
}

/// Splits merged digits again, at the darkest column; the comma stays whole (it is narrower than a digit).
pub fn split_merged(boxes: &[Box4], image: Option<(&Gray, u8)>) -> Vec<Box4> {
    if boxes.len() < 2 {
        return boxes.to_vec();
    }
    let single = median(boxes.iter().map(|b| b[2] - b[0] + 1));
    if single < 2 {
        return boxes.to_vec();
    }
    let mut result = Vec::new();
    for &boxr in boxes {
        let wide = boxr[2] - boxr[0] + 1;
        let parts = (wide as f64 / single as f64).round_ties_even() as i32;
        if !(parts >= 2 && wide as f64 >= single as f64 * 1.65) {
            result.push(boxr);
            continue;
        }
        result.extend(cut_box(boxr, parts, 1.max(single / 3), image));
    }
    result
}

/// More cuts for the case that `split_merged` gave nothing readable: with large HUD digits whole rows merge,
/// the median is then too wide itself. The narrowest area in full digit height is the single width, and for
/// each merged area the neighbouring counts are tried too. Which one is right decides the match with the
/// possible values.
pub fn split_variants(boxes: &[Box4], raster: &Gray, threshold: u8) -> Vec<Vec<Box4>> {
    if boxes.len() < 2 {
        return Vec::new();
    }
    let digit_h = median(boxes.iter().map(|b| b[3] - b[1] + 1)) as f64;
    let narrow: Vec<i32> = boxes
        .iter()
        .filter(|b| (b[3] - b[1] + 1) as f64 >= digit_h * 0.85 && (b[2] - b[0] + 1) as f64 >= digit_h * 0.35)
        .map(|b| b[2] - b[0] + 1)
        .collect();
    let Some(&single) = narrow.iter().min() else { return Vec::new() };
    let mut options: Vec<Vec<Vec<Box4>>> = Vec::new();
    for &boxr in boxes {
        let wide = boxr[2] - boxr[0] + 1;
        if (wide as f64) < single as f64 * 1.65 {
            options.push(vec![vec![boxr]]);
            continue;
        }
        let guess = 2.max((wide as f64 / single as f64).round_ties_even() as i32);
        let counts: BTreeSet<i32> = [2.max(guess - 1), guess, guess + 1].into_iter().collect();
        options.push(
            counts
                .into_iter()
                .map(|parts| cut_box(boxr, parts, 1.max(single / 3), Some((raster, threshold))))
                .collect(),
        );
    }
    let merged = options.iter().filter(|o| o.len() > 1).count();
    if merged == 0 || merged > VARIANT_MAX_MERGED {
        return Vec::new();
    }
    let mut variants: Vec<Vec<Box4>> = vec![Vec::new()];
    for choice in &options {
        let mut next = Vec::new();
        for v in &variants {
            for piece in choice {
                let mut joined = v.clone();
                joined.extend(piece.iter().copied());
                next.push(joined);
            }
        }
        variants = next;
    }
    variants
}

/// Cuts off the head of the location symbol when it ends above the base line. With large type the head is as
/// high as a digit and survives the cut by height, but it ends 4 to 5 points above the digits' base line.
fn drop_raised_head(mut chars: Vec<Box4>, digit_h: i32) -> Vec<Box4> {
    while chars.len() > MIN_CHARS {
        let base = median(chars[1..].iter().map(|c| c[3]));
        if (base - chars[0][3]) as f64 > 2f64.max(digit_h as f64 * HEAD_RAISE) {
            chars.remove(0);
        } else {
            break;
        }
    }
    chars
}

/// A separator at the third place from the end (from four characters on)? The game always sets one from a
/// thousand: narrow and flat where the digits are equally high. When the comma is pale and falls out at a
/// higher line, its gap remains: clearly wider before the last three digits than between the others.
fn separator_fits(chars: &[Box4]) -> bool {
    if chars.len() < 4 {
        return true;
    }
    let digit_h = median(chars.iter().map(|c| c[3] - c[1] + 1));
    let digit_w = median(chars.iter().map(|c| c[2] - c[0] + 1));
    let sep = chars[chars.len() - 4];
    if ((sep[3] - sep[1] + 1) as f64) <= digit_h as f64 * 0.7
        && ((sep[2] - sep[0] + 1) as f64) <= 2f64.max(digit_w as f64 * 0.7)
    {
        return true;
    }
    let gaps: Vec<i32> = chars.windows(2).map(|p| p[1][0] - p[0][2] - 1).collect();
    let before_last_three = gaps[gaps.len() - 3];
    let mut others: Vec<i32> = gaps[..gaps.len() - 3].to_vec();
    others.extend_from_slice(&gaps[gaps.len() - 2..]);
    let widest = others.iter().copied().max().unwrap_or(0);
    before_last_three >= 2.max(2 * widest + 1)
}

/// The separator out, recognised by its size, not its place.
fn only_digits(chars: &[Box4]) -> Vec<Box4> {
    if chars.len() < 2 {
        return chars.to_vec();
    }
    let middle = median(chars.iter().map(|c| c[3] - c[1] + 1)) as f64;
    chars.iter().copied().filter(|c| (c[3] - c[1] + 1) as f64 > middle * 0.7).collect()
}

/// Finds the row of characters of the signature as a list of candidates: similarly high areas on a base line,
/// led by the location symbol (cut off from the left), close together, with the separator in the right place.
/// Gives the digit areas (without separator) of the row whose middle lies nearest to the middle of the crop.
/// With `variants` the fallback cuts of `split_variants` replace the one cut.
pub fn digit_rows(boxes: &[Box4], raster: &Gray, threshold: u8, variants: bool) -> Vec<Vec<Box4>> {
    if boxes.is_empty() {
        return Vec::new();
    }
    let width = raster.w as i32;
    let mut sorted: Vec<Box4> = boxes.to_vec();
    sorted.sort_by_key(|b| b[3]);
    let mut rows: Vec<Vec<Box4>> = Vec::new();
    for boxr in sorted {
        let height = (boxr[3] - boxr[1] + 1) as f64;
        let mut placed = false;
        for row in rows.iter_mut() {
            let base = row.iter().map(|b| b[3] as i64).sum::<i64>() as f64 / row.len() as f64;
            let mean_h = row.iter().map(|b| (b[3] - b[1] + 1) as i64).sum::<i64>() as f64 / row.len() as f64;
            if (boxr[3] as f64 - base).abs() <= 2f64.max(mean_h * 0.45) && (0.3..=2.2).contains(&(height / mean_h)) {
                row.push(boxr);
                placed = true;
                break;
            }
        }
        if !placed {
            rows.push(vec![boxr]);
        }
    }
    let mut best: Vec<Vec<Box4>> = Vec::new();
    let mut best_distance: Option<f64> = None;
    for row in rows {
        if row.len() < MIN_CHARS + 1 {
            continue;
        }
        let mut ordered = row;
        ordered.sort_by_key(|b| b[0]);
        let middle = median(ordered.iter().map(|b| b[3] - b[1] + 1)) as f64;
        ordered.retain(|b| (0.3..=1.9).contains(&((b[3] - b[1] + 1) as f64 / middle)));
        if ordered.len() < MIN_CHARS + 1 {
            continue;
        }
        let mut bundles: Vec<Vec<Box4>> = vec![vec![ordered[0]]];
        for &boxr in &ordered[1..] {
            let last = bundles.last().and_then(|b| b.last()).copied().unwrap_or(boxr);
            if (boxr[0] - last[2]) as f64 > 6f64.max(middle * 2.0) {
                bundles.push(vec![boxr]);
            } else if let Some(bundle) = bundles.last_mut() {
                bundle.push(boxr);
            }
        }
        // The first of the longest bundles: a tie goes to the first.
        let mut longest = 0;
        for (i, b) in bundles.iter().enumerate() {
            if b.len() > bundles[longest].len() {
                longest = i;
            }
        }
        let ordered = bundles.swap_remove(longest);
        if ordered.len() < MIN_CHARS + 1 {
            continue;
        }
        let digit_h = median(ordered.iter().map(|b| b[3] - b[1] + 1));
        // A number with a unit (m, km, Mm, Gm: a distance) is never a signature: the unit ends in a letter
        // that is lower than the digits, while a signature ends in a digit.
        let last = ordered[ordered.len() - 1];
        if !LEGACY_ROWS.load(Ordering::Relaxed) {
            if ((last[3] - last[1] + 1) as f64) < digit_h as f64 * UNIT_RATIO {
                continue;
            }
            // The small print around the pill (names, labels) is no row of digits either.
            if digit_h < MIN_DIGIT_HEIGHT {
                continue;
            }
        }
        let mut start = 0;
        while start < ordered.len() {
            let ratio = (ordered[start][3] - ordered[start][1] + 1) as f64 / digit_h as f64;
            if (0.85..=1.15).contains(&ratio) {
                break;
            }
            start += 1;
        }
        if start == 0 || start >= ordered.len() {
            continue;
        }
        let candidates: Vec<Vec<Box4>>;
        let rest: Vec<Box4>;
        if variants {
            let mut list: Vec<Vec<Box4>> = Vec::new();
            for split in split_variants(&ordered[start..], raster, threshold) {
                let split = drop_raised_head(split, digit_h);
                if split.len() >= MIN_CHARS && !list.contains(&split) {
                    list.push(split);
                }
            }
            if list.is_empty() {
                continue;
            }
            rest = list[0].clone();
            candidates = list;
        } else {
            rest = drop_raised_head(split_merged(&ordered[start..], Some((raster, threshold))), digit_h);
            if rest.len() < MIN_CHARS || !separator_fits(&rest) {
                continue;
            }
            candidates = vec![rest.clone()];
        }
        if width != 0 {
            let distance = ((rest[0][0] + rest[rest.len() - 1][2]) as f64 / 2.0 - width as f64 / 2.0).abs();
            if best_distance.is_none_or(|d| distance < d) {
                best = candidates;
                best_distance = Some(distance);
            }
        } else if best.is_empty() || rest.len() > best[0].len() {
            best = candidates;
        }
    }
    best.iter().map(|c| only_digits(c)).collect()
}

/// An area on the fixed grid, aspect ratio kept.
pub fn normalize(raster: &Gray, boxr: Box4, threshold: u8) -> Pattern {
    let (left, top, right, bottom) = (boxr[0], boxr[1], boxr[2], boxr[3]);
    let (src_w, src_h) = (right - left + 1, bottom - top + 1);
    let factor = (NORM_W as f64 / src_w as f64).min(NORM_H as f64 / src_h as f64);
    let dst_w = 1.max((src_w as f64 * factor) as i32);
    let dst_h = 1.max((src_h as f64 * factor) as i32);
    let (pad_x, pad_y) = ((NORM_W as i32 - dst_w) / 2, (NORM_H as i32 - dst_h) / 2);
    let mut pattern = [0u8; CELLS];
    for y in 0..dst_h {
        let sy = top + (src_h - 1).min((y as f64 / factor) as i32);
        for x in 0..dst_w {
            let sx = left + (src_w - 1).min((x as f64 / factor) as i32);
            if raster.at(sx, sy) > threshold {
                pattern[((pad_y + y) as usize) * NORM_W + (pad_x + x) as usize] = 1;
            }
        }
    }
    pattern
}

/// The closed areas of a character (count, highest position from the top).
///
/// ⚠ The feature that separates the zero from the eight. Areas under `MIN_HOLE` points do not count: a single
/// bright point can pinch an opening shut and turn a zero into an eight.
pub fn holes(pattern: &Pattern) -> Holes {
    let found: Vec<f64> = hole_centres(pattern).into_iter().map(|(y, _)| y).collect();
    if found.is_empty() {
        return (0, None);
    }
    (found.len(), found.iter().copied().reduce(f64::min))
}

/// The centre of each closed area of at least `MIN_HOLE` points: (from the top, from the left) as shares of the
/// grid, in the order they are found.
fn hole_centres(pattern: &Pattern) -> Vec<(f64, f64)> {
    let (w, h) = (NORM_W as i32, NORM_H as i32);
    let mut seen = [false; CELLS];
    let mut stack: Vec<(i32, i32)> = Vec::new();
    for x in 0..w {
        stack.push((x, 0));
        stack.push((x, h - 1));
    }
    for y in 0..h {
        stack.push((0, y));
        stack.push((w - 1, y));
    }
    while let Some((x, y)) = stack.pop() {
        if !(0..w).contains(&x) || !(0..h).contains(&y) {
            continue;
        }
        let i = (y * w + x) as usize;
        if seen[i] || pattern[i] != 0 {
            continue;
        }
        seen[i] = true;
        stack.extend([(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]);
    }
    let mut found: Vec<(f64, f64)> = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            let i0 = (y0 * w + x0) as usize;
            if pattern[i0] != 0 || seen[i0] {
                continue;
            }
            let mut ys: Vec<i32> = Vec::new();
            let mut xs: Vec<i32> = Vec::new();
            let mut inner = vec![(x0, y0)];
            seen[i0] = true;
            while let Some((x, y)) = inner.pop() {
                ys.push(y);
                xs.push(x);
                for (nx, ny) in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                    if (0..w).contains(&nx) && (0..h).contains(&ny) {
                        let n = (ny * w + nx) as usize;
                        if !seen[n] && pattern[n] == 0 {
                            seen[n] = true;
                            inner.push((nx, ny));
                        }
                    }
                }
            }
            if ys.len() >= MIN_HOLE {
                found.push((
                    ys.iter().map(|&y| y as i64).sum::<i64>() as f64 / ys.len() as f64 / h as f64,
                    xs.iter().map(|&x| x as i64).sum::<i64>() as f64 / xs.len() as f64 / w as f64,
                ));
            }
        }
    }
    found
}

/// A zero whose slash splits its inside in two: exactly two holes, the lower clearly right of the upper.
///
/// ⚠ The two holes of an 8 (and of a 6 whose opening ran shut) lie one above the other; only the sideways shift
/// tells the slashed zero from them. Measured on the stretched pattern, so that the width of the digit does not
/// shrink the shift.
pub fn slashed_zero(pattern: &Pattern) -> bool {
    let mut centres = hole_centres(&fill(pattern));
    if centres.len() != 2 {
        return false;
    }
    centres.sort_by(|a, b| a.0.total_cmp(&b.0));
    centres[1].1 - centres[0].1 >= SLASH_SHIFT
}

/// Same hole structure? Count equal and position near enough. The tolerance is so narrow that the hole of the
/// 6 (low) and that of the 0 (middle) are not taken as equal.
fn holes_match(a: Holes, b: Holes) -> bool {
    if a.0 != b.0 {
        return false;
    }
    match (a.1, b.1) {
        (Some(x), Some(y)) => (x - y).abs() <= HOLE_POSITION_TOLERANCE,
        _ => true,
    }
}

/// Does the hole structure fit the digit it is to be taught as?
pub fn plausible_template(digit: char, pattern: &Pattern) -> bool {
    let Some((counts, position)) = required_holes(digit) else { return false };
    if slashed_zero(pattern) {
        return digit == '0';
    }
    let actual = holes(pattern);
    if !counts.contains(&actual.0) {
        return false;
    }
    match (position, actual.1) {
        (Some(required), Some(found)) => (found - required).abs() <= LEARN_HOLE_TOLERANCE,
        _ => true,
    }
}

// ------------------------------------------------------------ templates

/// A pattern cut to its outline and stretched over the whole grid.
pub fn fill(pattern: &Pattern) -> Pattern {
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0usize, 0usize);
    let mut any = false;
    for (i, &v) in pattern.iter().enumerate() {
        if v != 0 {
            any = true;
            let (x, y) = (i % NORM_W, i / NORM_W);
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
        }
    }
    if !any {
        return *pattern;
    }
    let (width, height) = (right - left + 1, bottom - top + 1);
    let mut result = [0u8; CELLS];
    for y in 0..NORM_H {
        let source = (top + y * height / NORM_H) * NORM_W + left;
        for x in 0..NORM_W {
            result[y * NORM_W + x] = pattern[source + x * width / NORM_W];
        }
    }
    result
}

/// The pattern moved by whole cells; cells pushed off the grid are lost.
fn shifted(pattern: &Pattern, dx: i32, dy: i32) -> Pattern {
    let mut result = [0u8; CELLS];
    for y in 0..NORM_H as i32 {
        for x in 0..NORM_W as i32 {
            let (sx, sy) = (x - dx, y - dy);
            if (0..NORM_W as i32).contains(&sx) && (0..NORM_H as i32).contains(&sy) {
                result[(y as usize) * NORM_W + x as usize] = pattern[(sy as usize) * NORM_W + sx as usize];
            }
        }
    }
    result
}

/// Each cell set when at least `need` of the cells of its 3x3 neighbourhood (itself included) are set.
fn neighbourhood(pattern: &Pattern, need: usize) -> Pattern {
    let mut result = [0u8; CELLS];
    for y in 0..NORM_H as i32 {
        for x in 0..NORM_W as i32 {
            let mut count = 0;
            for (dx, dy) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy))) {
                let (nx, ny) = (x + dx, y + dy);
                if (0..NORM_W as i32).contains(&nx) && (0..NORM_H as i32).contains(&ny) {
                    count += usize::from(pattern[(ny as usize) * NORM_W + nx as usize] != 0);
                }
            }
            result[(y as usize) * NORM_W + x as usize] = u8::from(count >= need);
        }
    }
    result
}

/// Variants of a pattern a second grab of the same digit could look like: moved one cell to each side, slightly
/// blurred and one cell thicker. Only those that plausibly are the digit and differ from the
/// pattern and from each other (after [`fill`]) are returned.
pub fn derived_patterns(digit: char, pattern: &Pattern) -> Vec<Pattern> {
    let mut raw: Vec<Pattern> =
        [(-1, 0), (1, 0), (0, -1), (0, 1)].iter().map(|&(dx, dy)| shifted(pattern, dx, dy)).collect();
    raw.push(neighbourhood(pattern, 4));
    raw.push(neighbourhood(pattern, 1));
    keep_new(digit, &fill(pattern), raw)
}

/// The candidates that, filled, are plausible for the digit and differ from the base and from each other.
fn keep_new(digit: char, base: &Pattern, candidates: Vec<Pattern>) -> Vec<Pattern> {
    let mut out: Vec<Pattern> = Vec::new();
    for candidate in candidates {
        let filled = fill(&candidate);
        if filled != *base && !out.contains(&filled) && plausible_template(digit, &filled) {
            out.push(filled);
        }
    }
    out
}

#[derive(Clone, Debug)]
struct Example {
    pattern: Pattern,
    bits: [u64; CELLS / 64],
    holes: Holes,
    /// Shaped like a slashed zero ([`slashed_zero`]).
    slash: bool,
}

fn bits_of(pattern: &Pattern) -> [u64; CELLS / 64] {
    let mut bits = [0u64; CELLS / 64];
    for (i, &v) in pattern.iter().enumerate() {
        if v != 0 {
            bits[i / 64] |= 1 << (i % 64);
        }
    }
    bits
}

/// The templates of the digits, each with its hole structure, in the order of the files.
#[derive(Clone, Debug, Default)]
pub struct Templates {
    digits: Vec<(char, Vec<Example>)>,
}

impl Templates {
    pub fn is_empty(&self) -> bool {
        self.digits.is_empty()
    }

    /// How many of the ten digits have templates.
    pub fn learned_digits(&self) -> usize {
        "0123456789".chars().filter(|d| self.digits.iter().any(|(c, e)| c == d && !e.is_empty())).count()
    }

    /// For each digit in the order of the digits: how many examples, and their points weighted by place.
    pub fn digest(&self) -> Vec<(char, usize, u64)> {
        let mut out: Vec<(char, usize, u64)> = self
            .digits
            .iter()
            .map(|(digit, examples)| {
                let weighted = examples
                    .iter()
                    .map(|e| e.pattern.iter().enumerate().map(|(i, &v)| i as u64 * u64::from(v)).sum::<u64>())
                    .sum();
                (*digit, examples.len(), weighted)
            })
            .collect();
        out.sort_by_key(|entry| entry.0);
        out
    }

    /// Adds the derived variants ([`derived_patterns`]) of every example held now.
    pub fn add_derived(&mut self) {
        for (digit, examples) in &mut self.digits {
            let originals: Vec<Pattern> = examples.iter().map(|e| e.pattern).collect();
            for pattern in originals {
                for variant in derived_patterns(*digit, &pattern) {
                    if !examples.iter().any(|e| e.pattern == variant) {
                        examples.push(Example {
                            pattern: variant,
                            bits: bits_of(&variant),
                            holes: holes(&variant),
                            slash: slashed_zero(&variant),
                        });
                    }
                }
            }
        }
    }

    /// Adds the examples of one file (`{"format": 1, "ziffern": {"0": [[…384…], …]}}`); a broken file adds none.
    pub fn add_file(&mut self, path: &str) {
        if let Ok(data) = jsonio::read_json(path) {
            self.add_value(&data);
        }
    }

    /// As [`Templates::add_file`], from the text of a file.
    pub fn add_text(&mut self, text: &str) {
        if let Ok(data) = jsonio::loads(text) {
            self.add_value(&data);
        }
    }

    fn add_value(&mut self, data: &Value) {
        if data.get("format") != Some(&Value::int(1)) {
            return;
        }
        for field in ["ziffern", "abgeleitet"] {
            if let Some(Value::Object(digits)) = data.get(field) {
                self.add_digits(digits);
            }
        }
    }

    fn add_digits(&mut self, digits: &[(String, Value)]) {
        for (key, examples) in digits {
            let mut chars = key.chars();
            let (Some(digit), None) = (chars.next(), chars.next()) else { continue };
            if required_holes(digit).is_none() {
                continue;
            }
            let at = match self.digits.iter().position(|(c, _)| *c == digit) {
                Some(at) => at,
                None => {
                    self.digits.push((digit, Vec::new()));
                    self.digits.len() - 1
                }
            };
            let Value::Array(examples) = examples else { continue };
            for example in examples {
                let Some(pattern) = pattern_of(example) else { continue };
                let filled = fill(&pattern);
                let bucket = &mut self.digits[at].1;
                if !bucket.iter().any(|e| e.pattern == filled) {
                    bucket.push(Example {
                        pattern: filled,
                        bits: bits_of(&filled),
                        holes: holes(&filled),
                        slash: slashed_zero(&filled),
                    });
                }
            }
        }
    }
}

fn pattern_of(value: &Value) -> Option<Pattern> {
    let Value::Array(list) = value else { return None };
    if list.len() != CELLS {
        return None;
    }
    let mut pattern = [0u8; CELLS];
    for (slot, v) in pattern.iter_mut().zip(list) {
        *slot = match v {
            Value::Int(text) => u8::from(text != "0"),
            Value::Bool(b) => u8::from(*b),
            Value::Float(x) => u8::from(*x != 0.0),
            _ => return None,
        };
    }
    Some(pattern)
}

/// The shipped templates (text of the file) and the taught ones (file of the player).
pub fn templates(shipped: &str, own_file: &str) -> Templates {
    let mut known = Templates::default();
    known.add_text(shipped);
    known.add_file(own_file);
    known
}

/// For which grey images the shared file of the templates taught holds good. Written last before
/// [`GREEN_LEARNED_SINCE`], its digits all come from the brightest channel: compared in the green channel they
/// make clear digits unclear or let a wrong digit fit better. Then it holds good for the bright grey image
/// alone, else for both.
pub fn shared_own_variants(shared_file: &str) -> ByVariant<bool> {
    let written = std::fs::metadata(shared_file)
        .and_then(|m| m.modified())
        .map(|time| time.duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64()));
    match written {
        Ok(written) if written < GREEN_LEARNED_SINCE as f64 => ByVariant { green: false, bright: true },
        _ => ByVariant::both(true),
    }
}

/// The shipped templates, the shared ones where they hold good for this grey image, and those taught from it.
pub fn templates_for(shipped: &str, files: &OwnFiles, variant: Variant) -> Templates {
    let mut known = Templates::default();
    known.add_text(shipped);
    if *shared_own_variants(&files.shared).get(variant) {
        known.add_file(&files.shared);
    }
    known.add_file(files.of(variant));
    known
}

/// The templates of each grey image ([`templates_for`]).
pub fn templates_by_variant(shipped: &str, files: &OwnFiles) -> ByVariant<Templates> {
    ByVariant {
        green: templates_for(shipped, files, Variant::Green),
        bright: templates_for(shipped, files, Variant::Bright),
    }
}

// ------------------------------------------------------------ reading

/// All numbers the scanner can show: every signature times every chunk count, plus the base signatures.
pub fn possible_values(mining: &MiningData) -> Vec<u64> {
    let mut values: BTreeSet<u64> = BTreeSet::new();
    for element in mining.elements() {
        if element.signature == 0.0 {
            continue;
        }
        for count in 1..=mining_data::max_chunks(&element.rarity) {
            values.insert((element.signature * f64::from(count)) as u64);
        }
    }
    for (_, signature, most) in mining.deposits() {
        for count in 1..=most {
            values.insert((signature * f64::from(count)) as u64);
        }
    }
    values.into_iter().collect()
}

/// Per character the distance to each digit that has templates.
pub type Column = Vec<(char, f64)>;

fn column_get(column: &Column, digit: char) -> Option<f64> {
    column.iter().find(|(c, _)| *c == digit).map(|(_, d)| *d)
}

fn score_value(table: &[Column], text: &str) -> Option<f64> {
    let (mut total, mut worst) = (0.0f64, 0.0f64);
    for (column, digit) in table.iter().zip(text.chars()) {
        let distance = column_get(column, digit)?;
        total += distance;
        worst = worst.max(distance);
    }
    // ⚠ The mean alone hides a single wrong digit: `3,400` would become `3,000`.
    if worst > MAX_DIGIT_DISTANCE {
        return None;
    }
    Some(total / table.len() as f64)
}

/// The distance to the nearest example. A different hole structure costs [`HOLE_PENALTY`]; so does a slashed
/// zero against two holes one above the other, which the count and the highest position alone take as equal.
///
/// Returns (with the hole penalty, by the points alone).
fn nearest(bits: &[u64; CELLS / 64], own: Holes, own_slash: bool, examples: &[Example]) -> (f64, f64) {
    let size = CELLS as f64;
    let (mut best, mut plain): (Option<f64>, Option<f64>) = (None, None);
    for example in examples {
        let ones: u32 = bits.iter().zip(&example.bits).map(|(a, b)| (a ^ b).count_ones()).sum();
        let same = holes_match(own, example.holes) && own_slash == example.slash;
        let penalty = if same { 0.0 } else { HOLE_PENALTY };
        let points = f64::from(ones) / size;
        let distance = points + penalty;
        if best.is_none_or(|b| distance < b) {
            best = Some(distance);
        }
        if plain.is_none_or(|b| points < b) {
            plain = Some(points);
        }
    }
    (best.unwrap_or(f64::INFINITY), plain.unwrap_or(f64::INFINITY))
}

/// An 8 that by its points alone looks at least as much like a 6 counts as unclear.
///
/// ⚠ A 6 whose opening ran shut has two holes one above the other like an 8. Every 6 template then costs the
/// hole penalty and the 8 wins although it fits no better: `36,000` was read as `38,000`.
fn shut_six_guard(own: Holes, own_slash: bool, column: &mut Column, plain: &[(char, f64)]) {
    if own.0 != 2 || own_slash {
        return;
    }
    let of = |digit: char| plain.iter().find(|(d, _)| *d == digit).map(|(_, x)| *x);
    if let (Some(six), Some(eight)) = (of('6'), of('8'))
        && eight >= six
        && let Some(entry) = column.iter_mut().find(|(d, _)| *d == '8')
    {
        entry.1 += HOLE_PENALTY;
    }
}

/// Per character the distance to each digit.
///
/// ⚠ Compared STRETCHED (`fill`): both sides fill the grid. The digits of the screen and those of the shipped
/// templates differ in aspect ratio; with the ratio kept no template fits.
pub fn digit_table(patterns: &[Pattern], known: &Templates) -> Vec<Column> {
    patterns
        .iter()
        .map(|pattern| {
            let filled = fill(pattern);
            let own = holes(&filled);
            let own_slash = slashed_zero(&filled);
            let bits = bits_of(&filled);
            let (mut column, plain): (Column, Vec<(char, f64)>) = known
                .digits
                .iter()
                .filter(|(_, examples)| !examples.is_empty())
                .map(|(digit, examples)| {
                    let (best, points) = nearest(&bits, own, own_slash, examples);
                    ((*digit, best), (*digit, points))
                })
                .unzip();
            shut_six_guard(own, own_slash, &mut column, &plain);
            column
        })
        .collect()
}

/// The number digit by digit, only when EVERY digit is sure.
pub fn free_reading(table: &[Column]) -> Option<u64> {
    let mut digits = String::new();
    for column in table {
        let mut ranked: Vec<(f64, char)> = column.iter().map(|(d, x)| (*x, *d)).collect();
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if ranked.is_empty() || ranked[0].0 > MAX_DIGIT_DISTANCE {
            return None;
        }
        if ranked.len() > 1 && ranked[1].0 - ranked[0].0 < FREE_MARGIN {
            return None;
        }
        digits.push(ranked[0].1);
    }
    if digits.is_empty() || digits.starts_with('0') {
        return None;
    }
    digits.parse().ok()
}

/// Picks the possible value that fits best: (value, mean distance) or (None, distance).
pub fn match_values(patterns: &[Pattern], known: &Templates, values: &[u64]) -> (Option<u64>, f64) {
    if patterns.is_empty() || known.is_empty() || values.is_empty() {
        return (None, 1.0);
    }
    if !values.iter().any(|v| v.to_string().len() == patterns.len()) {
        return (None, 1.0);
    }
    match_table(&digit_table(patterns, known), values)
}

/// Whether every possible value ends in 0 or 5. Only then a number read digit by digit ([`free_reading`]) is
/// held to that ending ([`on_five_grid`]).
fn ends_on_five_grid(values: &[u64]) -> bool {
    values.iter().all(|v| v.is_multiple_of(5))
}

/// The free reading `free` with its last digit set to the nearer of 0 and 5 (by the distances of the last
/// character, a tie goes to 0), when the possible values all end in 0 or 5 and `free` does not. `None` when the
/// last character has no template for 0 or 5, or that sister value does not fit its digits either.
fn on_five_grid(table: &[Column], values: &[u64], free: u64) -> Option<u64> {
    if free.is_multiple_of(5) || !ends_on_five_grid(values) {
        return Some(free);
    }
    let last = table.last()?;
    let (zero, five) = (column_get(last, '0')?, column_get(last, '5')?);
    let sister = free - free % 10 + u64::from(five < zero) * 5;
    score_value(table, &sister.to_string()).filter(|mean| *mean <= MAX_DISTANCE).map(|_| sister)
}

/// [`match_values`] on the distances of the characters.
fn match_table(table: &[Column], values: &[u64]) -> (Option<u64>, f64) {
    let mut scored: Vec<(f64, u64)> = Vec::new();
    for &value in values {
        let text = value.to_string();
        if text.len() != table.len() {
            continue;
        }
        if let Some(score) = score_value(table, &text) {
            scored.push((score, value));
        }
    }
    if scored.is_empty() {
        return (None, 1.0);
    }
    scored.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let (best, value) = scored[0];
    if best > MAX_DISTANCE {
        return (None, best);
    }
    if scored.len() > 1 && scored[1].0 - best < VALUE_MARGIN {
        return (None, best);
    }
    // ⚠ Do not snap when the digits are sure of something else: a value the mining data does not know (`1,700`)
    // would be read as `7,200` although every digit is sure.
    if let Some(free) = free_reading(table).and_then(|free| on_five_grid(table, values, free))
        && free != value
    {
        return (FREE_WINS.then_some(free), best);
    }
    (Some(value), best)
}

/// Why nothing was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    NoText,
    NotTaught,
    NoValues,
    Unsure,
}

impl Reason {
    /// The key word the pages know.
    pub fn key(self) -> &'static str {
        match self {
            Reason::NoText => "kein_text",
            Reason::NotTaught => "nicht_angelernt",
            Reason::NoValues => "keine_werte",
            Reason::Unsure => "unsicher",
        }
    }
}

/// The result of reading one crop.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    pub value: Option<u64>,
    pub reason: Option<Reason>,
    /// How well it fitted (0 = exactly).
    pub distance: f64,
    /// The digit areas of the best row, for teaching.
    pub digits: Vec<Box4>,
    pub threshold: Option<u8>,
}

impl Reading {
    fn silent() -> Reading {
        Reading { value: None, reason: Some(Reason::NoText), distance: 1.0, digits: Vec::new(), threshold: None }
    }
}

/// Reads the signature in a crop.
pub fn read(raster: &Gray, known: &Templates, values: &[u64]) -> Reading {
    read_in(raster, known, values, false)
}

/// A row of a crop counts as bright ground at a threshold when at least this share of its points is brighter.
const GROUND_SHARE: f64 = 0.9;

/// The crop with the rows of bright ground at `threshold` made dark: a bright planet or haze right under the row
/// of digits, which at that threshold would run into the digits and swallow them. A row of digits always has the
/// ground of the pill between its strokes and never counts as ground.
fn without_ground(raster: &Gray, threshold: u8) -> Gray {
    let mut out = raster.clone();
    let least = (raster.w as f64 * GROUND_SHARE).ceil() as usize;
    for row in out.data.chunks_mut(raster.w.max(1)) {
        if row.iter().filter(|v| **v > threshold).count() >= least {
            row.fill(0);
        }
    }
    out
}

/// [`read`]; with `ground` the rows of bright ground are left out at every threshold ([`without_ground`]).
fn read_in(raster: &Gray, known: &Templates, values: &[u64], ground: bool) -> Reading {
    let mut result = Reading::silent();
    if raster.w == 0 || raster.h == 0 {
        return result;
    }
    let levels = thresholds(raster);
    let mut boxes_at: HashMap<u8, Vec<Box4>> = HashMap::new();
    let mut boxes = |threshold: u8| -> Vec<Box4> {
        boxes_at
            .entry(threshold)
            .or_insert_with(|| {
                if ground {
                    components(&without_ground(raster, threshold), threshold)
                } else {
                    components(raster, threshold)
                }
            })
            .clone()
    };
    let mut best: Option<(f64, u64, u8, Vec<Box4>)> = None;
    let mut fallback: Option<(u8, Vec<Box4>)> = None;
    // ⚠ Two passes: first the usual cut, and ONLY when that reads nothing the fallback cuts. So the fallback
    // cannot change a reading that was right before.
    for variants in [false, true] {
        if best.is_some() || known.is_empty() {
            break;
        }
        for &threshold in &levels {
            for digits in digit_rows(&boxes(threshold), raster, threshold, variants) {
                if !variants && fallback.as_ref().is_none_or(|f| digits.len() > f.1.len()) {
                    fallback = Some((threshold, digits.clone()));
                }
                let patterns: Vec<Pattern> = digits.iter().map(|&b| normalize(raster, b, threshold)).collect();
                let (value, distance) = match_values(&patterns, known, values);
                if let Some(value) = value
                    && best.as_ref().is_none_or(|b| distance < b.0)
                {
                    best = Some((distance, value, threshold, digits));
                } else if best.is_none() && result.distance > distance {
                    result.distance = distance;
                }
            }
        }
    }
    // ⚠ When no threshold reads a value, the digit areas of the longest row are sampled again at every higher
    // threshold. With a faint location symbol the row stands only at the lowest threshold, where the upper
    // opening of a 6 runs shut and it gets two holes like an 8.
    if best.is_none()
        && !known.is_empty()
        && let Some((found_at, digits)) = &fallback
    {
        for &threshold in levels.iter().filter(|&&t| t > *found_at) {
            let patterns: Vec<Pattern> = digits.iter().map(|&b| normalize(raster, b, threshold)).collect();
            let (value, distance) = match_values(&patterns, known, values);
            if let Some(value) = value
                && best.as_ref().is_none_or(|b| distance < b.0)
            {
                best = Some((distance, value, threshold, digits.clone()));
            }
        }
    }
    if known.is_empty() {
        for &threshold in &levels {
            for digits in digit_rows(&boxes(threshold), raster, threshold, false) {
                if fallback.as_ref().is_none_or(|f| digits.len() > f.1.len()) {
                    fallback = Some((threshold, digits));
                }
            }
        }
    }
    if let Some((distance, value, threshold, digits)) = best {
        result.value = Some(value);
        result.reason = None;
        result.distance = distance;
        result.threshold = Some(threshold);
        result.digits = digits;
        return result;
    }
    if let Some((threshold, digits)) = fallback {
        result.threshold = Some(threshold);
        result.digits = digits;
        result.reason = Some(if known.is_empty() {
            Reason::NotTaught
        } else if values.is_empty() {
            Reason::NoValues
        } else {
            Reason::Unsure
        });
    }
    result
}

/// How far left of the first digit the location symbol may stand, and how far its parts may lie apart, in
/// heights of a digit.
const SYMBOL_REACH: f64 = 1.2;
const SYMBOL_JOIN: f64 = 0.6;
/// The size of the location symbol in heights of a digit: width, height, and the widest it may be against its height.
const SYMBOL_WIDTH: (f64, f64) = (0.7, 2.0);
const SYMBOL_HEIGHT: (f64, f64) = (1.15, 2.5);
const SYMBOL_ASPECT: f64 = 1.5;

/// Does the row of digits of the reading stand in a signature box: with the location symbol left of the first
/// digit? A symbol is a compact figure taller than a digit, close to the row; words of letters, a distance with
/// its unit or a label are not, so such a picture is never offered for teaching.
pub fn has_locator(raster: &Gray, reading: &Reading) -> bool {
    let (Some(threshold), Some(first)) = (reading.threshold, reading.digits.first()) else {
        return false;
    };
    let top = reading.digits.iter().map(|d| d[1]).min().unwrap_or(first[1]);
    let bottom = reading.digits.iter().map(|d| d[3]).max().unwrap_or(first[3]);
    let left = reading.digits.iter().map(|d| d[0]).min().unwrap_or(first[0]);
    let height = f64::from(bottom - top + 1);
    if height < 1.0 {
        return false;
    }
    let near = |gap: i32| f64::from(gap) <= SYMBOL_JOIN * height;
    // The parts of the symbol: left of the digits and touching the height of the row.
    let mut parts: Vec<Box4> = components(raster, threshold)
        .into_iter()
        .filter(|b| b[2] < left && b[3] >= top - height as i32 && b[1] <= bottom + height as i32)
        .collect();
    parts.sort_by_key(|b| std::cmp::Reverse(b[2]));
    let Some(nearest) = parts.first().copied() else {
        return false;
    };
    if f64::from(left - nearest[2] - 1) > SYMBOL_REACH * height {
        return false;
    }
    // Grow the figure by every part that lies close to it.
    let mut figure = nearest;
    let mut taken = vec![false; parts.len()];
    taken[0] = true;
    loop {
        let mut grew = false;
        for (at, part) in parts.iter().enumerate() {
            if taken[at] {
                continue;
            }
            let across = (part[0] - figure[2] - 1).max(figure[0] - part[2] - 1);
            let along = (part[1] - figure[3] - 1).max(figure[1] - part[3] - 1);
            if near(across) && near(along) {
                taken[at] = true;
                figure =
                    [figure[0].min(part[0]), figure[1].min(part[1]), figure[2].max(part[2]), figure[3].max(part[3])];
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    let (w, h) = (f64::from(figure[2] - figure[0] + 1), f64::from(figure[3] - figure[1] + 1));
    (SYMBOL_WIDTH.0 * height..=SYMBOL_WIDTH.1 * height).contains(&w)
        && (SYMBOL_HEIGHT.0 * height..=SYMBOL_HEIGHT.1 * height).contains(&h)
        && w <= SYMBOL_ASPECT * h
}

// ------------------------------------------------------------ finding the pill

/// Where could a pill stand? Boxes (left, top, width, height), the most central first.
///
/// Works on the BGRA bytes: nests of short bright strokes in the size of a number; whether it is a signature
/// `read` decides afterwards, chat and labels fall out there.
pub fn pill_candidates(raw: &[u8], width: i32, height: i32) -> Vec<(i32, i32, i32, i32)> {
    candidates_in(raw, width, height, None).0
}

/// The least and most width and height of a nest of bright strokes that can be a pill, in points.
const NEST_WIDTH: (i32, i32) = (32, 320);
const NEST_HEIGHT: (i32, i32) = (8, 40);

/// A bright run longer than this many points is no stroke of a digit but bright ground: a planet or haze behind
/// the pill. Bound to the size of a pill, not to a number of its own: a stroke across a digit is never wider than
/// the digit is high, and no digit is higher than the highest nest ([`NEST_HEIGHT`]).
const GROUND_RUN: i32 = NEST_HEIGHT.1;

/// [`pill_candidates`] with the runs of bright ground left out: the second look of [`search`].
///
/// ⚠ The report of 10.11.2026 (Drake Golem, 3840 × 2160): the pill stands under the middle screen of the cockpit,
/// right above a bright planet in haze. From the lower edge of the digits on, every point of the whole width is as
/// bright in the green channel as the digits; their cells join those of the haze into one nest far too high to be a
/// number, and no candidate is left.
pub fn pill_candidates_without_ground(raw: &[u8], width: i32, height: i32) -> Vec<(i32, i32, i32, i32)> {
    candidates_in(raw, width, height, Some(GROUND_RUN)).0
}

/// The candidates, and whether a run of bright ground ([`GROUND_RUN`]) was there at all: only then does a second
/// look without it have anything to find.
fn candidates_in(raw: &[u8], width: i32, height: i32, ground_run: Option<i32>) -> (Vec<(i32, i32, i32, i32)>, bool) {
    if width <= 0 || height <= 0 || raw.len() < (width * height * 4) as usize {
        return (Vec::new(), false);
    }
    let mut ground = false;
    let mut cells: HashMap<(i32, i32), i32> = HashMap::new();
    let mut y = 0;
    while y < height {
        let row = (y * width) as usize * 4;
        let mut x = 0;
        while x < width {
            if raw[row + x as usize * 4 + 1] < 165 {
                x += 1;
                continue;
            }
            // Runs of bright points, cut into pieces of at most 14, each counted in the cell of its start.
            let start = x;
            while x < width && raw[row + x as usize * 4 + 1] >= 165 {
                x += 1;
            }
            if x - start > GROUND_RUN {
                ground = true;
                if ground_run.is_some_and(|longest| x - start > longest) {
                    continue;
                }
            }
            let mut at = start;
            while at < x {
                let end = (at + 14).min(x);
                *cells.entry((y / CELL, at / CELL)).or_insert(0) += end - at;
                at = end;
            }
        }
        y += 2;
    }
    let hot: BTreeSet<(i32, i32)> = cells.into_iter().filter(|(_, n)| *n >= 5).map(|(k, _)| k).collect();
    let mut seen: BTreeSet<(i32, i32)> = BTreeSet::new();
    let mut found: Vec<(f64, (i32, i32, i32, i32))> = Vec::new();
    for &start in &hot {
        if seen.contains(&start) {
            continue;
        }
        let mut stack = vec![start];
        let mut group: Vec<(i32, i32)> = Vec::new();
        seen.insert(start);
        while let Some((cy, cx)) = stack.pop() {
            group.push((cy, cx));
            for dy in -1..=1 {
                for dx in -2..=2 {
                    let near = (cy + dy, cx + dx);
                    if hot.contains(&near) && seen.insert(near) {
                        stack.push(near);
                    }
                }
            }
        }
        let ys: Vec<i32> = group.iter().map(|c| c.0).collect();
        let xs: Vec<i32> = group.iter().map(|c| c.1).collect();
        let (min_x, max_x) = (*xs.iter().min().unwrap_or(&0), *xs.iter().max().unwrap_or(&0));
        let (min_y, max_y) = (*ys.iter().min().unwrap_or(&0), *ys.iter().max().unwrap_or(&0));
        let (w, h) = ((max_x - min_x + 1) * CELL, (max_y - min_y + 1) * CELL);
        if (NEST_WIDTH.0..=NEST_WIDTH.1).contains(&w) && (NEST_HEIGHT.0..=NEST_HEIGHT.1).contains(&h) {
            let left = 0.max(min_x * CELL - 28);
            let top = 0.max(min_y * CELL - 10);
            let boxr = (left, top, (width - left).min(w + 56), (height - top).min(h + 20));
            let distance = (left as f64 + w as f64 / 2.0 - width as f64 / 2.0).abs()
                + (top as f64 + h as f64 / 2.0 - height as f64 / 2.0).abs();
            found.push((distance, boxr));
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    (found.into_iter().map(|(_, b)| b).collect(), ground)
}

/// The grey value of one point (four bytes B, G, R, A) in the grey image `variant`.
fn gray_value(point: &[u8], variant: Variant) -> u8 {
    match variant {
        Variant::Green => point[1],
        Variant::Bright => point[0].max(point[1]).max(point[2]),
    }
}

/// A part of BGRA bytes as grey image `variant`.
pub fn gray_crop(raw: &[u8], width: i32, boxr: (i32, i32, i32, i32), variant: Variant) -> Gray {
    let (left, top, w, h) = boxr;
    let mut data = Vec::with_capacity((w * h).max(0) as usize);
    for y in top..top + h {
        let start = ((y * width + left) * 4) as usize;
        for x in 0..w as usize {
            data.push(gray_value(&raw[start + x * 4..start + x * 4 + 4], variant));
        }
    }
    Gray::new(w.max(0) as usize, h.max(0) as usize, data)
}

/// BGRA bytes as grey image `variant`.
pub fn to_gray(raw: &[u8], width: i32, height: i32, variant: Variant) -> Gray {
    gray_crop(raw, width, (0, 0, width, height), variant)
}

/// What the search over a large part of the screen found.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub reading: Reading,
    /// The crop around the pill in the grey image the reading was taken from, for showing.
    pub image: Option<Gray>,
    /// The grey image the reading was taken from.
    pub variant: Option<Variant>,
    /// The crop around the pill in every grey image, for teaching.
    pub images: Images,
    /// Where it was cut from: left, top, width, height.
    pub boxr: Option<(i32, i32, i32, i32)>,
}

impl Found {
    pub fn silent() -> Found {
        Found { reading: Reading::silent(), image: None, variant: None, images: Images::default(), boxr: None }
    }
}

/// Is `result`, read later, better than `other`? Only when `other` holds no value (see [`read_variants`]).
fn safer(result: &Reading, other: &Reading) -> bool {
    if result.value.is_some() {
        return other.value.is_none();
    }
    other.value.is_none() && !result.digits.is_empty() && other.digits.is_empty()
}

/// Reads the same place in each grey image; the surest reading counts.
///
/// | Case | Chosen |
/// |---|---|
/// | the front grey image reads a value | that one, also when a later one reads another value better; the later ones are not even read |
/// | only a later one reads a value | that one |
/// | none reads a value | the front one with a row of digits, else the front one |
///
/// ⚠⚠ **No comparing of distances.** In the brightest channel the orange-red outline runs into small digits;
/// there a 5 can fit 1 better than it fits 5 in the green channel (56,000 read as 16,000). The later grey image
/// fills gaps only.
pub fn read_variants(images: &Images, known: &ByVariant<Templates>, values: &[u64]) -> Found {
    read_variants_in(images, known, values, false)
}

/// [`read_variants`]; with `ground` every grey image is read with the rows of bright ground left out.
fn read_variants_in(images: &Images, known: &ByVariant<Templates>, values: &[u64], ground: bool) -> Found {
    let mut chosen: Option<(Reading, Variant)> = None;
    for variant in VARIANTS {
        let Some(raster) = images.get(variant) else { continue };
        let reading = read_in(raster, known.get(variant), values, ground);
        if chosen.as_ref().is_none_or(|(current, _)| safer(&reading, current)) {
            chosen = Some((reading, variant));
        }
        if chosen.as_ref().is_some_and(|(current, _)| current.value.is_some()) {
            break;
        }
    }
    match chosen {
        Some((reading, variant)) => Found {
            reading,
            image: images.get(variant).clone(),
            variant: Some(variant),
            images: images.clone(),
            boxr: None,
        },
        None => Found { images: images.clone(), ..Found::silent() },
    }
}

/// Finds the pill in a large crop and reads it in every grey image. Without a value: the first candidate with a
/// row of digits. The pill is looked for in the green channel only.
///
/// ⚠ Two looks. The first is the one every limit was measured with. Only when it reads no value AND it met bright
/// ground (a run longer than [`GROUND_RUN`]), a second look leaves that ground out (long bright runs when finding
/// the pill, rows of bright ground when reading it): a pill right above a bright planet. So the second look cannot
/// change a value the first one read, and a beat without a pill and without bright ground costs what it did.
pub fn search(
    raw: &[u8],
    width: i32,
    height: i32,
    known: &ByVariant<Templates>,
    values: &[u64],
    limit: usize,
) -> Found {
    let (all, ground) = candidates_in(raw, width, height, None);
    let boxes: Vec<_> = all.into_iter().take(limit).collect();
    let first = look(raw, width, &boxes, known, values, false);
    if first.reading.value.is_some() || !ground {
        return first;
    }
    let more: Vec<_> = pill_candidates_without_ground(raw, width, height)
        .into_iter()
        .filter(|b| !boxes.contains(b))
        .take(limit)
        .collect();
    let second = look(raw, width, &more, known, values, true);
    if second.reading.value.is_some() || (first.reading.digits.is_empty() && !second.reading.digits.is_empty()) {
        second
    } else {
        first
    }
}

/// Reads the candidate boxes in their order: the first with a value, else the first with a row of digits.
fn look(
    raw: &[u8],
    width: i32,
    boxes: &[(i32, i32, i32, i32)],
    known: &ByVariant<Templates>,
    values: &[u64],
    ground: bool,
) -> Found {
    let mut first_with_digits: Option<Found> = None;
    for &boxr in boxes {
        let images = ByVariant {
            green: Some(gray_crop(raw, width, boxr, Variant::Green)),
            bright: Some(gray_crop(raw, width, boxr, Variant::Bright)),
        };
        let mut found = read_variants_in(&images, known, values, ground);
        found.boxr = Some(boxr);
        if found.reading.value.is_some() {
            return found;
        }
        if !found.reading.digits.is_empty() && first_with_digits.is_none() {
            first_with_digits = Some(found);
        }
    }
    first_with_digits.unwrap_or_else(Found::silent)
}

// ------------------------------------------------------------ teaching

/// What teaching did, for the player.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Digits found in the image.
    pub found: usize,
    /// Newly stored.
    pub new: usize,
    /// Would have been read right before (best template is the typed digit, near enough).
    pub known: usize,
    /// Places (1-based) that did not look like their digit and were NOT stored.
    pub unclear: Vec<usize>,
}

/// Teaches the crop as the typed number; `Err` carries a key word. A count of digits that does not fit the
/// typed number is not guessed: a wrongly assigned template poisons every later reading.
pub fn learn(
    raster: &Gray,
    typed: &str,
    known: &Templates,
    own_file: &str,
    sample_folder: &str,
) -> Result<Tally, &'static str> {
    learn_into(raster, typed, known, own_file, Some(sample_folder))
}

/// Teaches the same place from each grey image, each into its own file; the image of the number is kept once.
///
/// Returns the outcome of the grey image `first` (else the first present) if it worked there, else of the first
/// where it worked, else of the first tried. `known` holds the templates of each grey image as they were before.
pub fn learn_variants(
    images: &Images,
    typed: &str,
    first: Option<Variant>,
    known: &ByVariant<Templates>,
    files: &OwnFiles,
    sample_folder: &str,
) -> Result<Tally, &'static str> {
    let mut order: Vec<(Variant, &Gray)> =
        VARIANTS.into_iter().filter_map(|v| images.get(v).as_ref().map(|raster| (v, raster))).collect();
    if let Some(at) = order.iter().position(|(v, _)| Some(*v) == first) {
        let front = order.remove(at);
        order.insert(0, front);
    }
    if order.is_empty() {
        return Err("anlernen_anzahl");
    }
    let outcomes: Vec<Result<Tally, &'static str>> =
        order.iter().map(|&(v, raster)| learn_into(raster, typed, known.get(v), files.of(v), None)).collect();
    let kept = outcomes.iter().position(Result::is_ok);
    if let Some(at) = kept {
        let digits: String = typed.chars().filter(char::is_ascii_digit).collect();
        save_sample(sample_folder, order[at].1, &digits);
    }
    outcomes.into_iter().nth(kept.unwrap_or(0)).unwrap_or(Err("anlernen_anzahl"))
}

/// [`learn`] without keeping the image when `sample_folder` is `None`.
fn learn_into(
    raster: &Gray,
    typed: &str,
    known: &Templates,
    own_file: &str,
    sample_folder: Option<&str>,
) -> Result<Tally, &'static str> {
    let typed_digits: Vec<char> = typed.chars().filter(|c| c.is_ascii_digit()).collect();
    if typed_digits.len() < 2 {
        return Err("anlernen_leer");
    }
    let mut candidates: Vec<(usize, Vec<Pattern>, Vec<Box4>, u8)> = Vec::new();
    for variants in [false, true] {
        if !candidates.is_empty() {
            break;
        }
        for threshold in thresholds(raster) {
            for digits in digit_rows(&components(raster, threshold), raster, threshold, variants) {
                if digits.len() == typed_digits.len() {
                    let patterns: Vec<Pattern> = digits.iter().map(|&b| normalize(raster, b, threshold)).collect();
                    let fitting =
                        patterns.iter().zip(&typed_digits).filter(|(p, d)| plausible_template(**d, p)).count();
                    candidates.push((fitting, patterns, digits, threshold));
                }
            }
        }
    }
    if candidates.is_empty() {
        return Err("anlernen_anzahl");
    }
    // Stable: the first of the equally fitting stays first.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.0));
    let (_, patterns, boxes, used_threshold) = candidates.swap_remove(0);
    // The same boxes cut at the nearest other thresholds: brighter or darker type of a second grab.
    let other_thresholds: Vec<u8> = {
        let mut others: Vec<u8> = thresholds(raster).into_iter().filter(|t| *t != used_threshold).collect();
        others.sort_by_key(|t| (i32::from(*t) - i32::from(used_threshold)).abs());
        others.truncate(DERIVED_THRESHOLDS);
        others
    };

    // Only a file that is not there starts empty: a file that cannot be read (locked, not JSON) would be
    // replaced by the one set of this teaching and lose everything taught before.
    let (mut own, mut derived_own) = match jsonio::read_json(own_file) {
        Ok(Value::Object(map)) => {
            let field = |name: &str| {
                map.iter()
                    .find(|(k, _)| k == name)
                    .and_then(|(_, v)| if let Value::Object(m) = v { Some(m.clone()) } else { None })
                    .unwrap_or_default()
            };
            (field("ziffern"), field("abgeleitet"))
        }
        Err(jsonio::ReadError::NotFound) => (Vec::new(), Vec::new()),
        Ok(_) | Err(_) => return Err("anlernen_lesen"),
    };
    let columns = digit_table(&patterns, known);
    let mut tally = Tally { found: patterns.len(), ..Tally::default() };
    for (position, (pattern, &digit)) in patterns.iter().zip(&typed_digits).enumerate() {
        let position = position + 1;
        if !plausible_template(digit, pattern) {
            tally.unclear.push(position);
            continue;
        }
        // ⚠ `known` means the digit would have been read right BEFORE teaching. Bit equality does not do:
        // with small type the same digit looks different in every grab.
        let column = &columns[position - 1];
        let mut best: Option<(char, f64)> = None;
        for &(d, x) in column {
            if best.is_none_or(|(_, b)| x < b) {
                best = Some((d, x));
            }
        }
        if best.is_some_and(|(d, _)| d == digit) && column_get(column, digit).is_some_and(|x| x <= MAX_DIGIT_DISTANCE) {
            tally.known += 1;
        } else {
            tally.new += 1;
        }
        let key = digit.to_string();
        let slot = match own.iter().position(|(k, _)| *k == key) {
            Some(at) => at,
            None => {
                own.push((key.clone(), Value::Array(Vec::new())));
                own.len() - 1
            }
        };
        let list = match &mut own[slot].1 {
            Value::Array(list) => list,
            other => {
                *other = Value::Array(Vec::new());
                let Value::Array(list) = other else { unreachable!() };
                list
            }
        };
        let value = Value::Array(pattern.iter().map(|&v| Value::int(i64::from(v))).collect());
        if !list.contains(&value) {
            list.push(value);
            let excess = list.len().saturating_sub(MAX_OWN_PER_DIGIT);
            list.drain(..excess);
        }
        let mut variants: Vec<Pattern> =
            other_thresholds.iter().map(|&t| normalize(raster, boxes[position - 1], t)).collect();
        variants.extend(derived_patterns(digit, pattern));
        for variant in keep_new(digit, &fill(pattern), variants) {
            let slot = match derived_own.iter().position(|(k, _)| *k == key) {
                Some(at) => at,
                None => {
                    derived_own.push((key.clone(), Value::Array(Vec::new())));
                    derived_own.len() - 1
                }
            };
            let Value::Array(list) = &mut derived_own[slot].1 else { continue };
            let value = Value::Array(variant.iter().map(|&v| Value::int(i64::from(v))).collect());
            if !list.contains(&value) {
                list.push(value);
                let excess = list.len().saturating_sub(MAX_DERIVED_PER_DIGIT);
                list.drain(..excess);
            }
        }
    }
    let file = Value::Object(vec![
        ("format".into(), Value::int(1)),
        ("raster".into(), Value::Array(vec![Value::int(NORM_W as i64), Value::int(NORM_H as i64)])),
        ("ziffern".into(), Value::Object(own)),
        ("abgeleitet".into(), Value::Object(derived_own)),
    ]);
    if let Some(folder) = std::path::Path::new(own_file).parent() {
        let _ = jsonio::make_dirs(&folder.to_string_lossy());
    }
    jsonio::write_text_atomic(own_file, &jsonio::dumps(&file, None, false), Newline::Keep)
        .map_err(|_| "anlernen_speichern")?;
    if let Some(folder) = sample_folder {
        save_sample(folder, raster, &typed_digits.iter().collect::<String>());
    }
    Ok(tally)
}

// ------------------------------------------------------------ PNG and sample images

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// A grey image as PNG.
pub fn png_bytes(raster: &Gray) -> Vec<u8> {
    let mut raw = Vec::with_capacity((raster.w + 1) * raster.h);
    for y in 0..raster.h {
        raw.push(0);
        raw.extend_from_slice(raster.row(y));
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut block = kind.to_vec();
        block.extend_from_slice(body);
        out.extend_from_slice(&block);
        out.extend_from_slice(&crc32(&block).to_be_bytes());
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&(raster.w as u32).to_be_bytes());
    header.extend_from_slice(&(raster.h as u32).to_be_bytes());
    header.extend_from_slice(&[8, 0, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// The files of the taught images, oldest first.
pub fn samples(folder: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
    let mut files: Vec<(std::time::SystemTime, String)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let time = e.metadata().ok()?.modified().ok()?;
            name.ends_with(".png").then_some((time, name))
        })
        .collect();
    files.sort();
    files.into_iter().map(|(_, n)| n).collect()
}

/// Stores the taught image with the right number: `<number>_<time>.png`, at most `SAMPLE_LIMIT`, the oldest go
/// first. Only the crop around the number, nothing of the rest of the screen.
pub fn save_sample(folder: &str, raster: &Gray, number: &str) -> bool {
    if jsonio::make_dirs(folder).is_err() {
        return false;
    }
    let millis =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_millis()).unwrap_or(0);
    let name = format!("{number}_{}-{millis:03}.png", jsonio::time_stamp());
    let path = format!("{}/{name}", folder.trim_end_matches(['/', '\\']));
    if jsonio::write_new(&path, &png_bytes(raster)).is_err() {
        return false;
    }
    let files = samples(folder);
    let excess = files.len().saturating_sub(SAMPLE_LIMIT);
    for old in &files[..excess] {
        let _ = jsonio::remove_file(&format!("{}/{old}", folder.trim_end_matches(['/', '\\'])));
    }
    true
}

/// The name of the reporter as a folder and file name; empty stays empty.
pub fn reporter_folder(name: &str) -> String {
    let clean: String = name.chars().map(|c| if c.is_alphanumeric() || "-_. ".contains(c) { c } else { '_' }).collect();
    clean.trim_matches([' ', '.', '_']).chars().take(40).collect()
}

/// How the archive of the images is named, with the reporter in it when there is one.
pub fn archive_name(name: &str) -> String {
    let who = reporter_folder(name);
    if who.is_empty() { "scan-bilder.zip".to_string() } else { format!("scan-bilder-{who}.zip") }
}

/// The taught images, the kept pictures of the refinery scanner (`refinery_folder`, see
/// `refinery_scan::screen::keep`) and the own templates (the shared file and one for each grey image) as ZIP
/// (bytes), or `None` when there are no images.
pub fn sample_archive(name: &str, sample_folder: &str, files: &OwnFiles, refinery_folder: &str) -> Option<Vec<u8>> {
    let names = samples(sample_folder);
    let refinery = crate::refinery_scan::screen::kept_in(refinery_folder);
    if names.is_empty() && refinery.is_empty() {
        return None;
    }
    let who = reporter_folder(name);
    let root = if who.is_empty() { String::new() } else { format!("{who}/") };
    let mut builder = crate::zip::Builder::new(jsonio::local_time());
    for (folder, list, inside) in [(sample_folder, &names, "bilder"), (refinery_folder, &refinery, "raffinerie")] {
        for file in list {
            let bytes = fs::read(format!("{}/{file}", folder.trim_end_matches(['/', '\\']))).ok()?;
            builder.add(&format!("{root}{inside}/{file}"), &bytes).ok()?;
        }
    }
    for (name, path) in files.all() {
        if let Ok(bytes) = fs::read(path) {
            builder.add(&format!("{root}{name}"), &bytes).ok()?;
        }
    }
    builder.finish().ok()
}

#[cfg(test)]
mod tests;
