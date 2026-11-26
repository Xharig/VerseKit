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

use std::cell::RefCell;
use std::collections::BTreeSet;

use super::org::{self, Plan};
use super::*;
use crate::basetool::orgstock::{Entry, Lot, OrgStockData};
use crate::overlay::keys;
use crate::testdir::Dir;

fn paths_in(dir: &Dir) -> Paths {
    paths_at(&dir.path().to_string_lossy())
}

fn paths_at(home: &str) -> Paths {
    let vars = [(crate::envvars::ENV_HOME, home), (crate::envvars::ENV_NO_NET, "1")];
    Paths::new(crate::paths::Host::new(crate::pypath::Flavor::native(), &vars, home))
}

/// A booking into the own stock waits for the lock of the stock files before it holds the saves: the sync holds
/// the stock files while it saves, and the other order made both wait for ever (Mein Lager hung on 26.11.).
#[test]
fn a_booking_and_a_sync_that_saves_under_the_stock_lock_do_not_wait_for_each_other() {
    use std::sync::mpsc;
    use std::time::Duration;
    let dir = Dir::new("lagerscan-sperren");
    let home = dir.path().to_string_lossy().into_owned();
    let (done, finished) = mpsc::channel();
    let (held, holding) = mpsc::channel();
    let sync_home = home.clone();
    let sync_done = done.clone();
    std::thread::spawn(move || {
        let paths = paths_at(&sync_home);
        let _stock = stock::lock();
        held.send(()).expect("the test waits");
        // The booking runs meanwhile and gets as far as it can.
        std::thread::sleep(Duration::from_millis(300));
        let saved = jsonio::save_json(&paths.app_file("abgleich-probe.json"), &jsonio::Value::Bool(true), 1, false);
        let _ = sync_done.send(("sync", saved.is_ok()));
    });
    holding.recv().expect("the sync holds the stock files");
    std::thread::spawn(move || {
        let paths = paths_at(&home);
        let outcome = run(&context(&paths, Target::Own, None, 1), Reading::Sure(pieces("Parallax", 5.0)));
        let _ = done.send(("booking", matches!(outcome, Outcome::Booked(_))));
    });
    for _ in 0..2 {
        let (who, good) = finished.recv_timeout(Duration::from_secs(20)).expect("neither waits for the other for ever");
        assert!(good, "{who} went through");
    }
    assert_eq!(trade_amount(&paths_in(&dir), "Parallax", "Orison", true), Some(5.0));
}

/// A picked answer whose booking fails keeps its question open: nothing the player picked is lost. Once the
/// booking goes through, the question is gone.
#[test]
fn a_failed_booking_keeps_the_question() {
    let dir = Dir::new("lagerscan-frage-bleibt");
    let paths = paths_in(&dir);
    let ctx = context(&paths, Target::Own, None, 1);
    run(&ctx, Reading::Unsure { item: pieces("Parallx", 1.0), candidates: vec!["Parallax".into()], picture: None });
    let file = paths.app_file(stock::TRADE_FILE);
    std::fs::write(&file, "kein json {").expect("written");
    let outcome = answer(&paths, None, 0, Some("Parallax"), 2).expect("answered");
    assert_eq!(outcome, Outcome::Failed(Target::Own));
    assert_eq!(Journal::load(&paths).questions.len(), 1, "the question stays open");
    // The reading put the broken file aside or not: either way it is gone now.
    let _ = std::fs::remove_file(&file);
    let outcome = answer(&paths, None, 0, Some("Parallax"), 3).expect("answered");
    assert!(matches!(outcome, Outcome::Booked(_)), "{outcome:?}");
    assert!(Journal::load(&paths).questions.is_empty(), "booked: the question is gone");
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), Some(1.0));
}

/// The lock of the stock files taken twice in one thread says so instead of waiting for ever.
#[test]
#[should_panic(expected = "stock::lock taken twice in one thread")]
fn the_stock_lock_taken_twice_in_one_thread_panics() {
    let _first = stock::lock();
    let _second = stock::lock();
}

fn pieces(name: &str, quantity: f64) -> ScannedItem {
    ScannedItem::new(name, quantity, Measure::Pieces, None)
}

fn goods(name: &str, quantity: f64, quality: Option<u32>) -> ScannedItem {
    ScannedItem::new(name, quantity, Measure::Scu, quality)
}

/// A run at Orison; each `now` of a test is a quarter of an hour of its own, so every scan starts a round (the
/// tests of the rounds use [`same_round`]).
fn context<'a>(paths: &'a Paths, target: Target, unit: Option<&'a dyn UnitStock>, now: i64) -> Run<'a> {
    Run { paths, target, place: Some("Orison".into()), place_since: None, unit, now: now * 1000 }
}

/// A run whose scans of one test stand within the pause of a round, `second` seconds on.
fn same_round<'a>(paths: &'a Paths, target: Target, unit: Option<&'a dyn UnitStock>, second: i64) -> Run<'a> {
    Run { paths, target, place: Some("Orison".into()), place_since: Some(1_000), unit, now: 1_000 + second }
}

/// A fingerprint of a made-up tile picture: the same `seed` gives the same picture, another seed another one.
fn print_bytes(seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_add(1);
    let cells = (0..crate::stock_scan::PRINT_CELLS * crate::stock_scan::PRINT_CELLS)
        .map(|_| {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            Some((((state >> 33) % 200) as i16 - 100) as i8)
        })
        .collect();
    crate::stock_scan::TilePrint { cells }.to_bytes()
}

/// A single tile: one piece, at a place on the screen, with a fingerprint of its picture.
fn tile(name: &str, quantity: f64, column: i32, print: u64) -> ScannedItem {
    ScannedItem {
        tile_rect: Some((100 + 90 * i64::from(column), 200, 80, 80)),
        tile_print: Some(print_bytes(print)),
        ..ScannedItem::new(name, quantity, Measure::Pieces, None)
    }
}

fn trade_amount(paths: &Paths, name: &str, place: &str, pieces: bool) -> Option<f64> {
    let entries = stock::load(&paths.app_file(stock::TRADE_FILE), "test");
    stock::find_trade(&entries, name, place, pieces).map(|i| stock::number_of(&entries[i], "menge"))
}

fn raw_amount(paths: &Paths, name: &str, quality: i64, place: &str) -> Option<f64> {
    let entries = stock::load(&paths.app_file(stock::RAW_FILE), "test");
    stock::find_raw(&entries, name, Some(quality), place).map(|i| stock::number_of(&entries[i], "menge"))
}

/// The stock of a unit made of a list: what it holds per item and place, and what was asked of it.
#[derive(Default)]
struct Pool {
    held: RefCell<Vec<(String, String, f64)>>,
    asked: RefCell<Vec<(String, String, Option<f64>)>>,
    /// What orders have set aside: taking below it is [`UnitError::Reserved`] without `force`.
    reserve: RefCell<f64>,
}

impl UnitStock for Pool {
    fn set(&self, item: &ScannedItem, place: &str, amount: Option<f64>, force: bool) -> Result<Option<f64>, UnitError> {
        self.asked.borrow_mut().push((item.name.clone(), place.to_string(), amount));
        let mut held = self.held.borrow_mut();
        let at = held.iter().position(|(n, p, _)| *n == item.name && p == place);
        let before = at.map(|i| held[i].2);
        let reserve = *self.reserve.borrow();
        if !force && reserve > 0.0 && amount.unwrap_or(0.0) < before.unwrap_or(0.0) {
            return Err(UnitError::Reserved(reserve));
        }
        match (at, amount) {
            (Some(i), Some(a)) => held[i].2 = a,
            (Some(i), None) => {
                held.remove(i);
            }
            (None, Some(a)) => held.push((item.name.clone(), place.to_string(), a)),
            (None, None) => {}
        }
        Ok(before)
    }
}

/// A scan sets the amount at the place; the amount that lay there is replaced, never added to.
#[test]
fn a_scan_sets_the_amount_and_never_adds() {
    let dir = Dir::new("lagerscan-setzen");
    let paths = paths_in(&dir);
    let again = |quantity: f64, now: i64| run(&context(&paths, Target::Own, None, now), Reading::Sure(pieces("Parallax", quantity)));
    assert!(matches!(again(5.0, 1), Outcome::Booked(ref s) if s.before.is_none()));
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), Some(5.0));
    let Outcome::Booked(scan) = again(3.0, 2) else { panic!("set") };
    assert_eq!(scan.before, Some(5.0));
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), Some(3.0), "3, not 8");
    // The same amount twice stays what it is and says so.
    assert!(matches!(again(3.0, 3), Outcome::Same(_)));
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), Some(3.0), "not 6");
    let entries = stock::load(&paths.app_file(stock::TRADE_FILE), "test");
    assert_eq!(entries.len(), 1, "one entry, not one per scan");
    // Another place is another entry; the first stays.
    let other = Run { place: Some("Lorville".into()), ..context(&paths, Target::Own, None, 4) };
    run(&other, Reading::Sure(pieces("Parallax", 7.0)));
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), Some(3.0));
    assert_eq!(trade_amount(&paths, "Parallax", "Lorville", true), Some(7.0));
}

/// Goods with a quality go to the material stock, goods without one to the trade stock; each by its quality.
#[test]
fn goods_go_where_their_quality_says() {
    let dir = Dir::new("lagerscan-ware");
    let paths = paths_in(&dir);
    let ctx = |now| context(&paths, Target::Own, None, now);
    run(&ctx(1), Reading::Sure(goods("Agricium", 12.5, Some(700))));
    run(&ctx(2), Reading::Sure(goods("Agricium", 4.0, Some(500))));
    run(&ctx(3), Reading::Sure(goods("Agricium", 9.0, Some(700))));
    assert_eq!(raw_amount(&paths, "Agricium", 700, "Orison"), Some(9.0));
    assert_eq!(raw_amount(&paths, "Agricium", 500, "Orison"), Some(4.0));
    // Goods without a quality read are asked after, never booked on a guess; the pick books them.
    for (second, quantity, now) in [(4, 2.0, 4_000), (5, 6.0, 5_000)] {
        let asked = run(&ctx(second), Reading::Sure(goods("Scrap", quantity, None)));
        assert!(matches!(asked, Outcome::Asked { .. }), "{asked:?}");
        assert_eq!(trade_amount(&paths, "Scrap", "Orison", false), if second == 4 { None } else { Some(2.0) });
        assert!(matches!(answer(&paths, None, 0, Some("Scrap"), now), Some(Outcome::Booked(_))));
    }
    assert_eq!(trade_amount(&paths, "Scrap", "Orison", false), Some(6.0));
    assert_eq!(trade_amount(&paths, "Agricium", "Orison", false), None, "a quality never lies in the trade stock");
}

/// An entry keeps what the scan does not read: the price paid and the stolen mark stay.
#[test]
fn a_set_keeps_the_rest_of_the_entry() {
    let dir = Dir::new("lagerscan-rest");
    let paths = paths_in(&dir);
    let file = paths.app_file(stock::TRADE_FILE);
    let mut entries = Vec::new();
    stock::trade_add_item_with_cost(&mut entries, "Parallax", "5", "Orison", false, Some(77), 1200.0).expect("booked");
    assert!(stock::save(&file, &entries, "test"));
    run(&context(&paths, Target::Own, None, 1), Reading::Sure(pieces("parallax", 2.0)));
    let entries = stock::load(&file, "test");
    assert_eq!(entries.len(), 1);
    assert_eq!(stock::number_of(&entries[0], "menge"), 2.0);
    assert_eq!(stock::cost_of(&entries[0]), Some(1200.0), "the price is not the scan's to change");
    assert_eq!(entries[0].get("uex"), Some(&Value::int(77)));
}

/// Without a known place nothing is booked and nothing is written down.
#[test]
fn without_a_place_nothing_is_booked() {
    let dir = Dir::new("lagerscan-ort");
    let paths = paths_in(&dir);
    for place in [None, Some("  ".to_string())] {
        let ctx = Run { place, ..context(&paths, Target::Own, None, 1) };
        let outcome = run(&ctx, Reading::Sure(pieces("Parallax", 3.0)));
        assert_eq!(outcome, Outcome::NoPlace(Target::Own));
        assert_eq!(outcome.notice_in(Lang::De).kind, NoticeKind::Nothing);
    }
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), None);
    assert!(Journal::load(&paths).scans.is_empty());
}

/// The scan into the stock of the unit is shut without "Book as a logistician", and nothing reaches the stock.
#[test]
fn the_stock_of_the_unit_is_shut_without_the_logistician_setting() {
    let dir = Dir::new("lagerscan-sperre");
    let paths = paths_in(&dir);
    assert_eq!(check(&paths, Target::Unit), Some(Outcome::NotLogistician));
    assert_eq!(check(&paths, Target::Own), None, "the own stock never needs it");
    let said = Outcome::NotLogistician.notice_in(Lang::De);
    assert_eq!(said.target, Some(Target::Unit));
    assert!(said.detail.contains("Logistiker"), "{said:?}");
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(true));
    assert_eq!(check(&paths, Target::Unit), None);
    // A question answered later meets the same lock: the setting may have gone off meanwhile.
    let pool = Pool::default();
    let ctx = context(&paths, Target::Unit, Some(&pool), 1);
    let item = pieces("", 3.0);
    run(&ctx, Reading::Unsure { item, candidates: vec!["Parallax".into(), "Parallax Mk2".into()], picture: None });
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(false));
    let outcome = answer(&paths, Some(&pool), 0, Some("Parallax"), 2).expect("answered");
    assert_eq!(outcome, Outcome::NotLogistician);
    assert!(pool.asked.borrow().is_empty(), "nothing reached the unit");
}

/// A taken scan into the stock of the unit books there, not into the own stock; without a connection it says so.
#[test]
fn the_unit_stock_takes_a_scan_and_the_own_stock_stays_as_it_is() {
    let dir = Dir::new("lagerscan-einheit");
    let paths = paths_in(&dir);
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(true));
    let pool = Pool::default();
    let outcome = run(&context(&paths, Target::Unit, Some(&pool), 1), Reading::Sure(pieces("Parallax", 3.0)));
    let Outcome::Booked(scan) = &outcome else { panic!("{outcome:?}") };
    assert_eq!((scan.target, scan.before), (Target::Unit, None));
    assert_eq!(pool.held.borrow().as_slice(), [("Parallax".to_string(), "Orison".to_string(), 3.0)]);
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), None, "the own stock is untouched");
    let none = run(&context(&paths, Target::Unit, None, 2), Reading::Sure(pieces("Parallax", 3.0)));
    assert_eq!(none, Outcome::NoRight);
    assert_eq!(Journal::load(&paths).scans.len(), 1, "a refused scan leaves no scan");
}

/// A scan taken back restores the value before at the place: the old amount, or no entry where there was none.
#[test]
fn undo_restores_the_value_before() {
    let dir = Dir::new("lagerscan-zurueck");
    let paths = paths_in(&dir);
    let scan_of = |outcome: Outcome| match outcome {
        Outcome::Booked(scan) | Outcome::Same(scan) => scan,
        other => panic!("{other:?}"),
    };
    let ctx = |now| context(&paths, Target::Own, None, now);
    let first = scan_of(run(&ctx(1), Reading::Sure(pieces("Parallax", 5.0))));
    let second = scan_of(run(&ctx(2), Reading::Sure(pieces("Parallax", 3.0))));
    // The older one stands under the newer one of the same item and place.
    assert_eq!(undo(&paths, None, first.id), Err(UndoError::Newer));
    assert_eq!(undo(&paths, None, second.id), Ok(()));
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), Some(5.0), "back to the value before");
    assert_eq!(undo(&paths, None, second.id), Err(UndoError::Gone), "once");
    assert_eq!(undo(&paths, None, first.id), Ok(()));
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), None, "no entry where there was none");
    assert_eq!(undo(&paths, None, 99), Err(UndoError::Gone));
    assert!(Journal::load(&paths).scans.iter().all(|s| s.undone));
    // Goods with a quality come back by their own stack.
    let a = scan_of(run(&ctx(3), Reading::Sure(goods("Agricium", 10.0, Some(700)))));
    let b = scan_of(run(&ctx(4), Reading::Sure(goods("Agricium", 4.0, Some(700)))));
    assert_eq!(undo(&paths, None, b.id), Ok(()));
    assert_eq!(raw_amount(&paths, "Agricium", 700, "Orison"), Some(10.0));
    assert_eq!(undo(&paths, None, a.id), Ok(()));
    assert_eq!(raw_amount(&paths, "Agricium", 700, "Orison"), None);
}

/// In the stock of the unit a scan taken back is a booking against it, and only a logistician may make it.
#[test]
fn undo_in_the_unit_stock_is_a_booking_against_it() {
    let dir = Dir::new("lagerscan-zurueck-einheit");
    let paths = paths_in(&dir);
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(true));
    let pool = Pool::default();
    pool.held.borrow_mut().push(("Parallax".into(), "Orison".into(), 5.0));
    let Outcome::Booked(scan) = run(&context(&paths, Target::Unit, Some(&pool), 1), Reading::Sure(pieces("Parallax", 3.0)))
    else {
        panic!("booked")
    };
    assert_eq!(scan.before, Some(5.0));
    assert_eq!(undo(&paths, None, scan.id), Err(UndoError::NoRight), "no connection");
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(false));
    assert_eq!(undo(&paths, Some(&pool), scan.id), Err(UndoError::NoRight), "not a logistician any more");
    assert_eq!(pool.held.borrow()[0].2, 3.0);
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(true));
    assert_eq!(undo(&paths, Some(&pool), scan.id), Ok(()));
    assert_eq!(pool.held.borrow()[0].2, 5.0, "the value before is back");
    assert_eq!(pool.asked.borrow().last(), Some(&("Parallax".to_string(), "Orison".to_string(), Some(5.0))));
}

/// A reading that is not sure waits as a question; the player's pick books it, none of them books nothing.
#[test]
fn a_question_waits_for_the_pick() {
    let dir = Dir::new("lagerscan-frage");
    let paths = paths_in(&dir);
    let unsure = |picture: Option<Vec<u8>>| Reading::Unsure {
        item: pieces("Parallx", 4.0),
        candidates: vec!["Parallax".into(), "Parallel".into()],
        picture,
    };
    let ctx = |now| context(&paths, Target::Own, None, now);
    let outcome = run(&ctx(1), unsure(Some(vec![1, 2, 3])));
    assert_eq!(outcome, Outcome::Asked { target: Target::Own, candidates: 2 });
    assert_eq!(outcome.notice_in(Lang::En).kind, NoticeKind::Unsure);
    run(&ctx(2), unsure(None));
    let journal = Journal::load(&paths);
    assert_eq!(journal.questions.len(), 2);
    assert_eq!(journal.questions[0].picture.as_deref(), Some([1u8, 2, 3].as_slice()));
    assert_eq!(journal.questions[0].place, "Orison");
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), None, "nothing booked by a question");
    let booked = answer(&paths, None, 0, Some("Parallax"), 3).expect("answered");
    assert!(matches!(booked, Outcome::Booked(_)), "{booked:?}");
    assert_eq!(trade_amount(&paths, "Parallax", "Orison", true), Some(4.0));
    assert_eq!(answer(&paths, None, 0, None, 4), None, "none of them: gone, nothing booked");
    assert!(Journal::load(&paths).questions.is_empty());
    assert_eq!(answer(&paths, None, 0, Some("Parallax"), 5), None, "no such question");
    run(&ctx(6), unsure(None));
    assert!(discard_questions(&paths));
    assert!(Journal::load(&paths).questions.is_empty());
}

/// Nothing read, an empty name or an amount of nothing is not booked.
#[test]
fn what_is_not_a_reading_is_not_booked() {
    let dir = Dir::new("lagerscan-nichts");
    let paths = paths_in(&dir);
    let ctx = context(&paths, Target::Own, None, 1);
    assert_eq!(run(&ctx, Reading::Nothing), Outcome::Nothing(Target::Own));
    assert_eq!(run(&ctx, Reading::Sure(pieces("", 3.0))), Outcome::Nothing(Target::Own));
    assert_eq!(run(&ctx, Reading::Sure(pieces("Parallax", 0.0))), Outcome::Nothing(Target::Own));
    assert_eq!(run(&ctx, Reading::Sure(pieces("Parallax", f64::NAN))), Outcome::Nothing(Target::Own));
    assert!(Journal::load(&paths).scans.is_empty());
}

/// Every notice names the stock first, in both languages, and the two stocks differ in what they say.
#[test]
fn every_notice_names_its_stock() {
    let dir = Dir::new("lagerscan-hinweis");
    let paths = paths_in(&dir);
    let pool = Pool::default();
    let own = run(&context(&paths, Target::Own, None, 1), Reading::Sure(pieces("Parallax", 3.0)));
    let unit = run(&context(&paths, Target::Unit, Some(&pool), 2), Reading::Sure(pieces("Parallax", 3.0)));
    let outcomes = [
        own,
        unit,
        Outcome::Asked { target: Target::Own, candidates: 2 },
        Outcome::Nothing(Target::Unit),
        Outcome::NoPlace(Target::Unit),
        Outcome::NoPlace(Target::Own),
        Outcome::NotLogistician,
        Outcome::NoRight,
        Outcome::UnknownInUnit("Parallax".into()),
        Outcome::Failed(Target::Own),
        Outcome::Counted(Target::Unit, "FR-76".into()),
    ];
    for lang in Lang::ALL {
        for outcome in &outcomes {
            let notice = outcome.notice_in(lang);
            assert_eq!(notice.target, Some(outcome.target()), "{outcome:?}");
            assert_eq!(notice.title, language::t_in(lang, target_title_key(outcome.target())));
            assert!(!notice.title.starts_with("s_lsc_") && !notice.detail.starts_with("s_lsc_"), "{notice:?}");
            assert!(!notice.detail.is_empty(), "{outcome:?}");
        }
    }
    let (a, b) = (outcomes[0].notice_in(Lang::De), outcomes[1].notice_in(Lang::De));
    assert_ne!(a.title, b.title);
    assert!(a.detail.starts_with("3\u{d7} Parallax") && a.detail.contains("Orison"), "{a:?}");
    assert_eq!((a.kind, b.kind), (NoticeKind::Done, NoticeKind::Done));
    let game = Outcome::NoGame(Target::Own).notice_in(Lang::De);
    assert!(game.away, "the game away is told as it is for the other scans");
    // The amounts read as the language writes them.
    assert_eq!(amount_in(Lang::De, &goods("x", 3.5, None)), "3,5 SCU");
    assert_eq!(amount_in(Lang::En, &goods("x", 3.5, None)), "3.5 SCU");
    assert_eq!(amount_in(Lang::De, &goods("x", 100.0, None)), "100 SCU", "no zeros cut off a whole number");
    assert_eq!(amount_in(Lang::De, &goods("x", 0.125, None)), "0,125 SCU");
}

/// Components lie one by one: six tiles "X1" of the same item count six, each tile once.
#[test]
fn six_single_tiles_count_six() {
    let dir = Dir::new("lagerscan-runde");
    let paths = paths_in(&dir);
    for column in 0..6 {
        let outcome = run(&same_round(&paths, Target::Own, None, i64::from(column)), Reading::Sure(tile("FR-76", 1.0, column, 7)));
        if column == 0 {
            assert!(matches!(outcome, Outcome::Booked(_)), "the first one sets: {outcome:?}");
        } else {
            let Outcome::Added(scan) = &outcome else { panic!("{outcome:?}") };
            assert_eq!((scan.before, scan.after), (Some(f64::from(column)), f64::from(column) + 1.0));
            let said = outcome.notice_in(Lang::De);
            assert!(said.detail.starts_with(&format!("FR-76, jetzt {}\u{d7}", column + 1)), "{said:?}");
        }
        assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(f64::from(column) + 1.0));
    }
    assert_eq!(Journal::load(&paths).scans.len(), 6);
    // The next round sets again: a quarter of an hour on, one tile makes 1, not 7.
    let later = run(&same_round(&paths, Target::Own, None, 5_000), Reading::Sure(tile("FR-76", 1.0, 0, 7)));
    assert!(matches!(later, Outcome::Booked(ref s) if s.before == Some(6.0)), "{later:?}");
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(1.0));
}

/// The same tile twice is counted once, however often; a scrolled list puts another picture at the same place.
#[test]
fn the_same_tile_counts_once() {
    let dir = Dir::new("lagerscan-kachel");
    let paths = paths_in(&dir);
    let ctx = |second| same_round(&paths, Target::Own, None, second);
    run(&ctx(0), Reading::Sure(tile("FR-76", 1.0, 0, 7)));
    let again = run(&ctx(1), Reading::Sure(tile("FR-76", 1.0, 0, 7)));
    assert_eq!(again, Outcome::Counted(Target::Own, "FR-76".into()));
    assert_eq!(run(&ctx(2), Reading::Sure(tile("FR-76", 1.0, 0, 7))), again);
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(1.0));
    assert_eq!(Journal::load(&paths).scans.len(), 1, "a tile counted twice leaves no scan");
    let said = again.notice_in(Lang::De);
    assert_eq!((said.kind, said.cue()), (NoticeKind::Counted, Some(crate::sound::Cue::Counted)));
    assert!(said.detail.contains("FR-76") && said.target == Some(Target::Own), "{said:?}");
    // Scrolled: another tile of the same look-alike item at the old place has another print, and counts.
    let scrolled = run(&ctx(3), Reading::Sure(tile("FR-76", 1.0, 0, 8)));
    assert!(matches!(scrolled, Outcome::Added(_)), "{scrolled:?}");
    // Another place on the screen with the same print is another tile too.
    assert!(matches!(run(&ctx(4), Reading::Sure(tile("FR-76", 1.0, 3, 7))), Outcome::Added(_)));
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(3.0));
    // Another item in the same round sets its own amount and counts apart.
    let other = run(&ctx(5), Reading::Sure(tile("Behring P8", 2.0, 1, 9)));
    assert!(matches!(other, Outcome::Booked(_)), "{other:?}");
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(3.0));
    // The same in the stock of the unit does not meet the tiles of the own stock.
    let pool = Pool::default();
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(true));
    let unit = run(&same_round(&paths, Target::Unit, Some(&pool), 6), Reading::Sure(tile("FR-76", 1.0, 0, 7)));
    assert!(matches!(unit, Outcome::Booked(_)), "{unit:?}");
    let more = run(&same_round(&paths, Target::Unit, Some(&pool), 7), Reading::Sure(tile("FR-76", 1.0, 1, 7)));
    assert!(matches!(more, Outcome::Added(_)));
    assert_eq!(pool.held.borrow()[0].2, 2.0, "set, then added");
    let said = more.notice_in(Lang::De);
    assert!(said.detail.starts_with("FR-76, jetzt 2\u{d7}") && said.target == Some(Target::Unit), "{said:?}");
}

/// A round ends when the player leaves the place and when the scans stop for a while; the next scan sets anew.
#[test]
fn a_round_ends_with_the_place_and_with_the_pause() {
    let dir = Dir::new("lagerscan-runde-ende");
    let paths = paths_in(&dir);
    let at = |place: &str, since: Option<i64>, now: i64| Run {
        paths: &paths,
        target: Target::Own,
        place: Some(place.into()),
        place_since: since,
        unit: None,
        now,
    };
    run(&at("Orison", Some(100), 200), Reading::Sure(tile("FR-76", 1.0, 0, 1)));
    run(&at("Orison", Some(100), 210), Reading::Sure(tile("FR-76", 1.0, 1, 1)));
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(2.0));
    // Away and back: the arrival is later than the last scan, so it is a new round, though a minute passed only.
    let back = run(&at("Orison", Some(250), 260), Reading::Sure(tile("FR-76", 1.0, 0, 1)));
    assert!(matches!(back, Outcome::Booked(_)), "a new round sets: {back:?}");
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(1.0));
    // The same tile in the new round counts again; in the old round it was counted.
    assert!(matches!(run(&at("Orison", Some(250), 270), Reading::Sure(tile("FR-76", 1.0, 1, 1))), Outcome::Added(_)));
    // Within the pause it goes on, past the pause it sets anew.
    let near = run(&at("Orison", Some(250), 270 + ROUND_PAUSE), Reading::Sure(tile("FR-76", 1.0, 2, 1)));
    assert!(matches!(near, Outcome::Added(_)), "{near:?}");
    let far = run(&at("Orison", Some(250), 270 + 2 * ROUND_PAUSE + 1), Reading::Sure(tile("FR-76", 1.0, 3, 1)));
    assert!(matches!(far, Outcome::Booked(_)), "{far:?}");
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(1.0));
    // Another place has rounds of its own.
    assert!(matches!(run(&at("Lorville", Some(250), 300), Reading::Sure(tile("FR-76", 1.0, 0, 1))), Outcome::Booked(_)));
    assert_eq!(trade_amount(&paths, "FR-76", "Lorville", true), Some(1.0));
}

/// A scan that added is taken back by itself: the amount before it comes back, the ones under it stay; and the
/// tile of the one taken back counts again.
#[test]
fn undo_takes_back_one_addition() {
    let dir = Dir::new("lagerscan-runde-zurueck");
    let paths = paths_in(&dir);
    let ctx = |second| same_round(&paths, Target::Own, None, second);
    let scan_of = |outcome: Outcome| match outcome {
        Outcome::Booked(s) | Outcome::Added(s) => s,
        other => panic!("{other:?}"),
    };
    let first = scan_of(run(&ctx(0), Reading::Sure(tile("FR-76", 1.0, 0, 7))));
    let second = scan_of(run(&ctx(1), Reading::Sure(tile("FR-76", 1.0, 1, 7))));
    let third = scan_of(run(&ctx(2), Reading::Sure(tile("FR-76", 1.0, 2, 7))));
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(3.0));
    assert_eq!(undo(&paths, None, second.id), Err(UndoError::Newer), "the newest goes first");
    assert_eq!(undo(&paths, None, third.id), Ok(()));
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(2.0), "exactly that step back");
    // The tile of the scan taken back counts again, on top of the two that stand.
    let again = scan_of(run(&ctx(3), Reading::Sure(tile("FR-76", 1.0, 2, 7))));
    assert_eq!((again.before, again.after), (Some(2.0), 3.0));
    assert_eq!(undo(&paths, None, again.id), Ok(()));
    assert_eq!(undo(&paths, None, second.id), Ok(()));
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), Some(1.0));
    assert_eq!(undo(&paths, None, first.id), Ok(()));
    assert_eq!(trade_amount(&paths, "FR-76", "Orison", true), None);
    // The tile is stored and read back with the journal.
    let journal = Journal::load(&paths);
    assert_eq!(journal.scans[0].item.tile_rect, Some((100, 200, 80, 80)));
    assert_eq!(journal.scans[0].item.tile_print, Some(print_bytes(7)));
    assert_eq!(journal.scans[2].round, journal.scans[0].round);
}

/// A tile of ore: an amount in SCU with a quality, at a place on the screen.
fn ore(name: &str, quantity: f64, quality: Option<u32>, column: i64, print: u64) -> ScannedItem {
    ScannedItem {
        tile_rect: Some((100 + 90 * column, 200, 80, 80)),
        tile_print: Some(print_bytes(print)),
        ..ScannedItem::new(name, quantity, Measure::Scu, quality)
    }
}

/// The same ore in two qualities is two stocks: never added together, never one over the other, each taken back
/// for itself.
#[test]
fn the_same_ore_in_two_qualities_is_two_stocks() {
    let dir = Dir::new("lagerscan-erz");
    let paths = paths_in(&dir);
    let ctx = |second| same_round(&paths, Target::Own, None, second);
    let high = run(&ctx(0), Reading::Sure(ore("Quantanium", 1.5, Some(602), 0, 1)));
    let low = run(&ctx(1), Reading::Sure(ore("Quantanium", 0.25, Some(575), 1, 2)));
    let (Outcome::Booked(high), Outcome::Booked(low)) = (high, low) else { panic!("each sets its own stack") };
    assert_eq!(raw_amount(&paths, "Quantanium", 602, "Orison"), Some(1.5));
    assert_eq!(raw_amount(&paths, "Quantanium", 575, "Orison"), Some(0.25));
    // A second tile of 602 adds to the 602 only; the 575 stays.
    let more = run(&ctx(2), Reading::Sure(ore("Quantanium", 0.5, Some(602), 2, 3)));
    assert!(matches!(more, Outcome::Added(_)), "{more:?}");
    assert_eq!(raw_amount(&paths, "Quantanium", 602, "Orison"), Some(2.0));
    assert_eq!(raw_amount(&paths, "Quantanium", 575, "Orison"), Some(0.25));
    // The other quality is not a duplicate of a tile of this one, whatever the place on the screen says.
    let same_place = run(&ctx(3), Reading::Sure(ore("Quantanium", 0.75, Some(575), 0, 1)));
    assert!(matches!(same_place, Outcome::Added(_)), "{same_place:?}");
    assert_eq!(raw_amount(&paths, "Quantanium", 575, "Orison"), Some(1.0));
    assert_eq!(undo(&paths, None, high.id), Err(UndoError::Newer));
    assert_eq!(undo(&paths, None, low.id), Err(UndoError::Newer));
    let entries = stock::load(&paths.app_file(stock::RAW_FILE), "test");
    assert_eq!(entries.len(), 2, "two stacks, not one");
}

/// Amounts of goods keep the decimals of the card: nothing is rounded to a whole or to three places.
#[test]
fn ore_amounts_keep_the_decimals_of_the_card() {
    let dir = Dir::new("lagerscan-erz-menge");
    let paths = paths_in(&dir);
    let ctx = |second| same_round(&paths, Target::Own, None, second);
    run(&ctx(0), Reading::Sure(ore("Laranite", 0.041, Some(590), 0, 1)));
    assert_eq!(raw_amount(&paths, "Laranite", 590, "Orison"), Some(0.041));
    run(&ctx(1), Reading::Sure(ore("Laranite", 0.041, Some(590), 1, 2)));
    assert_eq!(raw_amount(&paths, "Laranite", 590, "Orison"), Some(0.082));
    let said = run(&ctx(2), Reading::Sure(ore("Laranite", 0.0005, Some(590), 2, 3))).notice_in(Lang::En);
    assert!(said.detail.contains("0.082") || said.detail.contains("0.083"), "{said:?}");
}

/// Stolen or mission goods (quality 0) are a stack of their own, apart from the others and from goods without a
/// quality.
#[test]
fn goods_of_quality_zero_stay_their_own_stack() {
    let dir = Dir::new("lagerscan-erz-null");
    let paths = paths_in(&dir);
    let ctx = |second| same_round(&paths, Target::Own, None, second);
    run(&ctx(0), Reading::Sure(ore("Agricium", 3.0, Some(0), 0, 1)));
    run(&ctx(1), Reading::Sure(ore("Agricium", 5.0, Some(700), 1, 2)));
    assert_eq!(raw_amount(&paths, "Agricium", 0, "Orison"), Some(3.0));
    assert_eq!(raw_amount(&paths, "Agricium", 700, "Orison"), Some(5.0));
    let entries = stock::load(&paths.app_file(stock::RAW_FILE), "test");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].get("qualitaet"), Some(&Value::int(0)), "a quality of 0 is written, not left out");
}

/// A quality that is not read for sure books nothing: the question goes to the tab, in both stocks.
#[test]
fn an_unsure_quality_is_a_question_and_never_a_booking() {
    let dir = Dir::new("lagerscan-erz-frage");
    let paths = paths_in(&dir);
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(true));
    let pool = Pool::default();
    for (target, unit) in [(Target::Own, None), (Target::Unit, Some(&pool as &dyn UnitStock))] {
        let outcome = run(&same_round(&paths, target, unit, 0), Reading::Sure(ore("Quantanium", 1.0, None, 0, 1)));
        assert!(matches!(outcome, Outcome::Asked { .. }), "{outcome:?}");
        assert_eq!(outcome.notice_in(Lang::De).kind, NoticeKind::Unsure);
    }
    let unsure = Reading::Unsure {
        item: ore("Quantanium", 1.0, Some(602), 0, 1),
        candidates: vec!["Quantanium".into(), "Quantainium".into()],
        picture: None,
    };
    run(&same_round(&paths, Target::Own, None, 1), unsure);
    assert!(raw_amount(&paths, "Quantanium", 602, "Orison").is_none());
    assert!(stock::load(&paths.app_file(stock::TRADE_FILE), "test").is_empty());
    assert!(pool.held.borrow().is_empty() && pool.asked.borrow().is_empty(), "nothing reached the unit");
    let journal = Journal::load(&paths);
    assert_eq!((journal.questions.len(), journal.scans.len()), (3, 0));
}

// ------------------------------------------------------------ the pool of the unit

fn lot(name: &str, place: &str, quality: i64, piece: bool, amount: f64, entries: Option<Vec<Entry>>) -> Lot {
    Lot {
        key: format!("{name}-{place}-{quality}"),
        unit_id: "u1".into(),
        unit_name: "Alpha".into(),
        material_bt: format!("bt-{name}"),
        material_name: name.into(),
        name: name.into(),
        kind: None,
        component: None,
        location: Vec::new(),
        place: place.into(),
        quality,
        stolen: false,
        amount,
        unit_q: if piece { "PIECE" } else { "SCU" }.into(),
        job: 0.0,
        mission: 0.0,
        entries,
    }
}

fn entry(holder: &str, amount: f64) -> Entry {
    Entry {
        id: format!("e-{holder}"),
        version: 3,
        withheld: false,
        holder: holder.into(),
        holder_ref: holder.into(),
        amount,
        unit: "PIECE".into(),
        note: String::new(),
        jobs: Vec::new(),
        missions: Vec::new(),
        free_jobs: amount,
        free_missions: amount,
    }
}

fn data(lots: Vec<Lot>, write: bool, logistician: bool) -> OrgStockData {
    OrgStockData {
        lots,
        write,
        logistician: if logistician { BTreeSet::from(["u1".to_string()]) } else { BTreeSet::new() },
        ..Default::default()
    }
}


fn op_all(plan: &Plan) -> Vec<(String, f64, String)> {
    plan.ops
        .iter()
        .map(|op| {
            let kind = op.get("op").and_then(Value::as_str).unwrap_or_default().to_string();
            let amount = match op.get("quantity").and_then(|q| q.get("amount")) {
                Some(Value::Int(n)) => n.parse().unwrap_or(0.0),
                Some(Value::Float(x)) => *x,
                _ => -1.0,
            };
            let entry = op.get("entry").and_then(Value::as_str).unwrap_or_default().to_string();
            (kind, amount, entry)
        })
        .collect()
}

fn plan_for(data: &OrgStockData, item: &ScannedItem, amount: Option<f64>, force: bool) -> Result<Plan, UnitError> {
    org::plan(data, "u1", "me", item, "Levski", amount, None, force)
}

/// An entry of an order's reservation: `held` pieces of it are set aside for an order.
fn reserved_entry(holder: &str, amount: f64, held: f64) -> Entry {
    Entry {
        free_jobs: amount - held,
        jobs: vec![crate::basetool::orgstock::JobLine { id: "job-1".into(), number: Some(1), amount: held, delivered: false }],
        ..entry(holder, amount)
    }
}

/// Lots of another logistician lie in the pool already: the scan counts the real stock against them, whoever
/// booked them, and books only the difference.
#[test]
fn the_scan_counts_against_the_lots_that_are_there() {
    let item = pieces("CF-447 Rhino Repeater", 9.0);
    let lenoro = || {
        data(vec![lot("CF-447 Rhino Repeater", "Levski", 0, true, 9.0, Some(vec![entry("lenoro", 9.0)]))], true, true)
    };
    // 9 lots, 9 tiles: it fits, nothing is booked.
    let fits = plan_for(&lenoro(), &item, Some(9.0), false).expect("plan");
    assert_eq!((fits.before, fits.ops.len()), (Some(9.0), 0), "not 18");
    // 9 lots, 11 tiles: two more in, in the name of the player, the lot of the other stays.
    let more = plan_for(&lenoro(), &item, Some(11.0), false).expect("plan");
    assert_eq!(op_all(&more), [("book-in".to_string(), 2.0, String::new())]);
    assert_eq!(more.ops[0].get("holder").and_then(Value::as_str), Some("me"));
    // No lot: a new one, the player is its owner.
    let empty = data(Vec::new(), true, true);
    let fresh = plan_for(&empty, &pieces("CF-447 Rhino Repeater", 3.0), Some(3.0), false);
    assert_eq!(fresh, Err(UnitError::Unknown("CF-447 Rhino Repeater".into())), "the id of the item first");
    let known = org::plan(&empty, "u1", "me", &pieces("CF-447 Rhino Repeater", 3.0), "Levski", Some(3.0), Some("bt-1"), false)
        .expect("plan");
    assert_eq!((known.before, op_all(&known)), (None, vec![("book-in".to_string(), 3.0, String::new())]));
    assert_eq!(known.ops[0].get("holder").and_then(Value::as_str), Some("me"));
    // Several lots of several owners add up.
    let split = data(
        vec![lot("CF-447 Rhino Repeater", "Levski", 0, true, 9.0, Some(vec![entry("lenoro", 4.0), entry("me", 5.0)]))],
        true,
        true,
    );
    assert_eq!(plan_for(&split, &item, Some(9.0), false).expect("plan").ops.len(), 0);
}

/// Less on the tiles than in the pool books the difference out, the entry of the player first; what orders have
/// set aside is not taken without a word.
#[test]
fn fewer_tiles_than_lots_book_out_but_never_take_what_is_set_aside() {
    let item = pieces("CF-447 Rhino Repeater", 7.0);
    // Free stock: out of the entries, his own first.
    let free = data(
        vec![lot("CF-447 Rhino Repeater", "Levski", 0, true, 9.0, Some(vec![entry("lenoro", 6.0), entry("me", 3.0)]))],
        true,
        true,
    );
    let out = plan_for(&free, &item, Some(7.0), false).expect("plan");
    assert_eq!(out.before, Some(9.0));
    assert_eq!(op_all(&out), [("book-out".to_string(), 2.0, "e-me".to_string())]);
    // Two more than his own entry holds: his own first, the rest from the other.
    let two = plan_for(&free, &pieces("CF-447 Rhino Repeater", 4.0), Some(4.0), false).expect("plan");
    assert_eq!(
        op_all(&two),
        [("book-out".to_string(), 3.0, "e-me".to_string()), ("book-out".to_string(), 2.0, "e-lenoro".to_string())]
    );
    // All nine are set aside for an order, 7 tiles: nothing is cut, it says how much is set aside.
    let held = data(
        vec![lot("CF-447 Rhino Repeater", "Levski", 0, true, 9.0, Some(vec![reserved_entry("lenoro", 9.0, 9.0)]))],
        true,
        true,
    );
    assert_eq!(plan_for(&held, &item, Some(7.0), false), Err(UnitError::Reserved(9.0)));
    // After the confirmation it is booked, out of the reservation of the order.
    let forced = plan_for(&held, &item, Some(7.0), true).expect("plan");
    assert_eq!(op_all(&forced), [("book-out".to_string(), 2.0, "e-lenoro".to_string())]);
    let jobs = forced.ops[0].get("fromJobOrders").and_then(|j| if let Value::Array(a) = j { Some(a.clone()) } else { None });
    assert_eq!(jobs.map(|a| a.len()), Some(1), "the order gives the two pieces");
    // Part of it free: the free part goes without a word, the rest is the question.
    let some = data(
        vec![lot("CF-447 Rhino Repeater", "Levski", 0, true, 9.0, Some(vec![reserved_entry("lenoro", 9.0, 8.0)]))],
        true,
        true,
    );
    let one_free = plan_for(&some, &item, Some(8.0), false).expect("one is free");
    assert_eq!(op_all(&one_free), [("book-out".to_string(), 1.0, "e-lenoro".to_string())]);
}

/// The tab asks for the word where a booking would take what is set aside: nothing is booked until it comes, and
/// a confirmed one is made; a dropped one is not.
#[test]
fn a_booking_over_the_reservation_waits_for_the_word() {
    let dir = Dir::new("lagerscan-vorgemerkt");
    let paths = paths_in(&dir);
    paths.set_setting(keys::LOGISTICIAN, Value::Bool(true));
    let pool = Pool::default();
    pool.held.borrow_mut().push(("FR-76".into(), "Orison".into(), 9.0));
    *pool.reserve.borrow_mut() = 9.0;
    let outcome = run(&same_round(&paths, Target::Unit, Some(&pool), 0), Reading::Sure(tile("FR-76", 7.0, 0, 1)));
    assert_eq!(outcome, Outcome::Reserved { target: Target::Unit, name: "FR-76".into(), reserved: 9.0 });
    let said = outcome.notice_in(Lang::De);
    assert_eq!((said.kind, said.target), (NoticeKind::Unsure, Some(Target::Unit)));
    assert!(said.detail.contains("9 Stück sind für Aufträge vorgemerkt"), "{said:?}");
    assert_eq!(pool.held.borrow()[0].2, 9.0, "nothing was cut");
    let journal = Journal::load(&paths);
    assert!(journal.scans.is_empty() && journal.confirms.len() == 1);
    assert_eq!((journal.confirms[0].reserved, journal.confirms[0].pending.wanted), (9.0, 7.0));
    // The word: booked, and the waiting one is gone.
    let done = confirm_reserved(&paths, Some(&pool), 0, 2_000).expect("confirmed");
    let Outcome::Booked(scan) = done else { panic!("{done:?}") };
    assert_eq!((scan.before, scan.after), (Some(9.0), 7.0));
    assert_eq!(pool.held.borrow()[0].2, 7.0);
    let said = Outcome::Booked(scan).notice_in(Lang::De);
    assert!(said.detail.starts_with("FR-76, war 9\u{d7}, jetzt 7\u{d7}"), "{said:?}");
    assert!(Journal::load(&paths).confirms.is_empty());
    assert_eq!(confirm_reserved(&paths, Some(&pool), 0, 2_001), None);
    // Dropped: nothing is booked.
    pool.held.borrow_mut()[0].2 = 9.0;
    run(&same_round(&paths, Target::Unit, Some(&pool), 5_000), Reading::Sure(tile("FR-76", 5.0, 0, 1)));
    assert!(drop_confirm(&paths, 0));
    assert_eq!(pool.held.borrow()[0].2, 9.0);
    assert!(Journal::load(&paths).confirms.is_empty());
    // The scan that fits says so.
    let fits = run(&same_round(&paths, Target::Unit, Some(&pool), 9_000), Reading::Sure(tile("FR-76", 9.0, 0, 1)));
    let said = fits.notice_in(Lang::De);
    assert!(matches!(fits, Outcome::Same(_)) && said.detail.starts_with("FR-76, 9\u{d7} (passt)"), "{said:?}");
}

/// Without the right to write, as a logistician of that unit, nothing is planned; goods match by their quality.
#[test]
fn the_pool_books_only_with_the_right_and_by_quality() {
    let item = pieces("Parallax", 3.0);
    let lots = || vec![lot("Parallax", "Levski", 0, true, 1.0, Some(vec![entry("me", 1.0)]))];
    for (write, logistician) in [(false, true), (true, false), (false, false)] {
        assert_eq!(
            plan_for(&data(lots(), write, logistician), &item, Some(3.0), false),
            Err(UnitError::NoRight),
            "write {write}, logistician {logistician}"
        );
    }
    // A lot without its entries leaves a booking out unknown: no guess. Booking in needs no entry.
    let blind = data(vec![lot("Parallax", "Levski", 0, true, 5.0, None)], true, true);
    assert_eq!(plan_for(&blind, &pieces("Parallax", 3.0), Some(3.0), false), Err(UnitError::NoRight));
    assert!(plan_for(&blind, &item, Some(9.0), false).is_ok());
    // The quality tells the stacks apart.
    let scu = |quality| ScannedItem::new("Agricium", 5.0, Measure::Scu, Some(quality));
    let mixed = data(
        vec![
            lot("Agricium", "Levski", 500, false, 2.0, Some(vec![entry("me", 2.0)])),
            lot("Agricium", "Levski", 700, false, 8.0, Some(vec![entry("me", 8.0)])),
        ],
        true,
        true,
    );
    let low = plan_for(&mixed, &scu(500), Some(5.0), false).expect("plan");
    assert_eq!((low.before, op_all(&low)), (Some(2.0), vec![("book-in".to_string(), 3.0, String::new())]));
    let high = plan_for(&mixed, &scu(700), Some(5.0), false).expect("plan");
    assert_eq!((high.before, op_all(&high)), (Some(8.0), vec![("book-out".to_string(), 3.0, "e-me".to_string())]));
}

/// The decimals of a card reach the booking of the unit stock as they are: 0.041 SCU is no 0.
#[test]
fn the_pool_books_the_decimals_of_a_card() {
    let item = ScannedItem::new("Agricium", 0.041, Measure::Scu, Some(500));
    // The item is known to the unit by another quality, so the booking needs no lookup.
    let known = data(vec![lot("Agricium", "Levski", 700, false, 8.0, Some(vec![entry("me", 8.0)]))], true, true);
    let planned = plan_for(&known, &item, Some(0.041), false).expect("plan");
    let ops = op_all(&planned);
    assert_eq!(ops.len(), 1, "{ops:?}");
    assert_eq!((ops[0].0.as_str(), ops[0].1), ("book-in", 0.041));
}

/// Fingerprints are compared by likeness: the same picture with a part hidden by the card is the same tile,
/// another picture is not, and bytes that are no fingerprint show no tile.
#[test]
fn tile_prints_are_compared_by_likeness() {
    let whole = print_bytes(7);
    let mut covered = crate::stock_scan::TilePrint::from_bytes(&whole).expect("a print");
    for cell in covered.cells.iter_mut().skip(160) {
        *cell = None;
    }
    assert!(tile_prints_same(&whole, &covered.to_bytes()), "a part hidden by the card");
    assert!(!tile_prints_same(&whole, &print_bytes(8)), "another picture");
    assert!(!tile_prints_same(&7u64.to_le_bytes(), &7u64.to_le_bytes()), "no fingerprint");
    // The same tile found a few pixels apart is at the same place; the next tile is not.
    let a = ScannedItem { tile_rect: Some((100, 200, 80, 80)), ..tile("FR-76", 1.0, 0, 7) };
    let b = ScannedItem { tile_rect: Some((103, 198, 81, 80)), ..tile("FR-76", 1.0, 0, 7) };
    assert!(a.same_tile(&b));
    assert!(!a.same_tile(&tile("FR-76", 1.0, 1, 7)));
}
