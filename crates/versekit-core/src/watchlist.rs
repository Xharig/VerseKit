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

//! The watch list as the watcher needs it (`watchlist.py`): which entry waits
//! for a blueprint, and ticking it off when the blueprint arrives.
//!
//! The file `watchlist.json` stays readable and editable by hand:
//!
//! ```text
//! {"namen": ["Attrition-5 Repeater"],
//!  "eintraege": [{"titel": "Helm meiner Wahl", "muster": ["adp-mk4", "woodland"]}]}
//! ```
//!
//! Names are what was clicked in the window (exact); patterns are pieces of a
//! name written from outside. ⚠ A pattern matches at word borders, not in the
//! middle of a word: `arden backpack` must not hit „Warden Backpack“. A
//! clicked name stays when the blueprint comes (it counts as done because it
//! is in the collection); only a pattern entry goes: it waited for „some
//! fitting part“, and that is there now.

use crate::jsonio::{self, Map, Newline, Value, map_set};
use crate::paths::{Paths, name_key};
use crate::text::py_strip;

pub const FILE: &str = "watchlist.json";

/// The list; a missing file is an empty list, a broken one is set aside (`load_json`).
pub fn load(paths: &Paths) -> Value {
    let valid = |v: &Value| matches!(v, Value::Object(_));
    let mut data = match jsonio::load_json(&paths.app_file(FILE), Some(&valid), "watchlist.load") {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    };
    for key in ["namen", "eintraege"] {
        if !matches!(data.iter().find(|(k, _)| k == key), Some((_, Value::Array(_)))) {
            map_set(&mut data, key, Value::Array(Vec::new()));
        }
    }
    Value::Object(data)
}

/// Writes through a side file so that a crash tears nothing, under the write lock.
pub fn save(paths: &Paths, data: &Value) -> bool {
    let _lock = jsonio::lock_saves();
    let target = paths.app_file(FILE);
    let folder = crate::pypath::Flavor::native().dirname(&target);
    if !folder.is_empty() && jsonio::make_dirs(&folder).is_err() {
        return false;
    }
    match jsonio::write_text_atomic(&target, &jsonio::dumps(data, Some(1), false), Newline::Native) {
        Ok(()) => true,
        Err(e) => {
            crate::errors::record("watchlist.save", &e);
            false
        }
    }
}

fn list<'a>(data: &'a Value, key: &str) -> &'a [Value] {
    match data.get(key) {
        Some(Value::Array(items)) => items,
        _ => &[],
    }
}

/// Does an observation fit this name (in comparison form)?
///
/// ⚠ Before and after the pattern no letter and no digit may stand; hyphens
/// and spaces count as borders, so that `abc-mk4 legs grey` still fits.
fn pattern_matches(entry: &Value, key: &str) -> bool {
    let Some(Value::Array(patterns)) = entry.get("muster") else { return false };
    patterns.iter().any(|p| {
        let pattern = match p {
            Value::Str(s) => py_strip(&s.to_lowercase()).to_string(),
            Value::Null => String::new(),
            other => py_strip(&jsonio::dumps(other, None, false).to_lowercase()).to_string(),
        };
        if pattern.is_empty() {
            return false;
        }
        let word = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
        // Every place, also overlapping ones, like a search of the whole text.
        key.char_indices().filter(|(at, _)| key[*at..].starts_with(pattern.as_str())).any(|(at, _)| {
            let before = key[..at].chars().next_back();
            let after = key[at + pattern.len()..].chars().next();
            !before.is_some_and(word) && !after.is_some_and(word)
        })
    })
}

/// Is this blueprint waited for? The title of the entry: the name itself for
/// a clicked one, the title of the observation for a pattern.
pub fn matching(data: &Value, name: &str) -> Option<String> {
    let key = name_key(name);
    if let Some(found) = list(data, "namen").iter().filter_map(Value::as_str).find(|n| name_key(n) == key) {
        return Some(found.to_string());
    }
    for entry in list(data, "eintraege") {
        if pattern_matches(entry, &key) {
            return Some(match entry.get("titel") {
                Some(Value::Str(title)) if !title.is_empty() => title.clone(),
                _ => name.to_string(),
            });
        }
    }
    None
}

/// An entry is met: gives the title that was waited for, and takes the
/// pattern entries that fit away. Called when a blueprint lands in the
/// collection.
pub fn fulfill(paths: &Paths, name: &str) -> Option<String> {
    // Read, changed and written under the write lock: the main window writes the same file.
    let _lock = crate::jsonio::lock_saves();
    let mut data = load(paths);
    let title = matching(&data, name)?;
    let key = name_key(name);
    let entries = list(&data, "eintraege");
    let kept: Vec<Value> = entries.iter().filter(|e| !pattern_matches(e, &key)).cloned().collect();
    if kept.len() != entries.len()
        && let Value::Object(map) = &mut data
    {
        map_set(map, "eintraege", Value::Array(kept));
        save(paths, &data);
    }
    Some(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(text: &str) -> Value {
        jsonio::loads(text).expect("Beispiel")
    }

    #[test]
    fn a_pattern_fits_at_word_borders_only() {
        let list = data(r#"{"namen": [], "eintraege": [{"titel": "Rucksack", "muster": ["arden backpack"]}]}"#);
        assert_eq!(matching(&list, "Warden Backpack Purgatory"), None);
        assert_eq!(matching(&list, "Arden Backpack Purgatory").as_deref(), Some("Rucksack"));
        assert_eq!(matching(&list, "The arden backpack-mk2").as_deref(), Some("Rucksack"));
    }

    #[test]
    fn a_clicked_name_beats_a_pattern_and_a_pattern_without_title_gives_the_name() {
        let list =
            data(r#"{"namen": ["Helm Eins"], "eintraege": [{"muster": ["helm"]}, {"titel": "", "muster": ["  "]}]}"#);
        assert_eq!(matching(&list, "HELM EINS").as_deref(), Some("Helm Eins"));
        assert_eq!(matching(&list, "Helm Zwei").as_deref(), Some("Helm Zwei"));
        assert_eq!(matching(&list, "Visier"), None);
    }

    /// Two writers of the list: a blueprint that ticks off a pattern entry while another writer adds a name.
    #[test]
    fn ticking_off_waits_for_another_writer_and_keeps_what_it_wrote() {
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let dir = crate::testdir::Dir::new("merkliste-zwei-schreiber");
        let home = dir.path().to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(crate::legacy::ENV_HOME, home.as_str())], &home));
        let start = data(r#"{"namen": [], "eintraege": [{"titel": "Rucksack", "muster": ["arden backpack"]}]}"#);
        assert!(save(&paths, &start));
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let writer = {
            let paths = Paths::new(paths.host.clone());
            std::thread::spawn(move || {
                let _lock = jsonio::lock_saves();
                let mut list = load(&paths);
                held_tx.send(()).expect("signal");
                // The ticking off starts meanwhile and has to wait for this write.
                std::thread::sleep(std::time::Duration::from_millis(400));
                if let Value::Object(map) = &mut list {
                    map_set(map, "namen", Value::Array(vec![Value::str("Helm Neu")]));
                }
                assert!(save(&paths, &list));
            })
        };
        held_rx.recv().expect("the writer holds the lock");
        assert_eq!(fulfill(&paths, "Arden Backpack Purgatory").as_deref(), Some("Rucksack"));
        writer.join().expect("writer");
        let now = load(&paths);
        assert_eq!(list(&now, "namen"), [Value::str("Helm Neu")], "the name of the other writer is lost");
        assert!(list(&now, "eintraege").is_empty(), "the pattern entry came back: {now:?}");
    }
}
