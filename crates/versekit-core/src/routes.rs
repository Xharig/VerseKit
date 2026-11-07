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

//! Trade routes (`routes.py` of 4.12): where to buy cheap, where to sell
//! dear, and what is left.
//!
//! UEX computes the single trips itself (endpoint `commodities_routes`): one
//! row says buy here, sell there, with both prices, the distance and the
//! stock. This module fetches the trips from one terminal at a time (one
//! answer stays far below the cut at 500 rows, a whole planet does not),
//! keeps them in `routen.json`, computes what fits the own cargo hold and
//! the own money, and chains trips: the destination of one is the start of
//! the next.
//!
//! * Three limits, not one: the cargo space, the stock at the start and the
//!   money. The need of the destination is a fourth.
//! * A chain follows only the best [`CHAIN_CANDIDATES`] destinations, each of
//!   them costs a request; on the way back of a round trip all trips count.
//! * The stock reported by players ages; the age belongs to every route.

use crate::jsonio::Value;
use crate::ordered::OrderedMap;
use crate::paths::Paths;
use crate::selling::{Selling, TRADE_TYPES};
use crate::stock_data::{py_float_value, truthy};
use crate::stock_fetch::{Fetch, switched_off};
use crate::uex::{self, Store};

/// The endpoint of the trips from one terminal; the number follows.
pub const SOURCE: &str = "https://api.uexcorp.uk/2.0/commodities_routes?id_terminal_origin=";
/// The file in the data folder.
pub const CACHE: &str = "routen.json";
/// The format number.
pub const FORMAT: i64 = 1;
/// How long the trips of a start count as fresh: six hours.
pub const SHELF_LIFE: f64 = 6.0 * 60.0 * 60.0;
/// How many starts the file keeps; it must lie above the number of trading posts.
pub const MAX_STARTS: usize = 200;
/// How many destinations a chain follows.
pub const CHAIN_CANDIDATES: usize = 5;
/// How many trips are looked at on the way back of a round trip.
pub const RETURN_CANDIDATES: usize = 400;
/// The most trips of one route.
pub const MAX_STOPS: i64 = 4;

const STORE: Store = Store::new(CACHE, FORMAT, SHELF_LIFE);

/// One trip as UEX reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trip {
    /// The good (`ware`).
    pub good: String,
    /// The number of the destination terminal (`ziel`).
    pub dest: String,
    /// The name of the destination terminal (`zielname`).
    pub dest_name: String,
    /// The place of the destination (`zielort`).
    pub dest_place: String,
    /// The star system of the destination (`zielsystem`).
    pub dest_system: String,
    /// The price to buy per SCU (`ek`).
    pub buy: f64,
    /// The price to sell per SCU (`vk`).
    pub sell: f64,
    /// The profit per SCU (`gewinn_scu`).
    pub profit_scu: f64,
    /// The distance in Gm; `0` where UEX knows none (`strecke`).
    pub distance: f64,
    /// SCU on the shelf at the start (`vorrat`).
    pub stock: i64,
    /// SCU the destination wants (`bedarf`).
    pub demand: i64,
}

/// A trip for a given hold and given money.
#[derive(Clone, Debug, PartialEq)]
pub struct Leg {
    pub trip: Trip,
    /// SCU that fit (`menge`).
    pub amount: i64,
    /// The profit of that amount (`gewinn`).
    pub profit: f64,
    /// The text key of what limits the amount, or `""` (`grenze`).
    pub limit: &'static str,
    /// The name of the start (best list only).
    pub start_name: String,
}

/// The trips from one start.
#[derive(Clone, Debug, Default)]
pub struct Start {
    /// When they were fetched (`geholt`).
    pub fetched: f64,
    /// The trips, the largest profit per SCU first (`fahrten`).
    pub trips: Vec<Trip>,
}

/// The stored trips of all starts.
#[derive(Clone, Debug, Default)]
pub struct Routes {
    pub starts: OrderedMap<Start>,
}

fn py_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => s.clone(),
        Some(Value::Int(n)) => n.clone(),
        Some(Value::Float(x)) => crate::jsonio::float_text(*x),
        Some(Value::Bool(true)) => "True".to_string(),
        _ => String::new(),
    }
}

fn text(value: Option<&Value>) -> String {
    match value {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

fn int_of(value: Option<&Value>) -> i64 {
    value.and_then(crate::stock_fetch::py_int).unwrap_or(0)
}

fn float_of(value: Option<&Value>) -> f64 {
    value.and_then(py_float_value).unwrap_or(0.0)
}

fn trip_of(value: &Value) -> Trip {
    Trip {
        good: text(value.get("ware")),
        dest: py_text(value.get("ziel")),
        dest_name: text(value.get("zielname")),
        dest_place: text(value.get("zielort")),
        dest_system: text(value.get("zielsystem")),
        buy: float_of(value.get("ek")),
        sell: float_of(value.get("vk")),
        profit_scu: float_of(value.get("gewinn_scu")),
        distance: float_of(value.get("strecke")),
        stock: int_of(value.get("vorrat")),
        demand: int_of(value.get("bedarf")),
    }
}

fn trip_value(trip: &Trip) -> Value {
    Value::Object(vec![
        uex::field("ware", Value::Str(trip.good.clone())),
        uex::field("ziel", Value::Str(trip.dest.clone())),
        uex::field("zielname", Value::Str(trip.dest_name.clone())),
        uex::field("zielort", Value::Str(trip.dest_place.clone())),
        uex::field("zielsystem", Value::Str(trip.dest_system.clone())),
        uex::field("ek", uex::float(trip.buy)),
        uex::field("vk", uex::float(trip.sell)),
        uex::field("gewinn_scu", uex::float(trip.profit_scu)),
        uex::field("strecke", uex::float(trip.distance)),
        uex::field("vorrat", Value::int(trip.stock)),
        uex::field("bedarf", Value::int(trip.demand)),
    ])
}

impl Routes {
    /// Reads the stored file; without a usable one nothing is known.
    pub fn load(paths: &Paths) -> Routes {
        STORE.load(paths).map_or_else(Routes::default, |data| Routes::from_value(&data))
    }

    /// The trips of a file already read.
    pub fn from_value(data: &Value) -> Routes {
        let mut starts = OrderedMap::new();
        if let Some(Value::Object(entries)) = data.get("starts") {
            for (ident, entry) in entries {
                let trips = match entry.get("fahrten") {
                    Some(Value::Array(rows)) => rows.iter().map(trip_of).collect(),
                    _ => Vec::new(),
                };
                starts.set(ident, Start { fetched: float_of(entry.get("geholt")), trips });
            }
        }
        Routes { starts }
    }

    fn value(&self) -> Value {
        Value::Object(vec![uex::field(
            "starts",
            Value::Object(
                self.starts
                    .iter()
                    .map(|(ident, start)| {
                        (
                            ident.to_string(),
                            Value::Object(vec![
                                uex::field("geholt", uex::float(start.fetched)),
                                uex::field("fahrten", Value::Array(start.trips.iter().map(trip_value).collect())),
                            ]),
                        )
                    })
                    .collect(),
            ),
        )])
    }

    /// How old the trips from this start are; `None` without them.
    pub fn age(&self, start: &str, now: f64) -> Option<f64> {
        let entry = self.starts.get(start)?;
        Some(now - entry.fetched)
    }

    /// All known trips from a terminal: `None` means not looked at yet, an
    /// empty list means nothing pays from there.
    pub fn trips(&self, start: &str) -> Option<&[Trip]> {
        self.starts.get(start).map(|entry| &entry.trips[..])
    }

    /// How many of the trading posts have their trips stored. Only posts of
    /// the list count: a number larger than its denominator spoils the display.
    pub fn known_starts(&self, posts: &[(String, String)]) -> usize {
        if posts.is_empty() {
            return self.starts.len();
        }
        self.starts.iter().filter(|(ident, _)| posts.iter().any(|(post, _)| post == ident)).count()
    }

    /// Forgets everything looked up (for checks).
    pub fn forget(paths: &Paths, now: f64) -> bool {
        STORE.save(paths, vec![uex::field("starts", Value::Object(Vec::new()))], true, now).is_ok()
    }
}

/// Fetches the trips from a terminal (`fetch`). `true` when trips are stored
/// afterwards; a fresh stored list is not fetched again unless `force`.
pub fn fetch_start(paths: &Paths, routes: &mut Routes, start: &str, force: bool, now: f64, fetch: Fetch<'_>) -> bool {
    if switched_off(&paths.host) || start.is_empty() {
        return false;
    }
    if !force && routes.age(start, now).is_some_and(|age| age < SHELF_LIFE) {
        return true;
    }
    let Some(data) = uex::fetch("routes", &format!("{SOURCE}{start}"), fetch) else { return false };
    let mut items = Vec::new();
    for row in uex::rows(&data) {
        let (Some(buy), Some(sell)) = (uex::number(row, "price_origin"), uex::number(row, "price_destination")) else {
            return false;
        };
        // No price to buy, no margin, no stock: no trip.
        if buy <= 0.0 || sell <= buy || !truthy(row.get("scu_origin")) {
            continue;
        }
        let Some(stock) = uex::whole(row, "scu_origin") else { return false };
        let (Some(demand), Some(distance)) = (uex::whole(row, "scu_destination"), uex::number(row, "distance")) else {
            return false;
        };
        let dest = if truthy(row.get("id_terminal_destination")) {
            py_text(row.get("id_terminal_destination"))
        } else {
            String::new()
        };
        items.push(Trip {
            good: uex::stripped(row, "commodity_name"),
            dest,
            dest_name: uex::stripped(row, "destination_terminal_name"),
            dest_place: uex::first_set(row, &["destination_planet_name", "destination_orbit_name"]),
            dest_system: uex::stripped(row, "destination_star_system_name"),
            buy,
            sell,
            profit_scu: sell - buy,
            distance,
            stock,
            demand,
        });
    }
    items.sort_by(|a, b| b.profit_scu.total_cmp(&a.profit_scu));
    // The file is read again before the own start joins it: another fetch may have written meanwhile.
    *routes = Routes::load(paths);
    routes.starts.set(start, Start { fetched: now, trips: items });
    trim(routes);
    save_routes(paths, routes, now)
}

fn save_routes(paths: &Paths, routes: &Routes, now: f64) -> bool {
    let Value::Object(fields) = routes.value() else { return false };
    STORE.save(paths, fields, true, now).is_ok()
}

/// Keeps the newest [`MAX_STARTS`] starts.
fn trim(routes: &mut Routes) {
    let count = routes.starts.len();
    if count <= MAX_STARTS {
        return;
    }
    let mut by_age: Vec<(String, f64)> =
        routes.starts.iter().map(|(ident, start)| (ident.to_string(), start.fetched)).collect();
    by_age.sort_by(|a, b| a.1.total_cmp(&b.1));
    let drop: Vec<String> = by_age.into_iter().take(count - MAX_STARTS).map(|(ident, _)| ident).collect();
    let mut kept = OrderedMap::new();
    for (ident, start) in routes.starts.iter() {
        if !drop.iter().any(|d| d == ident) {
            kept.set(ident, start.clone());
        }
    }
    routes.starts = kept;
}

// ------------------------------------------------------------ computing

/// Python's `a // b` for floats.
pub fn py_floordiv(a: f64, b: f64) -> f64 {
    let modulo = a % b;
    let mut div = (a - modulo) / b;
    if modulo != 0.0 && (b < 0.0) != (modulo < 0.0) {
        div -= 1.0;
    }
    if div != 0.0 {
        let mut floor = div.floor();
        if div - floor > 0.5 {
            floor += 1.0;
        }
        floor
    } else {
        0.0_f64.copysign(a / b)
    }
}

/// How much fits and what it brings: `(amount, profit)`. Three limits: the
/// cargo space, the stock at the start and the money; the need of the
/// destination is a fourth.
pub fn amount_and_profit(trip: &Trip, free_scu: i64, money: f64) -> (i64, f64) {
    let mut amount = free_scu.min(trip.stock);
    if trip.demand != 0 {
        amount = amount.min(trip.demand);
    }
    if trip.buy != 0.0 {
        amount = amount.min(py_floordiv(money, trip.buy) as i64);
    }
    amount = amount.max(0);
    (amount, amount as f64 * trip.profit_scu)
}

/// What limits the amount: the text key, or `""` where nothing does. What
/// the player can change (money, ship) comes first.
pub fn what_limits(trip: &Trip, free_scu: i64, money: f64) -> &'static str {
    let (amount, _) = amount_and_profit(trip, free_scu, money);
    if amount == 0 {
        return "";
    }
    if trip.buy != 0.0 && amount == py_floordiv(money, trip.buy) as i64 && amount < free_scu {
        return "s_rt_grenze_geld";
    }
    if amount == free_scu {
        return "s_rt_grenze_frachtraum";
    }
    if amount == trip.stock {
        return "s_rt_grenze_vorrat";
    }
    if trip.demand != 0 && amount == trip.demand {
        return "s_rt_grenze_bedarf";
    }
    ""
}

fn leg_of(trip: &Trip, amount: i64, profit: f64, limit: &'static str) -> Leg {
    Leg { trip: trip.clone(), amount, profit, limit, start_name: String::new() }
}

/// The best single trips from a place for this hold and this money.
pub fn single_trips(routes: &Routes, start: &str, scu: i64, money: f64, most: usize) -> Vec<Leg> {
    let mut result: Vec<Leg> = Vec::new();
    for trip in routes.trips(start).unwrap_or(&[]) {
        let (amount, profit) = amount_and_profit(trip, scu, money);
        if amount > 0 && profit > 0.0 {
            result.push(leg_of(trip, amount, profit, what_limits(trip, scu, money)));
        }
    }
    result.sort_by(|a, b| b.profit.total_cmp(&a.profit));
    result.truncate(most);
    result
}

/// What to do to get the trips from a place that are not stored yet: fetch
/// them (and store them), `true` when it worked.
pub type Missing<'a> = &'a mut dyn FnMut(&mut Routes, &str) -> bool;

/// How a chain is asked for.
#[derive(Clone, Copy, Debug)]
pub struct ChainOptions {
    /// Sort by the whole distance instead of the profit.
    pub short: bool,
    /// How many chains to give back.
    pub most: usize,
    /// How many trips (2 up to [`MAX_STOPS`]).
    pub stops: i64,
    /// The last trip must lead back to the start.
    pub round_trip: bool,
}

/// Several trips in a row: the destination of one is the start of the next.
/// Gives `(whole profit, trips)`.
pub fn chain(
    routes: &mut Routes,
    start: &str,
    scu: i64,
    money: f64,
    options: ChainOptions,
    missing: Missing<'_>,
) -> Vec<(f64, Vec<Leg>)> {
    let stops = options.stops.clamp(2, MAX_STOPS) as usize;
    let round_trip = options.round_trip;
    // A branch is (profit so far, place now, trips so far).
    let mut branches: Vec<(f64, String, Vec<Leg>)> = vec![(0.0, start.to_string(), Vec::new())];
    for step in 0..stops {
        let mut next_branches: Vec<(f64, String, Vec<Leg>)> = Vec::new();
        let last_step = step == stops - 1;
        for (profit_so_far, place, so_far) in &branches {
            if routes.trips(place).is_none() && !missing(routes, place) {
                continue;
            }
            // On the way back all trips count: the one that leads home is
            // hardly ever among the best five.
            let limit = if round_trip && last_step { RETURN_CANDIDATES } else { CHAIN_CANDIDATES };
            for leg in single_trips(routes, place, scu, money + profit_so_far, limit) {
                if leg.trip.dest.is_empty() {
                    continue;
                }
                let seen = so_far.iter().any(|b| b.trip.dest == leg.trip.dest) || leg.trip.dest == start;
                if seen && !(round_trip && last_step && leg.trip.dest == start) {
                    continue;
                }
                let mut way = so_far.clone();
                let dest = leg.trip.dest.clone();
                let profit = profit_so_far + leg.profit;
                way.push(leg);
                next_branches.push((profit, dest, way));
            }
        }
        // Weed out first, then cut: only branches that end at the start count.
        if round_trip && last_step {
            next_branches.retain(|branch| branch.1 == start);
        }
        next_branches.sort_by(|a, b| b.0.total_cmp(&a.0));
        next_branches.truncate(CHAIN_CANDIDATES);
        branches = next_branches;
        if branches.is_empty() {
            return Vec::new();
        }
    }
    let mut done: Vec<(f64, Vec<Leg>)> = branches
        .into_iter()
        .filter(|(_, place, way)| way.len() == stops && (!round_trip || place == start))
        .map(|(profit, _, way)| (profit, way))
        .collect();
    if options.short {
        // A trip without a distance drops out instead of passing as "0 Gm".
        done.retain(|(_, way)| way.iter().all(|leg| leg.trip.distance != 0.0));
        done.sort_by(|a, b| {
            let (da, db) = (distance_sum(&a.1), distance_sum(&b.1));
            da.total_cmp(&db).then(b.0.total_cmp(&a.0))
        });
    } else {
        done.sort_by(|a, b| b.0.total_cmp(&a.0));
    }
    done.truncate(options.most);
    done
}

/// Python's `sum()` of the distances: the order of the legs must not tip two equal ways apart.
fn distance_sum(way: &[Leg]) -> f64 {
    crate::text::py_sum(way.iter().map(|leg| leg.trip.distance))
}

// ------------------------------------------------------------ everywhere

/// Every terminal that trades in goods, `(number, name)`, in the order of the
/// price list. A list that does not know the kind takes all terminals.
pub fn trade_posts(selling: &Selling) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for (ident, spot) in selling.terminals.iter() {
        if let Some(kind) = &spot.kind
            && !TRADE_TYPES.contains(&kind.as_str())
        {
            continue;
        }
        let name = if !spot.name.is_empty() {
            spot.name.clone()
        } else if !spot.place.is_empty() {
            spot.place.clone()
        } else {
            "?".to_string()
        };
        result.push((ident.to_string(), name));
    }
    result
}

/// Fetches the trips of all trading posts. `progress(done, total)` is called
/// after each post, `cancel()` ends the run. Gives how many posts were done.
pub fn fetch_all(
    paths: &Paths,
    routes: &mut Routes,
    posts: &[(String, String)],
    now: f64,
    fetch: Fetch<'_>,
    progress: &mut dyn FnMut(usize, usize),
    cancel: &dyn Fn() -> bool,
) -> usize {
    if switched_off(&paths.host) {
        return 0;
    }
    let mut done = 0;
    for (ident, _) in posts {
        if cancel() {
            break;
        }
        fetch_start(paths, routes, ident, false, now, fetch);
        done += 1;
        progress(done, posts.len());
    }
    done
}

/// The best single trips of all stored starts; fetches nothing.
pub fn best_anywhere(routes: &Routes, posts: &[(String, String)], scu: i64, money: f64, most: usize) -> Vec<Leg> {
    let mut result = Vec::new();
    for (ident, entry) in routes.starts.iter() {
        for trip in &entry.trips {
            let (amount, profit) = amount_and_profit(trip, scu, money);
            if amount <= 0 || profit <= 0.0 {
                continue;
            }
            let mut leg = leg_of(trip, amount, profit, what_limits(trip, scu, money));
            leg.start_name =
                posts.iter().find(|(post, _)| post == ident).map_or("?".to_string(), |(_, name)| name.clone());
            result.push(leg);
        }
    }
    result.sort_by(|a, b| b.profit.total_cmp(&a.profit));
    result.truncate(most);
    result
}

/// The best chains of all stored starts, fetching nothing: `(profit, name of
/// the start, trips)`.
pub fn best_chains_anywhere(
    routes: &mut Routes,
    posts: &[(String, String)],
    scu: i64,
    money: f64,
    options: ChainOptions,
) -> Vec<(f64, String, Vec<Leg>)> {
    let mut result = Vec::new();
    let idents: Vec<String> = routes.starts.iter().map(|(ident, _)| ident.to_string()).collect();
    for ident in idents {
        let inner = ChainOptions { most: 3, ..options };
        for (profit, way) in chain(routes, &ident, scu, money, inner, &mut |_, _| false) {
            let name = posts.iter().find(|(post, _)| *post == ident).map_or("?".to_string(), |(_, name)| name.clone());
            result.push((profit, name, way));
        }
    }
    if options.short {
        result.sort_by(|a, b| distance_sum(&a.2).total_cmp(&distance_sum(&b.2)).then(b.0.total_cmp(&a.0)));
    } else {
        result.sort_by(|a, b| b.0.total_cmp(&a.0));
    }
    result.truncate(options.most);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SC_BP_NO_NET` of the environment given stops the fetching before anything is asked;
    /// without it the trips are asked for, whatever the shell of the check has.
    #[test]
    fn no_net_of_the_given_environment_asks_nothing() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let dir = std::env::temp_dir().join(format!("versekit-routen-aus-{}", crate::testdir::run_tag()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let data = dir.to_string_lossy().into_owned();
        let paths = |no_net: &str| {
            let vars = [("SC_BP_HOME", data.as_str()), (crate::legacy::ENV_NO_NET, no_net)];
            Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, "/"))
        };
        let asked = AtomicUsize::new(0);
        let fetch = |_: &str| -> Result<Vec<u8>, crate::scmdb::FetchError> {
            asked.fetch_add(1, Ordering::Relaxed);
            Err(crate::scmdb::FetchError::Missing)
        };
        let mut routes = Routes::default();
        let off = paths("1");
        assert!(!fetch_start(&off, &mut routes, "Area18", true, 0.0, &fetch));
        let posts = [("1".to_string(), "Area18".to_string())];
        assert_eq!(fetch_all(&off, &mut routes, &posts, 0.0, &fetch, &mut |_, _| {}, &|| false), 0);
        assert_eq!(asked.load(Ordering::Relaxed), 0);
        fetch_start(&paths("0"), &mut routes, "Area18", true, 0.0, &fetch);
        assert!(asked.load(Ordering::Relaxed) > 0, "without the switch the trips are asked for");
        let _ = std::fs::remove_dir_all(dir);
    }

    fn trip(good: &str, dest: &str, buy: f64, sell: f64, stock: i64, demand: i64, distance: f64) -> Trip {
        Trip {
            good: good.into(),
            dest: dest.into(),
            buy,
            sell,
            profit_scu: sell - buy,
            stock,
            demand,
            distance,
            ..Trip::default()
        }
    }

    fn book(entries: &[(&str, Vec<Trip>)]) -> Routes {
        let mut routes = Routes::default();
        for (ident, trips) in entries {
            routes.starts.set(ident, Start { fetched: 0.0, trips: trips.clone() });
        }
        routes
    }

    fn options(stops: i64, round_trip: bool, short: bool) -> ChainOptions {
        ChainOptions { short, most: 5, stops, round_trip }
    }

    #[test]
    fn the_distance_of_a_way_adds_like_python_whatever_the_order() {
        let way = |distances: [f64; 3]| -> Vec<Leg> {
            distances.iter().map(|d| leg_of(&trip("Gold", "B", 1.0, 2.0, 10, 0, *d), 10, 10.0, "")).collect()
        };
        let (up, down) = (distance_sum(&way([0.1, 0.2, 0.3])), distance_sum(&way([0.3, 0.2, 0.1])));
        assert_eq!(up, down, "the same legs in another order are the same distance");
        assert_eq!(up, 0.6);
    }

    #[test]
    fn python_floor_division_differs_from_floor_of_the_quotient() {
        assert_eq!(py_floordiv(1.0, 0.1), 9.0);
        assert_eq!((1.0_f64 / 0.1).floor(), 10.0);
        assert_eq!(py_floordiv(0.3, 0.1), 2.0);
        assert_eq!(py_floordiv(500000.0, 2643.0), 189.0);
    }

    #[test]
    fn the_amount_is_the_smallest_of_hold_stock_need_and_money() {
        let t = trip("Gold", "B", 100.0, 150.0, 45, 0, 1.0);
        assert_eq!(amount_and_profit(&t, 96, 1_000_000.0), (45, 45.0 * 50.0));
        assert_eq!(what_limits(&t, 96, 1_000_000.0), "s_rt_grenze_vorrat");
        assert_eq!(amount_and_profit(&t, 96, 2_000.0).0, 20);
        assert_eq!(what_limits(&t, 96, 2_000.0), "s_rt_grenze_geld");
        assert_eq!(what_limits(&t, 10, 1_000_000.0), "s_rt_grenze_frachtraum");
        let need = trip("Gold", "B", 100.0, 150.0, 45, 30, 1.0);
        assert_eq!(what_limits(&need, 96, 1_000_000.0), "s_rt_grenze_bedarf");
        assert_eq!(what_limits(&need, 0, 1_000_000.0), "");
    }

    #[test]
    fn the_file_keeps_the_newest_two_hundred_starts() {
        let mut routes = Routes::default();
        for i in 0..205 {
            routes.starts.set(&i.to_string(), Start { fetched: f64::from(i), trips: Vec::new() });
        }
        trim(&mut routes);
        assert_eq!(routes.starts.len(), MAX_STARTS);
        assert!(routes.starts.get("4").is_none(), "the oldest five are gone");
        assert!(routes.starts.get("5").is_some() && routes.starts.get("204").is_some());
    }

    #[test]
    fn when_money_and_hold_limit_alike_the_hold_is_named() {
        let t = trip("Gold", "B", 100.0, 150.0, 500, 0, 1.0);
        assert_eq!(amount_and_profit(&t, 10, 1000.0).0, 10);
        assert_eq!(what_limits(&t, 10, 1000.0), "s_rt_grenze_frachtraum", "a tie goes to the hold");
        assert_eq!(what_limits(&t, 10, 999.0), "s_rt_grenze_geld", "one coin less: the money");
    }

    #[test]
    fn a_round_trip_needs_the_trip_home_that_is_not_among_the_best() {
        // A -> B pays most; B -> A pays least. Without all trips on the way back no round trip exists.
        let mut trips_b = vec![trip("X", "C", 10.0, 90.0, 100, 0, 1.0)];
        for i in 0..6 {
            trips_b.push(trip("Y", &format!("D{i}"), 10.0, 40.0 + f64::from(i), 100, 0, 1.0));
        }
        trips_b.push(trip("Z", "A", 10.0, 12.0, 100, 0, 1.0));
        trips_b.sort_by(|a, b| b.profit_scu.total_cmp(&a.profit_scu));
        let mut routes = book(&[("A", vec![trip("W", "B", 10.0, 100.0, 100, 0, 2.0)]), ("B", trips_b)]);
        let found = chain(&mut routes, "A", 100, 100_000.0, options(2, true, false), &mut |_, _| false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1.last().map(|leg| leg.trip.dest.as_str()), Some("A"));
        let open = chain(&mut routes, "A", 100, 100_000.0, options(2, false, false), &mut |_, _| false);
        assert!(open.len() > 1);
    }

    #[test]
    fn the_short_route_drops_trips_without_a_distance() {
        let mut routes = book(&[
            ("A", vec![trip("W", "B", 10.0, 100.0, 100, 0, 0.0)]),
            ("B", vec![trip("X", "C", 10.0, 50.0, 100, 0, 3.0)]),
        ]);
        assert!(chain(&mut routes, "A", 100, 100_000.0, options(2, false, true), &mut |_, _| false).is_empty());
        assert_eq!(chain(&mut routes, "A", 100, 100_000.0, options(2, false, false), &mut |_, _| false).len(), 1);
    }

    #[test]
    fn a_place_is_not_visited_twice() {
        let mut routes = book(&[
            ("A", vec![trip("W", "B", 10.0, 100.0, 100, 0, 1.0)]),
            ("B", vec![trip("X", "A", 10.0, 50.0, 100, 0, 1.0), trip("Y", "C", 10.0, 20.0, 100, 0, 1.0)]),
        ]);
        let found = chain(&mut routes, "A", 100, 100_000.0, options(2, false, false), &mut |_, _| false);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].1[1].trip.dest, "C");
    }

    #[test]
    fn the_known_starts_count_only_trading_posts() {
        let routes = book(&[("A", vec![]), ("B", vec![]), ("SHOP", vec![])]);
        let posts = vec![("A".to_string(), "a".to_string()), ("B".to_string(), "b".to_string())];
        assert_eq!(routes.known_starts(&posts), 2);
        assert_eq!(routes.known_starts(&[]), 3);
    }
}
