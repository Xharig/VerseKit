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

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::orgstd::data::{OrderLine, Unit};
use versekit_core::orgstd::hub::Kind;
use versekit_core::theme;

use super::*;
use crate::org_connect_page::OrgConnectState;

struct Page {
    state: OrgOrdersState,
    orgs: OrgConnectState,
}

fn harness() -> Harness<'static, Page> {
    set_thread_language(Choice::De);
    let page = Page { state: OrgOrdersState::default(), orgs: OrgConnectState::sample() };
    let mut harness = Harness::builder()
        .with_size(vec2(1100.0, 1600.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, page: &mut Page| {
                let link = page.orgs.link();
                show_page(ui, theme::current(), &mut page.state, &link)
            },
            page,
        );
    harness.run_steps(3);
    harness
}

fn line(quantity: f64, delivered: f64) -> OrderLine {
    OrderLine { item: "Laranite".into(), quantity, unit: Unit::Scu, delivered }
}

fn order(lines: Vec<OrderLine>) -> OrgOrder {
    OrgOrder {
        id: "x".into(),
        title: "Test".into(),
        status: OrderStatus::Open,
        location: String::new(),
        due: String::new(),
        note: String::new(),
        lines,
    }
}

/// What a transport saw of a booking: its key.
type Keys = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

/// Fails the first `fail` requests as the net does, then answers every booking with the order as it stands.
struct Flaky {
    fail: std::sync::atomic::AtomicUsize,
    keys: Keys,
}

impl versekit_core::basetool::http::Transport for Flaky {
    fn send(
        &self,
        request: &versekit_core::basetool::http::Request,
    ) -> Result<versekit_core::basetool::http::Response, versekit_core::basetool::error::ApiError> {
        use std::sync::atomic::Ordering;
        if let Some((_, key)) = request.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("Idempotency-Key")) {
            self.keys.lock().unwrap().push(key.clone());
        }
        if self.fail.load(Ordering::SeqCst) > 0 {
            self.fail.fetch_sub(1, Ordering::SeqCst);
            return Err(versekit_core::basetool::error::ApiError::network("test"));
        }
        let body = r#"{"delivery_id":"d1","order":{"id":"a1","title":"Laranite","status":"open",
            "items":[{"item":"Laranite","quantity":100,"unit":"scu","delivered":50},
                     {"item":"Agricium","quantity":50,"unit":"scu","delivered":50}]}}"#;
        Ok(versekit_core::basetool::http::Response { status: 201, headers: vec![], body: body.as_bytes().to_vec() })
    }
}

fn booking_page(fail: usize) -> (Harness<'static, Page>, Keys) {
    use versekit_core::orgstd::client::OrgClient;
    use versekit_core::orgstd::hub::{Hub, system_clock};
    use versekit_core::orgstd::store::Store;
    set_thread_language(Choice::De);
    let keys = Keys::default();
    let client = OrgClient::new(
        std::sync::Arc::new(Flaky { fail: std::sync::atomic::AtomicUsize::new(fail), keys: keys.clone() }),
        "5.4.0",
    );
    let mut connection = crate::org_connect_page::sample_connection();
    connection.access_expires = u64::MAX / 2;
    let hub = Hub::new(client, Store::memory(vec![connection]), system_clock());
    let orgs = OrgConnectState::with_hub(hub.clone(), false);
    orgs.link().areas.set(crate::org_connect_page::SAMPLE_ADDRESS, Kind::Orders, true);
    hub.preload(crate::org_connect_page::SAMPLE_ADDRESS, crate::org_data::sample_data(1_800_000_000));
    let mut harness = Harness::builder()
        .with_size(vec2(1100.0, 1800.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, page: &mut Page| {
                let link = page.orgs.link();
                show_page(ui, theme::current(), &mut page.state, &link)
            },
            Page { state: OrgOrdersState::default(), orgs },
        );
    harness.run_steps(3);
    (harness, keys)
}

fn wait_for_label(harness: &mut Harness<'_, Page>, text: &str) {
    let start = std::time::Instant::now();
    while harness.query_all_by_label_contains(text).next().is_none() {
        harness.run_steps(1);
        assert!(start.elapsed() < std::time::Duration::from_secs(10), "{text:?} never showed");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn a_booking_sent_again_after_a_network_error_carries_the_same_key_and_a_new_form_a_new_one() {
    let (mut harness, keys) = booking_page(1);
    harness.get_by_role_and_label(Role::Button, &tf("s_oa_zeile_name", &[("titel", Arg::from("Laranite"))])).click();
    harness.run_steps(2);
    assert!(keys.lock().unwrap().is_empty(), "nothing is booked by opening an order");
    harness.get_by_label(&tf("s_oa_buchen_name", &[("titel", Arg::from("Laranite"))])).click();
    harness.run_steps(2);
    harness.get_all_by_role(Role::TextInput).next().unwrap().focus();
    harness.run_steps(1);
    harness.get_all_by_role(Role::TextInput).next().unwrap().type_text("10");
    harness.run_steps(1);
    harness.get_by_label(&t("s_oa_buchen_ja")).click();
    wait_for_label(&mut harness, &t("s_oa_f_netz"));
    assert_eq!(keys.lock().unwrap().len(), 1, "the first try went out and failed at the net");
    harness.get_by_label(&t("s_oa_buchen_ja")).click();
    wait_for_label(&mut harness, &tf("s_oa_gebucht", &[("liste", Arg::from(""))]).replace("{liste}", ""));
    let first = keys.lock().unwrap().clone();
    assert_eq!(first.len(), 2);
    assert_eq!(first[0], first[1], "the try again is the same booking");
    // The next form is another booking: another key.
    harness.get_by_label(&tf("s_oa_buchen_name", &[("titel", Arg::from("Laranite"))])).click();
    harness.run_steps(2);
    harness.get_all_by_role(Role::TextInput).next().unwrap().focus();
    harness.run_steps(1);
    harness.get_all_by_role(Role::TextInput).next().unwrap().type_text("5");
    harness.run_steps(1);
    harness.get_by_label(&t("s_oa_buchen_ja")).click();
    let start = std::time::Instant::now();
    while keys.lock().unwrap().len() < 3 && start.elapsed() < std::time::Duration::from_secs(10) {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let all = keys.lock().unwrap().clone();
    assert_eq!(all.len(), 3);
    assert_ne!(all[1], all[2], "a new form, a new key");
}

#[test]
fn the_same_key_stays_while_the_booking_fails_again_and_again() {
    let (mut harness, keys) = booking_page(2);
    harness.get_by_role_and_label(Role::Button, &tf("s_oa_zeile_name", &[("titel", Arg::from("Laranite"))])).click();
    harness.run_steps(2);
    harness.get_by_label(&tf("s_oa_buchen_name", &[("titel", Arg::from("Laranite"))])).click();
    harness.run_steps(2);
    harness.get_all_by_role(Role::TextInput).next().unwrap().focus();
    harness.run_steps(1);
    harness.get_all_by_role(Role::TextInput).next().unwrap().type_text("10");
    harness.run_steps(1);
    for tries in 1..=2 {
        harness.get_by_label(&t("s_oa_buchen_ja")).click();
        // The error of the try before stays on screen until this try reached the net: wait for that first.
        let start = std::time::Instant::now();
        while keys.lock().unwrap().len() < tries {
            harness.run_steps(1);
            assert!(start.elapsed() < std::time::Duration::from_secs(10), "try {tries} never went out");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        wait_for_label(&mut harness, &t("s_oa_f_netz"));
        harness.run_steps(2);
    }
    harness.get_by_label(&t("s_oa_buchen_ja")).click();
    let start = std::time::Instant::now();
    while keys.lock().unwrap().len() < 3 && start.elapsed() < std::time::Duration::from_secs(10) {
        harness.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let all = keys.lock().unwrap().clone();
    assert_eq!(all.len(), 3, "three tries");
    assert!(all.iter().all(|k| k == &all[0]), "one key for all of them: {all:?}");
}

#[test]
fn nothing_to_book_without_a_quantity_and_a_wrong_number_is_named() {
    let (mut harness, keys) = booking_page(0);
    harness.get_by_role_and_label(Role::Button, &tf("s_oa_zeile_name", &[("titel", Arg::from("Laranite"))])).click();
    harness.run_steps(2);
    harness.get_by_label(&tf("s_oa_buchen_name", &[("titel", Arg::from("Laranite"))])).click();
    harness.run_steps(2);
    harness.get_by_label(&t("s_oa_buchen_ja")).click();
    harness.run_steps(3);
    assert!(harness.query_all_by_label_contains(&t("s_oa_nichts")).next().is_some());
    harness.get_all_by_role(Role::TextInput).next().unwrap().focus();
    harness.run_steps(1);
    harness.get_all_by_role(Role::TextInput).next().unwrap().type_text("viel");
    harness.run_steps(1);
    harness.get_by_label(&t("s_oa_buchen_ja")).click();
    harness.run_steps(3);
    assert!(
        harness
            .query_all_by_label_contains(&tf("s_oa_keine_zahl", &[("ware", Arg::from("Laranite"))]))
            .next()
            .is_some()
    );
    assert!(keys.lock().unwrap().is_empty(), "nothing was sent");
}

#[test]
fn a_refused_booking_says_why_by_its_status() {
    use versekit_core::orgstd::{E_SERVER, OrgError};
    let status = |code: u16| OrgError { code: E_SERVER, status: code, retry_after: None };
    assert_eq!(booking_error_text(status(404)), t("s_oa_f_404"));
    assert_eq!(booking_error_text(status(409)), t("s_oa_f_409"));
    assert_eq!(booking_error_text(status(422)), t("s_oa_f_422"));
    assert_eq!(booking_error_text(status(400)), t("s_oa_f_400"));
    assert_eq!(booking_error_text(status(500)), t("s_oa_f_sonst"));
    assert_eq!(booking_error_text(OrgError::new(versekit_core::orgstd::E_NETWORK)), t("s_oa_f_netz"));
    assert_eq!(
        booking_error_text(OrgError { code: versekit_core::orgstd::E_SCOPE, status: 403, retry_after: None }),
        t("s_od_f_recht")
    );
}

#[test]
fn the_progress_counts_what_is_delivered_without_more_than_asked() {
    assert_eq!(progress_of(&order(vec![line(100.0, 40.0)])), 0.4);
    assert_eq!(
        progress_of(&order(vec![line(10.0, 30.0), line(10.0, 0.0)])),
        0.5,
        "an over-delivery is no more than done"
    );
    assert_eq!(progress_of(&order(Vec::new())), 0.0);
    assert_eq!(progress_of(&order(vec![line(0.0, 5.0)])), 0.0);
}

#[test]
fn the_open_orders_show_with_their_progress_in_both_languages_and_the_closed_ones_only_on_request() {
    for (choice, open) in [(Choice::De, "offen"), (Choice::En, "open")] {
        let mut harness = harness();
        set_thread_language(choice);
        harness.run_steps(2);
        assert!(harness.query_all_by_label_contains("Laranite").next().is_some());
        assert!(harness.query_all_by_label_contains("60 %").next().is_some(), "the progress of the open order");
        assert!(harness.query_all_by_label_contains(open).next().is_some());
        assert!(harness.query_all_by_label_contains("Quantanium").next().is_none(), "the done order stays away");
        harness.state_mut().state.closed = true;
        harness.run_steps(2);
        assert!(harness.query_all_by_label_contains("Quantanium").next().is_some());
    }
    set_thread_language(Choice::Auto);
}
