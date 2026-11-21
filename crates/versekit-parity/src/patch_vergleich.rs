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

//! `versekit-parity patch_vergleich <kanalordner-alt> <kanalordner-neu> [--erkul <patch-datei>]`: reads the
//! `Data.p4k` of two channel folders (copies, read only: the game is not asked whether it runs) with the readers of
//! the stands (`versekit_gamedata::stand_read::read_stand`), compares the two stands and prints **numbers only**:
//! the figures of every area, the gaps that grew, the classes. Nothing of the content, so that the run of a test
//! channel can be told in the vault. The same folder twice is the check against itself: `ERGEBNIS gleich`.
//!
//! With `--erkul` the comparison is held against a patch file of erkul (`Patches\patch-<version>.json`) for what both
//! have: the items erkul says are new or gone must be new or gone here, the items it says changed must be among the
//! changed ones. The result is `ERGEBNIS gleich` or the differences, counted.
//!
//! Run with a data folder of its own (`VERSEKIT_HOME=<temporary folder>`), so that no real data is touched.

use std::collections::BTreeSet;
use std::path::Path;

use versekit_core::jsonio::{self, Value};
use versekit_core::patch_changes::Fate;
use versekit_core::stand_diff::{self, Comparison};
use versekit_core::stands::Stand;
use versekit_gamedata::{GameData, stand_read};

fn read(folder: &str) -> Result<Stand, String> {
    let archive = Path::new(folder).join("Data.p4k");
    let game = GameData::open_file_after(&archive, &|| true, versekit_core::archive_lock::POLL)
        .map_err(|e| format!("{folder}: {e}"))?;
    stand_read::read_stand(&game, "2026-01-01").ok_or_else(|| format!("{folder}: no stand could be read"))
}

/// What erkul says against what the comparison found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ErkulCheck {
    pub new_total: usize,
    pub new_found: usize,
    pub gone_total: usize,
    pub gone_found: usize,
    pub changed_total: usize,
    pub changed_found: usize,
    /// Changed items that stand in a row shared by more entries than the row names: not decidable.
    pub changed_unknown: usize,
}

impl ErkulCheck {
    /// Everything erkul says is found.
    pub fn equal(&self) -> bool {
        self.new_found == self.new_total && self.gone_found == self.gone_total && self.changed_found + self.changed_unknown == self.changed_total
    }
}

fn names_of(item: &versekit_core::jsonio::Map) -> Vec<String> {
    ["id", "className", "name"]
        .iter()
        .filter_map(|key| item.iter().find(|(k, _)| k == key).and_then(|(_, v)| v.as_str()))
        .filter(|s| !s.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Holds the comparison against a patch of erkul: of its items, new and gone ones by name or key, changed ones
/// among the changed.
pub fn check_erkul(cmp: &Comparison, erkul: &Value) -> ErkulCheck {
    let keys = |fate: Fate| -> (BTreeSet<String>, bool) {
        let mut set = BTreeSet::new();
        let mut cut = false;
        for row in cmp.rows.iter().filter(|r| r.fate == fate) {
            set.insert(row.id.to_lowercase());
            set.insert(row.name.to_lowercase());
            set.extend(row.examples.iter().map(|e| e.to_lowercase()));
            cut |= row.count > row.examples.len().max(1);
        }
        (set, cut)
    };
    let (new_keys, _) = keys(Fate::New);
    let (gone_keys, _) = keys(Fate::Gone);
    let (changed_keys, changed_cut) = keys(Fate::Changed);
    let mut out = ErkulCheck::default();
    let Some(Value::Array(categories)) = erkul.get("categories") else { return out };
    for category in categories.iter().filter_map(Value::as_object) {
        for (key, wanted) in [("added", 0), ("removed", 1), ("modified", 2)] {
            let Some(Value::Array(items)) = category.iter().find(|(k, _)| k == key).map(|(_, v)| v) else { continue };
            for item in items.iter().filter_map(Value::as_object) {
                let names = names_of(item);
                let hit = |set: &BTreeSet<String>| names.iter().any(|n| set.contains(n));
                match wanted {
                    0 => {
                        out.new_total += 1;
                        out.new_found += usize::from(hit(&new_keys));
                    }
                    1 => {
                        out.gone_total += 1;
                        out.gone_found += usize::from(hit(&gone_keys));
                    }
                    _ => {
                        out.changed_total += 1;
                        if hit(&changed_keys) {
                            out.changed_found += 1;
                        } else if changed_cut {
                            out.changed_unknown += 1;
                        }
                    }
                }
            }
        }
    }
    out
}

fn print_numbers(cmp: &Comparison) {
    println!("von {} nach {} (Leser {} / {})", cmp.from.build, cmp.to.build, cmp.from.reader, cmp.to.reader);
    for c in &cmp.counts {
        println!(
            "  {:<28} alt {:>6} neu {:>6} | neu {:>5} weg {:>5} geaendert {:>5} gleich {:>6}",
            c.area, c.old, c.new, c.added, c.removed, c.changed, c.unchanged
        );
    }
    for s in &cmp.scales {
        println!("  Skala {:<22} {:?} -> {:?}", s.area, s.old, s.new);
    }
    for w in &cmp.warnings {
        println!("  Warnung {:<20} {} -> {} ({:?})", w.name, w.old, w.new, w.class);
    }
    let classes = cmp.classes();
    println!(
        "Klassen: bricht {} | sieht anders aus {} | neu oder geaendert {}",
        classes.breaks, classes.looks_different, classes.new_or_changed
    );
    println!("Zeilen: {}", cmp.rows.len());
}

pub fn run(args: &[String]) -> Result<(), String> {
    let [old, new, rest @ ..] = args else {
        return Err("patch_vergleich <kanalordner-alt> <kanalordner-neu> [--erkul <patch-datei>]".into());
    };
    let started = std::time::Instant::now();
    let before = read(old)?;
    let after = read(new)?;
    println!("gelesen in {:.1} s; Eintraege alt {}, neu {}", started.elapsed().as_secs_f64(), before.entry_count(), after.entry_count());
    let cmp = stand_diff::compare(&before, &after);
    print_numbers(&cmp);
    if old == new {
        if cmp.is_empty() {
            println!("ERGEBNIS gleich (Selbstvergleich)");
        } else {
            println!("ERGEBNIS Abweichung im Selbstvergleich");
            return Err("the same archive read twice differs".into());
        }
    }
    if let Some(at) = rest.iter().position(|a| a == "--erkul") {
        let file = rest.get(at + 1).ok_or("--erkul needs a file")?;
        let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
        let erkul = jsonio::loads(&text).map_err(|e| format!("{file}: {e}"))?;
        let check = check_erkul(&cmp, &erkul);
        println!(
            "Erkul: neu {}/{}, weg {}/{}, geaendert {}/{} (nicht entscheidbar {})",
            check.new_found, check.new_total, check.gone_found, check.gone_total, check.changed_found, check.changed_total, check.changed_unknown
        );
        if check.equal() {
            println!("ERGEBNIS gleich");
        } else {
            println!("ERGEBNIS Abweichungen gegenueber Erkul");
            return Err("the comparison differs from erkul".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use versekit_core::stands::{Area, Leaf, probe_of};

    fn stand(build: &str, hp: &[(&str, i64)]) -> Stand {
        let mut stand = Stand::new("LIVE", build, "2026-01-01", "r");
        let mut area = Area::default();
        for (key, value) in hp {
            area.entries.insert((*key).into(), [("hp".to_string(), Leaf::Int(*value))].into());
        }
        stand.areas.insert("werte.items".into(), area);
        stand.probe = probe_of(&stand.areas);
        stand
    }

    fn erkul(text: &str) -> Value {
        jsonio::loads(text).expect("json")
    }

    #[test]
    fn erkul_and_the_own_comparison_agree_when_what_erkul_names_is_found() {
        let cmp = stand_diff::compare(&stand("a", &[("x", 1), ("y", 2), ("z", 3)]), &stand("b", &[("x", 5), ("y", 2), ("w", 9)]));
        let patch = erkul(r#"{"categories": [{"kind": "ships", "added": [{"id": "W"}], "removed": [{"name": "z"}], "modified": [{"className": "x"}]}]}"#);
        let check = check_erkul(&cmp, &patch);
        assert_eq!((check.new_found, check.new_total, check.gone_found, check.changed_found), (1, 1, 1, 1));
        assert!(check.equal());
    }

    #[test]
    fn an_item_erkul_names_that_the_own_comparison_missed_is_a_difference() {
        let cmp = stand_diff::compare(&stand("a", &[("x", 1)]), &stand("b", &[("x", 1)]));
        let patch = erkul(r#"{"categories": [{"kind": "ships", "added": [{"id": "ghost"}], "removed": [], "modified": []}]}"#);
        let check = check_erkul(&cmp, &patch);
        assert_eq!((check.new_found, check.new_total), (0, 1));
        assert!(!check.equal());
    }
}
