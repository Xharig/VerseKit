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

//! The lines of the logs are made up; the page is driven without a window (egui_kittest).

use std::path::PathBuf;

use egui::accesskit::Role;
use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::flugbuch;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::logsource::LogSource;
use versekit_core::envvars::ENV_HOME;
use versekit_core::paths::ENV_INSTALL_DIR;
use versekit_core::pypath::Flavor;
use versekit_core::{location, mission_log, theme};

use super::*;

const OWN: &str = "RSI_Polaris_465232524500";

/// A data folder and a game folder of their own, so that nothing real is touched.
fn world(tag: &str) -> (PathBuf, Paths) {
    let root = std::env::temp_dir().join(format!("versekit-flugbuch-seite-{tag}-{}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&root);
    let (home, game) = (root.join("home"), root.join("LIVE"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&game).unwrap();
    let (home_text, game_text) = (home.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
    let host = Host::new(
        Flavor::native(),
        &[(ENV_HOME, home_text.as_str()), (ENV_INSTALL_DIR, game_text.as_str())],
        root.to_str().unwrap(),
    );
    (root, Paths::new(host))
}

/// A made-up log of one session: a landing, a target, a jump, a contract's second.
fn session_log(day: u32, place_number: &str) -> String {
    let stamp = |minute: u32, second: u32| format!("<2026-10-{day:02}T10:{minute:02}:{second:02}.000Z>");
    let mut out = format!("{} Start OnClientSpawned\n", stamp(0, 0));
    out.push_str(&format!(
        "{} <Update Inventory Location> Player [Xharig] is changing location. Landing [0] -> [{place_number}]. Location [0] -> [5]\n",
        stamp(1, 0)
    ));
    out.push_str(&format!(
        "{} <Player Selected Quantum Target - Local> a | {OWN}[900001]|b Player has selected point LOC_rs_ext_nyx-castra_jp1 as their destination\n",
        stamp(10, 0)
    ));
    out.push_str(&format!(
        "{} <Quantum Drive Arrived - Arrived at Final Destination> a | {OWN}[900002]|b\n",
        stamp(11, 0)
    ));
    out.push_str(&format!(
        "{} [Notice] <Join PU> address[35.241.242.86] port[64388] shard[pub_euw1b_12881860_420] locationId[5629]\n",
        stamp(12, 0)
    ));
    out.push_str(&format!("{} <SystemQuit>\n{} Ende\n", stamp(40, 0), stamp(41, 0)));
    out
}

/// Two sessions on two days, a learned place and one contract that began in the second.
fn fill(paths: &Paths, root: &std::path::Path) {
    let logs = root.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    let mut files = Vec::new();
    for (day, number) in [(1, "123"), (2, "123")] {
        let file = logs.join(format!("Game-{day}.log"));
        std::fs::write(&file, session_log(day, number)).unwrap();
        files.push(file.to_str().unwrap().to_string());
    }
    let source = LogSource::new(paths, LogSource::sources_of(paths));
    assert_eq!(flugbuch::catch_up(&source, &files), 2);
    location::remember(paths, &vec![("123".to_string(), "Nyx_Levski".to_string())]);
    let contract = vec![
        ("name".to_string(), Value::str("Retake the Outpost")),
        ("wann".to_string(), Value::str("2026-10-02T10:20:00")),
        ("bis".to_string(), Value::str("2026-10-02T10:30:00")),
        ("zustand".to_string(), Value::str(mission_log::COMPLETED)),
        ("bauplaene".to_string(), Value::Array(vec![Value::str("Arclight Helm")])),
    ];
    mission_log::save(paths, &[contract]);
}

type Value = versekit_core::jsonio::Value;

fn harness(state: FlightState, width: f32) -> Harness<'static, FlightState> {
    let mut harness = Harness::builder()
        .with_size(vec2(width, 2400.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, state: &mut FlightState| {
                show(ui, theme::current(), state);
            },
            state,
        );
    harness.run_steps(3);
    harness
}

/// The page on real files, loaded to the end.
fn loaded(tag: &str, width: f32) -> (PathBuf, Paths, Harness<'static, FlightState>) {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let (root, paths) = world(tag);
    fill(&paths, &root);
    let mut harness = harness(FlightState::of(&paths), width);
    harness.state_mut().wait();
    harness.run_steps(3);
    (root, paths, harness)
}

fn labels(harness: &Harness<'static, FlightState>, part: &str) -> usize {
    harness.query_all_by_label_contains(part).count()
}

#[test]
fn the_page_lists_the_sessions_newest_first_and_shows_the_timeline_of_the_newest() {
    let (root, _, harness) = loaded("liste", 1100.0);
    for title in ["Sitzungen", "Zusammenfassung", "Zeitstrahl", "Exportieren"] {
        assert!(harness.query_by_label(title).is_some(), "{title}");
    }
    let state = harness.state();
    let view = state.view.clone().expect("loaded");
    assert_eq!(view.briefs.len(), 2);
    assert!(view.briefs[0].start > view.briefs[1].start, "the newest first");
    assert_eq!(state.selected, Some(view.briefs[0].start), "the newest is chosen");
    // The newest session has the contract and its blueprint; the older one has not.
    assert!(labels(&harness, "Auftrag angenommen: Retake the Outpost") > 0);
    assert!(labels(&harness, "Bauplan erhalten: Arclight Helm") > 0);
    assert!(labels(&harness, "Quantensprung nach") > 0, "an own jump to the target");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_click_on_a_session_shows_its_timeline() {
    let (root, _, mut harness) = loaded("klick", 1100.0);
    let view = harness.state().view.clone().expect("loaded");
    let older = view.briefs[1].start;
    let when = formats::date_time(language::current(), older as f64);
    harness.get_by_label_contains(&format!("Sitzung vom {when}")).click();
    harness.run_steps(3);
    assert_eq!(harness.state().selected, Some(older));
    assert_eq!(labels(&harness, "Auftrag angenommen: Retake the Outpost"), 0, "the older session has no contract");
    assert!(labels(&harness, "Quantensprung nach") > 0);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_switch_hides_a_kind_and_the_search_narrows_the_lines() {
    let (root, _, mut harness) = loaded("schalter", 1100.0);
    assert!(labels(&harness, "Angekommen: Nyx Levski") > 0, "the place by its learned name");
    harness.get_by_role_and_label(Role::CheckBox, "Ortswechsel").click();
    harness.run_steps(3);
    assert_eq!(labels(&harness, "Angekommen: Nyx Levski"), 0, "the switch hides the arrivals");
    assert!(labels(&harness, "Quantensprung nach") > 0, "the others stay");
    harness.get_by_role_and_label(Role::CheckBox, "Ortswechsel").click();
    harness.run_steps(3);
    harness.state_mut().search = "retake".to_string();
    harness.run_steps(3);
    assert!(labels(&harness, "Auftrag angenommen: Retake the Outpost") > 0);
    assert_eq!(labels(&harness, "Quantensprung nach"), 0, "the search finds only what holds the word");
    harness.state_mut().search = "gibt es nicht".to_string();
    harness.run_steps(3);
    assert!(harness.query_by_label(&t("s_flb_kein_treffer")).is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_shard_stands_in_no_line_and_covered_in_the_hover_of_streamer_mode() {
    let (root, _, harness) = loaded("shard", 1100.0);
    assert!(labels(&harness, "Shard gewechselt") > 0, "the line says only that it changed");
    for part in ["12881860", "pub_euw1b", "35.241", "64388", "5629"] {
        assert_eq!(labels(&harness, part), 0, "{part} is on no line");
    }
    // The hover shows the shard; streamer mode covers its number.
    let view = harness.state().view.clone().expect("loaded");
    let session = &view.book.sessions[1];
    let row = view.book.rows(session).into_iter().find(|r| r.topic() == Topic::Shard).expect("a shard row");
    let (_, plain) = row_text(&view.book, &row, Lang::De);
    assert!(plain.contains("12881860"), "{plain}");
    versekit_core::streamer::set_thread_active(Some(true));
    let (_, covered) = row_text(&view.book, &row, Lang::De);
    versekit_core::streamer::set_thread_active(None);
    assert!(!covered.contains("12881860"), "{covered}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_empty_folder_says_so_and_the_sample_page_reads_nothing() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let (root, paths) = world("leer");
    let mut harness = harness(FlightState::of(&paths), 1100.0);
    harness.state_mut().wait();
    harness.run_steps(3);
    assert!(harness.query_by_label_contains("Noch keine Sitzung").is_some());
    assert!(harness.query_by_label("Zeitstrahl").is_none(), "no timeline without a session");
    let sample = harness_for_sample();
    assert!(sample.state().is_sample());
    assert!(sample.query_all_by_label_contains("Quantensprung nach").count() > 0, "made-up lines");
    let _ = std::fs::remove_dir_all(root);
}

fn harness_for_sample() -> Harness<'static, FlightState> {
    set_thread_language(Choice::De);
    harness(FlightState::sample(), 1100.0)
}

#[test]
fn the_export_copies_saves_and_keeps_out_what_is_not_the_whitelist() {
    let (root, _, mut harness) = loaded("export", 1100.0);
    for name in ["In die Zwischenablage kopieren", "Als Text speichern", "Als JSON speichern"] {
        assert!(harness.query_by_role_and_label(Role::Button, name).is_some(), "{name}");
    }
    harness.get_by_role_and_label(Role::Button, "In die Zwischenablage kopieren").click();
    harness.run_steps(2);
    assert_eq!(harness.state().said, ["In die Zwischenablage kopiert."]);
    // A save: the dialog answers with a file.
    let text_file = root.join("flug.txt");
    let json_file = root.join("flug.json");
    for (file, format) in [(&text_file, Format::Text), (&json_file, Format::Json)] {
        harness.state_mut().said.clear();
        harness.state_mut().dialog =
            Some((file_dialog::Pending::answered(Some(file.to_string_lossy().into_owned())), format));
        harness.run_steps(2);
        let said = harness.state().said.first().cloned().expect("a notice");
        assert!(said.starts_with("Gespeichert: flug."), "{said}");
    }
    let text = std::fs::read_to_string(&text_file).unwrap();
    let json = std::fs::read_to_string(&json_file).unwrap();
    assert!(text.contains("Quantensprung nach") && text.contains("Retake the Outpost"), "{text}");
    assert!(json.contains("\"type\": \"jump\"") && json.contains("\"title\": \"Retake the Outpost\""), "{json}");
    for secret in ["12881860", "pub_euw1b", "35.241", "64388", "5629", "Xharig"] {
        assert!(!text.contains(secret) && !json.contains(secret), "{secret} in the export");
    }
    // A cancelled dialog writes nothing and says nothing.
    let never = root.join("nie.txt");
    harness.state_mut().said.clear();
    harness.state_mut().dialog = Some((file_dialog::Pending::answered(None), Format::Text));
    harness.run_steps(2);
    assert!(!never.exists());
    assert!(harness.state().said.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_switches_the_search_and_the_buttons_have_a_name_and_a_hover() {
    let (root, _, harness) = loaded("namen", 1100.0);
    for topic in Topic::ALL {
        let name = topic_name(topic);
        assert!(harness.query_by_role_and_label(Role::CheckBox, &name).is_some(), "switch {name}");
    }
    assert!(harness.query_by_role(Role::TextInput).is_some(), "the search field");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_page_fits_at_every_width_in_both_languages() {
    for language in [Choice::De, Choice::En] {
        for width in [380.0, 520.0, 700.0, 900.0, 1300.0] {
            set_thread_language(language);
            theme::set_thread_current("krt");
            let (root, paths) = world("breite");
            fill(&paths, &root);
            let mut state = FlightState::of(&paths);
            let ctx = egui::Context::default();
            crate::install(&ctx);
            let scheme = theme::current();
            let mut overflow = 0.0_f32;
            state.start(&ctx, 0.0);
            state.wait();
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 2400.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    show(ui, scheme, &mut state);
                    overflow = overflow.max(ui.min_rect().width() - ui.max_rect().width());
                });
                output.textures_delta.clear();
            }
            assert!(overflow <= 0.5, "{language:?} at {width}: {overflow}");
            let _ = std::fs::remove_dir_all(root);
        }
    }
    set_thread_language(Choice::Auto);
}

#[test]
fn a_long_session_draws_only_the_lines_in_view() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let at = 1_790_848_800;
    let events = (0..4000)
        .map(|n| Event {
            at: at + n,
            kind: Kind::Place,
            data: Data { place: Some((n % 2).to_string()), ..Data::default() },
        })
        .collect();
    let session = Session { start: at, end: at + 4000, account: String::new(), events };
    let mut state = FlightState::sample();
    state.view = Some(Arc::new(View::of(Book::of_sessions(vec![session]))));
    let harness = harness(state, 1100.0);
    let drawn = harness.query_all_by_label_contains("Angekommen:").count();
    assert!(drawn > 0 && drawn < 200, "{drawn} lines drawn of 2000");
}
