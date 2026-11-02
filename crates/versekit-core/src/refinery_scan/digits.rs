// Verse-Kit — companion for Star Citizen.
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

//! Learned pictures of the digits of the terminal's font (`refinery_scan.py`, part „Ziffern lernen“): per digit
//! a list of patterns. With them unsure cells are read digit by digit ([`read_cell`]) and 0 and 8 are told apart
//! ([`super::CONFUSABLE`]). Kept in `raffinerie-ziffern.json` as `{"format": 1, "ziffern": {"0": [[…], …], …}}`.
//!
//! ⚠ The vectors are compared with the patterns Python wrote: every step of [`glyphs`] (truncating to whole
//! pixels, the mean, the length, the rounding to four places) follows the Python code exactly, else a learned
//! pattern of 4.12 would lie a little off.

use crate::jsonio::{self, Value};
use crate::paths::Paths;
use crate::pyfloat;
use crate::text::py_sum;

use super::{CONFUSABLE, Int, number};

pub const DIGIT_FILE: &str = "raffinerie-ziffern.json";
/// Size of a pattern in pixels.
pub const DIGIT_W: usize = 10;
pub const DIGIT_H: usize = 14;
/// A cell is enlarged to this height before it is cut into digits.
pub const CELL_HEIGHT: f64 = 42.0;
/// At most so many patterns per digit; the oldest fall out.
pub const DIGIT_KEEP: usize = 40;
/// Least number of patterns for 0 and for 8 from which [`classify`] assigns.
pub const DIGIT_MIN: usize = 2;
/// The other digit must lie this factor farther away than the one chosen.
pub const DIGIT_MARGIN: f64 = 1.3;
/// A digit may not lie farther than this from its nearest pattern: else it is one not learned yet.
pub const DIGIT_MAX_DISTANCE: f64 = 0.2;
/// Factor of distance between 0 and 8 in the assignment over all digits.
pub const CONFUSABLE_MARGIN: f64 = 2.0;

/// A grey picture: width, height and one byte per pixel, row by row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub gray: Vec<u8>,
}

/// One learned pattern: the numbers to compare with and the value as it stands in the file.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub values: Vec<f64>,
    raw: Value,
}

impl Pattern {
    fn new(values: Vec<f64>) -> Pattern {
        let raw = Value::Array(values.iter().map(|v| Value::Float(*v)).collect());
        Pattern { values, raw }
    }
}

/// The learned patterns per digit, in the order of the file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Digits {
    by_digit: Vec<(String, Vec<Pattern>)>,
}

/// A number as Python takes one in a sum: `true` is 1.
fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        Value::Int(text) => text.parse().ok(),
        Value::Float(x) => Some(*x),
        _ => None,
    }
}

impl Digits {
    /// `load_digits` on the content of the file: every entry of `ziffern` that is a list. A pattern that is not a
    /// list of numbers is left out.
    pub fn from_value(data: &Value) -> Digits {
        let Some(Value::Object(entries)) = data.get("ziffern") else { return Digits::default() };
        let mut by_digit = Vec::new();
        for (digit, value) in entries {
            let Value::Array(patterns) = value else { continue };
            let patterns = patterns
                .iter()
                .filter_map(|p| match p {
                    Value::Array(items) => {
                        let values: Option<Vec<f64>> = items.iter().map(as_number).collect();
                        values.map(|values| Pattern { values, raw: p.clone() })
                    }
                    _ => None,
                })
                .collect();
            by_digit.push((digit.clone(), patterns));
        }
        Digits { by_digit }
    }

    /// The content of the file (`save_digits`).
    pub fn to_value(&self) -> Value {
        let digits = self
            .by_digit
            .iter()
            .map(|(digit, patterns)| (digit.clone(), Value::Array(patterns.iter().map(|p| p.raw.clone()).collect())))
            .collect();
        Value::Object(vec![("format".into(), Value::int(1)), ("ziffern".into(), Value::Object(digits))])
    }

    /// The learned patterns of the player, or none.
    pub fn load(paths: &Paths) -> Digits {
        match jsonio::read_json(&paths.app_file(DIGIT_FILE)) {
            Ok(data) => Digits::from_value(&data),
            Err(_) => Digits::default(),
        }
    }

    /// `save_digits`: written whole or not at all.
    pub fn save(&self, paths: &Paths) -> bool {
        let text = jsonio::dumps(&self.to_value(), None, false);
        match jsonio::write_text_atomic(&paths.app_file(DIGIT_FILE), &text, jsonio::Newline::Native) {
            Ok(()) => true,
            Err(e) => {
                crate::errors::record("refinery_scan.save_digits", &e);
                false
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.by_digit.is_empty()
    }

    /// The patterns of one digit.
    pub fn get(&self, digit: &str) -> &[Pattern] {
        self.by_digit.iter().find(|(d, _)| d == digit).map_or(&[], |(_, p)| p.as_slice())
    }

    /// One more pattern for `digit`; afterwards only the newest [`DIGIT_KEEP`] stay.
    pub fn add(&mut self, digit: &str, vector: Vec<f64>) {
        let index = match self.by_digit.iter().position(|(d, _)| d == digit) {
            Some(index) => index,
            None => {
                self.by_digit.push((digit.to_string(), Vec::new()));
                self.by_digit.len() - 1
            }
        };
        let patterns = &mut self.by_digit[index].1;
        patterns.push(Pattern::new(vector));
        if patterns.len() > DIGIT_KEEP {
            let surplus = patterns.len() - DIGIT_KEEP;
            patterns.drain(..surplus);
        }
    }
}

/// A horizontal stretch of a digit: left and right edge in pixels.
type Span = (f64, f64);

/// `_digit_spans`: the horizontal stretches of the `count` digits. `cols` are the columns with writing.
/// Connected runs are the digits; without `count` they are given back as they are. Are there more than `count`,
/// the runs are joined over the smallest gaps; are there fewer, the stretch of writing is cut into `count` equal
/// parts.
fn digit_spans(cols: &[usize], count: Option<usize>) -> Vec<Span> {
    let mut spans: Vec<(usize, usize)> = vec![(cols[0], cols[0] + 1)];
    for &x in &cols[1..] {
        match spans.last_mut() {
            Some(last) if x == last.1 => last.1 = x + 1,
            _ => spans.push((x, x + 1)),
        }
    }
    let Some(count) = count else { return spans.iter().map(|(a, b)| (*a as f64, *b as f64)).collect() };
    while spans.len() > count {
        // The smallest gap, on a tie the first (Python's `min` over (gap, index)).
        let i = (0..spans.len() - 1).min_by_key(|&i| (spans[i + 1].0 - spans[i].1, i)).unwrap_or(0);
        spans[i] = (spans[i].0, spans[i + 1].1);
        spans.remove(i + 1);
    }
    if spans.len() == count {
        return spans.iter().map(|(a, b)| (*a as f64, *b as f64)).collect();
    }
    let x0 = cols[0] as f64;
    let x1 = (cols[cols.len() - 1] + 1) as f64;
    let step = (x1 - x0) / count as f64;
    (0..count).map(|i| (x0 + step * i as f64, x0 + step * (i + 1) as f64)).collect()
}

/// `glyphs`: the digits of a cell as patterns. Each stretch of a digit is brought to [`DIGIT_W`] × [`DIGIT_H`]
/// and set to mean 0 and length 1. `count` cuts into so many digits; without, the runs of writing between the
/// gaps count.
pub fn glyphs(crop: Option<&Gray>, count: Option<usize>) -> Vec<Vec<f64>> {
    let Some(crop) = crop else { return Vec::new() };
    if count == Some(0) || crop.gray.is_empty() {
        return Vec::new();
    }
    let (width, height, gray) = (crop.width, crop.height, &crop.gray);
    let low = *gray.iter().min().unwrap_or(&0);
    let high = *gray.iter().max().unwrap_or(&0);
    if i32::from(high) - i32::from(low) < 30 {
        return Vec::new();
    }
    let cut = (f64::from(low) + f64::from(high)) / 2.0;
    let bright = |x: usize, y: usize| f64::from(gray[y * width + x]) > cut;
    let cols: Vec<usize> = (0..width).filter(|&x| (0..height).any(|y| bright(x, y))).collect();
    let rows: Vec<usize> = (0..height).filter(|&y| (0..width).any(|x| bright(x, y))).collect();
    if cols.is_empty() || rows.is_empty() {
        return Vec::new();
    }
    let (y0, y1) = (rows[0] as f64, (rows[rows.len() - 1] + 1) as f64);
    let mut out = Vec::new();
    for (left, right) in digit_spans(&cols, count) {
        let step = right - left;
        let mut vector: Vec<f64> = Vec::with_capacity(DIGIT_W * DIGIT_H);
        for gy in 0..DIGIT_H {
            let sy = ((y0 + (gy as f64 + 0.5) * (y1 - y0) / DIGIT_H as f64) as usize).min(height - 1);
            for gx in 0..DIGIT_W {
                let sx = ((left + (gx as f64 + 0.5) * step / DIGIT_W as f64) as usize).min(width - 1);
                vector.push(f64::from(gray[sy * width + sx]));
            }
        }
        let mean = vector.iter().sum::<f64>() / vector.len() as f64;
        let centred: Vec<f64> = vector.iter().map(|v| v - mean).collect();
        let length = py_sum(centred.iter().map(|v| v * v)).powf(0.5);
        let norm = if length == 0.0 { 1.0 } else { length };
        out.push(centred.iter().map(|v| pyfloat::round(v / norm, 4)).collect());
    }
    out
}

/// `_distance`: one less the dot product, over the shorter of the two.
pub fn distance(a: &[f64], b: &[f64]) -> f64 {
    1.0 - py_sum(a.iter().zip(b).map(|(p, q)| p * q))
}

fn nearest(vector: &[f64], patterns: &[Pattern]) -> f64 {
    patterns.iter().map(|p| distance(vector, &p.values)).fold(f64::INFINITY, f64::min)
}

/// `classify`: which of 0 and 8 this picture of a digit is, or `None`. Only when each has at least
/// [`DIGIT_MIN`] patterns and the best lies [`CONFUSABLE_MARGIN`] times nearer than the second.
pub fn classify(vector: &[f64], digits: &Digits) -> Option<char> {
    let mut scores: Vec<(f64, char)> = Vec::new();
    for digit in CONFUSABLE {
        let patterns = digits.get(&digit.to_string());
        if patterns.len() < DIGIT_MIN {
            return None;
        }
        scores.push((nearest(vector, patterns), digit));
    }
    scores.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    if scores.len() > 1 && scores[0].0 * CONFUSABLE_MARGIN > scores[1].0 {
        return None;
    }
    scores.first().map(|s| s.1)
}

/// `classify_any`: which digit this picture is, among all with at least [`DIGIT_MIN`] patterns. `None` when the
/// nearest lies farther than [`DIGIT_MAX_DISTANCE`] or the second is not [`DIGIT_MARGIN`] times farther; for 0
/// and 8 the other of the two must lie [`CONFUSABLE_MARGIN`] times farther.
pub fn classify_any<'a>(vector: &[f64], digits: &'a Digits) -> Option<&'a str> {
    let mut scores: Vec<(f64, &str)> = digits
        .by_digit
        .iter()
        .filter(|(_, patterns)| patterns.len() >= DIGIT_MIN)
        .map(|(digit, patterns)| (nearest(vector, patterns), digit.as_str()))
        .collect();
    scores.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(b.1)));
    let (best_score, best) = *scores.first()?;
    if best_score > DIGIT_MAX_DISTANCE {
        return None;
    }
    if scores.len() > 1 && best_score * DIGIT_MARGIN > scores[1].0 {
        return None;
    }
    let confusable = |d: &str| d.chars().count() == 1 && d.chars().all(|c| CONFUSABLE.contains(&c));
    if confusable(best) {
        let other = scores.iter().find(|(_, d)| confusable(d) && *d != best).map(|(s, _)| *s);
        match other {
            Some(other) if best_score * CONFUSABLE_MARGIN <= other => {}
            _ => return None,
        }
    }
    Some(best)
}

/// `int(''.join(digits))`, or `None` when the joined text is no number.
fn joined(parts: &[&str]) -> Option<Int> {
    let text: String = parts.concat();
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// `read_cell`: a cell of numbers read from the learned patterns alone, or `None`. The digits are the runs
/// between the gaps; each must be assigned clearly by [`classify_any`]. Fails that and has the word `read` another
/// number of places (two digits touch), the cell is cut after that number and read once more.
pub fn read_cell(crop: &Gray, digits: &Digits, read: &str) -> Option<Int> {
    let mut attempts = vec![glyphs(Some(crop), None)];
    if let Some(value) = number(read, true) {
        let places = value.to_string().len();
        if places != attempts[0].len() {
            attempts.push(glyphs(Some(crop), Some(places)));
        }
    }
    for vectors in &attempts {
        if !(1..=5).contains(&vectors.len()) {
            continue;
        }
        let out: Option<Vec<&str>> = vectors.iter().map(|v| classify_any(v, digits)).collect();
        if let Some(out) = out {
            return joined(&out);
        }
    }
    None
}

/// `_loose`: the row of digits with 0 and 8 taken as alike, for assigning.
fn loose(text: &str) -> Option<String> {
    number(text, true).map(|value| value.to_string().replace('8', "0"))
}

/// A cell of an unsure row, kept for learning (`unsure_cells`): the material, the column (0 quality, 1 amount),
/// the word read and the grey picture of the cell.
#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub material: String,
    pub column: usize,
    pub read: String,
    pub crop: Gray,
}

/// A row typed by the player: material, quality, amount in cSCU.
pub type Typed = (String, Option<f64>, Option<f64>);

/// `learn` without the disk: from values typed after, into `digits`. Gives the number of digits learned. A cell
/// belongs to the typed value of the same material that has, with 0 and 8 alike, the same digits as the word read;
/// is there only one row of that material, the same number of places is enough. A cell without a readable word
/// teaches nothing.
pub fn learn_into(digits: &mut Digits, cells: &[Cell], typed: &[Typed]) -> usize {
    let mut learned = 0;
    for cell in cells {
        let values: Vec<String> = typed
            .iter()
            .filter(|row| row.0 == cell.material)
            .filter_map(|row| if cell.column == 0 { row.1 } else { row.2 })
            .map(|v| (v.trunc() as Int).to_string())
            .collect();
        let Some(read) = loose(&cell.read) else { continue };
        let mut found: Vec<&String> = values.iter().filter(|v| v.replace('8', "0") == read).collect();
        if found.is_empty() && values.len() == 1 && values[0].len() == read.len() {
            found = values.iter().collect();
        }
        if found.len() != 1 {
            continue;
        }
        let value = found[0];
        let vectors = glyphs(Some(&cell.crop), Some(value.len()));
        if vectors.len() != value.len() {
            continue;
        }
        for (digit, vector) in value.chars().zip(vectors) {
            digits.add(&digit.to_string(), vector);
            learned += 1;
        }
    }
    learned
}

/// `learn`: learn from typed values and keep the patterns. Gives the number of digits learned.
pub fn learn(paths: &Paths, cells: &[Cell], typed: &[Typed]) -> usize {
    if cells.is_empty() || typed.is_empty() {
        return 0;
    }
    let mut digits = Digits::load(paths);
    let learned = learn_into(&mut digits, cells, typed);
    if learned > 0 {
        digits.save(paths);
    }
    learned
}
