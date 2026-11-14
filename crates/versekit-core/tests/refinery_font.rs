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

//! Learns the font of the own text recognition (`refinery_scan/schrift.bin`) from the set of refinery pictures with
//! known lines (`refinery_test_set.rs` describes the set). Every picture is read as the key in the game reads it,
//! by the recognition of Windows; of its words only the sure ones teach: a number the expected lines of the folder
//! hold, a word of the expected lines or the station, or a word of the terminal ([`TERMINAL`]). Each such word is
//! cut by the own recognition ([`own_ocr::segment`]) in the same part and enlargement; when it falls into as many
//! letters as the word has, every letter learns its character.
//!
//! The answers of the recognition of Windows are kept per picture in `VERSEKIT_SCHRIFT_ZWISCHEN=<folder>` (JSON),
//! so a second run only cuts anew. `VERSEKIT_RAFFINERIE_TEIL=gerade|ungerade` learns from the folders with an
//! even or odd number only (the other half measures it, `refinery_test_set.rs` with `VERSEKIT_SCHRIFT`).
//!
//! Run with `VERSEKIT_RAFFINERIE_PRUEFSATZ=<set> VERSEKIT_SCHRIFT_AUS=<file> cargo test -p versekit-core --release
//! --test refinery_font -- --ignored --nocapture`.

#![cfg(windows)]

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use versekit_core::refinery_scan::digits::Digits;
use versekit_core::refinery_scan::own_ocr::{self, Font, Shape};
use versekit_core::refinery_scan::{Index, Job, Mode, Ocr, ScanError, Word, read_image};
use versekit_core::win_ocr::{Bitmap, Recognition};

const SET: &str = "VERSEKIT_RAFFINERIE_PRUEFSATZ";
const OUT: &str = "VERSEKIT_SCHRIFT_AUS";
const CACHE: &str = "VERSEKIT_SCHRIFT_ZWISCHEN";
const PART: &str = "VERSEKIT_RAFFINERIE_TEIL";
const KEEP: &str = "VERSEKIT_SCHRIFT_JE_ZEICHEN";
/// The small letters learned: those of the time left.
const LOWER_KEPT: &str = "hmd";

/// Words of the terminal, capitals: heads, labels, methods, the ores.
const TERMINAL: &[&str] = &[
    "REFINEMENT",
    "CENTER",
    "STATION",
    "PROFILE",
    "MATERIAL",
    "SPECIALIZATIONS",
    "YIELD",
    "REFINERY",
    "CAPACITY",
    "CURRENT",
    "USER",
    "DETAILS",
    "SELECTION",
    "WORK",
    "ORDER",
    "SETUP",
    "RAW",
    "MATERIALS",
    "MANIFEST",
    "REFINE",
    "PROCESSING",
    "AND",
    "COSTS",
    "LOW",
    "SPEED",
    "COST",
    "MODERATE",
    "HIGH",
    "QUALITY",
    "QTY",
    "SELECTED",
    "TOTAL",
    "TIME",
    "CANCEL",
    "CONFIRM",
    "CSCU",
    "REFINABLE",
    "INERT",
    "FREE",
    "SPACE",
    "BALANCE",
    "SYSTEM",
    "FERRON",
    "EXCHANGE",
    "DINYX",
    "SOLVENTATION",
    "PYROMETRIC",
    "CHROMALYSIS",
    "THERMONATIC",
    "DEPOSITION",
    "ELECTROSTAROLYSIS",
    "GASKIN",
    "PROCESS",
    "KAZEN",
    "WINNOWING",
    "XCR",
    "REACTION",
    "CORMACK",
    "METHOD",
    "AGRICIUM",
    "ALUMINUM",
    "ASLARITE",
    "BERYL",
    "BEXALITE",
    "BORASE",
    "COPPER",
    "CORUNDUM",
    "GOLD",
    "HEPHAESTANITE",
    "IRON",
    "LARANITE",
    "LINDINIUM",
    "OURATITE",
    "QUANTAINIUM",
    "QUARTZ",
    "RICCITE",
    "SAVRILIUM",
    "STILERON",
    "TARANITE",
    "TIN",
    "TITANIUM",
    "TORITE",
    "TUNGSTEN",
    "ORE",
    "(ORE)",
    "(RAW)",
    "RUBBLE",
    "CONSTRUCTION",
    "SALVAGE",
    "RECYCLED",
    "STATUS",
    "COMPLETE",
    "REMAINING",
    "EXTREME",
    "WORKLOAD",
    "LARGE",
    "SURCHARGE",
    "WILL",
    "BE",
    "ADDED",
    "HAS",
    "AN",
    "IN",
    "TO",
    "UEC",
    "AUEC",
];

/// One reading of the recognition of Windows: the job and the words it gave.
#[derive(Clone, Debug)]
struct Record {
    job: Job,
    words: Vec<Word>,
}

/// The recognition of Windows that writes down every job and its words.
struct Recorder<'a> {
    inner: Recognition<'a>,
    records: Vec<Record>,
}

impl Ocr for Recorder<'_> {
    fn read(&mut self, jobs: &[Job]) -> Result<Vec<Vec<Word>>, ScanError> {
        let out = self.inner.read(jobs)?;
        // A job outside the picture gives no list: then the lists cannot be told apart and none is kept.
        if out.len() == jobs.len() {
            self.records
                .extend(jobs.iter().zip(&out).map(|(job, words)| Record { job: job.clone(), words: words.clone() }));
        }
        Ok(out)
    }
}

fn mode_of(name: &str) -> Mode {
    match name {
        "gray" => Mode::Gray,
        "dark" => Mode::Dark,
        _ => Mode::Plain,
    }
}

fn to_json(records: &[Record]) -> Value {
    Value::Array(
        records
            .iter()
            .map(|r| {
                json!({
                    "job": [r.job.area.0, r.job.area.1, r.job.area.2, r.job.area.3],
                    "scale": r.job.scale,
                    "mode": r.job.mode.name(),
                    "words": r.words.iter().map(|w| json!([w.t, w.x, w.y, w.w, w.h])).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

fn from_json(value: &Value) -> Option<Vec<Record>> {
    value
        .as_array()?
        .iter()
        .map(|r| {
            let area: Vec<i64> = r["job"].as_array()?.iter().filter_map(Value::as_i64).collect();
            let [x, y, w, h] = area.as_slice() else { return None };
            let words = r["words"]
                .as_array()?
                .iter()
                .map(|w| {
                    let n = |i: usize| w[i].as_f64();
                    Some(Word::new(w[0].as_str()?, n(1)?, n(2)?, n(3)?, n(4)?))
                })
                .collect::<Option<Vec<_>>>()?;
            let job = Job { area: (*x, *y, *w, *h), scale: r["scale"].as_f64()?, mode: mode_of(r["mode"].as_str()?) };
            Some(Record { job, words })
        })
        .collect()
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

/// The share of the smaller frame two frames have in common.
fn overlap(a: &Word, b: &Word) -> f64 {
    let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    if w <= 0.0 || h <= 0.0 {
        return 0.0;
    }
    let (small, large) = ((a.w * a.h).min(b.w * b.h), (a.w * a.h).max(b.w * b.h));
    if small <= 0.0 { 0.0 } else { (w * h) / large.max(small) }
}

/// The sure words of a folder: numbers of its lines (the digits), words of its lines and its station (capitals).
struct Sure {
    numbers: HashSet<String>,
    words: HashSet<String>,
}

impl Sure {
    fn of(entry: &Value) -> Sure {
        let mut numbers = HashSet::new();
        let mut words: HashSet<String> = TERMINAL.iter().map(|w| w.to_string()).collect();
        if let Some(n) = entry["toRefine"].as_i64() {
            numbers.insert(n.to_string());
        }
        for line in entry["rows"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            let parts: Vec<&str> = line.split('|').collect();
            for part in &parts[1..parts.len().min(4)] {
                if part.trim().parse::<i64>().is_ok() {
                    numbers.insert(part.trim().to_string());
                }
            }
            words.extend(parts[0].split_whitespace().map(str::to_uppercase));
        }
        if let Some(location) = entry["location"].as_str() {
            words.extend(location.split_whitespace().map(str::to_uppercase));
        }
        Sure { numbers, words }
    }

    fn holds(&self, text: &str) -> bool {
        let text = text.trim();
        if text.chars().all(|c| c.is_ascii_digit()) {
            return self.numbers.contains(text);
        }
        self.words.contains(&text.to_uppercase())
    }
}

#[test]
#[ignore = "reads a set of pictures with known lines and writes a font"]
fn the_font_of_the_terminal_is_learned_from_the_set() {
    let (Some(set), Some(out)) = (std::env::var_os(SET).map(PathBuf::from), std::env::var_os(OUT).map(PathBuf::from))
    else {
        println!("uebersprungen: {SET} oder {OUT} nicht gesetzt");
        return;
    };
    let cache = std::env::var_os(CACHE).map(PathBuf::from);
    let part = std::env::var(PART).ok();
    let keep: usize = std::env::var(KEEP).ok().and_then(|k| k.parse().ok()).unwrap_or(40);
    versekit_core::writeguard::deny(true);
    versekit_core::writeguard::allow_under(None);
    let golden: Value =
        serde_json::from_slice(&std::fs::read(set.join("golden-expected.json")).expect("golden")).expect("JSON");
    let index = Index::new(&[], |_| false);
    let (mut samples, mut sure_words, mut cut_alike, mut cut_otherwise) = (Vec::<(char, Shape)>::new(), 0, 0, 0);
    // Why a sure word was not taken: no word of the own cutting on it, more letters, fewer letters.
    let (mut causes, mut examples) = ([0usize; 3], Vec::<String>::new());
    // The words of the other half, cut alike: (their characters, the cut word).
    let mut checks: Vec<(Vec<char>, own_ocr::Piece)> = Vec::new();
    for (name, entry) in golden.as_object().expect("folders") {
        let number: u32 = name.chars().filter(char::is_ascii_digit).collect::<String>().parse().unwrap_or(0);
        // The other half does not teach: its words check the font afterwards.
        let held_out = match part.as_deref() {
            Some("gerade") => !number.is_multiple_of(2),
            Some("ungerade") => number.is_multiple_of(2),
            _ => false,
        };
        let sure = Sure::of(entry);
        let mut pictures: Vec<PathBuf> =
            std::fs::read_dir(set.join(name)).map(|d| d.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        pictures.retain(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| ["png", "jpg", "jpeg"].contains(&e.to_lowercase().as_str()))
        });
        pictures.sort();
        for path in pictures {
            let bitmap = load(&path);
            let kept = cache.as_ref().map(|c| {
                c.join(format!(
                    "{}__{}.json",
                    name.replace(' ', "_"),
                    path.file_stem().unwrap_or_default().to_string_lossy().replace(' ', "_")
                ))
            });
            let records =
                match kept.as_ref().and_then(|k| std::fs::read(k).ok()).and_then(|b| serde_json::from_slice(&b).ok()) {
                    Some(value) => from_json(&value).expect("kept answers"),
                    None => {
                        let mut recorder = Recorder { inner: Recognition::new(&bitmap), records: Vec::new() };
                        read_image(
                            (bitmap.width, bitmap.height),
                            Some(&bitmap),
                            &mut recorder,
                            &index,
                            &Digits::default(),
                            None,
                        )
                        .expect("reading");
                        if let Some(k) = &kept {
                            let _ = std::fs::create_dir_all(k.parent().expect("folder"));
                            std::fs::write(k, serde_json::to_vec(&to_json(&recorder.records)).expect("JSON"))
                                .expect("kept");
                        }
                        recorder.records
                    }
                };
            for (n, record) in records.iter().enumerate() {
                let Some(cut) = own_ocr::segment(&bitmap, &record.job) else { continue };
                let pieces: Vec<(Word, &own_ocr::Piece)> = cut.words.iter().map(|p| (cut.word("", p), p)).collect();
                // Shown: both cuttings of one reading, ordered by place, for looking at the cutting.
                if std::env::var("VERSEKIT_SCHRIFT_ZEIGEN").is_ok_and(|v| v == format!("{name}#{n}")) {
                    let mut win: Vec<&Word> = record.words.iter().collect();
                    win.sort_by_key(|w| (w.y as i64 / 8, w.x as i64));
                    for w in win {
                        println!("  WIN  {:>5.0},{:>5.0} {:>4.0}x{:<3.0} {}", w.x, w.y, w.w, w.h, w.t);
                    }
                    let mut own: Vec<&(Word, &own_ocr::Piece)> = pieces.iter().collect();
                    own.sort_by_key(|(w, _)| (w.y as i64 / 8, w.x as i64));
                    for (w, p) in own {
                        println!(
                            "  EIG  {:>5.0},{:>5.0} {:>4.0}x{:<3.0} {} Buchstaben",
                            w.x,
                            w.y,
                            w.w,
                            w.h,
                            p.shapes.len()
                        );
                    }
                }
                for word in &record.words {
                    if !sure.holds(&word.t) {
                        continue;
                    }
                    sure_words += 1;
                    let chars: Vec<char> = word.t.trim().chars().filter(|c| !c.is_whitespace()).collect();
                    let best = pieces
                        .iter()
                        .map(|(frame, piece)| (overlap(frame, word), *piece))
                        .filter(|(o, _)| *o > 0.6)
                        .max_by(|a, b| a.0.total_cmp(&b.0));
                    match best {
                        Some((_, piece)) if piece.shapes.len() == chars.len() => {
                            cut_alike += 1;
                            if held_out {
                                checks.push((chars.clone(), (*piece).clone()));
                            } else {
                                samples.extend(chars.iter().copied().zip(piece.shapes.iter().cloned()));
                            }
                        }
                        Some((_, piece)) => {
                            cut_otherwise += 1;
                            let kind = if piece.shapes.len() > chars.len() { 1 } else { 2 };
                            causes[kind] += 1;
                            if examples.len() < 60 && sure_words % 7 == 0 {
                                examples.push(format!(
                                    "{} {:?} -> {} Teile, Rahmen {:?} / {:.0},{:.0},{:.0},{:.0}, Massstab {}",
                                    name,
                                    word.t,
                                    piece.shapes.len(),
                                    piece.frame,
                                    word.x,
                                    word.y,
                                    word.w,
                                    word.h,
                                    record.job.scale
                                ));
                            }
                        }
                        None => {
                            cut_otherwise += 1;
                            causes[0] += 1;
                        }
                    }
                }
            }
        }
        println!("{name}: {} Buchstaben bisher", samples.len());
    }
    let mut per_char: BTreeMap<char, usize> = BTreeMap::new();
    for (c, _) in &samples {
        *per_char.entry(*c).or_default() += 1;
    }
    for example in &examples {
        println!("  anders: {example}");
    }
    println!(
        "sichere Woerter {sure_words}, gleich geschnitten {cut_alike}, anders geschnitten {cut_otherwise} (kein Wort {}, mehr Buchstaben {}, weniger {})",
        causes[0], causes[1], causes[2]
    );
    println!("Buchstaben je Zeichen: {per_char:?}");
    // The terminal writes its table in capitals; of the small letters only those of the time left (`26h 49m`,
    // `2d`) are learned, the others of the texts beside it would only be mistaken for capitals.
    samples.retain(|(c, _)| !c.is_lowercase() || LOWER_KEPT.contains(*c));
    let font = Font::build(&samples, keep);
    if !checks.is_empty() {
        // Letters and words of the other half read with the font: right, unsure (`?`), wrong; the commonest
        // mistakes.
        let (mut right, mut unsure, mut wrong, mut words_right) = (0, 0, 0, 0);
        let mut mistakes: BTreeMap<(char, char), usize> = BTreeMap::new();
        for (chars, piece) in &checks {
            let read: Vec<char> = font.read(piece).chars().collect();
            let fold = |c: char| if LOWER_KEPT.contains(c) { c } else { c.to_ascii_uppercase() };
            words_right += usize::from(read.iter().zip(chars).all(|(r, c)| *r == fold(*c)));
            for (r, c) in read.iter().zip(chars) {
                match *r {
                    r if r == fold(*c) => right += 1,
                    '?' => unsure += 1,
                    r => {
                        wrong += 1;
                        *mistakes.entry((fold(*c), r)).or_default() += 1;
                    }
                }
            }
        }
        // How near a letter of the other half lies to its nearest picture: the bound beyond which a letter is
        // no letter of the font.
        let mut distances: Vec<f32> = checks
            .iter()
            .flat_map(|(_, p)| p.shapes.iter())
            .filter_map(|s| font.classify(s).map(|((_, d), _)| d))
            .collect();
        distances.sort_by(f32::total_cmp);
        let at = |q: f64| distances[((distances.len() - 1) as f64 * q) as usize];
        println!(
            "Abstand zum naechsten Bild: Median {:.4}, 90 % {:.4}, 99 % {:.4}, 99,9 % {:.4}, groesster {:.4}",
            at(0.5),
            at(0.9),
            at(0.99),
            at(0.999),
            at(1.0)
        );
        let mut top: Vec<((char, char), usize)> = mistakes.into_iter().collect();
        top.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        println!(
            "GEGENPROBE andere Haelfte: {} Woerter, ganz richtig {words_right}; Buchstaben richtig {right}, unsicher {unsure}, falsch {wrong}",
            checks.len()
        );
        println!("haeufigste Fehler (soll, gelesen): {:?}", &top[..top.len().min(25)]);
    }
    let bytes = font.to_bytes();
    std::fs::write(&out, &bytes).expect("font written");
    println!("Schrift: {} Bilder, {} Bytes -> {}", font.glyphs.len(), bytes.len(), out.display());
}
