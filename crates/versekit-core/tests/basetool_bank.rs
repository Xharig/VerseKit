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

//! The area bank of the sync against the stand-in of the Basetool in memory: accounts and own requests read,
//! raising, correcting and cancelling under kept keys, the cap of twenty, the queue of bank staff and its news, a
//! queue that fails, the bank switched off over there with its `Retry-After`, the seven days of memory, and that
//! nothing of the bank reaches the disk or the error log.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use versekit_core::basetool::bank::{self, BankArea, BankStore, Correction, Held, Kind, Raise, Write};
use versekit_core::basetool::connection::{Clock, system_clock};
use versekit_core::basetool::exchange_double::ExchangeDouble;
use versekit_core::basetool::scopes::{SETTING_BANK, SETTING_BANK_QUEUE, SETTING_STOCK};
use versekit_core::basetool::stock_area::StockArea;
use versekit_core::basetool::sync::{Phase, Setup, SyncHandle};
use versekit_core::jsonio::{Map, Value};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;

const RIGHTS: [&str; 5] =
    ["exchange.bank.read", "exchange.bank.request", "exchange.bank.queue", "exchange.stock.read", "exchange.stock.write"];

/// Every line the error log got, for all tests of this file.
static LOGGED: Mutex<Vec<String>> = Mutex::new(Vec::new());

struct Bench {
    dir: String,
    double: Arc<ExchangeDouble>,
    sync: SyncHandle,
    store: Arc<BankStore>,
    clock: Clock,
    skew: Arc<AtomicU64>,
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn bench(name: &str, switches: &[&str]) -> Bench {
    versekit_core::errors::set_sink(|label, text| {
        LOGGED.lock().unwrap_or_else(|e| e.into_inner()).push(format!("{label}: {text}"));
    });
    let dir = std::env::temp_dir().join(format!("versekit-bank-{}-{name}", std::process::id()));
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
    let store = BankStore::new();
    let skew = Arc::new(AtomicU64::new(0));
    let clock: Clock = {
        let (base, skew) = (system_clock(), skew.clone());
        Arc::new(move || base() + skew.load(Ordering::SeqCst) as f64)
    };
    let sync = SyncHandle::without_worker(Setup {
        conn: Arc::new(double.connection(clock.clone())),
        paths,
        clock: clock.clone(),
        version: "5.3.0".into(),
        own_account: Arc::new(|| Some("PlayerOne".to_string())),
        waker: Arc::new(|| {}),
        areas: vec![Box::new(StockArea), Box::new(BankArea { store: store.clone() })],
    });
    Bench { dir: data, double, sync, store, clock, skew }
}

impl Bench {
    fn now(&self) -> f64 {
        (self.clock)()
    }

    fn later(&self, seconds: u64) {
        self.skew.fetch_add(seconds, Ordering::SeqCst);
    }

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

    fn count(&self, area: &str, key: &str) -> Option<Value> {
        self.counts(area).iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    fn bank(&self) -> Arc<bank::Bank> {
        self.store.current(self.now()).1.expect("bank data")
    }

    fn write(&self, write: Write) -> Held {
        let known = self.store.current(self.now()).1.map(|b| b.requests.iter().map(|r| r.id.clone()).collect());
        Held::new(write, known.unwrap_or_default())
    }

    /// Every file the bench wrote, read as text.
    fn all_files(&self) -> String {
        fn walk(dir: &std::path::Path, out: &mut String) {
            for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if let Ok(bytes) = std::fs::read(&path) {
                    out.push_str(&String::from_utf8_lossy(&bytes));
                }
            }
        }
        let mut out = String::new();
        walk(std::path::Path::new(&self.dir), &mut out);
        out
    }
}

fn withdrawal(account: &str, amount: i64) -> Raise {
    Raise {
        kind: Kind::Withdrawal,
        account: account.into(),
        target: None,
        amount,
        note: "Tanken".into(),
        justification: "Freitag".into(),
    }
}

#[test]
fn accounts_and_own_requests_come_in_and_a_request_is_raised_corrected_and_cancelled() {
    let b = bench("antraege", &[SETTING_BANK]);
    let account = b.double.add_account("KB-0042", "Staffel Nord", 1_250_000, true);
    b.pass();
    let read = b.bank();
    assert_eq!(read.accounts.len(), 1);
    assert_eq!(read.accounts[0].balance, 1_250_000);
    assert!(read.requests.is_empty());
    assert!(read.queue.is_none() && !read.not_staff, "the queue was not asked for");

    let conn = b.sync.connection();
    let written = bank::send(conn, &b.write(Write::Raise(withdrawal(&account, 15_000)))).expect("raised");
    assert_eq!((written.status.as_str(), written.version), ("PENDING", 0));
    b.pass();
    let request = b.bank().requests[0].clone();
    assert_eq!((request.id.as_str(), request.amount, request.pending()), (written.id.as_str(), 15_000, true));

    let correction = Correction {
        id: request.id.clone(),
        version: request.version,
        amount: 12_000,
        note: "Tanken".into(),
        justification: "Freitag".into(),
        target: None,
    };
    let held = b.write(Write::Correct(correction.clone()));
    // The answer of the correction is lost: sent again under its key it is not applied twice.
    b.double.lose_next_answer(&format!("/bank/requests/{}/correction", request.id));
    assert!(bank::send(conn, &held).is_err());
    let again = bank::send(conn, &held).expect("the same key gives the first answer");
    assert_eq!(again.version, 1, "applied once");
    // Another correction under a new key with the old version: refused.
    let stale = bank::send(conn, &b.write(Write::Correct(correction))).expect_err("stale");
    assert_eq!(stale.code, "VERSION_CONFLICT");
    assert!(bank::settled(&stale), "a clear refusal");
    b.pass();
    let request = b.bank().requests[0].clone();
    assert_eq!((request.amount, request.version), (12_000, 1));
    let cancel = b.write(Write::Cancel { id: request.id.clone(), version: request.version });
    bank::send(conn, &cancel).expect("cancelled");
    b.pass();
    assert_eq!(b.bank().requests[0].status, "CANCELLED");
    assert_eq!(b.bank().pending(), 0);
}

#[test]
fn a_lost_answer_of_a_raise_gives_the_same_request_once_and_the_feed_answers_it() {
    let b = bench("verloren", &[SETTING_BANK]);
    let account = b.double.add_account("KB-1", "KRT", 900, true);
    b.pass();
    let held = b.write(Write::Raise(withdrawal(&account, 500)));
    b.store.set_held(Some(held.clone()));
    b.double.lose_next_answer("/bank/requests");
    let conn = b.sync.connection();
    let lost = bank::send(conn, &held).expect_err("the answer was lost");
    assert!(!bank::settled(&lost), "no answer: the outcome is open");
    let again = bank::send(conn, &held).expect("same key, same request");
    assert_eq!(b.double.bank_requests().len(), 1);
    assert_eq!(again.status, "PENDING");
    // The same key with another body is refused by the Basetool.
    let reused = Held { write: Write::Raise(withdrawal(&account, 501)), ..held.clone() };
    assert_eq!(bank::send(conn, &reused).expect_err("reused").code, "IDEMPOTENCY_KEY_REUSED");
    // The pass that shows the request lets the held raise go.
    assert!(b.store.held().is_some());
    b.pass();
    assert!(b.store.held().is_none(), "the feed shows its request");
}

#[test]
fn twenty_pending_requests_close_the_door_for_a_new_one() {
    let b = bench("deckel", &[SETTING_BANK]);
    let account = b.double.add_account("KB-1", "KRT", 900, true);
    let conn = b.sync.connection();
    for _ in 0..20 {
        bank::send(conn, &b.write(Write::Raise(withdrawal(&account, 100)))).expect("raised");
    }
    b.pass();
    assert!(b.bank().cap_reached());
    let refused = bank::send(conn, &b.write(Write::Raise(withdrawal(&account, 100)))).expect_err("cap");
    assert_eq!(refused.code, bank::CAP);
    assert!(bank::settled(&refused));
    let first = b.double.bank_requests()[0].get("id").and_then(Value::as_str).expect("id").to_string();
    b.double.decide_bank_request(&first, "CONFIRMED");
    b.pass();
    assert!(!b.bank().cap_reached());
}

#[test]
fn the_queue_shows_only_for_bank_staff_and_tells_only_new_entries_once() {
    let b = bench("warteschlange", &[SETTING_BANK, SETTING_BANK_QUEUE]);
    b.pass();
    assert!(b.bank().not_staff, "no bank staff: the queue stays hidden");
    assert!(b.bank().queue.is_none());
    b.double.set_queue(Some(&["q1"]));
    b.pass();
    assert_eq!(b.store.news().round, 0, "the first reading is the stand, no news");
    assert_eq!(b.bank().queue.as_ref().map(bank::Queue::len), Some(1));
    assert_eq!(b.bank().queue.as_ref().unwrap().open[0].requester, None, "the withheld name stays withheld");
    b.pass();
    assert_eq!(b.store.news().round, 0, "nothing new: no notice");
    b.double.set_queue(Some(&["q1", "q2"]));
    b.pass();
    assert_eq!(b.store.news(), bank::News { round: 1, count: 1 });
    // Thrown away (switched off a while): the round does not go back.
    b.store.clear();
    assert_eq!(b.store.news().round, 1);
}

#[test]
fn a_queue_that_cannot_be_read_costs_neither_the_accounts_nor_the_requests() {
    let b = bench("warteschlange-kaputt", &[SETTING_BANK, SETTING_BANK_QUEUE]);
    b.double.add_account("KB-1", "KRT", 900, true);
    b.double.set_queue(Some(&["q1"]));
    b.double.fail("/bank/queue", 400, "SCHEMA_INVALID", None, 1);
    b.pass();
    assert_eq!(b.bank().accounts.len(), 1, "the accounts stay");
    assert!(b.bank().queue.is_none());
    assert_eq!(b.count("bank", "queue_error"), Some(Value::str("SCHEMA_INVALID")));
}

#[test]
fn the_bank_switched_off_over_there_waits_alone_and_keeps_what_was_read() {
    let b = bench("aus", &[SETTING_BANK, SETTING_STOCK]);
    b.double.add_account("KB-1", "KRT", 900, true);
    b.pass();
    b.double.set_bank(false);
    b.pass();
    let status = b.sync.status().get().1.expect("status");
    assert_eq!(status.phase, Phase::Ok, "the pass went through");
    assert_eq!(b.count("bank", "error").and_then(|v| v.as_str().map(str::to_string)).as_deref(), Some(bank::DISABLED));
    assert!(b.counts("stock").iter().all(|(k, _)| k != "error"), "the stock ran");
    assert!(b.store.disabled());
    assert_eq!(b.bank().accounts.len(), 1, "what was read stays");
    // Its Retry-After holds the bank alone: the next pass does not ask it.
    b.double.set_bank(true);
    let asked = b.double.seen().iter().filter(|l| l.starts_with("GET /bank/accounts")).count();
    b.pass();
    assert_eq!(b.count("bank", "waiting"), Some(Value::int(1)));
    assert_eq!(b.double.seen().iter().filter(|l| l.starts_with("GET /bank/accounts")).count(), asked);
    assert!(b.store.disabled(), "still said");
    // Another error does not take "switched off" back; only a pass that went through does.
    b.later(301);
    b.double.fail("/bank/accounts", 500, "INTERNAL", None, 1);
    b.pass();
    assert!(b.store.disabled());
    b.pass();
    assert!(!b.store.disabled());
}

#[test]
fn nothing_of_the_bank_reaches_the_disk_or_the_error_log_and_memory_forgets_after_seven_days() {
    let b = bench("speicher", &[SETTING_BANK]);
    let account = b.double.add_account("KB-7777", "Geheimkonto Zebra", 4_242_424, true);
    let conn = b.sync.connection();
    bank::send(conn, &b.write(Write::Raise(withdrawal(&account, 31_337)))).expect("raised");
    b.pass();
    assert_eq!(b.bank().requests.len(), 1);
    // An error of the bank: into the log with code and status only.
    b.double.fail("/bank/accounts", 403, "SCOPE_MISSING", None, 1);
    b.pass();
    let logged = LOGGED.lock().unwrap_or_else(|e| e.into_inner()).join("\n");
    assert!(logged.contains("SCOPE_MISSING"), "{logged}");
    for secret in ["Detail-Geheimnis", "korrelation-geheim"] {
        assert!(!logged.contains(secret), "{secret} in the error log: {logged}");
    }
    let disk = b.all_files();
    for secret in ["KB-7777", "Geheimkonto Zebra", "4242424", "31337", &account, "Detail-Geheimnis"] {
        assert!(!disk.contains(secret), "{secret} on the disk");
    }
    assert!(b.store.current(b.now() + bank::KEEP + 60.0).1.is_none(), "gone after seven days");
    b.pass();
    assert!(b.store.current(b.now()).1.is_some());
    assert!(b.sync.paths().set_setting(SETTING_BANK, Value::Bool(false)));
    assert!(b.sync.paths().set_setting(SETTING_STOCK, Value::Bool(true)));
    b.pass();
    assert!(b.store.current(b.now()).1.is_none());
}

#[test]
fn the_history_is_read_with_its_id_encoded_and_shown_again_for_minutes_only() {
    let b = bench("verlauf", &[SETTING_BANK]);
    let account = b.double.add_account("KB-1", "KRT", 900, true);
    let conn = b.sync.connection();
    let history = b.store.read_history(conn, &account, b.now()).expect("history");
    assert_eq!(history.bookings[0].note, account);
    assert!(b.store.history(&account, b.now()).is_some());
    assert!(b.store.history(&account, b.now() + bank::HISTORY_FRESH + 1.0).is_none(), "old: read anew");
    assert_eq!(bank::path_segment("a/b c?"), "a%2Fb%20c%3F");
}
