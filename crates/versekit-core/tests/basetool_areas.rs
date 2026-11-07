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

//! The three areas blueprints, stock and hangar of the sync against a stand-in
//! of the Basetool in memory (`ExchangeDouble`): the first sync, the steady
//! state, removals on one side, conflicts and their decisions, a held mass
//! change, maintenance and refused tokens. Everything is written into throwaway
//! folders; nothing reaches the net.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use versekit_core::basetool::blueprint_area::{BlueprintArea, FileBlueprintHost};
use versekit_core::basetool::connection::system_clock;
use versekit_core::basetool::decisions;
use versekit_core::basetool::exchange_double::{CONFIRM_URL, ExchangeDouble};
use versekit_core::basetool::orgunits::OrgUnitsArea;
use versekit_core::basetool::scopes::{
    ROUTE_BLUEPRINTS, ROUTE_SHIPS, ROUTE_STOCK, SETTING_BLUEPRINTS, SETTING_SHIPS, SETTING_STOCK,
};
use versekit_core::basetool::ship_area::ShipArea;
use versekit_core::basetool::stock_area::StockArea;
use versekit_core::basetool::sync::{Phase, Setup, SyncHandle};
use versekit_core::collection::{self, Collection, NoCatalog, NoExports};
use versekit_core::jsonio::{Map, Value, map_set};
use versekit_core::paths::{Host, Paths};
use versekit_core::phrases::{BackRoute, NoArchive};
use versekit_core::pypath::Flavor;

struct Bench {
    dir: String,
    paths: Arc<Paths>,
    double: Arc<ExchangeDouble>,
    sync: SyncHandle,
    /// Seconds the clock of the sync runs ahead.
    skew: Arc<AtomicU64>,
}

fn bench(name: &str, switches: &[&str]) -> Bench {
    bench_with_names(name, switches, None)
}

fn bench_with_names(name: &str, switches: &[&str], on_names: Option<Arc<dyn Fn() + Send + Sync>>) -> Bench {
    build(name, switches, on_names, None)
}

/// A bench with the write guard of a test on (`VERSEKIT_BASETOOL_WRITE_MAX`): `max` changes for the whole run of
/// the bench.
fn guarded(name: &str, switches: &[&str], max: u32) -> Bench {
    build(name, switches, None, Some(max))
}

fn build(name: &str, switches: &[&str], on_names: Option<Arc<dyn Fn() + Send + Sync>>, guard: Option<u32>) -> Bench {
    let dir = std::env::temp_dir().join(format!("versekit-areas-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    let data = dir.to_string_lossy().replace('\\', "/");
    // An empty game folder of its own: the logs of the machine must not be read by a test.
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
    let units = OrgUnitsArea::default();
    let host = Arc::new(FileBlueprintHost::new(paths.clone(), Arc::new(NoCatalog)));
    let skew = Arc::new(AtomicU64::new(0));
    let clock: versekit_core::basetool::connection::Clock = {
        let (base, skew) = (system_clock(), skew.clone());
        Arc::new(move || base() + skew.load(Ordering::SeqCst) as f64)
    };
    let mut ships = ShipArea::new(units.units.clone());
    ships.on_names = on_names;
    let sync = SyncHandle::without_worker(Setup {
        conn: Arc::new(double.connection(clock.clone()).with_write_guard(guard)),
        paths: paths.clone(),
        clock,
        version: "5.0.0".into(),
        own_account: Arc::new(|| Some("PlayerOne".to_string())),
        waker: Arc::new(|| {}),
        areas: vec![Box::new(BlueprintArea::new(host)), Box::new(StockArea), Box::new(units.clone()), Box::new(ships)],
    });
    Bench { dir: data, paths, double, sync, skew }
}

impl Bench {
    fn pass(&self) {
        self.sync.pass_now().expect("a pass");
    }

    fn state(&self) -> Map {
        self.sync.state().expect("a state")
    }

    fn counts(&self, area: &str) -> Map {
        let status = self.sync.status().get().1.expect("status");
        match status.counts.iter().find(|(k, _)| k == area).map(|(_, v)| v) {
            Some(Value::Object(counts)) => counts.clone(),
            other => panic!("no counts for {area}: {other:?}"),
        }
    }

    /// The collection as the program keeps it.
    fn with_collection<R>(&self, work: impl FnOnce(&Collection) -> R) -> R {
        let names = BackRoute { paths: &self.paths, archive: &NoArchive };
        let collection = Collection {
            paths: &self.paths,
            catalog: &NoCatalog,
            names: &names,
            exports: &NoExports,
            clock: collection::now_text,
        };
        work(&collection)
    }

    fn add_local(&self, name: &str, source: &str) {
        self.with_collection(|c| {
            let mut data = c.load();
            c.add(&mut data, name, source, None);
            assert!(c.save(&mut data));
        });
    }

    fn remove_local(&self, name: &str) {
        self.with_collection(|c| {
            let mut data = c.load();
            assert!(collection::remove(&mut data, name));
            assert!(c.save(&mut data));
        });
    }

    fn local_names(&self) -> Vec<String> {
        let mut names = self.with_collection(|c| collection::names(&c.load()));
        names.sort();
        names
    }

    fn local_source(&self, name: &str) -> Option<String> {
        self.with_collection(|c| {
            let data = c.load();
            collection::blueprints(&data)
                .iter()
                .find(|(k, _)| *k == versekit_core::paths::name_key(name))
                .and_then(|(_, entry)| entry.get("quelle").and_then(Value::as_str).map(str::to_string))
        })
    }
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn count(counts: &Map, name: &str) -> i64 {
    match counts.iter().find(|(k, _)| k == name).map(|(_, v)| v) {
        Some(Value::Int(n)) => n.parse().unwrap_or(-1),
        other => panic!("{name}: {other:?}"),
    }
}

fn text_list(state: &Map, key: &str) -> Vec<String> {
    match state.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

fn object_keys(state: &Map, key: &str) -> Vec<String> {
    match state.iter().find(|(k, _)| k == key).map(|(_, v)| v) {
        Some(Value::Object(items)) => items.iter().map(|(k, _)| k.clone()).collect(),
        _ => Vec::new(),
    }
}

fn blueprint_key(name: &str) -> String {
    versekit_core::paths::name_key(name)
}

fn sent(double: &ExchangeDouble, route: &str) -> usize {
    double.ops_sent(route).iter().map(Vec::len).sum()
}

// ------------------------------------------------------------- blueprints

#[test]
fn the_first_sync_adds_in_both_directions_and_the_next_has_nothing_to_do() {
    let b = bench("bp-first", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "hand");
    b.add_local("Bastion Helm", "log");
    b.double.add_blueprint("Bastion Helm", false, Some("2026-10-21T12:00:00Z"));
    b.double.add_blueprint("Corsair Legs", false, Some("2026-10-20T08:30:00Z"));
    b.pass();
    // The one only here goes out, the one only there comes in as "basetool".
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle", "Bastion Helm", "Corsair Legs"]);
    assert_eq!(b.local_names(), ["Aether Rifle", "Bastion Helm", "Corsair Legs"]);
    assert_eq!(b.local_source("Corsair Legs").as_deref(), Some("basetool"));
    assert_eq!(b.local_source("Bastion Helm").as_deref(), Some("log"), "what the log proves stays a log find");
    let counts = b.counts("blueprints");
    assert_eq!((count(&counts, "added"), count(&counts, "local_added"), count(&counts, "removed")), (1, 1, 0));
    let ops = &b.double.ops_sent(ROUTE_BLUEPRINTS)[0];
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].get("ref").and_then(|r| r.get("bt")).and_then(Value::as_str), Some("bt-aether-rifle"));
    assert_eq!(ops[0].get("provenance").and_then(|p| p.get("source")).and_then(Value::as_str), Some("manual"));
    assert_eq!(ops[0].get("op").and_then(Value::as_str), Some("add"));
    // The state knows the links and the stand.
    let state = b.state();
    assert_eq!(object_keys(&state, "links").len(), 3);
    assert_eq!(object_keys(&state, "baseline").len(), 3);
    assert!(object_keys(&state, "conflicts").is_empty());
    // Again: nothing goes out, nothing changes.
    let before = b.double.bodies().len();
    b.pass();
    assert_eq!(b.double.bodies().len(), before, "a steady state sends nothing");
    let counts = b.counts("blueprints");
    assert_eq!((count(&counts, "added"), count(&counts, "local_added")), (0, 0));
}

#[test]
fn what_is_removed_in_the_browser_is_a_conflict_and_comes_back_only_with_consent() {
    let b = bench("bp-conflict", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "hand");
    b.pass();
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle"]);
    b.double.remove_blueprint_elsewhere("Aether Rifle");
    let sent_before = sent(&b.double, ROUTE_BLUEPRINTS);
    b.pass();
    // Not sent again: it is a conflict for the player.
    assert_eq!(sent(&b.double, ROUTE_BLUEPRINTS), sent_before);
    assert_eq!(object_keys(&b.state(), "conflicts"), [blueprint_key("Aether Rifle")]);
    assert_eq!(b.local_names(), ["Aether Rifle"], "it stays here, the game log proves it");
    b.pass();
    assert_eq!(sent(&b.double, ROUTE_BLUEPRINTS), sent_before, "and it stays unsent from pass to pass");
    // The player says: bring it back.
    assert!(b.sync.decide(decisions::override_conflicts(vec![blueprint_key("Aether Rifle")])));
    b.pass();
    let ops = b.double.ops_sent(ROUTE_BLUEPRINTS);
    let last = ops.last().expect("a send").first().expect("an op");
    assert_eq!(last.get("override"), Some(&Value::Bool(true)));
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle"]);
    assert!(object_keys(&b.state(), "conflicts").is_empty());
    assert!(text_list(&b.state(), "overrides").is_empty(), "the release is used up");
}

#[test]
fn a_blueprint_removed_here_is_asked_about_and_removed_over_there_only_when_released() {
    let b = bench("bp-removed", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "hand");
    b.add_local("Bastion Helm", "hand");
    b.pass();
    b.remove_local("Bastion Helm");
    b.pass();
    // Nothing is deleted because it is missing.
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle", "Bastion Helm"]);
    assert_eq!(object_keys(&b.state(), "local_removed"), [blueprint_key("Bastion Helm")]);
    // The player releases the removal.
    assert!(b.sync.decide(decisions::release_removals(vec![blueprint_key("Bastion Helm")])));
    b.pass();
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle"]);
    let state = b.state();
    assert!(text_list(&state, "pending_removals").is_empty());
    assert!(text_list(&state, "own_removed_keys").contains(&"bt-bastion-helm".to_string()));
    assert!(object_keys(&state, "local_removed").is_empty());
    assert_eq!(count(&b.counts("blueprints"), "removed"), 1);
    // Our own deletion mark is no conflict at the next pass.
    b.pass();
    assert!(object_keys(&b.state(), "conflicts").is_empty());
    assert_eq!(b.local_names(), ["Aether Rifle"]);
}

#[test]
fn a_blueprint_that_came_only_from_the_basetool_goes_when_it_is_removed_there() {
    let b = bench("bp-basetool-only", &[SETTING_BLUEPRINTS]);
    b.double.add_blueprint("Corsair Legs", false, None);
    b.pass();
    assert_eq!(b.local_source("Corsair Legs").as_deref(), Some("basetool"));
    b.double.remove_blueprint_elsewhere("Corsair Legs");
    b.pass();
    assert!(b.local_names().is_empty(), "no proof in the game: the removal counts here too");
    assert!(object_keys(&b.state(), "conflicts").is_empty());
    assert_eq!(count(&b.counts("blueprints"), "local_removed"), 1);
}

#[test]
fn start_blueprints_never_go_out_and_default_ones_are_never_removed_over_there() {
    let b = bench("bp-start", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "start");
    b.add_local("Bastion Helm", "hand");
    b.double.add_blueprint("Dust Gloves", true, None);
    b.pass();
    assert_eq!(b.double.blueprint_names(), ["Bastion Helm", "Dust Gloves"], "the start blueprint stays here");
    // Released for removal, but the Basetool gives it to everybody: it is not sent.
    b.remove_local("Dust Gloves");
    assert!(b.sync.decide(decisions::release_removals(vec![blueprint_key("Dust Gloves")])));
    b.pass();
    assert_eq!(b.double.blueprint_names(), ["Bastion Helm", "Dust Gloves"]);
    // Not even asked: the double would refuse the removal itself, so only the ops tell that the plan is right.
    let removals = b
        .double
        .ops_sent(ROUTE_BLUEPRINTS)
        .iter()
        .flatten()
        .filter(|op| op.get("op").and_then(Value::as_str) == Some("remove"))
        .count();
    assert_eq!(removals, 0, "a start blueprint is never sent for removal");
}

/// Sets or lifts the read-only mark of a file (the write of the sync into it then fails on Windows).
#[cfg(windows)]
fn read_only(path: &str, on: bool) {
    let mut permissions = std::fs::metadata(path).expect("file").permissions();
    permissions.set_readonly(on);
    std::fs::set_permissions(path, permissions).expect("permissions");
}

#[cfg(windows)]
#[test]
fn a_collection_that_cannot_be_written_ends_the_pass_and_the_next_one_takes_the_blueprint_over() {
    let b = bench("bp-write", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "hand");
    b.pass();
    let state_before = b.state();
    b.double.add_blueprint("Bastion Helm", false, Some("2026-10-21T12:00:00Z"));
    let file = b.paths.app_file(versekit_core::collection::FILE_NAME);
    let marked = file.clone();
    // The player's collection is locked just before the pass reads the stand over there.
    b.double.on_next_request("/me/blueprints", move || read_only(&marked, true));
    assert!(b.sync.pass_now().is_err(), "the write failed, the pass ends");
    read_only(&file, false);
    assert_eq!(b.local_names(), ["Aether Rifle"], "nothing was taken over");
    assert_eq!(b.state(), state_before, "the state of the failed pass is thrown away");
    b.pass();
    assert_eq!(b.local_names(), ["Aether Rifle", "Bastion Helm"], "the next pass takes it over");
}

#[test]
fn an_expired_cursor_takes_the_stand_anew_and_counts_what_is_missing_as_removed_elsewhere() {
    let b = bench("bp-expired", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "hand");
    b.double.add_blueprint("Bastion Helm", false, None);
    b.pass();
    b.double.remove_blueprint_elsewhere("Aether Rifle");
    b.double.expire_cursor();
    b.pass();
    // No first sync: the last stand is compared with the new one.
    assert_eq!(object_keys(&b.state(), "conflicts"), [blueprint_key("Aether Rifle")]);
    assert_eq!(sent(&b.double, ROUTE_BLUEPRINTS), 1, "only the first send exists");
}

#[test]
fn a_held_mass_change_waits_for_the_browser_and_goes_out_after_the_release() {
    let b = bench("bp-held", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "hand");
    b.double.hold_next(ROUTE_BLUEPRINTS);
    b.pass();
    assert_eq!(count(&b.counts("blueprints"), "paused"), 1);
    assert_eq!(b.double.blueprint_names(), Vec::<String>::new(), "nothing was taken");
    assert_eq!(b.sync.confirm_link(ROUTE_BLUEPRINTS).as_deref(), Some(CONFIRM_URL));
    // While it is held nothing goes out again.
    let tries = b.double.ops_sent(ROUTE_BLUEPRINTS).len();
    b.pass();
    assert_eq!(b.double.ops_sent(ROUTE_BLUEPRINTS).len(), tries);
    assert_eq!(count(&b.counts("blueprints"), "paused"), 1);
    // After the time is over the same set does not go out by itself ...
    b.skew.fetch_add(2000, Ordering::SeqCst);
    b.pass();
    assert_eq!(b.double.ops_sent(ROUTE_BLUEPRINTS).len(), tries, "an expired hold of the same set stays");
    assert_eq!(b.sync.confirm_link(ROUTE_BLUEPRINTS), None, "and its link is no longer valid");
    // ... but a sync button lets it go once.
    b.sync.release_held(&[ROUTE_BLUEPRINTS]);
    b.pass();
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle"]);
    assert_eq!(count(&b.counts("blueprints"), "added"), 1);
}

// ------------------------------------------------------- the write guard of a test

/// The three areas with something to send and something to fetch each.
fn seed_all_areas(b: &Bench) {
    b.add_local("Aether Rifle", "hand");
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary")], &[]);
    b.save_hangar(vec![ship("Cutlass Black", "Drake", "pledge")]);
    b.double.add_blueprint("Corsair Legs", false, Some("2026-10-20T08:30:00Z"));
    b.double.set_lot("Copper", "Area18", 100, false, 4.0);
    b.double.add_ship("Prospector", None, months(0), "");
}

const ALL_AREAS: [&str; 3] = [SETTING_BLUEPRINTS, SETTING_STOCK, SETTING_SHIPS];

#[test]
fn a_guard_of_zero_fetches_in_every_area_and_sends_from_none() {
    // Counter-check: the same data without a guard goes out in all three areas.
    let open = bench("guard-offen", &ALL_AREAS);
    seed_all_areas(&open);
    open.pass();
    for route in [ROUTE_BLUEPRINTS, ROUTE_STOCK, ROUTE_SHIPS] {
        assert!(!open.double.ops_sent(route).is_empty(), "without a guard {route} goes out");
    }

    let b = guarded("guard-null", &ALL_AREAS, 0);
    seed_all_areas(&b);
    // The pass is no failure and does not stop at the first area: every one fetches and keeps its send back.
    b.pass();
    for route in [ROUTE_BLUEPRINTS, ROUTE_STOCK, ROUTE_SHIPS] {
        assert!(b.double.ops_sent(route).is_empty(), "the guard kept {route} back");
    }
    for area in ["blueprints", "stock", "ships"] {
        assert_eq!(count(&b.counts(area), "paused"), 1, "{area} is kept back like a held send");
    }
    assert_eq!(b.double.blueprint_names(), ["Corsair Legs"], "over there is as it was");
    assert!(b.local_names().contains(&"Corsair Legs".to_string()), "fetching goes on");
    assert!(ship_named(&b.hangar_ships(), "Prospector").is_some(), "the hangar fetched as well");
    // Unlike a held send nothing is remembered: no pending confirmation, no link, no hold in the state.
    assert!(object_keys(&b.state(), "held").is_empty());
    assert_eq!(b.sync.confirm_link(ROUTE_BLUEPRINTS), None);
    // The next pass tries again and is kept back again, and it still does not fail.
    b.pass();
    assert!(b.double.ops_sent(ROUTE_BLUEPRINTS).is_empty());
}

#[test]
fn a_guard_of_one_lets_one_change_go_and_keeps_two_back() {
    let one = guarded("guard-eins", &[SETTING_BLUEPRINTS], 1);
    one.add_local("Aether Rifle", "hand");
    one.pass();
    assert_eq!(one.double.blueprint_names(), ["Aether Rifle"], "one change per send goes");
    assert_eq!(sent(&one.double, ROUTE_BLUEPRINTS), 1);

    let two = guarded("guard-zwei", &[SETTING_BLUEPRINTS], 1);
    two.add_local("Aether Rifle", "hand");
    two.add_local("Bastion Helm", "hand");
    two.pass();
    assert!(two.double.ops_sent(ROUTE_BLUEPRINTS).is_empty(), "two changes in one send are over the ceiling");
    assert_eq!(two.double.blueprint_names(), Vec::<String>::new());
    assert_eq!(count(&two.counts("blueprints"), "paused"), 1);
    assert_eq!(two.local_names(), ["Aether Rifle", "Bastion Helm"], "nothing here was given up");
}

#[test]
fn a_guard_of_one_lets_one_change_go_in_the_whole_run() {
    // Across areas: blueprints and stock have one change each, only the first area's goes.
    let b = guarded("guard-lauf", &[SETTING_BLUEPRINTS, SETTING_STOCK], 1);
    b.add_local("Aether Rifle", "hand");
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary")], &[]);
    b.pass();
    assert_eq!(sent(&b.double, ROUTE_BLUEPRINTS), 1, "the first change of the run went");
    assert!(b.double.ops_sent(ROUTE_STOCK).is_empty(), "the stock waits: the run has nothing left");
    assert_eq!(count(&b.counts("stock"), "paused"), 1);
    assert_eq!(b.sync.connection().write_left(), Some(0));
    // Across passes: a later change of the player is kept back as well, and the pass still does not fail.
    b.add_local("Bastion Helm", "hand");
    b.pass();
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle"], "nothing more went in this run");
    assert_eq!(count(&b.counts("blueprints"), "paused"), 1);
    assert!(b.local_names().contains(&"Bastion Helm".to_string()), "nothing here was given up");
}

// ------------------------------------------------------------------ stock

fn raw_row(material: &str, amount: f64, quality: i64, place: &str) -> Value {
    Value::Object(vec![
        ("material".into(), Value::str(material)),
        ("menge".into(), Value::Float(amount)),
        ("qualitaet".into(), Value::int(quality)),
        ("ort".into(), Value::str(place)),
    ])
}

fn trade_row(goods: &str, amount: i64, place: &str, piece: bool) -> Value {
    let mut row = vec![
        ("ware".to_string(), Value::str(goods)),
        ("menge".to_string(), Value::int(amount)),
        ("ort".to_string(), Value::str(place)),
    ];
    if piece {
        row.push(("stueck".to_string(), Value::Bool(true)));
    }
    Value::Object(row)
}

impl Bench {
    fn save_stock(&self, raw: &[Value], trade: &[Value]) {
        use versekit_core::stock;
        assert!(stock::save(&self.paths.app_file(stock::RAW_FILE), raw, "test"));
        assert!(stock::save(&self.paths.app_file(stock::TRADE_FILE), trade, "test"));
    }

    fn raw(&self) -> Vec<Value> {
        versekit_core::stock::load(&self.paths.app_file(versekit_core::stock::RAW_FILE), "test")
    }

    fn trade(&self) -> Vec<Value> {
        versekit_core::stock::load(&self.paths.app_file(versekit_core::stock::TRADE_FILE), "test")
    }
}

#[test]
fn an_item_from_over_there_comes_into_the_trade_stock_in_whole_pieces() {
    let b = bench("stock-item", &[SETTING_STOCK]);
    b.save_stock(&[], &[]);
    b.double.set_lot("Medpen", "Lorville", 0, false, 4.0);
    b.pass();
    let trade = b.trade();
    let medpen = find_row(&trade, "ware", "Medpen").expect("the item came in as goods in pieces");
    assert_eq!(medpen.get("stueck"), Some(&Value::Bool(true)));
    assert_eq!(medpen.get("menge"), Some(&Value::int(4)), "whole pieces are whole numbers");
    assert!(b.raw().is_empty());
}

fn amount(row: &Value) -> f64 {
    match row.get("menge") {
        Some(Value::Float(x)) => *x,
        Some(Value::Int(n)) => n.parse().unwrap_or(f64::NAN),
        other => panic!("{other:?}"),
    }
}

fn find_row<'a>(rows: &'a [Value], key: &str, name: &str) -> Option<&'a Value> {
    rows.iter().find(|r| r.get(key).and_then(Value::as_str) == Some(name))
}

#[test]
fn the_first_stock_sync_adds_only_and_leaves_a_difference_on_both_sides_to_the_player() {
    let b = bench("stock-first", &[SETTING_STOCK]);
    b.save_stock(
        &[raw_row("Iron", 3.0, 500, "Orbituary"), raw_row("Gold", 1.0, 100, "Lorville")],
        &[trade_row("Pistol", 2, "Orbituary", true)],
    );
    b.double.set_lot("Gold", "Lorville", 100, false, 2.0);
    b.double.set_lot("Copper", "Area18", 100, false, 4.0);
    b.pass();
    let counts = b.counts("stock");
    assert_eq!((count(&counts, "sent"), count(&counts, "taken"), count(&counts, "conflicts")), (2, 1, 1));
    // Out: what only stood here (iron, and the pistol in whole pieces).
    assert_eq!(b.double.lot_amount("Iron", "Orbituary", 500, false), 3.0);
    assert_eq!(b.double.lot_amount("Pistol", "Orbituary", 0, false), 2.0);
    let pistol = b.double.lots().into_iter().find(|l| l.name == "Pistol").expect("pistol");
    assert_eq!(pistol.unit, "PIECE");
    let ops = &b.double.ops_sent(ROUTE_STOCK)[0];
    let iron =
        ops.iter().find(|o| o.get("material").and_then(|m| m.get("bt")).and_then(Value::as_str) == Some("bt-iron"));
    let iron = iron.expect("iron");
    assert_eq!(iron.get("expectedQuantity").and_then(|q| q.get("amount")), Some(&Value::Float(0.0)));
    // In: what only stood there.
    let copper = find_row(&b.raw(), "material", "Copper").cloned().expect("copper came in");
    assert_eq!((amount(&copper), copper.get("qualitaet")), (4.0, Some(&Value::int(100))));
    assert_eq!(copper.get("ort").and_then(Value::as_str), Some("Area18"));
    // Both sides different: nobody decides but the player.
    assert_eq!(b.double.lot_amount("Gold", "Lorville", 100, false), 2.0);
    assert_eq!(amount(find_row(&b.raw(), "material", "Gold").expect("gold")), 1.0);
    let state = b.state();
    let stock = state.iter().find(|(k, _)| k == "stock").map(|(_, v)| v.clone()).expect("stock state");
    assert_eq!(stock.get("conflicts").and_then(Value::as_object).map(Vec::len), Some(1));
    assert_eq!(
        stock.get("conflict_names").and_then(|n| n.get("bt-gold|lorville|100|0")).and_then(Value::as_str),
        Some("Gold @ Lorville (Q100)")
    );
    // Steady: nothing more goes out, the open conflict stays open.
    let before = b.double.bodies().len();
    b.pass();
    assert_eq!(b.double.bodies().len(), before);
    assert_eq!(count(&b.counts("stock"), "conflicts"), 1);
}

#[test]
fn the_player_decides_a_stock_conflict_for_this_side_or_for_the_basetool() {
    let b = bench("stock-decide", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Gold", 1.0, 100, "Lorville"), raw_row("Iron", 5.0, 500, "Orbituary")], &[]);
    b.double.set_lot("Gold", "Lorville", 100, false, 2.0);
    b.double.set_lot("Iron", "Orbituary", 500, false, 9.0);
    b.pass();
    assert_eq!(count(&b.counts("stock"), "conflicts"), 2);
    // Gold: mine counts. Iron: theirs counts.
    assert!(b.sync.decide(decisions::stock_decide(vec!["bt-gold|lorville|100|0".into()], "mine")));
    assert!(b.sync.decide(decisions::stock_decide(vec!["bt-iron|orbituary|500|0".into()], "theirs")));
    b.pass();
    assert_eq!(b.double.lot_amount("Gold", "Lorville", 100, false), 1.0);
    let ops = b.double.ops_sent(ROUTE_STOCK);
    let gold = ops.last().and_then(|set| set.first()).expect("a send");
    assert_eq!(gold.get("override"), Some(&Value::Bool(true)));
    assert_eq!(gold.get("expectedQuantity").and_then(|q| q.get("amount")), Some(&Value::Float(2.0)));
    assert_eq!(amount(find_row(&b.raw(), "material", "Iron").expect("iron")), 9.0);
    assert_eq!(count(&b.counts("stock"), "conflicts"), 0);
    // Used up.
    let state = b.state();
    let stock = state.iter().find(|(k, _)| k == "stock").map(|(_, v)| v.clone()).expect("stock state");
    assert_eq!(stock.get("decisions").and_then(Value::as_object).map(Vec::len), Some(0));
}

#[test]
fn a_change_here_and_a_change_over_there_after_the_last_sync_meet_as_a_refused_send_and_then_a_conflict() {
    let b = bench("stock-race", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary")], &[]);
    b.pass();
    assert_eq!(b.double.lot_amount("Iron", "Orbituary", 500, false), 3.0);
    // The player has 5 now; just before the send another member makes it 7 over there.
    b.save_stock(&[raw_row("Iron", 5.0, 500, "Orbituary")], &[]);
    b.double.race_lot("Iron", "Orbituary", 500, false, 7.0);
    b.pass();
    let counts = b.counts("stock");
    assert_eq!((count(&counts, "sent"), count(&counts, "rejected")), (0, 1), "VERSION_CONFLICT is refused, not forced");
    assert_eq!(b.double.lot_amount("Iron", "Orbituary", 500, false), 7.0);
    // The next pass fetches and compares: both sides changed since the stand, the player decides.
    b.pass();
    assert_eq!(count(&b.counts("stock"), "conflicts"), 1);
    assert_eq!(amount(find_row(&b.raw(), "material", "Iron").expect("iron")), 5.0, "nothing is overwritten here");
}

#[test]
fn a_lot_removed_over_there_empties_the_row_here_when_nothing_changed_here() {
    let b = bench("stock-gone", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary"), raw_row("Gold", 1.0, 100, "Lorville")], &[]);
    b.pass();
    b.double.remove_lot_elsewhere("Iron", "Orbituary", 500, false);
    b.pass();
    assert!(find_row(&b.raw(), "material", "Iron").is_none());
    assert!(find_row(&b.raw(), "material", "Gold").is_some());
}

#[test]
fn a_place_the_basetool_does_not_know_stays_here_and_protects_the_lot_over_there() {
    let b = bench("stock-place", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Nirgendwo")], &[]);
    b.double.set_lot("Iron", "Nirgendwo", 500, false, 3.0);
    b.pass();
    let counts = b.counts("stock");
    assert_eq!((count(&counts, "skipped_location"), count(&counts, "sent")), (1, 0));
    // Over there somebody changes the lot. Here the row stays, but is not booked: it would look as if the lot
    // was emptied here, and a send to 0 would follow. That must not happen, and it is no conflict either.
    let sent_before = sent(&b.double, ROUTE_STOCK);
    b.double.set_lot("Iron", "Nirgendwo", 500, false, 5.0);
    b.pass();
    let counts = b.counts("stock");
    assert_eq!(sent(&b.double, ROUTE_STOCK), sent_before, "nothing goes out for a place that cannot be booked");
    assert_eq!((count(&counts, "guarded"), count(&counts, "conflicts"), count(&counts, "taken")), (1, 0, 0));
    assert_eq!(b.double.lot_amount("Iron", "Nirgendwo", 500, false), 5.0);
    assert_eq!(amount(find_row(&b.raw(), "material", "Iron").expect("iron")), 3.0, "the row stays as it is");
}

#[test]
fn an_unreadable_stock_file_is_left_alone_and_nothing_is_asked() {
    let b = bench("stock-broken", &[SETTING_STOCK]);
    let path = b.paths.app_file(versekit_core::stock::RAW_FILE);
    std::fs::write(&path, "{kaputt").expect("file");
    b.double.set_lot("Iron", "Orbituary", 500, false, 3.0);
    b.pass();
    assert_eq!(count(&b.counts("stock"), "unreadable"), 1);
    assert_eq!(std::fs::read_to_string(&path).expect("file"), "{kaputt", "the file is not written over");
    assert!(!b.double.seen().iter().any(|line| line.contains("/me/stock")), "not even asked");
}

#[test]
fn a_stock_the_player_wrote_during_the_pass_is_not_written_over_and_the_pass_starts_again() {
    let b = bench("stock-touched", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary")], &[]);
    b.double.set_lot("Iron", "Orbituary", 500, false, 3.0);
    b.pass();
    // Over there Iron grows to 9 (to take over), here a new row is booked (to send).
    b.double.set_lot("Iron", "Orbituary", 500, false, 9.0);
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary"), raw_row("Copper", 2.0, 500, "Orbituary")], &[]);
    let path = b.paths.app_file(versekit_core::stock::RAW_FILE);
    b.double.on_next_request(ROUTE_STOCK, move || {
        let rows = [
            raw_row("Iron", 3.0, 500, "Orbituary"),
            raw_row("Copper", 2.0, 500, "Orbituary"),
            raw_row("Gold", 1.0, 100, "Orbituary"),
        ];
        assert!(versekit_core::stock::save(&path, &rows, "test"));
    });
    assert!(b.sync.pass_now().is_err(), "the file changed after it was read");
    let rows = b.raw();
    assert!(find_row(&rows, "material", "Gold").is_some(), "what the player wrote stays");
    assert_eq!(amount(find_row(&rows, "material", "Iron").expect("iron")), 3.0, "nothing is taken over yet");
    // The next pass starts over with the new file and takes the 9 over.
    b.pass();
    let rows = b.raw();
    assert_eq!(amount(find_row(&rows, "material", "Iron").expect("iron")), 9.0);
    assert!(find_row(&rows, "material", "Gold").is_some());
    assert_eq!(b.double.lot_amount("Gold", "Orbituary", 100, false), 1.0, "and the new row goes out");
}

#[cfg(windows)]
#[test]
fn a_stock_that_cannot_be_written_ends_the_pass_and_the_take_over_is_not_counted_as_an_own_change() {
    let b = bench("stock-write", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary")], &[]);
    b.double.set_lot("Iron", "Orbituary", 500, false, 3.0);
    b.pass();
    let state_before = b.state();
    // Over there Iron grows to 9; here the file is locked just before the pass reads it.
    b.double.set_lot("Iron", "Orbituary", 500, false, 9.0);
    let file = b.paths.app_file(versekit_core::stock::RAW_FILE);
    let marked = file.clone();
    b.double.on_next_request("/me/stock", move || read_only(&marked, true));
    assert!(b.sync.pass_now().is_err(), "the write failed, the pass ends");
    read_only(&file, false);
    assert_eq!(amount(find_row(&b.raw(), "material", "Iron").expect("iron")), 3.0);
    assert_eq!(b.state(), state_before, "the baseline stays at the old stand");
    let sent_before = sent(&b.double, ROUTE_STOCK);
    // The next pass takes the 9 over; with a baseline moved on, it would have sent the 3 back instead.
    b.pass();
    assert_eq!(amount(find_row(&b.raw(), "material", "Iron").expect("iron")), 9.0);
    assert_eq!(b.double.lot_amount("Iron", "Orbituary", 500, false), 9.0);
    assert_eq!(sent(&b.double, ROUTE_STOCK), sent_before, "nothing went back to the Basetool");
}

#[test]
fn more_than_five_hundred_stock_changes_go_out_in_sends_of_five_hundred() {
    let b = bench("stock-batch", &[SETTING_STOCK]);
    // 501 different lots of one material: the quality tells them apart.
    let rows: Vec<Value> = (0..501).map(|quality| raw_row("Iron", 1.0, quality, "Orbituary")).collect();
    b.save_stock(&rows, &[]);
    b.pass();
    let sends: Vec<usize> = b.double.ops_sent(ROUTE_STOCK).iter().map(Vec::len).collect();
    assert_eq!(sends, [500, 1], "{sends:?}");
}

#[test]
fn an_amount_that_is_no_number_ends_the_pass_and_is_never_sent_as_nothing() {
    let b = bench("stock-amount", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary")], &[]);
    b.pass();
    assert_eq!(b.double.lot_amount("Iron", "Orbituary", 500, false), 3.0);
    let state_before = b.state();
    let sent_before = sent(&b.double, ROUTE_STOCK);
    // Somebody wrote a word into the file by hand.
    let mut broken = raw_row("Iron", 3.0, 500, "Orbituary");
    if let Value::Object(row) = &mut broken {
        map_set(row, "menge", Value::str("viel"));
    }
    b.save_stock(&[broken], &[]);
    assert!(b.sync.pass_now().is_err(), "the pass ends with an error");
    assert_eq!(sent(&b.double, ROUTE_STOCK), sent_before, "nothing goes out");
    assert_eq!(b.double.lot_amount("Iron", "Orbituary", 500, false), 3.0, "the lot over there is as it was");
    assert_eq!(b.state(), state_before, "the state of the pass is thrown away");
}

#[test]
fn a_held_stock_send_waits_and_remembers_the_hold_for_the_confirmation() {
    let b = bench("stock-held", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary")], &[]);
    b.double.hold_next(ROUTE_STOCK);
    b.pass();
    assert_eq!(count(&b.counts("stock"), "paused"), 1);
    assert_eq!(b.double.lot_amount("Iron", "Orbituary", 500, false), 0.0);
    assert_eq!(b.sync.confirm_link(ROUTE_STOCK).as_deref(), Some(CONFIRM_URL));
    let held = b.state().iter().find(|(k, _)| k == "held").map(|(_, v)| v.clone()).expect("held");
    assert!(held.get(ROUTE_STOCK).is_some());
}

// ----------------------------------------------------------------- hangar

fn ship(name: &str, maker: &str, origin: &str) -> Value {
    Value::Object(vec![
        ("name".into(), Value::str(name)),
        ("hersteller".into(), Value::str(maker)),
        ("herkunft".into(), Value::str(origin)),
        ("belegung".into(), Value::Object(Map::new())),
    ])
}

impl Bench {
    fn save_hangar(&self, ships: Vec<Value>) {
        use versekit_core::hangar;
        let data: Map = vec![("format".into(), Value::int(1)), ("schiffe".into(), Value::Array(ships))];
        assert!(hangar::write(&self.paths.app_file(hangar::FILE), &data));
    }

    fn hangar_ships(&self) -> Vec<Map> {
        let data = versekit_core::hangar::load(&self.paths.app_file(versekit_core::hangar::FILE));
        versekit_core::hangar::ships(&data).into_iter().cloned().collect()
    }
}

fn months(n: i64) -> Value {
    Value::Object(vec![("kind".into(), Value::str("MONTHS")), ("months".into(), Value::int(n))])
}

fn external_of(ship: &Map) -> String {
    versekit_core::basetool::ship_sync::external_id(ship)
}

fn ship_named<'a>(ships: &'a [Map], name: &str) -> Option<&'a Map> {
    ships.iter().find(|s| s.iter().any(|(k, v)| k == "name" && v.as_str() == Some(name)))
}

#[test]
fn the_first_hangar_sync_links_creates_and_takes_what_is_only_over_there() {
    let b = bench("ships-first", &[SETTING_SHIPS]);
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge"), ship("Cutlass Black", "Drake", "pledge")]);
    b.double.add_ship("Avenger Titan", None, months(6), "");
    b.double.add_ship("Prospector", None, months(0), "");
    b.pass();
    let counts = b.counts("ships");
    assert_eq!(
        (count(&counts, "linked"), count(&counts, "created"), count(&counts, "taken"), count(&counts, "rejected")),
        (1, 1, 1, 0),
        "{counts:?}"
    );
    let ships = b.double.ships();
    let avenger = ships.iter().find(|s| s.type_name == "Avenger Titan").expect("avenger");
    assert!(avenger.external.as_deref().is_some_and(|e| e.starts_with("vk-")), "linked by the identifier of the entry");
    assert!(ships.iter().any(|s| s.type_name == "Cutlass Black"), "created over there");
    assert_eq!(ships.len(), 3);
    // The one only over there came in, as a ship from the Basetool.
    let local = b.hangar_ships();
    let prospector = ship_named(&local, "Prospector").expect("taken in");
    assert_eq!(prospector.iter().find(|(k, _)| k == "herkunft").and_then(|(_, v)| v.as_str()), Some("basetool"));
    // The insurance of the Basetool came with the link (nothing was known here).
    let titan = ship_named(&local, "Avenger Titan").expect("avenger");
    assert_eq!(titan.iter().find(|(k, _)| k == "versicherung").map(|(_, v)| v.clone()), Some(Value::int(6)));
    // The ship that was taken in is hung on its entry at the next pass (a `link`), and then it is steady.
    b.pass();
    let second = b.double.ops_sent(ROUTE_SHIPS);
    assert_eq!(second.len(), 2);
    assert_eq!(second[1].len(), 1);
    assert_eq!(second[1][0].get("op").and_then(Value::as_str), Some("link"));
    assert!(b.double.ships().iter().all(|s| s.external.is_some()));
    let before = b.double.bodies().len();
    b.pass();
    assert_eq!(b.double.bodies().len(), before, "steady");
}

#[test]
fn a_ship_removed_here_is_asked_about_and_one_removed_over_there_is_not_made_again_without_consent() {
    let b = bench("ships-removed", &[SETTING_SHIPS]);
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge"), ship("Cutlass Black", "Drake", "pledge")]);
    b.pass();
    let ext_cutlass = external_of(&ship_entry("Cutlass Black", "Drake"));
    let ext_avenger = external_of(&ship_entry("Avenger Titan", "Aegis Dynamics"));
    // Removed here: asked about, not removed over there.
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge")]);
    b.pass();
    assert_eq!(b.double.ships().len(), 2);
    let state = b.state();
    let ships = state.iter().find(|(k, _)| k == "ships").map(|(_, v)| v.clone()).expect("ships state");
    assert!(ships.get("local_removed").and_then(|l| l.get(&ext_cutlass)).is_some());
    assert!(b.sync.decide(decisions::ships_release(vec![ext_cutlass.clone()])));
    b.pass();
    assert_eq!(b.double.ships().len(), 1);
    assert_eq!(count(&b.counts("ships"), "removed"), 1);
    // Removed over there: not made again by itself.
    let avenger_id = b.double.ships().iter().find(|s| s.type_name == "Avenger Titan").expect("avenger").id.clone();
    b.double.remove_ship_elsewhere(&avenger_id);
    b.pass();
    assert!(b.double.ships().is_empty(), "the entry here stays, the ship over there is not made again");
    assert_eq!(b.hangar_ships().len(), 1);
    let state = b.state();
    let ships = state.iter().find(|(k, _)| k == "ships").map(|(_, v)| v.clone()).expect("ships state");
    let elsewhere = ships.get("removed_elsewhere").cloned().unwrap_or(Value::Null);
    assert_eq!(elsewhere, Value::Array(vec![Value::str(&ext_avenger)]));
    // With consent it is made again.
    assert!(b.sync.decide(decisions::ships_override(vec![ext_avenger.clone()])));
    b.pass();
    assert_eq!(b.double.ships().len(), 1);
}

fn ship_entry(name: &str, maker: &str) -> Map {
    match ship(name, maker, "pledge") {
        Value::Object(map) => map,
        _ => unreachable!(),
    }
}

#[test]
fn a_hangar_file_that_cannot_be_read_is_left_alone() {
    let b = bench("ships-broken", &[SETTING_SHIPS]);
    let path = b.paths.app_file(versekit_core::hangar::FILE);
    std::fs::write(&path, "{kaputt").expect("file");
    b.double.add_ship("Prospector", Some("fremd"), months(0), "");
    b.pass();
    assert_eq!(count(&b.counts("ships"), "unreadable"), 1);
    assert_eq!(std::fs::read_to_string(&path).expect("file"), "{kaputt");
    assert!(!b.double.seen().iter().any(|line| line.contains("/me/ships")));
}

#[test]
fn a_change_of_the_insurance_over_there_comes_in_after_the_first_agreement() {
    let b = bench("ships-insurance", &[SETTING_SHIPS]);
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge")]);
    b.pass();
    let id = b.double.ships()[0].id.clone();
    // The member buys more months in the browser; here nothing changed since the last sync.
    b.double.set_insurance(&id, months(24));
    b.pass();
    let local = b.hangar_ships();
    let titan = ship_named(&local, "Avenger Titan").expect("avenger");
    assert_eq!(titan.iter().find(|(k, _)| k == "versicherung").map(|(_, v)| v.clone()), Some(Value::int(24)));
    assert_eq!(
        titan.iter().find(|(k, _)| k == "versicherung_basetool").map(|(_, v)| v.clone()),
        Some(Value::Bool(true)),
        "later imports leave an insurance from the Basetool alone"
    );
}

// ----------------------------------------------------------------- faults

#[test]
fn maintenance_with_retry_after_is_a_retry_and_the_connection_stays() {
    let b = bench("fault-503", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "hand");
    b.double.fail("", 503, "SERVICE_UNAVAILABLE", Some(120), 1);
    let error = b.sync.pass_now().expect_err("maintenance");
    assert_eq!(error.code, "SERVICE_UNAVAILABLE");
    assert_eq!(error.status, 503);
    assert_eq!(error.retry_after, Some(120));
    assert_eq!(error.action(), versekit_core::basetool::error::Action::Retry);
    assert!(b.sync.connection().connected(), "the connection stays");
    assert_eq!(b.sync.status().get().1.expect("status").phase, Phase::Error);
    b.double.heal();
    b.pass();
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle"], "the next pass goes through");
}

#[test]
fn a_refused_token_with_405_keeps_the_connection_and_invalid_grant_ends_it() {
    let b = bench("fault-token", &[SETTING_BLUEPRINTS]);
    b.add_local("Aether Rifle", "hand");
    b.double.fail_token(405, "", 1);
    let error = b.sync.pass_now().expect_err("no token");
    assert_eq!(error.code, "TOKEN_FAILED");
    assert_eq!(error.status, 405);
    assert_eq!(error.action(), versekit_core::basetool::error::Action::Stop, "a 405 is not tried again by itself");
    assert!(b.sync.connection().connected(), "the refresh token is kept");
    b.double.heal();
    b.pass();
    assert_eq!(b.double.blueprint_names(), ["Aether Rifle"]);
    b.double.fail_token(400, "invalid_grant", 1);
    // The access token is still valid: let the time run so that it has to be renewed.
    b.skew.fetch_add(1000, Ordering::SeqCst);
    let error = b.sync.pass_now().expect_err("revoked");
    assert_eq!(error.code, "invalid_grant");
    assert_eq!(error.action(), versekit_core::basetool::error::Action::Relogin);
    assert!(!b.sync.connection().connected(), "the refresh token is forgotten");
}

#[test]
fn a_missing_installation_id_stops_the_pass_with_its_own_code() {
    let b = bench("fault-id", &[SETTING_BLUEPRINTS]);
    b.double.script_no_installation(true);
    let error = b.sync.pass_now().expect_err("no id");
    assert_eq!(error.code, "INSTALLATION_ID_MISSING");
    let _ = (ROUTE_SHIPS, ROUTE_STOCK, SETTING_SHIPS, SETTING_STOCK);
}

#[test]
fn a_blueprint_the_basetool_knows_under_no_name_stays_only_here_is_counted_and_asked_again_only_after_a_day() {
    use versekit_core::basetool::overview::area_lines;
    use versekit_core::language::{Arg, tf};
    let b = bench("bp-unknown", &[SETTING_BLUEPRINTS]);
    b.add_local("Name Von Nirgendwo", "hand");
    b.pass();
    let counts = b.counts("blueprints");
    assert_eq!((count(&counts, "unresolved"), count(&counts, "added")), (1, 0), "{counts:?}");
    assert!(!b.double.blueprint_names().iter().any(|n| n.contains("Nirgendwo")), "never sent under a made-up name");
    assert!(b.local_names().iter().any(|n| n.contains("Nirgendwo")), "it stays here");
    let status = b.sync.status().get().1.expect("status");
    let lines = area_lines(SETTING_BLUEPRINTS, &status.counts, &b.state(), None);
    assert!(lines.contains(&tf("s_btv_unbekannt", &[("n", Arg::Int(1))])), "{lines:?}");
    // Asked again after a day, not at every pass.
    let questions = |b: &Bench| b.double.seen().iter().filter(|r| r.starts_with("POST /catalog/resolve")).count();
    let before = questions(&b);
    assert!(before > 0, "the first pass asked");
    b.pass();
    assert_eq!(questions(&b), before, "the answer is remembered");
    b.skew.store(86_400 + 600, Ordering::SeqCst);
    b.pass();
    assert!(questions(&b) > before, "after a day the question goes out again");
}

/// The lines under an area after the last pass.
fn lines_of(b: &Bench, setting: &str) -> Vec<String> {
    let status = b.sync.status().get().1.expect("status");
    versekit_core::basetool::overview::area_lines(setting, &status.counts, &b.state(), None)
}

fn said(key: &str, n: i64) -> String {
    use versekit_core::language::{Arg, tf};
    tf(key, &[("n", Arg::Int(n))])
}

#[test]
fn the_notes_after_a_stock_pass_tell_about_offers_and_rows_that_moved_and_refusals() {
    let b = bench("stock-notes", &[SETTING_STOCK]);
    // A medpen in the raw stock stands in the wrong store: it moves to the trade stock; iron goes out.
    b.save_stock(&[raw_row("Iron", 3.0, 500, "Orbituary"), raw_row("Medpen", 2.0, 0, "Lorville")], &[]);
    b.double.set_lot("Medpen", "Lorville", 0, false, 2.0);
    b.double.script_effect(ROUTE_STOCK, "offersReduced", 2);
    b.double.script_effect(ROUTE_STOCK, "offersRemoved", 1);
    b.pass();
    let lines = lines_of(&b, SETTING_STOCK);
    assert!(lines.contains(&said("s_btv_lager_angebote", 3)), "{lines:?}");
    assert!(lines.contains(&said("s_btv_lager_umgezogen", 1)), "{lines:?}");
    // The overview of the lots over there and the line of what went and came.
    assert!(lines.contains(&said("s_btv_lager_ueberblick", 2)), "{lines:?}");
    assert!(!lines.iter().any(|l| l.contains("Angebote") && l.contains("abgelehnt")), "no refusal, no line");
}

#[test]
fn the_notes_after_a_hangar_pass_tell_about_lost_missions_and_the_unit_of_the_basetool_that_wins() {
    let b = bench("hangar-notes", &[SETTING_SHIPS]);
    b.double.set_units(&[("u1", "Alpha"), ("u2", "Beta")]);
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge")]);
    b.pass();
    b.pass();
    let id = b.double.ships()[0].id.clone();
    // Both sides moved the ship to a unit since the last sync: the Basetool wins.
    let path = b.paths.app_file(versekit_core::hangar::FILE);
    let mut data = versekit_core::hangar::load(&path);
    versekit_core::hangar::edit_ships(&mut data, |ships| {
        map_set(&mut ships[0], "einheit", Value::str("u1"));
        // A second ship to create over there: that change set reports the missions that lost a ship.
        ships.push(ship_entry("Cutlass Black", "Drake"));
    });
    assert!(versekit_core::hangar::write(&path, &data));
    b.double.set_ship_unit(&id, Some("u2"));
    b.double.script_effect(ROUTE_SHIPS, "detachedFromMissions", 2);
    b.pass();
    let counts = b.counts("ships");
    let lines = lines_of(&b, SETTING_SHIPS);
    assert_eq!(count(&counts, "unit_taken"), 1, "{counts:?}");
    assert!(lines.contains(&said("s_btv_einheit_uebernommen", 1)), "{lines:?}");
    let ship = ship_named(&b.hangar_ships(), "Avenger Titan").cloned().expect("ship");
    assert_eq!(ship.iter().find(|(k, _)| k == "einheit").map(|(_, v)| v.clone()), Some(Value::str("u2")));
    assert_eq!(count(&counts, "detached"), 2, "{counts:?}");
    assert!(lines.contains(&said("s_btv_hangar_missionen", 2)), "{lines:?}");
}

#[test]
fn a_unit_the_basetool_refuses_is_counted_said_under_the_area_and_the_unit_over_there_is_taken() {
    let b = bench("hangar-unit-refused", &[SETTING_SHIPS]);
    b.double.set_units(&[("u1", "Alpha"), ("u2", "Beta")]);
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge")]);
    b.pass();
    b.pass();
    let id = b.double.ships()[0].id.clone();
    // Over there the ship stands in Beta; here it is moved to Alpha, and the Basetool refuses that unit.
    b.double.set_ship_unit(&id, Some("u2"));
    b.pass();
    let unit_here = |b: &Bench| {
        let ship = ship_named(&b.hangar_ships(), "Avenger Titan").cloned().expect("ship");
        ship.iter().find(|(k, _)| k == "einheit").map(|(_, v)| v.clone())
    };
    assert_eq!(unit_here(&b), Some(Value::str("u2")), "the unit of the Basetool came over first");
    let path = b.paths.app_file(versekit_core::hangar::FILE);
    let mut data = versekit_core::hangar::load(&path);
    versekit_core::hangar::edit_ships(&mut data, |ships| map_set(&mut ships[0], "einheit", Value::str("u1")));
    assert!(versekit_core::hangar::write(&path, &data));
    b.double.refuse_unit(&id, "ORG_UNIT_NOT_ALLOWED");
    let sent_before = b.double.ops_sent(ROUTE_SHIPS).len();
    b.pass();
    let sent = b.double.ops_sent(ROUTE_SHIPS);
    assert!(
        sent[sent_before..].iter().flatten().any(|op| op.get("orgUnit").and_then(Value::as_str) == Some("u1")),
        "the unit went out: {sent:?}"
    );
    let counts = b.counts("ships");
    assert_eq!(count(&counts, "unit_refused"), 1, "{counts:?}");
    assert_eq!(count(&counts, "rejected"), 0, "a refused unit is no refused change: {counts:?}");
    let lines = lines_of(&b, SETTING_SHIPS);
    assert!(lines.contains(&said("s_btv_einheit_abgelehnt", 1)), "{lines:?}");
    assert!(!lines.contains(&said("s_btv_einheit_uebernommen", 1)), "{lines:?}");
    assert_eq!(b.double.ships()[0].unit.as_deref(), Some("u2"), "the Basetool kept its unit");
    assert_eq!(unit_here(&b), Some(Value::str("u2")), "the hangar takes the unit of the Basetool");
    // The next pass has nothing to send for it and nothing to say.
    let sent_before = b.double.ops_sent(ROUTE_SHIPS).len();
    b.pass();
    assert_eq!(b.double.ops_sent(ROUTE_SHIPS).len(), sent_before, "the refused unit does not go out again");
    assert_eq!(count(&b.counts("ships"), "unit_refused"), 0);
}

#[test]
fn the_report_line_of_the_basetool_names_connection_rights_last_pass_and_refusal_only_with_an_area_on() {
    use versekit_core::language::Lang;
    use versekit_core::report_facts::basetool_line;
    let b = bench("report-line", &[SETTING_SHIPS]);
    assert_eq!(
        basetool_line(&b.paths, Lang::De, None).as_deref(),
        Some("Verbindung aus · Rechte — · zuletzt abgeglichen — · letzte Absage —"),
        "switched on, no sync of the program"
    );
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge")]);
    b.pass();
    let line = basetool_line(&b.paths, Lang::De, Some(&b.sync)).expect("a line");
    // The rights of the login, sorted, without the prefix.
    assert!(
        line.starts_with("Verbindung an · Rechte blueprints.read, blueprints.write, connect, hangar.read"),
        "{line}"
    );
    assert!(!line.contains("exchange.") && !line.contains("zuletzt abgeglichen —"), "{line}");
    assert!(line.ends_with("letzte Absage —"), "{line}");
    let stamp = line.split("zuletzt abgeglichen ").nth(1).and_then(|rest| rest.split(" · ").next()).unwrap_or("");
    assert_eq!(stamp.len(), "2026-10-30 12:00:00".len(), "{line}");
    // The code of a refusal stands at the end.
    b.double.fail("/me/ships", 503, "MAINTENANCE", None, 5);
    let _ = b.sync.pass_now();
    let line = basetool_line(&b.paths, Lang::En, Some(&b.sync)).expect("a line");
    assert!(line.starts_with("connection on") && line.ends_with("MAINTENANCE"), "{line}");
    b.double.heal();
    // Nobody who switched no area on gets the line.
    assert!(b.paths.set_setting(SETTING_SHIPS, Value::Bool(false)));
    assert_eq!(basetool_line(&b.paths, Lang::De, Some(&b.sync)), None);
}

#[test]
fn the_sync_takes_over_into_the_stock_only_when_the_page_does_not_hold_the_lock() {
    use std::sync::atomic::AtomicBool;
    let b = bench("stock-lock", &[SETTING_STOCK]);
    b.save_stock(&[], &[]);
    b.double.set_lot("Copper", "Area18", 100, false, 4.0);
    let guard = versekit_core::stock::lock();
    let done = Arc::new(AtomicBool::new(false));
    let worker = {
        let (sync, done) = (b.sync.clone(), done.clone());
        std::thread::spawn(move || {
            sync.pass_now().expect("a pass");
            done.store(true, Ordering::SeqCst);
        })
    };
    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(!done.load(Ordering::SeqCst), "the pass waits for the page");
    assert!(b.raw().is_empty(), "nothing written while the page holds the files");
    drop(guard);
    worker.join().expect("the pass ends");
    assert!(find_row(&b.raw(), "material", "Copper").is_some(), "taken over once the page let go");
}

fn titanium_row(amount: f64) -> versekit_core::basetool::stock_split::RowRef {
    versekit_core::basetool::stock_split::RowRef {
        lookup: "Titanium".into(),
        raw: true,
        item: false,
        quality: Some(600),
        place: "Area18".into(),
        stolen: false,
        amount,
    }
}

#[test]
fn a_share_goes_into_the_pool_of_a_unit_and_comes_back_and_each_refusal_keeps_its_reason() {
    use versekit_core::basetool::stock_split::{Failure, attach, send, share_op, snapshot, unshare_op};
    let b = bench("stock-split", &[SETTING_STOCK]);
    b.save_stock(&[raw_row("Titanium", 10.0, 600, "Area18")], &[]);
    b.double.set_units(&[("u1", "Alpha")]);
    b.double.set_lot("Titanium", "Area18", 600, false, 10.0);
    b.pass();
    let snap = snapshot(&b.state()).expect("the stock was synced");
    let shares = attach(&[titanium_row(10.0)], Some(&snap));
    assert_eq!((shares[0].personal, shares[0].shared), (10.0, 0.0));
    let lot = snap.lots.get(shares[0].lot.as_ref().expect("a lot")).expect("the lot").clone();

    send(&b.sync, &share_op(&lot, "u1", 4.0)).expect("shared");
    assert_eq!(b.double.lots()[0].pools, vec![("u1".to_string(), 4.0)]);
    let after = b.double.ops_sent(ROUTE_STOCK).len();
    b.pass();
    let snap = snapshot(&b.state()).expect("snapshot");
    let shares = attach(&[titanium_row(10.0)], Some(&snap));
    assert_eq!((shares[0].personal, shares[0].shared), (6.0, 4.0), "the new split came with the feed");
    assert_eq!(b.double.ops_sent(ROUTE_STOCK).len(), after, "the share is no change of the own stock");
    assert_eq!(b.double.lots()[0].amount, 10.0);
    assert_eq!(amount(&b.raw()[0]), 10.0, "the own file stays as it was");

    let lot = snap.lots.get(shares[0].lot.as_ref().expect("a lot")).expect("the lot").clone();
    // More than is personal, a stranger's unit, an old expectation: each its own reason.
    let refused = |op| match send(&b.sync, &op) {
        Err(Failure::Reason(reason)) => reason,
        other => panic!("{other:?}"),
    };
    assert_eq!(refused(share_op(&lot, "u1", 7.0)), "QUANTITY_ABOVE_EXPECTED");
    assert_eq!(refused(share_op(&lot, "u9", 1.0)), "NOT_A_MEMBER");
    assert_eq!(refused(unshare_op(&lot, "u1", 5.0)), "QUANTITY_ABOVE_EXPECTED");
    let stale = share_op(&lot, "u1", 1.0);
    send(&b.sync, &unshare_op(&lot, "u1", 4.0)).expect("taken back");
    assert_eq!(b.double.lots()[0].pools, Vec::<(String, f64)>::new());
    assert_eq!(refused(stale), "VERSION_CONFLICT", "the personal part is no longer what the page showed");
}

/// A game folder with a language file that knows the ship, so the entry has a key for its own name.
fn game_with_ship_names(b: &Bench) {
    let game = format!("{}/Spiel", b.dir);
    let folder = format!("{game}/data/Localization/german_(germany)");
    std::fs::create_dir_all(&folder).expect("language folder");
    std::fs::write(format!("{game}/Game.log"), "").expect("log");
    std::fs::write(format!("{folder}/global.ini"), "vehicle_NameAEGS_Avenger_Titan=Avenger Titan\n").expect("ini");
}

const TITAN_KEY: &str = "vehicle_NameAEGS_Avenger_Titan";

#[test]
fn a_name_changed_over_there_goes_into_the_names_file_and_calls_the_hook_once() {
    use std::sync::atomic::AtomicUsize;
    let calls = Arc::new(AtomicUsize::new(0));
    let hook: Arc<dyn Fn() + Send + Sync> = {
        let calls = calls.clone();
        Arc::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
        })
    };
    let b = bench_with_names("ships-names", &[SETTING_SHIPS], Some(hook));
    game_with_ship_names(&b);
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge")]);
    b.pass();
    assert_eq!(calls.load(Ordering::SeqCst), 0, "no name came in yet");
    let id = b.double.ships().first().expect("the ship was created over there").id.clone();
    b.double.rename_ship(&id, "Rote Kuh");
    b.pass();
    let stand = versekit_core::asop::load(&b.paths.app_file(versekit_core::asop::FILE));
    assert_eq!(versekit_core::asop::entry(&stand, TITAN_KEY).0, "Rote Kuh");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    b.pass();
    assert_eq!(calls.load(Ordering::SeqCst), 1, "steady: nothing new, nothing written");
}

#[test]
fn an_own_name_goes_out_and_the_ready_mark_and_the_unit_come_back() {
    let b = bench("ships-fields", &[SETTING_SHIPS]);
    game_with_ship_names(&b);
    b.save_hangar(vec![ship("Avenger Titan", "Aegis Dynamics", "pledge")]);
    let path = b.paths.app_file(versekit_core::asop::FILE);
    assert_eq!(versekit_core::asop::store(&path, TITAN_KEY, "Blitz", false).0, versekit_core::asop::Stored::Written);
    b.pass();
    let ship = b.double.ships().into_iter().next().expect("created");
    assert_eq!(ship.name, "Blitz", "the own name goes out");
    b.double.set_fitted(&ship.id, true);
    b.double.set_ship_unit(&ship.id, Some("u7"));
    b.pass();
    let local = b.hangar_ships();
    let entry = ship_named(&local, "Avenger Titan").expect("entry");
    let get = |key: &str| entry.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    assert_eq!(get("einsatzbereit"), Some(Value::Bool(true)));
    assert_eq!(get("einheit"), Some(Value::str("u7")));
}
