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

//! The start page the player chooses, and the hint on the blueprint list for a blueprint that is missing: what
//! the window does with them on the user's own data, without a screen.

use std::sync::atomic::{AtomicUsize, Ordering};

use egui::Pos2;
use versekit_core::catalog_cache::CatalogCache;
use versekit_core::language::t;
use versekit_core::paths::Paths;

use super::controls::tests::Bench;
use super::real_data::real_page;
use crate::detection_page::DetectionState;

fn shows(texts: &[(String, Pos2)], part: &str) -> bool {
    texts.iter().any(|(text, _)| text.contains(part))
}

/// A click on the tab called `label`: it may bear the name of the page in the menu and of the title of the page.
fn click_tab(bench: &mut Bench, label: &str) {
    let texts = bench.settle();
    let at = texts
        .iter()
        .filter(|(text, _)| text == label)
        .map(|(_, at)| *at)
        // Not the entry of the side bar (left of the page), and of the rest the lowest: the title of the page
        // stands above its tabs.
        .filter(|at| at.x > 200.0)
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .unwrap_or_else(|| panic!("{label} is not shown: {texts:?}"));
    bench.click_at(at);
}

/// How often the catalog was built by the stand-in below.
static BUILT: AtomicUsize = AtomicUsize::new(0);

/// A catalog build that asks no network and counts its calls.
fn build_without_network(_: &Paths) -> Option<CatalogCache> {
    BUILT.fetch_add(1, Ordering::SeqCst);
    Some(CatalogCache::default())
}

#[test]
fn a_chosen_page_that_is_hidden_is_not_started_on() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("start-versteckt", "{\"startseite\": \"hf_auftragslog\"}", &[]);
    page.open_overview();
    page.module_hidden.push("hf_auftragslog");
    page.apply_start_page();
    assert_eq!(page.page_key(), "", "the overview instead");
    assert!(page.start_choices().iter().all(|(key, _)| key != "hf_auftragslog"), "and not offered");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_card_pinned_before_the_move_keeps_its_place_and_carries_the_new_name() {
    let _language = crate::language_lock();
    let settings = "{\"karten_angeheftet\": [\"erkennung\"], \"startseite_zuerst\": \"karten\"}";
    let (page, dir) = real_page("karte-erkennung", settings, &[]);
    assert!(
        page.tiles.iter().any(|tile| tile.pinned && tile.key == "hf_allgemein"),
        "the stored name of the page leads to the card of the general settings"
    );
    assert!(page.tiles.iter().all(|tile| tile.key != "hf_erkennung"), "and no card of the old page is left");
    let (_, name, _) = super::tile_of("hf_allgemein");
    assert_eq!(name, t("hf_allgemein"), "no mark of the advanced pages on the card any more");
    let (_, advanced, _) = super::tile_of("hf_startprogramme");
    assert!(advanced.contains(&t("k_fortgeschritten")), "the mark stays on the pages that still have it");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_choice_of_the_start_page_offers_the_overview_and_every_page_with_its_group() {
    let _language = crate::language_lock();
    let (page, dir) = real_page("start-auswahl", "", &[]);
    let choices = page.start_choices();
    assert_eq!(choices[0], (String::new(), t("hf_uebersicht")), "the overview first");
    assert!(choices.iter().all(|(key, _)| key != "hf_erkennung"), "the detection page is not offered any more");
    let general = choices.iter().find(|(key, _)| key == "hf_allgemein").expect("the general settings are offered");
    assert_eq!(general.1, format!("{} › {}", t("hf_gruppe_einst"), t("hf_allgemein")));
    let pages: usize = super::NAV.iter().map(|(_, pages)| pages.len()).sum();
    assert!(choices.len() > pages / 2, "{} choices", choices.len());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_choice_of_the_start_page_is_a_row_of_the_general_page_and_is_written() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("start-zeile", "", &[]);
    page.go_to("hf_allgemein");
    let mut bench = Bench::new(page);
    let texts = bench.settle();
    assert!(shows(&texts, &t("s_fg_startseite")), "{texts:?}");

    // The field shows the overview; a click opens the list, a click on a page in it chooses that page.
    let menu = texts.iter().find(|(text, _)| *text == t("hf_gruppe_bp").to_uppercase()).expect("the menu").1.x + 100.0;
    let field = texts.iter().find(|(text, at)| *text == t("hf_uebersicht") && at.x > menu).expect("the field").1;
    bench.click_at(field);
    let wanted = format!("{} › {}", t("hf_gruppe_bp"), t("hf_auftragslog"));
    let list = bench.settle();
    let entry = list.iter().find(|(text, _)| *text == wanted).unwrap_or_else(|| panic!("{wanted}: {list:?}")).1;
    bench.click_at(entry);
    bench.settle();
    assert_eq!(bench.page.start_page, "hf_auftragslog");
    let file = std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
    assert!(file.contains("\"startseite\": \"hf_auftragslog\""), "{file}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_blueprint_list_asks_whether_one_is_missing_and_leads_to_the_detection_in_the_settings() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("liste-fehlt", "", &[]);
    page.go_to("hf_liste");
    let mut bench = Bench::new(page);
    let texts = bench.settle();
    for key in ["s_fg_fehlt", "s_fg_fehlt_knopf", "s_er_kat"] {
        assert!(shows(&texts, &t(key)), "{key}: {texts:?}");
    }
    bench.click(&t("s_fg_fehlt_knopf"));
    assert_eq!(bench.page.page_key(), "hf_allgemein");
    assert!(shows(&bench.settle(), &t("s_er_acc")), "the general settings are drawn with the detection at the top");
    // The hint belongs to the list only.
    bench.page.go_to("hf_fortschritt");
    assert!(!shows(&bench.settle(), &t("s_fg_fehlt")));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_button_on_the_blueprint_list_builds_the_catalog_again_and_tells_how_it_went() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("liste-katalog", "", &[]);
    page.detection = DetectionState::with_build(build_without_network);
    page.go_to("hf_liste");
    let mut bench = Bench::new(page);
    bench.settle();
    let before = BUILT.load(Ordering::SeqCst);
    bench.click(&t("s_er_kat"));
    let done = versekit_core::language::tf("s_er_kat_da", &[("0", versekit_core::language::Arg::from(0_i64))]);
    let mut told = false;
    crate::wait_for(|| {
        told = shows(&bench.settle(), &done);
        told
    });
    assert!(BUILT.load(Ordering::SeqCst) > before, "the catalog was built");
    assert!(told, "and the foot says how many blueprints it holds");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_blueprint_list_and_its_progress_are_two_tabs_of_one_page() {
    let _language = crate::language_lock();
    let mut page = super::SamplePage::preview();
    page.go_to("hf_liste");
    let mut bench = Bench::new(page);
    let shown = bench.settle();
    // The page is the list; both views are named by their tabs, and the old buttons that led across are gone.
    assert!(shown.iter().any(|(text, _)| *text == t("hf_fortschritt")), "{shown:?}");
    assert!(!shown.iter().any(|(text, _)| *text == t("s_fo_lead")), "the list shows no progress: {shown:?}");
    click_tab(&mut bench, &t("hf_fortschritt"));
    let shown = bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.list_progress), ("hf_liste", true));
    assert!(shown.iter().any(|(text, _)| *text == t("s_fo_lead")), "{shown:?}");
    // The tab of the list leads back, on the same page.
    click_tab(&mut bench, &t("hf_liste"));
    bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.list_progress), ("hf_liste", false));
}

#[test]
fn the_crafting_page_and_what_to_farm_are_two_tabs_of_one_page() {
    let _language = crate::language_lock();
    let mut page = super::SamplePage::preview();
    page.go_to("hf_herstellung");
    let mut bench = Bench::new(page);
    bench.settle();
    click_tab(&mut bench, &t("hf_farmliste"));
    let shown = bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.craft_farm), ("hf_herstellung", true));
    assert!(!shown.iter().any(|(text, _)| *text == t("s_mz_titel")), "the card of the notes is gone: {shown:?}");
    click_tab(&mut bench, &t("hf_herstellung"));
    bench.settle();
    assert!(!bench.page.craft_farm);
}

#[test]
fn the_mining_page_and_the_refineries_are_two_tabs_of_one_page() {
    let _language = crate::language_lock();
    let mut page = super::SamplePage::preview();
    page.go_to("hf_bergbau");
    let mut bench = Bench::new(page);
    let shown = bench.settle();
    // The page is the mining; the refineries are named by their tab, their table stays away.
    assert!(shown.iter().any(|(text, _)| *text == t("hf_raffinerien")), "{shown:?}");
    assert!(!shown.iter().any(|(text, _)| *text == t("s_rf_lead")), "the mining shows no table: {shown:?}");
    click_tab(&mut bench, &t("hf_raffinerien"));
    let shown = bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.mining_refineries), ("hf_bergbau", true));
    assert!(shown.iter().any(|(text, _)| *text == t("s_rf_lead")), "{shown:?}");
    assert!(!shown.iter().any(|(text, _)| *text == t("s_bg_lead")), "the refineries show no mining: {shown:?}");
    click_tab(&mut bench, &t("hf_bergbau"));
    let shown = bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.mining_refineries), ("hf_bergbau", false));
    assert!(shown.iter().any(|(text, _)| *text == t("s_bg_lead")), "{shown:?}");
}

#[test]
fn a_jump_to_a_raw_material_shows_the_mining_even_when_the_refineries_were_left_open() {
    let _language = crate::language_lock();
    let mut page = super::SamplePage::preview();
    page.go_to("hf_raffinerien");
    let mut bench = Bench::new(page);
    bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.mining_refineries), ("hf_bergbau", true));
    // Another page leads to a material in mining: the mining comes up with it, not the table of the refineries.
    let (to, group) = bench.page.jump_to_mining("Titanium");
    assert_eq!((to.as_str(), group), ("hf_bergbau", "hf_gruppe_abbau"));
    bench.page.go_to(&to);
    let shown = bench.settle();
    assert!(!bench.page.mining_refineries);
    assert!(shown.iter().any(|(text, _)| *text == t("s_bg_lead")), "{shown:?}");
}

#[test]
fn the_old_pages_of_the_progress_and_of_the_farm_list_open_their_view_and_the_choice_is_kept() {
    let _language = crate::language_lock();
    let mut page = super::SamplePage::preview();
    page.go_to("hf_fortschritt");
    let mut bench = Bench::new(page);
    bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.list_progress), ("hf_liste", true));
    bench.page.go_to("hf_farmliste");
    bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.craft_farm), ("hf_herstellung", true));
    bench.page.go_to("hf_raffinerien");
    bench.settle();
    assert_eq!((bench.page.page_key(), bench.page.mining_refineries), ("hf_bergbau", true));
    // Back at the page by the menu: it shows the view it was left in.
    bench.page.go_to("hf_liste");
    bench.settle();
    assert!(bench.page.list_progress);
}

/// A window on a page with what a screen reader sees of it, to click a switch by its name.
struct Seen {
    ctx: egui::Context,
    page: super::SamplePage,
}

type Nodes = Vec<(egui::Role, String, egui::Rect)>;

impl Seen {
    fn new(page: super::SamplePage) -> Seen {
        let ctx = egui::Context::default();
        crate::install(&ctx);
        ctx.enable_accesskit();
        Seen { ctx, page }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> (Vec<(String, Pos2)>, Nodes) {
        fn collect(shape: &egui::Shape, into: &mut Vec<(String, Pos2)>) {
            match shape {
                egui::Shape::Text(text) => into.push((text.galley.text().to_string(), text.pos)),
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, into)),
                _ => {}
            }
        }
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1400.0, 1000.0))),
            events,
            ..Default::default()
        };
        let page = &mut self.page;
        let mut output = self.ctx.run_ui(input, |ui| {
            page.show(ui);
        });
        output.textures_delta.clear();
        let mut texts = Vec::new();
        output.shapes.iter().for_each(|clipped| collect(&clipped.shape, &mut texts));
        let nodes = output
            .platform_output
            .accesskit_update
            .map(|update| {
                update
                    .nodes
                    .iter()
                    .filter_map(|(_, node)| {
                        let b = node.bounds()?;
                        let rect = egui::Rect::from_min_max(
                            Pos2::new(b.x0 as f32, b.y0 as f32),
                            Pos2::new(b.x1 as f32, b.y1 as f32),
                        );
                        Some((node.role(), node.label().unwrap_or_default().to_string(), rect))
                    })
                    .collect()
            })
            .unwrap_or_default();
        (texts, nodes)
    }

    fn settle(&mut self) -> Vec<(String, Pos2)> {
        let mut last = Vec::new();
        for _ in 0..4 {
            last = self.frame(Vec::new()).0;
        }
        last
    }

    fn click_named(&mut self, role: egui::Role, name: &str) {
        self.settle();
        let nodes = self.frame(Vec::new()).1;
        let at = nodes
            .iter()
            .find(|(r, n, _)| *r == role && n == name)
            .map(|(_, _, rect)| rect.center())
            .unwrap_or_else(|| panic!("no {role:?} `{name}`: {nodes:?}"));
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![egui::Event::PointerMoved(at)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.settle();
    }
}

#[test]
fn the_page_of_the_detection_is_gone_and_its_old_name_leads_to_the_general_settings_with_the_phrase_card() {
    let _language = crate::language_lock();
    // Not in the menu, not offered as a page to pin or to start with.
    assert!(
        !super::NAV.iter().any(|(_, pages)| pages.iter().any(|(page, _)| *page == "hf_erkennung")),
        "no entry in the menu"
    );
    assert!(!super::SamplePage::page_keys().contains(&"hf_erkennung"), "no page to draw");
    assert_eq!(versekit_core::overview::page_key("erkennung"), None, "no page name of its own");
    assert_eq!(versekit_core::overview::resolve("erkennung").0, "allgemein", "the old name is the general settings");

    // A page key still stored (a last page, a link) opens the general settings, where the phrase card stands.
    let (mut page, dir) = real_page("erkennung-schlank", "", &[]);
    page.go_to("hf_erkennung");
    let mut seen = Seen::new(page);
    let texts = seen.settle();
    assert_eq!(seen.page.page_key(), "hf_allgemein");
    assert!(shows(&texts, &t("s_er_satz")) && shows(&texts, &t("s_er_satz_h")), "the card stands there: {texts:?}");
    for key in ["s_er_alle", "s_lj_schalter", "s_er_kat"] {
        assert!(!shows(&texts, &t(key)), "{key} stands on its own page: {texts:?}");
    }
    let _ = std::fs::remove_dir_all(dir);

    // The old page as the start page opens the general settings too.
    let (page, dir) = real_page("erkennung-start", "{\"startseite\": \"hf_erkennung\"}", &[]);
    assert_eq!(page.page_key(), "hf_allgemein");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_switch_for_blueprints_without_a_known_way_stands_on_the_list_and_keeps_its_stored_value() {
    let _language = crate::language_lock();
    // Stored on before the move: the key is the same, the switch there says so and a click turns it off.
    let (mut page, dir) = real_page("liste-weg", "{\"alle_bauplaene\": true}", &[]);
    page.go_to("hf_liste");
    let mut seen = Seen::new(page);
    let texts = seen.settle();
    assert!(shows(&texts, &t("s_er_alle")), "{texts:?}");
    let file = || std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
    seen.click_named(egui::Role::CheckBox, &t("s_er_alle"));
    assert!(file().contains("\"alle_bauplaene\": false"), "{}", file());
    assert!(shows(&seen.settle(), &t("s_er_alle_weg")), "the foot says it");
    seen.click_named(egui::Role::CheckBox, &t("s_er_alle"));
    assert!(file().contains("\"alle_bauplaene\": true"), "{}", file());
    assert!(shows(&seen.settle(), &t("s_er_alle_hin")));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_switch_for_the_stock_from_the_log_stands_on_the_stock_page_and_keeps_its_stored_value() {
    let _language = crate::language_lock();
    let (mut page, dir) = real_page("lager-log", "{\"lager_aus_log\": false}", &[]);
    page.go_to("hf_meinlager");
    let mut seen = Seen::new(page);
    let texts = seen.settle();
    assert!(shows(&texts, &t("s_lj_schalter")), "{texts:?}");
    let file = || std::fs::read_to_string(dir.join("einstellungen.json")).unwrap();
    seen.click_named(egui::Role::CheckBox, &t("s_lj_schalter"));
    assert!(file().contains("\"lager_aus_log\": true"), "stored off before the move, on now: {}", file());
    assert!(shows(&seen.settle(), &t("s_lj_schalter_an")), "the page says it");
    seen.click_named(egui::Role::CheckBox, &t("s_lj_schalter"));
    assert!(file().contains("\"lager_aus_log\": false"), "{}", file());
    assert!(shows(&seen.settle(), &t("s_lj_schalter_aus")));
    let _ = std::fs::remove_dir_all(dir);
}
