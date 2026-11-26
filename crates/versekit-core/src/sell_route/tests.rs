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

use super::*;
use crate::routes::{Start, Trip};

/// What a good fetches at the terminals: `(terminal, price, level)`.
type Offers<'a> = &'a [(&'a str, f64, i64)];

/// A price list from `(terminal, system, nqa)` and `(good, offers)`.
fn list(spots: &[(&str, &str, bool)], goods: &[(&str, Offers<'_>)]) -> Selling {
    let terminals: Vec<String> = spots
        .iter()
        .map(|(id, system, nqa)| format!(r#""{id}":{{"o":"Place{id}","s":"{system}","n":"Name{id}","q":{nqa}}}"#))
        .collect();
    let wares: Vec<String> = goods
        .iter()
        .map(|(good, offers)| {
            let rows: Vec<String> = offers
                .iter()
                .map(|(t, price, level)| format!(r#"{{"t":"{t}","n":"Name{t}","p":{price},"d":0,"k":"","z":{level}}}"#))
                .collect();
            format!(r#""{good}":[{}]"#, rows.join(","))
        })
        .collect();
    let text = format!(r#"{{"terminals":{{{}}},"geholt":1,"waren":{{{}}}}}"#, terminals.join(","), wares.join(","));
    Selling::from_value(&crate::jsonio::loads(&text).expect("valid list"))
}

fn query(items: &[(&str, f64)]) -> Query {
    Query {
        items: items.iter().map(|(good, scu)| ((*good).to_string(), *scu)).collect(),
        max_stops: 4,
        ..Query::default()
    }
}

fn nowhere() -> Distances {
    Distances::default()
}

fn ids(route: &Route) -> Vec<&str> {
    route.stops.iter().map(|stop| stop.terminal.as_str()).collect()
}

fn book(pairs: &[(&str, &str, f64)]) -> Distances {
    let mut routes = Routes::default();
    for (a, b, gm) in pairs {
        let mut start = routes.starts.get(a).cloned().unwrap_or_default();
        start.trips.push(Trip { dest: (*b).to_string(), distance: *gm, ..Trip::default() });
        routes.starts.set(a, start);
    }
    Distances::from_routes(&routes)
}

#[test]
fn one_station_that_takes_everything_best_is_the_whole_route() {
    let selling = list(
        &[("1", "Stanton", false), ("2", "Stanton", false)],
        &[("Gold", &[("1", 100.0, 0), ("2", 90.0, 0)]), ("Iron", &[("1", 10.0, 0), ("2", 9.0, 0)])],
    );
    let route = plan(&selling, &query(&[("Gold", 10.0), ("Iron", 20.0)]), &nowhere());
    assert_eq!(ids(&route), ["1"]);
    assert_eq!(route.proceeds, 10.0 * 100.0 + 20.0 * 10.0);
    assert_eq!(route.best_single, route.proceeds);
    assert!(route.unsold.is_empty());
}

#[test]
fn goods_with_different_best_stations_make_a_route_of_two_and_beat_the_single_one() {
    let selling = list(
        &[("1", "Stanton", false), ("2", "Stanton", false)],
        &[("Gold", &[("1", 100.0, 0), ("2", 50.0, 0)]), ("Iron", &[("1", 5.0, 0), ("2", 40.0, 0)])],
    );
    let route = plan(&selling, &query(&[("Gold", 10.0), ("Iron", 10.0)]), &nowhere());
    assert_eq!(route.stops.len(), 2);
    assert_eq!(route.proceeds, 10.0 * 100.0 + 10.0 * 40.0);
    assert!(route.proceeds > route.best_single, "{route:?}");
    assert_eq!(route.best_single, 10.0 * 100.0 + 10.0 * 5.0);
}

#[test]
fn a_station_that_adds_nothing_is_not_taken() {
    let selling = list(
        &[("1", "Stanton", false), ("2", "Stanton", false)],
        &[("Gold", &[("1", 100.0, 0), ("2", 100.0, 0)]), ("Iron", &[("1", 10.0, 0), ("2", 10.0, 0)])],
    );
    assert_eq!(plan(&selling, &query(&[("Gold", 1.0), ("Iron", 1.0)]), &nowhere()).stops.len(), 1);
}

#[test]
fn the_number_of_stations_is_capped() {
    let selling = list(
        &[("1", "S", false), ("2", "S", false), ("3", "S", false)],
        &[("A", &[("1", 10.0, 0)]), ("B", &[("2", 10.0, 0)]), ("C", &[("3", 10.0, 0)])],
    );
    let mut asked = query(&[("A", 1.0), ("B", 1.0), ("C", 1.0)]);
    asked.max_stops = 2;
    let route = plan(&selling, &asked, &nowhere());
    assert_eq!(route.stops.len(), 2);
    assert_eq!(route.unsold.len(), 1, "the good of the third station stays unsold");
    asked.max_stops = 0;
    assert_eq!(plan(&selling, &asked, &nowhere()).stops.len(), 1, "at least one station");
}

#[test]
fn a_tie_goes_to_the_lower_terminal_number_whatever_the_order_of_the_list() {
    let offers_up: &[(&str, f64, i64)] = &[("9", 50.0, 0), ("10", 50.0, 0), ("100", 50.0, 0)];
    let offers_down: &[(&str, f64, i64)] = &[("100", 50.0, 0), ("10", 50.0, 0), ("9", 50.0, 0)];
    let spots = [("9", "S", false), ("10", "S", false), ("100", "S", false)];
    for offers in [offers_up, offers_down] {
        let selling = list(&spots, &[("Gold", offers)]);
        let route = plan(&selling, &query(&[("Gold", 5.0)]), &nowhere());
        assert_eq!(ids(&route), ["9"], "the shortest number wins the tie");
    }
}

#[test]
fn a_good_goes_to_the_better_price_and_a_tie_to_the_lower_number() {
    let selling = list(
        &[("2", "S", false), ("1", "S", false)],
        &[("Gold", &[("2", 60.0, 0), ("1", 60.0, 0)]), ("Iron", &[("2", 99.0, 0)])],
    );
    let route = plan(&selling, &query(&[("Gold", 1.0), ("Iron", 1.0)]), &nowhere());
    let gold = route.stops.iter().find(|stop| stop.lines.iter().any(|line| line.good == "Gold")).unwrap();
    assert_eq!(gold.terminal, "2", "station 2 is taken for the iron, so the gold goes with it");
}

#[test]
fn only_the_chosen_system_counts() {
    let selling = list(
        &[("1", "Stanton", false), ("2", "Pyro", false)],
        &[("Gold", &[("1", 10.0, 0), ("2", 99.0, 0)])],
    );
    let mut asked = query(&[("Gold", 1.0)]);
    asked.system = "Stanton".to_string();
    assert_eq!(ids(&plan(&selling, &asked, &nowhere())), ["1"]);
    asked.system = "Nyx".to_string();
    assert!(plan(&selling, &asked, &nowhere()).stops.is_empty());
    asked.system.clear();
    assert_eq!(ids(&plan(&selling, &asked, &nowhere())), ["2"]);
}

#[test]
fn no_questions_asked_leaves_out_the_others() {
    let selling =
        list(&[("1", "S", false), ("2", "S", true)], &[("Gold", &[("1", 99.0, 0), ("2", 10.0, 0)])]);
    let mut asked = query(&[("Gold", 1.0)]);
    asked.nqa_only = true;
    assert_eq!(ids(&plan(&selling, &asked, &nowhere())), ["2"]);
}

#[test]
fn a_station_the_list_calls_full_is_left_out_unless_asked_for() {
    let selling = list(&[("1", "S", false), ("2", "S", false)], &[("Gold", &[("1", 99.0, 6), ("2", 10.0, 5)])]);
    let mut asked = query(&[("Gold", 1.0)]);
    let route = plan(&selling, &asked, &nowhere());
    assert_eq!(ids(&route), ["2"]);
    assert_eq!(route.stops[0].lines[0].fill.map(|(key, _)| key), Some("s_vk_fuellt"), "filling up is shown");
    asked.include_full = true;
    assert_eq!(ids(&plan(&selling, &asked, &nowhere())), ["1"]);
}

#[test]
fn a_station_the_player_calls_full_is_left_out() {
    let selling = list(&[("1", "S", false), ("2", "S", false)], &[("Gold", &[("1", 99.0, 0), ("2", 10.0, 0)])]);
    let mut asked = query(&[("Gold", 1.0)]);
    asked.blocked = vec!["1".to_string()];
    assert_eq!(ids(&plan(&selling, &asked, &nowhere())), ["2"]);
    asked.include_full = true;
    assert_eq!(ids(&plan(&selling, &asked, &nowhere())), ["2"], "the player's word is not undone by the switch");
}

#[test]
fn amounts_of_zero_and_unknown_goods_are_ignored_and_a_good_without_station_stays_unsold() {
    let selling = list(&[("1", "S", false)], &[("Gold", &[("1", 10.0, 0)]), ("Iron", &[("1", 10.0, 6)])]);
    let route = plan(&selling, &query(&[("Gold", 0.0), ("Nothing", 5.0), ("Iron", 3.0)]), &nowhere());
    assert!(route.stops.is_empty());
    assert_eq!(route.unsold, [("Iron".to_string(), 3.0)]);
    assert_eq!(route.proceeds, 0.0);
}

#[test]
fn the_order_goes_to_the_nearest_known_station_from_the_start() {
    let selling = list(
        &[("1", "S", false), ("2", "S", false), ("3", "S", false)],
        &[("A", &[("1", 10.0, 0)]), ("B", &[("2", 11.0, 0)]), ("C", &[("3", 12.0, 0)])],
    );
    let mut asked = query(&[("A", 1.0), ("B", 1.0), ("C", 1.0)]);
    asked.start = Some("9".to_string());
    let far = book(&[("9", "1", 5.0), ("1", "3", 2.0), ("3", "2", 1.0)]);
    let route = plan(&selling, &asked, &far);
    assert_eq!(ids(&route), ["1", "3", "2"]);
    assert_eq!(route.stops[0].distance, Some(5.0));
    assert_eq!(route.distance, 5.0 + 2.0 + 1.0);
    assert_eq!(route.unknown_legs, 0);
    assert!(route.by_distance);
}

#[test]
fn where_no_distance_is_known_the_largest_proceeds_go_first_and_the_legs_are_counted() {
    let selling = list(
        &[("1", "S", false), ("2", "S", false), ("3", "S", false)],
        &[("A", &[("1", 10.0, 0)]), ("B", &[("2", 30.0, 0)]), ("C", &[("3", 20.0, 0)])],
    );
    let route = plan(&selling, &query(&[("A", 1.0), ("B", 1.0), ("C", 1.0)]), &nowhere());
    assert_eq!(ids(&route), ["2", "3", "1"]);
    assert!(route.stops.iter().all(|stop| stop.distance.is_none()));
    assert_eq!(route.unknown_legs, 2, "the first station has no leg without a start");
    assert!(!route.by_distance);
}

#[test]
fn a_pair_counts_in_both_directions_and_the_same_terminal_is_no_distance_away() {
    let known = book(&[("1", "2", 4.5)]);
    assert_eq!(known.between("2", "1"), Some(4.5));
    assert_eq!(known.between("1", "1"), Some(0.0));
    assert_eq!(known.between("1", "3"), None);
}

#[test]
fn without_the_network_the_route_is_still_calculated_and_nothing_is_asked() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = std::env::temp_dir().join(format!("versekit-verkaufsroute-{}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let data = dir.to_string_lossy().into_owned();
    let vars = [("VERSEKIT_HOME", data.as_str()), (crate::envvars::ENV_NO_NET, "1")];
    let paths = Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, "/"));
    let asked = AtomicUsize::new(0);
    let fetch = |_: &str| -> Result<Vec<u8>, crate::scmdb::FetchError> {
        asked.fetch_add(1, Ordering::Relaxed);
        Err(crate::scmdb::FetchError::Missing)
    };
    let selling = list(
        &[("1", "S", false), ("2", "S", false)],
        &[("A", &[("1", 10.0, 0)]), ("B", &[("2", 30.0, 0)])],
    );
    let route = calculate(&paths, &selling, &query(&[("A", 1.0), ("B", 1.0)]), 0.0, &fetch);
    assert_eq!(route.stops.len(), 2);
    assert_eq!(asked.load(Ordering::Relaxed), 0, "no question with the network switched off");
    assert!(route.stops.iter().all(|stop| stop.distance.is_none()));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_trips_of_the_chosen_stations_are_fetched_when_the_network_is_on() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let dir = std::env::temp_dir().join(format!("versekit-verkaufsroute-an-{}", crate::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let data = dir.to_string_lossy().into_owned();
    let vars = [("VERSEKIT_HOME", data.as_str()), (crate::envvars::ENV_NO_NET, "0")];
    let paths = Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, "/"));
    let asked = AtomicUsize::new(0);
    let fetch = |_: &str| -> Result<Vec<u8>, crate::scmdb::FetchError> {
        asked.fetch_add(1, Ordering::Relaxed);
        Err(crate::scmdb::FetchError::Missing)
    };
    let selling = list(
        &[("1", "S", false), ("2", "S", false)],
        &[("A", &[("1", 10.0, 0)]), ("B", &[("2", 30.0, 0)])],
    );
    let route = calculate(&paths, &selling, &query(&[("A", 1.0), ("B", 1.0)]), 0.0, &fetch);
    assert_eq!(route.stops.len(), 2, "a failed fetch does not stop the route");
    assert_eq!(asked.load(Ordering::Relaxed), 2, "one question per chosen station");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stored_trips_are_not_fetched_again() {
    let mut routes = Routes::default();
    routes.starts.set("1", Start { fetched: 0.0, trips: Vec::new() });
    assert_eq!(missing_starts(&routes, &["1".to_string(), "2".to_string()]), ["2"]);
}
