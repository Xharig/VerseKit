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

//! Where the player is: the last landing place from the Game.log, for the
//! location row of the overlay (like `location.py` of the Python version).
//!
//! ⚠ The change of place names the place only by a number. Its name shows
//! when the inventory is opened there (`RequestLocationInventory`, every
//! `Location:<number>` of the inventory lines in the eight lines after it) or
//! in an `ATC Location` line; those pairs are learned from the end of the
//! running log, or from every log (`learn_all`), into `orte-gelernt.json`.
//! Readable names come from `orte-namen.json`
//! (written from the game data); without it the key is shown with spaces.
//!
//! `Landing [0]` means: at no landing place, in flight or out in the open.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::{LazyLock, Mutex};

use regex::Regex;

use crate::jsonio::{self, Value};
use crate::language::{Phrase, t};
use crate::ordered::parallel_map;
use crate::paths::Paths;

pub const LEARNED_FILE: &str = "orte-gelernt.json";
pub const NAMES_FILE: &str = "orte-namen.json";
pub const NOWHERE: &str = "0";

static CHANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<Update Inventory Location> Player \[[^\]]*\] is changing location\. Landing \[(\d+)\] -> \[(\d+)\]")
        .expect("pattern")
});
static REQUEST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<RequestLocationInventory> Player\[[^\]]*\] requested inventory for Location\[([^\]]+)\]")
        .expect("pattern")
});
static PLACE_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Location:(\d+)").expect("pattern"));
static ATC: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ATC Location: ([A-Za-z][\w-]*) \[(\d+)\]").expect("pattern"));

/// The number of the rule by which [`learn_all`] names places; the read marks of the file count only for it.
const LEARN_RULE: i64 = 2;
/// The log lines after the name line in which the inventory of a place is named.
const NAME_WINDOW: usize = 8;
/// The lines that carry the number of a place as `Location:<number>`.
const INVENTORY_MARKS: [&str; 3] =
    ["<Query Inventory>", "<RequestInventory>", "<RequestOrCreatePersonalInventoryData>"];

/// One writer of the learned file at a time.
static LOCK: Mutex<()> = Mutex::new(());

/// (number, key) from log lines, in the order first seen; a later name for
/// the same number replaces the earlier one in its place (like a dict).
pub type Pairs = Vec<(String, String)>;

fn put(found: &mut Pairs, number: String, key: String) {
    match found.iter_mut().find(|(n, _)| *n == number) {
        Some((_, k)) => *k = key,
        None => found.push((number, key)),
    }
}

/// The lines as Python's `str.splitlines` cuts them.
fn py_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split(['\n', '\r', '\x0b', '\x0c', '\x1c', '\x1d', '\x1e', '\u{85}', '\u{2028}', '\u{2029}'])
}

/// The pairs of log lines: a name line names every place number that stands in an inventory line within the
/// next [`NAME_WINDOW`] lines, and an `ATC Location` line names its own number.
pub fn pairs_in<'a>(lines: impl IntoIterator<Item = &'a str>) -> Pairs {
    let mut found = Pairs::new();
    // The key of the last name line and how many lines after it still count.
    let mut pending: Option<(String, usize)> = None;
    for line in lines {
        if line.contains("<RequestLocationInventory>") {
            pending = REQUEST.captures(line).map(|c| (c[1].to_string(), NAME_WINDOW));
            continue;
        }
        if let Some((key, left)) = pending.as_mut() {
            if *left == 0 {
                pending = None;
            } else {
                *left -= 1;
                if INVENTORY_MARKS.iter().any(|mark| line.contains(mark))
                    && let Some(c) = PLACE_ID.captures(line)
                {
                    put(&mut found, c[1].to_string(), key.clone());
                }
            }
        }
        if line.contains("ATC Location:")
            && let Some(c) = ATC.captures(line)
        {
            put(&mut found, c[2].to_string(), c[1].to_string());
        }
    }
    found
}

/// The pairs of whole log files, read in the order given; a later name for the same number wins. A file
/// without a name line is not read line by line.
pub fn learn_pairs(files: &[String]) -> Pairs {
    let per_file = parallel_map(files, |path| {
        let Ok(bytes) = std::fs::read(path) else { return Pairs::new() };
        let text = String::from_utf8_lossy(&bytes);
        if !text.contains("<RequestLocationInventory>") && !text.contains("ATC Location:") {
            return Pairs::new();
        }
        pairs_in(py_lines(&text))
    });
    let mut found = Pairs::new();
    for pairs in per_file {
        for (number, key) in pairs {
            put(&mut found, number, key);
        }
    }
    found
}

/// The number of a log line that changes the place and the one it changes to: `(before, now)`.
pub fn change_in(line: &str) -> Option<(String, String)> {
    let c = CHANGE.captures(line)?;
    Some((c[1].to_string(), c[2].to_string()))
}

/// The learned places {number: key}; empty without a readable file.
pub fn learned(paths: &Paths) -> HashMap<String, String> {
    let Ok(data) = jsonio::read_json(&paths.app_file(LEARNED_FILE)) else { return HashMap::new() };
    match data.get("orte") {
        Some(Value::Object(places)) => {
            places.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect()
        }
        _ => HashMap::new(),
    }
}

/// Takes new pairs into the learned file, keeping what else it holds.
/// Returns how many places were new.
pub fn remember(paths: &Paths, pairs: &Pairs) -> usize {
    store(paths, pairs, &[])
}

/// The log files already read by [`learn_all`] {name: size}. The marks of another rule (the Python version
/// wrote its own into the same file) are not trusted: those logs were read the old way.
fn read_marks(paths: &Paths) -> HashMap<String, i64> {
    let Ok(data) = jsonio::read_json(&paths.app_file(LEARNED_FILE)) else { return HashMap::new() };
    if data.get("lernregel") != Some(&Value::int(LEARN_RULE)) {
        return HashMap::new();
    }
    match data.get("gelesen") {
        Some(Value::Object(marks)) => marks
            .iter()
            .filter_map(|(k, v)| match v {
                Value::Int(text) => text.parse().ok().map(|size| (k.clone(), size)),
                _ => None,
            })
            .collect(),
        _ => HashMap::new(),
    }
}

/// Learns the places from every one of the log files that is new or has grown since the last time, and
/// remembers which ones were read (name and size). Returns how many places were new.
pub fn learn_all(paths: &Paths, files: &[String]) -> usize {
    let known = read_marks(paths);
    let mut marks: Vec<(String, i64)> = Vec::new();
    let mut todo: Vec<String> = Vec::new();
    for path in files {
        let Ok(meta) = std::fs::metadata(path) else { continue };
        let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
        let name = paths.host.flavor.basename(path);
        if known.get(&name) == Some(&size) {
            continue;
        }
        marks.push((name, size));
        todo.push(path.clone());
    }
    store(paths, &learn_pairs(&todo), &marks)
}

/// The new pairs and the read marks go into the learned file; what else it holds stays.
fn store(paths: &Paths, pairs: &Pairs, marks: &[(String, i64)]) -> usize {
    if pairs.is_empty() && marks.is_empty() {
        return 0;
    }
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let target = paths.app_file(LEARNED_FILE);
    let mut data = match jsonio::read_json(&target) {
        Ok(Value::Object(map)) if matches!(map.iter().find(|(k, _)| k == "orte"), Some((_, Value::Object(_)))) => map,
        _ => vec![("orte".into(), Value::Object(Vec::new()))],
    };
    if !data.iter().any(|(k, _)| k == "gelesen") {
        data.push(("gelesen".into(), Value::Object(Vec::new())));
    }
    let Some((_, Value::Object(places))) = data.iter_mut().find(|(k, _)| k == "orte") else { return 0 };
    let fresh: Vec<&(String, String)> = pairs
        .iter()
        .filter(|(n, k)| places.iter().find(|(pn, _)| pn == n).and_then(|(_, v)| v.as_str()) != Some(k.as_str()))
        .collect();
    if fresh.is_empty() && marks.is_empty() {
        return 0;
    }
    let mut added = 0;
    for (number, key) in fresh {
        match places.iter_mut().find(|(pn, _)| pn == number) {
            Some((_, value)) => *value = Value::Str(key.clone()),
            None => {
                places.push((number.clone(), Value::Str(key.clone())));
                added += 1;
            }
        }
    }
    if !marks.is_empty() {
        let current = data.iter().any(|(k, v)| k == "lernregel" && *v == Value::int(LEARN_RULE));
        if let Some((_, Value::Object(read))) = data.iter_mut().find(|(k, _)| k == "gelesen") {
            if !current {
                read.clear();
            }
            for (name, size) in marks {
                jsonio::map_set(read, name, Value::int(*size));
            }
        }
        jsonio::map_set(&mut data, "lernregel", Value::int(LEARN_RULE));
    }
    let folder = paths.host.flavor.dirname(&target);
    if !folder.is_empty() && jsonio::make_dirs(&folder).is_err() {
        return 0;
    }
    let text = jsonio::dumps(&Value::Object(data), None, false);
    if jsonio::write_text_atomic(&target, &text, jsonio::Newline::Keep).is_err() {
        return 0;
    }
    added
}

/// The last change of place `(before, now)`, the pairs found in the read end of the file and the seconds of
/// the stamp on the line of the change (UTC; `None` without one). Read from the back in growing pieces: the
/// last change is nearly always within the last megabyte.
pub fn last_change(log_path: &str) -> Option<((String, String), Pairs, Option<i64>)> {
    let mut file = File::open(log_path).ok()?;
    let size = file.metadata().ok()?.len();
    let mut chunk: u64 = 1 << 20;
    loop {
        let start = size.saturating_sub(chunk);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut bytes = Vec::new();
        file.by_ref().take(size - start).read_to_end(&mut bytes).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        if let Some(found) = CHANGE.captures_iter(&text).last() {
            let pairs = pairs_in(py_lines(&text));
            let at = found.get(0).map_or(0, |m| m.start());
            let line_start = text[..at].rfind(['\n', '\r']).map_or(0, |i| i + 1);
            let since = crate::logtime::seconds(crate::logtime::time_of(&text[line_start..at]));
            return Some(((found[1].to_string(), found[2].to_string()), pairs, since));
        }
        if start == 0 {
            return None;
        }
        chunk = chunk.saturating_mul(4);
    }
}

/// The current place.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Place {
    /// The number from the log; `None` while no change stands in it.
    pub number: Option<String>,
    /// The learned key of that number.
    pub key: Option<String>,
    /// At no landing place.
    pub travelling: bool,
    /// The key of the place left last.
    pub before: Option<String>,
    /// When the change happened (seconds, UTC), from the stamp of its log line.
    pub since: Option<i64>,
}

/// Where the player is, from the running Game.log (or `log_path`); `None`
/// without a log. A missing name is looked for in the read end of the file
/// and remembered.
pub fn place(paths: &Paths, log_path: Option<&str>) -> Option<Place> {
    let log = match log_path {
        Some(p) => p.to_string(),
        None => paths.game_log(None)?,
    };
    let Some(((before, now), pairs, since)) = last_change(&log) else { return Some(Place::default()) };
    let mut places = learned(paths);
    let missing = [&now, &before].into_iter().any(|n| n != NOWHERE && !places.contains_key(n.as_str()));
    if missing && remember(paths, &pairs) > 0 {
        places = learned(paths);
    }
    let key_of = |n: &str| if n == NOWHERE { None } else { places.get(n).cloned() };
    Some(Place { key: key_of(&now), before: key_of(&before), travelling: now == NOWHERE, number: Some(now), since })
}

/// The readable names {key in lower case: name}; empty without the file.
pub fn names(paths: &Paths) -> HashMap<String, String> {
    let Ok(data) = jsonio::read_json(&paths.app_file(NAMES_FILE)) else { return HashMap::new() };
    match data.get("orte") {
        Some(Value::Object(names)) => {
            names.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect()
        }
        _ => HashMap::new(),
    }
}

/// `Stanton2_Orison` → `Orison`; without a name the key, only more readable.
pub fn display_name(key: &str, names: &HashMap<String, String>) -> String {
    names.get(&key.to_lowercase()).cloned().unwrap_or_else(|| key.replace('_', " "))
}

/// The row for the overlay, or `None` when there is nothing to show.
pub fn overlay_phrase(place: Option<&Place>, names: &HashMap<String, String>) -> Option<Phrase> {
    let place = place?;
    place.number.as_ref()?;
    if place.travelling {
        return Some(match &place.before {
            Some(before) => Phrase::new("ov_v_unterwegs").with("0", display_name(before, names)),
            None => Phrase::new("s_uv_unterwegs"),
        });
    }
    let name = match &place.key {
        Some(key) => display_name(key, names),
        None => t("s_uv_ort_unbekannt"),
    };
    Some(Phrase::new("ov_standort").with("0", name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\
<2026-10-21T10:00:00.000Z> <RequestLocationInventory> Player[Xy] requested inventory for Location[Stanton2_Orison]
<2026-10-21T10:00:01.000Z> <Query Inventory> Request[5] Inventory[1:Location:4711]
<2026-10-21T10:00:02.000Z> ATC Location: Stanton1_Lorville [815]
<2026-10-21T10:05:00.000Z> <Update Inventory Location> Player [Xy] is changing location. Landing [815] -> [4711]. Location [a] -> [b]
";

    #[test]
    fn pairs_come_from_inventory_and_atc_lines() {
        let pairs = pairs_in(LOG.lines());
        let pair = |n: &str, k: &str| (n.to_string(), k.to_string());
        assert_eq!(pairs, vec![pair("4711", "Stanton2_Orison"), pair("815", "Stanton1_Lorville")], "order of the log");
        // A later name for the same number replaces the earlier one in place.
        let again = pairs_in(["ATC Location: A_One [1]", "ATC Location: B_Two [2]", "ATC Location: C_Three [1]"]);
        assert_eq!(again, vec![pair("1", "C_Three"), pair("2", "B_Two")]);
        // A query without a request before it names nothing.
        assert!(pairs_in(["<Query Inventory> Request[1] Inventory[1:Location:9]"]).is_empty());
    }

    fn pair(n: &str, k: &str) -> (String, String) {
        (n.to_string(), k.to_string())
    }

    #[test]
    fn the_personal_inventory_line_names_the_place_too() {
        let lines = [
            "<RequestLocationInventory> Player[Xy] requested inventory for Location[Stanton4_NewBabbage]",
            "<RequestOrCreatePersonalInventoryData> Player[Xy] create/queried inventory request[Success] inventoryID[1:Location:3170699229]",
        ];
        assert_eq!(pairs_in(lines), vec![pair("3170699229", "Stanton4_NewBabbage")]);
        let old_form = [
            "<RequestLocationInventory> Player[Xy] requested inventory for Location[Stanton1_Lorville]",
            "<RequestInventory> Request[5] Inventory[1:Location:815]",
        ];
        assert_eq!(pairs_in(old_form), vec![pair("815", "Stanton1_Lorville")]);
    }

    #[test]
    fn every_number_within_the_window_gets_the_name() {
        let name = "<RequestLocationInventory> Player[Xy] requested inventory for Location[Stanton2_Orison]";
        let query = |n: u32| format!("<Query Inventory> Request[5] Inventory[1:Location:{n}]");
        let other = "something else";
        // Two numbers one after the other: both belong to the name.
        let two = [name.to_string(), query(11), other.to_string(), query(12)];
        assert_eq!(
            pairs_in(two.iter().map(String::as_str)),
            vec![pair("11", "Stanton2_Orison"), pair("12", "Stanton2_Orison")]
        );
        // The eighth line after the name still counts, the ninth does not.
        let mut lines = vec![name.to_string()];
        lines.extend((0..7).map(|_| other.to_string()));
        lines.push(query(18));
        lines.push(query(19));
        assert_eq!(pairs_in(lines.iter().map(String::as_str)), vec![pair("18", "Stanton2_Orison")]);
        // A line of another kind with a number in it names nothing.
        let queued = format!(
            "{name}\n<Inventory Mgmt Request Queued> Queued Request[1] Type[QueryInventory] Source Inventory[1:Location:77]"
        );
        assert!(pairs_in(queued.lines()).is_empty());
        // A new name line starts a new window.
        let again = [
            name.to_string(),
            query(21),
            "<RequestLocationInventory> Player[Xy] requested inventory for Location[Stanton1_Lorville]".to_string(),
            query(22),
        ];
        assert_eq!(
            pairs_in(again.iter().map(String::as_str)),
            vec![pair("21", "Stanton2_Orison"), pair("22", "Stanton1_Lorville")]
        );
        // The ATC line is read inside a window as well.
        let atc = [name.to_string(), "ATC Location: Stanton1_Lorville [815]".to_string(), query(23)];
        assert_eq!(
            pairs_in(atc.iter().map(String::as_str)),
            vec![pair("815", "Stanton1_Lorville"), pair("23", "Stanton2_Orison")]
        );
    }

    #[test]
    fn learning_all_logs_reads_each_one_once_and_keeps_the_marks() {
        use crate::legacy;
        use crate::paths::Host;
        use crate::pypath::Flavor;

        let root = std::env::temp_dir().join(format!("versekit-orte-alle-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&root);
        let (home, logs) = (root.join("home"), root.join("logs"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        let first = logs.join("a.log");
        let second = logs.join("b.log");
        std::fs::write(&first, LOG).unwrap();
        std::fs::write(&second, "<RequestLocationInventory> Player[Xy] requested inventory for Location[Stanton1_Lorville]\n<RequestOrCreatePersonalInventoryData> Player[Xy] create/queried inventory request[Success] inventoryID[1:Location:900]\n").unwrap();
        let files: Vec<String> = [&first, &second].iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let host = Host::new(Flavor::native(), &[(legacy::ENV_HOME, home.to_str().unwrap())], root.to_str().unwrap());
        let paths = Paths::new(host);
        // The Python version marked these logs as read by its own, narrower rule: that does not count.
        let marked = files
            .iter()
            .map(|f| format!("\"{}\": {}", paths.host.flavor.basename(f), std::fs::metadata(f).unwrap().len()))
            .collect::<Vec<_>>()
            .join(", ");
        std::fs::write(paths.app_file(LEARNED_FILE), format!("{{\"orte\": {{}}, \"gelesen\": {{{marked}}}}}")).unwrap();
        assert!(read_marks(&paths).is_empty());
        assert_eq!(learn_all(&paths, &files), 3, "4711, 815 and 900");
        let places = learned(&paths);
        assert_eq!(places.get("900").map(String::as_str), Some("Stanton1_Lorville"));
        assert_eq!(places.get("4711").map(String::as_str), Some("Stanton2_Orison"));
        let marks = read_marks(&paths);
        assert_eq!(marks.len(), 2);
        // Known by name and size: nothing is read again, even when the place was taken out by hand.
        assert_eq!(learn_all(&paths, &files), 0);
        // A file that grew is read again.
        std::fs::write(
            &second,
            format!("{}ATC Location: Stanton3_Area18 [901]\n", std::fs::read_to_string(&second).unwrap()),
        )
        .unwrap();
        assert_eq!(learn_all(&paths, &files), 1);
        assert_eq!(learned(&paths).get("901").map(String::as_str), Some("Stanton3_Area18"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_row_names_the_place_or_says_travelling() {
        let names: HashMap<String, String> = [("stanton2_orison".into(), "Orison".into())].into();
        let at = Place { number: Some("4711".into()), key: Some("Stanton2_Orison".into()), ..Place::default() };
        assert_eq!(overlay_phrase(Some(&at), &names), Some(Phrase::new("ov_standort").with("0", "Orison")));
        let away = Place {
            number: Some("0".into()),
            travelling: true,
            before: Some("Stanton1_Lorville".into()),
            ..Place::default()
        };
        assert_eq!(
            overlay_phrase(Some(&away), &names),
            Some(Phrase::new("ov_v_unterwegs").with("0", "Stanton1 Lorville"))
        );
        assert_eq!(overlay_phrase(Some(&Place::default()), &names), None, "no change in the log yet");
        assert_eq!(overlay_phrase(None, &names), None);
    }

    #[test]
    fn the_last_change_and_its_names_are_read_from_the_end() {
        let dir = std::env::temp_dir().join(format!("versekit-location-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("Game.log");
        // More than one megabyte of filler before the change: the reader grows its piece.
        let filler = "<2026-10-21T09:00:00.000Z> nothing to see here\n".repeat(30_000);
        std::fs::write(&log, format!("{LOG}{filler}")).unwrap();
        let ((before, now), pairs, since) = last_change(&log.to_string_lossy()).expect("a change");
        assert_eq!((before.as_str(), now.as_str()), ("815", "4711"));
        assert_eq!(pairs.len(), 2);
        // The stamp of the line of the change: 2026-10-21 10:05:00 UTC.
        assert_eq!(since, Some(1_792_577_100));
        // A line of the change without a stamp knows no time.
        std::fs::write(&log, "<Update Inventory Location> Player [Xy] is changing location. Landing [0] -> [12]\n")
            .unwrap();
        assert_eq!(last_change(&log.to_string_lossy()).map(|(_, _, since)| since), Some(None));
        std::fs::write(&log, "no change at all\n").unwrap();
        assert!(last_change(&log.to_string_lossy()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
