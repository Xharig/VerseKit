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

//! Measures the reading of the refinery terminal against a set of pictures with known lines. Every folder of the
//! set holds the pictures (png, jpg) of one job; `golden-expected.json` in the root names per folder the expected
//! lines (`name|quality|quantity|yield|selected`, `null` for an empty cell) and optionally the station
//! (`location`). Each picture is read as the key in the game reads the screen ([`read_image`]); of several job
//! cards on one picture the one with the most lines right counts, as the player picks the card to take. The pictures of a folder are joined as the refinery field joins
//! them ([`join_scrolled`]); a picture that shares no line with the field is added below it, as when the first
//! job is booked before the next is read. The lines read are aligned with the expected ones in order, and the
//! test prints per folder and in total how many lines and fields are right, and every difference.
//!
//! The reading gives material, quality and yield, not the quantity put in and not the switch to refine.
//!
//! Run with `VERSEKIT_RAFFINERIE_PRUEFSATZ=<folder> cargo test -p versekit-core --release --test
//! refinery_test_set -- --ignored --nocapture`. `VERSEKIT_KOPIE=<copy of real data>` takes the materials, places
//! and learned digits of that copy (read only) and measures a second time with the learned digits; without it a
//! list of the refinable ores and no places. `VERSEKIT_RAFFINERIE_AUFTRAG=<folder name>` measures one folder only;
//! `VERSEKIT_RAFFINERIE_DEBUG=1` also prints every reading of every table with the columns taken from its heads.
//! After the totals the run counts the errors by kind (a field left empty, 0 and 8 swapped …) with the lines of
//! each. `VERSEKIT_RAFFINERIE_MESSUNG=<file>` writes the numbers of the measurements as JSON (never beside the set),
//! and `VERSEKIT_RAFFINERIE_BASIS=<such a file>` fails the run when a measurement of the same name falls below it:
//! lines right more than half a point lower, or more fields read wrong.
//!
//! The recognition: `VERSEKIT_RAFFINERIE_LESER=windows` (the default under Windows) or `eigen` (the own one,
//! [`own_ocr`], the default elsewhere) with the shipped font or the one in `VERSEKIT_SCHRIFT=<file>`.
//! `VERSEKIT_RAFFINERIE_TEIL=gerade|ungerade` measures the folders with an even or odd number only (the other
//! half taught that font, `refinery_font.rs`). Beside the lines right the run counts the fields read wrong: a
//! value that stands but is not the one expected (an empty field is unsure, not wrong).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::Value;
use versekit_core::paths::{Host, Paths};
use versekit_core::refinery_scan::digits::{self, Cell, Digits, Typed};
use versekit_core::refinery_scan::own_ocr::{self, Font};
use versekit_core::refinery_scan::{
    Index, Job, Ocr, PASSES, ScanError, Word, find_tables, header_columns, join_scrolled, match_material, merge_words,
    read_image, rows, screen, search_jobs,
};
use versekit_core::win_ocr::Bitmap;

const SET: &str = "VERSEKIT_RAFFINERIE_PRUEFSATZ";
const COPY: &str = "VERSEKIT_KOPIE";
const ONLY: &str = "VERSEKIT_RAFFINERIE_AUFTRAG";
const DEBUG: &str = "VERSEKIT_RAFFINERIE_DEBUG";
const READER: &str = "VERSEKIT_RAFFINERIE_LESER";
const FONT: &str = "VERSEKIT_SCHRIFT";
const PART: &str = "VERSEKIT_RAFFINERIE_TEIL";
/// Where the numbers of the measurement are written (JSON), and the file of an earlier measurement to hold it to.
const RESULT: &str = "VERSEKIT_RAFFINERIE_MESSUNG";
const BASE: &str = "VERSEKIT_RAFFINERIE_BASIS";
/// `kreuz`: measures each half of the set with the digits learned from the other half. `..._SCHREIBEN=<file>`
/// writes the digits learned from the whole set as the file the program ships.
const DIGITS: &str = "VERSEKIT_RAFFINERIE_ZIFFERN";
const DIGITS_WRITE: &str = "VERSEKIT_RAFFINERIE_ZIFFERN_SCHREIBEN";
const GOLDEN: &str = "golden-expected.json";

/// The recognition a measurement reads with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reader {
    Windows,
    Own,
}

impl Reader {
    fn chosen() -> Reader {
        match std::env::var(READER).as_deref() {
            Ok("eigen") => Reader::Own,
            Ok("windows") => Reader::Windows,
            _ if cfg!(windows) => Reader::Windows,
            _ => Reader::Own,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Reader::Windows => "Erkennung von Windows",
            Reader::Own => "eigene Erkennung",
        }
    }

    /// The recognition over one picture.
    fn over<'a>(self, bitmap: &'a Bitmap, font: &'a Font) -> Box<dyn Ocr + 'a> {
        match self {
            #[cfg(windows)]
            Reader::Windows => Box::new(versekit_core::win_ocr::Recognition::new(bitmap)),
            #[cfg(not(windows))]
            Reader::Windows => panic!("the recognition of Windows only under Windows"),
            Reader::Own => Box::new(own_ocr::Recognition { bitmap, font }),
        }
    }
}

/// The ores of the refinery terminal, for a run without a copy of real data.
const ORES: [&str; 24] = [
    "Agricium",
    "Aluminum",
    "Aslarite",
    "Beryl",
    "Bexalite",
    "Borase",
    "Copper",
    "Corundum",
    "Gold",
    "Hephaestanite",
    "Iron",
    "Laranite",
    "Lindinium",
    "Ouratite",
    "Quantainium",
    "Quartz",
    "Riccite",
    "Savrilium",
    "Stileron",
    "Taranite",
    "Tin",
    "Titanium",
    "Torite",
    "Tungsten",
];

/// The answers of the recognition per picture, by the jobs asked for.
type Cache = HashMap<PathBuf, HashMap<String, Vec<Vec<Word>>>>;

/// The recognition of one picture that keeps every answer, so a second measurement reads nothing anew.
struct Remembered<'a> {
    inner: Box<dyn Ocr + 'a>,
    known: HashMap<String, Vec<Vec<Word>>>,
}

impl Ocr for Remembered<'_> {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        let key = format!("{jobs:?}");
        if let Some(words) = self.known.get(&key) {
            return Ok(words.clone());
        }
        let words = self.inner.read(jobs)?;
        self.known.insert(key, words.clone());
        Ok(words)
    }
}

/// A png or jpg as BGRA, top row first.
fn load(path: &Path) -> Bitmap {
    let picture = image::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).to_rgba8();
    let (width, height) = picture.dimensions();
    let mut raw = picture.into_raw();
    for pixel in raw.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Bitmap { width: i64::from(width), height: i64::from(height), raw }
}

/// Capitals and digits only: the form two names are compared in.
fn key(text: &str) -> String {
    text.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_uppercase()).collect()
}

/// One expected line.
#[derive(Clone, Debug)]
struct Expected {
    name: String,
    material: String,
    quality: Option<i64>,
    quantity: Option<i64>,
    yield_: Option<i64>,
    selected: bool,
}

impl Expected {
    /// `name|quality|quantity|yield|selected`; the material is the name as the reading's own matching makes it,
    /// square brackets taken as round ones.
    fn parse(line: &str, index: &Index) -> Option<Expected> {
        let parts: Vec<&str> = line.split('|').collect();
        let [name, quality, quantity, yield_, selected] = parts.as_slice() else { return None };
        let cell = |s: &str| s.trim().parse::<i64>().ok();
        let round = name.replace('[', "(").replace(']', ")");
        Some(Expected {
            name: name.to_string(),
            material: match_material(&round, index).unwrap_or_else(|| name.to_string()),
            quality: cell(quality),
            quantity: cell(quantity),
            yield_: cell(yield_),
            selected: selected.trim() == "true",
        })
    }

    /// A line the reading is made for: refined, with a yield above zero.
    fn bookable(&self) -> bool {
        self.selected && self.yield_.is_some_and(|y| y > 0)
    }

    fn show(&self) -> String {
        let cell = |v: Option<i64>| v.map_or_else(|| "-".to_string(), |v| v.to_string());
        format!(
            "{} [{}] q {} menge {} ausbeute {}",
            self.name,
            self.material,
            cell(self.quality),
            cell(self.quantity),
            cell(self.yield_)
        )
    }
}

/// One line read, from the text of the refinery field (`Material Q Menge cSCU`, `?` for an empty cell).
#[derive(Clone, Debug, PartialEq)]
struct Read {
    material: String,
    quality: Option<i64>,
    amount: Option<i64>,
}

impl Read {
    fn parse(line: &str) -> Option<Read> {
        let line = line.trim();
        let line = line.strip_suffix(" cSCU").unwrap_or(line);
        let parts: Vec<&str> = line.rsplitn(3, ' ').collect();
        let [amount, quality, material] = parts.as_slice() else { return None };
        Some(Read { material: material.to_string(), quality: quality.parse().ok(), amount: amount.parse().ok() })
    }

    fn show(&self) -> String {
        let cell = |v: Option<i64>| v.map_or_else(|| "?".to_string(), |v| v.to_string());
        format!("{} q {} ausbeute {}", self.material, cell(self.quality), cell(self.amount))
    }
}

/// How well an expected and a read line fit, `None` when they share neither material nor quality.
fn fit(expected: &Expected, read: &Read) -> Option<i32> {
    let material = key(&expected.material) == key(&read.material);
    let quality = expected.quality.is_some() && expected.quality == read.quality;
    if !material && !quality {
        return None;
    }
    Some(4 * i32::from(material) + 2 * i32::from(quality) + i32::from(expected.yield_ == read.amount))
}

/// Expected and read lines paired without regard to their order: the pairs of the best [`fit`] first, on the same
/// fit the pair of the nearer places in the two lists; every line is paired at most once.
fn align(expected: &[Expected], read: &[Read]) -> Vec<(usize, usize)> {
    let mut candidates: Vec<(i32, usize, usize, usize)> = Vec::new();
    for (i, line) in expected.iter().enumerate() {
        for (j, got) in read.iter().enumerate() {
            if let Some(score) = fit(line, got) {
                candidates.push((score, i.abs_diff(j), i, j));
            }
        }
    }
    candidates.sort_by_key(|&(score, distance, i, j)| (std::cmp::Reverse(score), distance, i, j));
    let (mut used_expected, mut used_read) = (vec![false; expected.len()], vec![false; read.len()]);
    let mut pairs = Vec::new();
    for (_, _, i, j) in candidates {
        if !used_expected[i] && !used_read[j] {
            used_expected[i] = true;
            used_read[j] = true;
            pairs.push((i, j));
        }
    }
    pairs.sort_unstable();
    pairs
}

/// Counts of a comparison.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Tally {
    expected: usize,
    whole: usize,
    material: usize,
    quality: usize,
    yield_: usize,
    missing: usize,
    extra: usize,
    /// Fields that stand but are wrong: another material, a quality or a yield other than the expected.
    wrong: usize,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        self.expected += other.expected;
        self.whole += other.whole;
        self.material += other.material;
        self.quality += other.quality;
        self.yield_ += other.yield_;
        self.missing += other.missing;
        self.extra += other.extra;
        self.wrong += other.wrong;
    }
}

fn share(part: usize, all: usize) -> String {
    if all == 0 { "-".to_string() } else { format!("{:.1} %", 100.0 * part as f64 / all as f64) }
}

/// The kinds of error a measurement counts, in the order they are printed.
const CLASSES: [&str; 11] = [
    "Zeile fehlt",
    "Material falsch",
    "Qualitaet leer, Wert mit 0 oder 8",
    "Qualitaet leer, Wert ohne 0 und 8",
    "Qualitaet 0 und 8 vertauscht",
    "Qualitaet anders falsch",
    "Ausbeute leer, Wert mit 0 oder 8",
    "Ausbeute leer, Wert ohne 0 und 8",
    "Ausbeute 0 und 8 vertauscht",
    "Ausbeute anders falsch",
    "Wert gelesen, Zelle sollte leer sein",
];

/// The kind of error of one field: `None` when it is right.
fn field_class(name: &str, expected: Option<i64>, got: Option<i64>) -> Option<&'static str> {
    let [with_confusable, without, swapped, other] = if name == "Qualitaet" {
        [CLASSES[2], CLASSES[3], CLASSES[4], CLASSES[5]]
    } else {
        [CLASSES[6], CLASSES[7], CLASSES[8], CLASSES[9]]
    };
    match (expected, got) {
        (e, g) if e == g => None,
        (None, Some(_)) => Some(CLASSES[10]),
        (Some(e), None) => {
            Some(if e.to_string().chars().any(|c| c == '0' || c == '8') { with_confusable } else { without })
        }
        (Some(e), Some(g)) => {
            let (e, g) = (e.to_string(), g.to_string());
            let alike = |a: &str, b: &str| a.len() == b.len() && a.replace('8', "0") == b.replace('8', "0");
            Some(if alike(&e, &g) { swapped } else { other })
        }
        (None, None) => None,
    }
}

/// The comparison of one folder: the tally of all lines, of the bookable lines, and the differences as text.
fn compare(expected: &[Expected], read: &[Read]) -> (Tally, Tally, Vec<String>) {
    let (all, bookable, notes, _) = compare_classed(expected, read);
    (all, bookable, notes)
}

/// [`compare`] and the kinds of error, each with the line it was found in.
fn compare_classed(expected: &[Expected], read: &[Read]) -> (Tally, Tally, Vec<String>, Vec<(&'static str, String)>) {
    let pairs = align(expected, read);
    let (mut all, mut bookable, mut notes) = (Tally::default(), Tally::default(), Vec::new());
    let mut classes: Vec<(&'static str, String)> = Vec::new();
    let mut paired_read = vec![false; read.len()];
    for (i, line) in expected.iter().enumerate() {
        let mut tally = Tally { expected: 1, ..Tally::default() };
        match pairs.iter().find(|(e, _)| *e == i) {
            Some(&(_, j)) => {
                paired_read[j] = true;
                let got = &read[j];
                let material = key(&line.material) == key(&got.material);
                let quality = line.quality == got.quality;
                let yield_ = line.yield_ == got.amount;
                tally.material = usize::from(material);
                tally.quality = usize::from(quality);
                tally.yield_ = usize::from(yield_);
                tally.whole = usize::from(material && quality && yield_);
                tally.wrong = usize::from(!material)
                    + usize::from(got.quality.is_some() && !quality)
                    + usize::from(got.amount.is_some() && !yield_);
                if !material {
                    classes.push((CLASSES[1], line.show()));
                }
                for (name, wanted, found) in
                    [("Qualitaet", line.quality, got.quality), ("Ausbeute", line.yield_, got.amount)]
                {
                    if let Some(class) = field_class(name, wanted, found) {
                        classes.push((class, format!("{} | gelesen {}", line.show(), got.show())));
                    }
                }
                if tally.whole == 0 {
                    let mut wrong = Vec::new();
                    if !material {
                        wrong.push("Material");
                    }
                    if !quality {
                        wrong.push("Qualitaet");
                    }
                    if !yield_ {
                        wrong.push("Ausbeute");
                    }
                    notes.push(format!(
                        "  falsch ({}): erwartet {} | gelesen {}",
                        wrong.join(", "),
                        line.show(),
                        got.show()
                    ));
                }
            }
            None => {
                tally.missing = 1;
                notes.push(format!("  fehlt: {}", line.show()));
                classes.push((CLASSES[0], line.show()));
            }
        }
        all.add(&tally);
        if line.bookable() {
            bookable.add(&tally);
        }
    }
    for (j, got) in read.iter().enumerate() {
        if !paired_read[j] {
            all.extra += 1;
            notes.push(format!("  zusaetzlich: {}", got.show()));
        }
    }
    (all, bookable, notes, classes)
}

/// One folder of the set.
#[derive(Clone)]
struct Folder {
    name: String,
    pictures: Vec<PathBuf>,
    expected: Vec<Expected>,
    location: Option<String>,
}

fn folders(set: &Path, index: &Index) -> Vec<Folder> {
    let golden: Value =
        serde_json::from_slice(&std::fs::read(set.join(GOLDEN)).expect("golden-expected.json")).expect("JSON");
    let only = std::env::var(ONLY).ok();
    let mut out = Vec::new();
    for (name, entry) in golden.as_object().expect("an object of folders") {
        if only.as_ref().is_some_and(|o| o != name) {
            continue;
        }
        let mut pictures: Vec<PathBuf> =
            std::fs::read_dir(set.join(name)).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        pictures.retain(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["png", "jpg", "jpeg"].contains(&e.to_lowercase().as_str()))
        });
        pictures.sort();
        let expected = entry["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|line| Expected::parse(line, index))
            .collect();
        let location = entry["location"].as_str().map(str::to_string);
        out.push(Folder { name: name.clone(), pictures, expected, location });
    }
    let number = |name: &str| name.chars().filter(char::is_ascii_digit).collect::<String>().parse::<u32>().unwrap_or(0);
    out.sort_by_key(|f| number(&f.name));
    out
}

/// What the pictures of a folder gave: the field after all pictures, the station, and a line per picture.
struct Reading {
    field: String,
    station: String,
    pictures: Vec<String>,
    /// The cells of the unsure rows of the cards taken, for learning digits.
    cells: Vec<Cell>,
}

/// What a measurement reads with: the recognition and, for the own one, the font.
#[derive(Clone, Copy)]
struct With<'a> {
    reader: Reader,
    font: &'a Font,
}

fn read_folder(folder: &Folder, index: &Index, learned: &Digits, cache: &mut Cache, with: With) -> Reading {
    let mut reading = Reading { field: String::new(), station: String::new(), pictures: Vec::new(), cells: Vec::new() };
    for path in &folder.pictures {
        let bitmap = load(path);
        let started = Instant::now();
        let mut ocr = Remembered { inner: with.reader.over(&bitmap, with.font), known: cache.remove(path).unwrap_or_default() };
        let read = read_image((bitmap.width, bitmap.height), Some(&bitmap), &mut ocr, index, learned, None);
        let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if read.as_ref().is_ok_and(|(jobs, _)| jobs.is_empty()) {
            // The rows of the search that hold a head of the table, as the search over the whole picture read them.
            let search = ocr.read(&search_jobs(bitmap.width, bitmap.height)).unwrap_or_default();
            for row in rows(&merge_words(&search)) {
                let line: Vec<String> =
                    row.iter().map(|w| format!("{}@{:.0},{:.0}h{:.0}", w.t, w.x, w.y, w.h)).collect();
                let text = line.join(" ").to_uppercase();
                if ["QUAL", "UALI", "YIELD", "QTY", "MATERIA"].iter().any(|k| text.contains(k)) {
                    reading.pictures.push(format!("    {file} Kopfzeile gelesen: {}", line.join("  ")));
                }
            }
        }
        if std::env::var_os(DEBUG).is_some() {
            // Per table and reading: the columns taken from the heads, and every row with the middles of its words.
            let search = merge_words(&ocr.read(&search_jobs(bitmap.width, bitmap.height)).unwrap_or_default());
            for table in find_tables(&search, bitmap.width, bitmap.height) {
                reading.pictures.push(format!("    {file} Tabelle {:?}", table.area));
                let jobs: Vec<Job> =
                    PASSES.iter().map(|(scale, mode)| Job { area: table.area, scale: *scale, mode: *mode }).collect();
                for (n, pass) in ocr.read(&jobs).unwrap_or_default().iter().enumerate() {
                    let table_rows = rows(pass);
                    reading.pictures.push(format!("      Lesung {n}: Spalten {:?}", header_columns(&table_rows)));
                    for row in table_rows {
                        let line: Vec<String> = row.iter().map(|w| format!("{}@{:.0}", w.t, w.x + w.w / 2.0)).collect();
                        reading.pictures.push(format!("        {}", line.join(" ")));
                    }
                }
            }
        }
        cache.insert(path.clone(), ocr.known);
        let (jobs, _) = match read {
            Ok(read) => read,
            Err(e) => {
                reading.pictures.push(format!("  {file}: Fehler {e}"));
                continue;
            }
        };
        // Of several cards the one the player would take: the most lines right, then the most lines.
        let best = jobs.iter().rev().max_by_key(|j| {
            let read: Vec<Read> = j.text.lines().filter_map(Read::parse).collect();
            (compare(&folder.expected, &read).0.whole, read.len())
        });
        let text = best.map(|j| j.text.clone()).unwrap_or_default();
        if let Some(job) = best {
            reading.cells.extend(job.cells.iter().cloned());
            if reading.station.is_empty() {
                reading.station = job.station.clone();
            }
        }
        reading.pictures.push(format!(
            "  {file}: {}x{} {:.1} s, {} Tabelle(n), {} Zeilen, unsicher {}, Station {:?}, Summe {:?}",
            bitmap.width,
            bitmap.height,
            started.elapsed().as_secs_f64(),
            jobs.len(),
            text.lines().count(),
            best.map_or(0, |j| j.unsure.len()),
            best.map_or("", |j| j.station.as_str()),
            best.map_or("", |j| j.total.as_str()),
        ));
        if text.is_empty() {
            continue;
        }
        reading.field = match join_scrolled(&reading.field, &text) {
            Some((all, _)) => all,
            None if reading.field.is_empty() => text,
            None => format!("{}\n{text}", reading.field),
        };
    }
    reading
}

/// What one measurement came to, for the file of the measurement and the comparison with a base.
#[derive(Clone, Debug, PartialEq)]
struct Outcome {
    label: String,
    all: Tally,
    bookable: Tally,
    /// Each kind of error with how often it was found.
    classes: Vec<(&'static str, usize)>,
}

impl Outcome {
    fn to_json(&self) -> Value {
        let tally = |t: &Tally| {
            serde_json::json!({
                "erwartet": t.expected, "ganz_richtig": t.whole, "falsch_gelesen": t.wrong,
                "fehlend": t.missing, "zusaetzlich": t.extra,
            })
        };
        let classes: serde_json::Map<String, Value> =
            self.classes.iter().map(|(name, n)| ((*name).to_string(), Value::from(*n))).collect();
        serde_json::json!({ "bezeichnung": self.label, "alle": tally(&self.all), "buchbar": tally(&self.bookable), "fehlerklassen": classes })
    }
}

fn print_tally(name: &str, t: &Tally) {
    println!(
        "{name}: {} Zeilen, ganz richtig {} ({}), Material {} ({}), Qualitaet {} ({}), Ausbeute {} ({}), fehlend {}, zusaetzlich {}, FALSCH GELESEN {} Felder, Menge nicht gelesen",
        t.expected,
        t.whole,
        share(t.whole, t.expected),
        t.material,
        share(t.material, t.expected),
        t.quality,
        share(t.quality, t.expected),
        t.yield_,
        share(t.yield_, t.expected),
        t.missing,
        t.extra,
        t.wrong,
    );
}

impl Outcome {
    /// The measurements of two parts of the set as one.
    fn merged(label: &str, parts: &[Outcome]) -> Outcome {
        let (mut all, mut bookable) = (Tally::default(), Tally::default());
        let mut classes: Vec<(&'static str, usize)> = CLASSES.iter().map(|name| (*name, 0)).collect();
        for part in parts {
            all.add(&part.all);
            bookable.add(&part.bookable);
            for (name, n) in &part.classes {
                if let Some((_, sum)) = classes.iter_mut().find(|(known, _)| known == name) {
                    *sum += n;
                }
            }
        }
        Outcome { label: label.to_string(), all, bookable, classes }
    }
}

/// The share of whole lines in percent points, 0 without lines.
fn percent(part: usize, all: usize) -> f64 {
    if all == 0 { 0.0 } else { 100.0 * part as f64 / all as f64 }
}

/// How far a measurement may fall below its base before the run fails, in points of the lines right.
const BASE_SLACK_POINTS: f64 = 0.5;

/// What is wrong with a measurement against its base, or nothing: the lines right fell by more than
/// [`BASE_SLACK_POINTS`] points, or more fields were read wrong. `base` is the entry of the same label.
fn regression(now: &Outcome, base: &Value) -> Vec<String> {
    let number = |value: &Value, part: &str, key: &str| value[part][key].as_u64().unwrap_or(0) as usize;
    let mut found = Vec::new();
    for (part, tally) in [("alle", &now.all), ("buchbar", &now.bookable)] {
        let (base_whole, base_all) = (number(base, part, "ganz_richtig"), number(base, part, "erwartet"));
        let (before, after) = (percent(base_whole, base_all), percent(tally.whole, tally.expected));
        if after + BASE_SLACK_POINTS < before {
            found.push(format!("{}: {part}: ganz richtig {after:.1} % statt {before:.1} %", now.label));
        }
        let base_wrong = number(base, part, "falsch_gelesen");
        if tally.wrong > base_wrong {
            found.push(format!("{}: {part}: falsch gelesen {} statt {base_wrong}", now.label, tally.wrong));
        }
    }
    found
}

/// The digits the folders teach: the cells of the unsure rows learn from the typed lines, as with a player who
/// types after a reading.
fn learn_digits(folders: &[Folder], index: &Index, cache: &mut Cache, with: With) -> Digits {
    let mut learned = Digits::default();
    for folder in folders {
        let reading = read_folder(folder, index, &Digits::default(), cache, with);
        let typed: Vec<Typed> = folder
            .expected
            .iter()
            .map(|line| (line.material.clone(), line.quality.map(|v| v as f64), line.yield_.map(|v| v as f64)))
            .collect();
        digits::learn_into(&mut learned, &reading.cells, &typed);
    }
    learned
}

/// One measurement over every folder, printed.
fn measure(
    label: &str,
    folders: &[Folder],
    index: &Index,
    learned: &Digits,
    cache: &mut Cache,
    with: With,
) -> Outcome {
    println!("\n######## {label}, {}", with.reader.name());
    let (mut all, mut bookable) = (Tally::default(), Tally::default());
    let (mut stations, mut stations_right) = (0, 0);
    let mut groups: Vec<(String, Tally)> = Vec::new();
    let mut table = Vec::new();
    let mut details = Vec::new();
    let mut classes: Vec<(&'static str, Vec<String>)> = CLASSES.iter().map(|name| (*name, Vec::new())).collect();
    for folder in folders {
        let reading = read_folder(folder, index, learned, cache, with);
        let read: Vec<Read> = reading.field.lines().filter_map(Read::parse).collect();
        let (tally, booked, notes, kinds) = compare_classed(&folder.expected, &read);
        for (kind, line) in kinds {
            if let Some((_, lines)) = classes.iter_mut().find(|(name, _)| *name == kind) {
                lines.push(format!("{}: {line}", folder.name));
            }
        }
        all.add(&tally);
        bookable.add(&booked);
        let station = match &folder.location {
            Some(location) => {
                stations += 1;
                let right = key(location) == key(&reading.station);
                stations_right += usize::from(right);
                if right { "ja".to_string() } else { format!("nein ({:?})", reading.station) }
            }
            None => "-".to_string(),
        };
        let kind = folder
            .pictures
            .first()
            .and_then(|p| p.extension())
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let height = folder.pictures.first().and_then(|p| image::image_dimensions(p).ok()).map_or(0, |d| d.1);
        let band = match height {
            0..1200 => "Ausschnitt",
            1200..2000 => "1440p",
            _ => "2160p",
        };
        let group = format!("{kind} {band}");
        let shape = format!("{kind} {height}");
        match groups.iter_mut().find(|(g, _)| *g == group) {
            Some((_, t)) => t.add(&tally),
            None => groups.push((group, tally)),
        }
        table.push(format!(
            "| {:<11} | {:>2} | {:<8} | {:>3} | {:>3} | {:>3} | {:>3} | {:>3} | {:>3} | {:>3} | {:>3} | {}",
            folder.name,
            folder.pictures.len(),
            shape,
            tally.expected,
            tally.whole,
            booked.expected,
            booked.whole,
            tally.expected - tally.material,
            tally.expected - tally.quality,
            tally.expected - tally.yield_,
            tally.extra,
            station,
        ));
        details.push(format!("== {} ({} Bilder)", folder.name, folder.pictures.len()));
        details.extend(reading.pictures);
        details.push(format!("  Feld:\n    {}", reading.field.replace('\n', "\n    ")));
        details.extend(notes);
    }
    for line in &details {
        println!("{line}");
    }
    println!("\n| Auftrag     | B  | Format  | erw | ok  | buE | buO | Mat | Qua | Aus | zus | Station");
    for line in &table {
        println!("{line}");
    }
    println!();
    print_tally("ALLE", &all);
    print_tally("BUCHBAR (refine, Ausbeute > 0)", &bookable);
    println!("STATION: {stations_right} von {stations} ({})", share(stations_right, stations));
    for (group, t) in &groups {
        println!("GRUPPE {group}: {} von {} ganz richtig ({})", t.whole, t.expected, share(t.whole, t.expected));
    }
    println!("\nFEHLERKLASSEN (Felder und Zeilen, die nicht stimmen):");
    for (name, lines) in &classes {
        println!("KLASSE {name}: {}", lines.len());
        for line in lines {
            println!("    {line}");
        }
    }
    Outcome {
        label: label.to_string(),
        all,
        bookable,
        classes: classes.iter().map(|(name, lines)| (*name, lines.len())).collect(),
    }
}

#[test]
#[ignore = "reads a set of pictures with known lines"]
fn the_test_set_of_refinery_pictures_is_measured() {
    let Some(set) = std::env::var_os(SET).map(PathBuf::from) else {
        println!("uebersprungen: {SET} nicht gesetzt");
        return;
    };
    versekit_core::writeguard::deny(true);
    versekit_core::writeguard::allow_under(None);
    let copy = std::env::var(COPY).ok().map(|root| Paths::for_copy(Host::current(), &root).expect("the copy"));
    let index = match &copy {
        Some(paths) => screen::index(paths),
        None => Index::new(&ORES.map(String::from), |_| false),
    };
    // Words of the terminal that are no material must not be taken for one, also not as a name cut short.
    let surface = [
        "TOTAL", "COST", "TIME", "STATUS", "DONE", "METHOD", "MATERIALS", "SELECTED", "REFINE", "YIELD", "QUALITY",
        "QTY", "PROCESSING", "REMAINING", "CANCEL", "CONFIRM", "SETUP", "WORK", "ORDER", "INERT", "CURRENT",
        "CAPACITY", "SELECT", "STORAGE", "OPTION", "GESAMTKOSTEN", "VERARBEITUNG", "ZEIT", "ABBRECHEN", "QUALITAET",
        "MENGE", "AUSBEUTE", "MATERIALIEN", "AUSGEWAEHLT", "RAFFINIEREN", "ERTRAG", "FERTIG", "VERBLEIBENDE",
    ];
    let taken: Vec<(&str, String)> =
        surface.iter().filter_map(|word| match_material(word, &index).map(|material| (*word, material))).collect();
    println!("Woerter des Terminals als Material: {taken:?}");
    assert!(taken.is_empty(), "{taken:?}");
    let mut folders = folders(&set, &index);
    let number = |name: &str| name.chars().filter(char::is_ascii_digit).collect::<String>().parse::<u32>().unwrap_or(0);
    match std::env::var(PART).as_deref() {
        Ok("gerade") => folders.retain(|f| number(&f.name) % 2 == 0),
        Ok("ungerade") => folders.retain(|f| number(&f.name) % 2 == 1),
        _ => {}
    }
    assert!(folders.iter().any(|f| !f.pictures.is_empty()), "no pictures");
    let font = match std::env::var_os(FONT) {
        Some(file) => Font::from_bytes(&std::fs::read(&file).expect("the font")).expect("a font"),
        None => Font::shipped().clone(),
    };
    let with = With { reader: Reader::chosen(), font: &font };
    if with.reader == Reader::Own {
        println!("Schrift: {} Bilder", font.glyphs.len());
    }
    let mut cache = HashMap::new();
    let source = if copy.is_some() { "Materialien und Orte der Kopie" } else { "Liste der Erze, keine Orte" };
    let mut outcomes =
        vec![measure(&format!("ohne gelernte Ziffern, {source}"), &folders, &index, &Digits::default(), &mut cache, with)];
    if let Some(paths) = &copy {
        let learned = Digits::load(paths);
        if !learned.is_empty() {
            outcomes.push(measure(
                &format!("mit gelernten Ziffern der Kopie, {source}"),
                &folders,
                &index,
                &learned,
                &mut cache,
                with,
            ));
        }
    }
    // The digits of the font learned from the typed lines of the set, the way a player teaches them. `kreuz`: the
    // folders with an even number teach the digits that read the odd ones and the other way round, so no folder is
    // read with digits learned from itself.
    if std::env::var(DIGITS).as_deref() == Ok("kreuz") {
        let (even, odd): (Vec<Folder>, Vec<Folder>) = folders.iter().cloned().partition(|f| number(&f.name) % 2 == 0);
        let (from_even, from_odd) =
            (learn_digits(&even, &index, &mut cache, with), learn_digits(&odd, &index, &mut cache, with));
        println!("Ziffern der geraden Auftraege: {}, der ungeraden: {}", from_even.count(), from_odd.count());
        let odd_read =
            measure(&format!("ungerade Auftraege mit Ziffern der geraden, {source}"), &odd, &index, &from_even, &mut cache, with);
        let even_read =
            measure(&format!("gerade Auftraege mit Ziffern der ungeraden, {source}"), &even, &index, &from_odd, &mut cache, with);
        let both = Outcome::merged(&format!("Kreuzmessung mit gelernten Ziffern, {source}"), &[odd_read, even_read]);
        println!("\n######## {}", both.label);
        print_tally("KREUZ ALLE", &both.all);
        print_tally("KREUZ BUCHBAR", &both.bookable);
        outcomes.push(both);
    }
    // The digits of the whole set into a file: what is shipped with the program (`daten/raffinerie-ziffern.json`).
    if let Some(file) = std::env::var_os(DIGITS_WRITE) {
        let learned = learn_digits(&folders, &index, &mut cache, with);
        println!("Ziffern gelernt: {} Muster", learned.count());
        let text = versekit_core::jsonio::dumps(&learned.to_value(), None, false);
        std::fs::write(&file, text).expect("the file of the digits");
    }
    // The file of the measurement: only numbers, written where the caller asks, never beside the set.
    if let Some(file) = std::env::var_os(RESULT) {
        let all: Vec<Value> = outcomes.iter().map(Outcome::to_json).collect();
        let text = serde_json::to_string_pretty(&serde_json::json!({ "messungen": all })).expect("JSON");
        std::fs::write(&file, text).expect("the file of the measurement");
    }
    // A base to hold the measurement to: a file of an earlier measurement (`messungen` by the same label).
    if let Some(file) = std::env::var_os(BASE) {
        let base: Value = serde_json::from_slice(&std::fs::read(&file).expect("the base")).expect("JSON");
        let mut problems = Vec::new();
        for now in &outcomes {
            let known = base["messungen"].as_array().into_iter().flatten().find(|m| m["bezeichnung"] == now.label);
            match known {
                Some(known) => problems.extend(regression(now, known)),
                None => println!("Basis: keine Messung \"{}\" in der Basis", now.label),
            }
        }
        assert!(problems.is_empty(), "gegen die Basis schlechter:\n{}", problems.join("\n"));
    }
}

/// An outcome with these numbers, for the tests of the comparison with a base.
fn outcome(whole: usize, wrong: usize) -> Outcome {
    let tally = Tally { expected: 100, whole, wrong, ..Tally::default() };
    Outcome { label: "x".into(), all: tally, bookable: tally, classes: Vec::new() }
}

#[test]
fn a_regression_against_the_base_turns_the_measurement_red() {
    let base = outcome(70, 10).to_json();
    // As good or better, or a hair worse: no complaint.
    assert!(regression(&outcome(70, 10), &base).is_empty());
    assert!(regression(&outcome(80, 5), &base).is_empty());
    assert!(regression(&outcome(70, 10), &outcome(70, 10).to_json()).is_empty());
    // Within the slack of half a point: no regression (shown on 1000 lines, 70.0 % against 69.6 % and 69.0 %).
    let wide = |whole: usize| {
        let tally = Tally { expected: 1000, whole, wrong: 10, ..Tally::default() };
        Outcome { label: "x".into(), all: tally, bookable: tally, classes: Vec::new() }
    };
    assert!(regression(&wide(696), &wide(700).to_json()).is_empty());
    assert!(!regression(&wide(690), &wide(700).to_json()).is_empty());
    // Five points less, or more wrong fields: red.
    assert!(!regression(&outcome(65, 10), &base).is_empty());
    assert!(!regression(&outcome(70, 11), &base).is_empty());
    // Counter-check: a base raised by five points turns the run that was fine red.
    assert!(!regression(&outcome(70, 10), &outcome(75, 10).to_json()).is_empty());
}

#[test]
fn the_measurement_counts_the_classes_of_errors() {
    let index = Index::new(&["Iron".to_string(), "Gold".to_string(), "Tin".to_string()], |_| false);
    let expected: Vec<Expected> = [
        "IRON [ORE]|880|50|180|true",
        "GOLD (ORE)|200|60|300|true",
        "TIN (ORE)|555|70|123|true",
        "IRON [ORE]|333|70|null|false",
    ]
    .into_iter()
    .filter_map(|line| Expected::parse(line, &index))
    .collect();
    // Iron: 0 and 8 swapped in the quality, the yield 180 read as 18. Gold: both read as nothing, the yield holds a 0.
    // Tin: the yield empty although it holds no 0 and no 8. The last line is not read at all; a yield stood where it
    // was empty.
    let read: Vec<Read> =
        ["Iron 800 18 cSCU", "Gold ? ? cSCU", "Tin 555 ? cSCU"].into_iter().filter_map(Read::parse).collect();
    let (_, _, _, classes) = compare_classed(&expected, &read);
    let count = |name: &str| classes.iter().filter(|(class, _)| *class == name).count();
    assert_eq!(count("Qualitaet 0 und 8 vertauscht"), 1);
    assert_eq!(count("Ausbeute anders falsch"), 1);
    assert_eq!(count("Qualitaet leer, Wert mit 0 oder 8"), 1);
    assert_eq!(count("Ausbeute leer, Wert mit 0 oder 8"), 1);
    assert_eq!(count("Ausbeute leer, Wert ohne 0 und 8"), 1);
    assert_eq!(count("Zeile fehlt"), 1);
    // A value read where none stands.
    let wrongly: Vec<Read> = ["Iron 333 99 cSCU"].into_iter().filter_map(Read::parse).collect();
    let last = &expected[3..];
    assert_eq!(compare_classed(last, &wrongly).3.iter().filter(|(class, _)| *class == CLASSES[10]).count(), 1);
}

#[test]
fn the_comparison_counts_each_wrong_field() {
    let index = Index::new(&["Iron".to_string(), "Gold".to_string()], |_| false);
    let expected: Vec<Expected> =
        ["IRON [ORE]|100|50|20|true", "GOLD (ORE)|200|60|30|true", "INERT MATERIALS|0|70|0|false"]
            .into_iter()
            .filter_map(|line| Expected::parse(line, &index))
            .collect();
    let read: Vec<Read> =
        ["Gold 200 30 cSCU", "Tin 5 5 cSCU", "Iron 100 21 cSCU"].into_iter().filter_map(Read::parse).collect();
    let (all, bookable, notes) = compare(&expected, &read);
    assert_eq!(
        all,
        Tally { expected: 3, whole: 1, material: 2, quality: 2, yield_: 1, missing: 1, extra: 1, wrong: 1 },
        "{notes:?}"
    );
    assert_eq!(bookable.expected, 2);
    assert_eq!(bookable.whole, 1);
    assert_eq!(Read::parse("Iron ? 17 cSCU"), Some(Read { material: "Iron".into(), quality: None, amount: Some(17) }));
}
