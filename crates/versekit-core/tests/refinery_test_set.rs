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

#![cfg(windows)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use serde_json::Value;
use versekit_core::paths::{Host, Paths};
use versekit_core::refinery_scan::digits::Digits;
use versekit_core::refinery_scan::{
    Index, Job, Ocr, PASSES, ScanError, Word, find_tables, header_columns, join_scrolled, match_material, merge_words,
    read_image, rows, screen, search_jobs,
};
use versekit_core::win_ocr::{Bitmap, Recognition};

const SET: &str = "VERSEKIT_RAFFINERIE_PRUEFSATZ";
const COPY: &str = "VERSEKIT_KOPIE";
const ONLY: &str = "VERSEKIT_RAFFINERIE_AUFTRAG";
const DEBUG: &str = "VERSEKIT_RAFFINERIE_DEBUG";
const GOLDEN: &str = "golden-expected.json";

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
    inner: Recognition<'a>,
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
    }
}

fn share(part: usize, all: usize) -> String {
    if all == 0 { "-".to_string() } else { format!("{:.1} %", 100.0 * part as f64 / all as f64) }
}

/// The comparison of one folder: the tally of all lines, of the bookable lines, and the differences as text.
fn compare(expected: &[Expected], read: &[Read]) -> (Tally, Tally, Vec<String>) {
    let pairs = align(expected, read);
    let (mut all, mut bookable, mut notes) = (Tally::default(), Tally::default(), Vec::new());
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
    (all, bookable, notes)
}

/// One folder of the set.
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
}

fn read_folder(folder: &Folder, index: &Index, learned: &Digits, cache: &mut Cache) -> Reading {
    let mut reading = Reading { field: String::new(), station: String::new(), pictures: Vec::new() };
    for path in &folder.pictures {
        let bitmap = load(path);
        let started = Instant::now();
        let mut ocr = Remembered { inner: Recognition::new(&bitmap), known: cache.remove(path).unwrap_or_default() };
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
        if let Some(job) = best
            && reading.station.is_empty()
        {
            reading.station = job.station.clone();
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

/// One measurement over every folder, printed.
fn measure(label: &str, folders: &[Folder], index: &Index, learned: &Digits, cache: &mut Cache) {
    println!("\n######## {label}");
    let (mut all, mut bookable) = (Tally::default(), Tally::default());
    let (mut stations, mut stations_right) = (0, 0);
    let mut groups: Vec<(String, Tally)> = Vec::new();
    let mut table = Vec::new();
    let mut details = Vec::new();
    for folder in folders {
        let reading = read_folder(folder, index, learned, cache);
        let read: Vec<Read> = reading.field.lines().filter_map(Read::parse).collect();
        let (tally, booked, notes) = compare(&folder.expected, &read);
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
    let print = |name: &str, t: &Tally| {
        println!(
            "{name}: {} Zeilen, ganz richtig {} ({}), Material {} ({}), Qualitaet {} ({}), Ausbeute {} ({}), fehlend {}, zusaetzlich {}, Menge nicht gelesen",
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
        );
    };
    println!();
    print("ALLE", &all);
    print("BUCHBAR (refine, Ausbeute > 0)", &bookable);
    println!("STATION: {stations_right} von {stations} ({})", share(stations_right, stations));
    for (group, t) in &groups {
        println!("GRUPPE {group}: {} von {} ganz richtig ({})", t.whole, t.expected, share(t.whole, t.expected));
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
    let folders = folders(&set, &index);
    assert!(folders.iter().any(|f| !f.pictures.is_empty()), "no pictures");
    let mut cache = HashMap::new();
    let source = if copy.is_some() { "Materialien und Orte der Kopie" } else { "Liste der Erze, keine Orte" };
    measure(&format!("ohne gelernte Ziffern, {source}"), &folders, &index, &Digits::default(), &mut cache);
    if let Some(paths) = &copy {
        let learned = Digits::load(paths);
        if !learned.is_empty() {
            measure(&format!("mit gelernten Ziffern der Kopie, {source}"), &folders, &index, &learned, &mut cache);
        }
    }
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
        Tally { expected: 3, whole: 1, material: 2, quality: 2, yield_: 1, missing: 1, extra: 1 },
        "{notes:?}"
    );
    assert_eq!(bookable.expected, 2);
    assert_eq!(bookable.whole, 1);
    assert_eq!(Read::parse("Iron ? 17 cSCU"), Some(Read { material: "Iron".into(), quality: None, amount: Some(17) }));
}
