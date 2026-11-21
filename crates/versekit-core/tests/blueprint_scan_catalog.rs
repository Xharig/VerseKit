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

//! The matching of the blueprint scan over a whole catalog, with made-up faults of the recognition: every name of
//! the catalog is read three times, each reading spoiled in its own way (characters mistaken, one dropped, one
//! wrong, the tag behind it), and the outcome is counted against the name. No name may come out sure and wrong.
//!
//! The catalog is read, never written. Run with `VERSEKIT_CATALOG=<katalog-spieldaten.json> cargo test -p
//! versekit-core --release --test blueprint_scan_catalog -- --ignored --nocapture`.

use versekit_core::blueprint_scan::{Names, Reading, Tally, best_of_lines, combine, entry_of};
use versekit_core::catalog_cache::{CatalogCache, grade_letter, parse};

/// A small generator of numbers, the same on every run.
struct Dice(u64);

impl Dice {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) % below.max(1) as u64) as usize
    }
}

/// Characters the recognition mistakes for one another, as it gives them.
const MISTAKEN: [(&str, &str); 9] =
    [("l", "I"), ("I", "l"), ("i", "l"), ("o", "0"), ("O", "0"), ("s", "5"), ("S", "5"), ("m", "rn"), ("B", "8")];

/// One way the recognition spoils a name: 0 clean, 1 a character mistaken, 2 one dropped, 3 one wrong.
fn spoiled(name: &str, how: usize, dice: &mut Dice) -> String {
    let chars: Vec<char> = name.chars().collect();
    match how {
        1 => {
            let found: Vec<(usize, &str)> = MISTAKEN
                .iter()
                .enumerate()
                .filter(|(_, (from, _))| name.contains(from))
                .map(|(i, _)| (i, ""))
                .collect();
            match found.get(dice.next(found.len())) {
                Some((i, _)) => name.replacen(MISTAKEN[*i].0, MISTAKEN[*i].1, 1),
                None => name.to_string(),
            }
        }
        2 if chars.len() > 8 => {
            let at = 1 + dice.next(chars.len() - 2);
            chars.iter().enumerate().filter(|(i, _)| *i != at).map(|(_, c)| c).collect()
        }
        3 if chars.len() > 8 => {
            let at = 1 + dice.next(chars.len() - 2);
            let wrong = ['x', 'q', 'k', 'v', 'w'][dice.next(5)];
            chars.iter().enumerate().map(|(i, c)| if i == at { wrong } else { *c }).collect()
        }
        _ => name.to_string(),
    }
}

/// The tag of a blueprint as the game writes it behind the name, where it has two parts at least.
fn tag_of(catalog: &CatalogCache, name: &str) -> String {
    let Some(blueprint) = catalog.blueprints.iter().find(|b| b.name == name) else { return String::new() };
    let entry = entry_of(blueprint);
    let parts = [
        entry.class.clone(),
        entry.size.map(|s| s.to_string()),
        blueprint.grade.and_then(grade_letter).map(String::from),
    ];
    if parts.iter().filter(|p| p.is_some()).count() < 2 {
        return String::new();
    }
    let part = |p: &Option<String>| p.clone().unwrap_or_else(|| "–".into());
    format!(" ({}/{}/{})", part(&parts[0]), part(&parts[1]), part(&parts[2]))
}

fn load() -> Option<(CatalogCache, Names)> {
    let path = std::env::var("VERSEKIT_CATALOG").ok()?;
    let data = versekit_core::jsonio::read_json(&path).expect("the catalog");
    let catalog = parse(&data);
    assert!(!catalog.blueprints.is_empty(), "no blueprints in {path}");
    let names = Names::of_catalog(&catalog);
    Some((catalog, names))
}

fn read(lines: &[String], names: &Names) -> Reading {
    best_of_lines(lines, names)
}

#[test]
#[ignore = "reads a catalog of the game data"]
fn no_name_of_the_catalog_comes_out_sure_and_wrong() {
    let Some((catalog, names)) = load() else { return };
    let mut dice = Dice(19);
    let mut by_kind = [Tally::default(); 5];
    let mut wrong = Vec::new();
    let mut missed = Vec::new();
    for name in names.all() {
        let tag = tag_of(&catalog, name);
        // Kind 0: clean; 1: characters mistaken; 2: one dropped; 3: one wrong; 4: all mixed, tag behind.
        for (kind, tally) in by_kind.iter_mut().enumerate() {
            let readings: Vec<Reading> = (0..3)
                .map(|pass| {
                    let how = if kind == 4 {
                        dice.next(4)
                    } else if pass == 0 && kind > 0 {
                        0
                    } else {
                        kind
                    };
                    let text = spoiled(name, how, &mut dice);
                    let text = if kind == 4 { format!("{text}{tag}") } else { text };
                    read(&[text], &names)
                })
                .collect();
            let outcome = combine(&readings);
            if outcome.certainty == versekit_core::blueprint_scan::Certainty::Sure
                && outcome.name.as_deref() != Some(name)
            {
                wrong.push(format!("{name} → {:?} from {:?}", outcome.name, outcome.lines));
            }
            if kind == 0 && outcome.certainty == versekit_core::blueprint_scan::Certainty::None {
                missed.push(name.to_string());
            }
            tally.count(&outcome, Some(name));
        }
    }
    for (kind, tally) in by_kind.iter().enumerate() {
        println!("kind {kind}: {tally}");
    }
    println!("clean and not found: {missed:?}");
    assert!(wrong.is_empty(), "{} sure and wrong:\n{}", wrong.len(), wrong.join("\n"));
    // A clean name is found, sure or to confirm, but for the names too short to read (fewer than three letters).
    let clean = &by_kind[0];
    assert_eq!(clean.asked_wrong, 0, "clean names: {clean}");
    assert!(missed.iter().all(|n| versekit_core::blueprint_scan::plain(n).chars().count() < 3), "{missed:?}");
}

/// A word lost in every reading is what text alone cannot tell from a shorter name: counted and shown, not hidden.
#[test]
#[ignore = "reads a catalog of the game data"]
fn a_last_word_lost_in_every_reading_is_counted() {
    let Some((catalog, names)) = load() else { return };
    let mut tally = Tally::default();
    let mut shown = 0;
    for name in names.all() {
        let Some((short, _)) = name.rsplit_once(' ') else { continue };
        if names.entry(short).is_none() {
            continue;
        }
        let tag = tag_of(&catalog, name);
        let line = format!("{short}{tag}");
        let outcome = combine(&[read(std::slice::from_ref(&line), &names), read(std::slice::from_ref(&line), &names)]);
        if shown < 5 {
            println!("{name} read as {line:?}: {:?} {:?}", outcome.certainty, outcome.name);
            shown += 1;
        }
        tally.count(&outcome, Some(name));
    }
    println!("last word lost: {tally}");
    assert_eq!(tally.wrong, 0, "a shorter name taken sure: {tally}");
}
