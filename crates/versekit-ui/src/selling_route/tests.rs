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

use egui_kittest::kittest::Queryable;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;

use super::*;

fn prices() -> Selling {
    let now = trade::now() as i64;
    let offer = |terminal: &str, name: &str, price: f64| {
        format!(r#"{{"t":"{terminal}","n":"{name}","p":{price},"d":{now},"k":"","z":3}}"#)
    };
    let text = format!(
        r#"{{"terminals":{{
            "7":{{"o":"Area18","s":"Stanton","n":"TDD","q":false}},
            "8":{{"o":"Lorville","s":"Stanton","n":"Teasa Spaceport","q":false}}}},"geholt":{now},"waren":{{
            "Iron":[{},{}],"Gold":[{},{}]}}}}"#,
        offer("7", "TDD", 1_500.0),
        offer("8", "Teasa Spaceport", 2_200.0),
        offer("7", "TDD", 9_000.0),
        offer("8", "Teasa Spaceport", 4_000.0),
    );
    Selling::from_value(&versekit_core::jsonio::loads(&text).expect("json"))
}

fn picks() -> Vec<String> {
    vec!["Iron".to_string(), "Gold".to_string()]
}

fn amounts() -> HashMap<String, f64> {
    HashMap::from([("Iron".to_string(), 10.0), ("Gold".to_string(), 5.0)])
}

/// What a test draws the page of: the state and everything it is calculated from.
struct Rig {
    state: RouteState,
    data: Selling,
    paths: Option<Arc<Paths>>,
}

impl Rig {
    fn new() -> Rig {
        Rig { state: RouteState::default(), data: prices(), paths: None }
    }

    fn inputs<'a>(&'a self, picks: &'a [String], amounts: &'a HashMap<String, f64>) -> Inputs<'a> {
        Inputs { data: &self.data, picks, amounts, nqa: false, paths: self.paths.as_ref(), version: 1 }
    }
}

fn harness(rig: Rig) -> egui_kittest::Harness<'static, Rig> {
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1000.0, 1600.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, rig: &mut Rig| {
                let (picks, amounts) = (picks(), amounts());
                rig.state.poll(ui);
                let inputs = Inputs {
                    data: &rig.data,
                    picks: &picks,
                    amounts: &amounts,
                    nqa: false,
                    paths: rig.paths.as_ref(),
                    version: 1,
                };
                results(ui, versekit_core::theme::current(), &mut rig.state, &inputs);
            },
            rig,
        );
    harness.run_steps(3);
    harness
}

fn stop_names(rig: &Rig) -> Vec<String> {
    let (picks, amounts) = (picks(), amounts());
    let inputs = rig.inputs(&picks, &amounts);
    let request = rig.state.request(&inputs).expect("goods to sell");
    sell_route::plan(&rig.data, &request.query, &Distances::default()).stops.into_iter().map(|s| s.name).collect()
}

#[test]
fn the_route_names_its_stations_the_missing_distance_with_its_reason_and_the_rule_of_the_order() {
    set_thread_language(Choice::De);
    let harness = harness(Rig::new());
    let keys = ["s_vk_ro_strecke_unbekannt", "s_vk_ro_voll", "s_vk_ro_verkauft", "s_vk_ro_regel"];
    for text in ["Teasa Spaceport", "TDD"].map(String::from).into_iter().chain(keys.map(t)) {
        assert!(has(&harness, &text), "{text:?} fehlt");
    }
    let german = t("s_vk_ro_strecke_unbekannt");
    set_thread_language(Choice::En);
    let harness = self::harness(Rig::new());
    for text in keys.map(t) {
        assert!(has(&harness, &text), "{text:?} missing");
    }
    assert_ne!(t("s_vk_ro_strecke_unbekannt"), german, "the reason is written in both languages");
    set_thread_language(Choice::Auto);
}

/// Is a node with a label that contains `text` on the page?
fn has(harness: &egui_kittest::Harness<'static, Rig>, text: &str) -> bool {
    harness.query_all_by_label_contains(text).next().is_some()
}

#[test]
fn a_station_reported_full_is_left_out_and_comes_back_with_a_click() {
    set_thread_language(Choice::De);
    let mut rig = Rig::new();
    assert_eq!(stop_names(&rig).len(), 2);
    rig.state.report_full("8", "Teasa Spaceport");
    assert_eq!(stop_names(&rig), ["TDD"], "without the full station all goods go to the other one");
    let mut harness = harness(rig);
    assert!(has(&harness, &t("s_vk_ro_gesperrt")));
    assert!(has(&harness, "Teasa Spaceport"), "named in the list of the full ones");
    harness.get_by_label_contains(&t("s_vk_ro_zuruecksetzen")).click();
    harness.run_steps(3);
    assert!(!has(&harness, &t("s_vk_ro_gesperrt")));
    assert_eq!(harness.state().state.route().map(|route| route.stops.len()), Some(2));
    set_thread_language(Choice::Auto);
}

#[test]
fn the_button_full_here_calculates_again_without_that_station() {
    set_thread_language(Choice::De);
    let mut harness = harness(Rig::new());
    assert_eq!(harness.state().state.route().map(|route| route.stops.len()), Some(2));
    harness.get_all_by_label_contains(&t("s_vk_ro_voll")).next().expect("a button").click();
    harness.run_steps(3);
    let route = harness.state().state.route().expect("a route");
    assert_eq!(route.stops.len(), 1, "one station is left");
    assert_eq!(harness.state().state.blocked.len(), 1);
    set_thread_language(Choice::Auto);
}

#[test]
fn what_was_sold_is_taken_off_and_the_rest_starts_at_the_last_station() {
    let mut rig = Rig::new();
    rig.state.sold_at("8", "Teasa Spaceport", vec![("Iron".to_string(), 10.0)]);
    let (picks, amounts) = (picks(), amounts());
    let request = rig.state.request(&rig.inputs(&picks, &amounts)).expect("gold is left");
    assert_eq!(request.query.items, [("Gold".to_string(), 5.0)]);
    assert_eq!(request.query.start.as_deref(), Some("8"));
    assert!(request.query.blocked.iter().any(|b| b == "8"), "the station sold at is no candidate again");
    rig.state.sold_at("7", "TDD", vec![("Gold".to_string(), 5.0)]);
    assert!(rig.state.request(&rig.inputs(&picks, &amounts)).is_none(), "nothing is left to sell");
}

#[test]
fn nothing_without_an_amount_is_calculated() {
    let rig = Rig::new();
    let picks = picks();
    let none = HashMap::new();
    assert!(rig.state.request(&rig.inputs(&picks, &none)).is_none());
}

#[test]
fn a_calculation_in_the_job_shows_the_spinner_and_then_the_route_without_asking_the_net() {
    set_thread_language(Choice::De);
    let dir = std::env::temp_dir().join(format!("versekit-verkaufsroute-ui-{}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    let home = dir.to_string_lossy().into_owned();
    let vars = [("VERSEKIT_HOME", home.as_str()), (versekit_core::envvars::ENV_NO_NET, "1")];
    let paths = Arc::new(Paths::new(Host::new(Flavor::native(), &vars, &home)));
    let mut rig = Rig::new();
    rig.paths = Some(paths);
    let ctx = egui::Context::default();
    crate::install(&ctx);
    // One frame of the page: the texts it draws.
    let frame = |rig: &mut Rig| -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let (picks, amounts) = (picks(), amounts());
            rig.state.poll(ui);
            let inputs = Inputs {
                data: &rig.data,
                picks: &picks,
                amounts: &amounts,
                nqa: false,
                paths: rig.paths.as_ref(),
                version: 1,
            };
            results(ui, versekit_core::theme::current(), &mut rig.state, &inputs);
        });
        output.textures_delta.clear();
        let mut out = Vec::new();
        output.shapes.iter().for_each(|clipped| walk(&clipped.shape, &mut out));
        out
    };
    let first = frame(&mut rig);
    assert!(rig.state.is_calculating(), "the calculation runs in a job");
    assert!(first.iter().any(|text| text.contains(&t("s_vk_ro_rechnet"))), "the spinner says so: {first:?}");
    assert!(!first.iter().any(|text| text.contains(&t("s_vk_ro_voll"))), "no card before the result");
    let mut shown = first;
    for _ in 0..300 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        shown = frame(&mut rig);
        if !rig.state.is_calculating() {
            break;
        }
    }
    assert!(shown.iter().any(|text| text.contains("Teasa Spaceport")), "the route stands there: {shown:?}");
    assert!(!shown.iter().any(|text| text.contains(&t("s_vk_ro_rechnet"))));
    let _ = std::fs::remove_dir_all(dir);
    set_thread_language(Choice::Auto);
}

#[test]
fn the_settings_name_the_systems_and_the_stations_count_is_kept() {
    let data = prices();
    assert_eq!(RouteState::systems(&data), [("Stanton".to_string(), 2)]);
    let (labels, idents) = start_labels(&data);
    assert_eq!(labels.len(), idents.len());
    assert!(labels.iter().any(|label| label.contains("Area18")), "{labels:?}");
}
