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

//! The client of the open org standard against the template server
//! (`versekit-orgserver`) on 127.0.0.1, through the real transport: discovery,
//! the code login with the panel, the API, refreshing, paging, refusals, and
//! the hub of the page with its store in a throwaway folder.

use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use versekit_core::basetool::http::{HttpTransport, Request, Transport};
use versekit_core::orgstd::client::{OrgClient, Poll};
use versekit_core::orgstd::discovery::Discovery;
use versekit_core::orgstd::hub::{Clock, Hub, Login, Outcome, View, system_clock};
use versekit_core::orgstd::store::{Connection, Store};
use versekit_core::orgstd::{E_DENIED, E_EXPIRED, E_FOREIGN, E_NO_STANDARD, E_RATE, E_RELOGIN, E_SCOPE};
use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;
use versekit_orgserver::{Config, Server, http};

#[path = "orga_standard/data.rs"]
mod lists;

const KEY: &str = "schluessel-des-piloten-0001";

/// A template server on a free port of 127.0.0.1; `public` turns the address it listens on into the one it names.
fn start_with(public: impl FnOnce(&str) -> String, setup: impl FnOnce(Config) -> Config) -> (Arc<Server>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let mut config = Config::new(&public(&url), "Testorga", "TST")
        .with_member("m1", "Pilot", KEY)
        .with_member("m2", "Zweite", "schluessel-der-zweiten-0002")
        .with_order(json!({ "id": "a1", "title": "Erz", "items": [{ "item": "Quantanium", "quantity": 10, "unit": "scu" }] }));
    config.poll_interval = 0;
    let server = Arc::new(Server::new(setup(config)));
    let handler = server.clone();
    std::thread::spawn(move || http::serve(listener, Arc::new(move |r: &http::Request| handler.handle(r))));
    (server, url)
}

fn start() -> (Arc<Server>, String) {
    start_with(str::to_string, |c| c)
}

fn transport() -> Arc<dyn Transport> {
    Arc::new(HttpTransport::new("", &Host::new(Flavor::native(), &[], "/")).unwrap())
}

fn client() -> OrgClient {
    OrgClient::new(transport(), "5.4.0")
}

fn now() -> u64 {
    (system_clock())()
}

/// A form sent to the panel as a browser set to German would.
fn panel(url: &str, pairs: &[(&str, &str)]) -> String {
    let body = versekit_core::basetool::connection::urlencode(pairs).into_bytes();
    let request = Request {
        method: "POST".into(),
        url: format!("{url}/device"),
        headers: vec![
            ("Content-Type".into(), "application/x-www-form-urlencoded".into()),
            ("Accept-Language".into(), "de-DE,de;q=0.9".into()),
        ],
        body: Some(body),
    };
    let answer = transport().send(&request).unwrap();
    assert_eq!(answer.status, 200);
    String::from_utf8(answer.body).unwrap()
}

/// Logs in through the panel with the rights `scopes` (all asked for when `None`).
fn connect(url: &str, scopes: Option<&[&str]>) -> Connection {
    connect_as(url, KEY, scopes)
}

/// [`connect`] as the member whose personal key is `key`.
fn connect_as(url: &str, key: &str, scopes: Option<&[&str]>) -> Connection {
    let client = client();
    let discovery = client.discover(url).unwrap();
    let code = client.start_login(&discovery).unwrap();
    assert_eq!(client.poll_login(&discovery, &code).unwrap(), Poll::Pending);
    let mut form = vec![("user_code", code.user_code.as_str()), ("key", key), ("action", "approve")];
    let chosen: Vec<&str> = scopes.map_or_else(|| discovery.scopes.iter().map(String::as_str).collect(), <[&str]>::to_vec);
    form.extend(chosen.iter().map(|s| ("scope", *s)));
    assert!(panel(url, &form).contains("Freigegeben"));
    let Poll::Done(tokens) = client.poll_login(&discovery, &code).unwrap() else { panic!("not confirmed") };
    let mut connection = Connection {
        discovery,
        member: String::new(),
        scopes: Vec::new(),
        access_token: String::new(),
        access_expires: 0,
        refresh_token: String::new(),
        connected_at: now(),
        checked_at: None,
    };
    OrgClient::take_tokens(&mut connection, tokens, now());
    connection
}

#[test]
fn a_member_connects_with_a_code_from_the_panel_and_the_api_answers() {
    let (server, url) = start();
    let client = client();
    let discovery = client.discover(&url).unwrap();
    assert_eq!((discovery.name.as_str(), discovery.tag.as_str(), discovery.version.as_str()), ("Testorga", "TST", "1"));
    assert_eq!(discovery.scopes.len(), 6);
    // The panel: the code form, a wrong key, then the rights asked for.
    let code = client.start_login(&discovery).unwrap();
    assert!(code.verification_uri.starts_with(&url) && code.verification_uri.ends_with("/device"));
    assert!(panel(&url, &[("user_code", &code.user_code), ("key", "falsch-falsch-falsch"), ("action", "show")]).contains("stimmt nicht"));
    let shown = panel(&url, &[("user_code", &code.user_code.to_lowercase().replace('-', " ")), ("key", KEY), ("action", "show")]);
    assert!(shown.contains("Hallo Pilot") && shown.contains("stock:write"), "{shown}");
    server.deny(&code.user_code);
    assert_eq!(client.poll_login(&discovery, &code).unwrap_err().code, E_DENIED);

    let mut connection = connect(&url, None);
    let me = client.me(&mut connection, now()).unwrap();
    assert_eq!(me.member, "Pilot");
    assert_eq!(me.scopes.len(), 6);
    let items = [json!({ "item": "Quantanium", "quantity": 12.5, "unit": "scu", "quality": 650, "location": "Area18" })];
    client.share_stock(&mut connection, &items, now()).unwrap();
    let stock = client.org_stock(&mut connection, now()).unwrap();
    assert_eq!(stock, [json!({ "item": "Quantanium", "quantity": 12.5, "unit": "scu", "quality": 650, "location": "Area18", "member": "Pilot" })]);
    client.share_blueprints(&mut connection, &["P4-AR Rifle".into(), "Arclight Pistol".into()], now()).unwrap();
    let blueprints = client.org_blueprints(&mut connection, now()).unwrap();
    assert_eq!(blueprints[0], json!({ "name": "Arclight Pistol", "holders": ["Pilot"] }));
    // A delivery sent twice with the same key counts once.
    let orders = client.orders(&mut connection, now()).unwrap();
    assert_eq!(orders[0]["status"], "open");
    let first = client.deliver(&mut connection, "a1", &[("Quantanium".into(), 4.0)], "key-1", now()).unwrap();
    let again = client.deliver(&mut connection, "a1", &[("Quantanium".into(), 4.0)], "key-1", now()).unwrap();
    assert_eq!(first, again);
    assert_eq!(first["order"]["items"][0]["delivered"], 4.0);
    let done = client.deliver(&mut connection, "a1", &[("Quantanium".into(), 6.0)], "key-2", now()).unwrap();
    assert_eq!(done["order"]["status"], "done");
    // After revoking, the connection is worthless.
    client.revoke(&connection.discovery, &connection.refresh_token).unwrap();
    assert_eq!(server.refresh_count(), 0);
    assert_eq!(client.me(&mut connection, now()).unwrap_err().code, E_RELOGIN);
}

#[test]
fn a_run_out_token_is_refreshed_and_the_old_refresh_token_is_worthless() {
    let (server, url) = start();
    let client = client();
    let mut connection = connect(&url, None);
    let first_refresh = connection.refresh_token.clone();
    server.expire_access();
    // The connection still thinks its token is good: the 401 makes the refresh.
    assert_eq!(client.me(&mut connection, now()).unwrap().member, "Pilot");
    assert_ne!(connection.refresh_token, first_refresh);
    assert_eq!(server.refresh_count(), 1);
    // The connection knows its token runs out: refreshed before the request.
    connection.access_expires = 0;
    let second_refresh = connection.refresh_token.clone();
    client.me(&mut connection, now()).unwrap();
    assert_ne!(connection.refresh_token, second_refresh);
    assert_eq!(client.refresh(&connection.discovery, &first_refresh).unwrap_err().code, E_RELOGIN);
}

#[test]
fn long_lists_come_in_pages_and_all_of_them_are_read() {
    let (server, url) = start();
    let client = client();
    let mut connection = connect(&url, None);
    let items: Vec<Value> = (0..1234).map(|n| json!({ "item": format!("Ware {n}"), "quantity": n })).collect();
    client.share_stock(&mut connection, &items, now()).unwrap();
    assert_eq!(server.stock_of("m1").len(), 1234);
    let read = client.org_stock(&mut connection, now()).unwrap();
    assert_eq!(read.len(), 1234);
    assert_eq!(read[1233]["item"], "Ware 1233");
}

#[test]
fn a_right_not_granted_and_too_many_requests_are_refused() {
    // A small limit, so that all requests fall into one or two minutes also on a slow machine.
    const LIMIT: u32 = 3;
    let (_server, url) = start_with(str::to_string, |mut c| {
        c.rate_limit = LIMIT;
        c
    });
    let client = client();
    let mut connection = connect(&url, Some(&["stock:read"]));
    assert_eq!(connection.scopes, ["stock:read"]);
    let refused = client.share_stock(&mut connection, &[json!({ "item": "X", "quantity": 1 })], now()).unwrap_err();
    assert_eq!((refused.code, refused.status), (E_SCOPE, 403));
    let mut last = None;
    for _ in 0..=LIMIT {
        if let Err(error) = client.me(&mut connection, now()) {
            last = Some(error);
            break;
        }
    }
    // Two requests a moment apart may fall into two minutes: then the limit is met in the second.
    if last.is_none() {
        last = (0..=LIMIT).find_map(|_| client.me(&mut connection, now()).err());
    }
    let limited = last.expect("the limit");
    assert_eq!((limited.code, limited.status), (E_RATE, 429));
    assert!(limited.retry_after.is_some_and(|s| (1..=60).contains(&s)));
}

#[test]
fn a_server_that_names_another_host_or_is_none_is_refused() {
    let (_server, url) = start_with(|u| u.replace("127.0.0.1", "localhost"), |c| c);
    assert_eq!(client().discover(&url).unwrap_err().code, E_FOREIGN);
    let (_plain, plain) = start();
    let refused = client().discover(&format!("{plain}/irgendwo")).map(|d| d.name);
    assert_eq!(refused, Ok("Testorga".to_string()), "a path the member types is dropped");
    // A host without the document is no org server.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let empty = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        http::serve(listener, Arc::new(|_: &http::Request| http::Response::problem(404, "NOT_FOUND", "none")))
    });
    assert_eq!(client().discover(&empty).unwrap_err().code, E_NO_STANDARD);
}

/// Waits until `done` holds for the view of `hub` (at most two minutes: on a loaded computer the threads of the hub
/// take their time; the wait ends as soon as it holds).
fn wait_for(hub: &Hub, done: impl Fn(&View) -> bool) -> View {
    let start = Instant::now();
    loop {
        let view = hub.view();
        if done(&view) {
            return view;
        }
        assert!(start.elapsed() < Duration::from_secs(120), "waited in vain: {view:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn hub_in(folder: &std::path::Path, clock: Clock) -> Hub {
    let store = Store::in_folder(&folder.to_string_lossy());
    Hub::with_min_interval(client(), store, clock, Duration::from_millis(50))
}

#[test]
fn the_hub_logs_in_checks_and_disconnects_and_keeps_no_token_in_view() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-hub");
    let hub = hub_in(dir.path(), system_clock());
    hub.connect(&url);
    let view = wait_for(&hub, |v| matches!(v.login, Some(Login::Code { .. })));
    let Some(Login::Code { org, user_code, verification_uri }) = view.login else { unreachable!() };
    assert_eq!(org, "Testorga");
    assert!(verification_uri.ends_with("/device"));
    assert!(server.approve(&user_code, "m1", Some(&["stock:read", "orders:read"])));
    let view = wait_for(&hub, |v| matches!(v.login, Some(Login::Connected { .. })));
    assert_eq!(view.login, Some(Login::Connected { org: "Testorga".into(), member: "Pilot".into() }));
    assert_eq!(view.connections.len(), 1);
    let entry = &view.connections[0];
    assert_eq!((entry.id.as_str(), entry.member.as_str()), (url.as_str(), "Pilot"));
    assert_eq!(entry.scopes, ["stock:read", "orders:read"]);
    let shown = format!("{view:?}");
    assert!(!shown.contains("vka_") && !shown.contains("vkr_"), "{shown}");
    // The store lies in its own folder; a second hub reads the connection from there.
    let again = hub_in(dir.path(), system_clock());
    assert_eq!(again.view().connections, view.connections);

    hub.check(&url);
    let view = wait_for(&hub, |v| v.busy.is_empty() && !v.outcomes.is_empty());
    assert_eq!(view.outcomes, [(url.clone(), Outcome::Checked)]);
    hub.disconnect(&url);
    let view = wait_for(&hub, |v| v.busy.is_empty() && v.connections.is_empty());
    assert_eq!(view.outcomes, [(url.clone(), Outcome::Disconnected { told: true })]);
    assert_eq!(server.refresh_count(), 0);
    assert!(Store::in_folder(&dir.path().to_string_lossy()).load().unwrap().is_empty());
}

#[test]
fn without_a_connection_everything_of_the_org_is_hidden() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-sichtbar");
    let hub = hub_in(dir.path(), system_clock());
    for scope in versekit_core::orgstd::SCOPES {
        assert!(!hub.shows(scope), "{scope} shows without a connection");
    }
    hub.connect(&url);
    let view = wait_for(&hub, |v| matches!(v.login, Some(Login::Code { .. })));
    let Some(Login::Code { user_code, .. }) = view.login else { unreachable!() };
    assert!(!hub.shows("stock:read"), "a login that is not confirmed yet shows nothing");
    assert!(server.approve(&user_code, "m1", Some(&["stock:read"])));
    wait_for(&hub, |v| matches!(v.login, Some(Login::Connected { .. })));
    assert!(hub.shows("stock:read"));
    assert!(!hub.shows("orders:read"), "a right not granted shows nothing");
    hub.disconnect(&url);
    wait_for(&hub, |v| v.busy.is_empty() && v.connections.is_empty());
    for scope in versekit_core::orgstd::SCOPES {
        assert!(!hub.shows(scope), "{scope} shows after disconnecting");
    }
}

#[test]
fn a_cancelled_or_expired_login_stores_nothing() {
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-hub-abbruch");
    let hub = hub_in(dir.path(), system_clock());
    hub.connect(&url);
    let view = wait_for(&hub, |v| matches!(v.login, Some(Login::Code { .. })));
    let Some(Login::Code { user_code, .. }) = view.login else { unreachable!() };
    hub.cancel_login();
    assert!(server.approve(&user_code, "m1", None));
    std::thread::sleep(Duration::from_millis(400));
    let view = hub.view();
    assert_eq!((view.login, view.connections.len()), (None, 0));
    // A poll on its way when the login was ended may still fetch the tokens; the hub gives them back at once.
    let mut waited = Duration::ZERO;
    while server.refresh_count() > 0 && waited < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(50));
        waited += Duration::from_millis(50);
    }
    assert_eq!(server.refresh_count(), 0, "nothing of the ended login stays on the server");
    assert!(Store::in_folder(&dir.path().to_string_lossy()).load().unwrap().is_empty(), "and nothing here");

    // A clock that runs ten minutes per look: the code is over before the member confirms.
    let ticks = Arc::new(AtomicU64::new(now()));
    let clock: Clock = Arc::new(move || ticks.fetch_add(601, Ordering::SeqCst));
    let hub = hub_in(dir.path(), clock);
    hub.connect(&url);
    let view = wait_for(&hub, |v| matches!(v.login, Some(Login::Failed(_))));
    assert!(matches!(view.login, Some(Login::Failed(e)) if e.code == E_EXPIRED), "{view:?}");
    assert!(view.connections.is_empty());
}

#[test]
fn a_disconnect_without_the_server_forgets_the_connection_here() {
    let (_server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-hub-weg");
    let connection = connect(&url, None);
    // The org server is gone: the same connection on a port where nothing answers.
    let gone = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://127.0.0.1:{}", gone.local_addr().unwrap().port());
    drop(gone);
    let mut stored = connection.to_json();
    let moved = stored["entdeckung"].to_string().replace(&url, &dead);
    stored["entdeckung"] = serde_json::from_str(&moved).unwrap();
    let away = Connection::from_json(&stored).unwrap();
    assert!(Discovery::from_json(&stored["entdeckung"]).is_some());
    Store::in_folder(&dir.path().to_string_lossy()).save(&[away]).unwrap();
    let hub = hub_in(dir.path(), system_clock());
    assert_eq!(hub.view().connections.len(), 1);
    hub.disconnect(&dead);
    let view = wait_for(&hub, |v| v.busy.is_empty() && v.connections.is_empty());
    assert_eq!(view.outcomes, [(dead.clone(), Outcome::Disconnected { told: false })]);
    hub.check(&dead);
    let view = wait_for(&hub, |v| v.busy.is_empty() && v.outcomes.iter().any(|(_, o)| matches!(o, Outcome::Failed(_))));
    assert!(matches!(view.outcomes[0].1, Outcome::Failed(e) if e.code == versekit_core::orgstd::E_GONE), "{view:?}");
}
