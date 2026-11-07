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

//! The way of a ship from the page "Shops" to the section "Planned" of the hangar page, in the whole
//! window on real data in a throwaway folder: the button plans exactly this ship once, a second visit
//! says it is planned already, the jump opens the section, the old page of the wishlist leads there
//! too, and nothing is cut off.

use egui::Pos2;
use versekit_core::hangar;
use versekit_core::language::{self, Arg, t, tf};

use super::SamplePage;
use super::docking::tests::{draw, draw_clicking};
use super::trade_pages::{catalog_file, page_with, shops_file, shows};

const SHIP: &str = "MISC Freelancer MAX";
const OTHER: &str = "Drake Caterpillar";

/// The UEX ships and what two of them cost: both are for sale, so both stand in the list of the shops.
fn two_ships() -> String {
    let fetched = crate::trade::now() - 7_200.0;
    format!(
        r#"{{"schiffe":{{"1":{{"name":"{SHIP}","scu":122,"werft":"MISC"}},"2":{{"name":"{OTHER}","scu":576,"werft":"Drake"}}}},"kauf":{{"1":[{{"stelle":"Dealer","ort":"Lorville","system":"Stanton","preis":1500000.0}}],"2":[{{"stelle":"Dealer","ort":"Lorville","system":"Stanton","preis":2000000.0}}]}},"miete":{{}},"format":5,"geholt":{fetched}}}"#
    )
}

fn page(name: &str, hangar_file: &str) -> (SamplePage, std::path::PathBuf) {
    // The plain scheme for this test thread: other tests switch the scheme of the whole program meanwhile, and
    // the scheme with the capital letters writes the buttons looked for here in another way.
    versekit_core::theme::set_thread_current("original");
    let files = [
        ("laeden-katalog.json", catalog_file()),
        ("laeden.json", shops_file()),
        ("schiffe.json", two_ships()),
        (hangar::FILE, hangar_file.to_string()),
    ];
    page_with(name, &files)
}

fn look(page: &mut SamplePage, key: &str, width: f32) -> (f32, Vec<(String, Pos2)>) {
    page.go_to(key);
    draw(page, width, 820.0)
}

/// The wished ships of the file in the folder, as (name, maker).
fn planned_in(dir: &std::path::Path) -> Vec<(String, String)> {
    let data = hangar::read(&dir.join(hangar::FILE).to_string_lossy()).expect("the hangar file");
    hangar::wishlist(&data)
        .iter()
        .map(|w| (hangar::field_text(w, "name").to_string(), hangar::field_text(w, "hersteller").to_string()))
        .collect()
}

fn said_planned(name: &str) -> String {
    tf("s_hg_im_hangar_vorgemerkt", &[("0", Arg::from(name))])
}

#[test]
fn the_button_plans_exactly_this_ship_once_and_a_second_visit_says_it_is_planned() {
    let _language = crate::language_lock();
    let (mut page, dir) = page("schiff-vormerken", r#"{"format": 1, "schiffe": []}"#);
    page.shops.pick_by_name(SHIP);
    let texts = look(&mut page, "hf_laeden", 1280.0).1;
    assert!(shows(&texts, &t("s_ld_vormerken")), "the ship has the button: {texts:?}");
    assert!(!shows(&texts, &t("s_ld_schon_geplant")));
    assert!(!shows(&texts, &t("s_ld_in_korb")), "a ship has no basket row");
    let after = draw_clicking(&mut page, 1280.0, 820.0, &[&t("s_ld_vormerken")]).remove(0);
    // The line under it, the way to the section, no button any more.
    assert!(shows(&after, &said_planned(SHIP)), "{after:?}");
    assert!(shows(&after, &t("s_ld_zu_geplant")), "{after:?}");
    assert!(!shows(&after, &t("s_ld_vormerken")), "{after:?}");
    // Stored the way the section of the hangar page stores a ship: name and maker UEX has in the name.
    let uex = versekit_core::ships::UexShips::load(&dir.join("schiffe.json").to_string_lossy());
    assert_eq!(planned_in(&dir), vec![(SHIP.to_string(), uex.manufacturer(SHIP))]);
    // A second visit to the same ship: planned already, no second entry, no button.
    page.shops.close_detail();
    page.shops.pick_by_name(SHIP);
    let texts = look(&mut page, "hf_laeden", 1280.0).1;
    assert!(shows(&texts, &t("s_ld_schon_geplant")), "{texts:?}");
    assert!(shows(&texts, &t("s_ld_zu_geplant")), "{texts:?}");
    assert!(!shows(&texts, &t("s_ld_vormerken")), "{texts:?}");
    assert_eq!(planned_in(&dir).len(), 1, "once");
    // Another ship still has the button, and planning it leaves the first one alone.
    page.shops.close_detail();
    page.shops.pick_by_name(OTHER);
    let texts = look(&mut page, "hf_laeden", 1280.0).1;
    assert!(shows(&texts, &t("s_ld_vormerken")) && !shows(&texts, &t("s_ld_schon_geplant")), "{texts:?}");
    draw_clicking(&mut page, 1280.0, 820.0, &[&t("s_ld_vormerken")]);
    let names: Vec<String> = planned_in(&dir).into_iter().map(|(name, _)| name).collect();
    assert_eq!(names, [OTHER, SHIP].map(String::from), "both, A to Z");
    let _ = std::fs::remove_dir_all(dir);
}

/// Coming back to the hangar after the shopping list changed the file: the page shows what the file holds now.
#[test]
fn back_on_the_hangar_after_the_shopping_list_changed_it_the_page_is_current() {
    let _language = crate::language_lock();
    let own = |names: &[&str]| {
        let ships: Vec<String> =
            names.iter().map(|n| format!(r#"{{"name": "{n}", "herkunft": "pledge", "belegung": {{}}}}"#)).collect();
        format!(r#"{{"format": 1, "schiffe": [{}]}}"#, ships.join(", "))
    };
    let (mut page, dir) = page("hangar-wiederkommen", &own(&[SHIP]));
    let texts = look(&mut page, "hf_hangar", 1280.0).1;
    assert!(shows(&texts, SHIP) && !shows(&texts, OTHER), "{texts:?}");
    // Another page writes the file (as the shopping list does when a planned ship is bought).
    let _ = look(&mut page, "hf_einkaufsliste", 1280.0);
    std::fs::write(dir.join(hangar::FILE), own(&[SHIP, OTHER])).unwrap();
    // Back on the hangar: within a second of its clock it holds the new ship.
    let mut texts = Vec::new();
    for _ in 0..40 {
        texts = look(&mut page, "hf_hangar", 1280.0).1;
        if shows(&texts, OTHER) {
            break;
        }
    }
    assert!(shows(&texts, OTHER), "the ship from the other page shows: {texts:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_ship_the_hangar_holds_is_not_planned_and_it_says_why() {
    let _language = crate::language_lock();
    let own = format!(r#"{{"format": 1, "schiffe": [{{"name": "{OTHER}", "herkunft": "pledge", "belegung": {{}}}}]}}"#);
    let (mut page, dir) = page("schiff-im-hangar", &own);
    page.shops.pick_by_name(OTHER);
    look(&mut page, "hf_laeden", 1280.0);
    let after = draw_clicking(&mut page, 1280.0, 820.0, &[&t("s_ld_vormerken")]).remove(0);
    assert!(shows(&after, &t("s_hg_wunsch_schon_da")), "{after:?}");
    assert!(shows(&after, &t("s_ld_vormerken")), "the button stays, nothing was planned: {after:?}");
    assert!(!shows(&after, &t("s_ld_zu_geplant")), "no way to a section that holds nothing of it: {after:?}");
    assert!(planned_in(&dir).is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_way_leads_to_the_section_planned_of_the_hangar_page() {
    let _language = crate::language_lock();
    let (mut page, dir) = page("schiff-zum-hangar", r#"{"format": 1, "schiffe": []}"#);
    page.shops.pick_by_name(SHIP);
    look(&mut page, "hf_laeden", 1280.0);
    let after = draw_clicking(&mut page, 1280.0, 820.0, &[&t("s_ld_vormerken"), &t("s_ld_zu_geplant")]).remove(1);
    assert_eq!(page.page_key(), "hf_hangar");
    // The section stands there with the ship in it.
    assert!(shows(&after, &t("s_hg_geplant_titel")), "{after:?}");
    assert!(shows(&after, &tf("s_hg_wunsch_meine", &[("n", Arg::from(1))])), "{after:?}");
    assert!(after.iter().any(|(text, _)| text == SHIP), "{after:?}");
    assert!(!page.hangar.planned_pending(), "the page scrolled to the section and is done with it");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_old_page_of_the_wishlist_and_the_filter_lead_to_the_section_too() {
    let _language = crate::language_lock();
    let ship = format!(
        r#"{{"format": 1, "schiffe": [], "wunsch": [{{"name": "{SHIP}", "hersteller": "", "belegung": {{}}}}]}}"#
    );
    let (mut page, dir) = page("schiff-alte-seite", &ship);
    // Counter-check: Mein Hangar opened on its own does not show the ship of the section yet, it lies below.
    let texts = look(&mut page, "hf_hangar", 1280.0).1;
    assert!(!texts.iter().any(|(text, _)| text == SHIP), "{texts:?}");
    page.go_to("hf_allgemein");
    draw(&mut page, 1280.0, 820.0);
    // A stored last page or a card with the old name: Mein Hangar opens, scrolled to the section.
    page.go_to("hf_wunschliste");
    let texts = draw(&mut page, 1280.0, 820.0).1;
    assert_eq!(page.page_key(), "hf_hangar");
    assert!(shows(&texts, &t("s_hg_geplant_titel")) && texts.iter().any(|(text, _)| text == SHIP), "{texts:?}");
    assert!(!page.hangar.planned_pending());
    // Counter-check: another page named is left alone.
    page.go_to("hf_allgemein");
    draw(&mut page, 1280.0, 820.0);
    assert_eq!(page.page_key(), "hf_allgemein");
    assert!(!page.hangar.planned_pending());
    // The menu filter finds the section by its name and by the old name of the page.
    for word in [t("s_hg_geplant_titel"), t("hf_wunschliste")] {
        let hits = super::nav_hits_in(&word, "hf_gruppe_logistik", "hf_hangar", false);
        let hit = format!("{} › {}", t("hf_hangar"), t("s_hg_geplant_titel"));
        assert!(hits.contains(&hit), "{word}: {hits:?}");
    }
    // Counter-check: a word that belongs to the page only does not find the section.
    let hits = super::nav_hits_in("zzz", "hf_gruppe_logistik", "hf_hangar", false);
    assert!(hits.is_empty(), "{hits:?}");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_ship_view_and_the_section_stick_out_nowhere_in_both_languages() {
    let _language = crate::language_lock();
    let (mut page, dir) = page("schiff-rand", r#"{"format": 1, "schiffe": []}"#);
    for english in [false, true] {
        language::set_language(if english { language::Choice::En } else { language::Choice::De });
        for width in [880.0, 1100.0, 1600.0] {
            // The button, then the line with the way to the section (planned in the first round).
            page.shops.close_detail();
            page.shops.pick_by_name(OTHER);
            let (overflow, texts) = look(&mut page, "hf_laeden", width);
            assert_eq!(overflow, 0.0, "shops width {width}, english {english}");
            assert!(shows(&texts, &t("s_ld_vormerken")) || shows(&texts, &t("s_ld_schon_geplant")), "{texts:?}");
            page.hangar.show_planned();
            let (overflow, texts) = look(&mut page, "hf_hangar", width);
            assert_eq!(overflow, 0.0, "hangar width {width}, english {english}");
            assert!(shows(&texts, &t("s_hg_geplant_titel")), "{texts:?}");
        }
        if !english {
            page.shops.close_detail();
            page.shops.pick_by_name(OTHER);
            look(&mut page, "hf_laeden", 1100.0);
            draw_clicking(&mut page, 1100.0, 820.0, &[&t("s_ld_vormerken")]);
        }
    }
    language::set_language(language::Choice::De);
    let _ = std::fs::remove_dir_all(dir);
}
