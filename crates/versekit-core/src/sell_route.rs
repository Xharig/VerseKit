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

//! A selling route: several goods, each with an amount, sold over a few
//! stations in a row.
//!
//! * Choosing: the stations are taken one by one, each time the one that
//!   raises the whole proceeds most (every good counts at the best price
//!   among the stations taken). A tie goes to the lower terminal number.
//! * Ordering: from the start (or the station with the largest proceeds) to
//!   the nearest station whose distance is known; where no distance is known
//!   the largest proceeds go first.
//! * Distances are those of the stored trips ([`crate::routes`]): a pair of
//!   terminals has one where UEX lists a trip between them.
//! * A station counts as full when the price list says so (fill level) or
//!   the player says so; a full station is left out.

use std::collections::HashMap;

use crate::paths::Paths;
use crate::routes::{self, Routes};
use crate::selling::{self, Offer, Selling};
use crate::stock_fetch::Fetch;

/// What is asked of the route.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
    /// The goods to sell with the amount in SCU; an amount of zero or less is ignored.
    pub items: Vec<(String, f64)>,
    /// Only stations of this star system; empty takes all.
    pub system: String,
    /// Only stations that ask no questions.
    pub nqa_only: bool,
    /// Stations the price list marks as full count too.
    pub include_full: bool,
    /// The most stations of the route (at least one).
    pub max_stops: usize,
    /// The terminal the way starts at.
    pub start: Option<String>,
    /// Terminals the player calls full.
    pub blocked: Vec<String>,
}

/// One good sold at a station.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub good: String,
    pub scu: f64,
    pub price: f64,
    pub proceeds: f64,
    /// What the fill level of the offer says: text key and whether it is a warning.
    pub fill: Option<(&'static str, bool)>,
}

/// One station of the route.
#[derive(Clone, Debug, PartialEq)]
pub struct Stop {
    pub terminal: String,
    pub name: String,
    pub place: String,
    pub system: String,
    pub nqa: bool,
    /// Gm from the station before (or the start); `None` where no distance is known.
    pub distance: Option<f64>,
    pub lines: Vec<Line>,
    pub proceeds: f64,
}

/// The route and what is around it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Route {
    pub stops: Vec<Stop>,
    pub proceeds: f64,
    /// What the best single station brings for the same goods (what it takes of them).
    pub best_single: f64,
    /// Goods no station takes: name and SCU.
    pub unsold: Vec<(String, f64)>,
    /// The sum of the known distances.
    pub distance: f64,
    /// How many legs have no known distance.
    pub unknown_legs: usize,
    /// Did the order follow the distances (at least one leg known)?
    pub by_distance: bool,
}

/// The distances between terminals as the stored trips know them.
#[derive(Clone, Debug, Default)]
pub struct Distances {
    pairs: HashMap<(String, String), f64>,
}

impl Distances {
    /// Reads the distances of all stored starts; a pair counts both ways.
    pub fn from_routes(routes: &Routes) -> Distances {
        let mut pairs = HashMap::new();
        for (origin, start) in routes.starts.iter() {
            for trip in &start.trips {
                if trip.distance > 0.0 && !trip.dest.is_empty() {
                    pairs.insert((origin.to_string(), trip.dest.clone()), trip.distance);
                    pairs.entry((trip.dest.clone(), origin.to_string())).or_insert(trip.distance);
                }
            }
        }
        Distances { pairs }
    }

    /// Gm between two terminals; `Some(0)` for the same one, `None` where unknown.
    pub fn between(&self, a: &str, b: &str) -> Option<f64> {
        if a == b {
            return Some(0.0);
        }
        self.pairs.get(&(a.to_string(), b.to_string())).copied()
    }
}

/// Orders terminal numbers the same way every time: by length, then as text.
fn id_order(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

/// The best offer of a terminal for a good, with an outlier dropped as everywhere.
struct Candidate<'a> {
    good: usize,
    terminal: &'a str,
    offer: &'a Offer,
}

fn candidates<'a>(selling: &'a Selling, query: &Query, items: &[(&str, f64)]) -> Vec<Candidate<'a>> {
    let mut found: Vec<Candidate<'a>> = Vec::new();
    for (index, (good, _)) in items.iter().enumerate() {
        for offer in selling.offers_without_outliers(good) {
            let spot = selling.terminals.get(&offer.terminal);
            if query.nqa_only && !spot.is_some_and(|spot| spot.nqa) {
                continue;
            }
            if !query.system.is_empty() && !spot.is_some_and(|spot| spot.system == query.system) {
                continue;
            }
            if query.blocked.contains(&offer.terminal) {
                continue;
            }
            if !query.include_full && offer.level >= selling::NO_DEMAND {
                continue;
            }
            if offer.price <= 0.0 {
                continue;
            }
            // One terminal may list a good twice: the better price counts.
            match found.iter_mut().find(|c| c.good == index && c.terminal == offer.terminal) {
                Some(have) if have.offer.price >= offer.price => {}
                Some(have) => have.offer = offer,
                None => found.push(Candidate { good: index, terminal: &offer.terminal, offer }),
            }
        }
    }
    found
}

/// The proceeds when `chosen` terminals take the goods, each at its best price among them.
fn proceeds_of(chosen: &[&str], found: &[Candidate<'_>], items: &[(&str, f64)]) -> f64 {
    let mut total = 0.0;
    for (index, (_, scu)) in items.iter().enumerate() {
        let best = found
            .iter()
            .filter(|c| c.good == index && chosen.contains(&c.terminal))
            .map(|c| c.offer.price)
            .reduce(f64::max);
        total += best.unwrap_or(0.0) * scu;
    }
    total
}

/// The stations chosen with what is sold at each, what the best single station brings, and the goods left unsold.
type Picked = (Vec<(String, Vec<Line>)>, f64, Vec<(String, f64)>);

/// Which stations to go to and what is sold where, before any order.
fn choose(selling: &Selling, query: &Query) -> Picked {
    let items: Vec<(&str, f64)> = query
        .items
        .iter()
        .filter(|(good, scu)| *scu > 0.0 && scu.is_finite() && selling.known(good))
        .map(|(good, scu)| (good.as_str(), *scu))
        .collect();
    let found = candidates(selling, query, &items);
    let mut terminals: Vec<&str> = found.iter().map(|c| c.terminal).collect();
    terminals.sort_by(|a, b| id_order(a, b));
    terminals.dedup();

    let best_single = terminals.iter().map(|t| proceeds_of(&[t], &found, &items)).fold(0.0, f64::max);
    let mut chosen: Vec<&str> = Vec::new();
    let mut current = 0.0;
    while chosen.len() < query.max_stops.max(1) {
        let mut best: Option<(&str, f64)> = None;
        for terminal in &terminals {
            if chosen.contains(terminal) {
                continue;
            }
            chosen.push(terminal);
            let gain = proceeds_of(&chosen, &found, &items) - current;
            chosen.pop();
            // Terminals come in order: only a larger gain replaces, so a tie keeps the lower number.
            if gain > 0.0 && best.is_none_or(|(_, top)| gain > top) {
                best = Some((terminal, gain));
            }
        }
        let Some((terminal, gain)) = best else { break };
        chosen.push(terminal);
        current += gain;
    }

    // Every good goes to the station with the best price; a tie to the lower number.
    let mut stops: Vec<(String, Vec<Line>)> = chosen.iter().map(|t| (t.to_string(), Vec::new())).collect();
    let mut sold = vec![false; items.len()];
    for (index, (good, scu)) in items.iter().enumerate() {
        let mut best: Option<&Candidate<'_>> = None;
        for c in found.iter().filter(|c| c.good == index && chosen.contains(&c.terminal)) {
            let better = best.is_none_or(|top| {
                c.offer.price > top.offer.price
                    || (c.offer.price == top.offer.price && id_order(c.terminal, top.terminal).is_lt())
            });
            if better {
                best = Some(c);
            }
        }
        if let Some(c) = best
            && let Some(stop) = stops.iter_mut().find(|(t, _)| t == c.terminal)
        {
            sold[index] = true;
            stop.1.push(Line {
                good: (*good).to_string(),
                scu: *scu,
                price: c.offer.price,
                proceeds: c.offer.price * scu,
                fill: selling::fill_level(Some(c.offer)),
            });
        }
    }
    stops.retain(|(_, lines)| !lines.is_empty());
    let unsold = items
        .iter()
        .zip(&sold)
        .filter(|(_, sold)| !**sold)
        .map(|((good, scu), _)| ((*good).to_string(), *scu))
        .collect();
    (stops, best_single, unsold)
}

fn stop_of(selling: &Selling, terminal: &str, lines: Vec<Line>) -> Stop {
    let spot = selling.terminals.get(terminal);
    let name = selling
        .goods
        .values()
        .flat_map(|offers| offers.iter())
        .find(|offer| offer.terminal == terminal && !offer.name.is_empty())
        .map(|offer| offer.name.clone())
        .or_else(|| spot.map(|spot| spot.name.clone()).filter(|name| !name.is_empty()))
        .unwrap_or_else(|| "?".to_string());
    Stop {
        terminal: terminal.to_string(),
        name,
        place: spot.map(|s| s.place.clone()).unwrap_or_default(),
        system: spot.map(|s| s.system.clone()).unwrap_or_default(),
        nqa: spot.is_some_and(|s| s.nqa),
        distance: None,
        proceeds: crate::text::py_sum(lines.iter().map(|line| line.proceeds)),
        lines,
    }
}

/// Puts the stations in a row: nearest known first, the largest proceeds where no distance is known.
fn order(mut left: Vec<Stop>, start: Option<&str>, distances: &Distances) -> Vec<Stop> {
    let by_proceeds = |a: &Stop, b: &Stop| {
        b.proceeds.total_cmp(&a.proceeds).then_with(|| id_order(&a.terminal, &b.terminal))
    };
    left.sort_by(by_proceeds);
    let mut result: Vec<Stop> = Vec::new();
    let mut here: Option<String> = start.map(str::to_string);
    while !left.is_empty() {
        let mut pick: Option<(usize, f64)> = None;
        if let Some(from) = &here {
            for (index, stop) in left.iter().enumerate() {
                if let Some(gm) = distances.between(from, &stop.terminal)
                    && pick.is_none_or(|(_, top)| gm < top)
                {
                    pick = Some((index, gm));
                }
            }
        }
        // `left` is sorted by proceeds, so a tie of distances keeps the larger proceeds.
        let (index, distance) = match pick {
            Some((index, gm)) => (index, Some(gm)),
            None => (0, None),
        };
        let mut stop = left.remove(index);
        stop.distance = distance;
        here = Some(stop.terminal.clone());
        result.push(stop);
    }
    result
}

/// The route over the stored prices and distances, fetching nothing.
pub fn plan(selling: &Selling, query: &Query, distances: &Distances) -> Route {
    let (picked, best_single, unsold) = choose(selling, query);
    let stops: Vec<Stop> = picked.into_iter().map(|(terminal, lines)| stop_of(selling, &terminal, lines)).collect();
    let stops = order(stops, query.start.as_deref(), distances);
    let proceeds = crate::text::py_sum(stops.iter().map(|stop| stop.proceeds));
    let known: Vec<f64> = stops.iter().filter_map(|stop| stop.distance).collect();
    // The first station has a leg only where a start is given.
    let skipped_first = usize::from(query.start.is_none());
    let unknown_legs = stops.iter().skip(skipped_first).filter(|stop| stop.distance.is_none()).count();
    Route {
        proceeds,
        best_single,
        unsold,
        distance: crate::text::py_sum(known.iter().copied()),
        unknown_legs,
        by_distance: stops.iter().skip(skipped_first).any(|stop| stop.distance.is_some()),
        stops,
    }
}

/// Which terminals of a route still lack their trips, so that their distances are not known.
fn missing_starts(routes: &Routes, terminals: &[String]) -> Vec<String> {
    terminals.iter().filter(|t| routes.trips(t).is_none()).cloned().collect()
}

/// The whole calculation for a job: chooses the stations, fetches the trips that are not stored
/// for them (nothing with the network switched off) and puts them in a row.
pub fn calculate(paths: &Paths, selling: &Selling, query: &Query, now: f64, fetch: Fetch<'_>) -> Route {
    let (picked, _, _) = choose(selling, query);
    let mut wanted: Vec<String> = picked.into_iter().map(|(terminal, _)| terminal).collect();
    if let Some(start) = &query.start {
        wanted.push(start.clone());
    }
    let mut book = Routes::load(paths);
    for terminal in missing_starts(&book, &wanted) {
        routes::fetch_start(paths, &mut book, &terminal, false, now, fetch);
    }
    plan(selling, query, &Distances::from_routes(&book))
}

#[cfg(test)]
mod tests;
