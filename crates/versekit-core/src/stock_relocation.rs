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

//! Moving the own stock to one home place: every entry of both stock files goes to the chosen place, equal
//! stacks there become one by the rules of booking, and the entries the caller strikes leave the stock.
//!
//! [`keep_before`] copies a stock file to `….vor-umzug` before the move, [`kept_before`] reads it back for
//! taking the move back. The [`Report`] (`lager-umzug.json`) keeps the struck entries for comparing with the
//! stock in the game. [`check`] counts the files again after writing: every entry at the home place, and
//! every amount what it was minus what was struck.
//!
//! Which items are consumables comes from the class of the item in the original texts of the game
//! (`item_Name<class>` of `namen-original.json`), see [`Consumables`].
//!
//! ⚠ An SCU entry of the materials is one crate and stays one: only pieces (gems) of equal quality and stolen
//! mark add up, like [`stock::raw_add`]. Goods and items add up like [`stock::trade_add`] and
//! [`stock::trade_add_item_with_cost`]: equal name, place and stolen mark, goods with goods, items with items.

use std::collections::HashMap;
use std::io;
use std::path::Path;

use crate::jsonio::{self, Value, map_set};
use crate::stock::{self, Row, Store};
use crate::stock_data::{Kind, StockData, format_is, lower, py_float_value, text_of, truthy};
use crate::text::py_strip;

/// The ending of the copy of a stock file made before a move.
pub const BACKUP_SUFFIX: &str = ".vor-umzug";
/// The report of the last move.
pub const REPORT_FILE: &str = "lager-umzug.json";
pub const REPORT_FORMAT: i64 = 1;
/// The file with the original names of the game, key to text.
pub const NAMES_FILE: &str = crate::asop::NAMES_FILE;
/// The key of the original texts in front of the class of an item.
const NAME_KEY: &str = "item_Name";

// ------------------------------------------------------------ categories

/// An item used up in play.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Consumable {
    Food,
    Drink,
    /// Pens and injectors (MedPen, OxyPen, …).
    Pen,
    /// Magazines, refills and batteries of personal weapons and tools.
    Ammo,
}

impl Consumable {
    pub const ALL: [Consumable; 4] = [Consumable::Food, Consumable::Drink, Consumable::Pen, Consumable::Ammo];

    pub fn text_key(self) -> &'static str {
        match self {
            Consumable::Food => "s_mlu_k_nahrung",
            Consumable::Drink => "s_mlu_k_getraenke",
            Consumable::Pen => "s_mlu_k_pens",
            Consumable::Ammo => "s_mlu_k_magazine",
        }
    }

    /// The consumable an item class of the game stands for: `Food…`, `Drink…` (also `DrinkWithPrice…`),
    /// `…_consumable_…` without keycards, `…_mag`.
    pub fn of_class(class: &str) -> Option<Consumable> {
        let class = class.trim().to_lowercase();
        if class.starts_with("food") {
            Some(Consumable::Food)
        } else if class.starts_with("drink") {
            Some(Consumable::Drink)
        } else if class.contains("_consumable_") && !class.contains("keycard") {
            Some(Consumable::Pen)
        } else if class.ends_with("_mag") {
            Some(Consumable::Ammo)
        } else {
            None
        }
    }
}

/// The consumables by item name (without case), from the original names of the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Consumables {
    by_name: HashMap<String, Consumable>,
}

impl Consumables {
    /// From `(key, text)` of the original names; only `item_Name<class>` keys count.
    pub fn from_names<'a>(names: impl IntoIterator<Item = (&'a str, &'a str)>) -> Consumables {
        let mut by_name = HashMap::new();
        for (key, text) in names {
            let Some(front) = key.get(..NAME_KEY.len()) else { continue };
            if !front.eq_ignore_ascii_case(NAME_KEY) {
                continue;
            }
            let name = lower(py_strip(text));
            if let Some(found) = Consumable::of_class(&key[NAME_KEY.len()..])
                && !name.is_empty()
            {
                by_name.entry(name).or_insert(found);
            }
        }
        Consumables { by_name }
    }

    /// From the stored original names; a missing or unreadable file gives none.
    pub fn load(file: &str) -> Consumables {
        match jsonio::read_json(file) {
            Ok(Value::Object(map)) => {
                Consumables::from_names(map.iter().filter_map(|(key, value)| Some((key.as_str(), value.as_str()?))))
            }
            _ => Consumables::default(),
        }
    }

    /// The consumable an item is; a name the original texts do not know counts as magazine when it says so.
    pub fn of(&self, name: &str) -> Option<Consumable> {
        let name = lower(py_strip(name));
        self.by_name.get(&name).copied().or_else(|| name.contains("magazine").then_some(Consumable::Ammo))
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// What the move list offers to strike: consumables, and every kind of material and item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Consumable(Consumable),
    /// A material for crafting or goods for sale (the tab Material).
    Material(Kind),
    /// An item that is no consumable (the tab Items).
    Item(Kind),
}

impl Category {
    /// Struck unless the player says otherwise: the consumables, which a wipe does not keep.
    pub fn struck_by_default(self) -> bool {
        matches!(self, Category::Consumable(_))
    }

    /// Order in the list: consumables, then materials, then items, each in the order of its kinds.
    pub fn order(self) -> (u8, usize) {
        let position = |kinds: &[Kind], kind: Kind| kinds.iter().position(|k| *k == kind).unwrap_or(kinds.len());
        match self {
            Category::Consumable(c) => (0, c as usize),
            Category::Material(k) => (1, position(&crate::stock_data::MATERIAL_KINDS, k)),
            Category::Item(k) => (2, position(&crate::stock_data::ITEM_KINDS, k)),
        }
    }

    /// The category of a row of the stock page.
    pub fn of_row(row: &Row, data: &StockData, consumables: &Consumables) -> Category {
        if row.item {
            return consumables
                .of(&row.name)
                .map_or_else(|| Category::Item(data.item_kind(&row.name)), Category::Consumable);
        }
        Category::Material(data.material_kind(&row.name, row.store == Store::Trade))
    }
}

// ------------------------------------------------------------ the move

/// Both stocks after a move.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Moved {
    pub raw: Vec<Value>,
    pub trade: Vec<Value>,
    /// The struck entries as they were, with the file they came from.
    pub struck: Vec<(Store, Value)>,
    /// Entries that lay at another place before.
    pub moved: usize,
    /// Entries that went into an equal stack at the home place.
    pub merged: usize,
}

impl Moved {
    /// Entries left in both stocks.
    pub fn kept(&self) -> usize {
        self.raw.len() + self.trade.len()
    }
}

fn quality_of(entry: &Value) -> Option<i64> {
    match entry.get("qualitaet") {
        None | Some(Value::Null) => None,
        Some(v) => py_float_value(v).map(stock::py_round_int),
    }
}

fn set(entry: &mut Value, key: &str, value: Value) {
    if let Value::Object(map) = entry {
        map_set(map, key, value);
    }
}

fn set_cost(entry: &mut Value, cost: Option<f64>) {
    match cost {
        Some(c) => set(entry, "einkauf", Value::Float(c)),
        None => {
            if let Value::Object(map) = entry {
                map.retain(|(k, _)| k != "einkauf");
            }
        }
    }
}

/// The entry of `kept` that `entry` adds onto at the home place.
fn stack_of(store: Store, kept: &[Value], entry: &Value, home: &str, data: &StockData) -> Option<usize> {
    let stolen = truthy(entry.get("gestohlen"));
    match store {
        Store::Raw => {
            let material = text_of(entry.get("material"));
            if !data.is_piece(material) {
                return None;
            }
            let quality = quality_of(entry);
            kept.iter().position(|other| {
                matches!(other, Value::Object(_))
                    && truthy(other.get("gestohlen")) == stolen
                    && stock::same_material_stack(material, quality, home, other)
            })
        }
        Store::Trade => {
            let goods = text_of(entry.get("ware"));
            let pieces = stock::is_pieces(entry);
            kept.iter().position(|other| {
                matches!(other, Value::Object(_))
                    && stock::is_pieces(other) == pieces
                    && stock::same_trade_stack(goods, home, stolen, other)
            })
        }
    }
}

/// Adds `entry` onto the equal stack `into`.
fn add_onto(store: Store, into: &mut Value, entry: &Value) {
    let (before, added) = (stock::number_of(into, "menge"), stock::number_of(entry, "menge"));
    match store {
        Store::Raw => set(into, "menge", Value::Float(stock::py_round(before + added, 6))),
        Store::Trade => {
            let mixed = stock::mixed_cost(before, stock::cost_of(into), added, stock::cost_of(entry));
            let pieces = stock::is_pieces(into);
            set_cost(into, if pieces { mixed.map(|c| stock::py_round(c, 4)) } else { mixed });
            set(into, "menge", Value::Float(before + added));
            if pieces
                && !matches!(into.get("uex"), Some(Value::Int(_)))
                && let Some(uex @ Value::Int(_)) = entry.get("uex")
            {
                set(into, "uex", uex.clone());
            }
        }
    }
}

/// Moves both stocks to `home`: what `strike` names (file, index) leaves, everything else goes to `home` and
/// equal stacks there add up. An entry that is no object stays as it is.
pub fn relocate(
    raw: &[Value],
    trade: &[Value],
    home: &str,
    data: &StockData,
    strike: impl Fn(Store, usize) -> bool,
) -> Moved {
    let home = py_strip(home).to_string();
    let mut out = Moved::default();
    for (store, entries) in [(Store::Raw, raw), (Store::Trade, trade)] {
        let mut kept: Vec<Value> = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            if !matches!(entry, Value::Object(_)) {
                kept.push(entry.clone());
                continue;
            }
            if strike(store, index) {
                out.struck.push((store, entry.clone()));
                continue;
            }
            if lower(py_strip(text_of(entry.get("ort")))) != lower(&home) {
                out.moved += 1;
            }
            let mut entry = entry.clone();
            set(&mut entry, "ort", Value::str(home.as_str()));
            match stack_of(store, &kept, &entry, &home, data) {
                Some(at) => {
                    add_onto(store, &mut kept[at], &entry);
                    out.merged += 1;
                }
                None => kept.push(entry),
            }
        }
        match store {
            Store::Raw => out.raw = kept,
            Store::Trade => out.trade = kept,
        }
    }
    out
}

// ------------------------------------------------------------ checking

/// What the files hold after a move, counted again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Check {
    /// Names of entries that do not lie at the home place.
    pub elsewhere: Vec<String>,
    /// Names whose amount is not the one before minus the struck.
    pub differs: Vec<String>,
}

impl Check {
    pub fn fine(&self) -> bool {
        self.elsewhere.is_empty() && self.differs.is_empty()
    }
}

type StackKey = (Store, String, bool, bool);

fn name_of(store: Store, entry: &Value) -> String {
    py_strip(text_of(entry.get(if store == Store::Raw { "material" } else { "ware" }))).to_string()
}

fn key_of(store: Store, entry: &Value) -> StackKey {
    (store, lower(&name_of(store, entry)), stock::is_pieces(entry), truthy(entry.get("gestohlen")))
}

fn totals<'a>(
    entries: impl Iterator<Item = (Store, &'a Value)>,
    into: &mut HashMap<StackKey, (String, f64)>,
    sign: f64,
) {
    for (store, entry) in entries.filter(|(_, e)| matches!(e, Value::Object(_))) {
        let slot = into.entry(key_of(store, entry)).or_insert_with(|| (name_of(store, entry), 0.0));
        slot.1 += sign * stock::number_of(entry, "menge");
    }
}

/// The entries of both stocks with the file they belong to.
fn tagged<'a>((raw, trade): (&'a [Value], &'a [Value])) -> Vec<(Store, &'a Value)> {
    raw.iter().map(|e| (Store::Raw, e)).chain(trade.iter().map(|e| (Store::Trade, e))).collect()
}

/// Counts the stocks after a move against the stocks before: every entry at `home`, and the amount of every
/// name, place aside, what it was minus what was struck.
pub fn check(
    before: (&[Value], &[Value]),
    struck: &[(Store, Value)],
    after: (&[Value], &[Value]),
    home: &str,
) -> Check {
    let (before, after) = (tagged(before), tagged(after));
    let mut out = Check::default();
    let home = lower(py_strip(home));
    for (store, entry) in after.iter().filter(|(_, e)| matches!(e, Value::Object(_))) {
        if lower(py_strip(text_of(entry.get("ort")))) != home {
            out.elsewhere.push(name_of(*store, entry));
        }
    }
    let mut sums = HashMap::new();
    totals(before.iter().copied(), &mut sums, 1.0);
    totals(struck.iter().map(|(s, e)| (*s, e)), &mut sums, -1.0);
    totals(after.iter().copied(), &mut sums, -1.0);
    let mut differs: Vec<String> =
        sums.into_values().filter(|(_, rest)| rest.abs() > 1e-6).map(|(name, _)| name).collect();
    differs.sort_by_key(|n| lower(n));
    out.differs = differs;
    out
}

// ------------------------------------------------------------ backup and report

/// The copy of a stock file made before a move.
pub fn backup_file(file: &str) -> String {
    format!("{file}{BACKUP_SUFFIX}")
}

/// Copies a stock file beside itself before a move; a file not there yet is kept as an empty stock.
pub fn keep_before(file: &str) -> io::Result<()> {
    let target = backup_file(file);
    if Path::new(file).exists() {
        jsonio::copy2(file, &target)
    } else {
        jsonio::write_text_atomic(&target, &stock::as_json(&[]), jsonio::Newline::Native)
    }
}

/// The stock kept before the last move; `None` when there is none or it is no stock.
pub fn kept_before(file: &str) -> Option<Vec<Value>> {
    let data = jsonio::read_json(&backup_file(file)).ok()?;
    if !matches!(data, Value::Object(_)) || !format_is(&data, stock::FORMAT) {
        return None;
    }
    match data.get("posten") {
        Some(Value::Array(entries)) => Some(entries.clone()),
        _ => None,
    }
}

/// The last move: when, where to, how many entries, and what was struck.
#[derive(Clone, Debug, PartialEq)]
pub struct Report {
    /// Seconds since 1970.
    pub time: i64,
    pub home: String,
    pub moved: usize,
    pub merged: usize,
    pub kept: usize,
    pub struck: Vec<(Store, Value)>,
}

fn store_code(store: Store) -> &'static str {
    match store {
        Store::Raw => "rohstoffe",
        Store::Trade => "handel",
    }
}

fn count_of(data: &Value, key: &str) -> Option<usize> {
    match data.get(key) {
        Some(Value::Int(n)) => n.parse().ok(),
        _ => None,
    }
}

impl Report {
    pub fn new(time: i64, home: &str, moved: &Moved) -> Report {
        Report {
            time,
            home: py_strip(home).to_string(),
            moved: moved.moved,
            merged: moved.merged,
            kept: moved.kept(),
            struck: moved.struck.clone(),
        }
    }

    pub fn to_value(&self) -> Value {
        let struck = self
            .struck
            .iter()
            .map(|(store, entry)| {
                Value::Object(vec![("teil".into(), Value::str(store_code(*store))), ("posten".into(), entry.clone())])
            })
            .collect();
        let count = |n: usize| Value::int(i64::try_from(n).unwrap_or(i64::MAX));
        Value::Object(vec![
            ("format".into(), Value::int(REPORT_FORMAT)),
            ("zeit".into(), Value::int(self.time)),
            ("heimatort".into(), Value::str(self.home.as_str())),
            ("umgebucht".into(), count(self.moved)),
            ("zusammengelegt".into(), count(self.merged)),
            ("behalten".into(), count(self.kept)),
            ("gestrichen".into(), Value::Array(struck)),
        ])
    }

    pub fn from_value(data: &Value) -> Option<Report> {
        if !format_is(data, REPORT_FORMAT) {
            return None;
        }
        let Some(Value::Array(rows)) = data.get("gestrichen") else { return None };
        let mut struck = Vec::new();
        for row in rows {
            let store = match row.get("teil").and_then(Value::as_str) {
                Some("rohstoffe") => Store::Raw,
                Some("handel") => Store::Trade,
                _ => return None,
            };
            let entry = row.get("posten").filter(|e| matches!(e, Value::Object(_)))?;
            struck.push((store, entry.clone()));
        }
        let time = match data.get("zeit") {
            Some(Value::Int(n)) => n.parse().ok()?,
            _ => return None,
        };
        Some(Report {
            time,
            home: data.get("heimatort").and_then(Value::as_str)?.to_string(),
            moved: count_of(data, "umgebucht")?,
            merged: count_of(data, "zusammengelegt")?,
            kept: count_of(data, "behalten")?,
            struck,
        })
    }

    pub fn save(&self, file: &str) -> io::Result<()> {
        jsonio::save_json(file, &self.to_value(), 1, false)
    }

    /// The report in `file`; `None` when there is none or it does not fit.
    pub fn load(file: &str) -> Option<Report> {
        Report::from_value(&jsonio::read_json(file).ok()?)
    }

    /// Takes the report away once the move is taken back; a report that is not there is fine.
    pub fn remove(file: &str) -> io::Result<()> {
        match jsonio::remove_file(file) {
            Err(e) if jsonio::is_not_found(&e) => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(text: &str) -> Vec<Value> {
        match jsonio::loads(text).expect("json") {
            Value::Array(list) => list,
            other => panic!("no list: {other:?}"),
        }
    }

    fn data_with_gems() -> StockData {
        StockData::sample()
    }

    #[test]
    fn classes_of_the_game_name_the_consumables() {
        assert_eq!(Consumable::of_class("Food_bar_busters_01_a"), Some(Consumable::Food));
        assert_eq!(Consumable::of_class("Drink_bottle_cruz_01_lux_a"), Some(Consumable::Drink));
        assert_eq!(Consumable::of_class("DrinkWithPrice_coffee"), Some(Consumable::Drink));
        assert_eq!(Consumable::of_class("crlf_consumable_healing_01"), Some(Consumable::Pen));
        assert_eq!(Consumable::of_class("rrs_consumable_sedative"), Some(Consumable::Pen));
        assert_eq!(Consumable::of_class("_fps_consumable_keycard_rockcrack_securestorage"), None, "keycards stay");
        assert_eq!(Consumable::of_class("behr_lmg_ballistic_01_mag"), Some(Consumable::Ammo));
        assert_eq!(Consumable::of_class("crlf_medgun_01_mag"), Some(Consumable::Ammo));
        assert_eq!(Consumable::of_class("flair_drink_screwtop_tumbler_1_a"), None, "a decoration stays");
        assert_eq!(Consumable::of_class("behr_lmg_ballistic_01"), None);
        assert_eq!(Consumable::of_class("MISL_S02_EM_TALN_Dominator"), None);
    }

    #[test]
    fn the_original_names_give_the_consumables_by_name() {
        let names = [
            ("item_NameFood_bar_busters_01_a", "Buster's Chocolate Bar"),
            ("item_Namecrlf_consumable_healing_01", " MedPen (Hemozal) "),
            ("item_Namebehr_lmg_ballistic_01_mag", "FS-9 Magazine (75 cap)"),
            ("item_Namebehr_lmg_ballistic_01", "FS-9 LMG"),
            ("Drink_bottle_cruz_01_lux_a", "CRUZ Lux"),
            ("item_name", "kurz"),
        ];
        let found = Consumables::from_names(names);
        assert_eq!(found.len(), 3);
        assert_eq!(found.of("buster's chocolate bar"), Some(Consumable::Food));
        assert_eq!(found.of("MedPen (Hemozal)"), Some(Consumable::Pen));
        assert_eq!(found.of("FS-9 Magazine (75 Cap)"), Some(Consumable::Ammo), "case as UEX writes it");
        assert_eq!(found.of("FS-9 LMG"), None);
        assert_eq!(found.of("CRUZ Lux"), None, "a key without item_Name in front does not count");
        assert_eq!(found.of("Neues Gewehr Magazine (9 Cap)"), Some(Consumable::Ammo), "unknown, but says so");
        assert_eq!(Consumables::default().of("Dominator II Missile"), None);
    }

    #[test]
    fn everything_goes_home_and_equal_stacks_add_up_by_the_rules_of_booking() {
        let data = data_with_gems();
        let raw = entries(
            r#"[{"material": "Iron", "menge": 4.5, "qualitaet": 420, "ort": "Orison"},
                {"material": "Iron", "menge": 1.5, "qualitaet": 420, "ort": "Levski"},
                {"material": "Hadanite", "menge": 14.0, "qualitaet": 500, "ort": "Area18"},
                {"material": "Hadanite", "menge": 2.0, "qualitaet": 500, "ort": "levski"},
                {"material": "Hadanite", "menge": 3.0, "qualitaet": 600, "ort": "Area18"},
                {"material": "Hadanite", "menge": 1.0, "qualitaet": 500, "ort": "Area18", "gestohlen": true}]"#,
        );
        let trade = entries(
            r#"[{"ware": "Gold", "menge": 10.0, "ort": "Area18", "gestohlen": false, "einkauf": 100.0},
                {"ware": "Gold", "menge": 10.0, "ort": "Levski", "gestohlen": false, "einkauf": 200.0},
                {"ware": "FR-66", "menge": 1.0, "ort": "Area18", "gestohlen": false, "stueck": true, "einkauf": 90.0},
                {"ware": "FR-66", "menge": 2.0, "ort": "Orison", "gestohlen": false, "stueck": true, "uex": 7, "einkauf": 120.0},
                {"ware": "FR-66", "menge": 1.0, "ort": "Orison", "gestohlen": true, "stueck": true},
                {"ware": "Gold", "menge": 1.0, "ort": "Orison", "gestohlen": false, "stueck": true}]"#,
        );
        let moved = relocate(&raw, &trade, " Levski ", &data, |_, _| false);
        let place = |e: &Value| e.get("ort").and_then(Value::as_str).map(str::to_string);
        assert!(moved.raw.iter().chain(&moved.trade).all(|e| place(e).as_deref() == Some("Levski")));
        // SCU crates stay apart; gems of equal quality and mark add up.
        assert_eq!(moved.raw.len(), 5, "{:?}", moved.raw);
        assert_eq!(stock::number_of(&moved.raw[2], "menge"), 16.0);
        assert_eq!(stock::number_of(&moved.raw[3], "menge"), 3.0);
        assert!(truthy(moved.raw[4].get("gestohlen")));
        // Goods add up with the mixed price; items with items, by stolen mark; the uex number comes along.
        assert_eq!(moved.trade.len(), 4, "{:?}", moved.trade);
        assert_eq!(stock::number_of(&moved.trade[0], "menge"), 20.0);
        assert_eq!(stock::cost_of(&moved.trade[0]), Some(150.0));
        assert_eq!(stock::number_of(&moved.trade[1], "menge"), 3.0);
        assert_eq!(stock::cost_of(&moved.trade[1]), Some(110.0));
        assert_eq!(moved.trade[1].get("uex"), Some(&Value::int(7)));
        assert!(truthy(moved.trade[2].get("gestohlen")));
        assert!(stock::is_pieces(&moved.trade[3]), "an item named like goods stays an item");
        assert_eq!((moved.moved, moved.merged), (9, 3), "levski in small letters counts as there already");
        assert!(moved.struck.is_empty());
    }

    #[test]
    fn struck_entries_leave_as_they_were_and_odd_entries_stay() {
        let data = StockData::default();
        let raw = entries(r#"[{"material": "Iron", "menge": 1.0, "ort": "Orison"}, "kein Posten"]"#);
        let trade = entries(
            r#"[{"ware": "MedPen (Hemozal)", "menge": 3.0, "ort": "Levski", "stueck": true},
                {"ware": "FS-9 LMG", "menge": 2.0, "ort": "Orbituary", "stueck": true}]"#,
        );
        let moved = relocate(&raw, &trade, "New Babbage", &data, |store, index| store == Store::Trade && index == 0);
        assert_eq!(moved.struck, vec![(Store::Trade, trade[0].clone())]);
        assert_eq!(moved.raw[1], Value::str("kein Posten"));
        assert_eq!(moved.trade.len(), 1);
        assert_eq!(moved.kept(), 3);
        // Amounts as before minus the struck, all at home.
        assert!(check((&raw, &trade), &moved.struck, (&moved.raw, &moved.trade), "new babbage").fine());
    }

    #[test]
    fn the_check_finds_an_entry_elsewhere_and_an_amount_that_changed() {
        let data = data_with_gems();
        let raw = entries(r#"[{"material": "Hadanite", "menge": 2.0, "qualitaet": 500, "ort": "Area18"}]"#);
        let trade = entries(r#"[{"ware": "Gold", "menge": 10.0, "ort": "Orison", "gestohlen": false}]"#);
        let moved = relocate(&raw, &trade, "Levski", &data, |_, _| false);
        assert!(check((&raw, &trade), &moved.struck, (&moved.raw, &moved.trade), "Levski").fine());
        let mut wrong_place = moved.trade.clone();
        set(&mut wrong_place[0], "ort", Value::str("Orison"));
        let found = check((&raw, &trade), &moved.struck, (&moved.raw, &wrong_place), "Levski");
        assert_eq!(found.elsewhere, vec!["Gold".to_string()]);
        let mut less = moved.raw.clone();
        set(&mut less[0], "menge", Value::Float(1.0));
        let found = check((&raw, &trade), &moved.struck, (&less, &moved.trade), "Levski");
        assert_eq!(found.differs, vec!["Hadanite".to_string()]);
        // Struck but still there counts as a difference too.
        let struck = vec![(Store::Trade, trade[0].clone())];
        assert_eq!(
            check((&raw, &trade), &struck, (&moved.raw, &moved.trade), "Levski").differs,
            vec!["Gold".to_string()]
        );
    }

    #[test]
    fn the_report_comes_back_as_it_went_out() {
        let trade = entries(r#"[{"ware": "MedPen (Hemozal)", "menge": 3.0, "ort": "Levski", "stueck": true}]"#);
        let raw = entries(r#"[{"material": "Iron", "menge": 1.0, "ort": "Orison"}]"#);
        let moved = relocate(&raw, &trade, "Levski", &StockData::default(), |store, _| store == Store::Trade);
        let report = Report::new(1_793_000_000, " Levski ", &moved);
        assert_eq!((report.moved, report.merged, report.kept, report.home.as_str()), (1, 0, 1, "Levski"));
        let back = Report::from_value(&jsonio::loads(&jsonio::dumps(&report.to_value(), Some(1), false)).unwrap());
        assert_eq!(back, Some(report.clone()));
        let mut other = report.to_value();
        set(&mut other, "format", Value::int(2));
        assert_eq!(Report::from_value(&other), None, "another format");
        let mut broken = report.to_value();
        set(&mut broken, "gestrichen", Value::Array(vec![Value::Object(vec![("teil".into(), Value::str("x"))])]));
        assert_eq!(Report::from_value(&broken), None);
    }

    #[test]
    fn the_copy_before_and_the_report_lie_beside_the_stock() {
        let dir = std::env::temp_dir().join(format!("versekit-umzug-kern-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(stock::RAW_FILE).to_string_lossy().replace('\\', "/");
        // No file yet: an empty stock is kept.
        keep_before(&file).unwrap();
        assert_eq!(kept_before(&file), Some(Vec::new()));
        let row = Value::Object(vec![("material".into(), Value::str("Iron")), ("menge".into(), Value::Float(3.0))]);
        assert!(stock::save(&file, std::slice::from_ref(&row), "test"));
        keep_before(&file).unwrap();
        assert_eq!(kept_before(&file), Some(vec![row]));
        assert!(std::path::Path::new(&format!("{file}.vor-umzug")).exists());
        std::fs::write(backup_file(&file), "{\"format\": 2, \"posten\": []}").unwrap();
        assert_eq!(kept_before(&file), None, "no stock of this format");
        let report_file = dir.join(REPORT_FILE).to_string_lossy().replace('\\', "/");
        assert_eq!(Report::load(&report_file), None);
        let report = Report { time: 5, home: "Levski".into(), moved: 1, merged: 0, kept: 1, struck: Vec::new() };
        report.save(&report_file).unwrap();
        assert_eq!(Report::load(&report_file), Some(report));
        Report::remove(&report_file).unwrap();
        Report::remove(&report_file).unwrap();
        assert_eq!(Report::load(&report_file), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn categories_come_in_the_order_of_the_list_and_only_consumables_are_struck_at_first() {
        let mut all = vec![
            Category::Item(Kind::Weapon),
            Category::Material(Kind::Mineral),
            Category::Consumable(Consumable::Ammo),
            Category::Material(Kind::Ore),
            Category::Consumable(Consumable::Food),
        ];
        all.sort_by_key(|c| c.order());
        assert_eq!(
            all,
            vec![
                Category::Consumable(Consumable::Food),
                Category::Consumable(Consumable::Ammo),
                Category::Material(Kind::Ore),
                Category::Material(Kind::Mineral),
                Category::Item(Kind::Weapon),
            ]
        );
        assert!(all.iter().filter(|c| c.struck_by_default()).count() == 2);
        let data = StockData::sample();
        let row = |name: &str, item: bool| Row {
            store: if item { Store::Trade } else { Store::Raw },
            index: 0,
            name: name.into(),
            amount: 1.0,
            piece: item,
            quality: None,
            place: String::new(),
            stolen: false,
            cost: None,
            item,
            leftover: false,
        };
        let found = Consumables::from_names([("item_Namecrlf_consumable_healing_01", "MedPen (Hemozal)")]);
        assert_eq!(
            Category::of_row(&row("MedPen (Hemozal)", true), &data, &found),
            Category::Consumable(Consumable::Pen)
        );
        assert_eq!(Category::of_row(&row("Hadanite", false), &data, &found), Category::Material(Kind::Mineral));
        assert_eq!(Category::of_row(&row("Unbekannt", true), &data, &found), Category::Item(Kind::Other));
    }
}
