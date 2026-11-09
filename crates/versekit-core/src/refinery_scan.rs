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

//! The refinery terminal read from the screen, port of `refinery_scan.py`: the part that works on the words the
//! text recognition gives (text and frame in pixels of the picture), without a screen and without the system's
//! recognition, which come in through [`Ocr`].
//!
//! First the whole picture is searched in enlarged tiles for the head of the yield table ([`search_jobs`],
//! [`find_tables`]): the terminal can stand anywhere on a wide screen. Then only that part is read several times,
//! the rows of the table are put together and made into lines in the form of the refinery field
//! (`Material Q Menge cSCU`). Nothing is written into the stock here: the result is text the page puts into the
//! refinery field; the button there enters it.
//!
//! ⚠ The reading works on words with their frames, not on the lines of the recognition: the columns of the
//! terminal stand far apart, and the recognition often cuts one row of the table into several lines. [`rows`]
//! puts every word of overlapping height back into one row.
//!
//! ⚠ The small digits of the terminal are read unreliably: digits fall away or are mistaken. So several readings
//! at different enlargements ([`PASSES`]), and a value only counts when at least two readings read it alike
//! ([`merge_passes`]). Unsure values stay empty and are named as unsure.

pub mod digits;
pub mod screen;

use std::collections::{HashMap, HashSet};

use crate::stock_data::{norm_material, ratio};
use crate::text::{py_isspace, py_strip, py_sum};

/// One word of the text recognition: its text and its frame in pixels of the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub t: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Word {
    pub fn new(t: &str, x: f64, y: f64, w: f64, h: f64) -> Word {
        Word { t: t.to_string(), x, y, w, h }
    }

    fn middle_x(&self) -> f64 {
        self.x + self.w / 2.0
    }

    fn middle_y(&self) -> f64 {
        self.y + self.h / 2.0
    }
}

/// An integer as Python keeps it, large enough for any row of digits the recognition gives.
pub type Int = i128;

/// The materials the reading knows: the names of the storable materials by their comparison form
/// (`norm_material`), the first name of a form wins, in the order given; and which of them count in pieces.
/// Beside them the storage locations, for the name of the station at the top of the terminal ([`station`]).
#[derive(Clone, Debug, Default)]
pub struct Index {
    by_form: Vec<(String, String)>,
    lookup: HashMap<String, usize>,
    pieces: HashSet<String>,
    places: Vec<String>,
}

impl Index {
    /// `storable` in their order (`crafting.storable`), `is_piece` for the names that count in pieces.
    pub fn new(storable: &[String], is_piece: impl Fn(&str) -> bool) -> Index {
        let mut index = Index::default();
        for name in storable {
            let form = norm_material(name);
            if !index.lookup.contains_key(&form) {
                index.lookup.insert(form.clone(), index.by_form.len());
                index.by_form.push((form, name.clone()));
            }
            if is_piece(name) {
                index.pieces.insert(name.clone());
            }
        }
        index
    }

    /// The same index knowing the storage locations `places`; without them no station is read.
    pub fn with_places(mut self, places: &[String]) -> Index {
        self.places = places.to_vec();
        self
    }

    /// The index of the stock data of the player, with its storage locations.
    pub fn of(data: &crate::stock_data::StockData) -> Index {
        Index::new(data.storable(), |name| data.is_piece(name)).with_places(data.places())
    }

    fn get(&self, form: &str) -> Option<&str> {
        self.lookup.get(form).map(|&i| self.by_form[i].1.as_str())
    }

    fn counts_in_pieces(&self, name: &str) -> bool {
        self.pieces.contains(name)
    }
}

/// `rows`: the words in rows of the table, top to bottom, each left to right. Two words share a row when the
/// vertical middle of the one lies within the height of the row so far (one pixel of slack).
pub fn rows(words: &[Word]) -> Vec<Vec<Word>> {
    let mut ordered: Vec<&Word> = words.iter().filter(|w| !py_strip(&w.t).is_empty()).collect();
    ordered.sort_by(|a, b| a.middle_y().total_cmp(&b.middle_y()));
    struct Line<'a> {
        top: f64,
        bottom: f64,
        words: Vec<&'a Word>,
    }
    let mut lines: Vec<Line> = Vec::new();
    for word in ordered {
        let middle = word.middle_y();
        match lines.iter_mut().find(|line| line.top - 1.0 <= middle && middle <= line.bottom + 1.0) {
            Some(line) => {
                line.words.push(word);
                line.top = line.top.min(word.y);
                line.bottom = line.bottom.max(word.y + word.h);
            }
            None => lines.push(Line { top: word.y, bottom: word.y + word.h, words: vec![word] }),
        }
    }
    lines.sort_by(|a, b| a.top.total_cmp(&b.top));
    lines
        .into_iter()
        .map(|line| {
            let mut words: Vec<Word> = line.words.into_iter().cloned().collect();
            words.sort_by(|a, b| a.x.total_cmp(&b.x));
            words
        })
        .collect()
}

/// Characters the recognition mistakes for digits in fields of numbers. The terminal writes the zero with a slash,
/// which the recognition gives as `Ø`.
const LOOKALIKES: [(char, char); 16] = [
    ('O', '0'),
    ('o', '0'),
    ('D', '0'),
    ('Q', '0'),
    ('Ø', '0'),
    ('ø', '0'),
    ('I', '1'),
    ('l', '1'),
    ('|', '1'),
    ('i', '1'),
    ('!', '1'),
    ('S', '5'),
    ('s', '5'),
    ('B', '8'),
    ('Z', '2'),
    ('z', '2'),
];

fn lookalike(c: char) -> Option<char> {
    LOOKALIKES.iter().find(|(from, _)| *from == c).map(|(_, to)| *to)
}

fn translate_lookalikes(text: &str) -> String {
    text.chars().map(|c| lookalike(c).unwrap_or(c)).collect()
}

/// `re.sub(r'(?<=\d)[.,\s](?=\d{3}(?!\d))', '', text)`: a point, comma or space between a digit and exactly three
/// digits goes, decided on the text as it was.
fn drop_thousands(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let digit = |i: usize| chars.get(i).is_some_and(char::is_ascii_digit);
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        let separator = *c == '.' || *c == ',' || py_isspace(*c);
        if separator && i > 0 && digit(i - 1) && digit(i + 1) && digit(i + 2) && digit(i + 3) && !digit(i + 4) {
            continue;
        }
        out.push(*c);
    }
    out
}

/// `number`: an integer from a word of the recognition, or `None`. Mistaken characters (O/0, l/1, S/5) are
/// swapped only in words with at least one real digit; with `loose` also in words of at least two characters
/// that consist of mistakable characters only (`sos`). Thousands separators fall away. Beside a real digit an `e`
/// counts as 0, and a digit 0 or an `e` in front of further characters as 8 (`33e` is 330, `074` is 874; `O122`
/// stays 122).
pub fn number(token: &str, loose: bool) -> Option<Int> {
    let text = py_strip(token).trim_matches(|c| matches!(c, '.' | ',' | ':' | ';'));
    if text.is_empty() {
        return None;
    }
    let has_digit = text.chars().any(|c| c.is_ascii_digit());
    let all_lookalike = text.chars().count() >= 2 && text.chars().all(|c| lookalike(c).is_some());
    if !has_digit && !(loose && all_lookalike) {
        return None;
    }
    // No value of the terminal begins with 0: a digit 0 or an `e` in front of further characters is an 8 read as the
    // slashed zero. A letter in front (`O122`) stays what the lookalikes make of it.
    let eight_in_front = has_digit && text.chars().count() > 1 && matches!(text.chars().next(), Some('0' | 'e'));
    // Beside a real digit an `e` elsewhere is the slashed zero read as a letter.
    let text = if has_digit { text.replace('e', "0") } else { text.to_string() };
    let mut text = drop_thousands(&translate_lookalikes(&text));
    if text.is_empty() || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if eight_in_front {
        text.replace_range(..1, "8");
    }
    text.parse::<Int>().ok()
}

/// `match_material`: the material of a name the recognition read, or `None`. First exactly (by the comparison
/// form), then loosely: the best hit must be at least 0.8 alike and beat the second clearly.
pub fn match_material(text: &str, index: &Index) -> Option<String> {
    let kept: String = text
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || ('\u{C0}'..='\u{FF}').contains(c) || matches!(c, '(' | ')' | ' '))
        .collect();
    let form = norm_material(&kept);
    let form = py_strip(&form);
    let mut wanted = String::new();
    // The symbol in front of a material is read as a single letter (`e IRON`), and the set-up card writes raw
    // materials as `RAW OURATITE`: neither is part of the name.
    let mut parts = form.split(py_isspace).filter(|p| !p.is_empty()).peekable();
    while parts.peek().is_some_and(|first| first.chars().count() == 1) {
        parts.next();
    }
    if parts.peek().is_some_and(|first| first.eq_ignore_ascii_case("raw")) {
        parts.next();
    }
    for part in parts {
        if !wanted.is_empty() {
            wanted.push(' ');
        }
        wanted.push_str(part);
    }
    if wanted.chars().count() < 3 {
        return None;
    }
    if let Some(name) = index.get(&wanted) {
        return Some(name.to_string());
    }
    let mut scored: Vec<(f64, &str)> =
        index.by_form.iter().map(|(form, _)| (ratio(&wanted, form), form.as_str())).collect();
    // Python sorts the pairs (ratio, form) from the largest: on the same ratio the larger form first.
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| b.1.cmp(a.1)));
    if let Some(best) = scored.first().filter(|(score, _)| *score >= 0.8) {
        if scored.len() > 1 && best.0 - scored[1].0 < 0.08 {
            return None;
        }
        return index.get(best.1).map(str::to_string);
    }
    cut_short(&wanted, index)
}

/// The fewest letters a name cut short by the terminal must keep.
const CUT_MIN_LEN: usize = 4;

/// A name the terminal cut short (`HEPH (RAW)`): the one material whose comparison form begins with the first word,
/// when that word has at least [`CUT_MIN_LEN`] letters and every further word is short (the kind, `raw`, `ore`).
/// `None` when no material or more than one begins so.
fn cut_short(wanted: &str, index: &Index) -> Option<String> {
    let mut parts = wanted.split(' ');
    let first = parts.next()?;
    if first.chars().count() < CUT_MIN_LEN || !parts.all(|p| p.chars().count() < CUT_MIN_LEN + 1) {
        return None;
    }
    let mut hits = index.by_form.iter().filter(|(form, _)| form.starts_with(first));
    let (form, _) = hits.next()?;
    if hits.next().is_some() {
        return None;
    }
    index.get(form).map(str::to_string)
}

/// A row of the table that begins with a material (`material_rows`).
#[derive(Clone, Debug, PartialEq)]
pub struct MaterialRow {
    pub material: String,
    /// The vertical middle and the height of the row.
    pub y: f64,
    pub height: f64,
    /// The text as it was read.
    pub raw: String,
    /// The numbers right of the name as (horizontal middle, value).
    pub numbers: Vec<(f64, Int)>,
    /// The same of words without a real digit, of mistakable characters only (`sos`).
    pub loose_numbers: Vec<(f64, Int)>,
}

/// `material_rows`: the rows of the table that begin with a material.
pub fn material_rows(table: &[Vec<Word>], index: &Index) -> Vec<MaterialRow> {
    let mut out = Vec::new();
    for words in table {
        let words: Vec<&Word> = words.iter().filter(|w| !py_strip(&w.t).is_empty()).collect();
        let mut name_parts: Vec<&str> = Vec::new();
        for word in &words {
            // After the first word of the name a word of mistakable characters only (`sos`) ends the name too.
            if number(&word.t, !name_parts.is_empty()).is_some() {
                break;
            }
            name_parts.push(py_strip(&word.t));
        }
        if name_parts.is_empty() || name_parts.len() > 5 {
            continue;
        }
        let Some(material) = match_material(&name_parts.join(" "), index) else { continue };
        let (mut numbers, mut loose_numbers) = (Vec::new(), Vec::new());
        for word in &words[name_parts.len()..] {
            let middle = word.middle_x();
            if let Some(value) = number(&word.t, false) {
                numbers.push((middle, value));
            } else if let Some(value) = number(&word.t, true) {
                loose_numbers.push((middle, value));
            }
        }
        let top = words.iter().map(|w| w.y).fold(f64::INFINITY, f64::min);
        let bottom = words.iter().map(|w| w.y + w.h).fold(f64::NEG_INFINITY, f64::max);
        out.push(MaterialRow {
            material,
            y: (top + bottom) / 2.0,
            height: (bottom - top).max(1.0),
            raw: words.iter().map(|w| py_strip(&w.t)).collect::<Vec<_>>().join(" "),
            numbers,
            loose_numbers,
        });
    }
    out
}

/// `columns`: the columns of numbers as horizontal middles, left to right, without a head. Every number of every
/// row (also the loose ones), in order; a gap wider than three heights of a row begins a new column.
pub fn columns(found: &[MaterialRow]) -> Vec<f64> {
    let mut xs: Vec<f64> =
        found.iter().flat_map(|row| row.numbers.iter().chain(&row.loose_numbers).map(|(x, _)| *x)).collect();
    if xs.is_empty() {
        return Vec::new();
    }
    xs.sort_by(f64::total_cmp);
    let mut heights: Vec<f64> = found.iter().map(|row| row.height).collect();
    heights.sort_by(f64::total_cmp);
    let gap = 3.0 * heights[heights.len() / 2];
    let mut groups: Vec<Vec<f64>> = vec![vec![xs[0]]];
    for x in &xs[1..] {
        let last = groups.last().and_then(|g| g.last()).copied().unwrap_or(*x);
        if x - last > gap {
            groups.push(vec![*x]);
        } else if let Some(group) = groups.last_mut() {
            group.push(*x);
        }
    }
    groups.iter().map(|g| py_sum(g.iter().copied()) / g.len() as f64).collect()
}

/// Heads of the columns, German and English, in capitals; parts of words, so misread letters still hit.
/// The same parts as the search for the table takes ([`HEAD_QUALITY`]): a head misread as `OUAUTiT` still marks
/// the column, else the numbers of `ANZ.` would be taken for the yield.
const QUALITY_HEADS: [&str; 7] = HEAD_QUALITY;
/// The German head is cut to `AUSBEU` on the terminal; the parts of the English head as in [`HEAD_YIELD`].
const YIELD_HEADS: [&str; 9] = ["AUSBEU", "ERTRAG", "YIELD", "BEUTE", "USBEU", "YIEL", "YTEL", "YLEL", "YELD"];
/// Heads of the quantity put in while setting a job up (`QTY`, `ANZ.`), letters as [`head`] keeps them.
const QUANTITY_HEADS: [&str; 4] = ["QTY", "OTY", "ANZ", "MENGE"];

/// `header_columns`: the middles of the columns from the row of heads, or `None`: quality first, then the yield,
/// then every further head right of the quality (`ANZ.` between them while setting a job up, `ZU`, `FERTIG`,
/// `TO-DO` … right of the yield), so that their numbers slip neither into the quality nor into the yield. Only a
/// head right of the quality is the yield (`MATERIALS YIELDED` left of it is not); a row with the head of the
/// quantity but no head of the yield read takes the next head right of the quantity as the yield.
pub fn header_columns(table: &[Vec<Word>]) -> Option<Vec<f64>> {
    heads_of(table).map(|(heads, _)| heads)
}

/// [`header_columns`] and whether the head of the yield was read by name (`false`: taken right of the quantity).
fn heads_of(table: &[Vec<Word>]) -> Option<(Vec<f64>, bool)> {
    for words in table {
        let words = &split_heads(words);
        let (mut quality, mut yield_, mut quantity) = (None, None, None);
        for word in words {
            let text = word.t.to_uppercase();
            let middle = word.middle_x();
            if quality.is_none() && QUALITY_HEADS.iter().any(|h| text.contains(h)) {
                quality = Some(middle);
            } else if let Some(left) = quality
                && middle > left
            {
                if yield_.is_none() && head(word, &YIELD_HEADS) {
                    yield_ = Some(middle);
                } else if quantity.is_none() && head(word, &QUANTITY_HEADS) {
                    quantity = Some(middle);
                }
            }
        }
        let named = yield_.is_some();
        if let (None, Some(amount)) = (yield_, quantity) {
            yield_ = words.iter().map(Word::middle_x).filter(|middle| *middle > amount + 1.0).min_by(f64::total_cmp);
        }
        if let (Some(quality), Some(yield_)) = (quality, yield_) {
            let mut out = vec![quality, yield_];
            out.extend(
                words
                    .iter()
                    .map(Word::middle_x)
                    .filter(|middle| *middle > quality + 1.0 && (middle - yield_).abs() > 1.0),
            );
            return Some((out, named));
        }
    }
    None
}

/// The columns of a table: those of the heads ([`header_columns`], quality first, then the yield), and a column of
/// numbers right of the yield that stands far from every head as one more. A head the recognition missed in one
/// reading (`ZU` of a running job) then leaves no gap that its numbers would slip into the yield through. Without
/// heads the columns of the numbers ([`columns`]).
pub fn table_columns(table: &[Vec<Word>], found: &[MaterialRow]) -> Vec<f64> {
    table_columns_with(table, found, None)
}

/// [`table_columns`], with `shared` as the heads when the reading has none of its own (the heads another reading
/// of the same part read), or only found the yield right of the quantity. Without any heads and with three or more
/// columns of numbers only the quality stays: the second column is the quantity of a job being set up or the yield
/// of a running one, which the numbers alone do not tell.
fn table_columns_with(table: &[Vec<Word>], found: &[MaterialRow], shared: Option<&[f64]>) -> Vec<f64> {
    let own = match heads_of(table) {
        Some((heads, true)) => Some(heads),
        Some((heads, false)) => Some(shared.map_or(heads, <[f64]>::to_vec)),
        None => shared.map(<[f64]>::to_vec),
    };
    let Some(mut heads) = own else {
        let mut numbers = columns(found);
        if numbers.len() >= 3 {
            numbers.truncate(1);
        }
        return numbers;
    };
    let (quality, yield_) = (heads[0], heads[1]);
    let tolerance = 0.35 * (yield_ - quality);
    for x in columns(found) {
        if x > yield_ + tolerance && heads.iter().all(|head| (head - x).abs() > tolerance) {
            heads.push(x);
        }
    }
    heads
}

/// One row of a reading: the material with quality and amount (cSCU), each `None` when not readable.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedRow {
    pub material: String,
    pub quality: Option<Int>,
    pub amount: Option<Int>,
    pub loose_quality: Option<Int>,
    pub loose_amount: Option<Int>,
    pub y: f64,
    pub height: f64,
    pub raw: String,
}

/// `parse_rows`: the yield of one reading. Every number belongs to the nearest column (by the heads, else by the
/// numbers themselves); the first column is the quality (0 to 1000), the second the yield (above 0).
pub fn parse_rows(table: &[Vec<Word>], index: &Index) -> Vec<ParsedRow> {
    parse_rows_with(table, index, None)
}

/// [`parse_rows`] with the heads `shared` for a reading that has none of its own ([`table_columns_with`]).
fn parse_rows_with(table: &[Vec<Word>], index: &Index, shared: Option<&[f64]>) -> Vec<ParsedRow> {
    let found = material_rows(table, index);
    let centers = table_columns_with(table, &found, shared);
    let slots = |pairs: &[(f64, Int)]| -> (Option<Int>, Option<Int>) {
        if centers.is_empty() {
            return (None, None);
        }
        let mut slots: Vec<(usize, Int)> = Vec::new();
        for (x, value) in pairs {
            let nearest = nearest(&centers, *x);
            if !slots.iter().any(|(slot, _)| *slot == nearest) {
                slots.push((nearest, *value));
            }
        }
        let get = |slot: usize| slots.iter().find(|(s, _)| *s == slot).map(|(_, v)| *v);
        let quality = get(0).filter(|q| (0..=1000).contains(q));
        let amount = get(1).filter(|a| *a > 0);
        (quality, amount)
    };
    found
        .into_iter()
        .map(|row| {
            let (quality, amount) = slots(&row.numbers);
            let (loose_quality, loose_amount) = slots(&row.loose_numbers);
            ParsedRow {
                material: row.material,
                quality,
                amount,
                loose_quality,
                loose_amount,
                y: row.y,
                height: row.height,
                raw: row.raw,
            }
        })
        .collect()
}

/// The index of the center nearest to `x`; on a tie the first (Python's `min` over the range).
fn nearest(centers: &[f64], x: f64) -> usize {
    let mut best = 0;
    for (i, center) in centers.iter().enumerate().skip(1) {
        if (center - x).abs() < (centers[best] - x).abs() {
            best = i;
        }
    }
    best
}

/// So many readings must have read a value alike for it to count.
pub const MIN_VOTES: usize = 2;
/// A second value read at least this often makes the cell disputed: with 1 every reading that gave a number at all
/// must give the same.
pub const RIVAL_VOTES: usize = 1;

/// `_vote`: the value most readings read, or `None`: when it comes fewer than `minimum` times, a second value
/// came as often, or a second value came at least `rival` times.
fn vote<T: PartialEq + Clone>(values: &[Option<T>], minimum: usize, rival: Option<usize>) -> Option<T> {
    let mut counts: Vec<(T, usize)> = Vec::new();
    for value in values.iter().flatten() {
        match counts.iter_mut().find(|(known, _)| known == value) {
            Some((_, n)) => *n += 1,
            None => counts.push((value.clone(), 1)),
        }
    }
    if counts.is_empty() {
        return None;
    }
    // Stable: on the same count the one read first.
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    if counts[0].1 < minimum {
        return None;
    }
    if counts.len() > 1 && counts[0].1 == counts[1].1 {
        return None;
    }
    if let Some(rival) = rival.filter(|r| *r > 0)
        && counts.len() > 1
        && counts[1].1 >= rival
    {
        return None;
    }
    Some(counts[0].0.clone())
}

/// Digits the recognition mistakes for each other in the font of the terminal: the zero has a slash and is read
/// as 8 by every reading alike (and the other way round). A value with one of them counts only when a second,
/// independent check ([`Checks::confirm`]) confirms it.
pub const CONFUSABLE: [char; 2] = ['0', '8'];

/// `needs_confirm`: whether the number holds a digit of [`CONFUSABLE`].
pub fn needs_confirm(value: Option<Int>) -> bool {
    value.is_some_and(|v| v.to_string().chars().any(|c| CONFUSABLE.contains(&c)))
}

/// `_guess`: the value read most often; on a tie the one read first. `None` when no reading gave a number.
fn guess(values: &[Option<Int>]) -> Option<Int> {
    let mut counts: Vec<(Int, usize)> = Vec::new();
    for value in values.iter().flatten() {
        match counts.iter_mut().find(|(known, _)| known == value) {
            Some((_, n)) => *n += 1,
            None => counts.push((*value, 1)),
        }
    }
    let most = counts.iter().map(|(_, n)| *n).max()?;
    counts.into_iter().find(|(_, n)| *n == most).map(|(v, _)| v)
}

/// What a check of a value with a confusable digit says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checked {
    /// The value read is right.
    Yes,
    /// The value is this one (an 8 that is a zero).
    Value(Int),
    /// Not to be told.
    No,
}

/// The checks of [`merge_passes`] that look at the picture: `confirm(row, column, value)` decides a value with a
/// confusable digit (column 0 the quality, 1 the amount); `reread(row, column)` reads a missing cell anew.
#[derive(Default)]
pub struct Checks<'a> {
    pub confirm: Option<Confirm<'a>>,
    pub reread: Option<Reread<'a>>,
}

/// `confirm(row, column, value)` of [`Checks`].
pub type Confirm<'a> = Box<dyn Fn(f64, usize, Int) -> Checked + 'a>;
/// `reread(row, column)` of [`Checks`].
pub type Reread<'a> = Box<dyn Fn(f64, usize) -> Option<Int> + 'a>;

/// A row that was not read for sure, with its vertical middle (for teaching).
#[derive(Clone, Debug, PartialEq)]
pub struct Unsure {
    pub material: String,
    pub y: f64,
}

/// What [`merge_passes`] gives.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Merged {
    /// (material, quality, amount in cSCU), top to bottom; with `guess` also the unsure ones with the values read
    /// most often.
    pub found: Vec<(String, Option<Int>, Option<Int>)>,
    /// The materials whose numbers were not read for sure.
    pub unsure: Vec<String>,
    /// The unsure rows with their place.
    pub details: Vec<Unsure>,
}

/// `merge_passes`: several readings of the same picture made into one yield. Rows of different readings belong
/// together when their vertical middles lie less than half a height of a row apart; material, quality and amount
/// are voted on each by itself, a value counting when at least `minimum` readings read it alike. A value with a
/// confusable digit counts only when [`Checks::confirm`] confirms it.
pub fn merge_passes(
    passes: &[Vec<Word>],
    index: &Index,
    minimum: Option<usize>,
    checks: &Checks,
    guess_unsure: bool,
) -> Merged {
    merge_passes_with(passes, index, minimum, checks, guess_unsure, None)
}

/// The heads that hold for every reading of a part: those of the first reading that read the head of the yield by
/// name, else `fallback` (the heads the search over the whole picture read over the part), else those of the first
/// reading that took the yield right of the quantity.
fn shared_heads(passes: &[Vec<Word>], fallback: Option<&[f64]>) -> Option<Vec<f64>> {
    let read: Vec<(Vec<f64>, bool)> = passes.iter().filter_map(|words| heads_of(&rows(words))).collect();
    read.iter()
        .find(|(_, named)| *named)
        .map(|(heads, _)| heads.clone())
        .or_else(|| fallback.map(<[f64]>::to_vec))
        .or_else(|| read.first().map(|(heads, _)| heads.clone()))
}

/// [`merge_passes`], where a reading without heads of its own takes the heads of [`shared_heads`].
fn merge_passes_with(
    passes: &[Vec<Word>],
    index: &Index,
    minimum: Option<usize>,
    checks: &Checks,
    guess_unsure: bool,
    fallback: Option<&[f64]>,
) -> Merged {
    let minimum = minimum.unwrap_or(MIN_VOTES).min(passes.len().max(1));
    let shared = shared_heads(passes, fallback);
    struct Cluster {
        y: f64,
        rows: Vec<ParsedRow>,
    }
    let mut clusters: Vec<Cluster> = Vec::new();
    for words in passes {
        for row in parse_rows_with(&rows(words), index, shared.as_deref()) {
            match clusters.iter_mut().find(|c| (c.y - row.y).abs() < row.height / 2.0) {
                Some(cluster) => cluster.rows.push(row),
                None => clusters.push(Cluster { y: row.y, rows: vec![row] }),
            }
        }
    }
    clusters.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut merged = Merged::default();
    for cluster in &clusters {
        let material = vote(&cluster.rows.iter().map(|r| Some(r.material.clone())).collect::<Vec<_>>(), 1, None);
        let mut quality = vote(&cluster.rows.iter().map(|r| r.quality).collect::<Vec<_>>(), minimum, Some(RIVAL_VOTES));
        let mut amount = vote(&cluster.rows.iter().map(|r| r.amount).collect::<Vec<_>>(), minimum, Some(RIVAL_VOTES));
        let Some(material) = material else { continue };
        for column in [0, 1] {
            let value = if column == 0 { quality } else { amount };
            if !needs_confirm(value) {
                continue;
            }
            let checked = match (&checks.confirm, value) {
                (Some(confirm), Some(v)) => confirm(cluster.y, column, v),
                _ => Checked::No,
            };
            let decided = match checked {
                Checked::Yes => continue,
                Checked::Value(v) => Some(v),
                Checked::No => None,
            };
            if column == 0 {
                quality = decided;
            } else {
                amount = decided;
            }
        }
        if let Some(reread) = &checks.reread
            && (quality.is_none() || amount.is_none())
        {
            if quality.is_none() {
                quality = reread(cluster.y, 0);
            }
            if amount.is_none() {
                amount = reread(cluster.y, 1);
            }
        }
        if quality.is_none() || amount.is_none() {
            merged.unsure.push(material.clone());
            merged.details.push(Unsure { material: material.clone(), y: cluster.y });
            if guess_unsure {
                // Real digits first; readings of mistakable characters only when no reading had real digits.
                let best = |real: &dyn Fn(&ParsedRow) -> Option<Int>, loose: &dyn Fn(&ParsedRow) -> Option<Int>| {
                    guess(&cluster.rows.iter().map(real).collect::<Vec<_>>())
                        .or_else(|| guess(&cluster.rows.iter().map(loose).collect::<Vec<_>>()))
                };
                if quality.is_none() {
                    quality = best(&|r| r.quality, &|r| r.loose_quality);
                }
                if amount.is_none() {
                    amount = best(&|r| r.amount, &|r| r.loose_amount);
                }
                if quality.is_some() || amount.is_some() {
                    merged.found.push((material, quality, amount));
                }
            }
            continue;
        }
        merged.found.push((material, quality, amount));
    }
    merged
}

/// `as_text`: the lines in the form of the refinery field (`Material Q Menge cSCU`). A value not read stands as
/// `?`. Goods counted in pieces get no unit: an added cSCU would only bring the hint of a mistaken unit.
pub fn as_text(found: &[(String, Option<Int>, Option<Int>)], index: &Index) -> String {
    let cell = |value: &Option<Int>| value.map_or_else(|| "?".to_string(), |v| v.to_string());
    found
        .iter()
        .map(|(name, quality, amount)| {
            let unit = if index.counts_in_pieces(name) { "" } else { " cSCU" };
            format!("{name} {} {}{unit}", cell(quality), cell(amount))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The readings over the part of the table: (enlargement, mode). The digits of the terminal are only about eight
/// pixels high on a screen of 1440 lines; only enlarged does the recognition read them, and every reading misreads
/// other digits.
pub const PASSES: [(f64, Mode); 4] = [(3.0, Mode::Plain), (5.0, Mode::Plain), (3.5, Mode::Dark), (4.0, Mode::Dark)];

/// How a part of the picture is prepared for the recognition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// As it is.
    Plain,
    /// Grey with the contrast spread, bright writing stays bright.
    Gray,
    /// Grey turned round, dark writing on a bright ground.
    Dark,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Plain => "plain",
            Mode::Gray => "gray",
            Mode::Dark => "dark",
        }
    }
}

/// The largest side `Windows.Media.Ocr` takes.
pub const OCR_LIMIT: f64 = 10000.0;
/// The search for the table enlarges so that the picture is about this high afterwards: the font of the terminal
/// grows with the height of the screen, and from about 20 pixels of height the recognition reads it.
pub const SEARCH_HEIGHT: f64 = 4320.0;

/// A part of the picture: x, y, width, height in pixels.
pub type Area = (i64, i64, i64, i64);

/// One reading the recognition is asked for: the part, the enlargement and the mode.
#[derive(Clone, Debug, PartialEq)]
pub struct Job {
    pub area: Area,
    pub scale: f64,
    pub mode: Mode,
}

/// `search_jobs`: the readings the table is searched with over the whole picture. Overlapping tiles of about
/// 0.9 × 0.5 heights of the picture, enlarged to [`SEARCH_HEIGHT`] / height (1 to 4); a tile overlaps the next by
/// a quarter, so a row on an edge stands whole in the neighbour.
pub fn search_jobs(width: i64, height: i64) -> Vec<Job> {
    let scale = (SEARCH_HEIGHT / height.max(1) as f64).clamp(1.0, 4.0);
    let mut tile_w = width.min((height as f64 * 0.9) as i64);
    let mut tile_h = height.min(1.max((height as f64 * 0.5) as i64));
    tile_w = tile_w.min((OCR_LIMIT / scale) as i64);
    tile_h = tile_h.min((OCR_LIMIT / scale) as i64);
    let starts = |total: i64, size: i64| -> Vec<i64> {
        if size >= total {
            return vec![0];
        }
        let step = 1.max((size as f64 * 0.75) as i64);
        let mut out: Vec<i64> = (0..total - size).step_by(step as usize).collect();
        out.push(total - size);
        out
    };
    let mut jobs = Vec::new();
    for y in starts(height, tile_h) {
        for x in starts(width, tile_w) {
            jobs.push(Job { area: (x, y, tile_w, tile_h), scale, mode: Mode::Plain });
        }
    }
    jobs
}

/// `merge_words`: the words of several tiles in one list, without doubles: a word read in two overlapping tiles
/// stands once (the same text, middles less than half a word apart).
pub fn merge_words(passes: &[Vec<Word>]) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    for words in passes {
        for word in words {
            let text = py_strip(&word.t);
            if text.is_empty() {
                continue;
            }
            let (cx, cy) = (word.middle_x(), word.middle_y());
            let limit = word.h.max(word.w / 2.0);
            let double = out.iter().any(|other| {
                other.t == text
                    && (other.middle_x() - cx).abs() < limit
                    && (other.middle_y() - cy).abs() < word.h.max(1.0)
            });
            if !double {
                out.push(Word { t: text.to_string(), ..word.clone() });
            }
        }
    }
    out
}

/// Heads of the yield table (`refinery_ui_JobCard_Table_*` of the `global.ini`, German and English), capitals
/// only; parts of words, so misread letters still hit (`QUALITÄT` as `OUAUTiT`). The terminal cuts the German head
/// of the yield to `AUSBEU`.
const HEAD_QUALITY: [&str; 7] = ["UALI", "UAUT", "QUAL", "QUAU", "ALIT", "ALLT", "AUTY"];

/// A head of the yield read into one word with the head left of it: the terminal sets `(CSCU)`, `QUALITÄT` and
/// `AUSBEU` so close that the recognition reads `(CSCWALlTäAUSBEU`. Such a word is cut where the yield begins,
/// each part with its share of the width; every other word stays as it is.
pub fn split_heads(words: &[Word]) -> Vec<Word> {
    let mut out = Vec::with_capacity(words.len());
    for word in words {
        let chars: Vec<char> = word.t.chars().collect();
        let upper: Vec<char> = chars.iter().map(|c| c.to_uppercase().next().unwrap_or(*c)).collect();
        let starts = |at: usize| {
            HEAD_YIELD.iter().any(|head| {
                let head: Vec<char> = head.chars().collect();
                upper.get(at..at + head.len()).is_some_and(|part| part == head.as_slice())
            })
        };
        let cut = (2..chars.len()).find(|&at| starts(at) && !starts(at - 1));
        match cut {
            Some(at) => {
                let left_w = word.w * at as f64 / chars.len() as f64;
                out.push(Word { t: chars[..at].iter().collect(), w: left_w, ..word.clone() });
                out.push(Word { t: chars[at..].iter().collect(), x: word.x + left_w, w: word.w - left_w, ..word.clone() });
            }
            None => out.push(word.clone()),
        }
    }
    out
}
/// The head of the yield, German and English; for English also the parts left when the recognition misreads one
/// letter (`YIELO`, `YtELD`, `YlELD`, `Y1ELD`), but not `IELD` alone, which `FIELD` and `SHIELD` hold.
const HEAD_YIELD: [&str; 8] = ["AUSBEU", "USBEU", "YIELD", "ERTRAG", "YIEL", "YTEL", "YLEL", "YELD"];
/// The heads of the column of materials left of it.
const HEAD_MATERIAL: [&str; 7] = ["WONNEN", "GEWONN", "MATERIA", "ATERIAL", "CSCU", "YIELDED", "IELDED"];

/// `_head`: whether the letters of the word (capitals A to Z only) hold one of the parts.
fn head(word: &Word, keys: &[&str]) -> bool {
    let text: String = word.t.to_uppercase().chars().filter(char::is_ascii_uppercase).collect();
    keys.iter().any(|key| text.contains(key))
}

/// The head under the name of the station at the top of the terminal (`STATION PROFIL`, `STATION PROFILE`), in
/// the comparison form of [`station_parts`] (`L` as `I`); parts of the word, so a letter misread at an end still
/// hits.
const HEAD_PROFILE: [&str; 2] = ["PROFI", "ROFII"];

/// How far above the head of the profile the name of the station may stand, in heights of the head.
const STATION_REACH: f64 = 10.0;

/// The shortest comparison form of a place that counts; shorter names hit too easily by chance.
const STATION_MIN_LEN: usize = 4;

/// The comparison form of a text for the name of the station: capitals, letters the recognition mistakes for one
/// another made one (`0` as `O`; `1` and `L` as `I`; `5` as `S`; `|` and `!` as `I` only when a letter or digit
/// follows), cut into parts at everything that is neither letter nor digit.
fn station_parts(text: &str) -> Vec<String> {
    let upper: Vec<char> = text.chars().flat_map(char::to_uppercase).collect();
    let mut parts = Vec::new();
    let mut part = String::new();
    for (at, &c) in upper.iter().enumerate() {
        let c = match c {
            '0' => 'O',
            '1' | 'L' => 'I',
            '|' | '!' if upper.get(at + 1).is_some_and(|next| next.is_alphanumeric()) => 'I',
            '5' => 'S',
            other => other,
        };
        if c.is_alphanumeric() {
            part.push(c);
        } else if !part.is_empty() {
            parts.push(std::mem::take(&mut part));
        }
    }
    if !part.is_empty() {
        parts.push(part);
    }
    parts
}

/// The readings of one row as parts, left to right. The recognition often gives one spot twice in different
/// spellings (`PROFIL` and `pROFlL`): words that lie over one another by more than half the narrower one are one
/// spot, spellings alike in comparison form are one. A spot with several spellings gives one reading per
/// spelling, the first reading the first spelling of every spot, the second the second (or the last there is).
fn row_readings(row: &[Word]) -> Vec<Vec<String>> {
    struct Spot {
        left: f64,
        right: f64,
        spellings: Vec<Vec<String>>,
    }
    let mut spots: Vec<Spot> = Vec::new();
    for word in row {
        let parts = station_parts(&word.t);
        if parts.is_empty() {
            continue;
        }
        let (left, right) = (word.x, word.x + word.w);
        let over = spots.last_mut().filter(|spot| {
            let shared = spot.right.min(right) - spot.left.max(left);
            shared > 0.5 * word.w.min(spot.right - spot.left)
        });
        match over {
            Some(spot) => {
                spot.left = spot.left.min(left);
                spot.right = spot.right.max(right);
                if !spot.spellings.contains(&parts) {
                    spot.spellings.push(parts);
                }
            }
            None => spots.push(Spot { left, right, spellings: vec![parts] }),
        }
    }
    let most = spots.iter().map(|spot| spot.spellings.len()).max().unwrap_or(0);
    (0..most)
        .map(|k| spots.iter().flat_map(|spot| spot.spellings[k.min(spot.spellings.len() - 1)].clone()).collect())
        .collect()
}

/// The places whose whole name stands in one row as a run of whole parts, in any of its readings. A name that
/// lies inside the run of a longer one found in the same reading counts not.
fn places_in_row<'a>(row: &[Word], places: &'a [String]) -> Vec<&'a str> {
    row_readings(row).iter().flat_map(|parts| places_in_parts(parts, places)).collect()
}

/// [`places_in_row`] for one reading of the row.
fn places_in_parts<'a>(parts: &[String], places: &'a [String]) -> Vec<&'a str> {
    let mut found: Vec<(&str, usize, usize)> = Vec::new();
    for place in places {
        let form: String = station_parts(place).concat();
        if form.chars().count() < STATION_MIN_LEN {
            continue;
        }
        for start in 0..parts.len() {
            let mut run = String::new();
            for (end, part) in parts.iter().enumerate().skip(start) {
                run.push_str(part);
                if !form.starts_with(&run) {
                    break;
                }
                if run == form {
                    found.push((place.as_str(), start, end));
                    break;
                }
            }
        }
    }
    let inside = |a: &(&str, usize, usize), b: &(&str, usize, usize)| {
        b.1 <= a.1 && a.2 <= b.2 && (b.1, b.2) != (a.1, a.2)
    };
    found.iter().filter(|a| !found.iter().any(|b| inside(a, b))).map(|a| a.0).collect()
}

/// The storage location the terminal stands at: the name of the station above the head of its profile
/// (`ARC-L1 WIDE FOREST STATION` over `STATION PROFIL`), compared with `places` without case and with the
/// letters the recognition mistakes. The name is the biggest writing above the head: of the rows at most
/// [`STATION_REACH`] heights of the head above it only those with the tallest words count (a tenth of slack), so
/// a place named in a smaller line over it counts not. Only a sure hit counts: the head is found, and those rows
/// name exactly one place. The name as the list writes it, or `None`.
pub fn station(words: &[Word], places: &[String]) -> Option<String> {
    if places.is_empty() {
        return None;
    }
    let lines = rows(words);
    let mut hits: Vec<&str> = Vec::new();
    let profile = |w: &&Word| {
        let form = station_parts(&w.t).concat();
        HEAD_PROFILE.iter().any(|key| form.contains(key))
    };
    let tallest = |line: &[Word]| line.iter().map(|w| w.h).fold(0.0, f64::max);
    for anchor in words.iter().filter(profile) {
        let reach = STATION_REACH * anchor.h.max(4.0);
        let above: Vec<&Vec<Word>> = lines
            .iter()
            .filter(|line| {
                let top = line.iter().map(|w| w.y).fold(f64::INFINITY, f64::min);
                let bottom = line.iter().map(|w| w.y + w.h).fold(f64::NEG_INFINITY, f64::max);
                bottom <= anchor.middle_y() && anchor.y - top <= reach
            })
            .collect();
        let biggest = above.iter().map(|line| tallest(line)).fold(0.0, f64::max);
        for line in above.into_iter().filter(|line| tallest(line) >= 0.9 * biggest) {
            hits.extend(places_in_row(line, places));
        }
    }
    hits.sort_unstable();
    hits.dedup();
    match hits.as_slice() {
        [one] => Some(one.to_string()),
        _ => None,
    }
}

/// A yield table found on the picture: its part and the sum as text with a point (`12.76`) or empty.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub area: Area,
    pub total: String,
}

/// `find_tables`: every yield table of the picture, left to right, top to bottom. Two job cards side by side have
/// a row of heads each and so an entry each. A row of heads is a row with a head of the quality and right of it
/// one of the yield; the list of materials at the left of the terminal (the profile of the station) has no head of
/// the quality and counts not.
pub fn find_tables(words: &[Word], width: i64, height: i64) -> Vec<Table> {
    struct Candidate<'a> {
        score: usize,
        quality: &'a Word,
        unit: f64,
        heads: Vec<&'a Word>,
        rights: Vec<&'a Word>,
    }
    let split = split_heads(words);
    let words = split.as_slice();
    let lines = rows(words);
    let mut candidates: Vec<Candidate> = Vec::new();
    for line in &lines {
        for quality in line.iter().filter(|w| head(w, &HEAD_QUALITY)) {
            let unit = quality.h.max(4.0);
            let qx = quality.x;
            let yields = line.iter().any(|w| head(w, &HEAD_YIELD) && 0.0 < w.x - qx && w.x - qx < 20.0 * unit);
            if !yields {
                continue;
            }
            let heads: Vec<&Word> =
                line.iter().filter(|w| head(w, &HEAD_MATERIAL) && 0.0 < qx - w.x && qx - w.x < 30.0 * unit).collect();
            let rights: Vec<&Word> = line.iter().filter(|w| 0.0 <= w.x - qx && w.x - qx < 25.0 * unit).collect();
            candidates.push(Candidate { score: heads.len() + rights.len(), quality, unit, heads, rights });
        }
    }
    // One entry per row of heads: a second head of the quality in the same table (misread, doubled) lies closer
    // than 30 heights of the font.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.score));
    let mut chosen: Vec<&Candidate> = Vec::new();
    for candidate in &candidates {
        let (q, unit) = (candidate.quality, candidate.unit);
        let near = chosen
            .iter()
            .any(|other| (q.x - other.quality.x).abs() < 30.0 * unit && (q.y - other.quality.y).abs() < 10.0 * unit);
        if !near {
            chosen.push(candidate);
        }
    }
    let mut tables: Vec<Table> =
        chosen.iter().filter_map(|c| table_box(words, width, height, c.quality, c.unit, &c.heads, &c.rights)).collect();
    tables.sort_by_key(|t| (t.area.0, t.area.1));
    tables
}

/// `_table_box`: the part and the sum of a row of heads found.
fn table_box(
    words: &[Word],
    width: i64,
    height: i64,
    quality: &Word,
    unit: f64,
    heads: &[&Word],
    rights: &[&Word],
) -> Option<Table> {
    let qx = quality.x;
    let left =
        if heads.is_empty() { qx - 24.0 * unit } else { heads.iter().map(|w| w.x).fold(f64::INFINITY, f64::min) };
    let header: Vec<&Word> = heads.iter().chain(rights).copied().collect();
    let right = header.iter().map(|w| w.x + w.w).fold(f64::NEG_INFINITY, f64::max);
    let top = header.iter().map(|w| w.y).fold(f64::INFINITY, f64::min);
    let header_bottom = header.iter().map(|w| w.y + w.h).fold(f64::NEG_INFINITY, f64::max);
    let below: Vec<&Word> = words
        .iter()
        .filter(|w| {
            w.y > header_bottom + 2.0 * unit
                && w.y < header_bottom + 60.0 * unit
                && left - unit <= w.x
                && w.x <= right
                && (head(w, &["SCU"]) || (head(w, &HEAD_YIELD) && w.x < qx))
        })
        .collect();
    let mut total = String::new();
    let bottom = if below.is_empty() {
        header_bottom + 55.0 * unit
    } else {
        let bottom = below.iter().map(|w| w.y).fold(f64::INFINITY, f64::min);
        let mut line: Vec<&Word> = words
            .iter()
            .filter(|w| (w.y - bottom).abs() < 3.0 * unit && left - unit <= w.x && w.x <= right + 4.0 * unit)
            .collect();
        line.sort_by(|a, b| a.x.total_cmp(&b.x));
        for word in line {
            if let Some(found) = first_decimal(&word.t) {
                total = found.replace(',', ".");
                break;
            }
        }
        bottom
    };
    let x0 = 0.max((left - unit) as i64);
    let y0 = 0.max((top - unit) as i64);
    let x1 = width.min((right + 2.0 * unit) as i64);
    let y1 = height.min(bottom as i64);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(Table { area: (x0, y0, x1 - x0, y1 - y0), total })
}

/// The first match of `\d+(?:[.,]\d+)?` in `text`.
fn first_decimal(text: &str) -> Option<String> {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| regex::Regex::new(r"\d+(?:[.,]\d+)?").expect("pattern"));
    pattern.find(text).map(|m| m.as_str().to_string())
}

/// `find_table`: the part of the first yield table, or `None`.
pub fn find_table(words: &[Word], width: i64, height: i64) -> Option<Area> {
    find_tables(words, width, height).first().map(|t| t.area)
}

/// Parts of the titles of the cards (`refinery_ui_WorkOrderCard_Title_*`, `refinery_ui_WorkOrderComplete`), German
/// and English, capitals.
const DONE_WORDS: [&str; 2] = ["ABGESCHLOSS", "COMPLETE"];
const RUNNING_WORDS: [&str; 2] = ["VERARBEIT", "PROCESSING"];

/// The state of a job card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// `fertig`
    Done,
    /// `laeuft`
    Running,
    /// Neither word was read.
    Unknown,
}

impl State {
    /// The word 4.12 keeps (`fertig`, `laeuft`, empty).
    pub fn word(self) -> &'static str {
        match self {
            State::Done => "fertig",
            State::Running => "laeuft",
            State::Unknown => "",
        }
    }
}

/// `job_state`: the state of the card over the table `area`: words whose horizontal middle lies within the table
/// count; a word of the end beats one of the work.
pub fn job_state(words: &[Word], area: Area) -> State {
    let (x, _, w, _) = area;
    let mut state = State::Unknown;
    for word in words {
        let middle = word.middle_x();
        if !(x as f64 <= middle && middle <= (x + w) as f64) {
            continue;
        }
        let text = word.t.to_uppercase();
        if DONE_WORDS.iter().any(|part| text.contains(part)) {
            return State::Done;
        }
        if RUNNING_WORDS.iter().any(|part| text.contains(part)) {
            state = State::Running;
        }
    }
    state
}

/// Parts of the label beside the time left (`VERBLEIBENDE ZEIT`, `TIME REMAINING`).
/// Also the parts a doubled reading leaves of the label (`LEIBENDE` of `VERBLEIBENDE`).
const TIME_LABEL_WORDS: [&str; 6] = ["VERBLEIB", "EIBEND", "ZEIT", "TIME", "REMAIN", "AINING"];

/// `_time_digits`: a word of time with mistaken digits put right (`Ih` becomes `1h`); only the part before the
/// unit is swapped, the unit stays.
fn time_digits(part: &str) -> String {
    static PATTERN: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = PATTERN.get_or_init(|| regex::Regex::new(r"(?i)^([\dOoDQIl|i!SsBZz]+)([dhms])$").expect("pattern"));
    match pattern.captures(part) {
        Some(found) => format!("{}{}", translate_lookalikes(&found[1]), &found[2]),
        None => part.to_string(),
    }
}

/// `remaining_seconds`: the time left of the card over the table `area` in seconds, or `None`. Searched is a row
/// in the column of the table that consists, besides the label, only of times (`1h 3m`, `43m 57s`, `2d 4h`).
pub fn remaining_seconds(words: &[Word], area: Area) -> Option<i64> {
    static ONLY: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static PART: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let only = ONLY.get_or_init(|| regex::Regex::new(r"(?i)^(?:\d+\s*[dhms]\s*)+$").expect("pattern"));
    let part = PART.get_or_init(|| regex::Regex::new(r"(?i)(\d+)\s*([dhms])").expect("pattern"));
    let (x, _, w, _) = area;
    let column: Vec<Word> = words
        .iter()
        .filter(|word| (x as f64) <= word.middle_x() && word.middle_x() <= (x + w) as f64)
        .cloned()
        .collect();
    for line in rows(&column) {
        let parts: Vec<String> = line
            .iter()
            .map(|word| py_strip(&word.t).to_string())
            .filter(|p| !p.is_empty() && !TIME_LABEL_WORDS.iter().any(|label| p.to_uppercase().contains(label)))
            .map(|p| time_digits(&p))
            .collect();
        let text = parts.join(" ");
        if text.is_empty() || !only.is_match(&text) {
            continue;
        }
        let seconds = part.captures_iter(&text).map(|found| {
            let n: i64 = found[1].parse().unwrap_or(0);
            let unit = match found[2].to_ascii_lowercase().as_str() {
                "d" => 86400,
                "h" => 3600,
                "m" => 60,
                _ => 1,
            };
            n.saturating_mul(unit)
        });
        return Some(seconds.fold(0i64, i64::saturating_add));
    }
    None
}

/// `_materials_in_order`: the materials of the table top to bottom, from the reading with the most rows of
/// materials.
pub fn materials_in_order(passes: &[Vec<Word>], index: &Index) -> Vec<String> {
    let mut best: Vec<String> = Vec::new();
    for words in passes {
        let names: Vec<String> = material_rows(&rows(words), index).into_iter().map(|r| r.material).collect();
        if names.len() > best.len() {
            best = names;
        }
    }
    best
}

// ------------------------------------------------------------ reading a picture

/// Why a reading failed (`ScanError`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanError {
    pub reason: Reason,
    pub detail: String,
}

/// The kinds of [`ScanError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The screen could not be grabbed.
    Grab,
    /// No text recognition on this system.
    Unsupported,
    /// Windows has no language for the text recognition.
    NoLanguage,
    /// The text recognition failed.
    Ocr,
}

impl Reason {
    /// The word 4.12 uses (`abgriff`, `nicht_unterstuetzt`, `keine_sprache`, `ocr`).
    pub fn word(self) -> &'static str {
        match self {
            Reason::Grab => "abgriff",
            Reason::Unsupported => "nicht_unterstuetzt",
            Reason::NoLanguage => "keine_sprache",
            Reason::Ocr => "ocr",
        }
    }
}

impl ScanError {
    pub fn new(reason: Reason, detail: impl Into<String>) -> ScanError {
        ScanError { reason, detail: detail.into() }
    }
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.detail.is_empty() {
            write!(f, "{}", self.reason.word())
        } else {
            write!(f, "{}: {}", self.reason.word(), self.detail)
        }
    }
}

/// The picture the cells of numbers are cut from (`win_ocr.read_bmp` and `win_ocr.scaled`).
pub trait Picture {
    /// Width and height in pixels.
    fn size(&self) -> (i64, i64);
    /// The part `area` (within the picture) enlarged by `scale` and prepared as `mode`: width, height and BGRA,
    /// row by row. `None` when that fails.
    fn scaled(&self, area: Area, scale: f64, mode: Mode) -> Option<(usize, usize, Vec<u8>)>;
}

/// The text recognition (`ocr_image`): for each job whose part lies in the picture one list of words, in pixels
/// of the picture. A job whose part lies wholly outside gives no list.
pub trait Ocr {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError>;
}

/// `cell_crop`: the grey picture of a cell of numbers, enlarged to [`digits::CELL_HEIGHT`]; the green channel of
/// the enlarged part.
pub fn cell_crop(picture: &dyn Picture, area: Area) -> Option<digits::Gray> {
    let (width, height) = picture.size();
    let (x, y) = (area.0.max(0), area.1.max(0));
    let (w, h) = (area.2.min(width - x), area.3.min(height - y));
    if w <= 0 || h <= 0 {
        return None;
    }
    let (tw, th, raw) = picture.scaled((x, y, w, h), digits::CELL_HEIGHT / h as f64, Mode::Plain)?;
    let gray: Vec<u8> = raw.iter().skip(1).step_by(4).copied().collect();
    (gray.len() == tw * th).then_some(digits::Gray { width: tw, height: th, gray })
}

/// `_centers`: the middles of the columns of the table, from the reading with the most numbers; a reading without
/// heads of its own takes those of [`shared_heads`].
fn centers(passes: &[Vec<Word>], index: &Index, fallback: Option<&[f64]>) -> Vec<f64> {
    let shared = shared_heads(passes, fallback);
    let (mut best, mut most) = (Vec::new(), -1i64);
    for words in passes {
        let table = rows(words);
        let found = material_rows(&table, index);
        let count: usize = found.iter().map(|r| r.numbers.len() + r.loose_numbers.len()).sum();
        if count as i64 > most {
            best = table_columns_with(&table, &found, shared.as_deref());
            most = count as i64;
        }
    }
    best
}

/// `_tolerance`: the middle height of all words of the readings, 10 without words.
fn tolerance(passes: &[Vec<Word>]) -> f64 {
    let mut heights: Vec<f64> = passes.iter().flatten().map(|w| w.h).collect();
    heights.sort_by(f64::total_cmp);
    heights.get(heights.len() / 2).copied().unwrap_or(10.0)
}

/// `_cell_word`: the word of the cell in row `y`, column `column` (0 quality, 1 amount), from the first reading
/// that has a number there.
fn cell_word<'a>(passes: &'a [Vec<Word>], y: f64, column: usize, tolerance: f64, centers: &[f64]) -> Option<&'a Word> {
    if centers.len() <= column {
        return None;
    }
    passes.iter().flatten().find(|word| {
        number(&word.t, true).is_some()
            && (word.middle_y() - y).abs() <= tolerance
            && nearest(centers, word.middle_x()) == column
    })
}

/// `_cell_crop_at`: the word of a cell and its grey picture.
fn cell_crop_at<'a>(
    picture: &dyn Picture,
    passes: &'a [Vec<Word>],
    y: f64,
    column: usize,
    centers: &[f64],
) -> Option<(&'a Word, digits::Gray)> {
    let word = cell_word(passes, y, column, tolerance(passes), centers)?;
    let area = (word.x as i64, word.y as i64, word.w as i64, word.h as i64);
    cell_crop(picture, area).map(|crop| (word, crop))
}

/// `_confirmer`: decides every 0 and 8 of a value at the learned patterns. First the whole cell is read
/// ([`digits::read_cell`]); if that works, that value holds. Else [`digits::classify`] assigns only each 0 and 8
/// of the value read.
fn confirmer<'a>(
    picture: &'a dyn Picture,
    passes: &'a [Vec<Word>],
    learned: &'a digits::Digits,
    index: &Index,
    fallback: Option<&[f64]>,
) -> Box<dyn Fn(f64, usize, Int) -> Checked + 'a> {
    let centers = centers(passes, index, fallback);
    Box::new(move |y, column, value| {
        let text = value.to_string();
        let Some((_, crop)) = cell_crop_at(picture, passes, y, column, &centers) else { return Checked::No };
        if let Some(whole) = digits::read_cell(&crop, learned, &text) {
            return Checked::Value(whole);
        }
        let found = digits::glyphs(Some(&crop), Some(text.len()));
        if found.len() != text.len() {
            return Checked::No;
        }
        let mut out = String::new();
        for (digit, vector) in text.chars().zip(&found) {
            if CONFUSABLE.contains(&digit) {
                match digits::classify(vector, learned) {
                    Some(decided) => out.push(decided),
                    None => return Checked::No,
                }
            } else {
                out.push(digit);
            }
        }
        out.parse().map_or(Checked::No, Checked::Value)
    })
}

/// `_rereader`: reads a cell of an unsure row from the learned patterns alone.
fn rereader<'a>(
    picture: &'a dyn Picture,
    passes: &'a [Vec<Word>],
    learned: &'a digits::Digits,
    index: &Index,
    fallback: Option<&[f64]>,
) -> Box<dyn Fn(f64, usize) -> Option<Int> + 'a> {
    let centers = centers(passes, index, fallback);
    Box::new(move |y, column| {
        let (word, crop) = cell_crop_at(picture, passes, y, column, &centers)?;
        digits::read_cell(&crop, learned, &word.t)
    })
}

/// `unsure_cells`: the cells of numbers of the unsure rows, for learning later; `fallback` as in [`shared_heads`].
pub fn unsure_cells(
    picture: Option<&dyn Picture>,
    passes: &[Vec<Word>],
    details: &[Unsure],
    index: &Index,
    fallback: Option<&[f64]>,
) -> Vec<digits::Cell> {
    let Some(picture) = picture else { return Vec::new() };
    let centers = centers(passes, index, fallback);
    let mut cells = Vec::new();
    for row in details {
        for column in [0, 1] {
            if let Some((word, crop)) = cell_crop_at(picture, passes, row.y, column, &centers) {
                cells.push(digits::Cell { material: row.material.clone(), column, read: word.t.clone(), crop });
            }
        }
    }
    cells
}

/// What is kept of a reading that was not sure, for a bug report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keep {
    /// The whole picture (no table found).
    Picture,
    /// The part of a table.
    Table(Area),
}

/// One job card read from the picture (an entry of `read_image`).
#[derive(Clone, Debug, PartialEq)]
pub struct ScannedJob {
    /// The lines for the refinery field.
    pub text: String,
    /// The materials without sure numbers.
    pub unsure: Vec<String>,
    /// The materials read, in order.
    pub materials: Vec<String>,
    /// The sum as text with a point, or empty.
    pub total: String,
    pub state: State,
    /// Time left in seconds; `None` when done or not readable.
    pub remaining: Option<i64>,
    /// The cells of the unsure rows, for learning.
    pub cells: Vec<digits::Cell>,
    /// The storage location read at the top of the terminal ([`station`]), as the list writes it; empty when
    /// there was no sure hit.
    pub station: String,
}

/// The lines of a reading joined to the lines that wait in the refinery field, when both show the same job
/// scrolled: a list longer than the card shows seven lines, the next reading after scrolling the rest. They are
/// the same job when they share a line. Each line stays as often as the one of the two that has it more often,
/// the field's lines first, the new ones after. `(text, lines added)`, or `None` when they share no line.
pub fn join_scrolled(before: &str, read: &str) -> Option<(String, usize)> {
    let lines = |text: &str| -> Vec<String> {
        text.lines().map(|l| py_strip(l).to_string()).filter(|l| !l.is_empty()).collect()
    };
    let (mut joined, new) = (lines(before), lines(read));
    if joined.is_empty() || !new.iter().any(|line| joined.contains(line)) {
        return None;
    }
    let mut added = 0;
    for line in new.iter() {
        let have = joined.iter().filter(|l| *l == line).count();
        let want = new.iter().filter(|l| *l == line).count();
        if have < want {
            joined.push(line.clone());
            added += 1;
        }
    }
    Some((joined.join("\n"), added))
}

/// The words whose middle lies within `area`.
fn within(words: &[Word], area: Area) -> Vec<Word> {
    let (x, y, w, h) = (area.0 as f64, area.1 as f64, area.2 as f64, area.3 as f64);
    words
        .iter()
        .filter(|word| {
            let (cx, cy) = (word.middle_x(), word.middle_y());
            x <= cx && cx <= x + w && y <= cy && cy <= y + h
        })
        .cloned()
        .collect()
}

/// `read_image`: a picture read, `(jobs, kept)` for the refinery field. First the search for the tables over the
/// whole picture ([`search_jobs`], [`find_tables`]), then per table enlarged readings of its part only
/// ([`PASSES`]), made into one by [`merge_passes`]; a reading without heads takes those of another reading, else
/// those the search read over the part. The jobs stand left to right. Each job carries the station read from the
/// search ([`station`]); without a table the list is empty, or one job without lines when a station was read.
///
/// `picture` gives the cells for the learned digits (`None`: no checks at the patterns). `keep`: a reading that
/// stays empty or unsure keeps a picture for the bug report, the part of the table when it was found, else the
/// whole picture; `kept` says whether that happened.
pub fn read_image(
    size: (i64, i64),
    picture: Option<&dyn Picture>,
    ocr: &mut dyn Ocr,
    index: &Index,
    learned: &digits::Digits,
    mut keep: Option<&mut dyn FnMut(Keep) -> bool>,
) -> Result<(Vec<ScannedJob>, bool), ScanError> {
    let (width, height) = size;
    let search = ocr.read(&search_jobs(width, height))?;
    let words = merge_words(&search);
    let tables = find_tables(&words, width, height);
    let at = station(&words, &index.places).unwrap_or_default();
    if tables.is_empty() {
        let kept = keep.as_mut().is_some_and(|keep| keep(Keep::Picture));
        // A station read without a table still goes to the field of the place, in a job without lines.
        let jobs = if at.is_empty() {
            Vec::new()
        } else {
            vec![ScannedJob {
                text: String::new(),
                unsure: Vec::new(),
                materials: Vec::new(),
                total: String::new(),
                state: State::Unknown,
                remaining: None,
                cells: Vec::new(),
                station: at,
            }]
        };
        return Ok((jobs, kept));
    }
    let (mut jobs, mut kept) = (Vec::new(), false);
    let checks_from = picture.filter(|_| !learned.is_empty());
    for table in &tables {
        let area = table.area;
        let passes =
            ocr.read(&PASSES.iter().map(|(scale, mode)| Job { area, scale: *scale, mode: *mode }).collect::<Vec<_>>())?;
        // The heads the search read over the part, for when no reading of the part reads them.
        let searched = header_columns(&rows(&within(&words, area)));
        let searched = searched.as_deref();
        let checks = match checks_from {
            Some(picture) => Checks {
                confirm: Some(confirmer(picture, &passes, learned, index, searched)),
                reread: Some(rereader(picture, &passes, learned, index, searched)),
            },
            None => Checks::default(),
        };
        let merged = merge_passes_with(&passes, index, None, &checks, true, searched);
        drop(checks);
        if let Some(keep) = keep.as_mut()
            && (!merged.unsure.is_empty() || merged.found.is_empty())
        {
            kept = keep(Keep::Table(area)) || kept;
        }
        let state = job_state(&words, area);
        jobs.push(ScannedJob {
            text: as_text(&merged.found, index),
            materials: materials_in_order(&passes, index),
            total: table.total.clone(),
            state,
            remaining: if state == State::Done { None } else { remaining_seconds(&words, area) },
            cells: unsure_cells(picture, &passes, &merged.details, index, searched),
            unsure: merged.unsure,
            station: at.clone(),
        });
    }
    Ok((jobs, kept))
}

#[cfg(test)]
mod tests;
