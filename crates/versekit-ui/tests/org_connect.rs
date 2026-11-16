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

//! The page "Connect org" without a window (egui_kittest without a renderer), against the template server of the
//! open org standard on 127.0.0.1: type the address, connect, confirm the code, check, disconnect; a refused
//! address; the preview in both languages.

use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::basetool::http::HttpTransport;
use versekit_core::language::{Arg, Choice, set_thread_language, t, tf};
use versekit_core::orgstd::client::OrgClient;
use versekit_core::orgstd::hub::{Hub, Kind, Login, system_clock};
use versekit_core::orgstd::store::Store;
use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;
use versekit_core::theme;
use versekit_orgserver::{Config, Server, http};
use versekit_ui::org_connect_page::{OrgConnectState, show_page};

fn start() -> (Arc<Server>, String) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let mut config = Config::new(&url, "Testorga", "TST").with_member("m1", "Pilot", "schluessel-des-piloten-0001");
    config.poll_interval = 0;
    let server = Arc::new(Server::new(config));
    let handler = server.clone();
    std::thread::spawn(move || http::serve(listener, Arc::new(move |r: &http::Request| handler.handle(r))));
    (server, url)
}

fn harness(state: OrgConnectState) -> Harness<'static, OrgConnectState> {
    Harness::builder()
        .with_size(vec2(1280.0, 1400.0))
        .with_context_setup(versekit_ui::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, state: &mut OrgConnectState| show_page(ui, theme::current(), state), state)
}

/// Steps the page until a label containing `text` shows.
fn wait_for_text(harness: &mut Harness<'_, OrgConnectState>, text: &str) {
    let start = Instant::now();
    loop {
        harness.step();
        if harness.query_by_label_contains(text).is_some() {
            return;
        }
        assert!(start.elapsed() < Duration::from_secs(20), "{text:?} never showed: {:?}", harness.state().hub().view());
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn type_address(harness: &mut Harness<'_, OrgConnectState>, address: &str) {
    harness.get_by_role(Role::TextInput).focus();
    harness.step();
    harness.get_by_role(Role::TextInput).type_text(address);
    harness.step();
    harness.get_by_label(&t("s_os_verbinden")).click();
    harness.step();
}

#[test]
fn a_member_connects_an_org_by_its_address_checks_and_disconnects_it() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let (server, url) = start();
    let dir = versekit_core::testdir::Dir::new("orga-seite");
    let transport = Arc::new(HttpTransport::new("", &Host::new(Flavor::native(), &[], "/")).unwrap());
    let client = OrgClient::new(transport, "5.4.0");
    let store = Store::in_folder(&dir.path().to_string_lossy());
    let hub = Hub::with_min_interval(client, store, system_clock(), Duration::from_millis(50));
    let mut harness = harness(OrgConnectState::with_hub(hub.clone(), false));
    harness.step();
    assert!(harness.query_by_label_contains(&t("s_os_keine")).is_some());

    type_address(&mut harness, &url);
    wait_for_text(&mut harness, &t("s_os_code_warnung"));
    let Some(Login::Code { user_code, .. }) = hub.view().login else { panic!("no code") };
    assert!(harness.query_by_label(&user_code).is_some(), "the code is shown");
    assert!(harness.query_by_label_contains(&format!("{url}/device")).is_some(), "the bare address is shown");
    assert!(server.approve(&user_code, "m1", Some(&["stock:read", "orders:read"])));
    wait_for_text(&mut harness, "Verbunden mit Testorga als Pilot");
    assert!(harness.query_by_label_contains("Testorga [TST]").is_some());
    assert!(harness.query_by_label_contains("Orga-Bestand lesen, Aufträge lesen").is_some());
    let typed = harness.get_by_role(Role::TextInput).value().unwrap_or_default();
    assert!(typed.is_empty(), "the field is emptied after connecting: {typed:?}");

    // Each right granted has the switch of its area, off from the start; the right not granted has none.
    let link = harness.state().link();
    let switch = |harness: &Harness<'_, OrgConnectState>, key: &str| harness.query_by_role_and_label(Role::CheckBox, &t(key)).is_some();
    assert!(switch(&harness, "s_ob_lager") && switch(&harness, "s_ob_auftraege"));
    assert!(!switch(&harness, "s_ob_bauplaene"), "no right for blueprints, no switch");
    assert!(!link.visible(Kind::Stock) && !link.visible(Kind::Orders), "off from the start");
    harness.get_by_role_and_label(Role::CheckBox, &t("s_ob_lager")).click();
    harness.step();
    assert!(link.visible(Kind::Stock) && !link.visible(Kind::Orders), "only the switch that was clicked");
    harness.get_by_role_and_label(Role::CheckBox, &t("s_ob_lager")).click();
    harness.step();
    assert!(!link.visible(Kind::Stock), "off again");
    harness.get_by_role_and_label(Role::CheckBox, &t("s_ob_auftraege")).click();
    harness.step();
    assert!(link.areas.on(&url, Kind::Orders));

    // The buttons of an entry name its org for a screen reader.
    let org = [("org", Arg::from("Testorga"))];
    harness.get_by_label(&tf("s_os_pruefen_name", &org)).click();
    wait_for_text(&mut harness, &t("s_os_ok"));
    harness.get_by_label(&tf("s_os_trennen_name", &org)).click();
    wait_for_text(&mut harness, &t("s_os_getrennt"));
    wait_for_text(&mut harness, &t("s_os_keine"));
    assert_eq!(server.refresh_count(), 0);
    assert!(Store::in_folder(&dir.path().to_string_lossy()).load().unwrap().is_empty());
    assert!(!link.areas.on(&url, Kind::Orders), "disconnecting takes the entry of the org with it");
    set_thread_language(Choice::Auto);
}

/// Confirms a code on the page `/device` of a server outside the test, as a member does in the browser.
fn confirm_on_panel(url: &str, user_code: &str, key: &str) {
    use std::io::{Read, Write};
    let address = url.strip_prefix("http://").expect("plain http, only on 127.0.0.1");
    let encode = |text: &str| -> String {
        text.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect()
    };
    let body = format!(
        "user_code={}&key={}&action=approve&scope=stock%3Aread&scope=orders%3Aread",
        encode(user_code),
        encode(key)
    );
    let mut stream = std::net::TcpStream::connect(address).unwrap();
    write!(
        stream,
        "POST /device HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    assert!(answer.contains("Confirmed. VerseKit connects in a moment."), "the panel did not confirm");
}

/// The way of the first test against an org server outside the test, reached on the own computer (for example
/// through a tunnel): `VERSEKIT_ORGA_EXTERN=http://127.0.0.1:<port>` and `VERSEKIT_ORGA_EXTERN_SCHLUESSEL` (the key
/// of a member there), run with `--ignored`.
#[test]
#[ignore = "needs an org server outside the test"]
fn a_member_connects_an_org_server_outside_the_test() {
    let (Ok(url), Ok(key)) = (std::env::var("VERSEKIT_ORGA_EXTERN"), std::env::var("VERSEKIT_ORGA_EXTERN_SCHLUESSEL"))
    else {
        panic!("VERSEKIT_ORGA_EXTERN and VERSEKIT_ORGA_EXTERN_SCHLUESSEL are not set");
    };
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let dir = versekit_core::testdir::Dir::new("orga-seite-extern");
    let transport = Arc::new(HttpTransport::new("", &Host::new(Flavor::native(), &[], "/")).unwrap());
    let client = OrgClient::new(transport, "5.4.0");
    let store = Store::in_folder(&dir.path().to_string_lossy());
    let hub = Hub::with_min_interval(client, store, system_clock(), Duration::from_millis(50));
    let mut harness = harness(OrgConnectState::with_hub(hub.clone(), false));
    harness.step();

    type_address(&mut harness, &url);
    wait_for_text(&mut harness, &t("s_os_code_warnung"));
    let Some(Login::Code { org, user_code, .. }) = hub.view().login else { panic!("no code") };
    confirm_on_panel(&url, &user_code, &key);
    wait_for_text(&mut harness, &format!("Verbunden mit {org} als"));
    assert!(harness.query_by_label_contains("Orga-Bestand lesen, Aufträge lesen").is_some());

    let names = [("org", Arg::from(org.as_str()))];
    harness.get_by_label(&tf("s_os_pruefen_name", &names)).click();
    wait_for_text(&mut harness, &t("s_os_ok"));
    harness.get_by_label(&tf("s_os_trennen_name", &names)).click();
    wait_for_text(&mut harness, &t("s_os_getrennt"));
    wait_for_text(&mut harness, &t("s_os_keine"));
    assert!(Store::in_folder(&dir.path().to_string_lossy()).load().unwrap().is_empty());
    set_thread_language(Choice::Auto);
}

#[test]
fn an_unencrypted_address_is_refused_before_anything_goes_out() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let state = OrgConnectState::sample();
    let hub = state.hub().clone();
    let mut harness = harness(state);
    harness.step();
    type_address(&mut harness, "http://orga.example.org");
    harness.step();
    assert!(harness.query_by_label_contains(&t("s_os_f_unsicher")).is_some());
    assert_eq!(hub.view().login, None, "no login started");
    set_thread_language(Choice::Auto);
}

/// Outside the page "Connect org" (and the wiring in `sample.rs`) every file that uses the open org standard asks
/// `shows`, so that without a connection nothing of it is seen or found.
#[test]
fn outside_the_connect_page_the_org_standard_shows_only_when_connected() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![src.clone()];
    let mut found = Vec::new();
    while let Some(path) = files.pop() {
        if path.is_dir() {
            files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy();
        // The pages and the window ask; the files of test code only build a made-up org or draw a page of it.
        let test_code = matches!(name.as_ref(), "tests.rs" | "org_areas.rs" | "streamer_mode.rs");
        if !name.ends_with(".rs") || name == "org_connect_page.rs" || name == "sample.rs" || test_code {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        // `area_visible` and `orgs_with` ask for the connection, the right and the switch of the area at once.
        // `writable` asks for the right to share with an org, `visible` and `orgs` for the area of a page.
        let asks = [".shows(", ".area_visible(", ".orgs_with(", ".writable(", ".visible(", ".orgs("]
            .iter()
            .any(|ask| text.contains(ask));
        if (text.contains("orgstd") || text.contains("OrgConnectState")) && !asks {
            found.push(path.display().to_string());
        }
    }
    assert!(found.is_empty(), "uses the org standard without asking whether it shows: {found:?}");
}

#[test]
fn the_preview_shows_a_made_up_org_in_both_languages() {
    theme::set_thread_current("krt");
    for (choice, connected, rights) in [
        (Choice::De, "Verbundene Orgas", "Orga-Bestand lesen"),
        (Choice::En, "Connected orgs", "Read the org's stock"),
    ] {
        set_thread_language(choice);
        let mut harness = harness(OrgConnectState::sample());
        harness.step();
        assert!(harness.query_by_label_contains(connected).is_some(), "{connected}");
        assert!(harness.query_by_label_contains("Nova [NOVA]").is_some());
        assert!(harness.query_by_label_contains(rights).is_some(), "{rights}");
        assert!(harness.query_by_label_contains(&t("s_os_beispiel")).is_some());
    }
    set_thread_language(Choice::Auto);
}
