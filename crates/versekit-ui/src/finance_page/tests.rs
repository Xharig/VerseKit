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

use egui::Pos2;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::envvars::ENV_HOME;
use versekit_core::language::{Choice, Lang, set_thread_language};
use versekit_core::paths::ENV_INSTALL_DIR;
use versekit_core::pypath::Flavor;
use versekit_core::theme;

use super::*;

/// A data folder of its own with a game folder of its own (or the installation of the computer applies).
fn world(tag: &str) -> (versekit_core::testdir::Dir, Paths) {
    let dir = versekit_core::testdir::Dir::new(tag);
    let (home, game) = (dir.path().join("home"), dir.path().join("LIVE"));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&game).unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(3 * 86_400);
    std::fs::write(game.join("Game.log"), "<2026-10-01T10:00:00.000Z> Log started\n").unwrap();
    std::fs::File::options().write(true).open(game.join("Game.log")).unwrap().set_modified(old).unwrap();
    let (home_text, game_text) = (home.to_string_lossy().into_owned(), game.to_string_lossy().into_owned());
    let host = Host::new(
        Flavor::native(),
        &[(ENV_HOME, home_text.as_str()), (ENV_INSTALL_DIR, game_text.as_str())],
        dir.path().to_str().unwrap(),
    );
    (dir, Paths::new(host))
}

fn write(paths: &Paths, name: &str, text: &str) {
    std::fs::write(paths.app_file(name), text).unwrap();
}

/// A journal with one session that began a day ago: a sale of goods the boxes prove (1 000 000), one that is only a
/// request (500 000), a sale in a shop (20 000), a purchase (5 000), a transfer received (40 000) and one sent
/// (15 000), all within the last hours.
fn journal(paths: &Paths, now: i64) {
    let ago = |seconds: i64| now - seconds;
    let deal = |time: i64, price: i64, proof: &str| {
        let proof = if proof.is_empty() { String::new() } else { format!(r#", "nachweis": "{proof}""#) };
        format!(
            r#"{{"zeit": {time}, "klasse": "", "guid": "g", "menge": 1, "preis": {price}.0, "ort": "", "laden": "SCShop"{proof}}}"#
        )
    };
    let transfer =
        |time: i64, way: &str, amount: i64| format!(r#"{{"zeit": {time}, "richtung": "{way}", "betrag": {amount}}}"#);
    write(
        paths,
        "lagerjournal.json",
        &format!(
            r#"{{"format": {format}, "sitzungen": {{"{start}": {{"von": {start}, "bis": {end}, "kaeufe": [{buy}], "verkaeufe": [{sold}], "bewegungen": [], "ueberweisungen": [{moved}]}}}}, "gelesen": {{}}}}"#,
            format = versekit_core::stock_journal::FORMAT,
            start = ago(86_400),
            end = ago(60),
            buy = deal(ago(3000), 5_000, ""),
            sold = [
                deal(ago(4000), 1_000_000, "kiste_weg"),
                deal(ago(3900), 500_000, "angefragt"),
                deal(ago(3800), 20_000, ""),
            ]
            .join(", "),
            moved = [transfer(ago(2000), "erhalten", 40_000), transfer(ago(1900), "gesendet", 15_000)].join(", "),
        ),
    );
}

fn placed(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
    fn collect(shape: &egui::Shape, into: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => into.push(text.galley.text().to_string()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
            _ => {}
        }
    }
    let mut into = Vec::new();
    shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut into));
    into
}

/// Draws the page for a few frames at this width; returns its texts and how far it sticks out.
fn draw(state: &mut FinanceState, width: f32) -> (Vec<String>, f32) {
    thread_local! {
        static CTX: egui::Context = {
            let ctx = egui::Context::default();
            crate::install(&ctx);
            ctx
        };
    }
    let scheme = theme::by_id("original").unwrap();
    let (mut shown, mut overflow) = (Vec::new(), 0.0);
    CTX.with(|ctx| {
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(width, 2400.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                show_page(ui, scheme, state);
                overflow = (ui.min_rect().width() - ui.max_rect().width()).max(0.0);
            });
            shown = placed(&output.shapes);
            output.textures_delta.clear();
        }
    });
    (shown, overflow)
}

fn has(texts: &[String], part: &str) -> bool {
    texts.iter().any(|text| text.contains(part))
}

/// The text after the first one that is `title`: the value of a tile.
fn after(texts: &[String], title: &str) -> String {
    texts[texts.iter().position(|t| t == title).unwrap_or_else(|| panic!("{title}: {texts:?}")) + 1].clone()
}

fn de(value: f64) -> String {
    formats::money(Lang::De, value)
}

/// A real page on a folder with the journal, read to the end.
fn real(tag: &str) -> (versekit_core::testdir::Dir, Paths, FinanceState) {
    let (dir, paths) = world(tag);
    journal(&paths, now_seconds());
    let mut state = FinanceState::of(&paths);
    draw(&mut state, 1000.0);
    state.wait();
    (dir, paths, state)
}

#[test]
fn the_sample_page_shows_the_periods_the_sources_and_the_count_with_made_up_numbers() {
    set_thread_language(Choice::De);
    let mut state = FinanceState::sample();
    let (shown, overflow) = draw(&mut state, 1000.0);
    for part in [
        "Finanzen",
        "7 Tage",
        "Einnahmen",
        "Ausgaben",
        "Ergebnis",
        "aUEC pro Spielstunde",
        "Nach Quelle",
        "Handel (Waren am Terminal)",
        "Läden (Gegenstände)",
        "Überweisungen zwischen Spielern",
        "Konto",
        "Kontoauszug",
        "Zuletzt aus dem Spiel eingetragen am",
        "Stand danach",
        "Auswertung",
    ] {
        // The page title stands in the window, not in the page: only the rest is asked for here.
        if part != "Finanzen" {
            assert!(has(&shown, part), "{part}: {shown:?}");
        }
    }
    assert_eq!(overflow, 0.0);
    // The sample sells goods the log shows only as a request: it is named apart.
    assert!(has(&shown, "angefragt"), "{shown:?}");
}

#[test]
fn a_contract_with_a_reward_is_income_a_source_and_a_line_of_the_statement_and_one_without_stays_a_count() {
    set_thread_language(Choice::De);
    let mut state = FinanceState::sample();
    state.period = Period::Week;
    let (shown, overflow) = draw(&mut state, 1000.0);
    assert_eq!(overflow, 0.0);
    // The statement names the contract with its title; the one with variants shows the lowest amount as ca.
    assert!(has(&shown, "Auftrag abgeschlossen: Beispielauftrag"), "{shown:?}");
    assert!(has(&shown, "Auftrag abgeschlossen: Beispiel-Geleit"), "{shown:?}");
    // A contract that cost a stake and pays nothing is a line with a minus, spending and no income.
    assert!(has(&shown, "Auftrag angenommen, Einsatz: Beispiel-Lieferung"), "{shown:?}");
    assert!(!has(&shown, "Auftrag abgeschlossen: Beispiel-Lieferung"), "{shown:?}");
    assert!(has(&shown, &de(-9_000.0)), "{shown:?}");
    assert!(has(&shown, "Das zählt als Ausgabe, nie als Einnahme"), "{shown:?}");
    assert!(has(&shown, &format!("+{}", de(25_000.0))), "{shown:?}");
    assert!(has(&shown, &format!("ca. +{}", de(18_000.0))), "{shown:?}");
    // The sources row and the line under it: three contracts, two with an amount, one of them only "at least".
    assert!(has(&shown, &format!("ca. {}", de(43_000.0))), "{shown:?}");
    assert!(has(&shown, "Mit Betrag aus dem Katalog: 2, zusammen"), "{shown:?}");
    assert!(has(&shown, "Gebucht ist der niedrigste Betrag"), "{shown:?}");
    // The same page in English (the names of the sample were made in German).
    set_thread_language(Choice::En);
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "Contract completed: Beispielauftrag"), "{shown:?}");
    assert!(has(&shown, "the lowest amount is booked"), "{shown:?}");
}

#[test]
fn the_sums_count_the_sale_from_the_ship_in_full_and_name_it_and_transfers_are_no_income() {
    set_thread_language(Choice::De);
    let (_dir, _, mut state) = real("summen");
    state.period = Period::Week;
    let (shown, overflow) = draw(&mut state, 1000.0);
    // Income: 1 000 000 (boxes prove it) + 500 000 (sold from the ship, the request has the exact amount) + 20 000
    // (shop).
    assert_eq!(after(&shown, "Einnahmen"), de(1_520_000.0), "{shown:?}");
    assert_eq!(after(&shown, "Ausgaben"), de(5_000.0));
    // The result is a figure, no estimate.
    assert_eq!(after(&shown, "Ergebnis"), format!("+{}", de(1_515_000.0)));
    assert!(has(&shown, &format!("Davon {}", de(500_000.0))), "{shown:?}");
    assert!(!has(&shown, "Dazu kommen") && !has(&shown, "nicht mitgezählt"), "{shown:?}");
    // The sources: the sale from the ship is in the trade row and named there, the transfers have a row of their own.
    assert!(has(&shown, &de(1_500_000.0)) && has(&shown, "2 (1 angefragt)"), "{shown:?}");
    assert!(has(&shown, &de(40_000.0)) && has(&shown, &de(15_000.0)), "{shown:?}");
    // No contract in the files: said once, with no amount.
    assert!(has(&shown, "Keine Aufträge abgeschlossen."), "{shown:?}");
    assert_eq!(overflow, 0.0);
}

#[test]
fn the_period_before_is_named_and_the_difference_is_signed() {
    set_thread_language(Choice::De);
    let (_dir, _, mut state) = real("vorher");
    state.period = Period::Today;
    let (shown, _) = draw(&mut state, 1000.0);
    // The journal's session began 24 hours ago: whatever of it falls on yesterday is the comparison.
    assert!(has(&shown, "Gestern: "), "{shown:?}");
    state.period = Period::Month;
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "Vormonat: "), "{shown:?}");
    state.period = Period::Session;
    let (shown, _) = draw(&mut state, 1000.0);
    // No session of the game is recorded in this folder: the page says so instead of inventing a start.
    assert!(has(&shown, "Noch keine Sitzung aufgezeichnet."), "{shown:?}");
}

#[test]
fn a_free_period_needs_two_readable_days_and_says_what_is_wrong() {
    set_thread_language(Choice::De);
    let (_dir, _, mut state) = real("frei");
    state.period = Period::Custom;
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "Wähl den ersten und den letzten Tag"), "{shown:?}");
    state.from = "31.02.2026".into();
    state.to = "01.03.2026".into();
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "nicht lesbar"), "{shown:?}");
    state.from = "05.03.2026".into();
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "„Von“ liegt nach „Bis“."), "{shown:?}");
    // Both days in the past of the journal: nothing in the period, said plainly.
    state.from = "01.03.2026".into();
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "Im Zeitraum steht nichts im Log"), "{shown:?}");
    // The days of today: the sums are there.
    let today = formats::local_time(now_seconds()).map(|t| formats::strftime("%d.%m.%Y", &t)).unwrap();
    let yesterday = formats::local_time(now_seconds() - 86_400).map(|t| formats::strftime("%d.%m.%Y", &t)).unwrap();
    state.from = yesterday;
    state.to = today;
    let (shown, _) = draw(&mut state, 1000.0);
    assert_eq!(after(&shown, "Einnahmen"), de(1_520_000.0), "{shown:?}");
}

#[test]
fn the_amounts_are_covered_in_streamer_mode() {
    set_thread_language(Choice::De);
    let (_dir, _, mut state) = real("streamer");
    versekit_core::streamer::set_thread_active(Some(true));
    let (covered, _) = draw(&mut state, 1000.0);
    versekit_core::streamer::set_thread_active(None);
    assert!(!has(&covered, "1.520.000") && !has(&covered, "1,520,000"), "{covered:?}");
    assert!(has(&covered, "Nach Quelle"));
}

#[test]
fn the_account_runs_from_the_balance_entered_and_a_new_balance_books_the_difference_as_a_check() {
    set_thread_language(Choice::De);
    let (_dir, paths) = world("konto");
    let now = now_seconds();
    journal(&paths, now);
    // The balance from before every event of the journal; since then the log shows
    // +1 500 000 + 20 000 - 5 000 + 40 000 - 15 000 = +1 540 000 (the sale from the ship is in it).
    let first = finance::count(None, 2_000_000, now - 5000, &[]);
    assert!(finance::save_counts(&paths, &[first]));
    let mut state = FinanceState::of(&paths);
    draw(&mut state, 1000.0);
    state.wait();
    // Computed: 3 540 000. Before the player enters anything the account shows exactly that.
    assert_eq!(state.data.as_ref().unwrap().account.balance(), Some(3_540_000.0));
    let (shown, _) = draw(&mut state, 1000.0);
    assert_eq!(after(&shown, "Konto"), de(3_540_000.0), "{shown:?}");
    // The game says 3 800 000: 260 000 the log does not show.
    assert!(state.start(Job::Count(3_800_000), &egui::Context::default(), 0.0));
    state.wait();
    assert_eq!(state.take_notice().as_deref(), Some("Stand eingetragen."));
    let counts = finance::counts(&paths);
    assert_eq!(counts.len(), 2);
    assert_eq!((counts[1].balance, counts[1].logged), (3_800_000, Some(1_540_000.0)));
    assert_eq!((counts[1].unseen, counts[1].requested), (Some(260_000.0), 500_000.0));
    let (shown, _) = draw(&mut state, 1000.0);
    assert_eq!(after(&shown, "Konto"), de(3_800_000.0), "{shown:?}");
    assert!(has(&shown, "Abgleich mit dem eingetragenen Stand"), "{shown:?}");
    assert!(has(&shown, &format!("+{}", de(260_000.0))), "{shown:?}");
}

#[test]
fn a_balance_below_the_computed_one_books_a_negative_check() {
    set_thread_language(Choice::De);
    let (_dir, paths) = world("konto-minus");
    let now = now_seconds();
    write(
        &paths,
        "lagerjournal.json",
        &format!(r#"{{"format": {}, "sitzungen": {{}}, "gelesen": {{}}}}"#, versekit_core::stock_journal::FORMAT),
    );
    assert!(finance::save_counts(&paths, &[finance::count(None, 500_000, now - 60, &[])]));
    let mut state = FinanceState::of(&paths);
    draw(&mut state, 1000.0);
    state.wait();
    assert!(state.start(Job::Count(450_000), &egui::Context::default(), 0.0));
    state.wait();
    let account = &state.data.as_ref().unwrap().account;
    let last = account.entries.last().unwrap();
    assert_eq!((last.kind, last.amount, last.balance), (Booking::Check, -50_000.0, 450_000.0));
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, &de(-50_000.0)), "{shown:?}");
}

#[test]
fn without_a_balance_entered_the_page_asks_for_one_and_the_first_one_only_sets_the_start() {
    set_thread_language(Choice::De);
    let (_dir, paths, mut state) = real("konto-erst");
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "Kontostand aus dem Spiel eintragen"), "{shown:?}");
    assert!(!has(&shown, "Stand danach"), "no statement without a start: {shown:?}");
    assert!(state.start(Job::Count(1_000_000), &egui::Context::default(), 0.0));
    state.wait();
    let counts = finance::counts(&paths);
    assert_eq!((counts.len(), counts[0].logged, counts[0].unseen), (1, None, None));
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "Zuletzt aus dem Spiel eingetragen am"), "{shown:?}");
    assert!(has(&shown, "Startstand"), "{shown:?}");
    assert!(!has(&shown, "Abgleich mit"), "no difference is claimed on the first balance: {shown:?}");
}

#[test]
fn the_statement_has_the_newest_line_first_with_time_name_sign_and_the_balance_after() {
    set_thread_language(Choice::De);
    let (_dir, paths) = world("auszug");
    let now = now_seconds();
    journal(&paths, now);
    assert!(finance::save_counts(&paths, &[finance::count(None, 2_000_000, now - 5000, &[])]));
    let mut state = FinanceState::of(&paths);
    draw(&mut state, 1000.0);
    state.wait();
    let (shown, _) = draw(&mut state, 1000.0);
    let line = |part: &str| shown.iter().position(|t| t.contains(part)).unwrap_or_else(|| panic!("{part}: {shown:?}"));
    // The journal's moments, newest first: transfer sent, transfer received, shop purchase, shop sale, the request.
    let order = ["Überweisung gesendet", "Überweisung erhalten", "Kauf im Laden", "Verkauf im Laden", "Ware verkauft"];
    let places: Vec<usize> = order.iter().map(|part| line(part)).collect();
    assert!(places.windows(2).all(|pair| pair[0] < pair[1]), "newest first: {places:?} in {shown:?}");
    // The oldest line is the start; each line names the day and the time.
    assert!(line("Startstand") > places[4]);
    let stamp_now = stamp(Lang::De, now - 1900);
    assert!(has(&shown, &stamp_now), "{stamp_now} in {shown:?}");
    // The sale from the ship is marked as a request, the one the boxes prove is not.
    let marked: Vec<&String> = shown.iter().filter(|t| t.contains("nur als Verkaufsanfrage im Log")).collect();
    assert_eq!(marked.len(), 1, "{shown:?}");
    // Amount with its sign and the balance after, e.g. -5 000 and 3 515 000 after the purchase.
    assert!(has(&shown, &de(-5_000.0)) && has(&shown, &de(3_515_000.0)), "{shown:?}");
    assert!(has(&shown, &format!("+{}", de(40_000.0))), "{shown:?}");
}

#[test]
fn a_long_statement_shows_a_page_and_more_on_a_click() {
    set_thread_language(Choice::De);
    let mut data = Data::sample();
    let entry = data.account.entries[1].clone();
    data.account.entries.extend((0..40).map(|n| Entry { time: entry.time + n, ..entry.clone() }));
    let mut state = FinanceState::sample();
    state.data = Some(Arc::new(data));
    let lines = |shown: &[String]| shown.iter().filter(|t| t.contains(" verkauft (") || t.contains("Verkauf im")).count();
    let (before, _) = draw(&mut state, 1000.0);
    assert!(has(&before, "weitere zeigen"), "{before:?}");
    assert!(lines(&before) <= STATEMENT_PAGE, "{}", lines(&before));
    let mut harness = harness_for(state);
    harness.get_by_label_contains("weitere zeigen").click();
    harness.run_steps(3);
    assert_eq!(harness.state().shown, 2 * STATEMENT_PAGE);
}

#[test]
fn taking_the_last_balance_back_restores_the_account_before() {
    set_thread_language(Choice::De);
    let (_dir, paths) = world("zurueck");
    let now = now_seconds();
    journal(&paths, now);
    assert!(finance::save_counts(&paths, &[finance::count(None, 2_000_000, now - 5000, &[])]));
    let mut state = FinanceState::of(&paths);
    draw(&mut state, 1000.0);
    state.wait();
    assert!(state.start(Job::Count(9_999_999), &egui::Context::default(), 0.0));
    state.wait();
    assert_eq!(state.data.as_ref().unwrap().account.balance(), Some(9_999_999.0));
    assert!(state.start(Job::Undo, &egui::Context::default(), 0.0));
    state.wait();
    assert_eq!(state.take_notice().as_deref(), Some("Letzter Stand zurückgenommen."));
    assert_eq!(finance::counts(&paths).len(), 1);
    assert_eq!(state.data.as_ref().unwrap().account.balance(), Some(3_540_000.0));
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(!has(&shown, "Abgleich mit"), "{shown:?}");
    // The last balance goes too: no account, the page asks again.
    assert!(state.start(Job::Undo, &egui::Context::default(), 0.0));
    state.wait();
    assert!(finance::counts(&paths).is_empty());
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "Kontostand aus dem Spiel eintragen"), "{shown:?}");
}

#[test]
fn the_button_to_take_back_stands_only_where_a_balance_exists() {
    set_thread_language(Choice::De);
    let mut harness = harness_for(FinanceState::sample());
    harness.get_by_label("Letzten Stand zurücknehmen").click();
    harness.run_steps(3);
    assert_eq!(harness.state_mut().take_notice().as_deref(), Some("Letzter Stand zurückgenommen."));
    let mut empty = FinanceState::sample();
    let mut data = Data::sample();
    data.counts.clear();
    data.account = Account::default();
    empty.data = Some(Arc::new(data));
    let harness = harness_for(empty);
    assert!(harness.query_by_label("Letzten Stand zurücknehmen").is_none());
}

#[test]
fn the_account_shows_in_both_languages_at_the_narrowest_width_without_cutting() {
    for language in [Choice::De, Choice::En] {
        set_thread_language(language);
        for width in [300.0, 360.0, 520.0] {
            let mut state = FinanceState::sample();
            let (shown, overflow) = draw(&mut state, width);
            assert_eq!(overflow, 0.0, "{language:?} at {width}");
            assert!(has(&shown, if matches!(language, Choice::De) { "Kontoauszug" } else { "Statement" }), "{shown:?}");
        }
    }
}

#[test]
fn a_balance_is_read_as_whole_numbers_with_or_without_separators() {
    assert_eq!(parse_balance("1250000"), Some(1_250_000));
    assert_eq!(parse_balance(" 1.250.000 "), Some(1_250_000));
    assert_eq!(parse_balance("1,250,000"), Some(1_250_000));
    assert_eq!(parse_balance("0"), Some(0));
    for wrong in ["", " ", "abc", "-5", "12.5x", "1e6", "99999999999999999999999"] {
        assert_eq!(parse_balance(wrong), None, "{wrong}");
    }
}

#[test]
fn a_switched_off_journal_says_so_and_shows_no_amounts() {
    set_thread_language(Choice::De);
    let (_dir, paths) = world("aus");
    journal(&paths, now_seconds());
    write(&paths, "einstellungen.json", r#"{"lager_aus_log": false}"#);
    let mut state = FinanceState::of(&paths);
    draw(&mut state, 1000.0);
    state.wait();
    let (shown, _) = draw(&mut state, 1000.0);
    assert!(has(&shown, "Es ist ausgeschaltet"), "{shown:?}");
    assert!(!has(&shown, &de(1_520_000.0)), "{shown:?}");
}

#[test]
fn the_chosen_period_is_kept() {
    set_thread_language(Choice::De);
    let (_dir, paths) = world("zeitraum");
    write(&paths, "einstellungen.json", r#"{"finanzen_zeitraum": "monat"}"#);
    assert_eq!(FinanceState::of(&paths).period, Period::Month);
    write(&paths, "einstellungen.json", r#"{"finanzen_zeitraum": "quatsch"}"#);
    assert_eq!(FinanceState::of(&paths).period, Period::Week);
}

#[test]
fn the_page_fits_at_every_width_in_both_languages() {
    // The layout check of all pages (`tests/layout_guard.rs`) does the same; this one runs in the full run only.
    if crate::skip_unless_full_run() {
        return;
    }
    for language in [Choice::De, Choice::En] {
        set_thread_language(language);
        for width in crate::test_pick(&[360.0, 520.0, 760.0, 1280.0]) {
            let mut state = FinanceState::sample();
            for period in Period::ALL {
                state.period = period;
                state.from = "2026-11-01".into();
                state.to = "2026-11-20".into();
                let (_, overflow) = draw(&mut state, width);
                assert_eq!(overflow, 0.0, "{period:?} at {width}");
            }
        }
    }
}

fn harness_for(state: FinanceState) -> Harness<'static, FinanceState> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 1600.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(|ui, state: &mut FinanceState| show_page(ui, theme::current(), state), state);
    harness.run_steps(3);
    harness
}

#[test]
fn a_click_on_a_period_chooses_it_and_the_free_one_brings_its_two_fields() {
    set_thread_language(Choice::De);
    let mut harness = harness_for(FinanceState::sample());
    assert_eq!(harness.get_all_by_role(Role::TextInput).count(), 1, "the balance field alone");
    harness.get_by_label("Heute").click();
    harness.run_steps(3);
    assert_eq!(harness.state().period, Period::Today);
    harness.get_by_label("Frei").click();
    harness.run_steps(3);
    assert_eq!(harness.state().period, Period::Custom);
    assert_eq!(harness.get_all_by_role(Role::TextInput).count(), 3, "from, to and the balance");
}

#[test]
fn the_button_of_the_count_refuses_a_text_and_takes_a_number() {
    set_thread_language(Choice::De);
    let mut harness = harness_for(FinanceState::sample());
    harness.get_by_label("Stand eintragen").click();
    harness.run_steps(3);
    assert!(harness.state().balance_error);
    assert!(harness.query_by_label_contains("Das ist keine Zahl").is_some());
    harness.get_all_by_role(Role::TextInput).next().unwrap().focus();
    harness.run_steps(2);
    harness.get_all_by_role(Role::TextInput).next().unwrap().type_text("1.250.000");
    harness.run_steps(2);
    assert_eq!(harness.state().balance, "1.250.000");
    harness.get_by_label("Stand eintragen").click();
    harness.run_steps(3);
    assert!(!harness.state().balance_error);
    assert_eq!(harness.state_mut().take_notice().as_deref(), Some("Stand eingetragen."));
}

#[test]
fn the_balance_of_the_statistics_leads_to_this_page() {
    use crate::statistics_page::{Page, StatsState, show};
    set_thread_language(Choice::De);
    let (_dir, paths) = world("statistik-weg");
    journal(&paths, now_seconds());
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1000.0, 4000.0))
        .with_context_setup(crate::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, state: &mut StatsState| {
                show(ui, theme::current(), state, Page::Overview);
            },
            StatsState::of(&paths),
        );
    harness.run_steps(2);
    harness.state_mut().wait();
    harness.run_steps(3);
    assert!(!harness.state_mut().take_finance_jump(), "nothing asked yet");
    harness.get_by_label("Zeiträume und Quellen ansehen").click();
    harness.run_steps(2);
    assert!(harness.state_mut().take_finance_jump());
}

#[test]
fn the_english_page_has_its_own_words() {
    set_thread_language(Choice::En);
    let mut state = FinanceState::sample();
    let (shown, _) = draw(&mut state, 1000.0);
    for part in [
        "By source",
        "Account",
        "Statement",
        "Balance after",
        "Evaluation",
        "Income",
        "Spending",
        "Transfers between players",
        "7 days",
        "Transfer received",
        "only as a sell request in the log",
    ] {
        assert!(has(&shown, part), "{part}: {shown:?}");
    }
    assert!(!has(&shown, "Nach Quelle") && !has(&shown, "Kontoauszug"), "{shown:?}");
}
