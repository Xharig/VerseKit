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

use std::sync::{Arc, Mutex};

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::basetool::error::ApiError;
use versekit_core::basetool::http::{Request, Response, Transport};
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::orgstd::client::OrgClient;
use versekit_core::orgstd::hub::{Hub, system_clock};
use versekit_core::orgstd::store::Store;
use versekit_core::stock::Store as Kept;
use versekit_core::theme;

use super::*;
use crate::org_connect_page::{OrgConnectState, SAMPLE_ADDRESS, sample_connection};

/// Answers every request with a stored count and keeps what was sent.
struct Recording(Arc<Mutex<Vec<(String, String)>>>);

impl Transport for Recording {
    fn send(&self, request: &Request) -> Result<Response, ApiError> {
        let body = request.body.as_ref().map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
        self.0.lock().unwrap().push((request.method.clone(), body));
        Ok(Response { status: 200, headers: vec![], body: br#"{"items_stored": 2, "blueprints_stored": 2}"#.to_vec() })
    }
}

struct Page {
    state: OrgShareState,
    link: OrgLink,
    rows: Vec<Row>,
    data: StockData,
    blueprints: Vec<String>,
    blueprint: bool,
}

fn row(name: &str, amount: f64, place: &str) -> Row {
    Row {
        store: Kept::Trade,
        index: 0,
        name: name.into(),
        amount,
        piece: false,
        quality: None,
        place: place.into(),
        stolen: false,
        cost: Some(777_333.5),
        item: false,
        leftover: false,
    }
}

/// A page over a made-up org that may be shared with, the requests it gets, and the connected state of the hub.
fn page(scopes: &[&str], rows: Vec<Row>, sample: bool) -> (Harness<'static, Page>, Arc<Mutex<Vec<(String, String)>>>) {
    set_thread_language(Choice::De);
    let sent = Arc::new(Mutex::new(Vec::new()));
    let client = OrgClient::new(Arc::new(Recording(sent.clone())), "5.4.0");
    let mut connection = sample_connection();
    connection.scopes = scopes.iter().map(|s| s.to_string()).collect();
    connection.access_expires = u64::MAX / 2;
    let hub = Hub::new(client, Store::memory(vec![connection]), system_clock());
    let mut orgs = OrgConnectState::with_hub(hub, sample);
    orgs.link().areas.set(SAMPLE_ADDRESS, versekit_core::orgstd::hub::Kind::Stock, true);
    let page = Page {
        state: OrgShareState::default(),
        link: orgs.link(),
        rows,
        data: StockData::sample(),
        blueprints: Vec::new(),
        blueprint: false,
    };
    let _ = &mut orgs;
    let mut harness = Harness::builder()
        .with_size(vec2(1000.0, 900.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, page: &mut Page| {
                let build = || BlueprintShare { names: page.blueprints.clone(), ..BlueprintShare::default() };
                let source =
                    if page.blueprint { Source::Blueprints(&build) } else { Source::Stock(&page.rows, &page.data) };
                let link = page.link.clone();
                show(ui, theme::current(), &mut page.state, &link, &source);
            },
            page,
        );
    harness.run_steps(3);
    (harness, sent)
}

const ALL: [&str; 6] =
    ["stock:read", "stock:write", "blueprints:read", "blueprints:write", "orders:read", "orders:write"];

fn puts(sent: &Arc<Mutex<Vec<(String, String)>>>) -> Vec<String> {
    sent.lock().unwrap().iter().filter(|(method, _)| method == "PUT").map(|(_, body)| body.clone()).collect()
}

#[test]
fn nothing_is_offered_without_the_right_to_share() {
    let (harness, _) = page(&["stock:read", "orders:write"], vec![row("Laranite", 5.0, "Area18")], false);
    assert!(harness.query_all_by_label_contains(&t("s_ot_teilen")).next().is_none());
}

#[test]
fn a_click_opens_the_card_and_sends_nothing_and_cancel_closes_it() {
    let (mut harness, sent) = page(&ALL, vec![row("Laranite", 5.0, "Area18")], false);
    assert!(harness.query_by_label_contains(&tf("s_ot_noch_nicht", &[("orga", Arg::from("Nova"))])).is_some());
    harness.get_by_label(&tf("s_ot_teilen_name", &[("orga", Arg::from("Nova"))])).click();
    harness.run_steps(2);
    assert!(
        harness.query_all_by_label_contains(&t("s_ot_ersetzt")).next().is_some(),
        "the card says what the org does"
    );
    let who = tf("s_ot_orga", &[("name", Arg::from("Nova")), ("adresse", Arg::from(SAMPLE_ADDRESS))]);
    assert!(harness.query_all_by_label_contains(&who).next().is_some(), "the org with its address");
    assert!(harness.query_all_by_label_contains(&tf("s_ot_zeilen", &[("n", Arg::from(1))])).next().is_some());
    assert!(puts(&sent).is_empty(), "opening the card sends nothing");
    harness.get_by_label(&t("s_os_abbrechen")).click();
    harness.run_steps(2);
    assert!(harness.query_all_by_label_contains(&t("s_ot_ersetzt")).next().is_none(), "closed");
    assert!(puts(&sent).is_empty());
}

#[test]
fn share_sends_once_without_the_price_and_the_line_tells_how_many_went() {
    let (mut harness, sent) = page(&ALL, vec![row("Laranite", 5.0, "Area18"), row("Geheimware", 1.0, "")], false);
    harness.get_by_label(&tf("s_ot_teilen_name", &[("orga", Arg::from("Nova"))])).click();
    harness.run_steps(2);
    let left =
        tf("s_ot_weggelassen", &[("n", Arg::from(1)), ("a", Arg::from(1)), ("b", Arg::from(0)), ("c", Arg::from(0))]);
    assert!(harness.query_all_by_label_contains(&left).next().is_some(), "what stays home is counted: {left}");
    harness.get_by_label(&t("s_ot_teilen_ja")).click();
    let start = std::time::Instant::now();
    while puts(&sent).is_empty() && start.elapsed() < std::time::Duration::from_secs(10) {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run_steps(5);
    let bodies = puts(&sent);
    assert_eq!(bodies.len(), 1, "one request: {bodies:?}");
    assert!(
        bodies[0].contains("Laranite") && !bodies[0].contains("777") && !bodies[0].contains("Geheimware"),
        "{}",
        bodies[0]
    );
    assert!(harness.query_all_by_label_contains(&t("s_ot_ersetzt")).next().is_none(), "the card closed");
    harness.run_steps(3);
    let line = tf("s_ot_geteilt", &[("orga", Arg::from("Nova")), ("zeit", Arg::from("#")), ("n", Arg::from(1))]);
    let head = line.split('#').next().unwrap_or_default().to_string();
    assert!(harness.query_all_by_label_contains(&head).next().is_some(), "the line says what went: {head}");
}

#[test]
fn a_list_that_cannot_be_sent_has_a_card_with_the_reason_and_the_button_stays_blocked() {
    let (mut harness, sent) = page(&ALL, vec![row("Geheimware", 1.0, "")], false);
    harness.get_by_label(&tf("s_ot_teilen_name", &[("orga", Arg::from("Nova"))])).click();
    harness.run_steps(2);
    assert!(harness.query_all_by_label_contains(&t("s_ot_leer")).next().is_some());
    harness.get_by_label(&t("s_ot_teilen_ja")).click();
    harness.run_steps(5);
    assert!(sent.lock().unwrap().is_empty(), "nothing is sent");
    assert!(harness.query_all_by_label_contains(&t("s_ot_ersetzt")).next().is_some(), "the card stays");
}

#[test]
fn stopping_the_sharing_has_its_own_card_and_sends_the_empty_list_only_after_it() {
    let (mut harness, sent) = page(&ALL, vec![row("Laranite", 5.0, "Area18")], false);
    let orga = [("orga", Arg::from("Nova"))];
    // Nothing shared yet: no button to stop.
    assert!(harness.query_by_label(&tf("s_ot_beenden_name", &orga)).is_none());
    harness.get_by_label(&tf("s_ot_teilen_name", &orga)).click();
    harness.run_steps(2);
    harness.get_by_label(&t("s_ot_teilen_ja")).click();
    let start = std::time::Instant::now();
    while puts(&sent).is_empty() && start.elapsed() < std::time::Duration::from_secs(10) {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    harness.run_steps(8);
    assert_eq!(puts(&sent).len(), 1);
    // Now the button stands; a click opens the card, which sends nothing.
    harness.query_all_by_label(&tf("s_ot_beenden_name", &orga)).next().expect("the button to stop").click();
    harness.run_steps(2);
    assert!(harness.query_all_by_label_contains(&t("s_ot_beenden_frage")).next().is_some());
    assert_eq!(puts(&sent).len(), 1, "no second request yet");
    harness.get_by_label(&t("s_ot_beenden")).click();
    let start = std::time::Instant::now();
    while puts(&sent).len() < 2 && start.elapsed() < std::time::Duration::from_secs(10) {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(puts(&sent).len(), 2);
    assert!(puts(&sent)[1].contains("\"items\":[]"), "the empty list: {}", puts(&sent)[1]);
}

#[test]
fn the_preview_shares_nothing() {
    let (mut harness, sent) = page(&ALL, vec![row("Laranite", 5.0, "Area18")], true);
    harness.get_by_label(&tf("s_ot_teilen_name", &[("orga", Arg::from("Nova"))])).click();
    harness.run_steps(2);
    harness.get_by_label(&t("s_ot_teilen_ja")).click();
    harness.run_steps(5);
    assert!(sent.lock().unwrap().is_empty());
    assert!(harness.query_all_by_label_contains(&t("s_ot_vorschau")).next().is_some());
}

#[test]
fn the_blueprints_card_names_what_stays_home_and_sends_the_names() {
    let (mut harness, sent) = page(&ALL, Vec::new(), false);
    {
        let page = harness.state_mut();
        page.blueprint = true;
        page.blueprints = vec!["P4-AR Rifle".into(), "Arclight Pistol".into()];
    }
    harness.run_steps(2);
    let orga = [("orga", Arg::from("Nova"))];
    harness.get_by_label(&tf("s_ot_teilen_bp_name", &orga)).click();
    harness.run_steps(2);
    assert!(harness.query_all_by_label_contains(&t("s_ot_nie_bp")).next().is_some());
    assert!(harness.query_all_by_label_contains(&tf("s_ot_namen", &[("n", Arg::from(2))])).next().is_some());
    harness.get_by_label(&t("s_ot_teilen_ja")).click();
    let start = std::time::Instant::now();
    while puts(&sent).is_empty() && start.elapsed() < std::time::Duration::from_secs(10) {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(puts(&sent)[0].contains("P4-AR Rifle"), "{:?}", puts(&sent));
}
