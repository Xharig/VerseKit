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

//! The sums of the statistics pages against the numbers of 4.12 on real data.
//!
//! Run by hand, it needs a copy of a data folder and the numbers of 4.12
//! worked out on the same copy (never the live data):
//!
//! ```text
//! python3 -I tools/stats_reference.py <repo> <py.json> <flat copy of the data>
//! STATS_PARITY_DATA=<flat copy> STATS_PARITY_PY=<py.json> \
//!     cargo test -p versekit-core --test stats_parity -- --ignored
//! ```

use std::collections::HashMap;

use serde_json::Value;
use versekit_core::legacy;
use versekit_core::mission_log;
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::stats_report as report;
use versekit_core::{playtime, stats_report};

fn pairs(value: &Value) -> Vec<(String, i64)> {
    value
        .as_array()
        .expect("list")
        .iter()
        .map(|pair| (pair[0].as_str().expect("name").to_string(), pair[1].as_i64().expect("count")))
        .collect()
}

#[test]
#[ignore = "needs a copy of real data and the numbers of 4.12 (see the module doc)"]
fn the_sums_equal_the_numbers_of_the_python_version() {
    let data = std::env::var("STATS_PARITY_DATA").expect("STATS_PARITY_DATA");
    let py: Value = serde_json::from_str(
        &std::fs::read_to_string(std::env::var("STATS_PARITY_PY").expect("STATS_PARITY_PY")).expect("file"),
    )
    .expect("json");
    let host = Host::new(Flavor::native(), &[(legacy::ENV_HOME, &data)], &data);
    let paths = Paths::new(host);
    let own = Some("Xharig");
    let sessions = stats_report::load_sessions(&paths);
    let spans = playtime::spans(&paths);

    let summary = report::summary(&sessions, &spans, None, own);
    // Two zeros equal each other: the copy must hold something of the account.
    assert!(summary.recorded > 0 && summary.playtime > 0, "the copy holds nothing of the account {own:?}");
    let want = &py["summary"];
    let n = |key: &str| want[key].as_i64().unwrap_or_else(|| panic!("{key}"));
    assert_eq!(summary.playtime, n("spielzeit"));
    assert_eq!(summary.sessions, n("sitzungen"));
    assert_eq!(summary.average, n("schnitt"));
    assert_eq!(summary.longest, n("laengste"));
    assert_eq!(summary.contracts, n("auftraege"));
    assert_eq!(summary.failed, n("fehlgeschlagen"));
    assert_eq!(summary.jumps, n("spruenge"));
    assert_eq!(summary.deaths, n("tode"));
    assert_eq!(summary.last_death, want["letzter_tod"].as_i64());
    assert_eq!(summary.lost, n("verloren"));
    assert_eq!(summary.most_lost.as_deref(), want["meist_verloren"].as_str());
    assert_eq!(summary.recorded, n("erfasst"));
    assert_eq!(summary.since, want["seit"].as_i64());
    for (day, row) in want["waermekarte"].as_array().expect("grid").iter().enumerate() {
        let row: Vec<i64> = row.as_array().expect("row").iter().map(|v| v.as_i64().expect("n")).collect();
        assert_eq!(summary.heat[day].to_vec(), row, "heat map, day {day}");
    }

    let ships = report::ships(&sessions, own);
    let want = &py["ships"];
    assert_eq!(ships.ships as i64, want["schiffe"].as_i64().expect("n"));
    assert_eq!(ships.sessions_with_ship, want["sitzungen_mit_schiff"].as_i64().expect("n"));
    assert_eq!(ships.used, pairs(&want["genutzt"]));
    assert_eq!(ships.lost, pairs(&want["verloren"]));
    assert_eq!(ships.lost_total, want["verloren_gesamt"].as_i64().expect("n"));
    for slot in &ships.weapons {
        let [list, total] = &want["waffen"][slot.slot].as_array().expect("slot")[..] else { panic!("slot") };
        assert_eq!(slot.top, pairs(list), "weapons {}", slot.slot);
        assert_eq!(slot.total, total.as_i64().expect("n"));
    }

    let log = mission_log::load(&paths);
    let missions = report::missions(&sessions, &log, own);
    let want = &py["missions"];
    assert_eq!(missions.completed, want["abgeschlossen"].as_i64().expect("n"));
    assert_eq!(missions.failed, want["fehlgeschlagen"].as_i64().expect("n"));
    assert_eq!(missions.abandoned, want["abgebrochen"].as_i64().expect("n"));
    assert_eq!(missions.frequent, pairs(&want["haeufigste"]));
    let latest: Vec<(String, String, String)> = want["letzte"]
        .as_array()
        .expect("list")
        .iter()
        .map(|row| {
            let text = |i: usize| row[i].as_str().expect("text").to_string();
            (text(0), text(1), text(2))
        })
        .collect();
    let got: Vec<(String, String, String)> =
        missions.latest.iter().map(|r| (r.name.clone(), r.stamp.clone(), r.state.clone())).collect();
    assert_eq!(got, latest);

    let quantum = report::quantum(&sessions, own);
    let want = &py["quantum"];
    assert_eq!(quantum.jumps, want["spruenge"].as_i64().expect("n"));
    assert_eq!(quantum.selections, want["zielwahlen"].as_i64().expect("n"));
    assert_eq!(quantum.targets, pairs(&want["ziele"]));
    assert_eq!(quantum.starts, pairs(&want["starts"]));

    let stability = report::stability(&sessions, own);
    let want = &py["stability"];
    assert_eq!(stability.sessions, want["sitzungen"].as_i64().expect("n"));
    assert_eq!(stability.clean, want["sauber"].as_i64().expect("n"));
    assert_eq!(stability.crashed, want["absturz"].as_i64().expect("n"));
    assert_eq!(stability.open, want["ohne_ende"].as_i64().expect("n"));
    assert_eq!(stability.reasons, pairs(&want["gruende"]));
    assert_eq!(stability.drops, want["abbrueche"].as_i64().expect("n"));
    let last: Vec<(i64, i64, String, i64)> = want["letzte"]
        .as_array()
        .expect("list")
        .iter()
        .map(|r| {
            (
                r[0].as_i64().expect("n"),
                r[1].as_i64().expect("n"),
                r[2].as_str().expect("s").to_string(),
                r[3].as_i64().expect("n"),
            )
        })
        .collect();
    let got: Vec<(i64, i64, String, i64)> = stability
        .latest
        .iter()
        .map(|l| {
            let end = match l.end {
                report::End::Clean => "sauber",
                report::End::Crash => "absturz",
                report::End::Open => "offen",
            };
            (l.start, l.duration, end.to_string(), l.drops)
        })
        .collect();
    assert_eq!(got, last);

    let names = report::saved_names(&paths, report::PLACE_FILE);
    let want = py["places"].as_object().expect("places");
    for (key, expected) in want {
        let place = report::place_parts(key, &names);
        let text_key = expected[0].as_str();
        assert_eq!((place.text_key, place.name.as_str()), (text_key, expected[1].as_str().expect("name")), "{key}");
    }
    let vehicles = report::saved_names(&paths, report::NAME_FILE);
    let shown: HashMap<String, String> = py["display"]
        .as_object()
        .expect("display")
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().expect("s").to_string()))
        .collect();
    for (class, name) in shown {
        assert_eq!(report::display_name(&class, true, &vehicles), name, "{class}");
    }

    // The export: the same numbers, the same rows.
    let value = report::export_value(&sessions, &spans, None, own, "x", 0);
    let text = versekit_core::jsonio::dumps(&value, Some(1), false);
    let ours: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(ours["gesamt"], py["export"]["gesamt"]);
    assert_eq!(ours["waermekarte"], py["export"]["waermekarte"]);
    assert_eq!(ours["sitzungen"], py["export"]["sitzungen"]);

    let state = report::evaluation(&paths, None);
    let want = &py["state"];
    assert_eq!(state.sessions as i64, want["sitzungen"].as_i64().expect("n"));
    assert_eq!(state.files as i64, want["dateien"].as_i64().expect("n"));
    assert_eq!(state.size as i64, want["groesse"].as_i64().expect("n"));
}
