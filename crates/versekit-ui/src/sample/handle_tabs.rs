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

//! The page "Handle-Suche" in the whole window on real data in a throwaway folder: two tabs with a card
//! each, a click on a tab switches the content, and the page "Beziehungen" that is gone leads to the second tab.

use versekit_core::language::t;

use super::SamplePage;
use super::docking::tests::{draw, draw_clicking};
use super::trade_pages::{page_with, shows};

/// The tab of the handle search that stands, by name.
fn tab_of(page: &SamplePage) -> &'static str {
    match page.handles.tab {
        crate::handle_page::Tab::Search => "search",
        crate::handle_page::Tab::Relations => "relations",
    }
}

#[test]
fn the_tabs_hold_their_cards_and_the_old_relations_page_leads_to_the_second_tab() {
    let _language = crate::language_lock();
    versekit_core::theme::set_thread_current("original");
    let (mut page, dir) = page_with("handle-reiter", &[]);
    page.go_to("hf_handlesuche");
    let search = draw(&mut page, 1280.0, 820.0).1;
    for key in ["s_hqt_reiter_suche", "s_hqt_reiter_beziehungen", "s_hqt_karte_suche", "s_hq_verlauf"] {
        assert!(shows(&search, &t(key)), "{key}: {search:?}");
    }
    assert!(!shows(&search, &t("s_hqt_karte_austausch")), "the relations wait on their tab: {search:?}");
    // A click on the tab shows the cards of the relations instead.
    let relations = draw_clicking(&mut page, 1280.0, 820.0, &[&t("s_hqt_reiter_beziehungen")]).remove(0);
    assert_eq!(tab_of(&page), "relations");
    for key in ["s_hqt_karte_austausch", "s_hqs_liste"] {
        assert!(shows(&relations, &t(key)), "{key}: {relations:?}");
    }
    assert!(!shows(&relations, &t("s_hqt_karte_suche")), "{relations:?}");
    // The page that is gone (a stored last page, a pinned card, a jump) opens the second tab.
    page.go_to("hf_handlesuche");
    page.handles.tab = crate::handle_page::Tab::Search;
    draw(&mut page, 1280.0, 820.0);
    page.go_to("hf_beziehungen");
    let led = draw(&mut page, 1280.0, 820.0).1;
    assert_eq!(page.page_key(), "hf_handlesuche");
    assert_eq!(tab_of(&page), "relations");
    assert!(shows(&led, &t("s_hqt_karte_austausch")), "{led:?}");
    // Counter-check: the page itself, opened by its own key, stays on the tab chosen last.
    page.handles.tab = crate::handle_page::Tab::Search;
    page.go_to("hf_handlesuche");
    let plain = draw(&mut page, 1280.0, 820.0).1;
    assert!(shows(&plain, &t("s_hqt_karte_suche")) && !shows(&plain, &t("s_hqt_karte_austausch")), "{plain:?}");
    // The menu has one entry for both.
    let menu: Vec<&str> = super::NAV.iter().flat_map(|(_, pages)| pages.iter().map(|(key, _)| *key)).collect();
    assert!(menu.contains(&"hf_handlesuche") && !menu.contains(&"hf_beziehungen"), "{menu:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_old_names_of_the_relations_lead_to_the_handle_search() {
    use versekit_core::overview;
    assert_eq!(overview::resolve("beziehungen").0, "handlesuche");
    assert_eq!(overview::page_key(overview::resolve("beziehungen").0), Some("hf_handlesuche"));
    let ctx = egui::Context::default();
    let mut page = super::SamplePage::preview();
    assert!(page.open_page_named(&ctx, "beziehungen"));
    assert_eq!(page.page_key(), "hf_handlesuche");
    assert_eq!(page.handles.tab, crate::handle_page::Tab::Relations);
    // Counter-check: the name of the search itself brings no tab.
    page.handles.tab = crate::handle_page::Tab::Search;
    assert!(page.open_page_named(&ctx, "handlesuche"));
    assert_eq!(page.handles.tab, crate::handle_page::Tab::Search);
    // A pin kept under the old name counts as the pin of the search, once.
    let dir = versekit_core::testdir::Dir::new("handle-reiter-pin");
    let home = dir.path().to_string_lossy().into_owned();
    let paths = versekit_core::paths::Paths::new(versekit_core::paths::Host::new(
        versekit_core::pypath::Flavor::native(),
        &[(versekit_core::legacy::ENV_HOME, home.as_str())],
        &home,
    ));
    paths.set_setting(overview::PINNED_SETTING, overview::list_value(&["beziehungen".into(), "handlesuche".into()]));
    assert_eq!(overview::pinned(&paths), ["handlesuche"]);
}
