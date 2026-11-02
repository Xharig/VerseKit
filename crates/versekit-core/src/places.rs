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

//! The storage places (`places.py` of 4.12): stations, cities and outposts
//! of the game from the UEX terminal list, in `orte.json`.
//!
//! The list is a closed one: only places that have a storage in the game.
//! Reading it is `stock_data`; this module fetches it. It follows the patch:
//! stations come with a game build, not overnight. Without a network an old
//! list is used; without any list the place stays a free field.
//!
//! ⚠ A fetch writes `orte.json` anew. The places the player adds (a purchase of the log at a station the terminal
//! list does not name) are kept apart in `orte-eigene.json` and joined to the list where it is read.

use crate::jsonio::{self, Map, Value, map_set};
use crate::paths::Paths;
use crate::playtime::write_beside;
use crate::stock_data::{close_matches, lower};
use crate::stock_fetch::{Fetch, switched_off};
use crate::uex::{self, DAY, Store};

/// The endpoint of the terminals.
pub const SOURCE: &str = "https://api.uexcorp.uk/2.0/terminals";
/// The file in the data folder.
pub const CACHE: &str = "orte.json";
/// The format number.
pub const FORMAT: i64 = 1;
/// The emergency limit when the game build is unknown, in seconds.
pub const SHELF_LIFE: f64 = 30.0 * DAY;
/// The fields the name of a place is taken from, in this order.
pub const FIELDS: [&str; 3] = ["space_station_name", "city_name", "outpost_name"];

const STORE: Store = Store::new(CACHE, FORMAT, SHELF_LIFE).bound();

/// How old the stored list is in seconds, or `None`.
pub fn age(paths: &Paths, now: f64) -> Option<f64> {
    STORE.age(paths, now)
}

/// Fetches the list when it is missing or stale. `true` when a usable list is there after it.
pub fn update(paths: &Paths, now: f64, fetch: Fetch<'_>) -> bool {
    if switched_off(&paths.host) {
        return false;
    }
    if !STORE.stale(paths, now) {
        return true;
    }
    let Some(data) = uex::fetch("places", SOURCE, fetch) else { return false };
    let rows = uex::rows(&data);
    if rows.is_empty() {
        return false;
    }
    let mut names: Vec<String> = Vec::new();
    for row in rows {
        for field in FIELDS {
            let name = uex::stripped(row, field);
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
    }
    if names.is_empty() {
        return false;
    }
    names.sort_by(|a, b| lower(a).cmp(&lower(b)).then_with(|| a.cmp(b)));
    let list = Value::Array(names.into_iter().map(Value::Str).collect());
    STORE.save(paths, vec![uex::field("orte", list)], false, now).is_ok()
}

/// Suggestions for an input: the places that contain the text, else the
/// closest by spelling. The text and the places are compared without case.
pub fn similar(places: &[String], name: &str, most: usize) -> Vec<String> {
    let text = lower(crate::text::py_strip(name));
    if text.is_empty() {
        return Vec::new();
    }
    let hits: Vec<String> = places.iter().filter(|place| lower(place).contains(&text)).take(most).cloned().collect();
    if !hits.is_empty() {
        return hits;
    }
    close_matches(&text, places, most, 0.6)
}

/// The places the player added, apart from the fetched list.
pub const OWN_FILE: &str = "orte-eigene.json";
pub const OWN_FORMAT: i64 = 1;

/// A place the player added: its name and, when the program could tell, its star system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnPlace {
    pub name: String,
    pub system: Option<String>,
}

/// The added places in the file `path`; a missing, damaged or foreign file gives none.
pub fn own_in(path: &str) -> Vec<OwnPlace> {
    let Ok(data) = jsonio::read_json(path) else { return Vec::new() };
    if data.get("format") != Some(&Value::int(OWN_FORMAT)) {
        return Vec::new();
    }
    let Some(Value::Array(rows)) = data.get("orte") else { return Vec::new() };
    let mut out: Vec<OwnPlace> = Vec::new();
    for row in rows {
        let name = row.get("name").and_then(Value::as_str).map(crate::text::py_strip).unwrap_or_default();
        if name.is_empty() || out.iter().any(|p| lower(&p.name) == lower(name)) {
            continue;
        }
        let system = row.get("system").and_then(Value::as_str).map(str::to_string).filter(|s| !s.is_empty());
        out.push(OwnPlace { name: name.to_string(), system });
    }
    out
}

/// The added places of the data folder.
pub fn own(paths: &Paths) -> Vec<OwnPlace> {
    own_in(&paths.app_file(OWN_FILE))
}

fn save_own(paths: &Paths, places: &[OwnPlace]) -> bool {
    let rows = places
        .iter()
        .map(|p| {
            let mut row: Map = vec![("name".into(), Value::str(p.name.as_str()))];
            if let Some(system) = &p.system {
                map_set(&mut row, "system", Value::str(system.as_str()));
            }
            Value::Object(row)
        })
        .collect();
    let mut data: Map = Vec::new();
    map_set(&mut data, "orte", Value::Array(rows));
    write_beside(paths, OWN_FILE, &mut data, OWN_FORMAT, "places.save_own")
}

/// Adds the places that are not there yet (by name without case); `Some` with the ones added, `None` when the
/// file could not be written.
pub fn add_own(paths: &Paths, new: &[OwnPlace]) -> Option<Vec<OwnPlace>> {
    let mut places = own(paths);
    let mut added = Vec::new();
    for place in new {
        let name = crate::text::py_strip(&place.name);
        if name.is_empty() || places.iter().any(|p| lower(&p.name) == lower(name)) {
            continue;
        }
        let place = OwnPlace { name: name.to_string(), system: place.system.clone() };
        places.push(place.clone());
        added.push(place);
    }
    if added.is_empty() || save_own(paths, &places) { Some(added) } else { None }
}

/// Takes these names out of the added places (without case); false when the file could not be written.
pub fn remove_own(paths: &Paths, names: &[String]) -> bool {
    let places = own(paths);
    let kept: Vec<OwnPlace> =
        places.iter().filter(|p| !names.iter().any(|n| lower(n) == lower(&p.name))).cloned().collect();
    kept.len() == places.len() || save_own(paths, &kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn places() -> Vec<String> {
        ["Checkmate Station", "Lorville", "Pyro Gateway (Stanton)"].map(String::from).to_vec()
    }

    #[test]
    fn a_part_of_the_name_finds_the_place() {
        assert_eq!(similar(&places(), "pyro", 4), vec!["Pyro Gateway (Stanton)"]);
        assert_eq!(similar(&places(), "  ", 4), Vec::<String>::new());
    }

    #[test]
    fn a_typo_finds_the_closest_place() {
        assert_eq!(similar(&places(), "lorvile", 4), vec!["Lorville"]);
    }

    fn scratch(name: &str) -> (String, Paths) {
        use crate::legacy;
        use crate::paths::Host;
        use crate::pypath::Flavor;
        let home = std::env::temp_dir()
            .join(format!("versekit-orte-eigene-{}-{name}", crate::testdir::run_tag()))
            .to_string_lossy()
            .replace('\\', "/");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("folder");
        let host = Host::new(Flavor::native(), &[(legacy::ENV_HOME, &home)], &home);
        (home, Paths::new(host))
    }

    fn own_place(name: &str, system: Option<&str>) -> OwnPlace {
        OwnPlace { name: name.into(), system: system.map(str::to_string) }
    }

    #[test]
    fn added_places_are_kept_once_with_their_system_and_can_be_taken_out() {
        let (home, paths) = scratch("add");
        assert!(own(&paths).is_empty());
        let added = add_own(&paths, &[own_place("Checkmate", Some("Pyro")), own_place(" Pyro Gateway ", None)]);
        assert_eq!(added, Some(vec![own_place("Checkmate", Some("Pyro")), own_place("Pyro Gateway", None)]));
        // The same name in another case is no new place.
        assert_eq!(add_own(&paths, &[own_place("CHECKMATE", None)]), Some(Vec::new()));
        assert_eq!(own(&paths), vec![own_place("Checkmate", Some("Pyro")), own_place("Pyro Gateway", None)]);
        let text = std::fs::read_to_string(paths.app_file(OWN_FILE)).unwrap();
        assert!(text.contains("\"system\": \"Pyro\"") && text.contains("\"format\": 1"), "{text}");
        assert!(remove_own(&paths, &["checkmate".to_string()]));
        assert_eq!(own(&paths), vec![own_place("Pyro Gateway", None)]);
        let _ = std::fs::remove_dir_all(home);
    }

    #[test]
    fn a_damaged_or_foreign_file_of_added_places_gives_none() {
        let (home, paths) = scratch("kaputt");
        std::fs::write(paths.app_file(OWN_FILE), "{kaputt").unwrap();
        assert!(own(&paths).is_empty());
        std::fs::write(paths.app_file(OWN_FILE), r#"{"format": 2, "orte": [{"name": "X"}]}"#).unwrap();
        assert!(own(&paths).is_empty());
        std::fs::write(paths.app_file(OWN_FILE), r#"{"format": 1, "orte": [{"name": ""}, {"x": 1}, {"name": "Y"}]}"#)
            .unwrap();
        assert_eq!(own(&paths), vec![own_place("Y", None)]);
        let _ = std::fs::remove_dir_all(home);
    }
}
