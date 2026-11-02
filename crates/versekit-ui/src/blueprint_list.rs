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

//! The blueprint list: every blueprint of the catalog, what one has, what is
//! missing and where to get it. Search, state toggles with counts, filters,
//! groups per kind, a fixed legend, and per row the origin when opened.

use std::collections::{BTreeSet, HashMap, HashSet};

use egui::{Align, Layout, RichText, Sense, Ui, UiBuilder, Vec2};
use versekit_core::blueprints::{self as bp, Group};
use versekit_core::catalog_cache::{self, Blueprint, CatalogCache, Mission, Place, Way};
use versekit_core::collection::{Collection, NoCatalog};
use versekit_core::export::{self, FileExports};
use versekit_core::jsonio::{self, Value};
use versekit_core::language::{Arg, t, tf};
use versekit_core::overview::{self, Period};
use versekit_core::paths::{Paths, name_key};
use versekit_core::theme::{Color, IconTint, Scheme};
use versekit_core::{collection, formats, language};
use versekit_gamedata::list::{self, Pending};

use crate::layout::GAP;
use crate::widgets::{self, ButtonKind};
use crate::{color, icons};

/// The state toggles, in this order; their counts are worked out from the
/// other filters.
const STATES: [&str; 7] =
    ["filter_alle", "filter_habe", "filter_fehlt", "filter_merk", "filter_vorgemerkt", "filter_neu", "filter_deckel"];
/// Classes, sizes and grades always offered, also when no blueprint has them right now.
const FIXED_CLASSES: [&str; 5] = ["Military", "Stealth", "Industrial", "Civilian", "Competition"];
const FIXED_SIZES: [i64; 6] = [1, 2, 3, 4, 5, 6];
const FIXED_GRADES: [i64; 4] = [1, 2, 3, 4];
/// Contracts shown above the rows for a search of at least this many characters.
const CONTRACT_SEARCH_MIN: usize = 3;
const CONTRACTS_SHOWN: usize = 8;

/// The watch list file of 4.12 (`namen`: clicked stars, `eintraege`: own observations).
const WATCH_FILE: &str = "watchlist.json";
/// The hangar file of 4.12; its `merkzettel` are the farm notes.
const FARM_FILE: &str = "hangar.json";

/// A name as the farm notes compare it: lower case, only letters a-z and digits.
fn slim(text: &str) -> String {
    text.to_lowercase().chars().filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).collect()
}

/// No reverse lookup of translated names: the list hands over the catalog's own name.
pub(crate) struct NoEnglishNames;

impl versekit_core::phrases::EnglishNames for NoEnglishNames {
    fn english_names(&self, _name: &str) -> Vec<String> {
        Vec::new()
    }
}

/// An own observation of the watch list: a title and the patterns it waits for.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Observation {
    pub title: String,
    pub patterns: Vec<String>,
}

/// Everything the list shows, read once.
pub struct ListData {
    pub catalog: CatalogCache,
    /// Keys of the blueprints one has.
    pub have: HashSet<String>,
    /// When and from where each blueprint of the collection came, by key (entries with a readable time).
    pub received: HashMap<String, (i64, String)>,
    /// Keys of the blueprints on the watch list (clicked stars).
    pub watched: HashSet<String>,
    pub observations: Vec<Observation>,
    /// Items noted for farming, as [`slim`] names.
    pub noted: HashSet<String>,
    /// True for sample contents (nothing of the user's).
    pub sample: bool,
    /// The watch list file and its contents as read; `None` for the sample.
    watch_file: Option<String>,
    watch_data: Value,
    /// The paths of the data folder, for the collection and the farm notes;
    /// `None` for the sample.
    paths: Option<Paths>,
    /// A write of the watch list failed.
    pub save_failed: bool,
    caps: HashMap<String, (i64, Option<String>)>,
    size_kinds: HashSet<String>,
    grade_kinds: HashSet<String>,
    /// Category and subcategory per blueprint ([`bp::classify_blueprint`]: from the recipe where the catalog
    /// carries it).
    categories: Vec<(String, &'static str)>,
    /// Blueprints without a known way are left out of every catalog the list
    /// takes (`alle_bauplaene` off).
    hide_no_way: bool,
    /// The build of the catalog from the game data, while it runs: the list
    /// shows what it has until the result is taken over by [`ListData::poll`].
    building: Option<Pending>,
    /// Time and size of `bestand.json` as last read, and when that was looked at ([`ListData::follow_collection`]).
    collection_seen: Option<(std::time::SystemTime, u64)>,
    collection_looked: Option<f64>,
}

/// How often the list looks whether the collection was written elsewhere, in seconds.
pub const COLLECTION_LOOK: f64 = 1.0;

fn collection_stamp(paths: &Paths) -> Option<(std::time::SystemTime, u64)> {
    let meta = std::fs::metadata(paths.app_file(collection::FILE_NAME)).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

impl ListData {
    pub fn new(catalog: CatalogCache, have: HashSet<String>, watched: HashSet<String>, noted: HashSet<String>) -> Self {
        let mut data = ListData {
            catalog,
            have,
            received: HashMap::new(),
            watched,
            observations: Vec::new(),
            noted,
            sample: false,
            watch_file: None,
            watch_data: Value::Object(Vec::new()),
            paths: None,
            save_failed: false,
            caps: HashMap::new(),
            size_kinds: HashSet::new(),
            grade_kinds: HashSet::new(),
            categories: Vec::new(),
            hide_no_way: false,
            building: None,
            collection_seen: None,
            collection_looked: None,
        };
        data.derive();
        data
    }

    /// Works out what depends on the catalog alone.
    fn derive(&mut self) {
        self.caps = bp::reward_caps(&self.catalog);
        self.size_kinds = bp::kinds_with_spread(&self.catalog, |b| b.size);
        self.grade_kinds = bp::kinds_with_spread(&self.catalog, |b| b.grade);
        self.categories = self.catalog.blueprints.iter().map(bp::classify_blueprint).collect();
    }

    /// The list from the data folder: the catalog from the installed game
    /// (without one, the 4.12 cache), collection, watch list and farm notes.
    /// Blueprints without a known way only with `alle_bauplaene`. `None`
    /// without a catalog.
    ///
    /// Never waits for the game data: when the catalog has to be built again
    /// (after a patch), the list starts with the last one built from the
    /// archive, else the 4.12 cache, else an empty catalog, and
    /// [`ListData::poll`] takes the new one over once a thread has built it.
    pub fn load(paths: &Paths) -> Option<Self> {
        Self::load_with_supply(paths, std::sync::Arc::new(list::Runtime::default()))
    }

    /// Like [`ListData::load`], with the `supply` the build takes from the
    /// network.
    pub fn load_with_supply(paths: &Paths, supply: std::sync::Arc<dyn list::Supply>) -> Option<Self> {
        let archive = paths.archive_file().map(std::path::PathBuf::from);
        Self::load_archive(paths, archive.as_deref(), &paths.app_file(list::CACHE_FILE), supply)
    }

    /// Like [`ListData::load`], with the channel folder of the game (`None`:
    /// no installation) and the cache file of the catalog given.
    pub fn load_in(paths: &Paths, channel_folder: Option<&std::path::Path>, cache_file: &str) -> Option<Self> {
        Self::load_in_with(paths, channel_folder, cache_file, std::sync::Arc::new(list::Runtime::default()))
    }

    /// Like [`ListData::load_in`], with the `supply` the build takes from the
    /// network (scmdb's catalog and the known errors in the data).
    pub fn load_in_with(
        paths: &Paths,
        channel_folder: Option<&std::path::Path>,
        cache_file: &str,
        supply: std::sync::Arc<dyn list::Supply>,
    ) -> Option<Self> {
        let archive = channel_folder.map(|folder| folder.join(versekit_gamedata::ARCHIVE_NAME));
        Self::load_archive(paths, archive.as_deref(), cache_file, supply)
    }

    /// Like [`ListData::load_in_with`], with the archive file itself given
    /// (`None`: no archive).
    pub fn load_archive(
        paths: &Paths,
        archive: Option<&std::path::Path>,
        cache_file: &str,
        supply: std::sync::Arc<dyn list::Supply>,
    ) -> Option<Self> {
        let start = list::start_catalog_archive(paths, archive, cache_file, supply);
        Self::load_from(paths, start.catalog, start.rebuild.and_then(list::Rebuild::spawn))
    }

    /// Like [`ListData::load`], with the catalog to start with and the build
    /// that is running given.
    pub fn load_from(paths: &Paths, catalog: Option<CatalogCache>, building: Option<Pending>) -> Option<Self> {
        let catalog = match catalog {
            Some(catalog) => catalog,
            None if building.is_some() => CatalogCache::default(),
            None => return None,
        };
        let mut data = Self::load_with(paths, catalog);
        data.building = building;
        Some(data)
    }

    /// Whether the catalog is still being built from the game data.
    pub fn is_building(&self) -> bool {
        self.building.is_some()
    }

    /// Takes over a catalog built in the background; true when the list
    /// changed. Call it on every frame: it also has the build ask the window
    /// for a new frame when it is done.
    ///
    /// The open row of `state` stays open (by its blueprint, not its place),
    /// and a filter choice the new catalog no longer offers is dropped. A new
    /// catalog equal to the one shown changes nothing at all.
    pub fn poll(&mut self, ctx: &egui::Context, state: &mut ListState) -> bool {
        let Some(building) = &self.building else { return false };
        if !building.has_wake() {
            let ctx = ctx.clone();
            building.set_wake(move || ctx.request_repaint());
        }
        // Should a wake-up ever be missed, the window looks again soon.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        let list::Progress::Done(built) = building.poll() else { return false };
        self.building = None;
        built.is_some_and(|catalog| self.replace_catalog(catalog, state))
    }

    /// "Also blueprints with no known source" was switched (page Erkennung): the list and the progress follow
    /// at once, with the catalog from the cache file (no waiting, no network). False when nothing changed.
    pub fn follow_all_setting(&mut self, paths: &Paths, state: &mut ListState) -> bool {
        self.reload_have(paths);
        let all = paths.setting_bool("alle_bauplaene", false);
        match list::start_catalog(paths).catalog {
            Some(catalog) => self.follow_all(catalog, all, state),
            None => {
                self.hide_no_way = !all;
                false
            }
        }
    }

    /// Reads the collection again: another one has written it (the page Erkennung took foreign blueprints out).
    pub fn reload_have(&mut self, paths: &Paths) {
        self.collection_seen = collection_stamp(paths);
        match jsonio::read_json(&paths.app_file(collection::FILE_NAME)).ok() {
            Some(Value::Object(map)) => self.take_collection(&map),
            _ => self.take_collection(&jsonio::Map::new()),
        }
    }

    /// Reads the collection again when it was written since it was read: the overlay writes a find in a process
    /// of its own, and the list and the progress show it without being opened anew. Looked at every
    /// [`COLLECTION_LOOK`] seconds of `now` (the time of the frame). True when it was read again.
    pub fn follow_collection(&mut self, now: f64) -> bool {
        if self.collection_looked.is_some_and(|at| (0.0..COLLECTION_LOOK).contains(&(now - at))) {
            return false;
        }
        self.collection_looked = Some(now);
        let Some(paths) = self.paths.take() else { return false };
        let changed = collection_stamp(&paths) != self.collection_seen;
        if changed {
            self.reload_have(&paths);
        }
        self.paths = Some(paths);
        changed
    }

    /// What one has and when it came, from the blueprints of a collection.
    fn take_collection(&mut self, data: &jsonio::Map) {
        self.have = collection::keys(data).into_iter().collect();
        self.received = received_by_key(collection::blueprints(data));
    }

    /// [`ListData::follow_all_setting`] with the catalog and the switch given.
    fn follow_all(&mut self, catalog: CatalogCache, all: bool, state: &mut ListState) -> bool {
        self.hide_no_way = !all;
        self.replace_catalog(catalog, state)
    }

    /// Shows `catalog` instead of the one shown; false when they are equal.
    fn replace_catalog(&mut self, mut catalog: CatalogCache, state: &mut ListState) -> bool {
        if self.hide_no_way {
            catalog.blueprints.retain(|b| !b.no_way);
        }
        if catalog == self.catalog {
            return false;
        }
        let open_key = state.open.and_then(|i| self.catalog.blueprints.get(i)).map(|b| b.key.clone());
        self.catalog = catalog;
        self.derive();
        state.open = open_key.and_then(|key| self.catalog.blueprints.iter().position(|b| b.key == key));
        if state.open.is_none() {
            state.more_ways = false;
        }
        // The kind first: the offers of the others follow from it.
        for f in 0..state.picks.len() {
            let choices = filter_choices(self, &state.picks[0]);
            if !state.picks[f].is_empty() && !choices[f].iter().any(|(value, _)| *value == state.picks[f]) {
                state.picks[f].clear();
            }
        }
        true
    }

    /// Reads the farm notes again: another page may have put an item on them or struck it.
    pub fn reload_noted(&mut self) {
        let Some(paths) = &self.paths else { return };
        if let Ok(Value::Object(file)) = jsonio::read_json(&paths.app_file(FARM_FILE))
            && let Some((_, Value::Array(items))) = file.iter().find(|(k, _)| k == "merkzettel")
        {
            self.noted = items.iter().filter_map(|m| m.get("name").and_then(Value::as_str)).map(slim).collect();
        } else {
            self.noted.clear();
        }
    }

    /// Like [`ListData::load`], with the catalog given.
    pub fn load_with(paths: &Paths, mut catalog: CatalogCache) -> Self {
        let hide_no_way = !paths.setting_bool("alle_bauplaene", false);
        if hide_no_way {
            catalog.blueprints.retain(|b| !b.no_way);
        }
        let read = |file: &str| jsonio::read_json(&paths.app_file(file)).ok();
        // Taken before the reading: a write in between is seen at the next look.
        let seen = collection_stamp(paths);
        let (have, received) = match read(collection::FILE_NAME) {
            Some(Value::Object(map)) => {
                (collection::keys(&map).into_iter().collect(), received_by_key(collection::blueprints(&map)))
            }
            _ => (HashSet::new(), HashMap::new()),
        };
        let watch = read(WATCH_FILE);
        let watched = match watch.as_ref().and_then(|w| w.get("namen")) {
            Some(Value::Array(names)) => names.iter().filter_map(Value::as_str).map(name_key).collect(),
            _ => HashSet::new(),
        };
        let observations = match watch.as_ref().and_then(|w| w.get("eintraege")) {
            Some(Value::Array(entries)) => entries
                .iter()
                .map(|e| Observation {
                    title: e.get("titel").and_then(Value::as_str).unwrap_or_default().to_string(),
                    patterns: match e.get("muster") {
                        Some(Value::Array(p)) => {
                            p.iter().filter_map(Value::as_str).map(|s| s.trim().to_lowercase()).collect()
                        }
                        _ => Vec::new(),
                    },
                })
                .collect(),
            _ => Vec::new(),
        };
        let noted = match read(FARM_FILE).as_ref().and_then(|h| h.get("merkzettel")) {
            Some(Value::Array(items)) => {
                items.iter().filter_map(|m| m.get("name").and_then(Value::as_str)).map(slim).collect()
            }
            _ => HashSet::new(),
        };
        let mut data = ListData::new(catalog, have, watched, noted);
        data.collection_seen = seen;
        data.received = received;
        data.hide_no_way = hide_no_way;
        data.observations = observations;
        data.watch_file = Some(paths.app_file(WATCH_FILE));
        if let Some(watch @ Value::Object(_)) = watch {
            data.watch_data = watch;
        }
        data.paths = Some(Paths::new(paths.host.clone()));
        data
    }

    /// The data folder of the list, with own data.
    pub(crate) fn paths(&self) -> Option<&Paths> {
        self.paths.as_ref()
    }

    /// The list with a data folder of its own (tests).
    #[cfg(test)]
    pub(crate) fn with_paths(mut self, paths: Paths) -> ListData {
        self.paths = Some(paths);
        self
    }

    /// Have it or not, written at once into the collection as 4.12 does
    /// (`bestand.json`, source `hand`, with its previous version), followed
    /// by the output files in the export folder.
    fn toggle_have(&mut self, i: usize) {
        self.save_failed = false;
        let Some(paths) = &self.paths else {
            let key = self.catalog.blueprints[i].key.clone();
            if self.have.remove(&key) {
                self.received.remove(&key);
            } else {
                self.received.insert(key.clone(), (export::now_unix(), "hand".to_string()));
                self.have.insert(key);
            }
            return;
        };
        let exports = FileExports { paths, version: env!("CARGO_PKG_VERSION"), clock: export::now_unix };
        let collection = Collection {
            paths,
            catalog: &NoCatalog,
            names: &NoEnglishNames,
            exports: &exports,
            clock: collection::now_text,
        };
        let name = self.catalog.blueprints[i].name.clone();
        // Read, change and write as one step.
        let _lock = versekit_core::jsonio::lock_saves();
        let mut stock = collection.load();
        if collection::contains(&stock, &name) {
            collection::remove(&mut stock, &name);
        } else {
            collection.add(&mut stock, &name, "hand", None);
        }
        if collection.save(&mut stock) {
            self.take_collection(&stock);
        } else {
            self.save_error("blueprint_list.save_collection", &"the collection could not be written");
        }
    }

    /// A write that failed: the page says so (until the next write) and the
    /// error log keeps it for the error report.
    fn save_error(&mut self, label: &str, error: &dyn std::fmt::Display) {
        self.save_failed = true;
        versekit_core::errors::record(label, error);
    }

    /// On the farm notes or off, written at once into `hangar.json` as 4.12
    /// does (indent 1, no backup): a new item with count 1 and `bauen`.
    fn toggle_noted(&mut self, i: usize) {
        self.save_failed = false;
        let name = self.catalog.blueprints[i].name.trim().to_string();
        let wanted = slim(&name);
        let on = !self.noted.remove(&wanted);
        if on {
            self.noted.insert(wanted.clone());
        }
        let Some(paths) = &self.paths else { return };
        let file = paths.app_file(FARM_FILE);
        // One writer of `hangar.json` at a time; a file there that is no
        // hangar is not overwritten.
        let _lock = versekit_core::hangar::lock();
        let mut hangar = match versekit_core::hangar::read(&file) {
            Some(map) => map,
            None if std::path::Path::new(&file).exists() => {
                self.save_error("blueprint_list.save_notes", &"the farm notes file is no hangar and stays as it is");
                return;
            }
            None => versekit_core::hangar::empty(),
        };
        if !matches!(hangar.iter().find(|(k, _)| k == "merkzettel"), Some((_, Value::Array(_)))) {
            jsonio::map_set(&mut hangar, "merkzettel", Value::Array(Vec::new()));
        }
        if let Some((_, Value::Array(notes))) = hangar.iter_mut().find(|(k, _)| k == "merkzettel") {
            let same = |m: &Value| m.get("name").and_then(Value::as_str).is_some_and(|n| slim(n) == wanted);
            if on {
                if !notes.iter().any(same) {
                    notes.push(Value::Object(vec![
                        ("name".into(), Value::str(&name)),
                        ("ref".into(), Value::str("")),
                        ("anzahl".into(), Value::int(1)),
                        ("weg".into(), Value::str("bauen")),
                    ]));
                }
            } else {
                notes.retain(|m| !same(m));
            }
        }
        if !versekit_core::hangar::write(&file, &hangar) {
            self.save_error("blueprint_list.save_notes", &"the farm notes could not be written");
        }
    }

    /// Writes the watch list as 4.12 does (`json.dump`, indent 1, no
    /// backup file); the sample writes nothing.
    fn save_watchlist(&mut self) {
        let Some(file) = &self.watch_file else { return };
        self.save_failed = false;
        let text = jsonio::dumps(&self.watch_data, Some(1), false);
        if let Err(error) = jsonio::write_text_atomic(file, &text, jsonio::Newline::Native) {
            let file = file.clone();
            self.save_error("blueprint_list.save_watch_list", &format!("{file}: {error}"));
        }
    }

    /// The list of a field of the watch list file, created if missing.
    fn watch_list_mut(&mut self, field: &str) -> &mut Vec<Value> {
        if !matches!(self.watch_data, Value::Object(_)) {
            self.watch_data = Value::Object(Vec::new());
        }
        let Value::Object(map) = &mut self.watch_data else { unreachable!("made an object above") };
        if !matches!(map.iter().find(|(k, _)| k == field), Some((_, Value::Array(_)))) {
            jsonio::map_set(map, field, Value::Array(Vec::new()));
        }
        match map.iter_mut().find(|(k, _)| k == field) {
            Some((_, Value::Array(items))) => items,
            _ => unreachable!("set above"),
        }
    }

    /// Star on or off, written at once. Off also drops the observations
    /// whose pattern meets the blueprint, as 4.12 does.
    fn toggle_watch(&mut self, i: usize) {
        let (key, name) = (self.catalog.blueprints[i].key.clone(), self.catalog.blueprints[i].name.clone());
        if self.watched.remove(&key) {
            self.watch_list_mut("namen").retain(|n| n.as_str().is_none_or(|n| name_key(n) != key));
            let meets = |o: &Observation| o.patterns.iter().any(|p| at_word_bounds(&key, p));
            let dropped: Vec<String> = self.observations.iter().filter(|o| meets(o)).map(|o| o.title.clone()).collect();
            self.observations.retain(|o| !meets(o));
            self.watch_list_mut("eintraege")
                .retain(|e| !dropped.iter().any(|t| e.get("titel").and_then(Value::as_str) == Some(t.as_str())));
        } else {
            self.watched.insert(key);
            self.watch_list_mut("namen").push(Value::str(name.trim()));
        }
        self.save_watchlist();
    }

    /// Drops own observation `k`, written at once.
    fn drop_observation(&mut self, k: usize) {
        if k >= self.observations.len() {
            return;
        }
        let title = self.observations.remove(k).title;
        let entries = self.watch_list_mut("eintraege");
        if let Some(at) = entries.iter().position(|e| e.get("titel").and_then(Value::as_str) == Some(title.as_str())) {
            entries.remove(at);
        }
        self.save_watchlist();
    }

    /// Made-up contents for the preview to pass on: the kinds of the game,
    /// sizes, grades and classes only where the game has them.
    pub fn sample() -> Self {
        let catalog = sample_catalog();
        let have =
            catalog.blueprints.iter().enumerate().filter(|(i, _)| i % 3 != 0).map(|(_, b)| b.key.clone()).collect();
        let watched = catalog.blueprints.iter().step_by(11).map(|b| b.key.clone()).collect();
        let noted = catalog.blueprints.iter().step_by(17).map(|b| slim(&b.name)).collect();
        let mut data = ListData::new(catalog, have, watched, noted);
        // Made-up receipt times spread over about six weeks, from every source.
        let now = export::now_unix();
        data.received = data
            .catalog
            .blueprints
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 3 != 0)
            .map(|(i, b)| {
                let source = ["log", "hand", "basetool", "import", "start", "nachlese", "launcher"][i % 7];
                (b.key.clone(), (now - ((i as i64 * 53) % 1000) * 3600, source.to_string()))
            })
            .collect();
        data.sample = true;
        data
    }

    /// Per kind category: (key, name, have, total), in the order of the kind filter.
    pub fn progress(&self) -> Vec<(String, String, usize, usize)> {
        let mut per: HashMap<&str, (usize, usize)> = HashMap::new();
        for (blueprint, (category, _)) in self.catalog.blueprints.iter().zip(&self.categories) {
            let entry = per.entry(category.as_str()).or_default();
            entry.1 += 1;
            if self.have.contains(&blueprint.key) {
                entry.0 += 1;
            }
        }
        let mut list: Vec<(String, String, usize, usize)> =
            per.into_iter().map(|(k, (have, total))| (k.to_string(), bp::category_name(k), have, total)).collect();
        list.sort_by_key(|(k, name, ..)| {
            (bp::TOP_CATEGORIES.iter().position(|c| c == k).unwrap_or(bp::TOP_CATEGORIES.len()), name.to_lowercase())
        });
        list
    }

    /// On the watch list: a clicked star, or a pattern of an own observation
    /// at word boundaries.
    fn is_watched(&self, blueprint: &Blueprint) -> bool {
        self.watched.contains(&blueprint.key)
            || self.observations.iter().any(|o| o.patterns.iter().any(|p| at_word_bounds(&blueprint.key, p)))
    }
}

/// When and from where each entry of a collection came, by key; entries without a readable time are left out.
fn received_by_key(entries: &[(String, Value)]) -> HashMap<String, (i64, String)> {
    overview::received(entries).into_iter().map(|r| (r.key, (r.at, r.source))).collect()
}

/// Does `pattern` stand in `text` with no letter or digit right before or after it?
fn at_word_bounds(text: &str, pattern: &str) -> bool {
    if pattern.is_empty() {
        return false;
    }
    let edge = |c: Option<char>| !c.is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    text.match_indices(pattern)
        .any(|(at, _)| edge(text[..at].chars().next_back()) && edge(text[at + pattern.len()..].chars().next()))
}

/// What the list is set to.
#[derive(Default)]
pub struct ListState {
    pub search: String,
    /// Index into `STATES`.
    pub state: usize,
    /// Picked value per filter: kind, subkind, class, size, source, grade, patch
    /// (empty = all).
    pub picks: [String; 7],
    /// Only the blueprints one received in this period (the list then stands flat, newest first).
    pub period: Period,
    /// Only the blueprints of this contract (from the contract overview).
    pub contract: Option<String>,
    /// The keys of the contract's blueprints when a jump brought them: a
    /// title from the game (with the place filled in) is not the one in the
    /// catalog (with a placeholder), so the list filters by these.
    pub contract_keys: Option<BTreeSet<String>>,
    /// The blueprint whose row is open (index into the catalog).
    pub open: Option<usize>,
    /// The further ways of the open row are unfolded.
    pub more_ways: bool,
    /// Height of the open row's detail, measured on the frame before.
    detail_height: f32,
    /// The first blueprint the list showed in the last frame (index into the catalog).
    shown_first: Option<usize>,
    /// A jump to the top is due (search or filter changed).
    to_top: bool,
    /// Where a jump into the list came from, shown with ✕ to loosen it.
    pub jump: Option<String>,
    /// Only blueprints of this kind, by its readable name (a jump from the progress).
    pub kind: Option<String>,
    /// The blueprint whose crafting page was asked for; the window takes it.
    pub craft_jump: Option<String>,
    /// The blueprints (by key) that have a recipe on the crafting page: only
    /// those get the button to it. Set by the window.
    pub craftable: std::sync::Arc<HashSet<String>>,
}

/// Where a jump into the list leads.
pub enum ListJump {
    /// A category of the kind filter.
    Category(String),
    /// A kind by its readable name, as the progress groups them.
    Kind(String),
    /// The blueprints of one contract: its title and the keys of its blueprints.
    Contract(String, BTreeSet<String>),
    /// The blueprints received in a period.
    Received(Period),
}

impl ListState {
    /// Something narrows the list: search, state toggle, a filter or a jump.
    pub fn is_narrowed(&self) -> bool {
        !self.search.trim().is_empty()
            || self.state != 0
            || self.contract.is_some()
            || self.kind.is_some()
            || self.period != Period::All
            || self.picks.iter().any(|p| !p.is_empty())
    }

    /// Back to all blueprints; the open row stays open.
    pub fn reset(&mut self) {
        *self = ListState { open: self.open, to_top: true, ..ListState::default() };
    }

    /// Opens the row of the first blueprint the list showed in the last frame, as a click on it does. Whether
    /// there was one.
    pub fn choose_first(&mut self) -> bool {
        self.open = self.shown_first;
        self.open.is_some()
    }

    /// A category was picked in the kind filter: it replaces a jump to a kind of the progress (both kept, the list
    /// would show what lies in both, mostly nothing, while the menu counts the category) and the subcategory of
    /// the category before. A contract stays and keeps the label that loosens it.
    pub fn category_picked(&mut self) {
        self.picks[1].clear();
        self.kind = None;
        if self.contract.is_none() {
            self.jump = None;
        }
    }

    /// Opens the list on what `to` names, as a jump from `from`.
    pub fn jump(&mut self, to: ListJump, from: String) {
        let mut state = ListState { jump: Some(from), to_top: true, ..ListState::default() };
        match to {
            ListJump::Category(key) => state.picks[0] = key,
            ListJump::Kind(kind) => state.kind = Some(kind),
            ListJump::Contract(title, keys) => {
                state.contract = Some(title);
                state.contract_keys = Some(keys);
            }
            ListJump::Received(period) => state.period = period,
        }
        *self = state;
    }
}

/// Sub-line of a row: class, size and grade where they mean something for
/// that kind, then the manufacturer.
fn sub_line(data: &ListData, blueprint: &Blueprint) -> String {
    let kind = bp::kind_id(&blueprint.kind);
    let mut parts: Vec<String> = Vec::new();
    if let Some(class) = &blueprint.class {
        parts.push(class.clone());
    }
    if let Some(size) = blueprint.size.filter(|_| data.size_kinds.contains(kind)) {
        parts.push(format!("S{size}"));
    }
    if let Some(letter) =
        blueprint.grade.filter(|_| data.grade_kinds.contains(kind)).and_then(catalog_cache::grade_letter)
    {
        parts.push(letter.to_string());
    }
    parts.extend(blueprint.maker.clone());
    parts.join(" · ")
}

/// The id of a period in its drop-down; empty for all of them, like the all choice of the other filters.
fn period_id(period: Period) -> &'static str {
    match period {
        Period::All => "",
        Period::Today => "today",
        Period::Days7 => "7",
        Period::Days14 => "14",
        Period::Month => "month",
    }
}

/// The text key of a period's name.
pub fn period_key(period: Period) -> &'static str {
    match period {
        Period::All => "erhalten_alle",
        Period::Today => "erhalten_heute",
        Period::Days7 => "erhalten_7",
        Period::Days14 => "erhalten_14",
        Period::Month => "erhalten_monat",
    }
}

/// Is `text` the name of a period, as a jump into the list labels itself?
fn is_period_label(text: &str) -> bool {
    Period::EVERY.iter().any(|&period| text == t(period_key(period)))
}

/// The text key of the name of a source in the collection; `None` for one that is not known.
fn source_key(source: &str) -> Option<&'static str> {
    match source {
        "log" | "nachlese" => Some("erhalten_q_spiel"),
        "basetool" => Some("erhalten_q_basetool"),
        "import" => Some("erhalten_q_import"),
        "launcher" => Some("erhalten_q_launcher"),
        "hand" => Some("erhalten_q_hand"),
        "start" => Some("erhalten_q_start"),
        _ => None,
    }
}

/// Newest receipt first; blueprints of the same second by name.
fn newest_first(data: &ListData, a: usize, b: usize) -> std::cmp::Ordering {
    let (a, b) = (&data.catalog.blueprints[a], &data.catalog.blueprints[b]);
    let at = |blueprint: &Blueprint| data.received.get(&blueprint.key).map_or(i64::MIN, |(at, _)| *at);
    at(b).cmp(&at(a)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(|| a.key.cmp(&b.key))
}

/// When and from where blueprint `i` came, for its row: "29.10.2026 02:50 · Game"; `None` without a time.
fn received_text(data: &ListData, i: usize) -> Option<String> {
    let (at, source) = data.received.get(&data.catalog.blueprints[i].key)?;
    let when = formats::date_time(language::current(), *at as f64);
    Some(match source_key(source) {
        Some(key) => format!("{when} · {}", t(key)),
        None => when,
    })
}

/// How many blueprints received since `since` are not in the catalog the list shows (the tile counts them).
fn outside_catalog(data: &ListData, since: i64) -> usize {
    let known: HashSet<&str> = data.catalog.blueprints.iter().map(|b| b.key.as_str()).collect();
    data.received.iter().filter(|(key, (at, _))| *at >= since && !known.contains(key.as_str())).count()
}

/// Did blueprint `blueprint` come to the player at or after `since`?
fn received_since(data: &ListData, blueprint: &Blueprint, since: i64) -> bool {
    data.have.contains(&blueprint.key) && data.received.get(&blueprint.key).is_some_and(|(at, _)| *at >= since)
}

/// How many blueprints of the catalog the player received in each period of [`Period::EVERY`] at `now`.
fn period_counts(data: &ListData, now: i64) -> [usize; 5] {
    Period::EVERY.map(|period| match period.since(now) {
        None => data.catalog.blueprints.len(),
        Some(since) => data.catalog.blueprints.iter().filter(|b| received_since(data, b, since)).count(),
    })
}

/// Does blueprint `i` pass every filter except the state toggle? `since` is where the period begins (`None`
/// for all of them).
fn passes(data: &ListData, state: &ListState, i: usize, search: &str, since: Option<i64>) -> bool {
    let blueprint = &data.catalog.blueprints[i];
    let (category, subcategory) = &data.categories[i];
    let kind = bp::kind_id(&blueprint.kind);
    let [pick_kind, pick_sub, pick_class, pick_size, pick_source, pick_grade, pick_patch] = &state.picks;
    if !search.is_empty() && !bp::hits_kind(&blueprint.kind, search) && !bp::matches(blueprint, search) {
        return false;
    }
    if since.is_some_and(|since| !received_since(data, blueprint, since)) {
        return false;
    }
    if let Some(keys) = &state.contract_keys {
        if !keys.contains(&blueprint.key) {
            return false;
        }
    } else if let Some(contract) = &state.contract
        && !blueprint.ways.iter().any(|w| w.contract.trim() == contract)
    {
        return false;
    }
    if let Some(wanted) = &state.kind
        && bp::kind_readable(&blueprint.kind) != *wanted
    {
        return false;
    }
    (pick_kind.is_empty() || category == pick_kind)
        && (pick_sub.is_empty() || *subcategory == pick_sub)
        && (pick_class.is_empty() || blueprint.class.as_deref() == Some(pick_class.as_str()))
        && (pick_size.is_empty()
            || (data.size_kinds.contains(kind) && blueprint.size.map(|s| s.to_string()).as_deref() == Some(pick_size)))
        && (pick_grade.is_empty()
            || (data.grade_kinds.contains(kind)
                && blueprint.grade.map(|g| g.to_string()).as_deref() == Some(pick_grade)))
        && (pick_source.is_empty()
            || match pick_source.split_once(':') {
                Some(("f", faction)) => blueprint.ways.iter().any(|w| w.faction == faction),
                Some(("t", pool)) => blueprint.pool.as_deref() == Some(pool),
                _ => true,
            })
        && (pick_patch.is_empty() || blueprint.since.as_deref() == Some(pick_patch.as_str()))
}

/// The keys of the blueprints the list shows when set to `state`.
#[cfg(test)]
pub(crate) fn shown_keys(data: &ListData, state: &ListState) -> BTreeSet<String> {
    shown_keys_at(data, state, export::now_unix())
}

/// [`shown_keys`] with the clock at `now`.
#[cfg(test)]
pub(crate) fn shown_keys_at(data: &ListData, state: &ListState, now: i64) -> BTreeSet<String> {
    let search = state.search.trim().to_lowercase();
    let since = state.period.since(now);
    (0..data.catalog.blueprints.len())
        .filter(|&i| passes(data, state, i, &search, since) && fits_state(data, i, state.state))
        .map(|i| data.catalog.blueprints[i].key.clone())
        .collect()
}

/// Does blueprint `i` fit state toggle `s`?
fn fits_state(data: &ListData, i: usize, s: usize) -> bool {
    let blueprint = &data.catalog.blueprints[i];
    let have = data.have.contains(&blueprint.key);
    match s {
        1 => have,
        2 => !have,
        3 => data.is_watched(blueprint),
        4 => data.noted.contains(&slim(&blueprint.name)),
        5 => !data.catalog.version.is_empty() && blueprint.since.as_deref() == Some(data.catalog.version.as_str()),
        6 => !have && data.caps.contains_key(&blueprint.key),
        _ => true,
    }
}

/// The choices of each filter: (value, label); the first one is the all choice.
fn filter_choices(data: &ListData, picked_kind: &str) -> [Vec<(String, String)>; 7] {
    let blueprints = &data.catalog.blueprints;
    let count = |values: &mut dyn Iterator<Item = String>| {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for value in values {
            *counts.entry(value).or_default() += 1;
        }
        counts
    };
    // Kind: the groups in their order, then kinds standing alone, each with its count.
    let kinds = count(&mut data.categories.iter().map(|(c, _)| c.clone()));
    let mut kind_keys: Vec<&String> = kinds.keys().collect();
    kind_keys.sort_by_key(|k| {
        let order = bp::TOP_CATEGORIES.iter().position(|c| c == k).unwrap_or(bp::TOP_CATEGORIES.len());
        (order, bp::category_name(k).to_lowercase())
    });
    let mut kind = vec![(String::new(), t("ff_alle_arten"))];
    kind.extend(kind_keys.into_iter().map(|k| (k.clone(), format!("{} ({})", bp::category_name(k), kinds[k]))));
    // Subkind: only for the picked kind.
    let subs = count(
        &mut data.categories.iter().filter(|(c, s)| c == picked_kind && !s.is_empty()).map(|(_, s)| (*s).to_string()),
    );
    let mut sub_keys: Vec<&String> = subs.keys().collect();
    sub_keys.sort_by_key(|k| bp::subcategory_name(k).to_lowercase());
    let sub_first = if subs.is_empty() {
        t("ff_alle_unterarten")
    } else {
        tf("ff_unterart_waehlen", &[("0", Arg::from(subs.len()))])
    };
    let mut sub = vec![(String::new(), sub_first)];
    sub.extend(sub_keys.into_iter().map(|k| (k.clone(), format!("{} ({})", bp::subcategory_name(k), subs[k]))));
    // Class: the fixed ones plus what the catalog has.
    let classes = count(&mut blueprints.iter().filter_map(|b| b.class.clone()));
    let mut class_values: Vec<String> = FIXED_CLASSES.iter().map(|c| c.to_string()).collect();
    let mut extra: Vec<&String> = classes.keys().filter(|c| !class_values.contains(c)).collect();
    extra.sort();
    class_values.extend(extra.into_iter().cloned());
    let mut class = vec![(String::new(), t("ff_alle_klassen"))];
    class.extend(class_values.into_iter().map(|c| {
        let label = format!("{c} ({})", classes.get(&c).copied().unwrap_or(0));
        (c, label)
    }));
    // Size and grade: only blueprints of kinds where they vary count.
    let numbers = |fixed: &[i64], field: fn(&Blueprint) -> Option<i64>, kinds: &HashSet<String>| {
        let counts = count(
            &mut blueprints
                .iter()
                .filter(|b| kinds.contains(bp::kind_id(&b.kind)))
                .filter_map(field)
                .map(|n| n.to_string()),
        );
        let mut values: Vec<i64> = fixed.to_vec();
        values.extend(counts.keys().filter_map(|k| k.parse().ok()).filter(|n| !fixed.contains(n)));
        values.sort_unstable();
        (values, counts)
    };
    let (sizes, size_counts) = numbers(&FIXED_SIZES, |b| b.size, &data.size_kinds);
    let mut size = vec![(String::new(), t("ff_alle_groessen"))];
    size.extend(sizes.into_iter().map(|n| {
        let label = format!(
            "{} ({})",
            tf("ff_groesse", &[("0", Arg::from(n))]),
            size_counts.get(&n.to_string()).copied().unwrap_or(0)
        );
        (n.to_string(), label)
    }));
    let (grades, grade_counts) = numbers(&FIXED_GRADES, |b| b.grade, &data.grade_kinds);
    let mut grade = vec![(String::new(), t("ff_alle_grade"))];
    grade.extend(grades.into_iter().map(|n| {
        let letter = catalog_cache::grade_letter(n).map_or_else(|| n.to_string(), |c| c.to_string());
        let label = format!(
            "{} ({})",
            tf("ff_grad", &[("0", Arg::from(letter.as_str()))]),
            grade_counts.get(&n.to_string()).copied().unwrap_or(0)
        );
        (n.to_string(), label)
    }));
    // Source: every faction, then every reward pool; without counts.
    let mut factions: Vec<String> =
        blueprints.iter().flat_map(|b| b.ways.iter().map(|w| w.faction.clone())).filter(|f| !f.is_empty()).collect();
    factions.sort();
    factions.dedup();
    let mut pools: Vec<String> = blueprints.iter().filter_map(|b| b.pool.clone()).collect();
    pools.sort();
    pools.dedup();
    let mut source = vec![(String::new(), t("ff_alle_quellen"))];
    source.extend(factions.into_iter().map(|f| (format!("f:{f}"), f)));
    source.extend(pools.into_iter().map(|p| (format!("t:{p}"), p)));
    // Patch: newest first, the short form unless two share it.
    let patches = bp::patches(&data.catalog);
    let mut patch = vec![(String::new(), t("ff_alle_patches"))];
    patch.extend(patches.iter().map(|(full, short, n)| {
        let twice = patches.iter().filter(|(_, s, _)| s == short).count() > 1;
        (full.clone(), format!("{} ({n})", if twice { full } else { short }))
    }));
    [kind, sub, class, size, source, grade, patch]
}

/// One drop-down filter; returns whether the pick changed.
fn choice(ui: &mut Ui, scheme: &Scheme, salt: usize, choices: &[(String, String)], picked: &mut String) -> bool {
    let before = picked.clone();
    let label = choices.iter().find(|(v, _)| v == picked).map_or_else(|| choices[0].1.clone(), |(_, l)| l.clone());
    let text =
        if picked.is_empty() { RichText::new(&label) } else { RichText::new(&label).color(color(scheme.accent)) };
    let combo = egui::ComboBox::from_id_salt(("bauplan-filter", salt)).selected_text(text).show_ui(ui, |list| {
        for (value, label) in choices {
            list.selectable_value(picked, value.clone(), label);
        }
    });
    // Named after its all choice, e.g. the all sizes text.
    widgets::name_combo(&combo.response, &choices[0].1, &label);
    *picked != before
}

/// One line of the list: a group head or a blueprint.
enum Line<'a> {
    Head(&'a Group, usize),
    Row(usize, usize),
}

/// Draws the list. Clicks on check, star and diamond change `data` (the
/// caller saves).
/// What a page says when there is no catalog to show and none is being
/// built: no game data and no scmdb to be had.
pub fn no_catalog(ui: &mut Ui, scheme: &Scheme) {
    widgets::empty_state(ui, scheme, "scan-search", &t("kein_katalog"));
    ui.label(RichText::new(t("kat_fehlt_hilfe")).small().color(color(scheme.sub)));
}

/// What a page of the catalog says while there is nothing to list: that the
/// catalog is being built, or that there is none. `false` (and nothing
/// drawn) when the catalog holds blueprints.
pub fn empty_notice(ui: &mut Ui, scheme: &Scheme, data: &ListData) -> bool {
    if !data.catalog.blueprints.is_empty() {
        return false;
    }
    if data.is_building() {
        widgets::empty_state(ui, scheme, "clock", &t("kat_baut"));
        ui.label(RichText::new(t("kat_baut_hilfe")).small().color(color(scheme.sub)));
    } else {
        no_catalog(ui, scheme);
    }
    true
}

pub fn show(ui: &mut Ui, scheme: &Scheme, data: &mut ListData, state: &mut ListState) {
    show_at(ui, scheme, data, state, export::now_unix());
}

/// [`show`] with the clock at `now`, the seconds of the periods are counted from.
pub fn show_at(ui: &mut Ui, scheme: &Scheme, data: &mut ListData, state: &mut ListState, now: i64) {
    if empty_notice(ui, scheme, data) {
        return;
    }
    if data.save_failed {
        ui.label(RichText::new(t("s_bp_speichern_fehler")).color(color(scheme.red)));
    }
    let total = data.catalog.blueprints.len();
    let mine = data.catalog.blueprints.iter().filter(|b| data.have.contains(&b.key)).count();
    let share = (100.0 * mine as f64 / total as f64).round_ties_even() as i64;
    ui.label(
        RichText::new(tf("von_gesamt", &[("0", Arg::from(mine)), ("1", Arg::from(total)), ("2", Arg::from(share))]))
            .color(color(scheme.sub)),
    );

    // Search: blueprint, contract, manufacturer, class, grade, size.
    let width = ui.available_width();
    if widgets::search_field(ui, &mut state.search, &t("s_bp_suche_platz"), scheme, width).changed() {
        state.to_top = true;
        state.contract = None;
        state.contract_keys = None;
    }
    let search = state.search.trim().to_lowercase();
    if search.chars().count() >= CONTRACT_SEARCH_MIN {
        let contracts = bp::contracts_with(&data.catalog, &search, CONTRACTS_SHOWN);
        if !contracts.is_empty() {
            ui.label(
                RichText::new(tf("s_bp_auftrag_kopf", &[("0", Arg::from(state.search.trim()))]))
                    .strong()
                    .color(color(scheme.accent)),
            );
            for (title, n) in &contracts {
                let on = state.contract.as_deref() == Some(title.as_str());
                let text = tf("s_bp_auftrag_zeile", &[("0", Arg::from(title.as_str())), ("1", Arg::from(*n))]);
                if widgets::list_row(ui, scheme, on, &text).clicked() {
                    state.contract = if on { None } else { Some(title.clone()) };
                    state.contract_keys = None;
                    state.to_top = true;
                }
            }
            ui.label(RichText::new(t("s_bp_auftrag_klick")).small().color(color(scheme.sub)));
        }
    }

    // State toggles with counts over the other filters.
    let since = state.period.since(now);
    let passing: Vec<usize> = (0..total).filter(|&i| passes(data, state, i, &search, since)).collect();
    let words: Vec<String> = STATES.iter().map(|k| t(k)).collect();
    let counts: Vec<(&str, usize)> = words
        .iter()
        .enumerate()
        .map(|(s, w)| (w.as_str(), passing.iter().filter(|&&i| fits_state(data, i, s)).count()))
        .collect();
    // The reset button stands in the page head ([`ListState::reset`]).
    let narrowed = state.is_narrowed();
    if widgets::count_toggles(ui, scheme, &counts, &mut state.state) {
        state.to_top = true;
    }
    if state.state == 6 {
        ui.label(RichText::new(t("deckel_warnung")).color(color(scheme.gold)));
    }
    if let Some(from) = state.jump.clone() {
        ui.horizontal(|row| {
            if widgets::jump_filter(row, scheme, &from) {
                state.jump = None;
                state.picks[0].clear();
                state.picks[1].clear();
                if is_period_label(&from) {
                    state.period = Period::All;
                }
                state.kind = None;
                state.contract = None;
                state.contract_keys = None;
            }
        });
    }

    // The filters; a filter with only the all choice is left out. The counter at the right.
    let choices = filter_choices(data, &state.picks[0]);
    let mut shown: Vec<usize> = passing.iter().copied().filter(|&i| fits_state(data, i, state.state)).collect();
    // A period lists flat, the newest first.
    if state.period != Period::All {
        shown.sort_by(|&a, &b| newest_first(data, a, b));
    }
    state.shown_first = shown.first().copied();
    ui.horizontal_wrapped(|row| {
        for (f, options) in choices.iter().enumerate() {
            if options.len() <= 1 || (f == 1 && state.picks[0].is_empty()) {
                continue;
            }
            if choice(row, scheme, f, options, &mut state.picks[f]) {
                state.to_top = true;
                if f == 0 {
                    state.category_picked();
                }
            }
        }
        // The period: only where something came with a time, or while one is picked.
        if !data.received.is_empty() || state.period != Period::All {
            let counts = period_counts(data, now);
            let options: Vec<(String, String)> = Period::EVERY
                .iter()
                .zip(counts)
                .map(|(&period, n)| {
                    let name = t(period_key(period));
                    (period_id(period).to_string(), if period == Period::All { name } else { format!("{name} ({n})") })
                })
                .collect();
            let mut picked = period_id(state.period).to_string();
            if choice(row, scheme, state.picks.len(), &options, &mut picked) {
                state.period = Period::EVERY.into_iter().find(|&p| period_id(p) == picked).unwrap_or_default();
                state.to_top = true;
                // The label of a jump to a period no longer says what the list shows.
                if state.jump.as_deref().is_some_and(is_period_label) {
                    state.jump = None;
                }
            }
        }
        let count = if narrowed {
            tf("ff_treffer", &[("0", Arg::from(shown.len())), ("1", Arg::from(total))])
        } else {
            tf("ff_alle_treffer", &[("0", Arg::from(total))])
        };
        row.label(RichText::new(count).small().color(color(scheme.sub)));
    });
    ui.label(RichText::new(t("bp_grenze")).small().color(color(scheme.sub)));
    // What the tile counts but the catalog does not hold cannot be listed; the list says how many.
    if let Some(since) = since
        && let outside @ 1.. = outside_catalog(data, since)
    {
        let note = tf("erhalten_ausserhalb", &[("0", Arg::from(outside))]);
        ui.label(RichText::new(note).small().color(color(scheme.sub)));
    }

    legend(ui, scheme);

    if state.state == 3
        && !data.observations.is_empty()
        && let Some(k) = observations(ui, scheme, data)
    {
        data.drop_observation(k);
    }

    if shown.is_empty() {
        let empty = match state.state {
            _ if state.period != Period::All => "erhalten_leer",
            3 => "merkliste_leer",
            4 => "vorgemerkt_leer",
            5 => "neu_leer",
            6 => "deckel_leer",
            _ => "nichts_gefunden",
        };
        widgets::list_frame(ui, scheme, |list| {
            list.set_width(list.available_width());
            widgets::empty_state(list, scheme, "scan-search", &t(empty));
        });
        return;
    }
    if state.open.is_some_and(|o| !shown.contains(&o)) {
        state.open = None;
    }

    // The rows: group heads and blueprints, only the visible ones drawn.
    let groups = if state.period == Period::All { bp::groups(&data.catalog, &shown) } else { Vec::new() };
    let mut lines: Vec<Line> = Vec::new();
    for group in &groups {
        let have = group.rows.iter().filter(|&&i| data.have.contains(&data.catalog.blueprints[i].key)).count();
        lines.push(Line::Head(group, have));
        lines.extend(group.rows.iter().enumerate().map(|(k, &i)| Line::Row(i, k)));
    }
    // Within a period the rows stand flat in the order of `shown`, with no group heads.
    if state.period != Period::All {
        lines.extend(shown.iter().enumerate().map(|(k, &i)| Line::Row(i, k)));
    }
    let text_height = ui.text_style_height(&egui::TextStyle::Body);
    let small_height = ui.text_style_height(&egui::TextStyle::Small);
    let head_height = small_height + 14.0;
    let row_height = text_height + small_height + 10.0;
    let height_of = |line: &Line| match line {
        Line::Head(..) => head_height,
        Line::Row(i, _) if state.open == Some(*i) => row_height + state.detail_height,
        Line::Row(..) => row_height,
    };
    let tops: Vec<f32> = lines
        .iter()
        .scan(0.0, |y, line| {
            let top = *y;
            *y += height_of(line);
            Some(top)
        })
        .collect();
    let total_height = tops.last().copied().unwrap_or(0.0) + lines.last().map_or(0.0, height_of);
    let room = crate::layout::list_room(ui, ui.text_style_height(&egui::TextStyle::Body) + 10.0);
    let period = state.period;
    let mut clicked: Option<(usize, usize)> = None;
    let mut measured = None;
    widgets::list_frame(ui, scheme, |list| {
        list.set_width(list.available_width());
        let mut area = egui::ScrollArea::vertical()
            .id_salt("bauplan-liste")
            .max_height(room)
            .min_scrolled_height(room)
            .auto_shrink([false, false]);
        if std::mem::take(&mut state.to_top) {
            area = area.vertical_scroll_offset(0.0);
        }
        area.show_viewport(list, |rows, viewport| {
            rows.set_min_height(total_height);
            let origin = rows.min_rect().min;
            let width = rows.available_width();
            let first =
                tops.partition_point(|&top| top + head_height.max(row_height) < viewport.min.y).saturating_sub(1);
            for (n, line) in lines.iter().enumerate().skip(first) {
                if tops[n] > viewport.max.y {
                    break;
                }
                let rect =
                    egui::Rect::from_min_size(origin + egui::vec2(0.0, tops[n]), egui::vec2(width, height_of(line)));
                let mut child = rows.new_child(UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)));
                match line {
                    Line::Head(group, have) => {
                        child.add_space(8.0);
                        child.horizontal(|head| {
                            head.label(
                                RichText::new(group.title.to_uppercase()).small().strong().color(color(scheme.accent)),
                            );
                            head.label(
                                RichText::new(format!("{have}/{}", group.rows.len())).small().color(color(scheme.sub)),
                            );
                        });
                    }
                    Line::Row(i, k) => {
                        // Every second row striped, counted anew after each head.
                        if k % 2 == 1 {
                            rows.painter().rect_filled(rect, 0.0, color(scheme.bar));
                        }
                        let open = state.open == Some(*i);
                        let came = (period != Period::All).then(|| received_text(data, *i)).flatten();
                        if let (Some(action), _) = row(&mut child, scheme, data, *i, open, came.as_deref()) {
                            clicked = Some((*i, action));
                        }
                        if open {
                            let top = child.cursor().min.y;
                            child.indent(("bauplan-herkunft", *i), |detail| {
                                let craftable = state.craftable.contains(&data.catalog.blueprints[*i].key);
                                origin_detail(
                                    detail,
                                    scheme,
                                    data,
                                    *i,
                                    &mut state.more_ways,
                                    craftable.then_some(&mut state.craft_jump),
                                );
                            });
                            measured = Some(child.cursor().min.y - top + 6.0);
                        }
                    }
                }
            }
        });
    });
    if let Some(height) = measured
        && (height - state.detail_height).abs() > 0.5
    {
        state.detail_height = height;
        ui.ctx().request_repaint();
    }
    if let Some((i, action)) = clicked {
        match action {
            ROW_CHECK => data.toggle_have(i),
            ROW_STAR => data.toggle_watch(i),
            ROW_DIAMOND => data.toggle_noted(i),
            _ => {
                state.open = if state.open == Some(i) { None } else { Some(i) };
                state.more_ways = false;
            }
        }
    }
}

/// Where an entry of the legend landed: middle height of symbol and text,
/// the width its text got and the width it needs.
#[cfg_attr(not(test), expect(dead_code, reason = "read by the layout test"))]
struct LegendEntry {
    symbol: f32,
    text: f32,
    shown: f32,
    full: f32,
}

/// The legend above the rows, so the symbols explain themselves. Symbol and
/// text are one piece: a line break only comes between entries.
fn legend(ui: &mut Ui, scheme: &Scheme) -> Vec<LegendEntry> {
    let mut middles = Vec::new();
    ui.horizontal_wrapped(|row| {
        let inner = row.spacing().item_spacing.x;
        // The gap between entries; inside an entry the usual spacing.
        row.spacing_mut().item_spacing.x = GAP;
        for (icon, tone, key) in [
            ("star", scheme.yellow, "s_bp_leg_stern"),
            ("diamond", scheme.accent, "s_bp_leg_farm"),
            ("gift", scheme.accent, "s_bp_leg_start"),
        ] {
            // An entry that no longer fits the line starts the next one.
            let words = RichText::new(t(key)).small().color(color(scheme.sub));
            let full = egui::WidgetText::from(words.clone())
                .into_galley(row, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Small)
                .size()
                .x;
            let needed = 14.0 + inner + full + 1.0;
            // ⚠ In a wrapping row `available_width` is the whole line; what is
            // left of the current line is `available_size_before_wrap`.
            if row.available_size_before_wrap().x < needed && row.cursor().min.x > row.max_rect().min.x + 1.0 {
                row.end_row();
            }
            row.horizontal(|entry| {
                entry.spacing_mut().item_spacing.x = inner;
                let symbol = icons::image(icon, tone, 14.0).map(|image| entry.add(image).rect.center().y);
                let text = entry.add(egui::Label::new(words).truncate());
                middles.push(LegendEntry {
                    symbol: symbol.unwrap_or_default(),
                    text: text.rect.center().y,
                    shown: text.rect.width(),
                    full,
                });
            });
        }
    });
    middles
}

const ROW_CHECK: usize = 0;
const ROW_STAR: usize = 1;
const ROW_DIAMOND: usize = 2;
const ROW_OPEN: usize = 3;

/// A small symbol that is a click; `tone` when on, pale when off.
fn symbol_button(ui: &mut Ui, scheme: &Scheme, icon: &str, tone: Color, on: bool, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    let tint = if on {
        tone
    } else if response.hovered() {
        scheme.sub
    } else {
        scheme.line
    };
    if let Some(image) = icons::image(icon, tint, 16.0) {
        image.paint_at(ui, egui::Rect::from_center_size(rect.center(), Vec2::splat(16.0)));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked()
}

/// A place for a symbol of the fixed columns: the symbol when `shown`, or
/// the empty place. Returns whether it was clicked.
fn symbol_slot(ui: &mut Ui, scheme: &Scheme, shown: bool, icon: &str, tone: Color, on: bool, hint: &str) -> bool {
    if shown {
        return symbol_button(ui, scheme, icon, tone, on, hint);
    }
    ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
    false
}

/// One row: check, name with its sub-line, at the right star, diamond and
/// where it comes from. `came` (when and from where it was received) leads
/// the sub-line. Returns what was clicked and where the star column begins at
/// its right edge.
fn row(
    ui: &mut Ui,
    scheme: &Scheme,
    data: &ListData,
    i: usize,
    open: bool,
    came: Option<&str>,
) -> (Option<usize>, f32) {
    let blueprint = &data.catalog.blueprints[i];
    let have = data.have.contains(&blueprint.key);
    let watched = data.is_watched(blueprint);
    let noted = data.noted.contains(&slim(&blueprint.name));
    let mut action = None;
    let mut star_x = 0.0;
    ui.horizontal(|line| {
        line.add_space(4.0);
        let check = if have { "check" } else { "circle" };
        if symbol_button(
            line,
            scheme,
            check,
            scheme.accent,
            have,
            &t(if have { "filter_habe" } else { "filter_fehlt" }),
        ) {
            action = Some(ROW_CHECK);
        }
        line.with_layout(Layout::right_to_left(Align::Center), |right| {
            right.add_space(6.0);
            // Where it comes from: the source button, a gift for a starter blueprint,
            // or a question mark, in a column as wide as the source button, so star
            // and diamond stand one under the other in every row.
            let word = t("hk_knopf");
            let cell = widgets::icon_button_size(right, scheme, ButtonKind::Normal, &word);
            let (rect, _) = right.allocate_exact_size(cell, Sense::hover());
            let mut origin = right.new_child(
                UiBuilder::new().max_rect(rect).layout(Layout::centered_and_justified(egui::Direction::LeftToRight)),
            );
            if !blueprint.ways.is_empty() {
                let label = widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("info");
                if origin.add(label).on_hover_text(t("hinweis_quellen")).clicked() {
                    action = Some(ROW_OPEN);
                }
            } else {
                let (icon, tone, hint) = if blueprint.start {
                    ("gift", scheme.accent, "hinweis_startbauplan")
                } else {
                    ("circle-question-mark", scheme.sub, "hinweis_ohne_quelle")
                };
                if let Some(image) = icons::image(icon, tone, 16.0) {
                    origin.add(image).on_hover_text(t(hint));
                }
            }
            // Diamond where one has it or noted it, star where one misses it or
            // watches it; an empty place otherwise, so the columns stay.
            let diamond_hint = t(if noted { "s_bp_farm_drauf" } else { "s_bp_farm_merken" });
            if symbol_slot(right, scheme, have || noted, "diamond", scheme.accent, noted, &diamond_hint) {
                action = Some(ROW_DIAMOND);
            }
            let star_hint = t(if watched { "nicht_mehr_merken" } else { "merken" });
            star_x = right.cursor().max.x;
            if symbol_slot(right, scheme, !have || watched, "star", scheme.yellow, watched, &star_hint) {
                action = Some(ROW_STAR);
            }
            if let Some(pool) = &blueprint.pool {
                widgets::badge(right, scheme, scheme.blue, pool);
            }
            right.with_layout(Layout::left_to_right(Align::Center), |middle| {
                let arrow = if open { "chevron-down" } else { "chevron-right" };
                if let Some(image) = icons::themed(arrow, scheme, IconTint::Normal, 12.0) {
                    middle.add(image);
                }
                middle.vertical(|names| {
                    let tone = if have { scheme.fg } else { scheme.sub };
                    let name = egui::Label::new(RichText::new(&blueprint.name).color(color(tone)))
                        .truncate()
                        .sense(Sense::click());
                    if names.add(name).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        action = Some(ROW_OPEN);
                    }
                    let mut sub = sub_line(data, blueprint);
                    // The time and the source lead; the rest of the sub-line only follows where it fits.
                    if let Some(came) = came {
                        let full = if sub.is_empty() { came.to_string() } else { format!("{came} · {sub}") };
                        let needed = egui::WidgetText::from(RichText::new(full.as_str()).small())
                            .into_galley(names, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Small)
                            .size()
                            .x;
                        sub = if needed <= names.available_width() { full } else { came.to_string() };
                    }
                    if !sub.is_empty() {
                        names.add(egui::Label::new(RichText::new(sub).small().color(color(scheme.sub))).truncate());
                    }
                });
            });
        });
    });
    (action, star_x)
}

/// Where a blueprint comes from, in its open row: ways, rank, reward, cap,
/// shareable and lockout; the easiest way written out, the others folded.
fn origin_detail(
    ui: &mut Ui,
    scheme: &Scheme,
    data: &ListData,
    i: usize,
    more_ways: &mut bool,
    craft_jump: Option<&mut Option<String>>,
) {
    let blueprint = &data.catalog.blueprints[i];
    let have = data.have.contains(&blueprint.key);
    let lang = language::current();
    // What it takes to craft it: only where the crafting page knows the recipe.
    if let Some(craft_jump) = craft_jump {
        let word = t("s_p9_zutaten_zeigen");
        let button = ui.add(widgets::Button::new(&word, ButtonKind::Normal, scheme).icon("zap"));
        if button.on_hover_text(t("s_p9_zutaten_zeigen_hilfe")).clicked() {
            *craft_jump = Some(blueprint.name.clone());
        }
        ui.add_space(2.0);
    }
    ui.horizontal_wrapped(|line| {
        let mut parts = vec![bp::kind_readable(&blueprint.kind)];
        parts.extend(blueprint.class.clone());
        parts.push(t(if have { "hk_hast_du" } else { "hk_fehlt_dir" }));
        if blueprint.ways.len() > 1 {
            parts.push(t("hk_leichtester"));
        }
        line.label(RichText::new(parts.join(" · ")).small().color(color(scheme.sub)));
        if !blueprint.ways.is_empty() {
            let ways = if blueprint.ways.len() == 1 {
                t("hk_ein_weg")
            } else {
                tf("hk_wege", &[("0", Arg::from(blueprint.ways.len()))])
            };
            widgets::badge(line, scheme, scheme.accent, &ways);
        }
    });
    if !have && let Some((cap, rank)) = data.caps.get(&blueprint.key) {
        ui.horizontal(|line| {
            if let Some(image) = icons::image("triangle-alert", scheme.gold, 14.0) {
                line.add(image);
            }
            let cap = formats::count(lang, *cap);
            let text = tf("deckel_zeile", &[("0", Arg::from(rank.as_deref().unwrap_or("—"))), ("1", Arg::from(cap))]);
            line.label(RichText::new(text).small().strong().color(color(scheme.gold)));
        });
    }
    let (shareable, lockout) = bp::contract_traits(&data.catalog, &blueprint.key);
    if shareable.is_some() || lockout.is_some() {
        ui.horizontal_wrapped(|line| {
            if let Some(shareable) = shareable {
                let (icon, key) = if shareable { ("users", "hk_teilbar") } else { ("user", "hk_nicht_teilbar") };
                if let Some(image) = icons::image(icon, scheme.sub, 14.0) {
                    line.add(image);
                }
                line.label(RichText::new(t(key)).small().color(color(scheme.sub)));
                line.add_space(GAP);
            }
            if let Some(minutes) = lockout {
                if let Some(image) = icons::image("timer", scheme.sub, 14.0) {
                    line.add(image);
                }
                let text = tf("hk_sperre", &[("0", Arg::from(bp::duration_text(minutes)))]);
                line.label(RichText::new(text).small().color(color(scheme.sub)));
            }
        });
    }
    let Some(best) = blueprint.ways.first() else {
        let text = if blueprint.start {
            t("hk_start")
        } else if let Some(pool) = &blueprint.pool {
            format!("{}: {pool}\n{}", t("hk_topf"), t("hk_topf_text"))
        } else {
            t("hk_keine")
        };
        ui.label(RichText::new(text).color(color(scheme.sub)));
        return;
    };
    egui::Grid::new(("bauplan-weg", i)).num_columns(2).spacing([14.0, 2.0]).show(ui, |grid| {
        let dash = || "—".to_string();
        let rank = match (&best.rank, best.rank_reputation.filter(|r| *r > 0)) {
            (Some(rank), Some(rep)) => format!("{rank} {}", reputation(lang, rep)),
            (Some(rank), None) => rank.clone(),
            _ => dash(),
        };
        for (label, value) in [
            ("hk_auftrag", if best.contract.is_empty() { dash() } else { best.contract.clone() }),
            ("hk_fraktion", if best.faction.is_empty() { dash() } else { best.faction.clone() }),
            ("hk_annahme", best.place.as_ref().map_or_else(dash, bp::place_text)),
            ("hk_rang", rank),
            ("hk_belohnung", best.reward.map_or_else(dash, |uec| formats::money(lang, uec as f64))),
        ] {
            grid.label(RichText::new(t(label)).small().color(color(scheme.sub)));
            grid.label(value);
            grid.end_row();
        }
    });
    if blueprint.ways.len() > 1 {
        ui.add_space(4.0);
        let arrow = if *more_ways { "chevron-down" } else { "chevron-right" };
        let more = tf("hk_weitere", &[("0", Arg::from(blueprint.ways.len() - 1))]);
        let head = widgets::Button::new(&more, ButtonKind::Normal, scheme).icon(arrow);
        if ui.add(head).clicked() {
            *more_ways = !*more_ways;
        }
        if *more_ways {
            // The further ways roll in a room of their own with a height limit, so many ways do not burst the box.
            // The room takes the limit also where the open row was lower on the frame before (its height is
            // measured), and shrinks to the ways where they need less.
            let limit = further_ways_height(ui.ctx());
            egui::ScrollArea::vertical()
                .id_salt(("bauplan-weitere-wege", i))
                .max_height(limit)
                .min_scrolled_height(limit)
                .auto_shrink([false, true])
                .show(ui, |ways| {
                    for way in &blueprint.ways[1..] {
                        further_way(ways, scheme, lang, way);
                    }
                });
        }
    }
}

/// The room of the further ways is at most this share of the window high,
/// but never lower than [`FURTHER_WAYS_MIN`].
const FURTHER_WAYS_SHARE: f32 = 0.3;
const FURTHER_WAYS_MIN: f32 = 120.0;

/// The height limit of the room the further ways roll in, in the window of this frame.
fn further_ways_height(ctx: &egui::Context) -> f32 {
    further_ways_limit(ctx.content_rect().height())
}

/// The height limit of the room the further ways roll in, in a window `window` high.
fn further_ways_limit(window: f32) -> f32 {
    (window * FURTHER_WAYS_SHARE).max(FURTHER_WAYS_MIN)
}

/// The reputation points as text, e.g. `(2.200 Ruf)`.
fn reputation(lang: language::Lang, points: i64) -> String {
    tf("ruf_punkte", &[("0", Arg::from(formats::count(lang, points)))])
}

/// One further way, in three short lines.
fn further_way(ui: &mut Ui, scheme: &Scheme, lang: language::Lang, way: &Way) {
    ui.add_space(4.0);
    let head: Vec<&str> = [way.faction.as_str(), way.kind.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
    ui.label(RichText::new(head.join(" · ")).small().color(color(scheme.gold)));
    if !way.contract.is_empty() {
        ui.label(RichText::new(format!("„{}“", way.contract)).small());
    }
    let mut below = Vec::new();
    if let Some(rank) = &way.rank {
        let mut text = tf("ab_rang", &[("0", Arg::from(rank.as_str()))]);
        if let Some(rep) = way.rank_reputation.filter(|r| *r > 0) {
            text = format!("{text} {}", reputation(lang, rep));
        }
        below.push(text);
    }
    if let Some(uec) = way.reward {
        below.push(formats::money(lang, uec as f64));
    }
    if let Some(gain) = way.reputation_gain.filter(|g| *g > 0) {
        below.push(tf("ruf_gewinn", &[("0", Arg::from(gain))]));
    }
    below.extend(way.place.as_ref().map(bp::place_text));
    if !below.is_empty() {
        ui.label(RichText::new(below.join(" · ")).small().color(color(scheme.sub)));
    }
}

/// Widest an observation card gets; several stand side by side.
const OBSERVATION_WIDTH: f32 = 280.0;

/// The own observations of the watch list, above the rows: small cards side
/// by side, each with ✕ to drop it. Returns the one whose ✕ was clicked.
fn observations(ui: &mut Ui, scheme: &Scheme, data: &ListData) -> Option<usize> {
    let mut dropped = None;
    ui.horizontal(|head| {
        head.label(RichText::new(t("merk_eigene")).strong().color(color(scheme.accent)));
        head.label(RichText::new(t("merk_eigene_h")).small().color(color(scheme.sub)));
    });
    let width = OBSERVATION_WIDTH.min(ui.available_width());
    let height = ui.text_style_height(&egui::TextStyle::Body) + ui.text_style_height(&egui::TextStyle::Small) + 16.0;
    ui.horizontal_wrapped(|row| {
        for (k, observation) in data.observations.iter().enumerate() {
            // A fixed size, so the wrapping row knows it before drawing.
            row.allocate_ui_with_layout(Vec2::new(width, height), Layout::top_down(Align::Min), |cell| {
                egui::Frame::new()
                    .fill(color(scheme.field))
                    .stroke(egui::Stroke::new(1.0, color(scheme.control_border())))
                    .corner_radius(4)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(cell, |card| {
                        card.set_width(width - 18.0);
                        card.horizontal(|top| {
                            top.with_layout(Layout::right_to_left(Align::Center), |right| {
                                let hint = t("merk_eigene_weg");
                                if symbol_button(right, scheme, "x", scheme.red, false, &hint) {
                                    dropped = Some(k);
                                }
                                right.with_layout(Layout::left_to_right(Align::Center), |text| {
                                    text.add(egui::Label::new(&observation.title).truncate());
                                });
                            });
                        });
                        let waits = tf("merk_wartet", &[("0", Arg::from(observation.patterns.join(", ")))]);
                        card.add(egui::Label::new(RichText::new(waits).small().color(color(scheme.sub))).truncate());
                    });
            });
        }
    });
    ui.add_space(4.0);
    dropped
}

// ------------------------------------------------------------ Sample contents

/// Kinds of the sample with base names, makers, and whether size, grade and
/// class mean something for them (as in the game: only for ship parts).
const SAMPLE_KINDS: [(&str, &[&str], &[&str], bool); 8] = [
    ("Cooler", &["Arctic", "Glacier", "Frost", "Snowpack"], &["Lightning Power Ltd.", "Aegis Dynamics"], true),
    ("PowerPlant", &["Lumen", "Fierell", "Durango"], &["Amon & Reese", "Juno Starwerk"], true),
    ("Shield", &["FR-66", "Palisade", "Bulwark"], &["Gorgon Defender", "Basilisk"], true),
    ("QuantumDrive", &["Atlas", "Voyage", "Drift"], &["Wei-Tek", "RSI"], true),
    ("WeaponPersonal", &["Arclight Pistol", "Karna Rifle", "Coda Pistol"], &["Klaus & Werner", "Kastak Arms"], false),
    ("Char_Armor_Helmet", &["Morozov-SH Helmet", "Pembroke Helmet"], &["Roussimoff Rehabilitation", "RSI"], false),
    ("Char_Armor_Torso", &["Morozov-SH Core", "Pembroke Core"], &["Roussimoff Rehabilitation", "RSI"], false),
    ("Char_Armor_Backpack", &["Strata Backpack", "Novikov Backpack"], &["Clark Defense Systems", "Caldera"], false),
];
const SAMPLE_VARIANTS: [&str; 6] = ["", "Black", "Desert", "Arctic", "Crimson", "Forest"];
const SAMPLE_CLASSES: [&str; 5] = ["Military", "Civilian", "Industrial", "Stealth", "Competition"];

fn sample_catalog() -> CatalogCache {
    let mut blueprints = Vec::new();
    for (k, (kind, names, makers, part)) in SAMPLE_KINDS.iter().enumerate() {
        for (n, base) in names.iter().enumerate() {
            for (v, variant) in SAMPLE_VARIANTS.iter().enumerate() {
                let i = blueprints.len();
                let name = if variant.is_empty() { base.to_string() } else { format!("{base} {variant}") };
                let ways = if i % 4 == 3 {
                    Vec::new()
                } else {
                    (0..1 + i % 3)
                        .map(|w| Way {
                            contract: ["Protect Fuel Tanks", "Retake the Outpost", "Deliver Supplies"][(i + w) % 3]
                                .into(),
                            kind: ["Mercenary", "Delivery", "Bounty Hunter"][(i + w) % 3].into(),
                            faction: ["Foxwell Enforcement", "Covalex Shipping", "Headhunters"][(i + w) % 3].into(),
                            reward: Some(25_000 + 5_000 * ((i + w) % 7) as i64),
                            reputation_gain: Some(50 + 25 * (w as i64)),
                            rank: Some(["Neutral", "Contractor", "Senior"][(i + w) % 3].into()),
                            rank_reputation: Some([0, 2_200, 25_000][(i + w) % 3]),
                            place: Some(Place {
                                system: ["Stanton", "Pyro", "Nyx"][(i + w) % 3].into(),
                                places: vec!["Hurston".into(), "Crusader".into()],
                                more: (i % 3) as i64,
                            }),
                        })
                        .collect()
                };
                // Hand weapons with their recipe, so the subtypes split them as in the game data.
                let (recipe_subtype, recipe_kind) = match *kind {
                    "WeaponPersonal" if base.ends_with("Rifle") => ("rifle", "weapons"),
                    "WeaponPersonal" => ("pistol", "weapons"),
                    _ => ("", ""),
                };
                let recipe_tag =
                    if recipe_kind.is_empty() { String::new() } else { format!("BP_CRAFT_{}", base.replace(' ', "_")) };
                blueprints.push(Blueprint {
                    key: name_key(&name),
                    name,
                    kind: (*kind).into(),
                    sub: String::new(),
                    size: Some(if *part { (n + v) as i64 % 4 + 1 } else { 1 }),
                    grade: Some(if *part { (v as i64) % 4 + 1 } else { 1 }),
                    class: part.then(|| SAMPLE_CLASSES[(k + v) % SAMPLE_CLASSES.len()].to_string()),
                    maker: Some(makers[v % makers.len()].to_string()),
                    start: i % 4 == 3 && i % 8 == 7,
                    pool: (i % 16 == 3).then(|| "XenoThreat".to_string()),
                    since: Some(if i % 9 == 0 { "4.3.1-live.1" } else { "4.3.0-live.1" }.into()),
                    no_way: false,
                    ways,
                    recipe_tag,
                    recipe_subtype: recipe_subtype.into(),
                    recipe_kind: recipe_kind.into(),
                });
            }
        }
    }
    let missions = blueprints
        .iter()
        .enumerate()
        .filter(|(i, b)| !b.ways.is_empty() && i % 5 != 4)
        .map(|(i, b)| Mission {
            key: format!("sample_{i}"),
            blueprints: vec![b.key.clone()],
            reputation_max: (i % 2 == 0).then_some(25_000),
            rank_max: (i % 2 == 0).then(|| "Senior".to_string()),
            shareable: Some(i % 3 != 0),
            cooldown: (i % 4 == 0).then_some(15),
            ..Mission::default()
        })
        .collect();
    CatalogCache { version: "4.3.1-live.1".into(), fetched: String::new(), blueprints, missions, reputation: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The list of a test folder from its own 4.12 cache, never from a game
    /// installed on this machine.
    fn from_cache(paths: &Paths) -> ListData {
        ListData::load_with(paths, catalog_cache::load(paths).expect("the test catalog"))
    }

    /// What another page put on the farm notes shows in the list when it is entered.
    #[test]
    fn the_farm_notes_are_read_again() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;
        let dir = std::env::temp_dir().join(format!("versekit-merkzettel-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &text)], &text));
        let mut data = ListData::new(CatalogCache::default(), HashSet::new(), HashSet::new(), HashSet::new());
        data.paths = Some(Paths::new(paths.host.clone()));
        data.reload_noted();
        assert!(data.noted.is_empty());
        std::fs::write(paths.app_file(FARM_FILE), r#"{"merkzettel": [{"name": "Arctic-Storm Helm", "anzahl": 1}]}"#)
            .unwrap();
        data.reload_noted();
        assert!(data.noted.contains(&slim("arctic storm helm")), "{:?}", data.noted);
        std::fs::write(paths.app_file(FARM_FILE), r#"{"merkzettel": []}"#).unwrap();
        data.reload_noted();
        assert!(data.noted.is_empty());
    }

    /// The real data of a copy (`VERSEKIT_KOPIE`), read only: what the list shows.
    #[test]
    #[ignore = "needs a copy of real data"]
    fn real_copy_counts() {
        let Ok(root) = std::env::var("VERSEKIT_KOPIE") else { return };
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(None);
        let paths = Paths::for_copy(versekit_core::paths::Host::current(), &root).expect("copy");
        // The finished catalog, not the stand-in a window shows meanwhile.
        let data = ListData::load_with(&paths, list::list_catalog(&paths).expect("catalog"));
        let denied = versekit_core::writeguard::take_denied();
        versekit_core::writeguard::deny(false);
        let all: Vec<usize> = (0..data.catalog.blueprints.len()).collect();
        let mine = data.catalog.blueprints.iter().filter(|b| data.have.contains(&b.key)).count();
        println!(
            "Baupläne {} · habe ich {mine} · Merkliste {} · vorgemerkt {}",
            all.len(),
            data.watched.len(),
            data.noted.len()
        );
        for (s, key) in STATES.iter().enumerate() {
            println!("  {}: {}", t(key), all.iter().filter(|&&i| fits_state(&data, i, s)).count());
        }
        for group in bp::groups(&data.catalog, &all) {
            let have = group.rows.iter().filter(|&&i| data.have.contains(&data.catalog.blueprints[i].key)).count();
            println!("  {} {have}/{}", group.title.to_uppercase(), group.rows.len());
        }
        for (area, kinds) in crate::blueprint_progress::counts(&data, None) {
            let total: usize = kinds.values().map(|(n, _)| n).sum();
            let have: usize = kinds.values().map(|(_, h)| h).sum();
            println!("  Bereich {}: {have} / {total}", bp::TOP_GROUPS[area]);
        }
        println!("  Größe zählt bei {:?}", data.size_kinds);
        println!("  Grad zählt bei {:?}", data.grade_kinds);
        let cooler = data.catalog.blueprints.iter().find(|b| b.name == "ArcticStorm").expect("ArcticStorm");
        println!("  ArcticStorm: {} · Wege {}", sub_line(&data, cooler), cooler.ways.len());
        println!("  nicht geschrieben (Sperre): {denied:?}");
    }

    /// The first start after a patch with the real `Data.p4k` of a game
    /// channel (`VERSEKIT_KANAL`) and a data folder holding a 4.12 cache
    /// (`VERSEKIT_DATEN`, only read; the catalog is kept in a throwaway
    /// folder): from the call to the first drawn frame, the list must not
    /// wait for the build. Run with `cargo test --release`.
    #[test]
    #[ignore = "needs the real Data.p4k"]
    fn real_archive_first_frame_after_a_patch() {
        use std::time::{Duration, Instant};
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let (Ok(channel), Ok(data)) = (std::env::var("VERSEKIT_KANAL"), std::env::var("VERSEKIT_DATEN")) else {
            return;
        };
        let paths = Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &data)], &data));
        let scratch = std::env::temp_dir().join(format!("versekit-katalog-echt-{}", versekit_core::testdir::run_tag()));
        std::fs::create_dir_all(&scratch).unwrap();
        let cache = scratch.join(list::CACHE_FILE).to_string_lossy().into_owned();
        versekit_core::writeguard::deny(true);
        versekit_core::writeguard::allow_under(Some(&scratch.to_string_lossy()));
        let channel = std::path::Path::new(&channel);

        // The build once, then the cache as a patch leaves it: of another stand of the archive.
        let waited = list::list_catalog_in(&paths, Some(channel), &cache).expect("a catalog");
        let Some(Value::Object(mut stored)) = jsonio::read_json(&cache).ok() else { panic!("the cache is no object") };
        let Some((_, Value::Object(source))) = stored.iter_mut().find(|(k, _)| k == "quelle") else {
            panic!("no stamp")
        };
        jsonio::map_set(source, "archiv_stand", Value::Null);
        let text = jsonio::dumps(&Value::Object(stored), None, false);
        jsonio::write_text_atomic(&cache, &text, jsonio::Newline::Native).unwrap();

        let ctx = egui::Context::default();
        crate::install(&ctx);
        let started = Instant::now();
        let start = list::start_catalog_in(&paths, Some(channel), &cache);
        let building = start.rebuild.and_then(list::Rebuild::spawn);
        let mut data = ListData::load_from(&paths, start.catalog, building).expect("a list");
        let loaded = started.elapsed();
        let mut state = ListState::default();
        frame(&ctx, &mut data, &mut state);
        let first = started.elapsed();
        println!("Liste bereit nach {loaded:?}, erstes Bild nach {first:?}, im Bau: {}", data.is_building());
        assert!(data.is_building(), "the build had not ended at the first frame");
        assert!(first < Duration::from_millis(100), "erstes Bild nach {first:?}");
        for _ in 0..3000 {
            frame(&ctx, &mut data, &mut state);
            if !data.is_building() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let visible = waited.blueprints.iter().filter(|b| !(data.hide_no_way && b.no_way)).count();
        assert!(!data.is_building() && data.catalog.blueprints.len() == visible);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Star and diamond stand one under the other in every row: with a source
    /// button, a gift, a question mark, a pool badge, the star or the diamond
    /// missing, the star column begins at the same place.
    #[test]
    fn symbols_stand_in_columns() {
        let mut data = ListData::sample();
        let blueprints = &data.catalog.blueprints;
        let with_way = blueprints.iter().position(|b| !b.ways.is_empty()).unwrap();
        let start = blueprints.iter().position(|b| b.start).unwrap();
        let pool = blueprints.iter().position(|b| b.pool.is_some()).unwrap();
        let unknown = blueprints.iter().position(|b| b.ways.is_empty() && !b.start && b.pool.is_none()).unwrap();
        let (have_key, missing_key) = (blueprints[with_way].key.clone(), blueprints[unknown].key.clone());
        data.have.insert(have_key);
        data.have.remove(&missing_key);
        let scheme = versekit_core::theme::current();
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut columns = Vec::new();
        for _ in 0..2 {
            columns.clear();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 600.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                for i in [with_way, start, pool, unknown] {
                    columns.push((data.catalog.blueprints[i].name.clone(), row(ui, scheme, &data, i, false, None).1));
                }
            });
            output.textures_delta.clear();
        }
        let first = columns[0].1;
        assert!(first > 0.0);
        for (name, x) in &columns {
            assert!((x - first).abs() < 0.5, "Stern von „{name}“ bei {x:.1}, erwartet {first:.1}: {columns:?}");
        }
    }

    /// Each symbol of the legend stands on the line of its text, and no text
    /// is cut while a line of its own would hold it, at every width and size.
    #[test]
    fn legend_symbols_stay_with_their_text() {
        let scheme = versekit_core::theme::current();
        let mut findings = Vec::new();
        for points in versekit_core::font_size::POINTS_RANGE.0..=versekit_core::font_size::POINTS_RANGE.1 {
            for width in [300.0, 420.0, 560.0, 700.0, 900.0, 1100.0, 1400.0] {
                let ctx = egui::Context::default();
                crate::install(&ctx);
                crate::apply_size(&ctx, points);
                let mut entries = Vec::new();
                let mut line = 0.0;
                for _ in 0..3 {
                    let input = egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 400.0))),
                        ..Default::default()
                    };
                    let mut output = ctx.run_ui(input, |ui| {
                        line = ui.available_width();
                        entries = legend(ui, scheme);
                    });
                    output.textures_delta.clear();
                }
                for (k, e) in entries.iter().enumerate() {
                    if (e.symbol - e.text).abs() >= 4.0 {
                        findings.push(format!(
                            "Größe {points} Breite {width}: Eintrag {k} Symbol eine Zeile neben dem Text"
                        ));
                    }
                    if e.shown + 0.5 < e.full && e.full + 30.0 < line {
                        findings.push(format!(
                            "Größe {points} Breite {width}: Eintrag {k} gekürzt ({:.0} von {:.0})",
                            e.shown, e.full
                        ));
                    }
                }
            }
        }
        assert!(findings.is_empty(), "{}", findings.join("\n"));
    }

    /// Star and ✕ of an observation write the watch list as 4.12 does:
    /// indent 1, the other fields kept, no backup file.
    #[test]
    fn the_watch_list_is_written_like_4_12() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let dir = std::env::temp_dir().join(format!("versekit-merkliste-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        std::fs::write(
            dir.join(catalog_cache::CACHE_FILE),
            r#"{"version": "4.3.1", "bauplaene": {
                "a": {"n": "Arden Backpack", "a": "Char_Armor_Backpack"},
                "b": {"n": "Glacier", "a": "Cooler"}}}"#,
        )
        .unwrap();
        let watch = dir.join(WATCH_FILE);
        std::fs::write(
            &watch,
            "{\"namen\": [\"Glacier\"], \"eintraege\": [{\"titel\": \"Rucksack\", \"muster\": [\"arden backpack\"]}, \
             {\"titel\": \"Helm\", \"muster\": [\"morozov\"]}], \"fremd\": 1}",
        )
        .unwrap();
        let host = Host::new(Flavor::native(), &[("SC_BP_HOME", &dir_text)], &dir_text);
        let mut data = from_cache(&Paths::new(host));
        let glacier = data.catalog.blueprints.iter().position(|b| b.name == "Glacier").unwrap();
        let arden = data.catalog.blueprints.iter().position(|b| b.name == "Arden Backpack").unwrap();
        assert!(data.is_watched(&data.catalog.blueprints[arden]), "by its pattern");
        data.toggle_watch(glacier);
        data.drop_observation(1);
        let text = std::fs::read_to_string(&watch).unwrap();
        let nl = if cfg!(windows) { "\r\n" } else { "\n" };
        let expected = [
            "{",
            " \"namen\": [],",
            " \"eintraege\": [",
            "  {",
            "   \"titel\": \"Rucksack\",",
            "   \"muster\": [",
            "    \"arden backpack\"",
            "   ]",
            "  }",
            " ],",
            " \"fremd\": 1",
            "}",
        ]
        .join(nl);
        assert_eq!(text, expected);
        assert!(!dir.join("watchlist.bak.json").exists(), "4.12 keeps no backup of the watch list");
        data.toggle_watch(arden);
        let text = std::fs::read_to_string(&watch).unwrap();
        assert!(text.contains("\"Arden Backpack\""), "{text}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Check and diamond write as 4.12 does: the collection with source
    /// `hand` and its previous version, the farm notes in `hangar.json` with
    /// count 1 and `bauen`; everything else in the files stays.
    #[test]
    fn check_and_diamond_are_written_like_4_12() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let dir = std::env::temp_dir().join(format!("versekit-haken-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        std::fs::write(
            dir.join(catalog_cache::CACHE_FILE),
            r#"{"version": "4.3.1", "bauplaene": {"b": {"n": "Glacier", "a": "Cooler"}}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join(collection::FILE_NAME),
            r#"{"version": 3, "stand": "x", "bauplaene": {"arctic": {"name": "Arctic", "quelle": "log", "zeit": "y"}}}"#,
        )
        .unwrap();
        let hangar = dir.join(FARM_FILE);
        std::fs::write(&hangar, "{\"format\": 1, \"schiffe\": [{\"name\": \"Cutlass\"}]}").unwrap();
        let host = Host::new(Flavor::native(), &[("SC_BP_HOME", &dir_text)], &dir_text);
        let mut data = from_cache(&Paths::new(host));
        let glacier = data.catalog.blueprints.iter().position(|b| b.name == "Glacier").unwrap();

        data.toggle_have(glacier);
        let stock = std::fs::read_to_string(dir.join(collection::FILE_NAME)).unwrap();
        assert!(stock.contains("\"glacier\"") && stock.contains("\"quelle\": \"hand\""), "{stock}");
        assert!(stock.contains("\"arctic\""), "the rest of the collection stays: {stock}");
        assert!(dir.join("bestand.bak.json").exists(), "the collection keeps its previous version");
        assert!(data.have.contains("glacier"));
        data.toggle_have(glacier);
        assert!(!std::fs::read_to_string(dir.join(collection::FILE_NAME)).unwrap().contains("\"glacier\""));

        data.toggle_noted(glacier);
        let nl = if cfg!(windows) { "\r\n" } else { "\n" };
        let expected = [
            "{",
            " \"format\": 1,",
            " \"schiffe\": [",
            "  {",
            "   \"name\": \"Cutlass\"",
            "  }",
            " ],",
            " \"merkzettel\": [",
            "  {",
            "   \"name\": \"Glacier\",",
            "   \"ref\": \"\",",
            "   \"anzahl\": 1,",
            "   \"weg\": \"bauen\"",
            "  }",
            " ]",
            "}",
        ]
        .join(nl);
        assert_eq!(std::fs::read_to_string(&hangar).unwrap(), expected);
        assert!(data.noted.contains("glacier"));
        data.toggle_noted(glacier);
        assert!(!std::fs::read_to_string(&hangar).unwrap().contains("Glacier"));
        assert!(!data.save_failed);

        // A file there that is no hangar is left as it is.
        let other = "{\"format\": 2, \"schiffe\": {}}";
        std::fs::write(&hangar, other).unwrap();
        data.toggle_noted(glacier);
        assert_eq!(std::fs::read_to_string(&hangar).unwrap(), other);
        assert!(data.save_failed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A write that fails is not silent: the list says so, the error log
    /// keeps it, and the next write that works takes the message away.
    #[test]
    fn a_failed_write_is_shown_and_logged() {
        use std::sync::{Arc, Mutex};
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let _language = crate::language_lock();
        let logged: Arc<Mutex<Vec<String>>> = Arc::default();
        let sink = Arc::clone(&logged);
        versekit_core::errors::set_sink(move |label, _| sink.lock().unwrap().push(label.to_string()));
        let dir = std::env::temp_dir().join(format!("versekit-fehler-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        std::fs::write(
            dir.join(catalog_cache::CACHE_FILE),
            r#"{"version": "4.3.1", "bauplaene": {"b": {"n": "Glacier", "a": "Cooler"}}}"#,
        )
        .unwrap();
        // A farm notes file that is no hangar is not overwritten: the write fails.
        std::fs::write(dir.join(FARM_FILE), "{\"format\": 2, \"schiffe\": {}}").unwrap();
        let host = Host::new(Flavor::native(), &[("SC_BP_HOME", &dir_text)], &dir_text);
        let mut data = from_cache(&Paths::new(host));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut state = ListState::default();
        let message = t_de("s_bp_speichern_fehler");
        assert!(!frame(&ctx, &mut data, &mut state).contains(&message));

        data.toggle_noted(0);
        assert!(data.save_failed);
        let texts = frame(&ctx, &mut data, &mut state);
        assert!(texts.contains(&message), "{texts:?}");
        assert!(logged.lock().unwrap().iter().any(|l| l == "blueprint_list.save_notes"));

        // A write that works takes the message away.
        data.toggle_have(0);
        assert!(!data.save_failed);
        assert!(!frame(&ctx, &mut data, &mut state).contains(&message));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// After a check the export folder holds the four output files of 4.12,
    /// each matching what the export module makes from the saved collection.
    #[test]
    fn a_check_writes_the_output_files() {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let dir = std::env::temp_dir().join(format!("versekit-ablage-{}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir_text = dir.to_string_lossy().into_owned();
        std::fs::write(
            dir.join(catalog_cache::CACHE_FILE),
            r#"{"version": "4.3.1", "bauplaene": {"glacier": {"n": "Glacier", "a": "Cooler", "s": 1, "g": 2}}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join(collection::FILE_NAME),
            r#"{"version": 3, "stand": "x", "bauplaene": {"arctic": {"name": "Arctic", "quelle": "log", "zeit": "2026-08-24 07:57:59"}}}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join(export::CONTRACT_LOG_FILE),
            r#"{"format": 2, "auftraege": [{"name": "Hilfe <EM4>[BP]</EM4>", "wann": "2026-09-01T10:00:00"}]}"#,
        )
        .unwrap();
        let old = dir.join(export::FOLDER).join("scmdb-import-2026-09-13.json");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, "alt").unwrap();
        let host = Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &dir_text)], &dir_text);
        let paths = Paths::new(host);
        let mut data = from_cache(&paths);
        let glacier = data.catalog.blueprints.iter().position(|b| b.name == "Glacier").unwrap();

        data.toggle_have(glacier);
        assert!(!data.save_failed);
        let folder = dir.join(export::FOLDER);
        let read = |kind: export::Kind| std::fs::read_to_string(folder.join(kind.file_name())).unwrap();
        let stock = match jsonio::read_json(&paths.app_file(collection::FILE_NAME)).unwrap() {
            Value::Object(map) => map,
            _ => panic!("collection is not an object"),
        };
        let tags = export::unique_tags(&paths);
        let basetool = jsonio::dumps(&export::for_basetool(&stock, &tags).unwrap(), Some(1), false);
        assert_eq!(read(export::Kind::Basetool), basetool);
        assert!(basetool.contains("\"productName\": \"Glacier\"") && basetool.contains("\"receivedAt\""));
        let log = export::contract_log(&paths).unwrap();
        assert_eq!(read(export::Kind::Contracts), export::contract_log_json(&log));
        assert!(read(export::Kind::Contracts).contains("\"name\": \"Hilfe\""));
        let scmdb = jsonio::loads(&read(export::Kind::Scmdb)).unwrap();
        let version = env!("CARGO_PKG_VERSION");
        assert_eq!(scmdb.get("watcherVersion"), Some(&Value::str(version)));
        let expected = export::for_scmdb(&stock, &tags, version, 0).unwrap();
        assert_eq!(scmdb.get("blueprints"), expected.get("blueprints"));
        let full = jsonio::loads(&read(export::Kind::Full)).unwrap();
        let expected = export::complete(&stock, &export::catalog_view(&paths), 0).unwrap();
        assert_eq!(full.get("bauplaene"), expected.get("bauplaene"));
        assert_eq!(full.get("anzahl"), Some(&Value::int(2)));
        assert!(!old.exists() && folder.join(export::OLD_FOLDER).join("scmdb-import-2026-09-13.json").exists());

        data.toggle_have(glacier);
        assert!(!data.save_failed);
        let basetool = read(export::Kind::Basetool);
        assert!(!basetool.contains("Glacier") && basetool.contains("\"productName\": \"Arctic\""), "{basetool}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An empty data folder of its own for a test.
    fn empty_paths(name: &str) -> Paths {
        use versekit_core::paths::Host;
        use versekit_core::pypath::Flavor;

        let dir = std::env::temp_dir().join(format!("versekit-katalog-{}-{name}", versekit_core::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.to_string_lossy().into_owned();
        Paths::new(Host::new(Flavor::native(), &[(versekit_core::legacy::ENV_HOME, &text)], &text))
    }

    /// Every text a frame drew.
    fn drawn_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn collect(shape: &egui::epaint::Shape, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
        out
    }

    /// One frame of the list as the window runs it: take over a finished
    /// catalog, then draw. Returns the texts drawn.
    fn frame(ctx: &egui::Context, data: &mut ListData, state: &mut ListState) -> Vec<String> {
        frame_at(ctx, data, state, export::now_unix(), 900.0).0
    }

    /// One frame of the list in a page `width` wide with the clock at `now`: the texts drawn and the
    /// lines that were cut short or reach out of their room.
    fn frame_at(
        ctx: &egui::Context,
        data: &mut ListData,
        state: &mut ListState,
        now: i64,
        width: f32,
    ) -> (Vec<String>, Vec<String>) {
        frame_sized(ctx, data, state, now, egui::vec2(width, 600.0))
    }

    /// [`frame_at`] in a page of `size`.
    fn frame_sized(
        ctx: &egui::Context,
        data: &mut ListData,
        state: &mut ListState,
        now: i64,
        size: Vec2,
    ) -> (Vec<String>, Vec<String>) {
        fn cut(shape: &egui::epaint::Shape, clip: egui::Rect, out: &mut Vec<String>) {
            match shape {
                egui::epaint::Shape::Text(text) => {
                    if text.galley.elided {
                        out.push(format!("gekürzt: {:?}", text.galley.text()));
                    }
                    for row in &text.galley.rows {
                        let rect = row.rect_without_leading_space().translate(text.pos.to_vec2());
                        // Only sideways: a row at the top or bottom edge of the scrolling list is cut by the scrolling.
                        if !row.row.text().trim().is_empty()
                            && (rect.min.x < clip.min.x - 0.5 || rect.max.x > clip.max.x + 0.5)
                        {
                            out.push(format!("abgeschnitten: {:?}", row.row.text()));
                        }
                    }
                }
                egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| cut(shape, clip, out)),
                _ => {}
            }
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };
        let scheme = versekit_core::theme::current();
        let mut output = ctx.run_ui(input, |ui| {
            data.poll(ui.ctx(), state);
            show_at(ui, scheme, data, state, now);
        });
        output.textures_delta.clear();
        let mut cut_lines = Vec::new();
        output.shapes.iter().for_each(|clipped| cut(&clipped.shape, clipped.clip_rect, &mut cut_lines));
        (drawn_texts(&output.shapes), cut_lines)
    }

    /// The catalog of the sample with one blueprint more: what a patch brings.
    fn patched(catalog: &CatalogCache) -> CatalogCache {
        let mut catalog = catalog.clone();
        let mut extra = catalog.blueprints[2].clone();
        extra.name = "Neu nach dem Patch".into();
        extra.key = name_key("Neu nach dem Patch");
        catalog.blueprints.push(extra);
        catalog
    }

    /// After a patch the list comes up with the old catalog while the new one
    /// is built in the background, and takes the new one over without a
    /// second look at the user: the window never waits for the build.
    #[test]
    fn the_first_frame_does_not_wait_for_the_build() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::{Arc, mpsc};
        use std::time::{Duration, Instant};

        let _language = crate::language_lock();
        versekit_core::language::set_language(versekit_core::language::Choice::De);
        let paths = empty_paths("first-frame");
        let stale = sample_catalog();
        let new = patched(&stale);
        let (done, in_build) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        let (flag, running) = (Arc::clone(&done), Arc::clone(&in_build));
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let repaints = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&repaints);
        // Only a request of the build thread counts, not the ones egui makes itself.
        ctx.set_repaint_observer(move |info| {
            if info.delay.is_zero() && std::thread::current().name() == Some("catalog-build") {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        let (go, wait) = mpsc::channel::<()>();
        let copy = new.clone();
        let started = Instant::now();
        let building = Pending::spawn(move || {
            running.store(true, Ordering::SeqCst);
            // A build that takes its time: it ends when the test says so.
            wait.recv_timeout(Duration::from_secs(5)).ok()?;
            flag.store(true, Ordering::SeqCst);
            Some(copy)
        });
        let mut data = ListData::load_from(&paths, Some(stale.clone()), building).expect("a list");
        let loaded = started.elapsed();
        assert!(loaded < Duration::from_millis(250), "the list waited {loaded:?} for the build");
        let mut state = ListState::default();
        let texts = frame(&ctx, &mut data, &mut state);
        assert!(!done.load(Ordering::SeqCst), "the first frame came before the build was done");
        assert!(data.is_building() && data.catalog == stale, "the old catalog is shown while the new one is built");
        assert!(texts.iter().any(|t| t.contains(&format!("von {}", stale.blueprints.len()))), "{texts:?}");

        // The build ends: the window is asked for a frame, and that frame shows the new catalog.
        repaints.store(0, Ordering::SeqCst);
        go.send(()).unwrap();
        let waited = Instant::now();
        while repaints.load(Ordering::SeqCst) == 0 {
            assert!(waited.elapsed() < Duration::from_secs(10), "the build did not wake the window");
            std::thread::sleep(Duration::from_millis(10));
        }
        let texts = frame(&ctx, &mut data, &mut state);
        assert!(!data.is_building() && data.catalog == new);
        assert!(texts.iter().any(|t| t.contains(&format!("von {}", new.blueprints.len()))), "{texts:?}");
        assert!(in_build.load(Ordering::SeqCst));
    }

    /// `load` is the way the window gets its list: with an archive that has
    /// changed since the cache was built, it starts with the cache at once. The
    /// archive here is no archive; a build that read it first would fail, and
    /// the list would have nothing to start with.
    #[test]
    fn load_starts_with_the_last_build_after_a_patch() {
        let _language = crate::language_lock();
        let paths = empty_paths("load-in");
        let folder = std::path::PathBuf::from(paths.app_folder());
        let channel = folder.join("LIVE");
        std::fs::create_dir_all(&channel).unwrap();
        let archive = channel.join(versekit_gamedata::ARCHIVE_NAME);
        std::fs::write(&archive, b"changed by a patch").unwrap();
        let built = sample_catalog();
        let Value::Object(mut stored) = catalog_cache::to_value(&built) else { panic!("an object") };
        // The stamp of a build of this archive in the layout of today, from another stand of the file.
        stored.insert(
            0,
            (
                "quelle".into(),
                Value::Object(vec![
                    ("format".into(), Value::int(list::CACHE_FORMAT)),
                    ("archiv".into(), Value::str(archive.to_string_lossy())),
                    ("archiv_stand".into(), Value::Null),
                    ("scmdb".into(), Value::str("-")),
                    ("versuch".into(), Value::str("0")),
                ]),
            ),
        );
        let cache = paths.app_file(list::CACHE_FILE);
        std::fs::write(&cache, jsonio::dumps(&Value::Object(stored), None, false)).unwrap();

        let offline = std::sync::Arc::new(list::Offline);
        let mut data =
            ListData::load_in_with(&paths, Some(&channel), &cache, offline).expect("the last build stands in");
        assert!(data.is_building());
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut state = ListState::default();
        assert_eq!(data.catalog, built);
        for _ in 0..1000 {
            frame(&ctx, &mut data, &mut state);
            if !data.is_building() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!data.is_building(), "the build of an unreadable archive ends");
        assert_eq!(data.catalog, built, "and leaves the list as it was");
    }

    /// With no catalog at all (a first start with the game installed) the list
    /// says calmly that it is being built, and shows the catalog when it comes.
    #[test]
    fn without_any_catalog_the_list_says_it_is_being_built() {
        use std::sync::mpsc;
        use versekit_core::language::{Choice, set_language};

        let _language = crate::language_lock();
        set_language(Choice::De);
        let paths = empty_paths("no-catalog");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let (go, wait) = mpsc::channel::<()>();
        let new = sample_catalog();
        let copy = new.clone();
        let building = Pending::spawn(move || {
            wait.recv().ok()?;
            Some(copy)
        });
        let mut data = ListData::load_from(&paths, None, building).expect("a list while it is built");
        let mut state = ListState::default();
        let texts = frame(&ctx, &mut data, &mut state);
        assert!(texts.iter().any(|t| *t == t_de("kat_baut")), "{texts:?}");
        assert!(!texts.iter().any(|t| *t == t_de("kein_katalog")), "{texts:?}");
        go.send(()).unwrap();
        for _ in 0..1000 {
            frame(&ctx, &mut data, &mut state);
            if !data.is_building() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(data.catalog, new);
        assert!(ListData::load_from(&paths, None, None).is_none(), "nothing to show and nothing coming: no list");
    }

    /// A build that fails leaves the list as it is; one without any catalog
    /// falls back to the plain empty page.
    #[test]
    fn a_failed_build_leaves_the_list_alone() {
        use versekit_core::language::{Choice, set_language};

        let _language = crate::language_lock();
        set_language(Choice::De);
        let paths = empty_paths("failed");
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let stale = sample_catalog();
        let mut data = ListData::load_from(&paths, Some(stale.clone()), Pending::spawn(|| None)).unwrap();
        let mut state = ListState::default();
        for _ in 0..1000 {
            frame(&ctx, &mut data, &mut state);
            if !data.is_building() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!data.is_building() && data.catalog == stale);

        let mut empty = ListData::load_from(&paths, None, Pending::spawn(|| None)).unwrap();
        for _ in 0..1000 {
            if !empty.is_building() {
                break;
            }
            frame(&ctx, &mut empty, &mut state);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let texts = frame(&ctx, &mut empty, &mut state);
        assert!(texts.iter().any(|t| *t == t_de("kein_katalog")), "{texts:?}");
    }

    /// The open row stays open at its blueprint when the catalog changes
    /// under it, and a catalog equal to the shown one changes nothing.
    #[test]
    fn the_open_row_follows_its_blueprint_across_the_swap() {
        let paths = empty_paths("swap");
        let stale = sample_catalog();
        let mut data = ListData::load_with(&paths, stale.clone());
        let at = 7;
        let key = data.catalog.blueprints[at].key.clone();
        let mut state = ListState { open: Some(at), more_ways: true, ..ListState::default() };

        assert!(!data.replace_catalog(stale.clone(), &mut state), "an equal catalog is no change");
        assert_eq!((state.open, state.more_ways), (Some(at), true));

        // Two blueprints come in before it: the row moves down by two.
        let mut moved = stale.clone();
        for name in ["Zwischen Eins", "Zwischen Zwei"] {
            let mut extra = moved.blueprints[0].clone();
            extra.name = name.into();
            extra.key = name_key(name);
            moved.blueprints.insert(0, extra);
        }
        assert!(data.replace_catalog(moved.clone(), &mut state));
        assert_eq!(state.open, Some(at + 2));
        assert_eq!(data.catalog.blueprints[at + 2].key, key);
        assert_eq!(data.categories.len(), moved.blueprints.len(), "what hangs on the catalog is worked out again");
        assert!(state.more_ways);

        // Its blueprint is gone: the row closes.
        let mut gone = moved.clone();
        gone.blueprints.retain(|b| b.key != key);
        assert!(data.replace_catalog(gone, &mut state));
        assert_eq!((state.open, state.more_ways), (None, false));
    }

    /// A find the overlay writes into the collection shows in the open list, without opening it anew; the file
    /// is looked at once a second, and one that did not change is not read again.
    #[test]
    fn a_find_written_by_the_overlay_shows_in_the_open_list() {
        let paths = empty_paths("fund-vom-overlay");
        let mut data = ListData::load_with(&paths, sample_catalog());
        let name = data.catalog.blueprints[0].name.clone();
        assert!(!data.have.contains(&name_key(&name)));
        assert!(!data.follow_collection(0.0), "nothing written since the list was read");
        // The overlay, in a process of its own, writes the find.
        let collection = Collection {
            paths: &paths,
            catalog: &collection::NoCatalog,
            names: &NoEnglishNames,
            exports: &collection::NoExports,
            clock: collection::now_text,
        };
        let mut stock = collection.load();
        assert!(collection.add(&mut stock, &name, "log", None));
        assert!(collection.save(&mut stock));
        assert!(!data.follow_collection(0.5), "looked at once a second");
        assert!(data.follow_collection(1.0), "the second after: read again");
        assert!(data.have.contains(&name_key(&name)), "the find stands in the list");
        assert!(data.received.contains_key(&name_key(&name)), "with its time");
        assert!(!data.follow_collection(2.0), "unchanged: not read again");
    }

    /// Blueprints without a known way stay out of a catalog that comes later,
    /// as of the first.
    #[test]
    fn a_later_catalog_keeps_hiding_blueprints_without_a_way() {
        let paths = empty_paths("hidden");
        let mut data = ListData::load_with(&paths, sample_catalog());
        assert!(data.hide_no_way);
        let mut with = sample_catalog();
        with.blueprints[0].no_way = true;
        let total = with.blueprints.len();
        let mut state = ListState::default();
        assert!(data.replace_catalog(with.clone(), &mut state));
        assert_eq!(data.catalog.blueprints.len(), total - 1);

        // With `alle_bauplaene` on, they come in.
        let mut all = ListData::load_with(&paths, sample_catalog());
        all.hide_no_way = false;
        assert!(all.replace_catalog(with, &mut state));
        assert_eq!(all.catalog.blueprints.len(), total);
    }

    /// The switch of the page Erkennung brings the blueprints without a way in and takes them out again,
    /// without a new start.
    #[test]
    fn the_switch_for_blueprints_without_a_way_changes_the_list_at_once() {
        let paths = empty_paths("schalter");
        let mut data = ListData::load_with(&paths, sample_catalog());
        let mut with = sample_catalog();
        with.blueprints[0].no_way = true;
        let total = with.blueprints.len();
        let mut state = ListState::default();
        assert!(data.follow_all(with.clone(), false, &mut state) || data.catalog.blueprints.len() == total - 1);
        assert_eq!(data.catalog.blueprints.len(), total - 1, "off: the one without a way stays out");
        assert!(data.follow_all(with.clone(), true, &mut state), "on: it comes in");
        assert_eq!(data.catalog.blueprints.len(), total);
        assert!(data.follow_all(with, false, &mut state), "off again: it goes");
        assert_eq!(data.catalog.blueprints.len(), total - 1);
    }

    /// A filter choice that the new catalog no longer offers is dropped, one
    /// it still offers stays.
    #[test]
    fn a_filter_choice_the_new_catalog_lacks_is_dropped() {
        let paths = empty_paths("picks");
        let mut data = ListData::load_with(&paths, sample_catalog());
        let mut state = ListState::default();
        state.picks[3] = "9".into();
        state.picks[4] = "f:Foxwell Enforcement".into();
        let choices = filter_choices(&data, "");
        assert!(choices[4].iter().any(|(v, _)| v == "f:Foxwell Enforcement"), "the sample offers it");
        let mut new = sample_catalog();
        for blueprint in &mut new.blueprints {
            for way in &mut blueprint.ways {
                if way.faction == "Foxwell Enforcement" {
                    way.faction = "Covalex Shipping".into();
                }
            }
        }
        assert!(data.replace_catalog(new, &mut state));
        assert_eq!(state.picks[3], "", "size 9 does not exist");
        assert_eq!(state.picks[4], "", "no way is offered by that faction any more");
        state.picks[2] = "Military".into();
        assert!(data.replace_catalog(sample_catalog(), &mut state));
        assert_eq!(state.picks[2], "Military", "a choice that is still offered stays");
    }

    /// The text of a key in German, whatever language the test run has set.
    fn t_de(key: &str) -> String {
        versekit_core::language::set_language(versekit_core::language::Choice::De);
        t(key)
    }

    #[test]
    fn patterns_at_word_bounds() {
        assert!(at_word_bounds("abc-mk4 legs grey", "mk4 legs"));
        assert!(!at_word_bounds("warden backpack purgatory camo", "arden backpack"));
        assert!(at_word_bounds("warden backpack", "warden backpack"));
    }

    #[test]
    fn sample_has_no_grades_where_the_game_has_none() {
        let data = ListData::sample();
        let helmet = data.catalog.blueprints.iter().find(|b| b.kind == "Char_Armor_Helmet").unwrap();
        let cooler = data.catalog.blueprints.iter().find(|b| b.kind == "Cooler").unwrap();
        assert!(!sub_line(&data, helmet).contains(" · A"), "{}", sub_line(&data, helmet));
        assert!(cooler.class.is_some() && helmet.class.is_none());
        assert!(data.grade_kinds.contains("Cooler") && !data.grade_kinds.contains("Char_Armor_Helmet"));
    }

    /// A jump from the progress to a kind, then a category picked in the filter: the list shows what the menu
    /// counts for that category, not the empty intersection. And a jump after a picked category drops it.
    #[test]
    fn a_category_picked_after_a_jump_from_the_progress_shows_what_its_menu_counts() {
        let data = ListData::sample();
        let shown = |state: &ListState| -> Vec<usize> {
            (0..data.catalog.blueprints.len()).filter(|&i| passes(&data, state, i, "", None)).collect()
        };
        let helmet = data.catalog.blueprints.iter().position(|b| b.kind == "Char_Armor_Helmet").expect("a helmet");
        let cooler = data.catalog.blueprints.iter().position(|b| b.kind == "Cooler").expect("a cooler");
        let category = data.categories[cooler].0.clone();
        assert_ne!(category, data.categories[helmet].0, "the sample: helmet and cooler in different categories");
        let mut state = ListState::default();
        state.jump(ListJump::Kind(bp::kind_readable("Char_Armor_Helmet")), "Fortschritt".into());
        assert!(shown(&state).contains(&helmet) && !shown(&state).contains(&cooler), "the jump shows the helmets");
        // The category as the menu offers it, with its count.
        let counted: usize = filter_choices(&data, "")[0]
            .iter()
            .find(|(value, _)| *value == category)
            .and_then(|(_, label)| label.rsplit('(').next()?.trim_end_matches(')').parse().ok())
            .expect("the menu counts the category");
        state.picks[0] = category.clone();
        state.category_picked();
        let rows = shown(&state);
        assert_eq!(rows.len(), counted, "the list shows what the menu counts: {rows:?}");
        assert!(rows.contains(&cooler) && state.kind.is_none() && state.jump.is_none());
        // The other way round: a jump drops the picked category.
        state.jump(ListJump::Kind(bp::kind_readable("Char_Armor_Helmet")), "Fortschritt".into());
        assert!(state.picks[0].is_empty() && shown(&state).contains(&helmet));
        // A contract stays when a category is picked, and so does the label that loosens it.
        let mut contract = ListState::default();
        contract.jump(ListJump::Contract("Auftrag".into(), BTreeSet::new()), "Aufträge".into());
        contract.picks[0] = category;
        contract.category_picked();
        assert!(contract.contract.is_some() && contract.jump.is_some());
    }

    /// The subtypes come from the recipe the catalog carries (`rezept_*`, as the build from the game data writes
    /// them): ship weapons split into gatling and repeater, hand weapons into pistol and rifle, and a picked subtype
    /// narrows the list to it.
    #[test]
    fn the_subtypes_of_the_list_come_from_the_recipe() {
        let _language = crate::language_lock();
        let source = r#"{"version": "4.12.0-live.1", "bauplaene": {
            "a": {"n": "Alpha Gatling", "a": "WeaponGun", "sub": "Gun", "s": 4,
                "rezept_tag": "BP_CRAFT_APAR_BallisticGatling_S4", "rezept_unterart": "ballistic", "rezept_art": "weapons"},
            "b": {"n": "Beta Repeater", "a": "WeaponGun", "sub": "Gun", "s": 2,
                "rezept_tag": "BP_CRAFT_KLWE_LaserRepeater_S2", "rezept_unterart": "laser", "rezept_art": "weapons"},
            "c": {"n": "Gamma Pistol", "a": "WeaponPersonal", "sub": "Small",
                "rezept_tag": "BP_CRAFT_behr_pistol_ballistic_01", "rezept_unterart": "pistol", "rezept_art": "weapons"},
            "d": {"n": "Delta Rifle", "a": "WeaponPersonal", "sub": "Medium",
                "rezept_tag": "BP_CRAFT_behr_rifle_ballistic_01", "rezept_unterart": "rifle", "rezept_art": "weapons"},
            "e": {"n": "Epsilon Gun", "a": "WeaponGun", "sub": "Gun"}
        }}"#;
        let catalog = catalog_cache::parse(&jsonio::loads(source).expect("the catalog"));
        let data = ListData::new(catalog, HashSet::new(), HashSet::new(), HashSet::new());
        let subtypes = |kind: &str| -> BTreeSet<String> {
            filter_choices(&data, kind)[1].iter().skip(1).map(|(value, _)| value.clone()).collect()
        };
        assert_eq!(
            subtypes("schiffswaffe"),
            BTreeSet::from(["ballistic_gatling".to_string(), "laser_repeater".to_string()]),
            "the ship weapons split by the tag of their recipe"
        );
        assert_eq!(
            subtypes("fpswaffe"),
            BTreeSet::from(["gewehr".to_string(), "pistole".to_string()]),
            "the hand weapons split by the subtype of their recipe"
        );
        let mut state = ListState::default();
        state.picks[0] = "schiffswaffe".into();
        assert_eq!(shown_keys(&data, &state).len(), 3, "a blueprint without a recipe stays in its category");
        state.picks[1] = "ballistic_gatling".into();
        assert_eq!(shown_keys(&data, &state), BTreeSet::from(["alpha gatling".to_string()]));
    }

    /// A blueprint with one way, `further` more ways and the contract of way `w` named `Auftrag {w}`.
    fn with_ways(further: usize) -> ListData {
        let ways = (0..=further)
            .map(|w| Way {
                contract: format!("Auftrag {w}"),
                kind: "Mercenary".into(),
                faction: "Foxwell Enforcement".into(),
                reward: Some(25_000),
                rank: Some("Contractor".into()),
                rank_reputation: Some(2_200),
                place: Some(Place { system: "Stanton".into(), places: vec!["Hurston".into()], more: 0 }),
                ..Way::default()
            })
            .collect();
        let blueprint = Blueprint {
            key: "viele wege".into(),
            name: "Viele Wege".into(),
            kind: "Cooler".into(),
            ways,
            ..Blueprint::default()
        };
        let catalog = CatalogCache { blueprints: vec![blueprint], ..CatalogCache::default() };
        ListData::new(catalog, HashSet::new(), HashSet::new(), HashSet::new())
    }

    /// The further ways roll in a room of their own: with many of them the box grows by no more than the height
    /// limit and the last way waits below; with few the room shrinks to them and shows all. Nothing is cut, in any
    /// size and both languages.
    #[test]
    fn many_further_ways_roll_in_a_room_of_their_own() {
        use versekit_core::language::Choice;
        let _language = crate::language_lock();
        let now = export::now_unix();
        let mut findings = Vec::new();
        for choice in [Choice::De, Choice::En] {
            versekit_core::language::set_language(choice);
            let (smallest, largest) = versekit_core::font_size::POINTS_RANGE;
            for points in [smallest, largest] {
                for width in [560.0, 1600.0] {
                    let place = format!("{choice:?} Größe {points} Breite {width}");
                    let ctx = egui::Context::default();
                    crate::install(&ctx);
                    crate::apply_size(&ctx, points);
                    // The height of the box with the further ways folded and unfolded.
                    let height = |further: usize, unfolded: bool| {
                        let mut data = with_ways(further);
                        let mut state = ListState { open: Some(0), more_ways: unfolded, ..ListState::default() };
                        let (mut drawn, mut cut) = (Vec::new(), Vec::new());
                        for _ in 0..4 {
                            (drawn, cut) = frame_sized(&ctx, &mut data, &mut state, now, egui::vec2(width, 1000.0));
                        }
                        (state.detail_height, drawn, cut)
                    };
                    // The page of the test frames is 1000 high.
                    let limit = further_ways_limit(1000.0);
                    let (folded, _, _) = height(30, false);
                    let (unfolded, drawn, cut) = height(30, true);
                    let grown = unfolded - folded;
                    if grown > limit + 12.0 {
                        findings.push(format!("{place}: 30 ways grow the box by {grown} (limit {limit})"));
                    }
                    let first = drawn.iter().any(|d| d.contains("Auftrag 1"));
                    if !first || drawn.iter().any(|d| d.contains("Auftrag 30")) {
                        findings.push(format!("{place}: not the first further way at the top of its room"));
                    }
                    if !cut.is_empty() {
                        findings.push(format!("{place}: {cut:?}"));
                    }
                    let (_, drawn, cut) = height(2, true);
                    if !drawn.iter().any(|d| d.contains("Auftrag 2")) {
                        findings.push(format!("{place}: two further ways do not all show"));
                    }
                    if !cut.is_empty() {
                        findings.push(format!("{place}: {cut:?}"));
                    }
                }
            }
        }
        versekit_core::language::set_language(Choice::De);
        assert!(findings.is_empty(), "weitere Wege:\n{}", findings.join("\n"));
    }

    #[test]
    fn states_split_the_list() {
        let data = ListData::sample();
        let state = ListState::default();
        let all: Vec<usize> =
            (0..data.catalog.blueprints.len()).filter(|&i| passes(&data, &state, i, "", None)).collect();
        let have = all.iter().filter(|&&i| fits_state(&data, i, 1)).count();
        let missing = all.iter().filter(|&&i| fits_state(&data, i, 2)).count();
        assert_eq!(have + missing, all.len());
        assert!(all.iter().any(|&i| fits_state(&data, i, 6)), "some can be lost by rising");
    }

    /// The seconds of a local time `YYYY-MM-DD HH:MM:SS`.
    fn at(text: &str) -> i64 {
        export::local_stamp(text).expect("a readable time")
    }

    /// A time stamp as the collection writes it.
    fn collection_time(seconds: i64) -> String {
        let time = formats::local_time(seconds).expect("a local time");
        formats::strftime("%Y-%m-%d %H:%M:%S", &time)
    }

    /// The sample list where only the first blueprints are owned, received at `times` from `sources`.
    fn received_list(times: &[&str], sources: &[&str]) -> ListData {
        let mut data = ListData::sample();
        data.have.clear();
        data.received.clear();
        for (n, (blueprint, time)) in data.catalog.blueprints.clone().iter().zip(times).enumerate() {
            data.have.insert(blueprint.key.clone());
            data.received.insert(blueprint.key.clone(), (at(time), sources[n % sources.len()].to_string()));
        }
        data
    }

    fn keys_of(data: &ListData, numbers: &[usize]) -> BTreeSet<String> {
        numbers.iter().map(|&n| data.catalog.blueprints[n].key.clone()).collect()
    }

    /// Each period lists exactly the blueprints received in it: the edges of the day, the rolling days and
    /// the month to the second, only what one has, together with the other filters.
    #[test]
    fn each_period_lists_exactly_what_came_in_it() {
        let now = at("2026-10-22 20:00:00");
        let mut data = received_list(
            &[
                "2026-10-22 10:00:00",
                "2026-10-21 23:59:59",
                "2026-10-15 20:00:00",
                "2026-10-15 19:59:59",
                "2026-10-08 20:00:00",
                "2026-10-08 19:59:59",
                "2026-10-01 00:00:00",
                "2026-09-30 23:59:59",
            ],
            &["log"],
        );
        // Received once, but taken out of the collection since: no longer owned, so not listed.
        let gone = data.catalog.blueprints[8].key.clone();
        data.received.insert(gone, (at("2026-10-22 12:00:00"), "log".into()));
        let mut state = ListState::default();
        let expected = [
            (Period::Today, vec![0]),
            (Period::Days7, vec![0, 1, 2]),
            (Period::Days14, vec![0, 1, 2, 3, 4]),
            (Period::Month, vec![0, 1, 2, 3, 4, 5, 6]),
        ];
        for (period, numbers) in expected {
            state.period = period;
            assert_eq!(shown_keys_at(&data, &state, now), keys_of(&data, &numbers), "{period:?}");
            // "Have it" changes nothing, "missing" leaves nothing.
            state.state = 1;
            assert_eq!(shown_keys_at(&data, &state, now), keys_of(&data, &numbers), "{period:?} have");
            state.state = 2;
            assert!(shown_keys_at(&data, &state, now).is_empty(), "{period:?} missing");
            state.state = 0;
        }
        state.period = Period::All;
        assert_eq!(shown_keys_at(&data, &state, now).len(), data.catalog.blueprints.len());
        // A search narrows within the period.
        state.period = Period::Month;
        state.search = data.catalog.blueprints[2].name.to_lowercase();
        assert!(shown_keys_at(&data, &state, now).contains(&data.catalog.blueprints[2].key));
        assert!(!shown_keys_at(&data, &state, now).contains(&data.catalog.blueprints[0].key));
    }

    /// The filter counts each period as the list shows it, and a period narrows the list and resets with it.
    #[test]
    fn the_period_filter_counts_narrows_and_resets() {
        let now = at("2026-10-22 20:00:00");
        let data = received_list(
            &["2026-10-22 10:00:00", "2026-10-20 10:00:00", "2026-10-10 10:00:00", "2026-09-01 10:00:00"],
            &["log"],
        );
        let counts = period_counts(&data, now);
        assert_eq!(counts[1..], [1, 2, 3, 3], "today, 7 days, 14 days, month");
        for (period, count) in Period::EVERY.into_iter().zip(counts).skip(1) {
            let state = ListState { period, ..ListState::default() };
            assert_eq!(shown_keys_at(&data, &state, now).len(), count, "{period:?}");
        }
        let mut state = ListState::default();
        assert!(!state.is_narrowed());
        state.period = Period::Days14;
        assert!(state.is_narrowed());
        state.reset();
        assert_eq!(state.period, Period::All);
        assert!(!state.is_narrowed());
        // A jump brings the period along, labelled with it.
        state.jump(ListJump::Received(Period::Days7), "x".into());
        assert_eq!((state.period, state.jump.as_deref()), (Period::Days7, Some("x")));
    }

    /// With a period the list stands flat, the newest first, each row with its time and source; blueprints of
    /// the same second by name. The period is a field of the filter row, and an empty period says so.
    #[test]
    fn a_period_lists_flat_newest_first_with_time_and_source() {
        let _language = crate::language_lock();
        versekit_core::language::set_language(versekit_core::language::Choice::De);
        let now = at("2026-10-22 20:00:00");
        let mut data = received_list(
            &["2026-10-21 08:00:00", "2026-10-22 09:30:00", "2026-10-21 08:00:00", "2026-10-22 18:15:00"],
            &["log", "hand", "basetool", "start"],
        );
        let names: Vec<String> = data.catalog.blueprints[..4].iter().map(|b| b.name.clone()).collect();
        let mut state = ListState { period: Period::Days7, ..ListState::default() };
        let ctx = egui::Context::default();
        crate::install(&ctx);
        let mut drawn = Vec::new();
        for _ in 0..3 {
            drawn = frame_at(&ctx, &mut data, &mut state, now, 900.0).0;
        }
        let position =
            |text: &str| drawn.iter().position(|d| d == text).unwrap_or_else(|| panic!("{text:?} in {drawn:?}"));
        // 18:15 (#3), 09:30 (#1), then the two of 21.10. 08:00 by name (#0 and #2).
        let mut by_name = [names[0].clone(), names[2].clone()];
        by_name.sort_by_key(|n| n.to_lowercase());
        let order = [names[3].clone(), names[1].clone(), by_name[0].clone(), by_name[1].clone()];
        let places: Vec<usize> = order.iter().map(|n| position(n)).collect();
        assert!(places.windows(2).all(|w| w[0] < w[1]), "{order:?} at {places:?}");
        assert!(drawn.iter().any(|d| d.starts_with("22.10.2026 18:15 · Start")), "{drawn:?}");
        assert!(drawn.iter().any(|d| d.starts_with("22.10.2026 09:30 · Von Hand")), "{drawn:?}");
        assert!(drawn.iter().any(|d| d.starts_with("21.10.2026 08:00 · Spiel")), "{drawn:?}");
        assert!(drawn.iter().any(|d| d.starts_with("21.10.2026 08:00 · Basetool")), "{drawn:?}");
        // The field of the filter row names the period and counts it.
        assert!(drawn.contains(&format!("{} (4)", t("erhalten_7"))), "{drawn:?}");
        // No group heads in the flat list.
        let heads: Vec<String> = bp::groups(&data.catalog, &[0, 1]).iter().map(|g| g.title.to_uppercase()).collect();
        assert!(heads.iter().all(|h| !drawn.contains(h)), "{heads:?} in {drawn:?}");
        // The same in English.
        versekit_core::language::set_language(versekit_core::language::Choice::En);
        let drawn = frame_at(&ctx, &mut data, &mut state, now, 900.0).0;
        assert!(drawn.iter().any(|d| d.starts_with("2026-10-22 18:15 · Start")), "{drawn:?}");
        assert!(drawn.iter().any(|d| d.starts_with("2026-10-22 09:30 · By hand")), "{drawn:?}");
        // Nothing came today after 20:00 of another day: the empty period says so.
        state.period = Period::Today;
        let drawn = frame_at(&ctx, &mut data, &mut state, at("2026-10-25 12:00:00"), 900.0).0;
        assert!(drawn.contains(&t("erhalten_leer")), "{drawn:?}");
        versekit_core::language::set_language(versekit_core::language::Choice::De);
        assert!(drawn.iter().all(|d| !names.contains(d)), "no row in an empty period: {drawn:?}");
    }

    /// The tile counts "this week" over every entry of the collection; the list with "7 days" shows exactly
    /// those in the catalog and says how many more the catalog does not hold.
    #[test]
    fn the_tile_and_the_list_count_the_same_week() {
        let paths = empty_paths("zeitraum-zaehlung");
        let now = export::now_unix();
        let catalog = sample_catalog();
        let entry = |name: &str, seconds: i64, source: &str| {
            format!(
                r#""{}": {{"name": "{name}", "quelle": "{source}", "zeit": "{}"}}"#,
                name_key(name),
                collection_time(seconds)
            )
        };
        let names: Vec<&str> = catalog.blueprints.iter().map(|b| b.name.as_str()).collect();
        let entries = [
            entry(names[0], now - 3_600, "log"),
            entry(names[1], now - 3 * 86_400, "basetool"),
            entry(names[2], now - 6 * 86_400, "hand"),
            entry(names[3], now - 8 * 86_400, "import"),
            entry(names[4], now - 20 * 86_400, "start"),
            // Not in the catalog: the tile counts it, the list cannot show it.
            entry("Unbekannter Entwurf", now - 7_200, "log"),
        ];
        std::fs::write(paths.app_file(collection::FILE_NAME), format!(r#"{{"bauplaene": {{{}}}}}"#, entries.join(",")))
            .unwrap();
        let mut data = ListData::load_with(&paths, catalog.clone());
        let own = match jsonio::read_json(&paths.app_file(collection::FILE_NAME)).unwrap() {
            Value::Object(map) => collection::blueprints(&map).to_vec(),
            _ => panic!("a collection"),
        };
        let keys: HashSet<&str> = catalog.blueprints.iter().map(|b| b.key.as_str()).collect();
        let week = overview::blueprint_numbers(&own, &|key| keys.contains(key), catalog.blueprints.len(), now).week;
        assert_eq!(week, 4);
        let state = ListState { period: Period::Days7, ..ListState::default() };
        let listed = shown_keys_at(&data, &state, now).len();
        let outside = outside_catalog(&data, Period::Days7.since(now).unwrap());
        assert_eq!((listed, outside), (3, 1));
        assert_eq!(listed + outside, week, "the tile's +N is the rows plus the note");
        // A blueprint ticked by hand comes into "today" with its time, and out again when unticked.
        let state = ListState { period: Period::Today, ..ListState::default() };
        let before = shown_keys_at(&data, &state, now).len();
        let missing = (0..catalog.blueprints.len()).find(|&i| !data.have.contains(&catalog.blueprints[i].key)).unwrap();
        data.toggle_have(missing);
        assert_eq!(shown_keys_at(&data, &state, now).len(), before + 1);
        assert_eq!(data.received[&catalog.blueprints[missing].key].1, "hand");
        data.toggle_have(missing);
        assert_eq!(shown_keys_at(&data, &state, now).len(), before);
        let _ = std::fs::remove_dir_all(paths.app_folder());
    }

    /// The row of a period with its time and source, the field of the filter row and the note about what the
    /// catalog lacks are cut nowhere: not at the narrowest page, not in any size, in neither language, also
    /// with the longest source and a manufacturer that does not fit beside it.
    #[test]
    fn the_period_rows_cut_nothing_at_any_size_in_both_languages() {
        use versekit_core::language::Choice;
        let _language = crate::language_lock();
        let now = at("2026-10-22 20:00:00");
        let times: Vec<String> = (0..60).map(|n| collection_time(now - 3_000 * n)).collect();
        let times: Vec<&str> = times.iter().map(String::as_str).collect();
        let mut findings = Vec::new();
        for choice in [Choice::De, Choice::En] {
            versekit_core::language::set_language(choice);
            let (smallest, largest) = versekit_core::font_size::POINTS_RANGE;
            for points in [smallest, (smallest + largest) / 2, largest] {
                for width in [560.0, 620.0, 900.0, 1600.0] {
                    for period in [Period::Today, Period::Days7, Period::Days14, Period::Month] {
                        let mut data =
                            received_list(&times, &["log", "hand", "basetool", "import", "start", "launcher"]);
                        // One in the collection that the catalog lacks, so the note shows.
                        data.received.insert("nicht im katalog".into(), (now - 60, "log".into()));
                        let mut state = ListState { period, ..ListState::default() };
                        let ctx = egui::Context::default();
                        crate::install(&ctx);
                        crate::apply_size(&ctx, points);
                        let mut cut = Vec::new();
                        let mut drawn = Vec::new();
                        for _ in 0..3 {
                            (drawn, cut) = frame_at(&ctx, &mut data, &mut state, now, width);
                        }
                        let place = format!("{choice:?} Größe {points} Breite {width} {period:?}");
                        if !cut.is_empty() {
                            findings.push(format!("{place}: {cut:?}"));
                        }
                        if !drawn.iter().any(|d| d.contains(" · ") && d.contains(&collection_time_shown(now)))
                            && width >= 620.0
                        {
                            findings.push(format!("{place}: no row with its time"));
                        }
                    }
                }
            }
        }
        versekit_core::language::set_language(Choice::De);
        assert!(findings.is_empty(), "abgeschnitten:\n{}", findings.join("\n"));
    }

    /// The date part of the newest row at `now` as the list writes it in the current language.
    fn collection_time_shown(now: i64) -> String {
        formats::date_time(language::current(), now as f64)
    }
}
