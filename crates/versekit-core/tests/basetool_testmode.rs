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

//! The Basetool in the test mode (a copy of the data, `--daten`), against the stand-in of the
//! Basetool in memory (`ExchangeDouble`): with the lock no request of any kind reaches the
//! transport, the key of the installation is neither opened nor deleted; with
//! `VERSEKIT_BASETOOL_IM_TEST=1` the same calls go through. Everything is written into throwaway
//! folders; nothing reaches the net.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use versekit_core::basetool::blueprint_area::{BlueprintArea, FileBlueprintHost};
use versekit_core::basetool::config::Config;
use versekit_core::basetool::connection::{Connection, system_clock};
use versekit_core::basetool::demand::{DemandArea, DemandStore};
use versekit_core::basetool::error::ApiError;
use versekit_core::basetool::exchange_double::{API, ExchangeDouble, ISSUER};
use versekit_core::basetool::guard::{JOURNAL_FILE, Journal};
use versekit_core::basetool::http::{Request, Response, Transport};
use versekit_core::basetool::key::{KeyFailure, KeyVault, StoredKey};
use versekit_core::basetool::orgstock::{self, OrgStock, OrgStockArea};
use versekit_core::basetool::orgunits::OrgUnitsArea;
use versekit_core::basetool::proof::DpopKey;
use versekit_core::basetool::scopes::{AREAS, SEND_ROUTES, SETTING_ORGSTOCK_WRITE};
use versekit_core::basetool::secrets::{MemorySecrets, REFRESH_SECRET, SecretStore};
use versekit_core::basetool::ship_area::ShipArea;
use versekit_core::basetool::stock_area::StockArea;
use versekit_core::basetool::sync::{Phase, Setup, SyncHandle};
use versekit_core::collection::NoCatalog;
use versekit_core::jsonio::Value;
use versekit_core::legacy::ENV_BASETOOL_KEYNAME;
use versekit_core::paths::{COPY_DATA_FOLDER, ENV_INSTALL_DIR, Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::testmode::{
    self, BASETOOL_LOCKED, ENV_BASETOOL_IN_TEST, ENV_BASETOOL_JOURNAL, ENV_BASETOOL_WRITE_MAX, WRITE_GUARD,
};

/// The key in memory, with how often it was opened and deleted.
#[derive(Default)]
struct CountingVault {
    inner: Option<StoredKey>,
    opened: AtomicUsize,
    deleted: AtomicUsize,
}

impl KeyVault for CountingVault {
    fn open(&self, create: bool) -> Result<Option<Arc<dyn DpopKey>>, KeyFailure> {
        self.opened.fetch_add(1, Ordering::SeqCst);
        self.inner.as_ref().expect("a store").open(create)
    }

    fn delete(&self) -> Result<(), KeyFailure> {
        self.deleted.fetch_add(1, Ordering::SeqCst);
        self.inner.as_ref().expect("a store").delete()
    }
}

struct Bench {
    root: std::path::PathBuf,
    paths: Arc<Paths>,
    double: Arc<ExchangeDouble>,
    vault: Arc<CountingVault>,
    sync: SyncHandle,
}

impl Drop for Bench {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The program's sync on a copy of the data (as `--daten` makes it) with every area switched on, bookings
/// included, against the stand-in. `vars` go on the host of the copy. The lock is the one the program
/// decides (`testmode::basetool_locked`).
fn bench(name: &str, vars: &[(&str, &str)]) -> Bench {
    bench_via(name, vars, |double| double)
}

/// `bench` with the transport wrapped (a spy that keeps what really went out).
fn bench_via(name: &str, vars: &[(&str, &str)], wrap: impl FnOnce(Arc<ExchangeDouble>) -> Arc<dyn Transport>) -> Bench {
    let root = versekit_core::testdir::fresh(&format!("basetool-testbetrieb-{name}"));
    std::fs::create_dir_all(root.join(COPY_DATA_FOLDER)).expect("data folder");
    // An empty game folder of its own: the logs of the machine are not read.
    let game = root.join("Spiel");
    std::fs::create_dir_all(&game).expect("game folder");
    let text = root.to_string_lossy().into_owned();
    let game = game.to_string_lossy().into_owned();
    let mut all = vec![(ENV_INSTALL_DIR, game.as_str())];
    all.extend_from_slice(vars);
    let paths = Arc::new(Paths::for_copy(Host::new(Flavor::native(), &all, &text), &text).expect("a copy"));
    for (setting, _) in AREAS {
        assert!(paths.set_setting(setting, Value::Bool(true)));
    }
    assert!(paths.set_setting(SETTING_ORGSTOCK_WRITE, Value::Bool(true)));
    let double = Arc::new(ExchangeDouble::new());
    let secrets = Arc::new(MemorySecrets::default());
    // The copy holds the token of the real installation.
    secrets.save(REFRESH_SECRET, "doppel-refresh").expect("token");
    let vault = Arc::new(CountingVault { inner: Some(StoredKey { secrets: secrets.clone() }), ..Default::default() });
    let config = Config {
        issuer: ISSUER.into(),
        api: API.into(),
        client_id: "versekit".into(),
        ca: String::new(),
        sandbox: true,
        overridden: true,
    };
    let clock = system_clock();
    let conn = Connection::new(config, wrap(double.clone()), secrets, vault.clone(), "5.0.0", clock.clone())
        // Wired as the program wires it (`Wire::system`): lock, kept key, write guard and journal from the host's
        // variables. The budget is the bench's own (the program shares one per run, `guard::run_budget`).
        .with_test_lock(testmode::basetool_locked(&paths))
        .with_kept_key(testmode::basetool_keeps_key(&paths))
        .with_write_guard(testmode::basetool_write_ceiling(&paths))
        .with_journal(Journal::from_env(&paths));
    let units = OrgUnitsArea::default();
    let blueprints = Arc::new(FileBlueprintHost::new(paths.clone(), Arc::new(NoCatalog)));
    let sync = SyncHandle::without_worker(Setup {
        conn: Arc::new(conn),
        paths: paths.clone(),
        clock,
        version: "5.0.0".into(),
        own_account: Arc::new(|| Some("PlayerOne".to_string())),
        waker: Arc::new(|| {}),
        areas: vec![
            Box::new(BlueprintArea::new(blueprints)),
            Box::new(StockArea),
            Box::new(units.clone()),
            Box::new(ShipArea::new(units.units.clone())),
            Box::new(DemandArea { store: DemandStore::new() }),
            Box::new(OrgStockArea { store: OrgStock::new() }),
        ],
    });
    Bench { root, paths, double, vault, sync }
}

/// Every way the program writes to the Basetool, called directly; the codes of the answers.
fn every_write(conn: &Connection) -> Vec<(String, Option<String>)> {
    let code = |result: Result<(), versekit_core::basetool::error::ApiError>| result.err().map(|e| e.code);
    let change_set = Value::Object(vec![("ops".into(), Value::Array(Vec::new()))]);
    let mut out = vec![
        ("label".to_string(), code(conn.label_installation("Testkopie").map(|_| ()))),
        ("account".to_string(), code(conn.account_check("PlayerOne").map(|_| ()))),
        ("login".to_string(), code(conn.start_login(&[]).map(|_| ()))),
    ];
    for route in SEND_ROUTES {
        out.push((route.to_string(), code(conn.request("POST", route, Some(&change_set), &[], Some("k")).map(|_| ()))));
    }
    let book_in = orgstock::op_book_in(
        "unit-1",
        "member-1",
        Value::Object(vec![("name".into(), Value::str("Iron"))]),
        Value::Object(vec![("name".into(), Value::str("Area18"))]),
        500,
        false,
        2.0,
        "scu",
        "",
        false,
    );
    out.push(("book-in".to_string(), code(orgstock::send(conn, vec![book_in]).map(|_| ()))));
    out.push(("demand".to_string(), code(DemandStore::new().refresh(conn, 0.0).map(|_| ()))));
    out
}

#[test]
fn in_the_test_mode_no_request_reaches_the_basetool_and_the_key_stays() {
    let bench = bench("gesperrt", &[]);
    let conn = bench.sync.connection().clone();
    assert!(testmode::on_copy(&bench.paths) && conn.test_locked(), "a copy of the data is locked");

    // A whole pass with every area: refused before anything goes out, and it does not try again by itself.
    let refused = bench.sync.pass_now().expect_err("no pass in the test mode");
    assert_eq!(refused.code, BASETOOL_LOCKED);
    let status = bench.sync.status().get().1.expect("status");
    assert_eq!((status.phase, status.code.as_str()), (Phase::Error, BASETOOL_LOCKED));

    // Each write on its own: the label, the account check, the change sets of the three areas, a booking in the
    // storage of the unit, the demand, a login.
    for (what, code) in every_write(&conn) {
        assert_eq!(code.as_deref(), Some(BASETOOL_LOCKED), "{what}");
    }
    // Disconnecting: nothing revoked at the server, the key of the real installation stays, the token of this
    // copy goes.
    bench.sync.disconnect().wait(std::time::Duration::from_secs(10)).expect("disconnected");
    assert!(!conn.connected(), "the copy forgot its token");
    assert_eq!(bench.vault.opened.load(Ordering::SeqCst), 0, "the key was not even opened");
    assert_eq!(bench.vault.deleted.load(Ordering::SeqCst), 0, "the key was not deleted");
    assert_eq!(bench.double.seen(), Vec::<String>::new(), "not one request reached the transport");
}

/// A key name of its own for the copy (`SC_BP_BASETOOL_KEYNAME`), as the test against the real Basetool sets it.
const OWN_KEY: (&str, &str) = (ENV_BASETOOL_KEYNAME, "VerseKit Echttest");

#[test]
fn opened_for_a_test_the_same_calls_reach_the_basetool() {
    // A ceiling the calls never reach: without one the opened copy only fetches.
    let bench = bench("offen", &[(ENV_BASETOOL_IN_TEST, "1"), OWN_KEY, (ENV_BASETOOL_WRITE_MAX, "1000")]);
    let conn = bench.sync.connection().clone();
    assert!(testmode::on_copy(&bench.paths) && !conn.test_locked() && !conn.key_kept());
    bench.sync.pass_now().expect("a pass");
    let codes = every_write(&conn);
    assert!(codes.iter().all(|(_, code)| code.as_deref() != Some(BASETOOL_LOCKED)), "{codes:?}");
    bench.sync.disconnect().wait(std::time::Duration::from_secs(10)).expect("disconnected");
    assert_eq!(bench.vault.deleted.load(Ordering::SeqCst), 1, "a key of its own is deleted as before");
    let seen = bench.double.seen();
    for wanted in
        ["POST /me/installation", "POST /me/account-check", "POST /me/blueprints/changes", "GET /me/blueprints"]
    {
        assert!(seen.iter().any(|line| line.starts_with(wanted)), "{wanted}: {seen:?}");
    }
    assert!(seen.iter().any(|line| line.starts_with("POST /org-stock/changes")), "the booking went out: {seen:?}");
}

#[test]
fn a_copy_with_the_key_of_the_real_installation_never_deletes_it() {
    // Opened for a test without a key name of its own: the key is the one of the player's own program.
    let bench = bench("schluessel-geteilt", &[(ENV_BASETOOL_IN_TEST, "1")]);
    let conn = bench.sync.connection().clone();
    assert!(!conn.test_locked() && conn.key_kept());
    bench.sync.pass_now().expect("a pass");
    assert!(bench.vault.opened.load(Ordering::SeqCst) > 0, "the key is used: requests went out");
    // The Basetool says the installation is revoked: the token goes, the key stays.
    bench.double.fail("/me/blueprints", 401, "INSTALLATION_REVOKED", None, 1);
    let revoked = conn.request("GET", "/me/blueprints", None, &[], None).expect_err("revoked");
    assert_eq!(revoked.code, "INSTALLATION_REVOKED");
    assert!(!conn.connected(), "the token of the copy went");
    assert_eq!(bench.vault.deleted.load(Ordering::SeqCst), 0, "INSTALLATION_REVOKED left the key");
    // "Disconnect" after a new token: the token goes, the key is still there.
    conn.secrets().save(REFRESH_SECRET, "doppel-refresh").expect("token again");
    bench.sync.disconnect().wait(std::time::Duration::from_secs(10)).expect("disconnected");
    assert!(!conn.connected());
    assert_eq!(bench.vault.deleted.load(Ordering::SeqCst), 0, "disconnecting left the key");
    // Counter-check: the same with a key name of its own deletes the key, on revocation and on "Disconnect".
    drop(bench);
    let own = self::bench("schluessel-eigen", &[(ENV_BASETOOL_IN_TEST, "1"), OWN_KEY]);
    let own_conn = own.sync.connection().clone();
    own.sync.pass_now().expect("a pass");
    own.double.fail("/me/blueprints", 401, "INSTALLATION_REVOKED", None, 1);
    let _ = own_conn.request("GET", "/me/blueprints", None, &[], None);
    assert_eq!(own.vault.deleted.load(Ordering::SeqCst), 1, "INSTALLATION_REVOKED deletes a key of its own");
    own_conn.secrets().save(REFRESH_SECRET, "doppel-refresh").expect("token again");
    own.sync.disconnect().wait(std::time::Duration::from_secs(10)).expect("disconnected");
    assert_eq!(own.vault.deleted.load(Ordering::SeqCst), 2, "disconnecting deletes a key of its own");
}

// ------------------------------------------------- write guard and journal (a test against the real Basetool)

/// A change set with `n` instructions.
fn changes(n: usize) -> Value {
    let op = |i: usize| {
        Value::Object(vec![
            ("op".into(), Value::str("add")),
            ("ref".into(), Value::Object(vec![("bt".into(), Value::str(format!("bt-{i}")))])),
        ])
    };
    Value::Object(vec![("ops".into(), Value::Array((0..n).map(op).collect()))])
}

/// Whether a request got past the write guard (it may still fail for other reasons).
fn passed<T>(result: Result<T, ApiError>) -> bool {
    result.err().is_none_or(|e| e.code != WRITE_GUARD)
}

fn send_changes(conn: &Connection, route: &str, n: usize) -> Result<(), ApiError> {
    conn.request("POST", route, Some(&changes(n)), &[], Some("k")).map(|_| ())
}

const OPEN: (&str, &str) = (ENV_BASETOOL_IN_TEST, "1");

#[test]
fn the_write_guard_refuses_a_send_over_the_ceiling_before_it_leaves() {
    let bench = bench("wache-eins", &[OPEN, (ENV_BASETOOL_WRITE_MAX, " 1 ")]);
    let conn = bench.sync.connection().clone();
    assert_eq!(conn.write_guard(), Some(1));
    // Reading and the three harmless writes go.
    assert!(passed(conn.request("GET", "/me/blueprints", None, &[], None)));
    assert!(passed(conn.label_installation("Echttest")));
    assert!(passed(conn.account_check("PlayerOne")));
    let before = bench.double.seen().len();

    let refused = send_changes(&conn, "/me/blueprints/changes", 2).expect_err("two changes, ceiling one");
    assert_eq!((refused.code.as_str(), refused.status), (WRITE_GUARD, 0));
    assert_eq!(refused.path, "POST /me/blueprints/changes");
    assert_eq!(refused.action(), versekit_core::basetool::error::Action::Stop, "no retry in the background");
    assert_eq!(bench.double.seen().len(), before, "nothing reached the transport");
    assert_eq!(conn.write_left(), Some(1), "a refused send takes nothing");

    // Counter-check: one change goes, and with it the budget of the run is used up.
    assert!(passed(send_changes(&conn, "/me/blueprints/changes", 1)));
    assert!(bench.double.seen().iter().any(|line| line.starts_with("POST /me/blueprints/changes")));
    assert_eq!(conn.write_left(), Some(0));
    // Every further send of this run is refused, on every route, the one that went included.
    let before = bench.double.seen().len();
    for route in ["/me/blueprints/changes", "/me/stock/changes", "/me/ships/changes", "/org-stock/changes"] {
        assert_eq!(send_changes(&conn, route, 1).expect_err("used up").code, WRITE_GUARD, "{route}");
    }
    assert_eq!(bench.double.seen().len(), before, "nothing more reached the transport");
    // Reading and the harmless writes still go.
    assert!(passed(conn.request("GET", "/me/blueprints", None, &[], None)));
    assert!(passed(conn.label_installation("Echttest")));

    // Anything else that writes is refused as well: the list of what may go is closed.
    let before = bench.double.seen().len();
    for (method, path) in [("POST", "/me/blueprints/clear"), ("DELETE", "/me/blueprints"), ("PUT", "/me/installation")]
    {
        let error = conn.request(method, path, Some(&changes(0)), &[], None).expect_err("refused");
        assert_eq!(error.code, WRITE_GUARD, "{method} {path}");
    }
    assert_eq!(bench.double.seen().len(), before);
}

#[test]
fn a_ceiling_of_zero_or_one_that_is_no_number_only_fetches() {
    for value in ["0", "", "abc", "-1", "1.5", "1 2"] {
        let bench = bench("wache-null", &[OPEN, (ENV_BASETOOL_WRITE_MAX, value)]);
        let conn = bench.sync.connection().clone();
        assert_eq!(conn.write_guard(), Some(0), "{value:?}");
        for n in [0, 1] {
            let error = send_changes(&conn, "/me/blueprints/changes", n).expect_err("refused");
            assert_eq!(error.code, WRITE_GUARD, "{value:?} with {n} changes");
        }
        assert!(passed(conn.request("GET", "/me/blueprints", None, &[], None)), "{value:?} still reads");
        assert!(passed(conn.label_installation("Echttest")), "{value:?} still names the connection");
        assert!(!bench.double.seen().iter().any(|line| line.contains("/changes")), "{value:?}");
    }
}

#[test]
fn opened_without_the_variable_the_copy_only_fetches_and_notes_nothing() {
    let bench = bench("wache-keine", &[OPEN]);
    let conn = bench.sync.connection().clone();
    assert_eq!(conn.write_guard(), Some(0), "a lost variable falls back to fetching only");
    for n in [0, 1, 50] {
        assert_eq!(send_changes(&conn, "/me/blueprints/changes", n).expect_err("refused").code, WRITE_GUARD, "{n}");
    }
    assert_eq!(conn.request("DELETE", "/me/blueprints", None, &[], None).expect_err("refused").code, WRITE_GUARD);
    assert!(passed(conn.request("GET", "/me/blueprints", None, &[], None)), "reading still goes");
    assert!(passed(conn.label_installation("Echttest")), "naming the connection still goes");
    assert!(!bench.double.seen().iter().any(|line| line.contains("/changes")), "no change reached the transport");
    // No journal either: not a file of that name anywhere under the copy.
    fn find(folder: &std::path::Path, name: &str) -> bool {
        std::fs::read_dir(folder).into_iter().flatten().flatten().any(|entry| {
            let path = entry.path();
            path.file_name().is_some_and(|n| n == name) || (path.is_dir() && find(&path, name))
        })
    }
    assert!(!find(&bench.root, JOURNAL_FILE));
}

/// A transport that answers the first send of a change set with a nonce challenge, as the Basetool does while it
/// has given none for the address yet, and keeps every send that reached it.
struct NonceOnce {
    inner: Arc<ExchangeDouble>,
    asked: std::sync::atomic::AtomicBool,
    sends: Arc<std::sync::Mutex<Vec<Request>>>,
}

impl Transport for NonceOnce {
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        if request.method == "POST" && request.url.ends_with("/changes") {
            self.sends.lock().expect("lock").push(request.clone());
            if !self.asked.swap(true, Ordering::SeqCst) {
                return Ok(Response {
                    status: 401,
                    headers: vec![
                        ("DPoP-Nonce".into(), "api-n1".into()),
                        ("WWW-Authenticate".into(), r#"DPoP error="use_dpop_nonce""#.into()),
                    ],
                    body: br#"{"code": "DPOP_INVALID"}"#.to_vec(),
                });
            }
        }
        Transport::send(&*self.inner, request)
    }
}

#[test]
fn a_send_asked_again_for_a_nonce_or_a_new_token_takes_its_change_once() {
    let ceiling = [OPEN, (ENV_BASETOOL_WRITE_MAX, "1")];
    let key = |request: &Request| request.headers.iter().find(|(k, _)| k == "Idempotency-Key").map(|(_, v)| v.clone());

    // The nonce challenge: the same send goes twice on the line, the budget is drawn once.
    let sends: Arc<std::sync::Mutex<Vec<Request>>> = Arc::default();
    let bench = {
        let sends = sends.clone();
        bench_via("wache-nonce", &ceiling, move |inner| {
            Arc::new(NonceOnce { inner, asked: std::sync::atomic::AtomicBool::new(false), sends })
        })
    };
    let conn = bench.sync.connection().clone();
    send_changes(&conn, "/me/blueprints/changes", 1).expect("it goes on the second try");
    let sent = sends.lock().expect("lock").clone();
    assert_eq!(sent.len(), 2, "asked for a nonce, then sent again");
    assert_eq!(sent[0].body, sent[1].body, "the same change");
    assert_eq!(key(&sent[0]), key(&sent[1]), "with the same key");
    assert_eq!(conn.write_left(), Some(0), "the change was taken once");
    assert_eq!(send_changes(&conn, "/me/stock/changes", 1).expect_err("used up").code, WRITE_GUARD);
    assert_eq!(sends.lock().expect("lock").len(), 2, "no second change reached the transport");
    drop(bench);

    // UNAUTHENTICATED: the token is renewed, the send repeated, the budget drawn once.
    let bench = self::bench("wache-erneuern", &ceiling);
    let conn = bench.sync.connection().clone();
    bench.double.fail("/me/blueprints/changes", 401, "UNAUTHENTICATED", None, 1);
    send_changes(&conn, "/me/blueprints/changes", 1).expect("it goes after the renewal");
    assert_eq!(conn.write_left(), Some(0), "the change was taken once");
    let posts = || bench.double.seen().iter().filter(|line| line.contains("/changes")).count();
    let before = posts();
    assert!(before >= 1, "{:?}", bench.double.seen());
    assert_eq!(send_changes(&conn, "/me/ships/changes", 1).expect_err("used up").code, WRITE_GUARD);
    assert_eq!(posts(), before, "no second change reached the transport");
}

/// The transport that keeps every request and every answer in full, to compare with the journal.
struct Spy {
    inner: Arc<ExchangeDouble>,
    sent: Arc<std::sync::Mutex<Vec<Request>>>,
    answers: Arc<std::sync::Mutex<Vec<Vec<u8>>>>,
}

/// The code to type and the device code the spy's login answers with (the stand-in has no login of its own).
const USER_CODE: &str = "QXJT-7KPD";
const DEVICE_CODE: &str = "geraete-geheim-4711";

impl Transport for Spy {
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        self.sent.lock().expect("lock").push(request.clone());
        let answer = if request.method == "POST" && request.url == format!("{ISSUER}/device") {
            let body = format!(
                r#"{{"device_code": "{DEVICE_CODE}", "user_code": "{USER_CODE}", "verification_uri": "{ISSUER}/device", "verification_uri_complete": "{ISSUER}/device?user_code={USER_CODE}", "expires_in": 600, "interval": 5}}"#
            );
            Ok(Response { status: 200, headers: Vec::new(), body: body.into_bytes() })
        } else {
            Transport::send(&*self.inner, request)
        };
        if let Ok(reply) = &answer {
            self.answers.lock().expect("lock").push(reply.body.clone());
        }
        answer
    }
}

/// The values of `keys` anywhere in the JSON `text` (the codes and tokens of a login).
fn values_of(text: &[u8], keys: &[&str], out: &mut Vec<String>) {
    fn walk(value: &Value, keys: &[&str], out: &mut Vec<String>) {
        match value {
            Value::Object(fields) => {
                for (key, inner) in fields {
                    match inner {
                        Value::Str(text) if keys.contains(&key.as_str()) && !text.is_empty() => out.push(text.clone()),
                        _ => walk(inner, keys, out),
                    }
                }
            }
            Value::Array(items) => items.iter().for_each(|item| walk(item, keys, out)),
            _ => {}
        }
    }
    if let Ok(value) = versekit_core::jsonio::loads(&String::from_utf8_lossy(text)) {
        walk(&value, keys, out);
    }
}

#[test]
fn the_journal_notes_only_the_allowed_facts_and_never_a_token() {
    const BODY_TOKEN: &str = "eyJBODYTOKEN.SECRETBODY.sig";
    const QUERY_TOKEN: &str = "QUERYSECRET987";
    let sent: Arc<std::sync::Mutex<Vec<Request>>> = Arc::default();
    let answers: Arc<std::sync::Mutex<Vec<Vec<u8>>>> = Arc::default();
    let vars = [OPEN, (ENV_BASETOOL_JOURNAL, "1"), (ENV_BASETOOL_WRITE_MAX, "1")];
    let bench = {
        let (sent, answers) = (sent.clone(), answers.clone());
        bench_via("journal", &vars, move |inner| Arc::new(Spy { inner, sent, answers }))
    };
    let conn = bench.sync.connection().clone();
    // A login (its code to type and its device code come back in the answer), a read with a secret in its
    // query, a send of one change that carries a secret token in its body, a send over the ceiling, and the
    // writes of the connection with the player's handle.
    let login = conn.start_login(&[]).expect("a code to type");
    assert_eq!(login.user_code, USER_CODE);
    let _ = conn.request("GET", "/me/blueprints", None, &[("cursor", QUERY_TOKEN)], None);
    let body = Value::Object(vec![(
        "ops".into(),
        Value::Array(vec![Value::Object(vec![
            ("op".into(), Value::str("add")),
            ("note".into(), Value::str(BODY_TOKEN)),
        ])]),
    )]);
    let _ = conn.request("POST", "/me/blueprints/changes", Some(&body), &[], Some("k"));
    let refused = conn.request("POST", "/me/stock/changes", Some(&changes(2)), &[], Some("k")).expect_err("refused");
    assert_eq!(refused.code, WRITE_GUARD);
    let _ = conn.label_installation("Echttest");
    let _ = conn.account_check("PlayerOne");

    let file = Journal::from_env(&bench.paths).expect("the journal is on").file().to_path_buf();
    let text = std::fs::read_to_string(&file).expect("the journal was written");
    let lines: Vec<&str> = text.lines().collect();
    let has = |needle: &[&str]| lines.iter().any(|l| needle.iter().all(|n| l.contains(n)));
    assert!(has(&["GET ", "/me/blueprints", "status=200", "abgewiesen=nein"]), "{text}");
    assert!(
        has(&["POST ", "/me/blueprints/changes", "status=200", "abgewiesen=nein", "aenderungen=1", "rest=0"]),
        "{text}"
    );
    assert!(has(&["POST ", "/me/stock/changes", "status=0", "abgewiesen=ja", "aenderungen=2", "rest=0"]), "{text}");
    assert!(has(&["POST ", "/me/installation", "abgewiesen=nein"]), "{text}");
    assert!(has(&["POST ", "/me/account-check", "abgewiesen=nein"]), "{text}");
    assert!(has(&["POST ", "token"]), "the renewal of the token is noted as well: {text}");
    assert!(has(&["POST ", "device"]), "the login is noted as well: {text}");
    assert!(lines.iter().all(|l| l.contains(" dauer_ms=") && l.contains(" gesendet_b=")), "{text}");
    // The size is the size of the body that went out, never the body.
    let requests = sent.lock().expect("lock").clone();
    let send = requests.iter().find(|r| r.url.ends_with("/me/blueprints/changes")).expect("the send went out");
    let size = send.body.as_ref().map_or(0, Vec::len);
    assert!(size > 0 && has(&["/me/blueprints/changes", &format!("gesendet_b={size} ")]), "{size}: {text}");
    assert!(!has(&["GET ", "/me/blueprints", "empfangen_b=0 "]), "the size of the answer is noted: {text}");

    // The journal against everything that really went and came: not one value of a header, not one body, not
    // one secret of the query or the body, not the code to type, the device code or a token of an answer, not
    // the player's handle.
    assert!(requests.iter().any(|r| r.headers.iter().any(|(k, v)| k == "DPoP" && v.starts_with("eyJ"))));
    assert!(requests.iter().any(|r| r.headers.iter().any(|(k, v)| k == "Authorization" && v.starts_with("DPoP "))));
    let mut secrets: Vec<String> =
        vec![USER_CODE.into(), DEVICE_CODE.into(), BODY_TOKEN.into(), QUERY_TOKEN.into(), "doppel-refresh".into()];
    for request in &requests {
        secrets.extend(request.headers.iter().filter(|(_, v)| v.len() >= 6).map(|(_, v)| v.clone()));
        secrets.extend(request.headers.iter().filter_map(|(_, v)| v.strip_prefix("DPoP ").map(str::to_string)));
        secrets.extend(request.body.iter().filter(|b| b.len() >= 6).map(|b| String::from_utf8_lossy(b).into_owned()));
    }
    let keys = ["user_code", "device_code", "access_token", "refresh_token", "verification_uri_complete"];
    let before = secrets.len();
    for answer in answers.lock().expect("lock").iter() {
        values_of(answer, &keys, &mut secrets);
    }
    assert!(secrets.len() >= before + 3, "the answers held a code to type, a device code and tokens");
    for secret in &secrets {
        assert!(!text.contains(secret.as_str()), "the journal holds a secret: {secret}");
    }
    for word in
        ["eyJ", "Bearer", "access_token", "refresh_token", "Authorization", "idempotency", "cursor=", "PlayerOne"]
    {
        assert!(!text.contains(word), "the journal holds {word:?}");
    }
}
