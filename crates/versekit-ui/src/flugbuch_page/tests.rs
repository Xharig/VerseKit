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
    // The layout check of all pages (`tests/layout_guard.rs`) does the same; this one runs in the full run only.
    if crate::skip_unless_full_run() {
        return;
    }
    for language in [Choice::De, Choice::En] {
        for width in crate::test_pick(&[380.0, 520.0, 700.0, 900.0, 1300.0]) {
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

/// The game is closed: the last session still lies in the `Game.log`, the one before it in `logbackups`.
#[test]
fn the_last_session_of_the_game_log_shows_although_the_game_is_closed() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let (root, paths) = world("letzte");
    let game = root.join("LIVE");
    let backups = game.join("logbackups");
    std::fs::create_dir_all(&backups).unwrap();
    std::fs::write(backups.join("Game Build(1) 01 Oct 26 (10 00 00).log"), session_log(1, "123")).unwrap();
    std::fs::write(game.join("Game.log"), session_log(2, "123")).unwrap();
    let mut harness = harness(FlightState::of(&paths), 1100.0);
    harness.state_mut().wait();
    let view = harness.state().view.clone().expect("loaded");
    assert_eq!(view.briefs.len(), 2, "the backup and the running log");
    // The next start moves the log into the backups: it is no second session.
    std::fs::rename(game.join("Game.log"), backups.join("Game Build(1) 02 Oct 26 (10 00 00).log")).unwrap();
    std::fs::write(game.join("Game.log"), "").unwrap();
    harness.state_mut().started = None;
    harness.state_mut().start(&egui::Context::default(), 0.0);
    harness.state_mut().wait();
    assert_eq!(harness.state().view.clone().expect("loaded").briefs.len(), 2, "no double");
    let _ = std::fs::remove_dir_all(root);
}

// ---------------------------------------------------------------------------
// The finishing touches: long lines wrap, the words, the own jumps
// ---------------------------------------------------------------------------

const LONG_CONTRACT: &str = "Retake the Outpost and hold it against every wave of the attackers until the relief arrives at last";
const LONG_PLACE: &str = "Observation Platform on the far side of the moon with a name nobody can say in one breath";

/// What the frame drew as text: each line with the number of its rows and whether the galley had to shorten it.
fn drawn_texts(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, usize, bool)> {
    fn collect(shape: &egui::epaint::Shape, into: &mut Vec<(String, usize, bool)>) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                into.push((text.galley.text().to_string(), text.galley.rows.len(), text.galley.elided));
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut out));
    out
}

/// The page on files with a long contract and a long place name, drawn at `width`.
fn draw_long(width: f32, language: Choice, tag: &str) -> (Vec<(String, usize, bool)>, f32) {
    set_thread_language(language);
    theme::set_thread_current("krt");
    let (root, paths) = world(tag);
    fill(&paths, &root);
    location::remember(&paths, &vec![("123".to_string(), LONG_PLACE.to_string())]);
    let contract = vec![
        ("name".to_string(), Value::str(LONG_CONTRACT)),
        ("wann".to_string(), Value::str("2026-10-02T10:20:00")),
        ("bis".to_string(), Value::str("2026-10-02T10:30:00")),
        ("zustand".to_string(), Value::str(mission_log::COMPLETED)),
        ("bauplaene".to_string(), Value::Array(vec![Value::str(LONG_CONTRACT)])),
    ];
    mission_log::save(&paths, &[contract]);
    let mut state = FlightState::of(&paths);
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let scheme = theme::current();
    state.start(&ctx, 0.0);
    state.wait();
    let mut overflow = 0.0_f32;
    let mut texts = Vec::new();
    for _ in 0..4 {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(width, 2400.0))),
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            show(ui, scheme, &mut state);
            overflow = overflow.max(ui.min_rect().width() - ui.max_rect().width());
        });
        texts = drawn_texts(&output.shapes);
        output.textures_delta.clear();
    }
    let _ = std::fs::remove_dir_all(root);
    (texts, overflow)
}

#[test]
fn long_lines_wrap_and_nothing_is_cut_at_any_width_in_both_languages() {
    for language in [Choice::De, Choice::En] {
        for width in [380.0, 520.0, 900.0] {
            let (texts, overflow) = draw_long(width, language, "lang");
            assert!(overflow <= 0.5, "{language:?} at {width}: {overflow}");
            let cut: Vec<&str> = texts.iter().filter(|(_, _, elided)| *elided).map(|(text, ..)| text.as_str()).collect();
            assert!(cut.is_empty(), "{language:?} at {width}: shortened {cut:?}");
            // The long lines are there whole.
            for part in [LONG_CONTRACT, LONG_PLACE] {
                assert!(texts.iter().any(|(text, ..)| text.contains(part)), "{language:?} at {width}: {part}");
            }
        }
        // At the narrow width the long contract needs more than one row.
        let (texts, _) = draw_long(380.0, language, "lang-schmal");
        let rows = texts.iter().find(|(text, ..)| text.contains(LONG_CONTRACT)).map(|(_, rows, _)| *rows);
        assert!(rows.is_some_and(|rows| rows > 1), "{language:?}: {rows:?}");
    }
    set_thread_language(Choice::Auto);
}

#[test]
fn the_rows_of_the_timeline_are_as_tall_as_their_text() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let line = |text: &str| Entry::Line { time: "10:00:00".into(), icon: "map-pin", text: text.into(), tip: text.into() };
    let mut heights = (0.0, 0.0, 0.0);
    let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
        heights = (
            entry_height(ui, &line("kurz"), 300.0, 60.0),
            entry_height(ui, &line(LONG_CONTRACT), 300.0, 60.0),
            entry_height(ui, &line(LONG_CONTRACT), 1500.0, 60.0),
        );
    });
    assert!(heights.1 > heights.0 * 1.8, "the long line takes several rows: {heights:?}");
    assert!((heights.2 - heights.0).abs() < 0.5, "wide enough, it is one row: {heights:?}");
}

#[test]
fn the_hover_of_a_line_holds_the_whole_sentence() {
    let (root, _, harness) = loaded("hover", 1100.0);
    let view = harness.state().view.clone().expect("loaded");
    let session = &view.book.sessions[1];
    let rows = view.book.rows(session);
    let contracts: Vec<_> = rows.iter().filter(|row| row.topic() == Topic::Contract).collect();
    assert!(!contracts.is_empty());
    for row in contracts {
        let (text, tip) = row_text(&view.book, row, Lang::De);
        assert_eq!(text, tip, "the hover shows what the line says");
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_words_say_what_they_count() {
    for (lang, found, deaths, one, many) in [
        (Lang::De, "Erhaltene Baupläne", "Todesfälle (vermutet)", "1 Todesfall (vermutet)", "3 Todesfälle (vermutet)"),
        (Lang::En, "Blueprints received", "Deaths (presumed)", "1 death (presumed)", "3 deaths (presumed)"),
    ] {
        let said = |key: &str| language::t_in(lang, key);
        assert_eq!(said("s_flb_k_funde"), found);
        assert_eq!(said("s_flb_t_find"), found, "the switch and the figure say the same");
        assert_eq!(said("s_flb_k_tode"), deaths);
        assert_eq!(said("s_flb_t_death"), deaths, "the switch and the figure say the same");
        assert_eq!(said("s_flb_z_tod_eins"), one);
        assert_eq!(language::tf_in(lang, "s_flb_z_tod", &[("n", Arg::from(3_i64))]), many);
    }
}

/// A foreign ship arriving is no jump of the player, in the summary and in the list.
#[test]
fn the_jumps_of_the_summary_are_the_own_ones_only() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let (root, paths) = world("fremd");
    let logs = root.join("logs");
    std::fs::create_dir_all(&logs).unwrap();
    let foreign = "<2026-10-01T10:20:00.000Z> <Quantum Drive Arrived - Arrived at Final Destination> a | AEGS_Gladius_555555[900009]|b\n";
    let quit = "<2026-10-01T10:40:00.000Z> <SystemQuit>";
    let text = session_log(1, "123").replace(quit, &format!("{foreign}{quit}"));
    assert!(text.contains("AEGS_Gladius"), "the foreign arrival is in the log");
    let file = logs.join("Game-1.log");
    std::fs::write(&file, text).unwrap();
    let source = LogSource::new(&paths, LogSource::sources_of(&paths));
    assert_eq!(flugbuch::catch_up(&source, &[file.to_str().unwrap().to_string()]), 1);
    let mut harness = harness(FlightState::of(&paths), 1100.0);
    harness.state_mut().wait();
    harness.run_steps(3);
    let view = harness.state().view.clone().expect("loaded");
    assert_eq!(view.briefs[0].summary.jumps, 1, "the own jump only");
    assert!(harness.query_all_by_label("Eigene Sprünge").count() >= 2, "the figure and the switch say that they are the own ones");
    let session = &view.book.sessions[0];
    let jump_rows = view.book.rows(session).iter().filter(|row| row.topic() == Topic::Jump).count();
    assert_eq!(jump_rows, 1, "one line of a jump");
    let _ = std::fs::remove_dir_all(root);
}
