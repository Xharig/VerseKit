// Verse-Kit — companion for Star Citizen.
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

//! The page "Kaufen & Verkaufen" says when its prices come from an earlier build of the game than the running one,
//! once the pass in the background (`stock_fetch::keep_with`) renewed the build. Without a window (egui_kittest).

use egui::vec2;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use versekit_core::language::{Choice, set_thread_language};
use versekit_core::paths::{Host, Paths};
use versekit_core::pypath::Flavor;
use versekit_core::scmdb::FetchError;
use versekit_core::stock_fetch::{self, GAME_VERSIONS_URL};
use versekit_core::theme;
use versekit_ui::selling_page::{SellingState, show_page};

/// UEX naming `live` as the running build; every other list is out of reach.
fn uex(live: &'static str) -> impl Fn(&str) -> Result<Vec<u8>, FetchError> + Sync {
    move |url: &str| {
        if url == GAME_VERSIONS_URL {
            Ok(format!(r#"{{"status": "ok", "data": {{"live": "{live}", "ptu": null}}}}"#).into_bytes())
        } else {
            Err(FetchError::Offline)
        }
    }
}

/// Whether the page, opened on the data folder of `paths`, shows a label containing `text`.
fn shows(paths: &Paths, text: &str) -> bool {
    let mut harness = Harness::builder()
        .with_size(vec2(1280.0, 820.0))
        .with_context_setup(versekit_ui::install)
        .with_accessibility_check(false)
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, state: &mut SellingState| show_page(ui, theme::current(), state),
            SellingState::with_files(paths),
        );
    for _ in 0..3 {
        harness.step();
    }
    harness.query_by_label_contains(text).is_some()
}

#[test]
fn prices_of_an_earlier_build_are_marked_once_the_background_renewed_the_build() {
    set_thread_language(Choice::De);
    theme::set_thread_current("krt");
    let dir = std::env::temp_dir().join(format!("versekit-verkauf-patch-{}", versekit_core::testdir::run_tag()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("folder");
    let home = dir.to_string_lossy().into_owned();
    let paths = Paths::new(Host::new(Flavor::native(), &[("SC_BP_HOME", home.as_str())], &home));
    let now = std::time::SystemTime::now();
    stock_fetch::keep_with(&paths, now, &uex("4.10.2"));
    assert_eq!(stock_fetch::live_build(&paths), "4.10.2");
    // The goods list and the prices for selling, fetched under that build.
    let fetched = now.duration_since(std::time::UNIX_EPOCH).expect("time").as_secs();
    std::fs::write(
        paths.app_file(versekit_core::stock_data::CATALOG_FILE),
        format!(r#"{{"teile":[],"waren":[],"format":1,"geholt":{fetched}.0,"spielstand":"4.10.2"}}"#),
    )
    .expect("goods");
    std::fs::write(
        paths.app_file(versekit_core::selling::CACHE),
        format!(r#"{{"terminals": {{}}, "waren": {{}}, "format": 5, "geholt": {fetched}.0, "spielstand": "4.10.2"}}"#),
    )
    .expect("prices");
    assert!(shows(&paths, "Stand:"), "the same build: only the age of the prices");
    assert!(!shows(&paths, "im Spiel läuft"));
    // Half a day later the pass in the background learns of the patch.
    stock_fetch::keep_with(&paths, now + std::time::Duration::from_secs(13 * 3600), &uex("4.11.0"));
    assert_eq!(stock_fetch::live_build(&paths), "4.11.0");
    assert!(shows(&paths, "Zahlen aus 4.10.2, im Spiel läuft 4.11.0"), "the page names both builds");
    set_thread_language(Choice::Auto);
    let _ = std::fs::remove_dir_all(dir);
}
