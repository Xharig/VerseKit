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

//! The basket: what the player put into the shopping list from the shops, with
//! how many of each; templates, lists of parts with how many kept under a name
//! ("F8C loadout"), made empty or from the basket, changed part by part and
//! put on the list again; and the stops of the shopping route ticked off on
//! the way. New in v5 (4.12
//! knew no such thing), kept in a file of its own, `einkaufskorb.json`, so that
//! the plan of the ships (`hangar.json`) and its checks stay as they are.
//!
//! The items become lines of the shopping list ([`lines`]): bought, never
//! built, each with a path of its own (`korb:<code>`), so that the sum, the
//! grouping and the route of `cart.rs` take them as they are.

use std::sync::Mutex;

use crate::cart::{self, BUY, Kind, Line, Origin, Sources};
use crate::jsonio::{self, Map, Value};
use crate::paths::Paths;

/// The file of the basket (in the folder `Intern`).
pub const FILE: &str = "einkaufskorb.json";
/// The format written.
const FORMAT: i64 = 1;
/// The start of the path of a line from the basket.
pub const PATH_PREFIX: &str = "korb:";

/// One writer of the file at a time.
static LOCK: Mutex<()> = Mutex::new(());

/// One item: the part (its code in the shops), its name, how many, and whether it is bought.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub reference: String,
    pub name: String,
    pub count: i64,
    pub done: bool,
}

/// A template: parts with how many, kept under a name (`saetze` in the file; an empty one keeps
/// `"posten": []`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Set {
    pub name: String,
    pub items: Vec<Item>,
}

/// The whole basket.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Basket {
    pub items: Vec<Item>,
    pub sets: Vec<Set>,
    /// The stops of the route ticked off (`System|Place`, see [`cart::stop_key`]).
    pub stops_done: Vec<String>,
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn text(map: &Map, key: &str) -> String {
    field(map, key).and_then(Value::as_str).unwrap_or("").trim().to_string()
}

/// A code the shops can be asked for: no space, no quotation mark (as `shops.rs` asks).
fn usable(reference: &str) -> bool {
    !reference.is_empty() && !reference.contains(char::is_whitespace) && !reference.contains('"')
}

fn items_of(value: Option<&Value>) -> Vec<Item> {
    let Some(Value::Array(list)) = value else { return Vec::new() };
    let mut items: Vec<Item> = Vec::new();
    for entry in list.iter().filter_map(Value::as_object) {
        let reference = text(entry, "ref");
        if !usable(&reference) {
            continue;
        }
        let count = match field(entry, "anzahl") {
            Some(Value::Int(n)) => n.parse::<i64>().unwrap_or(1),
            Some(Value::Float(n)) => *n as i64,
            _ => 1,
        }
        .max(1);
        let done = matches!(field(entry, "erledigt"), Some(Value::Bool(true)));
        let name = Some(text(entry, "name")).filter(|n| !n.is_empty()).unwrap_or_else(|| reference.clone());
        match items.iter_mut().find(|i| i.reference == reference) {
            Some(same) => same.count += count,
            None => items.push(Item { reference, name, count, done }),
        }
    }
    items
}

fn item_value(item: &Item, with_done: bool) -> Value {
    let mut map: Map = vec![
        ("ref".into(), Value::str(item.reference.as_str())),
        ("name".into(), Value::str(item.name.as_str())),
        ("anzahl".into(), Value::int(item.count)),
    ];
    if with_done && item.done {
        map.push(("erledigt".into(), Value::Bool(true)));
    }
    Value::Object(map)
}

/// The basket in the file; empty without one or with one that cannot be read.
pub fn load(paths: &Paths) -> Basket {
    let Ok(Value::Object(data)) = jsonio::read_json(&paths.app_file(FILE)) else { return Basket::default() };
    let sets = match field(&data, "saetze") {
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|set| {
                let name = text(set, "name");
                (!name.is_empty()).then(|| Set { name, items: items_of(field(set, "posten")) })
            })
            .collect(),
        _ => Vec::new(),
    };
    let stops_done = match field(&data, "stopps_erledigt") {
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    Basket { items: items_of(field(&data, "posten")), sets, stops_done }
}

/// Writes the basket, whole and at once.
///
/// # Errors
///
/// When the file cannot be written.
pub fn save(paths: &Paths, basket: &Basket) -> std::io::Result<()> {
    let _guard = LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let sets = basket
        .sets
        .iter()
        .map(|set| {
            Value::Object(vec![
                ("name".into(), Value::str(set.name.as_str())),
                ("posten".into(), Value::Array(set.items.iter().map(|i| item_value(i, false)).collect())),
            ])
        })
        .collect();
    let data = Value::Object(vec![
        ("format".into(), Value::int(FORMAT)),
        ("posten".into(), Value::Array(basket.items.iter().map(|i| item_value(i, true)).collect())),
        ("saetze".into(), Value::Array(sets)),
        ("stopps_erledigt".into(), Value::Array(basket.stops_done.iter().map(|k| Value::str(k.as_str())).collect())),
    ]);
    let target = paths.app_file(FILE);
    let folder = paths.host.flavor.dirname(&target);
    if !folder.is_empty() {
        jsonio::make_dirs(&folder)?;
    }
    jsonio::write_text_atomic(&target, &jsonio::dumps(&data, Some(1), false), jsonio::Newline::Keep)
}

/// Puts `count` of a part into `items`; a part that is there already gets more (and is open again).
/// False for a code the shops cannot be asked for or a count below one.
fn add_item(items: &mut Vec<Item>, reference: &str, name: &str, count: i64) -> bool {
    let reference = reference.trim();
    if !usable(reference) || count < 1 {
        return false;
    }
    match items.iter_mut().find(|i| i.reference == reference) {
        Some(same) => {
            same.count += count;
            same.done = false;
        }
        None => items.push(Item {
            reference: reference.to_string(),
            name: if name.trim().is_empty() { reference.to_string() } else { name.trim().to_string() },
            count,
            done: false,
        }),
    }
    true
}

/// Sets how many of a part `items` hold; at zero it goes. True when something changed.
fn set_item_count(items: &mut Vec<Item>, reference: &str, count: i64) -> bool {
    let Some(at) = items.iter().position(|i| i.reference == reference) else { return false };
    if count < 1 {
        items.remove(at);
        return true;
    }
    let changed = items[at].count != count;
    items[at].count = count;
    changed
}

impl Basket {
    /// Puts `count` of a part in; a part that is there already gets more (and is open again).
    /// False for a code the shops cannot be asked for or a count below one.
    pub fn add(&mut self, reference: &str, name: &str, count: i64) -> bool {
        add_item(&mut self.items, reference, name, count)
    }

    /// Sets how many of a part; at zero it goes. True when something changed.
    pub fn set_count(&mut self, reference: &str, count: i64) -> bool {
        set_item_count(&mut self.items, reference, count)
    }

    /// Ticks a part off as bought, or takes the tick back. True when something changed.
    pub fn set_done(&mut self, reference: &str, done: bool) -> bool {
        match self.items.iter_mut().find(|i| i.reference == reference) {
            Some(item) if item.done != done => {
                item.done = done;
                true
            }
            _ => false,
        }
    }

    /// Ticks a stop of the route off, or takes the tick back. True when something changed.
    pub fn set_stop_done(&mut self, key: &str, done: bool) -> bool {
        let there = self.stops_done.iter().any(|k| k == key);
        match (done, there) {
            (true, false) => self.stops_done.push(key.to_string()),
            (false, true) => self.stops_done.retain(|k| k != key),
            _ => return false,
        }
        true
    }

    /// Keeps the items under `name` (a set of that name is replaced, its place stays). False for an empty
    /// name or an empty basket.
    pub fn save_set(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.items.is_empty() {
            return false;
        }
        let items: Vec<Item> = self.items.iter().map(|i| Item { done: false, ..i.clone() }).collect();
        match self.sets.iter_mut().find(|s| s.name == name) {
            Some(set) => set.items = items,
            None => self.sets.push(Set { name: name.to_string(), items }),
        }
        true
    }

    /// The items of the set `name` instead of the current ones, none ticked, and the stops not ticked
    /// either: a new round of shopping. False when there is no such set.
    pub fn load_set(&mut self, name: &str) -> bool {
        let Some(set) = self.sets.iter().find(|s| s.name == name.trim()) else { return false };
        self.items = set.items.iter().map(|i| Item { done: false, ..i.clone() }).collect();
        self.stops_done.clear();
        true
    }

    /// Takes the set `name` away. True when there was one.
    pub fn delete_set(&mut self, name: &str) -> bool {
        self.take_set(name).is_some()
    }

    /// The set `name`.
    pub fn set(&self, name: &str) -> Option<&Set> {
        self.sets.iter().find(|s| s.name == name.trim())
    }

    fn set_mut(&mut self, name: &str) -> Option<&mut Set> {
        self.sets.iter_mut().find(|s| s.name == name.trim())
    }

    /// Makes an empty set `name` at the end. False for an empty name or one another set has.
    pub fn new_set(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.set(name).is_some() {
            return false;
        }
        self.sets.push(Set { name: name.to_string(), items: Vec::new() });
        true
    }

    /// Puts `count` of a part into the set `name`; a part that is there already gets more. False without
    /// such a set, for a code the shops cannot be asked for or a count below one.
    pub fn add_to_set(&mut self, name: &str, reference: &str, part: &str, count: i64) -> bool {
        self.set_mut(name).is_some_and(|set| add_item(&mut set.items, reference, part, count))
    }

    /// Sets how many of a part the set `name` holds; at zero the part goes. True when something changed.
    pub fn set_count_in_set(&mut self, name: &str, reference: &str, count: i64) -> bool {
        self.set_mut(name).is_some_and(|set| set_item_count(&mut set.items, reference, count))
    }

    /// Gives the set `from` the name `to`; its place and its parts stay. False without such a set, for an
    /// empty or unchanged name, or one another set has.
    pub fn rename_set(&mut self, from: &str, to: &str) -> bool {
        let to = to.trim();
        if to.is_empty() || to == from.trim() || self.set(to).is_some() {
            return false;
        }
        match self.set_mut(from) {
            Some(set) => {
                set.name = to.to_string();
                true
            }
            None => false,
        }
    }

    /// Takes the set `name` out, with the place it stood at, so that it can be put back.
    pub fn take_set(&mut self, name: &str) -> Option<(usize, Set)> {
        let at = self.sets.iter().position(|s| s.name == name.trim())?;
        Some((at, self.sets.remove(at)))
    }

    /// Puts a set taken out back at its place (or at the end, where the list got shorter). False where a
    /// set of its name is there by now.
    pub fn restore_set(&mut self, at: usize, set: Set) -> bool {
        if self.set(&set.name).is_some() {
            return false;
        }
        self.sets.insert(at.min(self.sets.len()), set);
        true
    }
}

/// The path of the line of a part of the basket.
pub fn path_of(reference: &str) -> String {
    format!("{PATH_PREFIX}{reference}")
}

/// The items as lines of the shopping list: bought (a part of the basket is never built), with the prices
/// of the shops (`sources`).
pub fn lines(basket: &Basket, sources: &dyn Sources) -> Vec<Line> {
    basket
        .items
        .iter()
        .map(|item| {
            let path = path_of(&item.reference);
            Line {
                kind: Kind::Part,
                ship: String::new(),
                origin: Origin::Basket,
                position: String::new(),
                name: item.name.clone(),
                reference: item.reference.clone(),
                way: BUY.to_string(),
                buy: cart::buy_option(&item.reference, sources),
                craft: cart::craft_option("", "", sources),
                done: item.done,
                count: item.count,
                paths: vec![path.clone()],
                path,
                slot_kind: String::new(),
                size: None,
                factory_ref: String::new(),
                factory_name: String::new(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths_in(name: &str) -> (Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("versekit-korb-{}-{name}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let home = dir.to_string_lossy().into_owned();
        let vars = [(crate::legacy::ENV_HOME, home.as_str()), (crate::legacy::ENV_NO_NET, "1")];
        (Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, &home)), dir)
    }

    #[test]
    fn parts_go_in_with_how_many_and_the_same_part_adds_up() {
        let mut basket = Basket::default();
        assert!(basket.add("uuid-helm", "Morozov Helmet", 2));
        assert!(basket.add("uuid-helm", "Morozov Helmet", 1));
        assert!(basket.add("uuid-rifle", "", 1));
        assert!(!basket.add("bad ref", "X", 1) && !basket.add("uuid-x", "X", 0), "no code to ask, or none of it");
        assert_eq!(
            basket.items.iter().map(|i| (i.name.as_str(), i.count)).collect::<Vec<_>>(),
            [("Morozov Helmet", 3), ("uuid-rifle", 1)],
            "a part without a name is named by its code"
        );
        assert!(basket.set_done("uuid-helm", true));
        assert!(basket.add("uuid-helm", "Morozov Helmet", 1));
        assert!(!basket.items[0].done, "more of a bought part is open again");
        assert!(basket.set_count("uuid-helm", 5) && !basket.set_count("uuid-helm", 5));
        assert!(basket.set_count("uuid-rifle", 0));
        assert_eq!(basket.items.len(), 1, "zero takes the part out");
    }

    #[test]
    fn a_list_is_kept_under_a_name_and_loaded_again() {
        let mut basket = Basket::default();
        assert!(!basket.save_set("FPS-Set Schwer"), "nothing to keep");
        basket.add("uuid-helm", "Morozov Helmet", 1);
        basket.add("uuid-core", "Morozov Core", 1);
        basket.set_done("uuid-helm", true);
        assert!(!basket.save_set("  "));
        assert!(basket.save_set(" FPS-Set Schwer "));
        assert!(basket.set_stop_done("Stanton|Area18", true));
        basket.items.clear();
        basket.add("uuid-other", "Other", 4);
        assert!(basket.load_set("FPS-Set Schwer"));
        assert_eq!(
            basket.items.iter().map(|i| (i.reference.as_str(), i.done)).collect::<Vec<_>>(),
            [("uuid-helm", false), ("uuid-core", false)]
        );
        assert!(basket.stops_done.is_empty(), "a new round of shopping");
        // The same name again replaces the set in its place.
        basket.add("uuid-legs", "Morozov Legs", 1);
        assert!(basket.save_set("FPS-Set Schwer"));
        assert_eq!(basket.sets.len(), 1);
        assert_eq!(basket.sets[0].items.len(), 3);
        assert!(!basket.load_set("Nothing"));
        assert!(basket.delete_set("FPS-Set Schwer") && !basket.delete_set("FPS-Set Schwer"));
    }

    #[test]
    fn the_basket_is_kept_in_its_file_and_a_broken_file_reads_as_empty() {
        let (paths, dir) = paths_in("datei");
        assert_eq!(load(&paths), Basket::default());
        let mut basket = Basket::default();
        basket.add("uuid-helm", "Morozov Helmet", 2);
        basket.set_done("uuid-helm", true);
        basket.add("uuid-core", "Morozov Core", 1);
        basket.save_set("FPS-Set Schwer");
        basket.set_stop_done("Stanton|Area18", true);
        save(&paths, &basket).unwrap();
        assert_eq!(load(&paths), basket);
        std::fs::write(paths.app_file(FILE), "{ not json").unwrap();
        assert_eq!(load(&paths), Basket::default());
        // Read with care: a code that cannot be asked goes, a missing count is one, the same code adds up.
        std::fs::write(
            paths.app_file(FILE),
            r#"{"format": 1, "posten": [{"ref": "bad ref"}, {"ref": "a"}, {"ref": "a", "anzahl": 2}], "saetze": [{"name": ""}]}"#,
        )
        .unwrap();
        let read = load(&paths);
        assert_eq!(read.items.iter().map(|i| (i.reference.as_str(), i.count)).collect::<Vec<_>>(), [("a", 3)]);
        assert!(read.sets.is_empty(), "a set without a name is none");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_template_gets_parts_with_how_many_and_loses_them_again() {
        let mut basket = Basket::default();
        assert!(basket.new_set(" Rüstung Mamba "));
        assert!(!basket.new_set("Rüstung Mamba") && !basket.new_set("  "), "a name once, and never an empty one");
        assert!(basket.new_set("F8C Ausstattung"));
        assert_eq!(basket.set("Rüstung Mamba").map(|s| s.items.len()), Some(0));
        assert!(basket.add_to_set("Rüstung Mamba", "uuid-helm", "Morozov Helmet", 2));
        assert!(basket.add_to_set("Rüstung Mamba", "uuid-helm", "Morozov Helmet", 1));
        assert!(basket.add_to_set("Rüstung Mamba", "uuid-core", "", 1));
        assert!(!basket.add_to_set("Rüstung Mamba", "bad ref", "X", 1), "no code to ask");
        assert!(!basket.add_to_set("Rüstung Mamba", "uuid-x", "X", 0), "none of it");
        assert!(!basket.add_to_set("Nothing", "uuid-x", "X", 1), "no such template");
        let parts = |basket: &Basket, name: &str| -> Vec<(String, String, i64)> {
            basket
                .set(name)
                .expect("set")
                .items
                .iter()
                .map(|i| (i.reference.clone(), i.name.clone(), i.count))
                .collect()
        };
        assert_eq!(
            parts(&basket, "Rüstung Mamba"),
            [("uuid-helm".into(), "Morozov Helmet".into(), 3), ("uuid-core".into(), "uuid-core".into(), 1)],
            "the same part adds up; a part without a name is named by its code"
        );
        assert!(basket.set_count_in_set("Rüstung Mamba", "uuid-helm", 5));
        assert!(!basket.set_count_in_set("Rüstung Mamba", "uuid-helm", 5), "unchanged");
        assert!(!basket.set_count_in_set("Rüstung Mamba", "uuid-none", 2), "no such part");
        assert!(basket.set_count_in_set("Rüstung Mamba", "uuid-core", 0), "zero takes the part out");
        assert_eq!(parts(&basket, "Rüstung Mamba"), [("uuid-helm".into(), "Morozov Helmet".into(), 5)]);
        assert!(parts(&basket, "F8C Ausstattung").is_empty(), "the other template is untouched");
        assert!(basket.items.is_empty(), "the list is untouched");
    }

    #[test]
    fn inserting_a_template_puts_exactly_its_parts_on_the_list() {
        let mut basket = Basket::default();
        basket.new_set("F8C Ausstattung");
        basket.add_to_set("F8C Ausstattung", "uuid-cooler", "Frost Cooler", 2);
        basket.add_to_set("F8C Ausstattung", "uuid-shield", "Arc Shield", 1);
        basket.new_set("Rüstung Mamba");
        basket.add_to_set("Rüstung Mamba", "uuid-helm", "Morozov Helmet", 1);
        // On the list before: another part, a part of the template ticked and with another count, a stop.
        basket.add("uuid-other", "Other", 4);
        basket.add("uuid-cooler", "Frost Cooler", 7);
        basket.set_done("uuid-cooler", true);
        basket.set_stop_done("Stanton|Area18", true);
        let templates = basket.sets.clone();
        assert!(basket.load_set("F8C Ausstattung"));
        assert_eq!(
            basket.items,
            [
                Item { reference: "uuid-cooler".into(), name: "Frost Cooler".into(), count: 2, done: false },
                Item { reference: "uuid-shield".into(), name: "Arc Shield".into(), count: 1, done: false },
            ]
        );
        assert!(basket.stops_done.is_empty(), "a new round of shopping");
        assert_eq!(basket.sets, templates, "the templates stay as they were");
        assert!(!basket.load_set("Nothing"));
        // An empty template empties the list.
        basket.new_set("Leer");
        assert!(basket.load_set("Leer") && basket.items.is_empty());
    }

    #[test]
    fn a_template_is_renamed_and_a_deleted_one_comes_back_at_its_place() {
        let mut basket = Basket::default();
        for name in ["A", "B", "C"] {
            basket.new_set(name);
        }
        basket.add_to_set("B", "uuid-helm", "Helm", 2);
        assert!(!basket.rename_set("B", "C") && !basket.rename_set("B", " ") && !basket.rename_set("B", "B"));
        assert!(!basket.rename_set("Nothing", "D"));
        assert!(basket.rename_set("B", " Rüstung Mamba "));
        let names = |basket: &Basket| basket.sets.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&basket), ["A", "Rüstung Mamba", "C"], "the place stays");
        assert_eq!(basket.set("Rüstung Mamba").map(|s| s.items.len()), Some(1), "the parts stay");
        let (at, taken) = basket.take_set("Rüstung Mamba").expect("taken");
        assert_eq!((at, names(&basket)), (1, vec!["A".to_string(), "C".to_string()]));
        assert!(basket.take_set("Rüstung Mamba").is_none());
        assert!(basket.restore_set(at, taken.clone()));
        assert_eq!(names(&basket), ["A", "Rüstung Mamba", "C"], "back at its place");
        assert!(!basket.restore_set(at, taken), "not twice");
        assert!(basket.delete_set("A") && !basket.delete_set("A"));
    }

    #[test]
    fn templates_written_before_keep_working_and_survive_a_round_trip() {
        let (paths, dir) = paths_in("vorlagen");
        // A file as the basket wrote it before templates could be changed: a template saved from the list.
        let before = r#"{
 "format": 1,
 "posten": [
  {
   "ref": "uuid-helm",
   "name": "Morozov Helmet",
   "anzahl": 2,
   "erledigt": true
  }
 ],
 "saetze": [
  {
   "name": "FPS-Set Schwer",
   "posten": [
    {
     "ref": "uuid-helm",
     "name": "Morozov Helmet",
     "anzahl": 2
    },
    {
     "ref": "uuid-core",
     "name": "Morozov Core",
     "anzahl": 1
    }
   ]
  },
  {
   "name": "Handgemacht"
  }
 ],
 "stopps_erledigt": [
  "Stanton|Area18"
 ]
}"#;
        std::fs::write(paths.app_file(FILE), before).unwrap();
        let mut read = load(&paths);
        let parts: Vec<(&str, &str, i64)> =
            read.sets[0].items.iter().map(|i| (i.reference.as_str(), i.name.as_str(), i.count)).collect();
        assert_eq!(read.sets[0].name, "FPS-Set Schwer");
        assert_eq!(parts, [("uuid-helm", "Morozov Helmet", 2), ("uuid-core", "Morozov Core", 1)]);
        assert_eq!(
            (read.sets[1].name.as_str(), read.sets[1].items.len()),
            ("Handgemacht", 0),
            "a template without parts is an empty one"
        );
        assert_eq!(read.items.len(), 1);
        assert_eq!(read.stops_done, ["Stanton|Area18"]);
        // Written back unchanged, the file says the same as before, with no field lost or added.
        save(&paths, &read).unwrap();
        let written = std::fs::read_to_string(paths.app_file(FILE)).unwrap();
        let mut expected = jsonio::loads(before).unwrap();
        if let Value::Object(top) = &mut expected
            && let Some((_, Value::Array(sets))) = top.iter_mut().find(|(k, _)| k == "saetze")
            && let Some(Value::Object(hand)) = sets.get_mut(1)
        {
            hand.push(("posten".into(), Value::Array(Vec::new())));
        }
        assert_eq!(jsonio::loads(&written).unwrap(), expected, "{written}");
        // Changed and kept: the change is read back, the rest stays.
        assert!(read.add_to_set("Handgemacht", "uuid-legs", "Morozov Legs", 3));
        assert!(read.set_count_in_set("FPS-Set Schwer", "uuid-core", 0));
        save(&paths, &read).unwrap();
        assert_eq!(load(&paths), read);
        let _ = std::fs::remove_dir_all(&dir);
    }

    struct Shops;

    impl Sources for Shops {
        fn shop_known(&self, reference: &str) -> bool {
            reference == "uuid-helm"
        }
        fn cheapest(&self, _reference: &str) -> Option<(f64, String, String)> {
            Some((1200.0, "Garrity Defense".into(), "Area18".into()))
        }
        fn shops_for(&self, _reference: &str) -> Vec<cart::Offer> {
            vec![cart::Offer {
                shop: "Garrity Defense".into(),
                place: "Area18".into(),
                system: "Stanton".into(),
                price: 1200.0,
            }]
        }
        fn raw_buy_price(&self, _material: &str) -> f64 {
            0.0
        }
        fn recipe_for(&self, _reference: &str, _name: &str) -> Option<cart::Recipe> {
            None
        }
        fn ship_offers(&self, _name: &str) -> Vec<cart::Offer> {
            Vec::new()
        }
        fn slots(&self, _ship: &Map, _chosen: &Map) -> Vec<cart::Slot> {
            Vec::new()
        }
    }

    #[test]
    fn the_items_become_lines_the_sum_and_the_route_of_the_shopping_list_take() {
        let mut basket = Basket::default();
        basket.add("uuid-helm", "Morozov Helmet", 2);
        basket.add("uuid-rare", "Rare Thing", 1);
        let lines = lines(&basket, &Shops);
        assert_eq!(lines[0].path, "korb:uuid-helm");
        assert_eq!((lines[0].way.as_str(), lines[0].origin), (BUY, Origin::Basket));
        assert_eq!(lines[0].buy.price, Some(1200.0));
        assert_eq!(lines[1].buy.condition, cart::Condition::NotChecked, "a part the shops do not know");
        let total = cart::total(&lines);
        assert_eq!((total.total, total.open), (2400.0, 1), "two helmets; the rare one has no number");
        let (stops, _) = cart::route(&lines, &Shops);
        assert_eq!(stops.len(), 1);
        assert_eq!(cart::stop_key(&stops[0]), "Stanton|Area18");
    }
}
