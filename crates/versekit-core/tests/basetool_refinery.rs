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

//! The area refinery of the sync against the stand-in of the Basetool in memory: a job read at the terminal becomes
//! an order once, a new reading moves its timer, booking its yield closes it without store lines, orders made in the
//! web come and go as jobs, a lost answer makes no second order, a wait after `Retry-After` sends nothing, and an
//! error of the area stops no other one. Everything in throwaway folders; nothing reaches the net.

use std::sync::Arc;

use versekit_core::basetool::connection::system_clock;
use versekit_core::basetool::exchange_double::{ExchangeDouble, REFINERIES};
use versekit_core::basetool::orgunits::OrgUnitsArea;
use versekit_core::basetool::refinery_area::RefineryArea;
use versekit_core::basetool::refinery_sync::rfc3339_text;
use versekit_core::basetool::scopes::{ROUTE_REFINERY, SETTING_REFINERY, SETTING_REFINERY_UNIT, SETTING_STOCK};
use versekit_core::basetool::stock_area::StockArea;
use versekit_core::basetool::sync::{Phase, Setup, SyncHandle};
use versekit_core::jsonio::{Map, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::refinery_jobs;
use versekit_core::refinery_scan::{ScannedJob, State};

const RIGHTS: [&str; 5] = [
    "exchange.refinery.read",
    "exchange.refinery.write",
    "exchange.orgunits.read",
    "exchange.stock.read",
    "exchange.stock.write",
];

struct Bench {
    dir: String,
    paths: Arc<Paths>,
    double: Arc<ExchangeDouble>,
    sync: SyncHandle,
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn bench(name: &str, switches: &[&str]) -> Bench {
    let dir = std::env::temp_dir().join(format!("versekit-refinery-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    let data = dir.to_string_lossy().replace('\\', "/");
    let game = format!("{data}/Spiel");
    std::fs::create_dir_all(&game).expect("game folder");
    let host = Host::new(
        Flavor::native(),
        &[(versekit_core::legacy::ENV_HOME, &data), (versekit_core::paths::ENV_INSTALL_DIR, &game)],
        &data,
    );
    let paths = Arc::new(Paths::new(host));
    for switch in switches {
        assert!(paths.set_setting(switch, Value::Bool(true)));
    }
    let double = Arc::new(ExchangeDouble::new());
    double.set_rights(&RIGHTS);
    double.set_units(&[("u1", "Staffel Mamba")]);
    let units = OrgUnitsArea::default();
    let clock = system_clock();
    let sync = SyncHandle::without_worker(Setup {
        conn: Arc::new(double.connection(clock.clone())),
        paths: paths.clone(),
        clock,
        version: "5.0.0".into(),
        own_account: Arc::new(|| Some("PlayerOne".to_string())),
        waker: Arc::new(|| {}),
        areas: vec![Box::new(StockArea), Box::new(units.clone()), Box::new(RefineryArea::new(units.units.clone()))],
    });
    Bench { dir: data, paths, double, sync }
}

fn now() -> f64 {
    system_clock()()
}

impl Bench {
    fn pass(&self) {
        self.sync.pass_now().expect("a pass");
    }

    fn counts(&self, area: &str) -> Map {
        let status = self.sync.status().get().1.expect("status");
        match status.counts.iter().find(|(k, _)| k == area).map(|(_, v)| v) {
            Some(Value::Object(counts)) => counts.clone(),
            other => panic!("no counts for {area}: {other:?}"),
        }
    }

    fn count(&self, area: &str, key: &str) -> i64 {
        match self.counts(area).iter().find(|(k, _)| k == key).map(|(_, v)| v) {
            Some(Value::Int(n)) => n.parse().unwrap_or(0),
            _ => 0,
        }
    }

    fn posts(&self) -> usize {
        self.double.seen().iter().filter(|l| l.starts_with(&format!("POST {ROUTE_REFINERY}"))).count()
    }

    /// The last refinery change set's operations.
    fn last_ops(&self) -> Vec<Map> {
        let bodies = self.double.bodies();
        let (_, body) = bodies.iter().rev().find(|(route, _)| route == ROUTE_REFINERY).expect("a change set");
        versekit_core::basetool::exchange_double::ops_map(body)
    }

    /// A job read at the terminal of the first refinery, ending in `seconds`.
    fn read(&self, seconds: i64) {
        let job = ScannedJob {
            text: "Gold 512 1200\nIron 300 300".into(),
            unsure: Vec::new(),
            materials: vec!["Gold".into(), "Iron".into()],
            total: "15".into(),
            state: State::Running,
            remaining: Some(seconds),
            cells: Vec::new(),
            station: REFINERIES[0].into(),
        };
        refinery_jobs::remember(&self.paths, &[job], now());
    }

    fn jobs(&self) -> Vec<Map> {
        refinery_jobs::load(&self.paths)
    }
}

fn field<'a>(map: &'a Map, key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

#[test]
fn a_read_job_becomes_one_order_moves_its_timer_and_closes_when_its_yield_is_booked() {
    let b = bench("eins", &[SETTING_REFINERY]);
    b.read(3600);
    b.pass();
    assert_eq!(b.count("refinery", "created"), 1, "{:?}", b.counts("refinery"));
    let orders = b.double.refinery_orders();
    assert_eq!(orders.len(), 1);
    let key = refinery_jobs::text(&b.jobs()[0], refinery_jobs::KEY);
    assert_eq!(orders[0].client_key.as_deref(), Some(key.as_str()), "the key stands in the file");
    assert_eq!(orders[0].goods, [("Gold".to_string(), 1200, 512), ("Iron".to_string(), 300, 300)]);
    let create = &b.last_ops()[0];
    assert_eq!(field(create, "orgUnit").and_then(|u| u.get("id")).and_then(Value::as_str), Some("u1"), "the only unit");

    // Nothing changed: no change set, one feed request.
    let posts = b.posts();
    b.pass();
    assert_eq!(b.posts(), posts, "nothing to send");

    // Read again with half an hour more: the timer moves, under the counter of the order.
    b.read(5400);
    b.pass();
    let update = &b.last_ops()[0];
    assert_eq!(field(update, "op").and_then(Value::as_str), Some("update"));
    assert_eq!(field(update, "versions").and_then(|v| v.get("gameData")), Some(&Value::int(0)));
    assert_eq!(b.double.refinery_orders().len(), 1, "no second order");

    // The yield booked into the own stock: the order is closed, nothing is stored over there.
    let read: Vec<String> = vec!["Gold".into(), "Iron".into()];
    assert!(refinery_jobs::mark_picked_up(&b.paths, &["gold".into(), "Iron".into()], Some((&read, "15")), now()));
    b.pass();
    let pickup = &b.last_ops()[0];
    assert_eq!(field(pickup, "op").and_then(Value::as_str), Some("pickup"));
    assert!(field(pickup, "lines").is_none(), "the stock sync books the stock");
    let order = &b.double.refinery_orders()[0];
    assert_eq!(order.status, "COMPLETED");
    assert!(order.picked_up);
    let posts = b.posts();
    b.pass();
    assert_eq!(b.posts(), posts, "a closed order is not touched again");
}

#[test]
fn orders_from_the_web_come_and_go_as_jobs() {
    let b = bench("web", &[SETTING_REFINERY]);
    let soon = rfc3339_text(now() + 600.0);
    let id = b.double.add_refinery_order(None, REFINERIES[1], "IN_PROGRESS", &soon, &[("Quantainium", 800, 700)]);
    b.pass();
    assert_eq!(b.count("refinery", "taken"), 1);
    let jobs = b.jobs();
    assert_eq!(refinery_jobs::text(&jobs[0], refinery_jobs::ORDER), id);
    assert_eq!(refinery_jobs::text(&jobs[0], "total"), "800");
    assert!((refinery_jobs::ends(&jobs[0]) - (now() + 600.0)).abs() < 5.0);
    assert_eq!(b.posts(), 0, "an order from over there is not sent back");
    // Completed in the web: the job leaves.
    b.double.change_refinery_order(&id, Some("COMPLETED"), None);
    b.pass();
    assert_eq!(b.count("refinery", "dropped"), 1);
    assert!(b.jobs().is_empty());
}

#[test]
fn a_lost_answer_makes_no_second_order() {
    let b = bench("verloren", &[SETTING_REFINERY]);
    b.read(3600);
    b.double.lose_next_answer(ROUTE_REFINERY);
    // The area fails alone; the pass goes through.
    b.pass();
    assert_eq!(b.count("refinery", "created"), 0);
    assert!(field(&b.counts("refinery"), "error").is_some(), "{:?}", b.counts("refinery"));
    assert_eq!(b.double.refinery_orders().len(), 1, "it came through all the same");
    // Read again meanwhile with another end: a key made anew from it would be another one.
    b.read(5400);
    b.pass();
    assert_eq!(b.double.refinery_orders().len(), 1, "the same key finds the order");
    let link = b.sync.state().expect("state");
    let links = field(&link, "refinery").and_then(|r| r.get("links")).cloned().expect("links");
    assert_eq!(links.as_object().map(Vec::len), Some(1), "{links:?}");
    // Linked now: the new end of the reading moves the timer of that one order, then nothing more goes out.
    b.pass();
    assert_eq!(field(&b.last_ops()[0], "op").and_then(Value::as_str), Some("update"));
    let posts = b.posts();
    b.pass();
    assert_eq!(b.posts(), posts, "nothing more");
    assert_eq!(b.double.refinery_orders().len(), 1);
}

#[test]
fn a_change_set_sent_again_after_a_lost_answer_goes_under_its_first_key() {
    let b = bench("schluessel", &[SETTING_REFINERY]);
    b.read(3600);
    b.double.lose_next_answer(ROUTE_REFINERY);
    b.pass();
    let keys = |b: &Bench| {
        let state = b.sync.state().expect("state");
        field(&state, "kept_keys").and_then(|k| k.get(ROUTE_REFINERY)).and_then(Value::as_object).map(Vec::len)
    };
    assert_eq!(keys(&b), Some(1), "kept until an answer comes");
    b.pass();
    assert!(b.double.seen().iter().any(|l| l.starts_with("POST") && l.ends_with("(replay)")), "the same key again");
    assert_eq!(b.double.refinery_orders().len(), 1);
    assert_eq!(keys(&b), None, "answered: gone");
}

#[test]
fn a_wait_after_retry_after_sends_nothing_and_an_error_stops_no_other_area() {
    let b = bench("warten", &[SETTING_REFINERY, SETTING_STOCK]);
    b.read(3600);
    b.double.fail(ROUTE_REFINERY, 503, "MAINTENANCE", Some(900), 1);
    b.pass();
    assert_eq!(b.count("refinery", "waiting"), 1, "{:?}", b.counts("refinery"));
    let posts = b.posts();
    b.pass();
    assert_eq!(b.posts(), posts, "nothing goes out while the wait stands");

    b.double.fail("/me/refinery-orders", 500, "INTERNAL", None, 1);
    b.pass();
    let status = b.sync.status().get().1.expect("status");
    assert_eq!(status.phase, Phase::Ok, "the pass went through");
    assert!(field(&b.counts("refinery"), "error").is_some());
    assert!(field(&b.counts("stock"), "error").is_none(), "the stock ran: {:?}", b.counts("stock"));
}

#[test]
fn without_a_unit_to_own_it_no_order_is_made() {
    let b = bench("einheit", &[SETTING_REFINERY]);
    b.double.set_units(&[("u1", "Staffel Mamba"), ("u2", "Kartell")]);
    b.read(3600);
    b.pass();
    b.pass();
    assert_eq!(b.count("refinery", "no_unit"), 1);
    assert_eq!(b.posts(), 0);
    assert!(b.paths.set_setting(SETTING_REFINERY_UNIT, Value::str("u2")));
    b.pass();
    let create = &b.last_ops()[0];
    assert_eq!(field(create, "orgUnit").and_then(|u| u.get("id")).and_then(Value::as_str), Some("u2"));
}
