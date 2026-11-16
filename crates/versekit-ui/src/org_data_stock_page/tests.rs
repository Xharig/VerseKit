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

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::orgstd::hub::{Hub, Kind};
use versekit_core::theme;

use super::*;
use crate::org_connect_page::OrgConnectState;

struct Page {
    state: OrgLotsState,
    orgs: OrgConnectState,
}

fn harness() -> Harness<'static, Page> {
    set_thread_language(Choice::De);
    let page = Page { state: OrgLotsState::default(), orgs: OrgConnectState::sample() };
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

fn lot(item: &str, quantity: f64, unit: Unit, quality: Option<u16>, location: &str, member: &str) -> OrgLot {
    OrgLot { item: item.into(), quantity, unit, quality, location: location.into(), member: member.into() }
}

#[test]
fn the_page_shows_the_stock_of_the_org_in_both_languages() {
    for (choice, ware, scu) in [(Choice::De, "Laranite", "42,5 SCU"), (Choice::En, "Laranite", "42.5 SCU")] {
        let mut harness = harness();
        set_thread_language(choice);
        harness.run_steps(2);
        assert!(harness.query_all_by_label_contains(ware).next().is_some(), "{ware}");
        assert!(harness.query_all_by_label_contains(scu).next().is_some(), "{scu}");
        assert!(harness.query_all_by_label_contains(&t("s_ol_bestand")).next().is_some());
    }
    set_thread_language(Choice::Auto);
}

#[test]
fn a_row_without_a_value_stands_last_whichever_way_the_column_is_sorted() {
    let lots = vec![
        lot("A", 1.0, Unit::Scu, None, "", ""),
        lot("B", 2.0, Unit::Scu, Some(900), "Area18", "Vega"),
        lot("C", 3.0, Unit::Scu, Some(100), "Levski", "Mira"),
    ];
    let mut state = OrgLotsState::default();
    for ascending in [true, false] {
        state.sort = (Column::Quality, ascending);
        let rows = arranged(&state, &lots, &[]);
        assert_eq!(*rows.last().unwrap(), 0, "unknown quality last, ascending {ascending}");
    }
    for column in [Column::Place, Column::Member] {
        state.sort = (column, false);
        assert_eq!(*arranged(&state, &lots, &[]).last().unwrap(), 0, "{column:?}");
    }
}

#[test]
fn search_and_filters_narrow_the_rows_and_all_words_must_fit() {
    let lots = vec![
        lot("Laranite", 5.0, Unit::Scu, Some(10), "Area18", "Vega"),
        lot("Laranite", 5.0, Unit::Scu, Some(10), "Levski", "Mira"),
        lot("Venture Arms", 1.0, Unit::Piece, None, "Area18", "Rook"),
    ];
    let places = vec!["Area18".to_string(), "Levski".to_string()];
    let mut state = OrgLotsState::default();
    assert_eq!(arranged(&state, &lots, &places).len(), 3);
    state.search = "lara vega".into();
    assert_eq!(arranged(&state, &lots, &places), [0]);
    state.search.clear();
    state.kind = 2;
    assert_eq!(arranged(&state, &lots, &places), [2]);
    state.kind = 0;
    state.place = 2;
    assert_eq!(arranged(&state, &lots, &places), [1]);
}

#[test]
fn new_data_do_not_reset_the_search_the_filters_or_the_sorting() {
    let mut harness = harness();
    {
        let page = harness.state_mut();
        page.state.search = "agricium".into();
        page.state.kind = 1;
        page.state.sort = (Column::Amount, false);
        page.state.sums = true;
        let data = crate::org_data::sample_data(1_800_000_000);
        page.orgs.link().hub.preload(crate::org_connect_page::SAMPLE_ADDRESS, data);
    }
    harness.run_steps(3);
    let page = harness.state();
    assert_eq!(page.state.search, "agricium");
    assert_eq!((page.state.kind, page.state.sort, page.state.sums), (1, (Column::Amount, false), true));
    assert!(harness.query_by_label_contains("Agricium").is_some());
    assert!(harness.query_by_label_contains("Hadanite").is_none(), "the search still narrows");
}

#[test]
fn the_sums_add_up_equal_wares_of_all_members() {
    let mut harness = harness();
    harness.state_mut().state.sums = true;
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("60,5 SCU").is_some(), "42,5 and 18 SCU of two members are one group");
    assert!(harness.query_by_label_contains(&tf("s_ol_mitglieder", &[("n", Arg::from(2))])).is_some());
}

#[test]
fn the_page_asks_for_data_when_it_opens_and_never_on_a_clock() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use versekit_core::basetool::error::ApiError;
    use versekit_core::basetool::http::{Request, Response, Transport};
    use versekit_core::orgstd::client::OrgClient;
    use versekit_core::orgstd::store::Store;

    struct Counting(Arc<AtomicUsize>);
    impl Transport for Counting {
        fn send(&self, _: &Request) -> Result<Response, ApiError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let body = r#"{ "items": [{ "item": "Laranite", "quantity": 4, "unit": "scu" }] }"#;
            Ok(Response { status: 200, headers: vec![], body: body.as_bytes().to_vec() })
        }
    }

    let count = Arc::new(AtomicUsize::new(0));
    let client = OrgClient::new(Arc::new(Counting(count.clone())), "5.4.0");
    let mut connection = crate::org_connect_page::sample_connection();
    connection.access_expires = u64::MAX / 2;
    // A clock that runs ten minutes with every look: whatever was fetched is always old again, so a page that asked
    // on every frame would ask on every frame.
    let ticks = Arc::new(AtomicUsize::new(0));
    let clock: versekit_core::orgstd::hub::Clock =
        Arc::new(move || 1_800_000_000 + 700 * ticks.fetch_add(1, Ordering::SeqCst) as u64);
    let hub = Hub::new(client, Store::memory(vec![connection]), clock);
    let orgs = OrgConnectState::with_hub(hub.clone(), false);
    orgs.link().areas.set(crate::org_connect_page::SAMPLE_ADDRESS, Kind::Stock, true);
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let mut state = OrgLotsState::default();
    let draw = |state: &mut OrgLotsState| {
        let link = orgs.link();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| show_page(ui, theme::current(), state, &link));
    };
    draw(&mut state);
    let start = std::time::Instant::now();
    while hub.working() && start.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(count.load(Ordering::SeqCst), 1, "one request when the page opens");
    for _ in 0..30 {
        draw(&mut state);
    }
    assert_eq!(count.load(Ordering::SeqCst), 1, "frames of an open page ask nothing");
    assert_eq!(hub.data(crate::org_connect_page::SAMPLE_ADDRESS).stock.rows.len(), 1);
}

#[test]
fn the_open_watch_notices_a_page_that_was_left_and_comes_back() {
    let ctx = egui::Context::default();
    let mut watch = crate::org_data::OpenWatch::default();
    let frame = |watch: &mut crate::org_data::OpenWatch, drawn: bool| {
        let mut opened = false;
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {
            if drawn {
                opened = watch.opened(&ctx);
            }
        });
        opened
    };
    assert!(frame(&mut watch, true), "the first frame opens it");
    assert!(!frame(&mut watch, true), "the next frame is the same page");
    assert!(!frame(&mut watch, false), "another page is drawn");
    assert!(frame(&mut watch, true), "back again: opened once more");
}
