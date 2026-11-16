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

//! The lists of an org in the hub: fetched in the background, held in memory
//! only, one fetch per org, the tokens kept after every fetch, nothing asked
//! of an org on another computer while the net is switched off.

use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::{Condvar, atomic::Ordering as Order};

use versekit_core::basetool::error::ApiError;
use versekit_core::basetool::http::Response;
use versekit_core::orgstd::discovery::Origin;
use versekit_core::orgstd::hub::{Kind, OrgData};
use versekit_core::orgstd::{E_GONE, E_OFFLINE};

use super::*;

const KEY_TWO: &str = "schluessel-der-zweiten-0002";

/// A transport that records every request and can hold the request for the stock until it is let go.
struct Probe {
    inner: Arc<dyn Transport>,
    seen: Mutex<Vec<String>>,
    /// The bodies of requests with one, and the `Idempotency-Key` of those that carry one.
    sent: Mutex<Vec<String>>,
    idempotency: Mutex<Vec<String>>,
    hold: Mutex<bool>,
    arrived: AtomicBool,
    released: Condvar,
}

impl Probe {
    fn over(inner: Arc<dyn Transport>) -> Arc<Probe> {
        Arc::new(Probe {
            inner,
            seen: Mutex::new(Vec::new()),
            sent: Mutex::new(Vec::new()),
            idempotency: Mutex::new(Vec::new()),
            hold: Mutex::new(false),
            arrived: AtomicBool::new(false),
            released: Condvar::new(),
        })
    }

    fn real() -> Arc<Probe> {
        Probe::over(transport())
    }

    fn count(&self, part: &str) -> usize {
        self.seen.lock().unwrap().iter().filter(|line| line.contains(part)).count()
    }

    fn hold_stock(&self) {
        *self.hold.lock().unwrap() = true;
    }

    fn let_go(&self) {
        *self.hold.lock().unwrap() = false;
        self.released.notify_all();
    }

    fn client(self: &Arc<Self>) -> OrgClient {
        OrgClient::new(self.clone(), "5.4.0")
    }
}

impl Transport for Probe {
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        self.seen.lock().unwrap().push(format!("{} {}", request.method, request.url));
        if let (Some(body), true) = (&request.body, request.method != "GET") {
            self.sent.lock().unwrap().push(String::from_utf8_lossy(body).into_owned());
        }
        if let Some((_, key)) = request.headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("Idempotency-Key")) {
            self.idempotency.lock().unwrap().push(key.clone());
        }
        if request.method == "GET" && request.url.split('?').next().is_some_and(|u| u.ends_with("/stock")) {
            self.arrived.store(true, Order::SeqCst);
            let mut hold = self.hold.lock().unwrap();
            while *hold {
                hold = self.released.wait(hold).unwrap();
            }
        }
        self.inner.send(request)
    }
}

impl Probe {
    /// The bodies of the requests that sent something.
    fn bodies(&self) -> Vec<String> {
        self.sent.lock().unwrap().clone()
    }

    fn keys(&self) -> Vec<String> {
        self.idempotency.lock().unwrap().clone()
    }
}

/// A transport with no net.
struct Dead;

impl Transport for Dead {
    fn send(&self, _: &Request) -> Result<Response, ApiError> {
        Err(ApiError::network("test"))
    }
}

fn hub_over(folder: &std::path::Path, client: OrgClient, offline: bool) -> Hub {
    let store = Store::in_folder(&folder.to_string_lossy());
    Hub::with_offline(client, store, system_clock(), offline)
}

/// Logs the hub in through the panel with the rights `scopes`.
fn log_in(hub: &Hub, server: &Server, url: &str, scopes: Option<&[&str]>) {
    hub.connect(url);
    let view = wait_for(hub, |v| matches!(v.login, Some(Login::Code { .. })));
    let Some(Login::Code { user_code, .. }) = view.login else { unreachable!() };
    assert!(server.approve(&user_code, "m1", scopes));
    wait_for(hub, |v| matches!(v.login, Some(Login::Connected { .. })));
}

fn wait_data(hub: &Hub, id: &str, done: impl Fn(&OrgData) -> bool) -> OrgData {
    let start = Instant::now();
    loop {
        let data = hub.data(id);
        if done(&data) {
            return data;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "waited in vain: {data:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn seed_other_member(url: &str) {
    let client = client();
    let mut other = connect_as(url, KEY_TWO, None);
    let items =
        [json!({ "item": "Zzyzx-Ware-4711", "quantity": 7, "unit": "scu", "quality": 321, "location": "Zzyzx-Ort" })];
    client.share_stock(&mut other, &items, now()).unwrap();
    // A server that offers only the stock refuses the blueprints; the stock is what counts there.
    let _ = client.share_blueprints(&mut other, &["Zzyzx-Plan".into()], now());
}

#[test]
fn the_hub_fetches_the_three_lists_checked_and_keeps_them_in_memory() {
    let (server, url) = start();
    seed_other_member(&url);
    let dir = versekit_core::testdir::Dir::new("orga-daten");
    let hub = hub_over(dir.path(), client(), false);
    log_in(&hub, &server, &url, None);
    assert_eq!(hub.data(&url), OrgData::default(), "nothing is fetched until a page asks");
    for kind in Kind::ALL {
        assert!(hub.refresh(&url, kind));
    }
    let data = wait_data(&hub, &url, |d| {
        !d.loading()
            && d.stock.fetched_at.is_some()
            && d.blueprints.fetched_at.is_some()
            && d.orders.fetched_at.is_some()
    });
    assert_eq!(data.stock.rows.len(), 1);
    let lot = &data.stock.rows[0];
    assert_eq!(
        (lot.item.as_str(), lot.quantity, lot.quality, lot.location.as_str(), lot.member.as_str()),
        ("Zzyzx-Ware-4711", 7.0, Some(321), "Zzyzx-Ort", "Zweite")
    );
    assert_eq!(data.blueprints.rows[0].name, "Zzyzx-Plan");
    assert_eq!(data.orders.rows[0].title, "Erz");
    assert!(data.stock.error.is_none() && data.stock.dropped == 0);
    // Nothing of other members lies in any file of the connection's folder.
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let bytes = std::fs::read(entry.unwrap().path()).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(!text.contains("Zzyzx"), "a member's data reached the disk");
    }
}

#[test]
fn a_list_is_only_fetched_with_its_right_and_a_missing_right_at_the_server_keeps_the_others() {
    let (server, url) = start();
    seed_other_member(&url);
    let dir = versekit_core::testdir::Dir::new("orga-daten-rechte");
    let hub = hub_over(dir.path(), client(), false);
    log_in(&hub, &server, &url, Some(&["stock:read"]));
    assert!(!hub.refresh(&url, Kind::Blueprints), "no right, no request");
    assert!(!hub.refresh(&url, Kind::Orders));
    assert_eq!(hub.data(&url).blueprints, Default::default());
    assert!(hub.refresh(&url, Kind::Stock));
    wait_data(&hub, &url, |d| d.stock.fetched_at.is_some());
    // The stored rights claim more than the server granted: the refusal of the server shows as a missing right.
    let mut stored = Store::in_folder(&dir.path().to_string_lossy()).load().unwrap();
    stored[0].scopes = versekit_core::orgstd::SCOPES.iter().map(|s| s.to_string()).collect();
    Store::in_folder(&dir.path().to_string_lossy()).save(&stored).unwrap();
    let again = hub_over(dir.path(), client(), false);
    assert!(again.refresh(&url, Kind::Stock));
    wait_data(&again, &url, |d| d.stock.fetched_at.is_some());
    assert!(again.refresh(&url, Kind::Blueprints));
    let data = wait_data(&again, &url, |d| d.blueprints.error.is_some());
    assert_eq!(data.blueprints.error.map(|e| (e.code, e.status)), Some((E_SCOPE, 403)));
    assert_eq!(data.stock.rows.len(), 1, "the stock stays");
    assert!(data.blueprints.pause_left(now()) > 0, "no new fetch right away");
    assert!(!again.refresh(&url, Kind::Blueprints), "the pause holds");
}

#[test]
fn only_one_fetch_runs_per_org_so_the_refresh_token_is_never_used_twice() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-daten-eins");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    server.expire_access();
    for kind in Kind::ALL {
        assert!(hub.refresh(&url, kind));
    }
    let data = wait_data(&hub, &url, |d| !d.loading());
    for error in [data.stock.error, data.blueprints.error, data.orders.error] {
        assert_eq!(error, None, "a fetch lost the connection: {data:?}");
    }
    assert!(
        data.stock.fetched_at.is_some() && data.blueprints.fetched_at.is_some() && data.orders.fetched_at.is_some()
    );
    assert_eq!(probe.count("/token"), 1 + 1, "the login and one refresh, not one per list");
    assert!(!hub.view().connections.is_empty());
    // The refreshed tokens were kept: the stored connection is still good.
    hub.check(&url);
    let view = wait_for(&hub, |v| v.busy.is_empty() && !v.outcomes.is_empty());
    assert_eq!(view.outcomes, [(url.clone(), Outcome::Checked)]);
}

#[test]
fn a_second_fetch_of_the_same_list_is_not_started_while_one_runs() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-daten-zwei");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    probe.hold_stock();
    assert!(hub.refresh(&url, Kind::Stock));
    assert!(!hub.refresh(&url, Kind::Stock));
    assert!(!hub.ensure(&url, Kind::Stock, 0));
    assert!(hub.working());
    probe.let_go();
    wait_data(&hub, &url, |d| d.stock.fetched_at.is_some());
    assert_eq!(probe.count("GET ") - probe.count("/me") - probe.count("/.well-known"), 1);
}

#[test]
fn a_disconnect_during_a_fetch_stays_a_disconnect_and_the_new_token_is_given_back() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-daten-trennen");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    // The access token is run out: the fetch swaps the refresh token first and stops at the stock only then.
    make_stale(dir.path());
    probe.hold_stock();
    assert!(hub.refresh(&url, Kind::Stock));
    let start_wait = Instant::now();
    while !probe.arrived.load(Order::SeqCst) {
        assert!(start_wait.elapsed() < Duration::from_secs(20), "the fetch never reached the stock");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(probe.count("/token"), 2, "the login and the swap of the refresh token came before the stock");
    hub.disconnect(&url);
    wait_for(&hub, |v| v.busy.is_empty() && v.connections.is_empty());
    probe.let_go();
    // Two tokens are given back: that of the disconnect and the new one of the fetch.
    let start_wait = Instant::now();
    while probe.count("/revoke") < 2 {
        assert!(start_wait.elapsed() < Duration::from_secs(20), "the new token was never given back");
        std::thread::sleep(Duration::from_millis(20));
    }
    let start_wait = Instant::now();
    while server.refresh_count() > 0 {
        assert!(start_wait.elapsed() < Duration::from_secs(20), "the token of the fetch stays on the server");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(Store::in_folder(&dir.path().to_string_lossy()).load().unwrap().is_empty(), "the entry came back");
    assert!(hub.view().connections.is_empty());
    assert_eq!(hub.data(&url), OrgData::default(), "the data of a disconnected org is gone");
}

/// Runs the access token of the stored connection out, so that the next request swaps the refresh token first.
fn make_stale(folder: &std::path::Path) {
    let store = Store::in_folder(&folder.to_string_lossy());
    let mut list = store.load().unwrap();
    list[0].access_expires = 0;
    store.save(&list).unwrap();
}

#[test]
fn a_fetch_does_not_write_its_tokens_over_a_login_that_has_the_same_refresh_token_but_is_another_connection() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-daten-gleich");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    make_stale(dir.path());
    probe.hold_stock();
    assert!(hub.refresh(&url, Kind::Stock));
    while !probe.arrived.load(Order::SeqCst) {
        std::thread::sleep(Duration::from_millis(10));
    }
    // The stored entry keeps its refresh token but is another connection (made at another time).
    let store = Store::in_folder(&dir.path().to_string_lossy());
    let mut list = store.load().unwrap();
    list[0].connected_at += 1;
    store.save(&list).unwrap();
    let kept = store.load().unwrap();
    probe.let_go();
    let start_wait = Instant::now();
    while hub.working() || probe.count("/revoke") < 1 {
        assert!(start_wait.elapsed() < Duration::from_secs(20), "the fetch never ended");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(store.load().unwrap(), kept, "the fetch wrote its tokens into another connection");
}

#[test]
fn a_fetch_does_not_write_its_tokens_over_a_newer_login() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-daten-neu");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    server.expire_access();
    probe.hold_stock();
    assert!(hub.refresh(&url, Kind::Stock));
    while !probe.arrived.load(Order::SeqCst) {
        std::thread::sleep(Duration::from_millis(10));
    }
    log_in(&hub, &server, &url, None);
    let newer = Store::in_folder(&dir.path().to_string_lossy()).load().unwrap();
    probe.let_go();
    wait_data(&hub, &url, |d| !d.loading());
    let after = Store::in_folder(&dir.path().to_string_lossy()).load().unwrap();
    assert_eq!(after, newer, "the older fetch left the newer login alone");
    hub.check(&url);
    let view = wait_for(&hub, |v| v.busy.is_empty() && !v.outcomes.is_empty());
    assert_eq!(view.outcomes, [(url.clone(), Outcome::Checked)]);
}

fn memory_connection(address: &str) -> Connection {
    let origin = Origin::parse(address).unwrap();
    let api = format!("{address}/versekit/v1");
    Connection {
        discovery: Discovery {
            origin,
            name: "Fern".into(),
            tag: "FRN".into(),
            homepage: None,
            version: "1".into(),
            api_base: api.clone(),
            device_endpoint: format!("{api}/device/code"),
            token_endpoint: format!("{api}/token"),
            revocation_endpoint: None,
            scopes: versekit_core::orgstd::SCOPES.iter().map(|s| s.to_string()).collect(),
        },
        member: "Vega".into(),
        scopes: versekit_core::orgstd::SCOPES.iter().map(|s| s.to_string()).collect(),
        access_token: "x".into(),
        access_expires: now() + 3000,
        refresh_token: "y".into(),
        connected_at: 1,
        checked_at: None,
    }
}

#[test]
fn with_the_net_switched_off_nothing_is_asked_of_an_org_on_another_computer() {
    let address = "https://orga.example.org";
    let counting = Probe::over(Arc::new(Dead));
    let store = Store::memory(vec![memory_connection(address)]);
    let hub = Hub::with_offline(counting.client(), store, system_clock(), true);
    assert!(hub.view().offline);
    for kind in Kind::ALL {
        assert!(!hub.refresh(address, kind));
    }
    assert_eq!(counting.seen.lock().unwrap().len(), 0, "a request went out");
    let data = hub.data(address);
    assert_eq!(data.stock.error.map(|e| e.code), Some(E_OFFLINE));
    assert!(data.stock.rows.is_empty() && !hub.working());
    // The same org with the net switched on is asked (and Dead answers with an error).
    let on =
        Hub::with_offline(counting.client(), Store::memory(vec![memory_connection(address)]), system_clock(), false);
    assert!(on.refresh(address, Kind::Stock));
    let data = wait_data(&on, address, |d| d.stock.error.is_some());
    assert_eq!(data.stock.error.map(|e| e.code), Some(versekit_core::orgstd::E_NETWORK));
    assert!(!counting.seen.lock().unwrap().is_empty());
}

#[test]
fn the_own_computer_stays_reachable_with_the_net_switched_off() {
    let (server, url) = start();
    seed_other_member(&url);
    let dir = versekit_core::testdir::Dir::new("orga-daten-lokal");
    let hub = hub_over(dir.path(), client(), true);
    log_in(&hub, &server, &url, None);
    assert!(hub.refresh(&url, Kind::Stock));
    let data = wait_data(&hub, &url, |d| d.stock.fetched_at.is_some());
    assert_eq!(data.stock.rows.len(), 1);
}

#[test]
fn a_server_that_offers_only_the_stock_is_asked_for_nothing_else() {
    let (server, url) = start_with(str::to_string, |c| c.with_scopes_offered(&["stock:read", "stock:write"]));
    seed_other_member(&url);
    let dir = versekit_core::testdir::Dir::new("orga-daten-lager");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    let view = hub.view();
    assert_eq!(view.connections[0].scopes, ["stock:read", "stock:write"]);
    assert!(hub.shows("stock:read") && !hub.shows("blueprints:read") && !hub.shows("orders:read"));
    assert!(!hub.refresh(&url, Kind::Blueprints) && !hub.refresh(&url, Kind::Orders));
    assert!(hub.refresh(&url, Kind::Stock));
    let data = wait_data(&hub, &url, |d| d.stock.fetched_at.is_some());
    assert_eq!(data.stock.rows.len(), 1);
    assert_eq!(probe.count("/blueprints") + probe.count("/orders"), 0);
}

#[test]
fn a_fetch_for_an_org_that_is_not_connected_does_nothing() {
    let (_server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-daten-keine");
    let hub = hub_over(dir.path(), client(), false);
    assert!(!hub.refresh(&url, Kind::Stock));
    assert!(!hub.ensure(&url, Kind::Stock, 0));
    assert_eq!(hub.data(&url), OrgData::default());
    let _ = E_GONE;
}

/// A server of pages without end: every page names another cursor.
struct Endless(AtomicU64);

impl Transport for Endless {
    fn send(&self, _: &Request) -> Result<Response, ApiError> {
        let n = self.0.fetch_add(1, Ordering::SeqCst);
        let body = json!({ "items": [{ "item": "A", "quantity": 1 }], "next_cursor": format!("c{n}") });
        Ok(Response { status: 200, headers: vec![], body: body.to_string().into_bytes() })
    }
}

/// A server whose last page names the cursor it was asked with.
struct Repeating;

impl Transport for Repeating {
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        let body = json!({ "items": [{ "item": "A", "quantity": 1 }], "next_cursor": "same", "url": request.url });
        Ok(Response { status: 200, headers: vec![], body: body.to_string().into_bytes() })
    }
}

#[test]
fn pages_without_end_and_a_repeated_cursor_stop_the_reading() {
    let mut connection = memory_connection("https://orga.example.org");
    let endless = OrgClient::new(Arc::new(Endless(AtomicU64::new(0))), "5.4.0");
    let error = endless.org_stock(&mut connection, now()).unwrap_err();
    assert_eq!(error.code, versekit_core::orgstd::E_ANSWER);
    let repeating = OrgClient::new(Arc::new(Repeating), "5.4.0");
    assert_eq!(repeating.org_stock(&mut connection, now()).unwrap().len(), 2, "the repeated cursor ends the list");
}

fn own_rows() -> Vec<versekit_core::stock::Row> {
    use versekit_core::stock::{Row, Store as Kept};
    let row = |name: &str, amount: f64, place: &str| Row {
        store: Kept::Trade,
        index: 0,
        name: name.into(),
        amount,
        piece: false,
        quality: None,
        place: place.into(),
        stolen: false,
        cost: Some(98_765.432_1),
        item: false,
        leftover: false,
    };
    vec![row("Laranite", 5.0, "Area18"), row("Construction Materials", 2.5, "Levski")]
}

fn wait_shared(hub: &Hub, id: &str, thing: Thing) -> versekit_core::orgstd::hub::Shared {
    let start = Instant::now();
    loop {
        let shared = hub.shared(id, thing);
        if !shared.running && (shared.at.is_some() || shared.error.is_some()) {
            return shared;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "the sharing never ended: {shared:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

use versekit_core::orgstd::hub::Thing;
use versekit_core::orgstd::share::{blueprint_share, stock_share};
use versekit_core::stock_data::StockData;

#[test]
fn the_own_stock_is_shared_once_after_the_call_and_read_again_without_a_price() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-teilen");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    let share = stock_share(&own_rows(), &StockData::sample());
    assert_eq!(share.items.len(), 2);
    assert_eq!(hub.shared(&url, Thing::Stock), Default::default(), "nothing went out by itself");
    assert_eq!(probe.count("PUT "), 0);
    assert!(hub.share_stock(&url, &share));
    let shared = wait_shared(&hub, &url, Thing::Stock);
    assert_eq!((shared.sent, shared.stored, shared.ended, shared.error), (2, Some(2), false, None));
    assert_eq!(probe.count("PUT "), 1, "one request");
    let bodies = probe.bodies().join("\n");
    assert!(
        !bodies.contains("98765") && !bodies.contains("einkauf") && !bodies.contains("price"),
        "the price went out: {bodies}"
    );
    // Read again: what the org holds of the member is what was sent.
    assert!(hub.refresh(&url, Kind::Stock));
    let data = wait_data(&hub, &url, |d| d.stock.fetched_at.is_some());
    let mut names: Vec<&str> = data.stock.rows.iter().map(|r| r.item.as_str()).collect();
    names.sort();
    assert_eq!(names, ["Construction Materials", "Laranite"]);
    assert!(data.stock.rows.iter().all(|r| r.member == "Pilot"));
    // The time and the number are kept next to the connections, nothing else of it.
    let kept = std::fs::read_to_string(dir.path().join("teilen.json")).unwrap();
    assert!(kept.contains("zeit") && kept.contains("\"zeilen\": 2"), "{kept}");
    assert!(!kept.contains("Laranite") && !kept.contains("Pilot") && !kept.contains("Area18"), "{kept}");
    let again = hub_over(dir.path(), client(), false);
    assert_eq!(again.shared(&url, Thing::Stock).sent, 2, "the last sharing is known at the next start");
    // Disconnecting takes it away.
    hub.disconnect(&url);
    wait_for(&hub, |v| v.busy.is_empty() && v.connections.is_empty());
    assert_eq!(hub.shared(&url, Thing::Stock), Default::default());
    let kept = std::fs::read_to_string(dir.path().join("teilen.json")).unwrap_or_default();
    assert!(!kept.contains(&url), "{kept}");
}

#[test]
fn an_empty_or_too_long_list_never_goes_out_and_only_ending_the_sharing_sends_the_empty_list() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-teilen-leer");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    let data = StockData::sample();
    let empty = stock_share(&[], &data);
    assert!(!hub.share_stock(&url, &empty), "an empty list is refused");
    assert!(!hub.share_blueprints(&url, &blueprint_share(Vec::new())));
    let long = stock_share(
        &(0..5001)
            .map(|n| {
                let mut row = own_rows().remove(0);
                row.place = format!("Ort {n}");
                row
            })
            .collect::<Vec<_>>(),
        &data,
    );
    assert_eq!(long.too_many, Some(5001));
    assert!(!hub.share_stock(&url, &long), "a list that is too long is refused, not cut");
    assert_eq!(probe.count("PUT "), 0, "nothing went out");
    assert!(server.stock_of("m1").is_empty());
    // Sharing, then ending: the empty list goes out only now.
    assert!(hub.share_stock(&url, &stock_share(&own_rows(), &data)));
    wait_shared(&hub, &url, Thing::Stock);
    assert_eq!(server.stock_of("m1").len(), 2);
    assert!(hub.end_share(&url, Thing::Stock));
    let shared = wait_shared(&hub, &url, Thing::Stock);
    assert!(shared.ended && shared.sent == 0 && shared.error.is_none(), "{shared:?}");
    assert!(server.stock_of("m1").is_empty(), "the org deleted what it had of the member");
    assert_eq!(probe.count("PUT "), 2);
}

#[test]
fn nothing_is_shared_without_the_right_and_blueprints_are_shared_by_their_names() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-teilen-recht");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, Some(&["stock:read", "blueprints:write"]));
    let data = StockData::sample();
    assert!(!hub.share_stock(&url, &stock_share(&own_rows(), &data)), "stock:write was not granted");
    assert!(!hub.deliver(&url, "a1", vec![("Quantanium".into(), 1.0)], "k"), "orders:write was not granted");
    assert_eq!(probe.count("PUT ") + probe.count("POST /versekit/v1/orders"), 0);
    let blueprints = blueprint_share(["P4-AR Rifle", "Arclight Pistol"].map(String::from));
    assert!(hub.share_blueprints(&url, &blueprints));
    let shared = wait_shared(&hub, &url, Thing::Blueprints);
    assert_eq!((shared.sent, shared.stored), (2, Some(2)));
}

#[test]
fn a_delivery_is_booked_once_with_one_key_however_often_it_is_sent_and_updates_the_order() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-buchen");
    let probe = Probe::real();
    let hub = hub_over(dir.path(), probe.client(), false);
    log_in(&hub, &server, &url, None);
    assert!(hub.refresh(&url, Kind::Orders));
    wait_data(&hub, &url, |d| d.orders.fetched_at.is_some());
    let key = versekit_core::orgstd::share::idempotency_key();
    for _ in 0..2 {
        assert!(hub.deliver(&url, "a1", vec![("Quantanium".into(), 4.0)], &key));
        let start = Instant::now();
        while hub.booking(&url).running {
            assert!(start.elapsed() < Duration::from_secs(20));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(hub.booking(&url).result.as_ref().is_some_and(Result::is_ok), "{:?}", hub.booking(&url));
    }
    assert_eq!(probe.keys(), [key.clone(), key], "the same key both times");
    let order = &hub.data(&url).orders.rows[0];
    assert_eq!(order.lines[0].delivered, 4.0, "booked once, not twice");
    // A new form, a new key: that is a second booking.
    let second = versekit_core::orgstd::share::idempotency_key();
    assert!(hub.deliver(&url, "a1", vec![("Quantanium".into(), 1.0)], &second));
    let start = Instant::now();
    while hub.booking(&url).running {
        assert!(start.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(hub.data(&url).orders.rows[0].lines[0].delivered, 5.0);
}

#[test]
fn a_refused_booking_says_why_by_its_status_and_keeps_the_order_as_it_was() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-buchen-fehler");
    let hub = hub_over(dir.path(), client(), false);
    log_in(&hub, &server, &url, None);
    let book = |order: &str, item: &str| {
        assert!(hub.deliver(&url, order, vec![(item.into(), 1.0)], &versekit_core::orgstd::share::idempotency_key()));
        let start = Instant::now();
        while hub.booking(&url).running {
            assert!(start.elapsed() < Duration::from_secs(20));
            std::thread::sleep(Duration::from_millis(10));
        }
        hub.booking(&url).result.unwrap().unwrap_err().status
    };
    assert_eq!(book("gibt-es-nicht", "Quantanium"), 404);
    assert_eq!(book("a1", "Etwas anderes"), 422);
    assert!(hub.deliver(&url, "a1", vec![("Quantanium".into(), 10.0)], "fertig"));
    while hub.booking(&url).running {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(book("a1", "Quantanium"), 409, "the order is closed");
}

#[test]
fn too_many_requests_pause_the_list_for_the_time_the_server_names() {
    const LIMIT: u32 = 3;
    let (server, url) = start_with(str::to_string, |mut c| {
        c.rate_limit = LIMIT;
        c
    });
    let dir = versekit_core::testdir::Dir::new("orga-daten-rate");
    let hub = hub_over(dir.path(), client(), false);
    log_in(&hub, &server, &url, None);
    let mut limited = None;
    for _ in 0..(3 * LIMIT) {
        if hub.refresh(&url, Kind::Stock) {
            let data = wait_data(&hub, &url, |d| !d.loading());
            if data.stock.error.is_some() {
                limited = data.stock.error.map(|e| (e.code, e.retry_after));
                break;
            }
        }
    }
    let (code, retry) = limited.expect("the limit");
    assert_eq!(code, E_RATE);
    let left = hub.data(&url).stock.pause_left(now());
    assert!(left > 0 && left <= retry.unwrap_or(60) + 1, "{left} {retry:?}");
}
