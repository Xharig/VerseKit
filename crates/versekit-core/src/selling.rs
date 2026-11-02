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

//! Prices per terminal (`selling.py` of 4.12): where a load is sold and where
//! a good is bought cheapest.
//!
//! The list `verkauf.json` comes from the UEX endpoint `commodities_prices_all`
//! and the terminal list; it holds the terminals, the offers a terminal pays
//! for a good (`waren`) and the offers a terminal sells it for (`kaeufe`).
//! At most one fetch per day; the button of the page is locked for an hour
//! after a success and for a minute after a failure.
//!
//! * Good names are compared exactly, never by part: `Copper` and
//!   `Copper (Ore)` are two goods with two prices.
//! * Only rows with a price above zero are kept.
//! * The fill levels are the seven steps of UEX; selling, "full" is bad,
//!   buying, "empty" is.

use std::sync::Mutex;

use crate::jsonio::Value;
use crate::ordered::OrderedMap;
use crate::paths::Paths;
use crate::stock_data::truthy;
use crate::stock_fetch::{self, Fetch, switched_off};
use crate::uex::{self, DAY, SaveError, Store};

/// The endpoint of all prices.
pub const SOURCE: &str = "https://api.uexcorp.uk/2.0/commodities_prices_all";
/// The endpoint of the terminals (system, place, name, kind, no questions asked).
pub const SOURCE_TERMINALS: &str = "https://api.uexcorp.uk/2.0/terminals";
/// The file in the data folder.
pub const CACHE: &str = "verkauf.json";
/// The format number: 5 holds the offers to buy (`kaeufe`).
pub const FORMAT: i64 = 5;

/// The kinds of terminal that trade in goods; every other kind is no use for a route.
pub const TRADE_TYPES: [&str; 2] = ["commodity", "commodity_raw"];

/// From this level a store fills up (a hint).
pub const FILLING_UP: i64 = 5;
/// From this level a store wants nothing more (a warning).
pub const NO_DEMAND: i64 = 6;

/// A day: the stored list counts as fresh for this long.
pub const SHELF_LIFE: f64 = DAY;
/// A report older than this counts as old, in seconds.
pub const OLD: f64 = 7.0 * DAY;
/// After a failure the button stays locked this long, in seconds.
pub const ERROR_LOCK: f64 = 60.0;
/// After a success the button stays locked this long, in seconds.
pub const LOCK: f64 = 60.0 * 60.0;

/// Goods that are gifts of an event and no goods to trade.
pub const NOT_IN_TOP_LIST: [&str; 2] = ["luminalia gift", "year of the rat envelope"];

/// From this multiple of the second highest offer on, a price is an outlier.
pub const OUTLIER_FACTOR: f64 = 3.0;

/// Fill level of an empty store when buying.
pub const EMPTY: i64 = 1;
/// Fill level of a nearly empty store when buying.
pub const NEARLY_EMPTY: i64 = 2;
/// Fill level of a low store when buying.
pub const LOW: i64 = 3;

const STORE: Store = Store::new(CACHE, FORMAT, SHELF_LIFE);

/// When a fetch failed last (seconds since 1970); only in memory.
static LAST_FAILURE: Mutex<f64> = Mutex::new(0.0);

fn last_failure() -> f64 {
    *LAST_FAILURE.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_last_failure(now: f64) {
    *LAST_FAILURE.lock().unwrap_or_else(|e| e.into_inner()) = now;
}

/// Forgets the last failure (for checks).
pub fn forget_failure() {
    set_last_failure(0.0);
}

// ------------------------------------------------------------ the data

/// A terminal (`terminals` of the file).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Terminal {
    /// The place: station, city, outpost or planet (`o`).
    pub place: String,
    /// The star system (`s`).
    pub system: String,
    /// Asks no questions about stolen goods (`q`).
    pub nqa: bool,
    /// The name of the terminal (`n`).
    pub name: String,
    /// The kind (`t`); `None` in a file that does not know it.
    pub kind: Option<String>,
}

/// One offer of a terminal for a good: what it pays (`waren`) or asks (`kaeufe`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Offer {
    /// The number of the terminal (`t`).
    pub terminal: String,
    /// The name of the terminal in the price row (`n`).
    pub name: String,
    /// The price per SCU (`p`).
    pub price: f64,
    /// When the price was reported, seconds since 1970; `0` without date (`d`).
    pub date: i64,
    /// The sizes of the boxes as UEX writes them (`k`).
    pub boxes: String,
    /// The fill level of the seven steps; `0` unknown (`z`).
    pub level: i64,
    /// The stock in SCU for an offer to buy (`m`).
    pub stock: i64,
}

/// The stored list.
#[derive(Clone, Debug, Default)]
pub struct Selling {
    pub terminals: OrderedMap<Terminal>,
    /// What a terminal pays: good → offers, the highest first.
    pub goods: OrderedMap<Vec<Offer>>,
    /// What a terminal asks: good → offers, the cheapest first.
    pub buys: OrderedMap<Vec<Offer>>,
    /// When it was fetched (`geholt`).
    pub fetched: Option<f64>,
    /// The game build it was fetched under (`spielstand`).
    pub build: String,
}

fn py_text(value: &Value) -> String {
    match value {
        Value::Null => "None".to_string(),
        Value::Bool(true) => "True".to_string(),
        Value::Bool(false) => "False".to_string(),
        Value::Int(n) => n.clone(),
        Value::Float(x) => crate::jsonio::float_text(*x),
        Value::Str(s) => s.clone(),
        other => crate::jsonio::dumps(other, None, false),
    }
}

fn text_or_empty(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn integer(value: Option<&Value>) -> i64 {
    value.and_then(stock_fetch::py_int).unwrap_or(0)
}

fn offer_of(value: &Value) -> Offer {
    Offer {
        terminal: value.get("t").map(py_text).unwrap_or_default(),
        name: text_or_empty(value.get("n")),
        price: value.get("p").and_then(crate::stock_data::py_float_value).unwrap_or(0.0),
        date: integer(value.get("d")),
        boxes: text_or_empty(value.get("k")),
        level: integer(value.get("z")),
        stock: integer(value.get("m")),
    }
}

fn offer_list(value: Option<&Value>) -> OrderedMap<Vec<Offer>> {
    let mut map = OrderedMap::new();
    if let Some(Value::Object(entries)) = value {
        for (name, rows) in entries {
            let offers = match rows {
                Value::Array(rows) => rows.iter().map(offer_of).collect(),
                _ => Vec::new(),
            };
            map.set(name, offers);
        }
    }
    map
}

impl Selling {
    /// Reads the stored file; without a usable one the list is empty.
    pub fn load(paths: &Paths) -> Selling {
        STORE.load(paths).map_or_else(Selling::default, |data| Selling::from_value(&data))
    }

    /// The list of a file already read.
    pub fn from_value(data: &Value) -> Selling {
        let mut terminals = OrderedMap::new();
        if let Some(Value::Object(entries)) = data.get("terminals") {
            for (ident, spot) in entries {
                terminals.set(
                    ident,
                    Terminal {
                        place: text_or_empty(spot.get("o")),
                        system: text_or_empty(spot.get("s")),
                        nqa: truthy(spot.get("q")),
                        name: text_or_empty(spot.get("n")),
                        kind: spot.get("t").map(py_text),
                    },
                );
            }
        }
        Selling {
            terminals,
            goods: offer_list(data.get("waren")),
            buys: offer_list(data.get("kaeufe")),
            fetched: data.get("geholt").filter(|v| truthy(Some(v))).and_then(crate::stock_data::py_float_value),
            build: crate::stock_data::text_of(data.get("spielstand")).to_string(),
        }
    }

    /// How old the list is in seconds, or `None` without a list.
    pub fn age(&self, now: f64) -> Option<f64> {
        self.fetched.map(|then| now - then)
    }

    /// Is any good on the list?
    pub fn is_empty(&self) -> bool {
        self.goods.is_empty() && self.buys.is_empty()
    }
}

// ------------------------------------------------------------ fetching

/// What an update did (`update` of 4.12: the reason code as a word).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    /// In order, or nothing to do.
    Done,
    /// The button is still locked (`gesperrt`).
    Locked,
    /// The fetch failed (`netz`).
    Network,
    /// The answer held no prices (`leer`).
    Empty,
    /// Fetching is switched off (`aus`).
    Off,
    /// The answer was fine, but the file could not be written.
    NotSaved,
}

impl Update {
    /// The code 4.12 returns as the reason.
    pub fn code(self) -> &'static str {
        match self {
            Update::Done => "",
            Update::Locked => "gesperrt",
            Update::Network => "netz",
            Update::Empty => "leer",
            Update::Off => "aus",
            Update::NotSaved => "nicht_gespeichert",
        }
    }
}

/// How old the stored list is in seconds, or `None`.
pub fn age(paths: &Paths, now: f64) -> Option<f64> {
    STORE.age(paths, now)
}

/// How many seconds the button "Update now" is still locked; `0` allows it.
pub fn wait_time(paths: &Paths, now: f64) -> i64 {
    let since_error = now - last_failure();
    let rest_error = ((ERROR_LOCK - since_error) as i64).max(0);
    match age(paths, now) {
        None => rest_error,
        Some(a) => rest_error.max(if a < LOCK { (LOCK - a) as i64 } else { 0 }),
    }
}

/// The offer to buy of a UEX price row, or `None` where the terminal does not
/// sell the good.
fn buy_row(row: &Value) -> Option<Option<Offer>> {
    let price = uex::number(row, "price_buy")?;
    if price <= 0.0 {
        return Some(None);
    }
    Some(Some(Offer {
        terminal: py_text(row.get("id_terminal").unwrap_or(&Value::Null)),
        name: uex::stripped(row, "terminal_name"),
        price,
        date: uex::whole(row, "date_modified")?,
        boxes: text_or_empty(row.get("container_sizes")),
        level: uex::whole(row, "status_buy")?,
        stock: uex::number(row, "scu_buy")? as i64,
    }))
}

fn offer_value(offer: &Offer, with_stock: bool) -> Value {
    let mut fields = vec![
        uex::field("t", Value::Str(offer.terminal.clone())),
        uex::field("n", Value::Str(offer.name.clone())),
        uex::field("p", uex::float(offer.price)),
        uex::field("d", Value::int(offer.date)),
        uex::field("k", Value::Str(offer.boxes.clone())),
        uex::field("z", Value::int(offer.level)),
    ];
    if with_stock {
        fields.push(uex::field("m", Value::int(offer.stock)));
    }
    Value::Object(fields)
}

fn offers_value(map: &OrderedMap<Vec<Offer>>, with_stock: bool) -> Value {
    Value::Object(
        map.iter()
            .map(|(name, offers)| {
                (name.to_string(), Value::Array(offers.iter().map(|offer| offer_value(offer, with_stock)).collect()))
            })
            .collect(),
    )
}

fn terminals_value(spots: &OrderedMap<Terminal>) -> Value {
    Value::Object(
        spots
            .iter()
            .map(|(ident, spot)| {
                let mut fields = vec![
                    uex::field("o", Value::Str(spot.place.clone())),
                    uex::field("s", Value::Str(spot.system.clone())),
                    uex::field("q", Value::int(i64::from(spot.nqa))),
                    uex::field("n", Value::Str(spot.name.clone())),
                ];
                if let Some(kind) = &spot.kind {
                    fields.push(uex::field("t", Value::Str(kind.clone())));
                }
                (ident.to_string(), Value::Object(fields))
            })
            .collect(),
    )
}

/// Fetches the prices (`update` of 4.12). Without `force` only when the
/// stored list is missing or older than a day; with `force` (the button) at
/// most once an hour.
pub fn update(paths: &Paths, now: f64, force: bool, fetch: Fetch<'_>) -> Update {
    if switched_off(&paths.host) {
        return Update::Off;
    }
    if force {
        if wait_time(paths, now) != 0 {
            return Update::Locked;
        }
    } else if !STORE.stale(paths, now) {
        return Update::Done;
    }
    let Some(data) = uex::fetch("selling", SOURCE, fetch) else {
        set_last_failure(now);
        return Update::Network;
    };
    let price_rows = uex::rows(&data);
    if price_rows.is_empty() {
        set_last_failure(now);
        return Update::Empty;
    }
    // The terminal list may fail: it only adds system, kind and the mark.
    let spots_data = uex::fetch("selling.terminals", SOURCE_TERMINALS, fetch);
    let mut terminals = OrderedMap::new();
    for spot in spots_data.as_ref().map_or(&[][..], uex::rows) {
        let Some(ident) = spot.get("id").filter(|v| !matches!(v, Value::Null)) else { continue };
        terminals.set(
            &py_text(ident),
            Terminal {
                place: first_raw(spot, &["space_station_name", "city_name", "outpost_name", "planet_name"]),
                system: first_raw(spot, &["star_system_name"]),
                nqa: truthy(spot.get("is_nqa")),
                name: uex::stripped(spot, "name"),
                kind: Some(uex::stripped(spot, "type")),
            },
        );
    }
    let mut goods: OrderedMap<Vec<Offer>> = OrderedMap::new();
    let mut buys: OrderedMap<Vec<Offer>> = OrderedMap::new();
    for row in price_rows {
        let name = uex::stripped(row, "commodity_name");
        if name.is_empty() {
            continue;
        }
        let Some(bought) = buy_row(row) else { return failed_row(now) };
        if let Some(offer) = bought {
            buys.entry_or_default(&name).push(offer);
        }
        let (Some(price), Some(date), Some(level)) =
            (uex::number(row, "price_sell"), uex::whole(row, "date_modified"), uex::whole(row, "status_sell"))
        else {
            return failed_row(now);
        };
        if price <= 0.0 {
            continue;
        }
        goods.entry_or_default(&name).push(Offer {
            terminal: py_text(row.get("id_terminal").unwrap_or(&Value::Null)),
            name: uex::stripped(row, "terminal_name"),
            price,
            date,
            boxes: text_or_empty(row.get("container_sizes")),
            level,
            stock: 0,
        });
    }
    if goods.is_empty() && buys.is_empty() {
        return Update::Empty;
    }
    let goods = sorted_offers(goods, true);
    let buys = sorted_offers(buys, false);
    let fields = vec![
        uex::field("terminals", terminals_value(&terminals)),
        uex::field("waren", offers_value(&goods, false)),
        uex::field("kaeufe", offers_value(&buys, true)),
    ];
    match STORE.save(paths, fields, true, now) {
        Ok(()) => Update::Done,
        Err(SaveError::Refused | SaveError::Failed(_)) => Update::NotSaved,
    }
}

fn failed_row(now: f64) -> Update {
    set_last_failure(now);
    Update::Network
}

/// `(x.get(a) or x.get(b) or … or '')` without stripping.
fn first_raw(row: &Value, keys: &[&str]) -> String {
    for key in keys {
        if let Some(Value::Str(text)) = row.get(key).filter(|v| truthy(Some(v))) {
            return text.clone();
        }
    }
    String::new()
}

/// The offers of every good ordered by price: the highest first when
/// `descending`, else the lowest first. Equal prices keep their order.
fn sorted_offers(map: OrderedMap<Vec<Offer>>, descending: bool) -> OrderedMap<Vec<Offer>> {
    let mut out = OrderedMap::new();
    for (name, mut offers) in map.iter().map(|(n, o)| (n.to_string(), o.clone())) {
        if descending {
            offers.sort_by(|a, b| b.price.total_cmp(&a.price));
        } else {
            offers.sort_by(|a, b| a.price.total_cmp(&b.price));
        }
        out.set(&name, offers);
    }
    out
}

// ------------------------------------------------------------ selling

/// What a fill level of a store that pays means: the text key and whether it is
/// a warning (red) or a hint (gold). `None`: say nothing.
pub fn fill_level(offer: Option<&Offer>) -> Option<(&'static str, bool)> {
    let level = offer.map_or(0, |offer| offer.level);
    if level >= NO_DEMAND {
        Some(("s_vk_voll", true))
    } else if level == FILLING_UP {
        Some(("s_vk_fuellt", false))
    } else {
        None
    }
}

impl Selling {
    /// All goods with at least one offer to buy, in the order of the alphabet.
    pub fn goods(&self) -> Vec<String> {
        let mut names: Vec<String> = self.goods.iter().map(|(name, _)| name.to_string()).collect();
        names.sort();
        names
    }

    /// Does the list know this good (exact comparison)?
    pub fn known(&self, name: &str) -> bool {
        self.goods.get(name).is_some()
    }

    /// All goods that at least one terminal sells, in the order of the alphabet.
    pub fn buy_goods(&self) -> Vec<String> {
        let mut names: Vec<String> = self.buys.iter().map(|(name, _)| name.to_string()).collect();
        names.sort();
        names
    }

    /// Does any terminal sell this good (exact comparison)?
    pub fn buy_known(&self, name: &str) -> bool {
        self.buys.get(name).is_some()
    }
}

/// One good a place takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub good: String,
    pub price: f64,
    pub boxes: String,
    /// Age in seconds; `None` without a date.
    pub age: Option<f64>,
    pub fill: Option<(&'static str, bool)>,
}

/// A place that takes some of the goods asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct Place {
    pub terminal: String,
    pub place: String,
    pub system: String,
    pub nqa: bool,
    pub hits: Vec<Hit>,
    pub count: usize,
    /// The sum of the prices per SCU: a number for ordering, no proceeds.
    pub sum: f64,
    /// The age of the oldest report behind the place.
    pub age: Option<f64>,
}

impl Selling {
    /// Where the goods are sold: the place that takes the most goods first,
    /// then by the sum of the prices. With `nqa_only` only terminals that ask
    /// no questions.
    pub fn places_for(&self, names: &[String], nqa_only: bool, now: f64) -> Vec<Place> {
        let wanted: Vec<&String> = names.iter().filter(|name| self.goods.get(name).is_some()).collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        let mut collected: OrderedMap<Place> = OrderedMap::new();
        for item in wanted {
            for offer in self.goods.get(item).map_or(&[][..], |offers| &offers[..]) {
                let spot = self.terminals.get(&offer.terminal);
                if nqa_only && !spot.is_some_and(|spot| spot.nqa) {
                    continue;
                }
                let entry = collected.entry_or_default(&offer.terminal);
                if entry.hits.is_empty() && entry.terminal.is_empty() {
                    entry.terminal = if offer.name.is_empty() { "?".to_string() } else { offer.name.clone() };
                    entry.place = spot.map(|s| s.place.clone()).unwrap_or_default();
                    entry.system = spot.map(|s| s.system.clone()).unwrap_or_default();
                    entry.nqa = spot.is_some_and(|s| s.nqa);
                }
                entry.hits.push(Hit {
                    good: item.clone(),
                    price: offer.price,
                    boxes: offer.boxes.clone(),
                    age: (offer.date != 0).then_some(now - offer.date as f64),
                    fill: fill_level(Some(offer)),
                });
            }
        }
        let mut result: Vec<Place> = collected.values().cloned().collect();
        for entry in &mut result {
            entry.hits.sort_by(|a, b| b.price.total_cmp(&a.price));
            entry.count = entry.hits.len();
            entry.sum = crate::text::py_sum(entry.hits.iter().map(|hit| hit.price));
            let ages: Vec<f64> = entry.hits.iter().filter_map(|hit| hit.age).collect();
            entry.age = ages.into_iter().reduce(f64::max);
        }
        result.sort_by(|a, b| b.count.cmp(&a.count).then(b.sum.total_cmp(&a.sum)));
        result
    }
}

impl Default for Place {
    fn default() -> Self {
        Place {
            terminal: String::new(),
            place: String::new(),
            system: String::new(),
            nqa: false,
            hits: Vec::new(),
            count: 0,
            sum: 0.0,
            age: None,
        }
    }
}

/// Whether a good belongs to "what pays best at the moment".
pub fn in_top_list(name: &str) -> bool {
    let name = crate::text::py_strip(name).to_lowercase();
    !NOT_IN_TOP_LIST.contains(&name.as_str())
}

/// The offers without the one value that falls out of line: from three offers
/// on, the highest is dropped when it is at least [`OUTLIER_FACTOR`] times the second.
fn without_outliers(rows: &[Offer]) -> Vec<&Offer> {
    let mut values: Vec<f64> = rows.iter().map(|z| z.price).collect();
    values.sort_by(f64::total_cmp);
    if values.len() < 3 || values[values.len() - 2] <= 0.0 {
        return rows.iter().collect();
    }
    let (highest, second) = (values[values.len() - 1], values[values.len() - 2]);
    if highest / second < OUTLIER_FACTOR {
        return rows.iter().collect();
    }
    rows.iter().filter(|z| z.price < highest).collect()
}

fn without_low_outliers(rows: &[&Offer]) -> Vec<Offer> {
    let mut values: Vec<f64> = rows.iter().map(|z| z.price).collect();
    values.sort_by(f64::total_cmp);
    if values.len() < 3 || values[0] <= 0.0 {
        return rows.iter().map(|z| (*z).clone()).collect();
    }
    if values[1] / values[0] < OUTLIER_FACTOR {
        return rows.iter().map(|z| (*z).clone()).collect();
    }
    let lowest = values[0];
    rows.iter().filter(|z| z.price > lowest).map(|z| (*z).clone()).collect()
}

impl Selling {
    /// What a good brings at most per SCU, or `0.0`. A single absurd offer is
    /// dropped unless `with_outliers`.
    pub fn best_price(&self, name: &str, with_outliers: bool) -> f64 {
        let rows = self.goods.get(name).map_or(&[][..], |rows| &rows[..]);
        let kept: Vec<&Offer> = if with_outliers { rows.iter().collect() } else { without_outliers(rows) };
        kept.iter().map(|z| z.price).reduce(f64::max).unwrap_or(0.0)
    }

    /// What a good costs at least per SCU, or `0.0`. An empty terminal does not
    /// count while another has stock; an absurdly low offer is dropped unless
    /// `with_outliers`.
    pub fn cheapest_price(&self, name: &str, with_outliers: bool) -> f64 {
        let rows = self.buys.get(name).map_or(&[][..], |rows| &rows[..]);
        let stocked: Vec<&Offer> = rows.iter().filter(|z| z.level != EMPTY).collect();
        let chosen: Vec<&Offer> = if stocked.is_empty() { rows.iter().collect() } else { stocked };
        let kept: Vec<Offer> =
            if with_outliers { chosen.iter().map(|z| (*z).clone()).collect() } else { without_low_outliers(&chosen) };
        kept.iter().map(|z| z.price).reduce(f64::min).unwrap_or(0.0)
    }
}

// ------------------------------------------------------------ buying

/// What a stock level means when buying: the text key and the grade of the
/// warning: `"warnung"` (empty, nearly empty), `"hinweis"` (low) or `""`.
pub fn stock_level(offer: Option<&Offer>) -> Option<(&'static str, &'static str)> {
    let level = offer.map_or(0, |offer| offer.level);
    let key = match level {
        1 => "s_vk_k_vorrat_1",
        2 => "s_vk_k_vorrat_2",
        3 => "s_vk_k_vorrat_3",
        4 => "s_vk_k_vorrat_4",
        5 => "s_vk_k_vorrat_5",
        6 => "s_vk_k_vorrat_6",
        7 => "s_vk_k_vorrat_7",
        _ => return None,
    };
    Some(if level <= NEARLY_EMPTY {
        (key, "warnung")
    } else if level == LOW {
        (key, "hinweis")
    } else {
        (key, "")
    })
}

/// A place that sells a good.
#[derive(Clone, Debug, PartialEq)]
pub struct BuyPlace {
    pub terminal: String,
    pub place: String,
    pub system: String,
    pub nqa: bool,
    pub price: f64,
    pub boxes: String,
    /// SCU on the shelf; `0` unknown or empty.
    pub stock: i64,
    pub level: Option<(&'static str, &'static str)>,
    pub age: Option<f64>,
}

impl Selling {
    /// Where a good can be bought, the cheapest first (all offers, outliers
    /// too). With `nqa_only` only terminals that ask no questions.
    pub fn buy_places(&self, name: &str, nqa_only: bool, now: f64) -> Vec<BuyPlace> {
        let mut result = Vec::new();
        for row in self.buys.get(name).map_or(&[][..], |rows| &rows[..]) {
            let spot = self.terminals.get(&row.terminal);
            if nqa_only && !spot.is_some_and(|spot| spot.nqa) {
                continue;
            }
            result.push(BuyPlace {
                terminal: if row.name.is_empty() { "?".to_string() } else { row.name.clone() },
                place: spot.map(|s| s.place.clone()).unwrap_or_default(),
                system: spot.map(|s| s.system.clone()).unwrap_or_default(),
                nqa: spot.is_some_and(|s| s.nqa),
                price: row.price,
                boxes: row.boxes.clone(),
                stock: row.stock,
                level: stock_level(Some(row)),
                age: (row.date != 0).then_some(now - row.date as f64),
            });
        }
        result.sort_by(|a, b| a.price.total_cmp(&b.price).then_with(|| a.terminal.cmp(&b.terminal)));
        result
    }
}

// ------------------------------------------------------------ helper lists

/// The comparable forms of everything that can be mined (by ship, vehicle or
/// hand): the [`good_key`] of every ore found at a place of the mining data.
pub fn mineable_keys(mining: &crate::mining_data::MiningData) -> std::collections::HashSet<String> {
    let mut keys: std::collections::HashSet<String> =
        mining.ores().iter().filter(|ore| !ore.name.is_empty()).map(|ore| good_key(mining, &ore.name)).collect();
    keys.remove("");
    keys
}

/// The comparable form of a good without the addition `(Ore)` or `(Raw)`:
/// ore and refined good are one raw material. The key goes through
/// [`MiningData::material_key`](crate::mining_data::MiningData::material_key),
/// so `Pressurized Ice`, `Ice (Raw)` and `Raw Ice` are one good.
pub fn good_key(mining: &crate::mining_data::MiningData, name: &str) -> String {
    let trimmed = crate::text::py_strip(name);
    let lowered = trimmed.to_lowercase();
    for suffix in ["(ore)", "(raw)"] {
        if lowered.ends_with(suffix) {
            let cut = trimmed.len() - suffix.len();
            return mining.material_key(crate::text::py_strip(&trimmed[..cut]));
        }
    }
    mining.material_key(trimmed)
}

/// SCU per piece of the goods that are counted in pieces, by comparable name
/// (`waren-guete.json`).
pub fn piece_sizes(paths: &Paths) -> std::collections::HashMap<String, f64> {
    let mut sizes = std::collections::HashMap::new();
    let Ok(data) = crate::jsonio::read_json(&paths.app_file(crate::stock_data::QUALITY_GOODS_FILE)) else {
        return sizes;
    };
    if let Some(Value::Array(entries)) = data.get("waren") {
        for entry in entries {
            let Value::Array(parts) = entry else { continue };
            if parts.len() != 3 {
                continue;
            }
            let (Some(name), Some(size)) = (parts[0].as_str(), crate::stock_data::py_float_value(&parts[2])) else {
                continue;
            };
            if size > 0.0 && !matches!(parts[2], Value::Str(_)) {
                sizes.insert(crate::stock_data::norm_material(name), size);
            }
        }
    }
    sizes
}

// ------------------------------------------------------------ the own cargo

/// The goods rows of the trade stock, without items (`trade_cargo.goods_rows`).
fn goods_rows(entries: &[Value]) -> impl Iterator<Item = &Value> {
    entries.iter().filter(|entry| !truthy(entry.get("stueck")))
}

fn wanted(entry: &Value, only_stolen: Option<bool>) -> bool {
    only_stolen.is_none_or(|stolen| truthy(entry.get("gestohlen")) == stolen)
}

fn goods_of(entry: &Value) -> Option<&str> {
    match entry.get("ware") {
        Some(Value::Str(name)) if !name.is_empty() => Some(name),
        _ => None,
    }
}

fn amount_of(entry: &Value) -> f64 {
    entry.get("menge").filter(|v| truthy(Some(v))).and_then(crate::stock_data::py_float_value).unwrap_or(0.0)
}

/// How much of each good lies in the stock, in SCU (`trade_cargo.amounts`).
/// `only_stolen`: only marked goods (`true`), only clean ones (`false`), all.
pub fn cargo_amounts(entries: &[Value], only_stolen: Option<bool>) -> OrderedMap<f64> {
    let mut total: OrderedMap<f64> = OrderedMap::new();
    for entry in goods_rows(entries).filter(|e| wanted(e, only_stolen)) {
        if let Some(goods) = goods_of(entry) {
            *total.entry_or_default(goods) += amount_of(entry);
        }
    }
    total
}

/// What the goods in the stock cost on average per SCU, weighted by the
/// amount, only over rows with a price paid (`trade_cargo.costs`).
pub fn cargo_costs(entries: &[Value], only_stolen: Option<bool>) -> OrderedMap<f64> {
    let mut paid: OrderedMap<f64> = OrderedMap::new();
    let mut weight: OrderedMap<f64> = OrderedMap::new();
    for entry in goods_rows(entries).filter(|e| wanted(e, only_stolen)) {
        let (Some(goods), Some(cost)) = (goods_of(entry), crate::stock::cost_of(entry)) else { continue };
        let amount = amount_of(entry);
        *paid.entry_or_default(goods) += amount * cost;
        *weight.entry_or_default(goods) += amount;
    }
    let mut out = OrderedMap::new();
    for (goods, sum) in paid.iter() {
        let weight = weight.get(goods).copied().unwrap_or(0.0);
        if weight > 0.0 {
            out.set(goods, sum / weight);
        }
    }
    out
}

/// How much of each good has a price paid (`trade_cargo.costed_amounts`).
pub fn cargo_costed_amounts(entries: &[Value], only_stolen: Option<bool>) -> OrderedMap<f64> {
    let mut total: OrderedMap<f64> = OrderedMap::new();
    for entry in goods_rows(entries).filter(|e| wanted(e, only_stolen)) {
        if let Some(goods) = goods_of(entry)
            && crate::stock::cost_of(entry).is_some()
        {
            *total.entry_or_default(goods) += amount_of(entry);
        }
    }
    total
}

/// The names of the goods in the stock, in the order of the alphabet.
pub fn goods_in_stock(entries: &[Value], only_stolen: Option<bool>) -> Vec<String> {
    let mut names: Vec<String> = cargo_amounts(entries, only_stolen).iter().map(|(n, _)| n.to_string()).collect();
    names.sort();
    names
}

/// Does marked (stolen) goods lie in the stock? (`trade_cargo.has_stolen`)
pub fn has_stolen(entries: &[Value]) -> bool {
    goods_rows(entries).any(|entry| truthy(entry.get("gestohlen")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(terminal: &str, price: f64, level: i64) -> Offer {
        Offer { terminal: terminal.into(), name: format!("T{terminal}"), price, level, ..Offer::default() }
    }

    #[test]
    fn an_outlier_is_dropped_from_three_offers_on() {
        let rows = [offer("1", 100.0, 1), offer("2", 110.0, 1), offer("3", 9000.0, 1)];
        assert_eq!(without_outliers(&rows).len(), 2);
        let two = [offer("1", 100.0, 1), offer("3", 9000.0, 1)];
        assert_eq!(without_outliers(&two).len(), 2);
    }

    #[test]
    fn the_outlier_rule_acts_at_exactly_three_times_the_second_offer() {
        let high = |third: f64| [offer("1", 100.0, 1), offer("2", 100.0, 1), offer("3", third, 1)];
        assert_eq!(without_outliers(&high(300.0)).len(), 2, "exactly three times goes");
        assert_eq!(without_outliers(&high(299.0)).len(), 3, "just below stays");
        let low = |second: f64| [offer("1", 300.0, 1), offer("2", second, 1), offer("3", 900.0, 1)];
        let refs = |rows: &[Offer; 3]| without_low_outliers(&rows.iter().collect::<Vec<_>>()).len();
        assert_eq!(refs(&low(900.0)), 2, "the cheap one at a third goes");
        assert_eq!(refs(&low(899.0)), 3, "just above stays");
    }

    #[test]
    fn fill_levels_say_nothing_in_the_normal_case() {
        assert_eq!(fill_level(Some(&offer("1", 1.0, 1))), None);
        assert_eq!(fill_level(Some(&offer("1", 1.0, 4))), None);
        assert_eq!(fill_level(Some(&offer("1", 1.0, 5))), Some(("s_vk_fuellt", false)));
        assert_eq!(fill_level(Some(&offer("1", 1.0, 6))), Some(("s_vk_voll", true)));
        assert_eq!(fill_level(None), None);
    }

    #[test]
    fn stock_levels_mark_empty_and_low() {
        assert_eq!(stock_level(Some(&offer("1", 1.0, 1))), Some(("s_vk_k_vorrat_1", "warnung")));
        assert_eq!(stock_level(Some(&offer("1", 1.0, 3))), Some(("s_vk_k_vorrat_3", "hinweis")));
        assert_eq!(stock_level(Some(&offer("1", 1.0, 7))), Some(("s_vk_k_vorrat_7", "")));
        assert_eq!(stock_level(Some(&offer("1", 1.0, 0))), None);
    }

    #[test]
    fn the_sum_of_a_place_adds_like_python() {
        // Added one after the other these prices give 16.900000000000002, Python's `sum()` 16.9.
        let mut selling = Selling::default();
        for (good, price) in [("A", 9.9), ("B", 4.7), ("C", 2.3)] {
            selling.goods.set(good, vec![offer("1", price, 1)]);
        }
        let names: Vec<String> = ["A", "B", "C"].iter().map(ToString::to_string).collect();
        let places = selling.places_for(&names, false, 0.0);
        assert_eq!(places.len(), 1);
        assert_eq!(places[0].sum, 16.9);
    }

    #[test]
    fn the_top_list_leaves_event_gifts_out() {
        assert!(!in_top_list(" Luminalia Gift "));
        assert!(in_top_list("Gold"));
    }

    #[test]
    fn the_mining_filter_knows_a_raw_material_under_all_its_names_and_only_what_is_found() {
        let text = r#"{"format": 1, "locations": [
          {"locationName": "Daymar", "system": "Stanton", "groups": [
            {"groupName": "SpaceShip_Mineables", "groupProbability": 1.0, "deposits": [
              {"compositionGuid": "c1", "relativeProbability": 1}, {"compositionGuid": "c2", "relativeProbability": 1}]}]}],
         "compositions": {
           "c1": {"name": "Ice Deposit", "parts": [{"elementName": "Ice (Raw)", "elementGuid": "e1", "probability": 1, "minPercent": 50, "maxPercent": 50}]},
           "c2": {"name": "Iron Deposit", "parts": [{"elementName": "Iron (Ore)", "elementGuid": "e2", "probability": 1, "minPercent": 50, "maxPercent": 50}]}},
         "elemente": {"e1": {"name": "Ice (Raw)", "materialName": "Pressurized Ice"},
                      "e2": {"name": "Iron (Ore)", "materialName": "Iron"},
                      "e3": {"name": "Quantainium (Raw)", "materialName": "Quantainium"}}}"#;
        let mining = crate::mining_data::MiningData::of(&crate::jsonio::loads(text).unwrap());
        let mineable = mineable_keys(&mining);
        // The good of the shops, the name at the places and the name of the refinery are one raw material.
        for name in ["Pressurized Ice", "Ice (Raw)", "Raw Ice", "Iron", "Iron (Ore)", " iron (raw) "] {
            assert!(mineable.contains(&good_key(&mining, name)), "{name} is mineable: {mineable:?}");
        }
        // An element that is found at no place is no mining good (4.12 `_mining_goods` reads `mining.ores()`).
        assert!(!mineable.contains(&good_key(&mining, "Quantainium")), "{mineable:?}");
        assert!(!mineable.contains(&good_key(&mining, "Gold")));
        assert_eq!(good_key(&mining, "Pressurized Ice"), good_key(&mining, "Raw Ice"));
    }
}
