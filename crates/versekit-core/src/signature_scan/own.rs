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

//! The digits the player taught: the files they stand in, which image each came from, and forgetting them.
//!
//! A file holds the taught patterns (`ziffern`) and the ones derived from them (`abgeleitet`), each per digit in
//! the order they were taught. Beside each list stands, place for place, the names of the images kept that gave
//! the pattern (`herkunft`, `herkunft_abgeleitet`); a pattern two images gave names both. Patterns taught before
//! these lists name none.
//!
//! | Forgotten | What goes |
//! |---|---|
//! | an image ([`forget_sample`]) | the image, every pattern only it gave, and, for patterns naming no image, those the image gives taught again |
//! | a digit pattern ([`forget_digit`]) | the pattern in every file, and the derived ones that came nearest from it |
//! | everything ([`forget_all`]) | the three files; the images stay |
//!
//! ⚠ Patterns naming no image are found again by teaching the image again ([`lesson`]): the image kept is the
//! crop of ONE grey image, so what the other grey image learned from the same number stays. Only forgetting
//! everything takes that too.

use super::*;

/// Which list of a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Taught by the player.
    Taught,
    /// Derived from a taught pattern.
    Derived,
}

impl Kind {
    /// The field of the patterns and the field of their images.
    fn fields(self) -> (&'static str, &'static str) {
        match self {
            Kind::Taught => ("ziffern", "herkunft"),
            Kind::Derived => ("abgeleitet", "herkunft_abgeleitet"),
        }
    }
}

/// One pattern of a file and the images it came from (empty: taught before images were named).
#[derive(Clone, Debug, PartialEq)]
struct Entry {
    pattern: Pattern,
    sources: Vec<String>,
}

/// The patterns per digit, in the order of the file.
type Lists = Vec<(char, Vec<Entry>)>;

/// One file of taught templates, read to be changed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnFile {
    taught: Lists,
    derived: Lists,
}

/// Why a file could not be changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileError {
    /// There but not readable (locked, not JSON): it is left as it is.
    Read,
    /// Could not be written.
    Write,
}

impl OwnFile {
    /// The file at `path`; a file that is not there is empty.
    pub fn open(path: &str) -> Result<OwnFile, FileError> {
        match jsonio::read_json(path) {
            Ok(data @ Value::Object(_)) => Ok(OwnFile::of_value(&data)),
            Err(jsonio::ReadError::NotFound) => Ok(OwnFile::default()),
            Ok(_) | Err(_) => Err(FileError::Read),
        }
    }

    fn of_value(data: &Value) -> OwnFile {
        let lists = |kind: Kind| -> Lists {
            let (field, from) = kind.fields();
            let Some(Value::Object(digits)) = data.get(field) else { return Vec::new() };
            let sources = data.get(from);
            let mut out: Lists = Vec::new();
            for (key, list) in digits {
                let mut chars = key.chars();
                let (Some(digit), None) = (chars.next(), chars.next()) else { continue };
                if !digit.is_ascii_digit() {
                    continue;
                }
                let Value::Array(list) = list else { continue };
                let named = match sources.and_then(|s| s.get(key)) {
                    Some(Value::Array(named)) => named.as_slice(),
                    _ => &[],
                };
                let entries: Vec<Entry> = list
                    .iter()
                    .enumerate()
                    .filter_map(|(at, value)| {
                        let pattern = pattern_of(value)?;
                        let sources = match named.get(at) {
                            Some(Value::Array(names)) => {
                                names.iter().filter_map(|n| n.as_str().map(str::to_string)).collect()
                            }
                            _ => Vec::new(),
                        };
                        Some(Entry { pattern, sources })
                    })
                    .collect();
                match out.iter_mut().find(|(d, _)| *d == digit) {
                    Some((_, known)) => known.extend(entries),
                    None => out.push((digit, entries)),
                }
            }
            out
        };
        OwnFile { taught: lists(Kind::Taught), derived: lists(Kind::Derived) }
    }

    fn lists(&self, kind: Kind) -> &Lists {
        match kind {
            Kind::Taught => &self.taught,
            Kind::Derived => &self.derived,
        }
    }

    fn lists_mut(&mut self, kind: Kind) -> &mut Lists {
        match kind {
            Kind::Taught => &mut self.taught,
            Kind::Derived => &mut self.derived,
        }
    }

    /// Adds a pattern of `digit` taught from the image `source`; a pattern already there gets the image named too.
    /// At most `limit` per digit: the oldest go.
    pub fn add(&mut self, kind: Kind, digit: char, pattern: &Pattern, source: Option<&str>, limit: usize) {
        let lists = self.lists_mut(kind);
        let at = match lists.iter().position(|(d, _)| *d == digit) {
            Some(at) => at,
            None => {
                lists.push((digit, Vec::new()));
                lists.len() - 1
            }
        };
        let list = &mut lists[at].1;
        if let Some(entry) = list.iter_mut().find(|e| e.pattern == *pattern) {
            if let Some(name) = source
                && !entry.sources.iter().any(|n| n == name)
            {
                entry.sources.push(name.to_string());
            }
            return;
        }
        list.push(Entry { pattern: *pattern, sources: source.map(|n| vec![n.to_string()]).unwrap_or_default() });
        let excess = list.len().saturating_sub(limit);
        list.drain(..excess);
    }

    /// The taught patterns: (digit, pattern, images).
    pub fn taught(&self) -> Vec<(char, Pattern, Vec<String>)> {
        self.taught.iter().flat_map(|(d, list)| list.iter().map(|e| (*d, e.pattern, e.sources.clone()))).collect()
    }

    /// Whether the list `kind` holds `pattern` for `digit`.
    pub fn contains(&self, kind: Kind, digit: char, pattern: &Pattern) -> bool {
        self.lists(kind).iter().any(|(d, list)| *d == digit && list.iter().any(|e| e.pattern == *pattern))
    }

    /// How many patterns, taught and derived.
    pub fn len(&self) -> usize {
        [&self.taught, &self.derived].iter().flat_map(|l| l.iter()).map(|(_, list)| list.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Takes the image `name` out of every pattern; a pattern only it gave goes. Gives how many went.
    fn drop_source(&mut self, name: &str) -> usize {
        let mut gone = 0;
        for kind in [Kind::Taught, Kind::Derived] {
            for (_, list) in self.lists_mut(kind) {
                let before = list.len();
                list.retain_mut(|e| {
                    let named = e.sources.len();
                    e.sources.retain(|n| n != name);
                    !(named > 0 && e.sources.is_empty())
                });
                gone += before - list.len();
            }
        }
        gone
    }

    /// Takes out the patterns that name no image and are among `patterns` (kind, digit, pattern).
    fn drop_unnamed(&mut self, patterns: &[(Kind, char, Pattern)]) -> usize {
        let mut gone = 0;
        for kind in [Kind::Taught, Kind::Derived] {
            for (digit, list) in self.lists_mut(kind) {
                let before = list.len();
                list.retain(|e| {
                    !(e.sources.is_empty() && patterns.iter().any(|(k, d, p)| *k == kind && d == digit && *p == e.pattern))
                });
                gone += before - list.len();
            }
        }
        gone
    }

    /// Takes out the taught `pattern` of `digit` and the derived patterns of the digit that lie nearer to it than
    /// to every other taught pattern of the digit. Gives how many went.
    fn drop_taught(&mut self, digit: char, pattern: &Pattern) -> usize {
        let taught: Vec<Pattern> = self
            .taught
            .iter()
            .filter(|(d, _)| *d == digit)
            .flat_map(|(_, list)| list.iter().map(|e| e.pattern))
            .collect();
        if !taught.contains(pattern) {
            return 0;
        }
        let distance = |a: &Pattern, b: &Pattern| {
            let (a, b) = (bits_of(&fill(a)), bits_of(&fill(b)));
            a.iter().zip(&b).map(|(x, y)| (x ^ y).count_ones()).sum::<u32>()
        };
        let nearest = |derived: &Pattern| taught.iter().min_by_key(|t| distance(t, derived)).copied();
        let mut gone = 0;
        for (d, list) in &mut self.derived {
            if *d == digit {
                let before = list.len();
                list.retain(|e| nearest(&e.pattern) != Some(*pattern));
                gone += before - list.len();
            }
        }
        for (d, list) in &mut self.taught {
            if *d == digit {
                let before = list.len();
                list.retain(|e| e.pattern != *pattern);
                gone += before - list.len();
            }
        }
        gone
    }

    fn to_value(&self) -> Value {
        let mut fields: Vec<(String, Value)> = vec![
            ("format".into(), Value::int(1)),
            ("raster".into(), Value::Array(vec![Value::int(NORM_W as i64), Value::int(NORM_H as i64)])),
        ];
        let pattern_value = |p: &Pattern| Value::Array(p.iter().map(|&v| Value::int(i64::from(v))).collect());
        for kind in [Kind::Taught, Kind::Derived] {
            let lists = self.lists(kind);
            let patterns = lists.iter().map(|(d, list)| (d.to_string(), Value::Array(list.iter().map(|e| pattern_value(&e.pattern)).collect())));
            fields.push((kind.fields().0.into(), Value::Object(patterns.collect())));
        }
        for kind in [Kind::Taught, Kind::Derived] {
            let lists = self.lists(kind);
            let names = lists.iter().map(|(d, list)| {
                let each = list.iter().map(|e| Value::Array(e.sources.iter().map(Value::str).collect()));
                (d.to_string(), Value::Array(each.collect()))
            });
            fields.push((kind.fields().1.into(), Value::Object(names.collect())));
        }
        Value::Object(fields)
    }

    /// Writes the file to `path`, the folder made if needed.
    pub fn save(&self, path: &str) -> Result<(), FileError> {
        if let Some(folder) = std::path::Path::new(path).parent() {
            let _ = jsonio::make_dirs(&folder.to_string_lossy());
        }
        jsonio::write_text_atomic(path, &jsonio::dumps(&self.to_value(), None, false), Newline::Keep)
            .map_err(|_| FileError::Write)
    }
}

/// Changes every file there is of `files`, writing those that changed. Gives how many patterns went.
fn change_all(files: &OwnFiles, mut change: impl FnMut(&mut OwnFile) -> usize) -> Result<usize, FileError> {
    let mut gone = 0;
    for (_, path) in files.all() {
        if !std::path::Path::new(path).exists() {
            continue;
        }
        let mut file = OwnFile::open(path)?;
        let before = file.clone();
        gone += change(&mut file);
        // ⚠ Also when no pattern went: an image taken out of the names of a pattern two images gave.
        if file != before {
            file.save(path)?;
        }
    }
    Ok(gone)
}

/// One digit pattern the player taught, as the page shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct TaughtDigit {
    pub digit: char,
    pub pattern: Pattern,
    /// The images it came from, over all files.
    pub sources: Vec<String>,
}

/// The taught patterns of every file, by digit from 0 to 9; a pattern in several files once.
///
/// ⚠ No mark for a pattern that looks unlike the shipped ones: measured on the files of the report of 22.11.2026,
/// such a mark fell on real digits of the small type of the Golem (14 of 132) and on almost none taught from a speed,
/// a distance or a heading (1 of 9). Which picture a digit came from tells the player more.
pub fn taught_digits(files: &OwnFiles) -> Vec<TaughtDigit> {
    let mut out: Vec<TaughtDigit> = Vec::new();
    for (_, path) in files.all() {
        let Ok(file) = OwnFile::open(path) else { continue };
        for (digit, pattern, sources) in file.taught() {
            if let Some(known) = out.iter_mut().find(|t| t.digit == digit && t.pattern == pattern) {
                for name in sources {
                    if !known.sources.contains(&name) {
                        known.sources.push(name);
                    }
                }
                continue;
            }
            out.push(TaughtDigit { digit, pattern, sources });
        }
    }
    out.sort_by_key(|t| t.digit);
    out
}

/// Forgets the taught `pattern` of `digit` in every file, with the derived patterns that came nearest from it.
/// Gives how many patterns went.
pub fn forget_digit(files: &OwnFiles, digit: char, pattern: &Pattern) -> Result<usize, FileError> {
    change_all(files, |file| file.drop_taught(digit, pattern))
}

/// The number a kept image was taught as, from its name (`17080_…png` is "17080").
pub fn sample_number(name: &str) -> String {
    name.split('_').next().unwrap_or_default().chars().filter(char::is_ascii_digit).collect()
}

/// Forgets the kept image `name` in `folder`: every pattern only it gave, the patterns naming no image that the
/// image gives taught again, and the image itself. Gives how many patterns went.
pub fn forget_sample(files: &OwnFiles, folder: &str, name: &str) -> Result<usize, FileError> {
    let path = format!("{}/{name}", folder.trim_end_matches(['/', '\\']));
    let again: Vec<(Kind, char, Pattern)> = fs::read(&path)
        .ok()
        .and_then(|bytes| gray_of_png(&bytes))
        .and_then(|raster| {
            let lesson = lesson(&raster, &sample_number(name), false).ok()?;
            let (taught, _) = lesson_patterns(&raster, &lesson);
            Some(
                taught
                    .into_iter()
                    .flat_map(|(_, digit, pattern, derived)| {
                        std::iter::once((Kind::Taught, digit, pattern))
                            .chain(derived.into_iter().map(move |p| (Kind::Derived, digit, p)))
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();
    let gone = change_all(files, |file| file.drop_source(name) + file.drop_unnamed(&again))?;
    if std::path::Path::new(&path).exists() {
        jsonio::remove_file(&path).map_err(|_| FileError::Write)?;
    }
    Ok(gone)
}

/// Why a pattern is no good as a taught digit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Wider than a digit is: two characters grown together, or a block.
    TooWide,
    /// Two parts that do not touch: a digit with a comma or a speck beside it.
    TwoParts,
    /// Nearer to the shipped templates of another digit than to those of its own.
    OtherDigit,
}

/// A part under this many points is a speck, not a part.
const MIN_PART: usize = 3;
/// The widest a digit may be against its height (the ink of a pattern, aspect kept).
const MAX_ASPECT: f64 = 0.9;
/// A pattern counts as another digit's when a shipped template of another digit lies within the first distance of it
/// and those of its own digit are at least the second away. Measured on 500 real digits: the nearest other digit of
/// a real one never lies under 0.12, a shipped five taught as a four lies at 0 (own 0.52).
/// ⚠ Teaching is stricter than showing: a half-covered, hollow one of a test picture lies at 0.04 from the five
/// (own 0.42) and is still a one the player taught right; marked as suspect it is only looked at.
const OTHER_NEAR: f64 = 0.08;
const OWN_FAR: f64 = 0.30;
const OTHER_NEAR_TEACHING: f64 = 0.05;
const OWN_FAR_TEACHING: f64 = 0.48;
/// The mean difference (of 255) under which two images count as the same.
const NEAR_SAME: f64 = 6.0;

/// Where the ink of a pattern lies: the width and height of its box and the number of parts (8-connected, a speck
/// does not count as a part).
fn ink_shape(pattern: &Pattern) -> (usize, usize, usize) {
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for (at, &v) in pattern.iter().enumerate() {
        if v != 0 {
            let (x, y) = (at % NORM_W, at / NORM_W);
            left = left.min(x);
            right = right.max(x);
            top = top.min(y);
            bottom = bottom.max(y);
        }
    }
    if left == usize::MAX {
        return (0, 0, 0);
    }
    let mut seen = [false; NORM_W * NORM_H];
    let mut parts = 0;
    for start in 0..pattern.len() {
        if pattern[start] == 0 || seen[start] {
            continue;
        }
        let (mut stack, mut size) = (vec![start], 0usize);
        seen[start] = true;
        while let Some(at) = stack.pop() {
            size += 1;
            let (x, y) = ((at % NORM_W) as i32, (at / NORM_W) as i32);
            for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= NORM_W as i32 || ny >= NORM_H as i32 {
                    continue;
                }
                let next = ny as usize * NORM_W + nx as usize;
                if pattern[next] != 0 && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        if size >= MIN_PART {
            parts += 1;
        }
    }
    (right - left + 1, bottom - top + 1, parts)
}

/// The width, height and number of parts of the ink of a pattern.
pub fn shape(pattern: &Pattern) -> (usize, usize, usize) {
    ink_shape(pattern)
}

/// What is wrong with the pattern as the digit `digit`, judged against the `shipped` templates; `None` when it is
/// usable. A pattern that is too wide, in two parts, or nearer another digit than its own is not taught, and one
/// already taught is shown as suspect.
pub fn problem(digit: char, pattern: &Pattern, shipped: &Templates, teaching: bool) -> Option<Problem> {
    let (w, h, parts) = ink_shape(pattern);
    if h == 0 {
        return Some(Problem::TwoParts);
    }
    if w as f64 > MAX_ASPECT * h as f64 {
        return Some(Problem::TooWide);
    }
    if parts >= 2 {
        return Some(Problem::TwoParts);
    }
    let column = digit_table(std::slice::from_ref(pattern), shipped).into_iter().next()?;
    let own = column_get(&column, digit)?;
    let other = column.iter().filter(|(d, _)| *d != digit).map(|(_, x)| *x).fold(f64::INFINITY, f64::min);
    let (near, far) = if teaching { (OTHER_NEAR_TEACHING, OWN_FAR_TEACHING) } else { (OTHER_NEAR, OWN_FAR) };
    (other <= near && own >= far).then_some(Problem::OtherDigit)
}

/// Forgets every image but the newest of each number: the images are told by the number they were taught as, and
/// every pattern only a removed image gave goes with it. Gives how many patterns and how many images went.
pub fn forget_duplicates(files: &OwnFiles, folder: &str) -> Result<(usize, usize), FileError> {
    let names = samples(folder);
    let mut newest: Vec<(String, &String)> = Vec::new();
    for name in names.iter().rev() {
        let number = sample_number(name);
        if !newest.iter().any(|(n, _)| *n == number) {
            newest.push((number, name));
        }
    }
    let (mut patterns, mut images) = (0, 0);
    for name in &names {
        if newest.iter().any(|(_, keep)| *keep == name) {
            continue;
        }
        patterns += forget_sample(files, folder, name)?;
        images += 1;
    }
    Ok((patterns, images))
}

/// Whether two images are nearly the same picture: the same size and, over all points, hardly any different.
pub fn near_same(a: &Gray, b: &Gray) -> bool {
    if a.w != b.w || a.h != b.h || a.data.is_empty() {
        return false;
    }
    let total: u64 = a.data.iter().zip(&b.data).map(|(x, y)| u64::from(x.abs_diff(*y))).sum();
    (total as f64 / a.data.len() as f64) <= NEAR_SAME
}

/// The kept image of `number` in `folder` that `raster` nearly equals, if there is one.
pub fn duplicate_of(folder: &str, number: &str, raster: &Gray) -> Option<String> {
    samples(folder).into_iter().filter(|name| sample_number(name) == number).find(|name| {
        let path = format!("{}/{name}", folder.trim_end_matches(['/', '\\']));
        fs::read(&path).ok().and_then(|bytes| gray_of_png(&bytes)).is_some_and(|kept| near_same(&kept, raster))
    })
}

/// Forgets every digit taught: the three files go, the shipped templates are left. The kept images stay.
pub fn forget_all(files: &OwnFiles) -> Result<(), FileError> {
    for (_, path) in files.all() {
        if std::path::Path::new(path).exists() {
            jsonio::remove_file(path).map_err(|_| FileError::Write)?;
        }
    }
    Ok(())
}

/// A kept image (a grey PNG as [`png_bytes`] writes it) as grey image; any other PNG gives `None`.
pub fn gray_of_png(bytes: &[u8]) -> Option<Gray> {
    let body = bytes.strip_prefix(b"\x89PNG\r\n\x1a\n")?;
    let (mut at, mut header, mut data) = (0usize, None, Vec::new());
    while at + 8 <= body.len() {
        let length = u32::from_be_bytes(body[at..at + 4].try_into().ok()?) as usize;
        let kind = &body[at + 4..at + 8];
        let content = body.get(at + 8..at + 8 + length)?;
        match kind {
            b"IHDR" => header = Some(content.to_vec()),
            b"IDAT" => data.extend_from_slice(content),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + length;
    }
    let header = header?;
    let w = u32::from_be_bytes(header.get(0..4)?.try_into().ok()?) as usize;
    let h = u32::from_be_bytes(header.get(4..8)?.try_into().ok()?) as usize;
    // 8 bits of grey, no interlace.
    if header.get(8..13)? != [8, 0, 0, 0, 0] || w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&data).ok()?;
    if raw.len() < (w + 1) * h {
        return None;
    }
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        let line = &raw[y * (w + 1)..(y + 1) * (w + 1)];
        let (filter, line) = (line[0], &line[1..]);
        for x in 0..w {
            let left = if x > 0 { out[y * w + x - 1] } else { 0 };
            let up = if y > 0 { out[(y - 1) * w + x] } else { 0 };
            let corner = if x > 0 && y > 0 { out[(y - 1) * w + x - 1] } else { 0 };
            let guess = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => paeth(left, up, corner),
                _ => return None,
            };
            out[y * w + x] = line[x].wrapping_add(guess);
        }
    }
    Some(Gray::new(w, h, out))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = ((p - i16::from(a)).abs(), (p - i16::from(b)).abs(), (p - i16::from(c)).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}
