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

//! The start page "Übersicht" as the settings keep it (`overview.py`,
//! `cards.py`): the tiles "at a glance" in their order and which are hidden,
//! the pages pinned as "your cards", and which of the two areas comes first.
//! The keys and their values are those of 4.12, so both read the same file.
//! The size of a tile is new in v5 and has a key of its own.

use crate::jsonio::{Map, Value};
use crate::paths::Paths;

/// The tiles in their order from the factory: (id, text key of the title)
/// (`overview.TILES`).
pub const TILES: [(&str, &str); 17] = [
    ("spiel", "s_uv_spiel"),
    ("standort", "s_uv_standort"),
    ("bauplaene", "s_uv_bauplaene"),
    ("uebersetzung", "s_uv_uebersetzung"),
    ("spielzeit", "s_uv_spielzeit"),
    ("rechner", "s_uv_rechner"),
    ("dienste", "s_uv_dienste"),
    ("raffinerie", "s_uv_raffinerie"),
    ("lager", "s_uv_lager"),
    ("auftraege", "s_uv_auftraege"),
    ("neu_patch", "s_uv_neu_patch"),
    ("merkliste", "s_uv_merkliste"),
    ("bedarf", "s_uv_bedarf"),
    ("funde", "s_uv_funde"),
    ("sicherung", "s_uv_sicherung"),
    ("notizen", "s_uv_notizen"),
    // New in v5: the Executive Hangar of the Contested Zones with its running timer.
    ("executive_hangar", "s_cz_hangar"),
];

/// The page each tile's "Öffnen" leads to, by the page names of 4.12.
pub const TILE_TARGETS: [(&str, &str); 17] = [
    ("spiel", "ordner"),
    ("standort", "statistik_quantum"),
    ("bauplaene", "fortschritt"),
    ("uebersetzung", "uebersetzung"),
    ("spielzeit", "statistik"),
    ("rechner", "grafik"),
    ("dienste", "serverstatus"),
    ("raffinerie", "meinlager"),
    ("lager", "meinlager"),
    ("auftraege", "auftragslog"),
    ("neu_patch", "liste"),
    ("merkliste", "fortschritt"),
    ("bedarf", "bedarf"),
    ("funde", "liste"),
    ("sicherung", "bestand"),
    ("notizen", "notizen"),
    ("executive_hangar", "contested"),
];

/// The hidden tiles, a list of ids (`overview.HIDDEN_SETTING`).
pub const HIDDEN_SETTING: &str = "uebersicht_aus";
/// All tile ids in the order the user gave them (`overview.ORDER_SETTING`).
pub const ORDER_SETTING: &str = "uebersicht_reihenfolge";
/// Which of the two lower areas stands on top: `kacheln` or `karten`
/// (`overview.ORDER_SECTIONS_SETTING`).
pub const FIRST_SETTING: &str = "startseite_zuerst";
/// The pinned pages, a list of page names in the order shown
/// (`cards.PINNED_SETTING`).
pub const PINNED_SETTING: &str = "karten_angeheftet";
/// Whether the play time shows in the head bar; while the tiles were never
/// chosen, the play time tile follows it.
pub const PLAYTIME_HEAD_SETTING: &str = "spielzeit_zeigen";
/// New in v5: the size of each tile, `{id: "klein" | "breit" | "hoch" | "gross"}`.
pub const SIZE_SETTING: &str = "uebersicht_groessen";
/// New in v5: the start page in one area instead of two.
pub const ONE_AREA_SETTING: &str = "uebersicht_ein_bereich";

/// The pages that can be pinned: (page name of 4.12, text key of its name in
/// v5) (`cards.PAGES`).
pub const PAGES: [(&str, &str); 42] = [
    ("liste", "hf_liste"),
    ("fortschritt", "hf_fortschritt"),
    ("auftragslog", "hf_auftragslog"),
    ("hangar", "hf_hangar"),
    ("einkaufsliste", "hf_einkaufsliste"),
    ("notizen", "hf_notizen"),
    ("contested", "hf_contested"),
    ("herstellung", "hf_herstellung"),
    ("farmliste", "hf_farmliste"),
    ("bedarf", "hf_bedarf"),
    ("staffellager", "hf_staffellager"),
    ("meinlager", "hf_meinlager"),
    ("bergbau", "hf_bergbau"),
    ("raffinerien", "hf_raffinerien"),
    ("bergung", "hf_bergung"),
    ("laeden", "hf_laeden"),
    ("verkauf", "hf_verkauf"),
    ("routen", "hf_routen"),
    ("statistik", "hf_st_uebersicht"),
    ("statistik_schiffe", "hf_st_schiffe"),
    ("statistik_auftraege", "hf_st_auftraege"),
    ("statistik_quantum", "hf_st_quantum"),
    ("statistik_stabil", "hf_st_stabil"),
    ("allgemein", "hf_allgemein"),
    ("anzeige", "hf_overlay"),
    ("darstellung", "hf_darstellung"),
    ("ordner", "hf_ordner"),
    ("module", "hf_module"),
    ("basetool", "k_basetool_verbindung"),
    ("bestand", "hf_sichern"),
    ("erkennung", "hf_erkennung"),
    ("startprogramme", "hf_startprogramme"),
    ("spiel", "hf_spiel"),
    ("uebersetzung", "hf_uebersetzung"),
    ("grafik", "hf_grafik"),
    ("blickwinkel", "hf_blickwinkel"),
    ("joysticks", "hf_joysticks"),
    ("achsen", "hf_achsen"),
    ("shader", "hf_shader"),
    ("patchaenderungen", "hf_patchaenderungen"),
    ("serverstatus", "hf_serverstatus"),
    ("danke", "hf_danke"),
];

/// Old page names that count as their new page (`my_stock_page.ALIASES`); the wishlist is the section
/// "Planned" of the hangar page, the dismantle calculation a view of the page "Dismantle & Salvage", the
/// evaluation of the statistics part of their overview, naming the ships part of the hangar page.
const PAGE_ALIASES: [(&str, &str); 6] = [
    ("lager", "meinlager"),
    ("handelslager", "meinlager"),
    ("wunschliste", "hangar"),
    ("zerlegen", "bergung"),
    ("statistik_auswertung", "statistik"),
    ("asop", "hangar"),
];

/// What an old page name chooses on the page it leads to (`my_stock_page.PRESETS`): the purpose and the view
/// of my storage, as 4.12 names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    /// `craft` or `sale`.
    pub purpose: &'static str,
    /// `material` or `items`.
    pub view: &'static str,
}

/// Old page names that bring a choice to their new page (`my_stock_page.PRESETS`): the cargo hold opens my storage
/// on the goods for sale.
const PAGE_PRESETS: [(&str, Preset); 1] = [("handelslager", Preset { purpose: "sale", view: "material" })];

/// The page an old or current page name of 4.12 leads to, with the choice an old name brings
/// (`my_stock_page.resolve`). A name that is no old one stays as it is.
pub fn resolve(page: &str) -> (&str, Option<Preset>) {
    let new = PAGE_ALIASES.iter().find(|(old, _)| *old == page).map_or(page, |(_, new)| new);
    (new, PAGE_PRESETS.iter().find(|(old, _)| *old == page).map(|(_, preset)| *preset))
}

/// The page key of v5 for a page name of 4.12.
pub fn page_key(page: &str) -> Option<&'static str> {
    PAGES.iter().find(|(name, _)| *name == page).map(|(_, key)| *key)
}

/// The page name of 4.12 for a page key of v5.
pub fn page_name(key: &str) -> Option<&'static str> {
    PAGES.iter().find(|(_, k)| *k == key).map(|(name, _)| *name)
}

/// The page a tile's "Öffnen" leads to, as a page key of v5.
pub fn tile_target(tile: &str) -> Option<&'static str> {
    let (_, page) = TILE_TARGETS.iter().find(|(id, _)| *id == tile)?;
    page_key(page)
}

/// The text key of a tile's title.
pub fn tile_title(tile: &str) -> Option<&'static str> {
    TILES.iter().find(|(id, _)| *id == tile).map(|(_, key)| *key)
}

/// The text key of a page's short explanation on its card (`cards.description_key`).
pub fn description_key(page: &str) -> String {
    format!("k_{page}")
}

fn list(settings: &Map, key: &str) -> Option<Vec<String>> {
    match settings.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
        Some(Value::Array(items)) => Some(items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()),
        _ => None,
    }
}

fn strings(items: &[String]) -> Value {
    Value::Array(items.iter().map(Value::str).collect())
}

/// The ids of the tiles shown (`overview.visible_tiles`): from the factory
/// all but the play time, while it is off in the head bar too.
pub fn visible_tiles(paths: &Paths) -> Vec<String> {
    let hidden = list(&paths.settings(), HIDDEN_SETTING).unwrap_or_else(|| {
        if paths.setting_bool(PLAYTIME_HEAD_SETTING, false) { Vec::new() } else { vec!["spielzeit".to_string()] }
    });
    TILES.iter().map(|(id, _)| id.to_string()).filter(|id| !hidden.contains(id)).collect()
}

/// The hidden tiles as the setting will hold them (`overview.set_visible`).
pub fn hidden_tiles(paths: &Paths) -> Vec<String> {
    let visible = visible_tiles(paths);
    TILES.iter().map(|(id, _)| id.to_string()).filter(|id| !visible.contains(id)).collect()
}

/// Show or hide a tile (`overview.set_visible`).
pub fn set_visible(paths: &Paths, tile: &str, on: bool) -> bool {
    let mut hidden = hidden_tiles(paths);
    if on {
        hidden.retain(|id| id != tile);
    } else if !hidden.iter().any(|id| id == tile) {
        hidden.push(tile.to_string());
    }
    paths.set_setting(HIDDEN_SETTING, strings(&hidden))
}

/// All tile ids in the order given (`overview.tile_order`): unknown ones fall
/// away, new ones come at the end.
pub fn tile_order(paths: &Paths) -> Vec<String> {
    let known: Vec<String> = TILES.iter().map(|(id, _)| id.to_string()).collect();
    let mut order: Vec<String> = list(&paths.settings(), ORDER_SETTING)
        .unwrap_or_default()
        .into_iter()
        .filter(|id| known.contains(id))
        .collect();
    let missing: Vec<String> = known.into_iter().filter(|id| !order.contains(id)).collect();
    order.extend(missing);
    order
}

/// Two tiles swap places (`overview.swap`); the whole order is kept.
pub fn swap(paths: &Paths, first: &str, second: &str) -> Vec<String> {
    let mut order = tile_order(paths);
    let (Some(i), Some(j)) = (order.iter().position(|id| id == first), order.iter().position(|id| id == second)) else {
        return order;
    };
    if i != j {
        order.swap(i, j);
        paths.set_setting(ORDER_SETTING, strings(&order));
    }
    order
}

/// The full order with the tiles of `shown` in their new sequence: the places
/// of the tiles not shown (not available here) stay where they were.
pub fn merged_order(stored: &[String], shown: &[String]) -> Vec<String> {
    let mut next = shown.iter();
    stored
        .iter()
        .map(|id| if shown.contains(id) { next.next().cloned().unwrap_or_else(|| id.clone()) } else { id.clone() })
        .collect()
}

/// Whether "your cards" stand above the tiles (`overview.sections_first`).
pub fn cards_first(paths: &Paths) -> bool {
    paths.setting(FIRST_SETTING).as_deref() == Some("karten")
}

/// The value of [`FIRST_SETTING`] for a choice.
pub fn first_value(cards_first: bool) -> Value {
    Value::str(if cards_first { "karten" } else { "kacheln" })
}

/// The pinned pages, known ones only and each once; old names count as their
/// new page (`cards.pinned`).
pub fn pinned(paths: &Paths) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for page in list(&paths.settings(), PINNED_SETTING).unwrap_or_default() {
        let (page, _) = resolve(&page);
        if page_key(page).is_some() && !result.iter().any(|p| p == page) {
            result.push(page.to_string());
        }
    }
    result
}

/// Pin or loosen a page (`cards.set_pinned`); the new list.
pub fn set_pinned(paths: &Paths, page: &str, on: bool) -> Vec<String> {
    let mut current = pinned(paths);
    if on && !current.iter().any(|p| p == page) {
        current.push(page.to_string());
    } else if !on {
        current.retain(|p| p != page);
    }
    paths.set_setting(PINNED_SETTING, strings(&current));
    current
}

/// Two pinned pages swap places (`cards.swap_pinned`).
pub fn swap_pinned(paths: &Paths, first: &str, second: &str) -> Vec<String> {
    let mut order = pinned(paths);
    if let (Some(i), Some(j)) = (order.iter().position(|p| p == first), order.iter().position(|p| p == second))
        && i != j
    {
        order.swap(i, j);
        paths.set_setting(PINNED_SETTING, strings(&order));
    }
    order
}

/// The value of a list setting.
pub fn list_value(items: &[String]) -> Value {
    strings(items)
}

/// The sizes kept for the tiles: (id, size name).
pub fn sizes(paths: &Paths) -> Vec<(String, String)> {
    match paths.settings().into_iter().find(|(k, _)| k == SIZE_SETTING).map(|(_, v)| v) {
        Some(Value::Object(map)) => {
            map.into_iter().filter_map(|(id, v)| v.as_str().map(|size| (id, size.to_string()))).collect()
        }
        _ => Vec::new(),
    }
}

/// The value of [`SIZE_SETTING`] for (id, size name) pairs.
pub fn sizes_value(sizes: &[(String, String)]) -> Value {
    Value::Object(sizes.iter().map(|(id, size)| (id.clone(), Value::str(size))).collect())
}

// ------------------------------------------------------------------ counting

/// A week in seconds.
pub const WEEK_SEC: i64 = 7 * 24 * 3600;

/// Sources that are no find in the game (`overview.NOT_FOUND_IN_GAME`).
pub const NOT_FOUND_IN_GAME: [&str; 4] = ["basetool", "start", "import", "launcher"];

fn field<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(Value::as_str)
}

/// The blueprints of the tile: owned of the catalog, of how many, how many
/// came this week, and the newest (`overview.blueprint_numbers`). Only what
/// stands in the catalog counts as owned; week and newest go over every entry
/// with a time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlueprintNumbers {
    pub mine: usize,
    pub total: usize,
    pub week: usize,
    pub newest: String,
    pub newest_at: Option<i64>,
}

/// `own` are the blueprints of the collection, `in_catalog` tells a key of the catalog.
pub fn blueprint_numbers(
    own: &[(String, Value)],
    in_catalog: &dyn Fn(&str) -> bool,
    total: usize,
    now: i64,
) -> BlueprintNumbers {
    let mut out = BlueprintNumbers { total, ..BlueprintNumbers::default() };
    out.mine = own.iter().filter(|(key, _)| in_catalog(key)).count();
    for (key, entry) in own {
        let Some(at) = field(entry, "zeit").and_then(crate::export::local_stamp) else { continue };
        if now - at <= WEEK_SEC {
            out.week += 1;
        }
        if out.newest_at.is_none_or(|newest| at > newest) {
            out.newest = field(entry, "name").filter(|n| !n.is_empty()).unwrap_or(key).to_string();
            out.newest_at = Some(at);
        }
    }
    out
}

/// A span of time the blueprint list narrows to by the time a blueprint was received.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Period {
    #[default]
    All,
    /// Since local midnight.
    Today,
    /// The last seven days, rolling: [`WEEK_SEC`], as the tile counts "this week".
    Days7,
    /// The last fourteen days, rolling.
    Days14,
    /// Since the first of the local month.
    Month,
}

impl Period {
    /// Every period in the order the filter offers them, the all choice first.
    pub const EVERY: [Period; 5] = [Period::All, Period::Today, Period::Days7, Period::Days14, Period::Month];

    /// The first second that belongs to the period at `now`; `None` for [`Period::All`].
    pub fn since(self, now: i64) -> Option<i64> {
        match self {
            Period::All => None,
            Period::Days7 => Some(now - WEEK_SEC),
            Period::Days14 => Some(now - 2 * WEEK_SEC),
            Period::Today => Some(local_start(now, false)),
            Period::Month => Some(local_start(now, true)),
        }
    }

    /// Does a blueprint received at `at` belong to the period at `now`? Like the tile's week, there is no
    /// upper bound: a time after `now` counts.
    pub fn contains(self, at: i64, now: i64) -> bool {
        self.since(now).is_none_or(|since| at >= since)
    }
}

/// Local midnight of the day of `now`, or of the first of its month.
fn local_start(now: i64, month: bool) -> i64 {
    let fallback = now - if month { 31 * 86_400 } else { 86_400 };
    let Some(time) = crate::formats::local_time(now) else { return fallback };
    let day = if month { 1 } else { time.day };
    crate::export::local_stamp(&format!("{:04}-{:02}-{day:02} 00:00:00", time.year, time.month)).unwrap_or(fallback)
}

/// A blueprint of the collection with a readable time of receipt.
#[derive(Clone, Debug, PartialEq)]
pub struct Received {
    pub key: String,
    /// The name as the collection holds it, else the key.
    pub name: String,
    pub at: i64,
    /// The `quelle` of the entry; empty when it has none.
    pub source: String,
}

/// Every entry of the collection with a readable `zeit`, whatever its source (as the tile counts), newest
/// first; entries of the same second by name.
pub fn received(own: &[(String, Value)]) -> Vec<Received> {
    let mut out: Vec<Received> = own
        .iter()
        .filter_map(|(key, entry)| {
            let at = field(entry, "zeit").and_then(crate::export::local_stamp)?;
            let name = field(entry, "name").filter(|n| !n.is_empty()).unwrap_or(key).to_string();
            Some(Received {
                key: key.clone(),
                name,
                at,
                source: field(entry, "quelle").unwrap_or_default().to_string(),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.at.cmp(&a.at).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then_with(|| a.key.cmp(&b.key))
    });
    out
}

/// How many of `received` belong to `period` at `now`.
pub fn count_in(received: &[Received], period: Period, now: i64) -> usize {
    received.iter().filter(|r| period.contains(r.at, now)).count()
}

/// Seconds played in the last seven days; `spans` are merged (from, to), the
/// part before is cut off (`overview.playtime_week`).
pub fn playtime_week(spans: &[(i64, i64)], now: i64) -> i64 {
    let start = now - WEEK_SEC;
    spans.iter().map(|(begin, end)| (end.min(&now) - begin.max(&start)).max(0)).sum()
}

/// The largest materials in storage: (name, amount, place with the most),
/// counted ones after the measured ones (`overview.storage_top`).
pub fn storage_top(rows: &[crate::stock::Row], most: usize) -> Vec<(String, f64, String, bool)> {
    /// One material: its name as first seen, the sum, counted in pieces, and the sum per place.
    struct Found {
        key: String,
        name: String,
        total: f64,
        piece: bool,
        places: Vec<(String, f64)>,
    }
    let mut found: Vec<Found> = Vec::new();
    for row in rows.iter().filter(|r| r.store == crate::stock::Store::Raw) {
        let name = row.name.trim();
        if name.is_empty() {
            continue;
        }
        let key = name.to_lowercase();
        let at = match found.iter().position(|f| f.key == key) {
            Some(at) => at,
            None => {
                found.push(Found { key, name: name.to_string(), total: 0.0, piece: row.piece, places: Vec::new() });
                found.len() - 1
            }
        };
        let entry = &mut found[at];
        entry.total += row.amount;
        let place = row.place.trim().to_string();
        match entry.places.iter_mut().find(|(p, _)| *p == place) {
            Some((_, amount)) => *amount += row.amount,
            None => entry.places.push((place, row.amount)),
        }
    }
    found.sort_by(|a, b| {
        a.piece.cmp(&b.piece).then(b.total.total_cmp(&a.total)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    found
        .into_iter()
        .take(most)
        .map(|f| {
            // The place with the most; the first of equal ones.
            let place = f
                .places
                .iter()
                .fold((String::new(), f64::MIN), |best, (p, a)| if *a > best.1 { (p.clone(), *a) } else { best });
            (f.name, f.total, place.0, f.piece)
        })
        .collect()
}

/// How much different there is in storage: (materials, goods, items), each
/// counted by name (`overview.storage_counts`).
pub fn storage_counts(rows: &[crate::stock::Row]) -> (usize, usize, usize) {
    use crate::stock::Store;
    let count = |keep: &dyn Fn(&crate::stock::Row) -> bool| {
        let mut names: Vec<String> =
            rows.iter().filter(|r| keep(r)).map(|r| r.name.trim().to_lowercase()).filter(|n| !n.is_empty()).collect();
        names.sort();
        names.dedup();
        names.len()
    };
    (
        count(&|r| r.store == Store::Raw),
        count(&|r| r.store == Store::Trade && !r.item),
        count(&|r| r.store == Store::Trade && r.item),
    )
}

/// Open contracts that give blueprints: (name, total, missing), the most
/// missing first; each title once (`overview.open_contracts`). `check` gives
/// (total, missing) of a title, or `None` for a contract without blueprints.
pub fn open_contracts(
    running: &[String],
    check: &dyn Fn(&str) -> Option<(usize, usize)>,
) -> Vec<(String, usize, usize)> {
    let mut seen: Vec<&str> = Vec::new();
    let mut result = Vec::new();
    for name in running {
        let name = name.trim();
        if name.is_empty() || seen.contains(&name) {
            continue;
        }
        seen.push(name);
        if let Some((total, missing)) = check(name) {
            result.push((name.to_string(), total, missing));
        }
    }
    result.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    result
}

/// The watch list: (found, total, open titles); a name counts as found once
/// it is owned, a pattern is always open (`overview.watch_numbers`).
pub fn watch_numbers(data: &Value, owned: &dyn Fn(&str) -> bool) -> (usize, usize, Vec<String>) {
    let names: Vec<String> = match data.get("namen") {
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let patterns: Vec<&Value> = match data.get("eintraege") {
        Some(Value::Array(list)) => list.iter().collect(),
        _ => Vec::new(),
    };
    let found = names.iter().filter(|n| owned(&crate::paths::name_key(n))).count();
    let mut sorted = names.clone();
    sorted.sort();
    let mut waiting: Vec<String> = sorted.into_iter().filter(|n| !owned(&crate::paths::name_key(n))).collect();
    waiting.extend(patterns.iter().map(|e| field(e, "titel").filter(|t| !t.is_empty()).unwrap_or("?").to_string()));
    (found, names.len() + patterns.len(), waiting)
}

/// The blueprints found in the game last: (name, time), newest first
/// (`overview.latest_finds`).
pub fn latest_finds(own: &[(String, Value)], most: usize) -> Vec<(String, i64)> {
    let mut finds: Vec<(String, i64)> = own
        .iter()
        .filter(|(_, entry)| !field(entry, "quelle").is_some_and(|q| NOT_FOUND_IN_GAME.contains(&q)))
        .filter_map(|(key, entry)| {
            let at = field(entry, "zeit").and_then(crate::export::local_stamp)?;
            Some((field(entry, "name").filter(|n| !n.is_empty()).unwrap_or(key).to_string(), at))
        })
        .collect();
    finds.sort_by_key(|f| std::cmp::Reverse(f.1));
    finds.truncate(most);
    finds
}

/// The blueprints that came with the catalog's version: (new, owned of them)
/// (`overview.patch_numbers`, `catalog.new_ones`); none without a version.
pub fn patch_numbers(catalog: &crate::catalog_cache::CatalogCache, owned: &dyn Fn(&str) -> bool) -> (usize, usize) {
    if catalog.version.is_empty() {
        return (0, 0);
    }
    let new: Vec<&str> = catalog
        .blueprints
        .iter()
        .filter(|b| b.since.as_deref() == Some(catalog.version.as_str()))
        .map(|b| b.key.as_str())
        .collect();
    (new.len(), new.iter().filter(|key| owned(key)).count())
}

/// How long a finished refinery job stays (`refinery_jobs.KEEP_DONE_SEC`).
pub const KEEP_DONE_SEC: f64 = 24.0 * 3600.0;

/// The end of a kept refinery job.
pub fn job_ends(job: &Map) -> f64 {
    match job.iter().find(|(k, _)| k == "ends").map(|(_, v)| v) {
        Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
        Some(Value::Float(x)) => *x,
        _ => 0.0,
    }
}

/// The refinery jobs as (running, done), each by its end; done ones older
/// than a day fall away (`refinery_jobs.current`).
pub fn refinery_current(jobs: &[Map], now: f64) -> (Vec<Map>, Vec<Map>) {
    let mut kept: Vec<Map> = jobs.iter().filter(|j| now - job_ends(j) <= KEEP_DONE_SEC).cloned().collect();
    kept.sort_by(|a, b| job_ends(a).total_cmp(&job_ends(b)));
    kept.into_iter().partition(|j| job_ends(j) > now)
}

/// The time left as 4.12 counts it down: `2d 3h`, `1h 5m`, `4m 10s`, `9s`
/// (`overview.countdown_text`).
pub fn countdown_text(seconds: f64) -> String {
    let left = seconds.max(0.0) as i64;
    let (days, hours, minutes, secs) = (left / 86_400, left % 86_400 / 3600, left % 3600 / 60, left % 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// The refinery jobs as lines of the overlay: the running ones by their end with the time left and the clock time
/// of the end, then the done ones; `true` marks a done one.
pub fn refinery_lines(jobs: &[Map], now: f64) -> Vec<(String, bool)> {
    let (running, done) = refinery_current(jobs, now);
    let label = |job: &Map| crate::refinery_jobs::job_label(job, 2);
    let mut lines: Vec<(String, bool)> = running
        .iter()
        .map(|job| {
            let ends = job_ends(job);
            let at = crate::formats::local_time(ends as i64)
                .map_or_else(String::new, |time| crate::formats::strftime("%H:%M", &time));
            let left = [("0", countdown_text(ends - now).into()), ("1", at.into())];
            (format!("{} · {}", label(job), crate::language::tf("s_uv_raff_noch", &left)), false)
        })
        .collect();
    lines.extend(done.iter().map(|job| (format!("{} · {}", label(job), crate::language::t("s_uv_raff_fertig")), true)));
    lines
}

#[cfg(test)]
mod tests;
