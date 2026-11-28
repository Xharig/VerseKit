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

//! Every token here is made up; no request leaves the test.

use super::*;
use versekit_core::language;
use versekit_core::paths::Host;
use versekit_core::pypath::Flavor;
use versekit_core::scmdb::FetchError;

const MADE_UP: &str = "vk-test-row-4d1c8e2a9b7f3065";

/// The token of the running program is one for the whole process: the checks that read or set it take turns.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn turn() -> std::sync::MutexGuard<'static, ()> {
    TURN.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Every text drawn in one frame of the row.
fn texts_of(row: &mut UexTokenRow, copy: bool) -> Vec<String> {
    fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
        match shape {
            egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
            egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
            _ => {}
        }
    }
    let scheme = versekit_core::theme::by_id("original").expect("original");
    let ctx = egui::Context::default();
    crate::install(&ctx);
    let mut texts = Vec::new();
    for _ in 0..2 {
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = row.control(ui, scheme, copy);
        });
        output.textures_delta.clear();
        texts.clear();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut texts);
        }
    }
    texts
}

#[test]
fn the_field_never_shows_the_token_also_not_in_streamer_mode() {
    let _turn = turn();
    let _language = crate::language_lock();
    language::set_thread_language(language::Choice::De);
    let mut row = UexTokenRow { typed: MADE_UP.to_string(), ..UexTokenRow::default() };
    // The mode for this thread alone: switched on for the whole program, every check drawing beside this one would
    // cover what it learned.
    for streamer in [false, true] {
        versekit_core::streamer::set_thread_active(Some(streamer));
        let texts = texts_of(&mut row, false);
        assert!(texts.iter().any(|text| *text == t("s_uex_speichern")), "the row is drawn: {texts:?}");
        assert!(!texts.iter().any(|text| text.contains(MADE_UP) || text.contains("vk-test")), "{texts:?}");
    }
    versekit_core::streamer::set_thread_active(None);
}

#[test]
fn on_the_sample_page_saving_and_removing_only_show_what_would_happen() {
    let _turn = turn();
    let _language = crate::language_lock();
    language::set_thread_language(language::Choice::De);
    let ctx = egui::Context::default();
    let mut row = UexTokenRow { typed: format!(" Bearer {MADE_UP} "), ..UexTokenRow::default() };
    row.apply(Wish::Save(Token::parse(&row.typed.clone()).unwrap()), None, &ctx);
    assert!(row.stored && row.typed.is_empty(), "the field is empty again once saved");
    assert_eq!(row.status(), Some((t("s_uex_gespeichert"), false)));
    row.apply(Wish::Test, None, &ctx);
    assert_eq!(row.status(), Some((t("s_uex_angenommen"), false)));
    row.apply(Wish::Remove, None, &ctx);
    assert!(!row.stored);
    assert_eq!(row.status(), Some((t("s_uex_entfernt"), false)));
    row.typed = "zwei teile".into();
    row.apply(Wish::Test, None, &ctx);
    assert_eq!(row.status(), Some((t("s_uex_ungueltig"), true)));
}

#[test]
fn saved_for_real_it_lands_in_the_own_store_and_a_refusal_says_copy_it_again() {
    let _turn = turn();
    let _language = crate::language_lock();
    language::set_thread_language(language::Choice::De);
    let dir = versekit_core::testdir::Dir::new("uex-token-zeile");
    let data = dir.path().join("daten");
    let secrets = dir.path().join("geheim");
    std::fs::create_dir_all(&data).unwrap();
    let (data_text, secrets_text) = (data.to_string_lossy().into_owned(), secrets.to_string_lossy().into_owned());
    let vars = [
        (versekit_core::envvars::ENV_HOME, data_text.as_str()),
        (versekit_core::envvars::ENV_SECRETS, secrets_text.as_str()),
        (versekit_core::envvars::ENV_SECRETS_FILE, "1"),
    ];
    let paths = Paths::new(Host::new(Flavor::native(), &vars, &data_text));
    let ctx = egui::Context::default();
    let mut row = UexTokenRow::default();
    row.apply(Wish::Save(Token::parse(MADE_UP).unwrap()), Some(&paths), &ctx);
    assert_eq!(row.status(), Some((t("s_uex_gespeichert"), false)));
    assert_eq!(uex_token::current(), Token::parse(MADE_UP), "in use at once");
    assert_eq!(std::fs::read_dir(&secrets).unwrap().count(), 1, "kept in the store of secrets");
    // UEX refuses it: the row says so in red, whatever was said before.
    let send = |_: &str, bearer: Option<&str>| match bearer {
        Some(_) => Err((401, FetchError::Failed(String::new()))),
        None => Ok(br#"{"status":"ok","data":[]}"#.to_vec()),
    };
    assert!(uex_token::fetch_through("https://api.uexcorp.uk/2.0/commodities", &send).is_ok());
    assert_eq!(row.status(), Some((t("s_uex_abgelehnt"), true)));
    let texts = texts_of(&mut row, false);
    assert!(texts.contains(&t("s_uex_abgelehnt")), "{texts:?}");
    row.apply(Wish::Remove, Some(&paths), &ctx);
    assert!(uex_token::current().is_none());
    assert_eq!(std::fs::read_dir(&secrets).unwrap().count(), 0, "deleted from the store");
    assert_eq!(row.status(), Some((t("s_uex_entfernt"), false)));
}

#[test]
fn on_a_copy_of_the_data_the_row_has_no_field() {
    let _language = crate::language_lock();
    language::set_thread_language(language::Choice::De);
    let mut row = UexTokenRow::default();
    let texts = texts_of(&mut row, true);
    assert!(texts.contains(&t("s_uex_kopie")), "{texts:?}");
    assert!(!texts.contains(&t("s_uex_speichern")) && !texts.contains(&t("s_uex_feld")), "{texts:?}");
}

/// The field has the name of the row for a screen reader, and the three buttons stand in the column of controls,
/// in both languages and whether a token is stored or not (egui_kittest, no window).
#[test]
fn the_field_is_named_and_the_buttons_fit_the_column_of_controls() {
    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;
    let _turn = turn();
    let _language = crate::language_lock();
    for (choice, stored) in [
        (language::Choice::De, false),
        (language::Choice::De, true),
        (language::Choice::En, false),
        (language::Choice::En, true),
    ] {
        language::set_thread_language(choice);
        let row = UexTokenRow { stored, ..UexTokenRow::default() };
        let mut harness = egui_kittest::Harness::builder()
            .with_size(egui::vec2(600.0, 300.0))
            .with_context_setup(crate::install)
            .with_accessibility_check(false)
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, row: &mut UexTokenRow| {
                    let scheme = versekit_core::theme::by_id("krt").expect("krt");
                    ui.set_max_width(CONTROL_WIDTH);
                    let _ = row.control(ui, scheme, false);
                },
                row,
            );
        harness.run_steps(3);
        let field = harness.get_by_role_and_label(Role::PasswordInput, &t("s_uex_token")).rect();
        let right = field.min.x + CONTROL_WIDTH + 0.5;
        assert!(field.max.x <= right, "{choice:?}: field {field:?}");
        for key in ["s_uex_speichern", "s_uex_entfernen", "s_uex_testen"] {
            let button = harness.get_by_role_and_label(Role::Button, &t(key)).rect();
            assert!(button.max.x <= right, "{choice:?} stored {stored}: {key} reaches {} > {right}", button.max.x);
            assert!(button.min.y >= field.max.y, "{choice:?}: {key} below the field");
        }
    }
}

#[test]
fn every_text_of_the_row_is_there_in_both_languages() {
    for key in [
        "s_uex_titel",
        "s_uex_token",
        "s_uex_token_h",
        "s_uex_feld",
        "s_uex_feld_gespeichert",
        "s_uex_speichern",
        "s_uex_testen",
        "s_uex_entfernen",
        "s_uex_erst_einfuegen",
        "s_uex_keiner",
        "s_uex_gespeichert",
        "s_uex_entfernt",
        "s_uex_angenommen",
        "s_uex_abgelehnt",
        "s_uex_offline",
        "s_uex_fehler",
        "s_uex_ungueltig",
        "s_uex_nicht_gespeichert",
        "s_uex_kopie",
        "s_uex_prueft",
    ] {
        for lang in [language::Lang::De, language::Lang::En] {
            assert!(language::text_in(lang, key).is_some_and(|text| !text.is_empty()), "{key} {lang:?}");
        }
    }
}
