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

//! The catalog watch (`Watcher._catalog_tick`, `catalog_new`): is something
//! newly craftable in the game? It compares the set of craftable items with
//! the one remembered in `catalog-seen.json`. The remembered state survives
//! restarts, otherwise everything would come twice after each start.
//!
//! ⚠ The first run only sets the base, and so does a run where the
//! remembered state covers less than half of the present one: that comes from
//! another source with another spelling, and every one of the roughly 700
//! blueprints would count as new.

use std::collections::BTreeSet;

use crate::jsonio::{self, Newline, Value};
use crate::paths::Paths;
use crate::watchlist;

pub const SEEN_FILE: &str = "catalog-seen.json";

/// What is new, or `None`: only the base is set.
pub fn catalog_new(now: &BTreeSet<String>, known: &BTreeSet<String>) -> Option<Vec<String>> {
    if known.is_empty() || now.intersection(known).count() * 2 < now.len() {
        return None;
    }
    Some(now.difference(known).cloned().collect())
}

/// One item of the catalog: the comparison key, the name as a person reads
/// it and the kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub name: String,
    pub kind: String,
}

/// A newly craftable item with the title of the watch list entry that waits for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub name: String,
    pub kind: String,
    pub title: Option<String>,
}

/// What the watch remembers of its last look: the mark it compared by.
#[derive(Clone, Debug, Default)]
pub struct Watch {
    mark: Option<String>,
}

fn remembered(paths: &Paths) -> BTreeSet<String> {
    match jsonio::read_json(&paths.app_file(SEEN_FILE)).ok().as_ref().and_then(|d| d.get("namen")) {
        Some(Value::Array(items)) => items.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => BTreeSet::new(),
    }
}

fn remember(paths: &Paths, now: &BTreeSet<String>, stamp: &str) {
    // Written under the write lock that every process of the program shares.
    let _lock = jsonio::lock_saves();
    let data = Value::Object(vec![
        ("stand".into(), Value::str(stamp)),
        ("namen".into(), Value::Array(now.iter().map(|k| Value::str(k.as_str())).collect())),
    ]);
    let target = paths.app_file(SEEN_FILE);
    let folder = crate::pypath::Flavor::native().dirname(&target);
    let _ = jsonio::make_dirs(&folder);
    // Silent on a failure, like the original: the next look tries again.
    let _ = jsonio::write_text_atomic(&target, &jsonio::dumps(&data, Some(1), false), Newline::Native);
}

impl Watch {
    /// One look. `mark` is what changes exactly when a patch brings new
    /// blueprints (the version of the catalog); `items` is what is craftable
    /// now; `stamp` is the time written with the remembered state.
    ///
    /// ⚠ The title of the watch list is asked with the name as a person
    /// reads it, not with the key: the key has no spaces or hyphens left, so
    /// a name or a pattern with a border in it could never match.
    ///
    /// The remembered state is read, compared and written as one step under
    /// the write lock: a second watch that looks at the same moment finds the
    /// state of the first one and tells nothing twice.
    pub fn look(&mut self, paths: &Paths, mark: &str, items: &[Item], stamp: &str) -> Vec<Found> {
        if mark.is_empty() || self.mark.as_deref() == Some(mark) {
            return Vec::new();
        }
        self.mark = Some(mark.to_string());
        if items.is_empty() {
            return Vec::new();
        }
        let _lock = jsonio::lock_saves();
        let now: BTreeSet<String> = items.iter().map(|i| i.key.clone()).collect();
        let fresh = match catalog_new(&now, &remembered(paths)) {
            Some(fresh) if !fresh.is_empty() => fresh,
            _ => {
                remember(paths, &now, stamp);
                return Vec::new();
            }
        };
        let list = watchlist::load(paths);
        let found = fresh
            .iter()
            .filter_map(|key| items.iter().find(|i| i.key == *key))
            .map(|item| Found {
                name: item.name.clone(),
                kind: item.kind.clone(),
                title: watchlist::matching(&list, &item.name),
            })
            .collect();
        remember(paths, &now, stamp);
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn the_first_run_and_another_spelling_only_set_the_base() {
        assert_eq!(catalog_new(&set(&["a", "b"]), &set(&[])), None);
        // The remembered state covers less than half of the present one.
        assert_eq!(catalog_new(&set(&["a", "b", "c", "d"]), &set(&["a", "zzz"])), None);
        assert_eq!(catalog_new(&set(&["a", "b", "c"]), &set(&["a", "b"])), Some(vec!["c".to_string()]));
        assert_eq!(catalog_new(&set(&["a", "b"]), &set(&["a", "b", "x"])), Some(Vec::new()));
        // Exactly half still counts.
        assert_eq!(catalog_new(&set(&["a", "b"]), &set(&["a", "x"])), Some(vec!["b".to_string()]));
    }

    /// Two watches on the same data folder (overlay and main window): the second waits for the first one's write
    /// and does not tell the same news again.
    #[test]
    fn a_second_look_waits_for_the_first_and_tells_nothing_twice() {
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let dir = crate::testdir::Dir::new("katalogwache-zwei-schreiber");
        let home = dir.path().to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(crate::legacy::ENV_HOME, home.as_str())], &home));
        let item = |key: &str| Item { key: key.into(), name: key.to_uppercase(), kind: "Waffe".into() };
        let items: Vec<Item> = ["a", "b", "c", "d"].iter().map(|k| item(k)).collect();
        remember(&paths, &set(&["a", "b", "c"]), "1");
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let first = {
            let paths = Paths::new(paths.host.clone());
            std::thread::spawn(move || {
                let _lock = jsonio::lock_saves();
                let known = remembered(&paths);
                held_tx.send(()).expect("signal");
                // The second look starts meanwhile and has to wait for this write.
                std::thread::sleep(std::time::Duration::from_millis(400));
                let mut now = known;
                now.insert("d".to_string());
                remember(&paths, &now, "2");
            })
        };
        held_rx.recv().expect("the first look holds the lock");
        let found = Watch::default().look(&paths, "4.3.1", &items, "3");
        first.join().expect("first look");
        assert!(found.is_empty(), "told a second time: {found:?}");
        assert_eq!(remembered(&paths), set(&["a", "b", "c", "d"]));
    }
}
